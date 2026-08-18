#!/usr/bin/env python3
"""Run and render the on-demand Solar LSP benchmark."""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import math
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path
from typing import Any, Iterable, Sequence
from urllib.parse import urlsplit


SCRIPT_DIR = Path(__file__).resolve().parent
FIXTURE_DIR = SCRIPT_DIR / "fixture"
UPSTREAM_PATH = SCRIPT_DIR / "upstream.json"
FILTER_PATH = SCRIPT_DIR / "lsp_filter.py"

RAW_SCHEMA_VERSION = 1
COMPARISON_SCHEMA_VERSION = 1
RAW_KIND = "solar-lsp-benchmark-raw"
COMPARISON_KIND = "solar-lsp-benchmark-comparison"

WARMUP_ITERATIONS = 5
MEASURED_ITERATIONS = 10
PASSES = (
    ("base-first", ("base", "head")),
    ("head-first", ("head", "base")),
)
METHODS = (
    "initialize",
    "textDocument/diagnostic",
    "textDocument/hover",
    "textDocument/definition",
    "textDocument/references",
    "textDocument/completion",
    "textDocument/documentSymbol",
)
THRESHOLD_PERCENT = 10.0
COMPARISON_METRIC_DECIMALS = 4
REQUEST_TIMEOUT_SECONDS = 10
INDEX_TIMEOUT_SECONDS = 30
PASS_TIMEOUT_SECONDS = 30 * 60
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_CONFIG_BYTES = 1024 * 1024
MAX_RESULTS_BYTES = 32 * 1024 * 1024

SHA_RE = re.compile(r"[0-9a-f]{40}\Z")
SHA256_RE = re.compile(r"[0-9a-f]{64}\Z")
REPOSITORY_RE = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")

METHOD_CONFIG: dict[str, dict[str, Any]] = {
    "textDocument/hover": {"line": 13, "col": 17, "expect": {}},
    "textDocument/definition": {
        "line": 9,
        "col": 22,
        "expect": {"file": "Math.sol", "line": 4},
    },
    "textDocument/references": {
        "line": 8,
        "col": 15,
        "expect": {"minCount": 2},
    },
    "textDocument/completion": {
        "line": 17,
        "col": 20,
        "trigger": ".",
        "expect": {"containsItems": [{"label": "value"}]},
    },
    "textDocument/documentSymbol": {"expect": {"minCount": 5}},
}

RESULT_METHOD_CONFIG: dict[str, dict[str, Any]] = {
    method: {
        key: value
        for key, value in config.items()
        if key in {"line", "col", "trigger"}
    }
    for method, config in METHOD_CONFIG.items()
}


class BenchmarkError(Exception):
    """Base class for an expected adapter failure."""


class ValidationError(BenchmarkError):
    """Raised when an untrusted artifact violates the benchmark contract."""


class ExecutionError(BenchmarkError):
    """Raised when the benchmark cannot be executed."""


@dataclass(frozen=True)
class Context:
    repository: str
    head_repository: str
    workflow_repository: str
    pr_number: int
    base_sha: str
    head_sha: str
    run_url: str


def validate_context(
    repository: str,
    head_repository: str,
    workflow_repository: str,
    pr_number: int,
    base_sha: str,
    head_sha: str,
    run_url: str,
) -> Context:
    repositories = {
        "repository": repository,
        "head repository": head_repository,
        "workflow repository": workflow_repository,
    }
    for label, value in repositories.items():
        if not isinstance(value, str) or REPOSITORY_RE.fullmatch(value) is None:
            raise ValidationError(f"{label} is not an owner/name pair")
    if isinstance(pr_number, bool) or not isinstance(pr_number, int) or pr_number <= 0:
        raise ValidationError("PR number must be a positive integer")
    if not isinstance(base_sha, str) or SHA_RE.fullmatch(base_sha) is None:
        raise ValidationError("base SHA must be 40 lowercase hexadecimal characters")
    if not isinstance(head_sha, str) or SHA_RE.fullmatch(head_sha) is None:
        raise ValidationError("head SHA must be 40 lowercase hexadecimal characters")
    if not isinstance(run_url, str):
        raise ValidationError("run URL must be a string")
    if any(
        ord(character) < 0x20 or ord(character) == 0x7F for character in run_url
    ):
        raise ValidationError("run URL contains control characters")

    try:
        parsed = urlsplit(run_url)
    except ValueError as error:
        raise ValidationError("run URL is not a valid URL") from error
    expected_path = re.compile(
        rf"/{re.escape(workflow_repository)}/actions/runs/[1-9][0-9]*\Z"
    )
    if (
        parsed.scheme != "https"
        or parsed.netloc != "github.com"
        or expected_path.fullmatch(parsed.path) is None
        or parsed.query
        or parsed.fragment
        or parsed.username is not None
        or parsed.password is not None
    ):
        raise ValidationError("run URL is not the expected GitHub Actions run URL")
    return Context(
        repository,
        head_repository,
        workflow_repository,
        pr_number,
        base_sha,
        head_sha,
        run_url,
    )


def _reject_json_constant(value: str) -> None:
    raise ValueError(f"non-finite JSON constant {value}")


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key}")
        result[key] = value
    return result


def _loads_json(data: bytes, label: str) -> Any:
    try:
        return json.loads(
            data,
            parse_constant=_reject_json_constant,
            object_pairs_hook=_reject_duplicate_keys,
        )
    except (UnicodeDecodeError, ValueError, RecursionError) as error:
        raise ValidationError(f"{label} is not valid strict JSON") from error


def _strict_json_equal(left: Any, right: Any) -> bool:
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(
            _strict_json_equal(left[key], right[key]) for key in left
        )
    if isinstance(left, list):
        return len(left) == len(right) and all(
            _strict_json_equal(item, expected)
            for item, expected in zip(left, right)
        )
    return left == right


def _read_regular_file(path: Path, max_bytes: int, label: str) -> bytes:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise ValidationError(f"{label} is missing") from error
    if not stat.S_ISREG(metadata.st_mode):
        raise ValidationError(f"{label} is not a regular file")
    if metadata.st_size > max_bytes:
        raise ValidationError(f"{label} exceeds the size limit")
    try:
        return path.read_bytes()
    except OSError as error:
        raise ValidationError(f"{label} could not be read") from error


def _read_json(path: Path, max_bytes: int, label: str) -> Any:
    return _loads_json(_read_regular_file(path, max_bytes, label), label)


def _write_bytes_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary_path, path)
    finally:
        if temporary_path.exists():
            temporary_path.unlink()


def _write_json_atomic(path: Path, value: Any) -> None:
    data = (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()
    _write_bytes_atomic(path, data)


def _write_text_atomic(path: Path, value: str) -> None:
    _write_bytes_atomic(path, value.encode())


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fixture_sha256() -> str:
    digest = hashlib.sha256(b"solar-lsp-fixture-v1\0")
    files = sorted(path for path in FIXTURE_DIR.rglob("*") if path.is_file())
    if not files:
        raise ExecutionError("LSP benchmark fixture is empty")
    for path in files:
        relative = path.relative_to(FIXTURE_DIR).as_posix().encode()
        digest.update(relative)
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def pinned_upstream() -> dict[str, Any]:
    value = _read_json(UPSTREAM_PATH, MAX_CONFIG_BYTES, "upstream metadata")
    if not isinstance(value, dict):
        raise ExecutionError("upstream metadata is not an object")
    return value


def generated_config(
    project: Path,
    output: Path,
    commands: dict[str, Path],
    server_order: Sequence[str],
) -> dict[str, Any]:
    return {
        "project": str(project.resolve()),
        "file": "Main.sol",
        "line": 13,
        "col": 17,
        "iterations": MEASURED_ITERATIONS,
        "warmup": WARMUP_ITERATIONS,
        "timeout": REQUEST_TIMEOUT_SECONDS,
        "index_timeout": INDEX_TIMEOUT_SECONDS,
        "output": str(output.resolve()),
        "response": "full",
        "benchmarks": list(METHODS),
        "methods": METHOD_CONFIG,
        "servers": [
            {
                "label": role,
                "cmd": str(Path(sys.executable).resolve()),
                "args": [
                    str(FILTER_PATH.resolve()),
                    str(commands[role].resolve()),
                    "lsp",
                ],
            }
            for role in server_order
        ],
    }


def sanitized_environment(runtime_root: Path) -> dict[str, str]:
    directories = {
        "HOME": runtime_root / "home",
        "TMPDIR": runtime_root / "tmp",
        "XDG_CACHE_HOME": runtime_root / "xdg" / "cache",
        "XDG_CONFIG_HOME": runtime_root / "xdg" / "config",
        "XDG_DATA_HOME": runtime_root / "xdg" / "data",
        "XDG_STATE_HOME": runtime_root / "xdg" / "state",
    }
    for directory in directories.values():
        directory.mkdir(parents=True, exist_ok=True)
    return {
        **{name: str(path) for name, path in directories.items()},
        "PATH": os.defpath,
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TZ": "UTC",
        "NO_COLOR": "1",
        "RUST_BACKTRACE": "0",
    }


def _check_executable(path: Path, label: str) -> Path:
    resolved = path.resolve()
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise ExecutionError(f"{label} is not an executable file")
    return resolved


def _expected_upstream_version() -> str:
    metadata = pinned_upstream()
    operating_system = {
        "darwin": "macos",
        "linux": "linux",
        "windows": "windows",
    }.get(platform.system().lower(), platform.system().lower())
    architecture = {
        "amd64": "x86_64",
        "arm64": "aarch64",
    }.get(platform.machine().lower(), platform.machine().lower())
    return (
        f"lsp-bench {metadata['version']}+commit."
        f"{metadata['commit'][:7]}.{operating_system}.{architecture}"
    )


def _verify_upstream_binary(binary: Path) -> None:
    expected = _expected_upstream_version()
    with tempfile.TemporaryDirectory(prefix="solar-lsp-version-") as directory:
        environment = sanitized_environment(Path(directory))
        try:
            completed = subprocess.run(
                [str(binary), "--version"],
                check=False,
                capture_output=True,
                text=True,
                timeout=10,
                env=environment,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ExecutionError("could not execute the pinned lsp-bench binary") from error
    version = completed.stdout.strip()
    if completed.returncode != 0 or version != expected:
        raise ExecutionError("lsp-bench binary version does not match upstream.json")


def _prepare_output_directory(output: Path) -> Path:
    output = output.resolve()
    if output.exists():
        if not output.is_dir():
            raise ExecutionError("benchmark output exists and is not a directory")
        if any(output.iterdir()):
            raise ExecutionError("benchmark output directory must be empty")
    else:
        output.mkdir(parents=True)
    return output


def _mapping(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValidationError(f"{path} must be an object")
    return value


def _array(value: Any, path: str) -> list[Any]:
    if not isinstance(value, list):
        raise ValidationError(f"{path} must be an array")
    return value


def _positive_number(value: Any, path: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValidationError(f"{path} must be a number")
    try:
        number = float(value)
    except OverflowError as error:
        raise ValidationError(f"{path} must be finite and positive") from error
    if not math.isfinite(number) or number <= 0:
        raise ValidationError(f"{path} must be finite and positive")
    _rounded_metric(number, path)
    return number


def _rounded_metric(number: float, path: str) -> float:
    rounded = round(number, COMPARISON_METRIC_DECIMALS)
    if not math.isfinite(rounded) or rounded <= 0:
        raise ValidationError(f"{path} must remain positive after trusted rounding")
    return rounded


def _nonnegative_integer(value: Any, path: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValidationError(f"{path} must be a non-negative integer")
    return value


def _require_sha256(value: Any, path: str) -> str:
    if not isinstance(value, str) or SHA256_RE.fullmatch(value) is None:
        raise ValidationError(f"{path} must be a SHA-256 digest")
    return value


def _validate_generated_config(
    value: Any, server_order: Sequence[str]
) -> dict[str, Any]:
    config = _mapping(value, "config")
    expected_keys = {
        "project",
        "file",
        "line",
        "col",
        "iterations",
        "warmup",
        "timeout",
        "index_timeout",
        "output",
        "response",
        "benchmarks",
        "methods",
        "servers",
    }
    if set(config) != expected_keys:
        raise ValidationError("config fields do not match the adapter contract")
    expected_values = {
        "file": "Main.sol",
        "line": 13,
        "col": 17,
        "iterations": MEASURED_ITERATIONS,
        "warmup": WARMUP_ITERATIONS,
        "timeout": REQUEST_TIMEOUT_SECONDS,
        "index_timeout": INDEX_TIMEOUT_SECONDS,
        "response": "full",
        "benchmarks": list(METHODS),
        "methods": METHOD_CONFIG,
    }
    for key, expected in expected_values.items():
        if config.get(key) != expected:
            raise ValidationError(f"config.{key} does not match the adapter contract")
    for key in ("project", "output"):
        path = config.get(key)
        if not isinstance(path, str) or not path or not Path(path).is_absolute():
            raise ValidationError(f"config.{key} must be an absolute path")

    servers = _array(config.get("servers"), "config.servers")
    if len(servers) != len(server_order):
        raise ValidationError("config.servers has the wrong number of roles")
    for index, (server, role) in enumerate(zip(servers, server_order)):
        server = _mapping(server, f"config.servers[{index}]")
        if set(server) != {"label", "cmd", "args"}:
            raise ValidationError(
                f"config.servers[{index}] contains unsupported server fields"
            )
        arguments = server.get("args")
        if server.get("label") != role or not isinstance(arguments, list):
            raise ValidationError(f"config.servers[{index}] does not match role {role}")
        if (
            len(arguments) != 3
            or not isinstance(arguments[0], str)
            or not Path(arguments[0]).is_absolute()
            or not arguments[0].endswith("/benches/lsp/lsp_filter.py")
            or not isinstance(arguments[1], str)
            or not Path(arguments[1]).is_absolute()
            or arguments[2] != "lsp"
        ):
            raise ValidationError(f"config.servers[{index}].args is not the filter contract")
        command = server.get("cmd")
        if not isinstance(command, str) or not command or not Path(command).is_absolute():
            raise ValidationError(f"config.servers[{index}].cmd must be absolute")
    return config


def _fixture_uri(config: dict[str, Any], file_name: str) -> str:
    return (Path(config["project"]) / file_name).as_uri()


def _position(value: Any, path: str) -> tuple[int, int]:
    position = _mapping(value, path)
    if set(position) != {"line", "character"}:
        raise ValidationError(f"{path} is not an LSP position")
    line = _nonnegative_integer(position.get("line"), f"{path}.line")
    character = _nonnegative_integer(position.get("character"), f"{path}.character")
    return line, character


def _lsp_range(
    value: Any, path: str
) -> tuple[tuple[int, int], tuple[int, int]]:
    location_range = _mapping(value, path)
    if set(location_range) != {"start", "end"}:
        raise ValidationError(f"{path} is not an LSP range")
    start = _position(location_range.get("start"), f"{path}.start")
    end = _position(location_range.get("end"), f"{path}.end")
    if end < start:
        raise ValidationError(f"{path} ends before it starts")
    return start, end


def _location(
    value: Any, path: str, *, allow_link: bool
) -> tuple[str, tuple[tuple[int, int], tuple[int, int]]]:
    location = _mapping(value, path)
    if "targetUri" in location or "targetRange" in location:
        required = {"targetUri", "targetRange", "targetSelectionRange"}
        allowed = required | {"originSelectionRange"}
        if not allow_link or not required <= set(location) <= allowed:
            raise ValidationError(f"{path} is not an LSP location link")
        uri = location.get("targetUri")
        location_range = _lsp_range(location.get("targetRange"), f"{path}.targetRange")
        _lsp_range(
            location.get("targetSelectionRange"), f"{path}.targetSelectionRange"
        )
        if "originSelectionRange" in location:
            _lsp_range(
                location["originSelectionRange"], f"{path}.originSelectionRange"
            )
    else:
        if set(location) != {"uri", "range"}:
            raise ValidationError(f"{path} is not an LSP location")
        uri = location.get("uri")
        location_range = _lsp_range(location.get("range"), f"{path}.range")
    if not isinstance(uri, str):
        raise ValidationError(f"{path} has no document URI")
    return uri, location_range


def _locations(
    value: Any, path: str, *, allow_single: bool, allow_links: bool
) -> list[tuple[str, tuple[tuple[int, int], tuple[int, int]]]]:
    if isinstance(value, list):
        values = value
    elif allow_single and isinstance(value, dict):
        values = [value]
    else:
        raise ValidationError(f"{path} is not an LSP location result")
    if not values:
        raise ValidationError(f"{path} contains no locations")
    return [
        _location(item, f"{path}[{index}]", allow_link=allow_links)
        for index, item in enumerate(values)
    ]


def _hover_text(value: Any, path: str) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        if not value:
            raise ValidationError(f"{path} is empty")
        texts = []
        for index, item in enumerate(value):
            item_path = f"{path}[{index}]"
            if isinstance(item, str):
                texts.append(item)
            else:
                marked = _mapping(item, item_path)
                if set(marked) != {"language", "value"}:
                    raise ValidationError(f"{item_path} is not an LSP marked string")
                if not isinstance(marked.get("language"), str) or not isinstance(
                    marked.get("value"), str
                ):
                    raise ValidationError(f"{item_path} is not an LSP marked string")
                texts.append(marked["value"])
        return "\n".join(texts)
    markup = _mapping(value, path)
    if (
        set(markup) != {"kind", "value"}
        or markup.get("kind") not in {"plaintext", "markdown"}
        or not isinstance(markup.get("value"), str)
    ):
        raise ValidationError(f"{path} is not LSP markup content")
    return markup["value"]


def _symbol_kind(value: Any, path: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= 26:
        raise ValidationError(f"{path} is not an LSP symbol kind")
    return value


def _validate_symbol_tags(value: Any, path: str) -> None:
    tags = _array(value, path)
    if any(isinstance(tag, bool) or not isinstance(tag, int) or tag != 1 for tag in tags):
        raise ValidationError(f"{path} contains an invalid LSP symbol tag")


def _validate_response(
    method: str, response: Any, path: str, config: dict[str, Any]
) -> None:
    if method == "initialize":
        if response != "ok":
            raise ValidationError(f"{path} is not a successful initialization")
        return

    if method == "textDocument/diagnostic":
        params = _mapping(response, path)
        if not {"uri", "diagnostics"} <= set(params) <= {
            "uri",
            "version",
            "diagnostics",
        }:
            raise ValidationError(f"{path} is not publishDiagnostics params")
        uri = params.get("uri")
        if uri != _fixture_uri(config, "Main.sol"):
            raise ValidationError(f"{path} is not for Main.sol")
        if "version" in params and (
            params["version"] is not None
            and (
                isinstance(params["version"], bool)
                or not isinstance(params["version"], int)
            )
        ):
            raise ValidationError(f"{path}.version is not an integer or null")
        diagnostics = _array(params.get("diagnostics"), f"{path}.diagnostics")
        if len(diagnostics) != 1:
            raise ValidationError(f"{path} does not contain exactly one diagnostic")
        diagnostic = _mapping(diagnostics[0], f"{path}.diagnostics[0]")
        required = {"range", "message", "severity", "code"}
        allowed = required | {
            "codeDescription",
            "source",
            "tags",
            "relatedInformation",
            "data",
        }
        if not required <= set(diagnostic) <= allowed:
            raise ValidationError(f"{path}.diagnostics[0] is not an LSP diagnostic")
        warning_range = _lsp_range(
            diagnostic.get("range"), f"{path}.diagnostics[0].range"
        )
        if (
            str(diagnostic.get("code")) != "2018"
            or diagnostic.get("severity") != 2
            or diagnostic.get("message")
            != "function state mutability can be restricted to view"
            or warning_range != ((16, 4), (18, 5))
        ):
            raise ValidationError(f"{path} does not contain the expected warning")
        if "source" in diagnostic and not isinstance(diagnostic["source"], str):
            raise ValidationError(f"{path}.diagnostics[0].source is not a string")
        if "codeDescription" in diagnostic:
            description = _mapping(
                diagnostic["codeDescription"], f"{path}.diagnostics[0].codeDescription"
            )
            if set(description) != {"href"} or not isinstance(
                description.get("href"), str
            ):
                raise ValidationError(
                    f"{path}.diagnostics[0].codeDescription is invalid"
                )
        if "tags" in diagnostic:
            _validate_symbol_tags(diagnostic["tags"], f"{path}.diagnostics[0].tags")
        if "relatedInformation" in diagnostic:
            related = _array(
                diagnostic["relatedInformation"],
                f"{path}.diagnostics[0].relatedInformation",
            )
            for index, information in enumerate(related):
                information_path = f"{path}.diagnostics[0].relatedInformation[{index}]"
                information = _mapping(information, information_path)
                if set(information) != {"location", "message"} or not isinstance(
                    information.get("message"), str
                ):
                    raise ValidationError(f"{information_path} is invalid")
                _location(
                    information.get("location"),
                    f"{information_path}.location",
                    allow_link=False,
                )
        return

    if method == "textDocument/hover":
        hover = _mapping(response, path)
        if not {"contents"} <= set(hover) <= {"contents", "range"}:
            raise ValidationError(f"{path} has no hover contents")
        contents = _hover_text(hover["contents"], f"{path}.contents")
        if "range" in hover:
            _lsp_range(hover["range"], f"{path}.range")
        if (
            re.search(
                r"\bfunction\s+double\s*\(\s*uint256"
                r"(?:\s+[A-Za-z_][A-Za-z0-9_]*)?\s*\)",
                contents,
            )
            is None
            or re.search(r"\breturns\s*\(\s*uint256\s*\)", contents) is None
        ):
            raise ValidationError(f"{path} is not the expected double function hover")
        return

    if method == "textDocument/definition":
        locations = _locations(
            response, path, allow_single=True, allow_links=True
        )
        expected_uri = _fixture_uri(config, "Math.sol")
        if any(uri != expected_uri for uri, _ in locations) or not any(
            location_range[0][0] == 4 for _, location_range in locations
        ):
            raise ValidationError(f"{path} does not resolve to Math.sol:5")
        return

    if method == "textDocument/references":
        references = _array(response, path)
        try:
            locations = _locations(
                references, path, allow_single=False, allow_links=False
            )
        except ValidationError as error:
            raise ValidationError(f"{path} contains an invalid reference") from error
        expected_uri = _fixture_uri(config, "Main.sol")
        positions = {location_range[0] for uri, location_range in locations}
        if any(uri != expected_uri for uri, _ in locations) or not {
            (8, 13),
            (13, 15),
        }.issubset(positions):
            raise ValidationError(f"{path} is missing the declaration or call reference")
        return

    if method == "textDocument/completion":
        if isinstance(response, dict):
            if not {"isIncomplete", "items"} <= set(response) <= {
                "isIncomplete",
                "items",
                "itemDefaults",
            } or not isinstance(response.get("isIncomplete"), bool):
                raise ValidationError(f"{path} is not an LSP completion list")
            if "itemDefaults" in response and not isinstance(
                response["itemDefaults"], dict
            ):
                raise ValidationError(f"{path}.itemDefaults is not an object")
            items = _array(response.get("items"), f"{path}.items")
        else:
            items = _array(response, path)
        labels = set()
        for index, item in enumerate(items):
            item = _mapping(item, f"{path}.items[{index}]")
            label = item.get("label")
            if not isinstance(label, str):
                raise ValidationError(f"{path}.items[{index}] has no label")
            if "kind" in item and (
                isinstance(item["kind"], bool)
                or not isinstance(item["kind"], int)
                or not 1 <= item["kind"] <= 25
            ):
                raise ValidationError(
                    f"{path}.items[{index}].kind is not an LSP completion item kind"
                )
            labels.add(label)
        if "value" not in labels:
            raise ValidationError(f"{path} is missing the value completion")
        return

    if method == "textDocument/documentSymbol":
        roots = _array(response, path)
        pending = [(item, f"{path}[{index}]") for index, item in enumerate(roots)]
        names: set[str] = set()
        visited = 0
        while pending:
            item, item_path = pending.pop()
            visited += 1
            if visited > 10_000:
                raise ValidationError(f"{path} is too complex")
            item = _mapping(item, item_path)
            if not isinstance(item.get("name"), str):
                raise ValidationError(f"{item_path} has no symbol name")
            _symbol_kind(item.get("kind"), f"{item_path}.kind")
            names.add(item["name"])
            if "location" in item:
                allowed = {
                    "name",
                    "kind",
                    "tags",
                    "deprecated",
                    "location",
                    "containerName",
                }
                if not {"name", "kind", "location"} <= set(item) <= allowed:
                    raise ValidationError(f"{item_path} is not LSP symbol information")
                uri, _ = _location(
                    item["location"], f"{item_path}.location", allow_link=False
                )
                if uri != _fixture_uri(config, "Main.sol"):
                    raise ValidationError(f"{item_path} is not for Main.sol")
                if "containerName" in item and not isinstance(
                    item["containerName"], str
                ):
                    raise ValidationError(f"{item_path}.containerName is not a string")
            else:
                required = {"name", "kind", "range", "selectionRange"}
                allowed = required | {"detail", "tags", "deprecated", "children"}
                if not required <= set(item) <= allowed:
                    raise ValidationError(f"{item_path} is not an LSP document symbol")
                symbol_range = _lsp_range(item["range"], f"{item_path}.range")
                selection_range = _lsp_range(
                    item["selectionRange"], f"{item_path}.selectionRange"
                )
                if (
                    selection_range[0] < symbol_range[0]
                    or selection_range[1] > symbol_range[1]
                ):
                    raise ValidationError(
                        f"{item_path}.selectionRange is outside the symbol range"
                    )
                if "detail" in item and not isinstance(item["detail"], str):
                    raise ValidationError(f"{item_path}.detail is not a string")
                children = item.get("children", [])
                if not isinstance(children, list):
                    raise ValidationError(f"{item_path}.children is not an array")
                pending.extend(
                    (child, f"{item_path}.children[{index}]")
                    for index, child in enumerate(children)
                )
            if "tags" in item:
                _validate_symbol_tags(item["tags"], f"{item_path}.tags")
            if "deprecated" in item and not isinstance(item["deprecated"], bool):
                raise ValidationError(f"{item_path}.deprecated is not a boolean")
        expected = {"Main", "value", "double", "compute", "completions"}
        if not expected.issubset(names):
            raise ValidationError(f"{path} is missing expected document symbols")
        return

    raise ValidationError(f"{path} uses an unexpected benchmark method")


def _expected_benchmark_input(method: str, config: dict[str, Any]) -> dict[str, Any] | None:
    if method in {"initialize", "textDocument/diagnostic"}:
        return None

    uri = (Path(config["project"]) / "Main.sol").as_uri()
    params: dict[str, Any] = {"textDocument": {"uri": uri}}
    if method != "textDocument/documentSymbol":
        method_config = METHOD_CONFIG[method]
        params["position"] = {
            "line": method_config.get("line", config["line"]),
            "character": method_config.get("col", config["col"]),
        }
    if method == "textDocument/references":
        params["context"] = {"includeDeclaration": True}
    elif method == "textDocument/completion":
        params["context"] = {
            "triggerKind": 2,
            "triggerCharacter": METHOD_CONFIG[method]["trigger"],
        }
    return {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}


def _validate_results(
    value: Any,
    server_order: Sequence[str],
    config: dict[str, Any],
) -> tuple[dict[str, dict[str, list[float]]], dict[str, str]]:
    results = _mapping(value, "results")
    if set(results) != {"timestamp", "date", "settings", "servers", "benchmarks"}:
        raise ValidationError("results fields do not match the upstream contract")
    for key in ("timestamp", "date"):
        if not isinstance(results.get(key), str) or not results[key]:
            raise ValidationError(f"results.{key} must be a non-empty string")
    settings = _mapping(results.get("settings"), "results.settings")
    expected_settings = {
        "iterations": MEASURED_ITERATIONS,
        "warmup": WARMUP_ITERATIONS,
        "timeout_secs": REQUEST_TIMEOUT_SECONDS,
        "index_timeout_secs": INDEX_TIMEOUT_SECONDS,
        "project": config["project"],
        "file": "Main.sol",
        "line": 13,
        "col": 17,
        "methods": RESULT_METHOD_CONFIG,
    }
    if set(settings) != set(expected_settings):
        raise ValidationError("results.settings fields do not match the config")
    for key, expected in expected_settings.items():
        if settings.get(key) != expected:
            raise ValidationError(f"results.settings.{key} does not match the config")

    server_versions = _array(results.get("servers"), "results.servers")
    if len(server_versions) != len(server_order):
        raise ValidationError("results.servers has the wrong number of roles")
    versions: dict[str, str] = {}
    for index, (server, role) in enumerate(zip(server_versions, server_order)):
        server = _mapping(server, f"results.servers[{index}]")
        if set(server) != {"name", "version"}:
            raise ValidationError(f"results.servers[{index}] has unexpected fields")
        if server.get("name") != role:
            raise ValidationError(f"results.servers[{index}] has the wrong role")
        if not isinstance(server.get("version"), str) or not server["version"]:
            raise ValidationError(f"results.servers[{index}] has no version")
        versions[role] = server["version"]

    benchmarks = _array(results.get("benchmarks"), "results.benchmarks")
    by_method: dict[str, dict[str, list[float]]] = {}
    for benchmark_index, benchmark in enumerate(benchmarks):
        benchmark = _mapping(benchmark, f"results.benchmarks[{benchmark_index}]")
        method = benchmark.get("name")
        if method not in METHODS or method in by_method:
            raise ValidationError(
                f"results.benchmarks[{benchmark_index}] has an unexpected method"
            )
        expected_input = _expected_benchmark_input(method, config)
        expected_fields = {"name", "servers"}
        if expected_input is not None:
            expected_fields.add("input")
        if set(benchmark) != expected_fields:
            raise ValidationError(
                f"results.benchmarks[{benchmark_index}] fields do not match {method}"
            )
        if expected_input is not None:
            raw_input = benchmark.get("input")
            if not isinstance(raw_input, str):
                raise ValidationError(f"benchmark {method} input must be JSON text")
            actual_input = _loads_json(raw_input.encode(), f"benchmark {method} input")
            if not _strict_json_equal(actual_input, expected_input):
                raise ValidationError(
                    f"benchmark {method} input does not match the config"
                )
        rows = _array(
            benchmark.get("servers"),
            f"results.benchmarks[{benchmark_index}].servers",
        )
        if len(rows) != len(server_order):
            raise ValidationError(f"benchmark {method} has the wrong number of roles")
        method_samples: dict[str, list[float]] = {}
        for row_index, (row, role) in enumerate(zip(rows, server_order)):
            row_path = f"benchmark {method} role {role}"
            row = _mapping(row, row_path)
            required_row_fields = {
                "server",
                "status",
                "p50_ms",
                "p95_ms",
                "mean_ms",
                "response",
                "iterations",
            }
            if not required_row_fields <= set(row) <= required_row_fields | {"rss_kb"}:
                raise ValidationError(f"{row_path} has unexpected fields")
            if row.get("server") != role:
                raise ValidationError(f"{row_path} has the wrong role label")
            if row.get("status") != "ok":
                raise ValidationError(f"{row_path} did not pass")
            for metric in ("p50_ms", "p95_ms", "mean_ms"):
                _positive_number(row.get(metric), f"{row_path}.{metric}")
            if "rss_kb" in row:
                _nonnegative_integer(row["rss_kb"], f"{row_path}.rss_kb")
            if "response" not in row:
                raise ValidationError(f"{row_path} has no canonical response")
            _validate_response(
                method, row["response"], f"{row_path}.response", config
            )

            iterations = _array(row.get("iterations"), f"{row_path}.iterations")
            if len(iterations) != MEASURED_ITERATIONS:
                raise ValidationError(f"{row_path} has the wrong sample count")
            samples: list[float] = []
            for iteration_index, iteration in enumerate(iterations):
                iteration_path = f"{row_path}.iterations[{iteration_index}]"
                iteration = _mapping(iteration, iteration_path)
                if set(iteration) != {"ms", "response"}:
                    raise ValidationError(f"{iteration_path} has unexpected fields")
                samples.append(_positive_number(iteration.get("ms"), f"{iteration_path}.ms"))
                if "response" not in iteration:
                    raise ValidationError(f"{iteration_path} has no response")
                _validate_response(
                    method,
                    iteration["response"],
                    f"{iteration_path}.response",
                    config,
                )
            method_samples[role] = samples
        by_method[method] = method_samples

    if set(by_method) != set(METHODS):
        raise ValidationError("results do not contain exactly the core method set")
    samples = {
        role: {method: by_method[method][role] for method in METHODS}
        for role in server_order
    }
    return samples, versions


def _run_pass(
    lsp_bench: Path,
    commands: dict[str, Path],
    output: Path,
    pass_name: str,
    server_order: Sequence[str],
) -> dict[str, Any]:
    artifact_directory = output / "passes" / pass_name
    artifact_directory.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix=f".{pass_name}-", dir=output) as temporary:
        runtime_root = Path(temporary)
        project = runtime_root / "project"
        shutil.copytree(FIXTURE_DIR, project)
        benchmark_output = runtime_root / "benchmark-output"
        config = generated_config(project, benchmark_output, commands, server_order)
        runtime_config = runtime_root / "config.json"
        artifact_config = artifact_directory / "config.json"
        _write_json_atomic(runtime_config, config)
        _write_json_atomic(artifact_config, config)

        command = [str(lsp_bench), "--config", str(runtime_config), "--verify"]
        try:
            completed = subprocess.run(
                command,
                check=False,
                cwd=runtime_root,
                env=sanitized_environment(runtime_root),
                timeout=PASS_TIMEOUT_SECONDS,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ExecutionError(f"{pass_name} benchmark execution failed") from error
        if completed.returncode != 0:
            raise ExecutionError(
                f"{pass_name} benchmark exited with status {completed.returncode}"
            )

        result_source = benchmark_output / "results.json"
        result_bytes = _read_regular_file(
            result_source, MAX_RESULTS_BYTES, f"{pass_name} results"
        )
        artifact_results = artifact_directory / "results.json"
        _write_bytes_atomic(artifact_results, result_bytes)
        _validate_results(
            _loads_json(result_bytes, f"{pass_name} results"), server_order, config
        )

    return {
        "name": pass_name,
        "server_order": list(server_order),
        "config": {
            "path": f"passes/{pass_name}/config.json",
            "sha256": sha256_file(artifact_config),
        },
        "results": {
            "path": f"passes/{pass_name}/results.json",
            "sha256": sha256_file(artifact_results),
        },
    }


def run_benchmark(
    lsp_bench: Path,
    base_binary: Path,
    head_binary: Path,
    context: Context,
    output: Path,
) -> Path:
    lsp_bench = _check_executable(lsp_bench, "lsp-bench")
    commands = {
        "base": _check_executable(base_binary, "base Solar binary"),
        "head": _check_executable(head_binary, "head Solar binary"),
    }
    if commands["base"] == commands["head"]:
        raise ExecutionError("base and head binaries must use distinct paths")
    _verify_upstream_binary(lsp_bench)
    output = _prepare_output_directory(output)

    pass_entries = [
        _run_pass(lsp_bench, commands, output, pass_name, server_order)
        for pass_name, server_order in PASSES
    ]
    manifest = {
        "schema_version": RAW_SCHEMA_VERSION,
        "kind": RAW_KIND,
        "context": {
            "repository": context.repository,
            "head_repository": context.head_repository,
            "workflow_repository": context.workflow_repository,
            "pr_number": context.pr_number,
            "base_sha": context.base_sha,
            "head_sha": context.head_sha,
            "run_url": context.run_url,
        },
        "protocol": {
            "warmup_iterations": WARMUP_ITERATIONS,
            "measured_iterations_per_pass": MEASURED_ITERATIONS,
            "passes": [name for name, _ in PASSES],
            "methods": list(METHODS),
            "threshold_percent": THRESHOLD_PERCENT,
        },
        "upstream": pinned_upstream(),
        "fixture": {"sha256": fixture_sha256()},
        "binaries": {
            role: {"sha256": sha256_file(command)} for role, command in commands.items()
        },
        "passes": pass_entries,
    }
    manifest_path = output / "manifest.json"
    _write_json_atomic(manifest_path, manifest)
    return manifest_path


def _validate_manifest(value: Any, expected: Context) -> dict[str, Any]:
    manifest = _mapping(value, "manifest")
    expected_keys = {
        "schema_version",
        "kind",
        "context",
        "protocol",
        "upstream",
        "fixture",
        "binaries",
        "passes",
    }
    if set(manifest) != expected_keys:
        raise ValidationError("manifest fields do not match the raw artifact contract")
    if manifest.get("schema_version") != RAW_SCHEMA_VERSION or manifest.get("kind") != RAW_KIND:
        raise ValidationError("manifest schema is unsupported")

    context = _mapping(manifest.get("context"), "manifest.context")
    expected_context = {
        "repository": expected.repository,
        "head_repository": expected.head_repository,
        "workflow_repository": expected.workflow_repository,
        "pr_number": expected.pr_number,
        "base_sha": expected.base_sha,
        "head_sha": expected.head_sha,
        "run_url": expected.run_url,
    }
    if context != expected_context:
        raise ValidationError("manifest context does not match the trusted workflow context")

    protocol = _mapping(manifest.get("protocol"), "manifest.protocol")
    expected_protocol = {
        "warmup_iterations": WARMUP_ITERATIONS,
        "measured_iterations_per_pass": MEASURED_ITERATIONS,
        "passes": [name for name, _ in PASSES],
        "methods": list(METHODS),
        "threshold_percent": THRESHOLD_PERCENT,
    }
    if protocol != expected_protocol:
        raise ValidationError("manifest protocol does not match the trusted adapter")
    if manifest.get("upstream") != pinned_upstream():
        raise ValidationError("manifest upstream metadata does not match the pinned release")
    if manifest.get("fixture") != {"sha256": fixture_sha256()}:
        raise ValidationError("manifest fixture digest does not match the trusted fixture")

    binaries = _mapping(manifest.get("binaries"), "manifest.binaries")
    if set(binaries) != {"base", "head"}:
        raise ValidationError("manifest binaries do not contain exactly base and head")
    for role in ("base", "head"):
        binary = _mapping(binaries[role], f"manifest.binaries.{role}")
        if set(binary) != {"sha256"}:
            raise ValidationError(f"manifest.binaries.{role} has unexpected fields")
        _require_sha256(binary.get("sha256"), f"manifest.binaries.{role}.sha256")
    return manifest


def _validate_artifact_layout(root: Path) -> None:
    expected_files = {
        "manifest.json",
        *(
            f"passes/{pass_name}/{file_name}"
            for pass_name, _ in PASSES
            for file_name in ("config.json", "results.json")
        ),
    }
    expected_directories = {
        "passes",
        *(f"passes/{pass_name}" for pass_name, _ in PASSES),
    }
    seen_files: set[str] = set()
    seen_directories: set[str] = set()
    try:
        entries = root.rglob("*")
        for entry in entries:
            relative = entry.relative_to(root).as_posix()
            metadata = entry.lstat()
            if stat.S_ISREG(metadata.st_mode) and relative in expected_files:
                seen_files.add(relative)
            elif stat.S_ISDIR(metadata.st_mode) and relative in expected_directories:
                seen_directories.add(relative)
            else:
                raise ValidationError(f"raw artifact contains unexpected entry {relative}")
    except OSError as error:
        raise ValidationError("raw artifact layout could not be inspected") from error
    if seen_files != expected_files or seen_directories != expected_directories:
        raise ValidationError("raw artifact layout is incomplete")


def validate_artifact(input_directory: Path, expected: Context) -> dict[str, dict[str, list[float]]]:
    root = input_directory.resolve()
    if not root.is_dir():
        raise ValidationError("raw artifact directory is missing")
    _validate_artifact_layout(root)
    manifest = _validate_manifest(
        _read_json(root / "manifest.json", MAX_MANIFEST_BYTES, "manifest"), expected
    )
    pass_entries = _array(manifest.get("passes"), "manifest.passes")
    if len(pass_entries) != len(PASSES):
        raise ValidationError("manifest has the wrong number of passes")

    merged = {
        role: {method: [] for method in METHODS} for role in ("base", "head")
    }
    commands_by_role: dict[str, str] = {}
    versions_by_role: dict[str, str] = {}
    for index, ((pass_name, server_order), entry) in enumerate(zip(PASSES, pass_entries)):
        entry = _mapping(entry, f"manifest.passes[{index}]")
        if set(entry) != {"name", "server_order", "config", "results"}:
            raise ValidationError(f"manifest.passes[{index}] has unexpected fields")
        if entry.get("name") != pass_name or entry.get("server_order") != list(server_order):
            raise ValidationError(f"manifest.passes[{index}] has the wrong pass order")

        config_metadata = _mapping(entry.get("config"), f"manifest.passes[{index}].config")
        results_metadata = _mapping(entry.get("results"), f"manifest.passes[{index}].results")
        expected_config_path = f"passes/{pass_name}/config.json"
        expected_results_path = f"passes/{pass_name}/results.json"
        if config_metadata.get("path") != expected_config_path or set(config_metadata) != {
            "path",
            "sha256",
        }:
            raise ValidationError(f"manifest.passes[{index}].config is invalid")
        if results_metadata.get("path") != expected_results_path or set(results_metadata) != {
            "path",
            "sha256",
        }:
            raise ValidationError(f"manifest.passes[{index}].results is invalid")

        config_path = root / expected_config_path
        results_path = root / expected_results_path
        expected_config_digest = _require_sha256(
            config_metadata.get("sha256"), f"manifest.passes[{index}].config.sha256"
        )
        expected_results_digest = _require_sha256(
            results_metadata.get("sha256"), f"manifest.passes[{index}].results.sha256"
        )
        config_bytes = _read_regular_file(config_path, MAX_CONFIG_BYTES, f"{pass_name} config")
        results_bytes = _read_regular_file(results_path, MAX_RESULTS_BYTES, f"{pass_name} results")
        if hashlib.sha256(config_bytes).hexdigest() != expected_config_digest:
            raise ValidationError(f"{pass_name} config digest does not match the manifest")
        if hashlib.sha256(results_bytes).hexdigest() != expected_results_digest:
            raise ValidationError(f"{pass_name} results digest does not match the manifest")

        config = _validate_generated_config(
            _loads_json(config_bytes, f"{pass_name} config"), server_order
        )
        for server in config["servers"]:
            role = server["label"]
            command = server["args"][1]
            previous_command = commands_by_role.setdefault(role, command)
            if previous_command != command:
                raise ValidationError(f"{role} command differs between passes")
        if commands_by_role.get("base") == commands_by_role.get("head"):
            raise ValidationError("base and head commands must use distinct paths")

        pass_samples, pass_versions = _validate_results(
            _loads_json(results_bytes, f"{pass_name} results"), server_order, config
        )
        for role in server_order:
            version = pass_versions[role]
            previous_version = versions_by_role.setdefault(role, version)
            if previous_version != version:
                raise ValidationError(f"{role} version differs between passes")
            for method in METHODS:
                merged[role][method].extend(pass_samples[role][method])

    expected_samples = MEASURED_ITERATIONS * len(PASSES)
    for role in ("base", "head"):
        for method in METHODS:
            if len(merged[role][method]) != expected_samples:
                raise ValidationError(f"merged {role} {method} sample count is incorrect")
    return merged


def percentile(samples: Iterable[float], percent: float) -> float:
    ordered = sorted(float(sample) for sample in samples)
    if not ordered:
        raise ValueError("percentile requires at least one sample")
    if not 0 < percent <= 100:
        raise ValueError("percentile must be in (0, 100]")
    index = max(0, math.ceil(percent / 100 * len(ordered)) - 1)
    return ordered[index]


def method_verdict(p50_delta: float | Decimal, p95_delta: float | Decimal) -> str:
    p50_delta = Decimal(str(p50_delta))
    p95_delta = Decimal(str(p95_delta))
    threshold = Decimal(str(THRESHOLD_PERCENT))
    if p50_delta >= threshold and p95_delta >= threshold:
        return "regression"
    if p50_delta <= -threshold and p95_delta <= -threshold:
        return "improvement"
    return "stable"


def _rounded_delta_percent(value: Decimal, path: str) -> float:
    if not value.is_finite():
        raise ValidationError(f"{path} must be finite")
    try:
        rounded = float(round(value, 2))
    except (ArithmeticError, OverflowError, ValueError) as error:
        raise ValidationError(f"{path} must remain finite after trusted rounding") from error
    if not math.isfinite(rounded):
        raise ValidationError(f"{path} must remain finite after trusted rounding")
    return rounded


def build_comparison(
    samples: dict[str, dict[str, list[float]]], context: Context
) -> dict[str, Any]:
    methods: list[dict[str, Any]] = []
    verdicts: list[str] = []
    for method in METHODS:
        base_samples = samples["base"][method]
        head_samples = samples["head"][method]
        base_p50 = percentile(base_samples, 50)
        base_p95 = percentile(base_samples, 95)
        head_p50 = percentile(head_samples, 50)
        head_p95 = percentile(head_samples, 95)
        base_p50_decimal = Decimal(str(base_p50))
        base_p95_decimal = Decimal(str(base_p95))
        p50_delta = (
            (Decimal(str(head_p50)) - base_p50_decimal) / base_p50_decimal * 100
        )
        p95_delta = (
            (Decimal(str(head_p95)) - base_p95_decimal) / base_p95_decimal * 100
        )
        verdict = method_verdict(p50_delta, p95_delta)
        verdicts.append(verdict)
        methods.append(
            {
                "name": method,
                "sample_count": len(base_samples),
                "base": {
                    "p50_ms": _rounded_metric(base_p50, f"{method} base p50"),
                    "p95_ms": _rounded_metric(base_p95, f"{method} base p95"),
                },
                "head": {
                    "p50_ms": _rounded_metric(head_p50, f"{method} head p50"),
                    "p95_ms": _rounded_metric(head_p95, f"{method} head p95"),
                },
                "delta_percent": {
                    "p50": _rounded_delta_percent(p50_delta, f"{method} p50 delta"),
                    "p95": _rounded_delta_percent(p95_delta, f"{method} p95 delta"),
                },
                "verdict": verdict,
            }
        )

    if "regression" in verdicts:
        overall = "regression"
    elif "improvement" in verdicts:
        overall = "improvement"
    else:
        overall = "stable"
    return {
        "schema_version": COMPARISON_SCHEMA_VERSION,
        "kind": COMPARISON_KIND,
        "repository": context.repository,
        "head_repository": context.head_repository,
        "workflow_repository": context.workflow_repository,
        "pr_number": context.pr_number,
        "base_sha": context.base_sha,
        "head_sha": context.head_sha,
        "run_url": context.run_url,
        "threshold_percent": THRESHOLD_PERCENT,
        "overall": overall,
        "methods": methods,
    }


def inconclusive_comparison(context: Context, reason: str) -> dict[str, Any]:
    return {
        "schema_version": COMPARISON_SCHEMA_VERSION,
        "kind": COMPARISON_KIND,
        "repository": context.repository,
        "head_repository": context.head_repository,
        "workflow_repository": context.workflow_repository,
        "pr_number": context.pr_number,
        "base_sha": context.base_sha,
        "head_sha": context.head_sha,
        "run_url": context.run_url,
        "threshold_percent": THRESHOLD_PERCENT,
        "overall": "inconclusive",
        "methods": [],
        "error": reason[:500],
    }


def markdown_escape(value: Any) -> str:
    escaped = html.escape(str(value), quote=False)
    for source, replacement in (
        ("\\", "\\\\"),
        ("|", "\\|"),
        ("`", "\\`"),
        ("[", "\\["),
        ("]", "\\]"),
        ("*", "\\*"),
        ("_", "\\_"),
        ("~", "\\~"),
        ("\r\n", "<br>"),
        ("\r", "<br>"),
        ("\n", "<br>"),
    ):
        escaped = escaped.replace(source, replacement)
    return escaped


def _delta_cell(value: float) -> str:
    return f"{value:+.2f}%"


def render_markdown(comparison: dict[str, Any]) -> str:
    repository = comparison["repository"]
    head_repository = comparison["head_repository"]
    base_sha = comparison["base_sha"]
    head_sha = comparison["head_sha"]
    lines = [
        "<!-- solar-lsp-benchmark -->",
        "## LSP benchmark",
        "",
        f"**Overall:** `{markdown_escape(comparison['overall'])}`",
        "",
        (
            f"Base: [`{base_sha}`](https://github.com/{repository}/commit/{base_sha})  "
        ),
        f"Head: [`{head_sha}`](https://github.com/{head_repository}/commit/{head_sha})  ",
        f"[Workflow run]({comparison['run_url']})",
        "",
    ]
    if comparison["overall"] == "inconclusive":
        lines.extend(
            [
                "The comparison is inconclusive because the raw benchmark artifact did not pass validation.",
                "",
                f"Reason: {markdown_escape(comparison.get('error', 'unknown validation error'))}",
                "",
            ]
        )
        return "\n".join(lines)

    lines.extend(
        [
            "| Method | Samples | Base p50 | Head p50 | Delta | Base p95 | Head p95 | Delta | Verdict |",
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |",
        ]
    )
    for method in comparison["methods"]:
        lines.append(
            "| "
            + " | ".join(
                [
                    markdown_escape(method["name"]),
                    str(method["sample_count"]),
                    f"{method['base']['p50_ms']:.2f} ms",
                    f"{method['head']['p50_ms']:.2f} ms",
                    _delta_cell(method["delta_percent"]["p50"]),
                    f"{method['base']['p95_ms']:.2f} ms",
                    f"{method['head']['p95_ms']:.2f} ms",
                    _delta_cell(method["delta_percent"]["p95"]),
                    markdown_escape(method["verdict"]),
                ]
            )
            + " |"
        )
    lines.extend(
        [
            "",
            (
                "A method changes only when both recomputed percentiles cross the "
                f"{THRESHOLD_PERCENT:.0f}% threshold. RSS is not part of the verdict."
            ),
            "",
        ]
    )
    return "\n".join(lines)


def render_artifact(
    input_directory: Path,
    context: Context,
    report_path: Path,
    comparison_path: Path,
) -> bool:
    try:
        samples = validate_artifact(input_directory, context)
        comparison = build_comparison(samples, context)
        valid = True
    except (BenchmarkError, OSError, ArithmeticError, KeyError, TypeError, ValueError) as error:
        comparison = inconclusive_comparison(context, str(error) or "artifact validation failed")
        valid = False
    _write_json_atomic(comparison_path.resolve(), comparison)
    _write_text_atomic(report_path.resolve(), render_markdown(comparison))
    return valid


def _add_context_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repository", required=True)
    parser.add_argument("--head-repository", required=True)
    parser.add_argument("--workflow-repository", required=True)
    parser.add_argument("--pr-number", required=True, type=int)
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--run-url", required=True)


def argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    run = subparsers.add_parser("run", help="run both benchmark pass orders")
    run.add_argument("--lsp-bench", required=True, type=Path)
    run.add_argument("--base-binary", required=True, type=Path)
    run.add_argument("--head-binary", required=True, type=Path)
    run.add_argument("--output", required=True, type=Path)
    _add_context_arguments(run)

    render = subparsers.add_parser(
        "render", help="validate an untrusted artifact and render trusted outputs"
    )
    render.add_argument("--input", required=True, type=Path)
    render.add_argument("--report", required=True, type=Path)
    render.add_argument("--comparison", required=True, type=Path)
    _add_context_arguments(render)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = argument_parser().parse_args(argv)
    try:
        context = validate_context(
            args.repository,
            args.head_repository,
            args.workflow_repository,
            args.pr_number,
            args.base_sha,
            args.head_sha,
            args.run_url,
        )
        if args.command == "run":
            manifest = run_benchmark(
                args.lsp_bench,
                args.base_binary,
                args.head_binary,
                context,
                args.output,
            )
            print(manifest)
            return 0
        valid = render_artifact(
            args.input, context, args.report, args.comparison
        )
        print(args.report)
        return 0 if valid else 1
    except BenchmarkError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
