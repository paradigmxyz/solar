#!/usr/bin/env python3
"""Run a focused Solc-vs-Solar symbolic differential."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import itertools
import json
import math
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any

if sys.platform != "win32":
    import resource

sys.path.insert(
    0, str(pathlib.Path(__file__).resolve().parents[2] / "tools/compiler-diff")
)
from compiler_diff.artifacts import read_attempt
from compiler_diff.compare import canonical_type as _canonical_type

SCHEMA = "solar:solsymdiff@v2"
TEST_NAME = "checkSymbolicDifferential"
DEFAULT_DYNAMIC_LENGTHS = (0, 1, 2, 3)
MAX_DYNAMIC_LENGTH = 256
SYMBOLIC_QUERY_TIMEOUT = 30
SYMBOLIC_MAX_SOLVER_QUERIES = 10_000
SYMBOLIC_MAX_CALLDATA_BYTES = 4096
SYMBOLIC_MAX_DEPTH = 100_000
_LOG_OPCODES = {0xA0, 0xA1, 0xA2, 0xA3, 0xA4}
# CALL, CALLCODE, DELEGATECALL, STATICCALL, CREATE, and CREATE2.
_INNER_FRAME_OPCODES = {0xF0, 0xF1, 0xF2, 0xF4, 0xF5, 0xFA}
# Labels of the fixed-size values whose encoding the label states.
_STORAGE_VALUE_LABEL = re.compile(
    r"u?int\d+|bytes\d+|bool|address( payable)?|contract .+|enum .+"
)
MIRROR_BASE_ADDRESS = "0x1000000000000000000000000000000000000004"
# Covers empty, short, selector-only, and one-word calldata.
RAW_CALLDATA_LENGTHS = (0, 3, 4, 36)
MAX_CHECKS = 256
# The comparisons the generated property names when it fails.
COMPARISONS = frozenset(
    {
        "balance",
        "call success",
        "code presence",
        "constructor returndata",
        "constructor success",
        "created accounts",
        "logs",
        "nonce",
        "returndata",
        "storage",
    }
)
UNSUPPORTED_PREFIX = "unsupported: "


class AbiDisagreement(ValueError):
    """The two builds expose different external functions or selectors."""


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="solsymdiff",
        description="Symbolically compare a contract compiled by Solc and Solar.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--source", required=True)
    parser.add_argument("--solc-attempt", type=pathlib.Path)
    parser.add_argument("--solar-attempt", type=pathlib.Path)
    parser.add_argument("--output-root", type=pathlib.Path)
    parser.add_argument("--contract", required=True)
    parser.add_argument("--signature")
    parser.add_argument(
        "--all-functions",
        action="store_true",
        help=(
            "compare every function, plus calldata that matches no selector, in one run"
        ),
    )
    parser.add_argument(
        "--depth",
        type=_positive_int,
        default=1,
        help=(
            "compare every sequence of up to this many calls; earlier calls "
            "are state-changing functions"
        ),
    )
    parser.add_argument(
        "--max-checks",
        type=_positive_int,
        default=MAX_CHECKS,
        help="refuse runs that would generate more call sequences",
    )
    parser.add_argument(
        "--constructor-args",
        type=_prefix_calldata,
        metavar="HEX",
        help="ABI-encoded constructor arguments instead of symbolic ones",
    )
    parser.add_argument(
        "--runtime-only",
        action="store_true",
        help="install the runtime without running the constructor",
    )
    parser.add_argument(
        "--allow-precompile-addresses",
        action="store_true",
        help=(
            "let top-level address inputs, callers, and the observer be "
            "precompile addresses"
        ),
    )
    parser.add_argument(
        "--arbitrary-storage",
        action="store_true",
        help=(
            "start every fixed storage slot of the target from a symbolic, "
            "validly encoded word instead of empty storage"
        ),
    )
    parser.add_argument(
        "--include-view",
        action="store_true",
        help="allow a view target under the clean zero-storage model",
    )
    parser.add_argument(
        "--include-stateful",
        action="store_true",
        help=(
            "allow a nonpayable target under the clean zero-storage, single-call model"
        ),
    )
    parser.add_argument(
        "--include-payable",
        action="store_true",
        help=(
            "allow a payable target and send it a symbolic value; every "
            "account's final balance is compared"
        ),
    )
    parser.add_argument(
        "--symbolic-caller",
        action="store_true",
        help="call the target from a symbolic msg.sender",
    )
    parser.add_argument(
        "--prefix-calldata",
        type=_prefix_calldata,
        action="append",
        default=[],
        metavar="HEX",
        help="prepare fixed zero-value target calls before symbolic comparison",
    )
    parser.add_argument("--project-root", type=pathlib.Path)
    parser.add_argument(
        "--include-path", type=pathlib.Path, action="append", default=[]
    )
    parser.add_argument("--remapping", action="append", default=[])
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--forge", default="forge")
    parser.add_argument("--solver", default="z3")
    parser.add_argument("--timeout", type=_positive_seconds, default=60.0)
    parser.add_argument(
        "--symbolic-timeout",
        type=_positive_int,
        default=SYMBOLIC_QUERY_TIMEOUT,
    )
    parser.add_argument("--max-paths", type=_positive_int, default=1024)
    parser.add_argument(
        "--max-solver-queries",
        type=_positive_int,
        default=SYMBOLIC_MAX_SOLVER_QUERIES,
    )
    parser.add_argument(
        "--max-calldata-bytes",
        type=_positive_int,
        default=SYMBOLIC_MAX_CALLDATA_BYTES,
    )
    parser.add_argument(
        "--max-depth",
        type=_positive_int,
        default=SYMBOLIC_MAX_DEPTH,
        help="opcode limit per path; one path runs both compilers' calls",
    )
    parser.add_argument(
        "--exploration-order",
        choices=("bfs", "dfs"),
        default="bfs",
    )
    parser.add_argument(
        "--dynamic-lengths",
        type=_dynamic_lengths,
        default=DEFAULT_DYNAMIC_LENGTHS,
    )
    parser.add_argument(
        "--input-length",
        type=_input_lengths,
        action="append",
        default=[],
        metavar="INDEX=LENGTHS",
    )
    parser.add_argument("--evm-version", default="osaka")
    parser.add_argument(
        "--optimize",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    parser.add_argument("--optimizer-runs", type=_nonnegative_int, default=200)
    parser.add_argument(
        "--via-ir",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    args = parser.parse_args(argv)

    try:
        result = run(args, output_root=args.output_root)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as err:
        result = {
            "schema": SCHEMA,
            "status": "incomplete",
            "reason": str(err),
            "source": str(args.source),
            "contract": args.contract,
            "signature": args.signature,
        }

    print(json.dumps(result, indent=2, sort_keys=True))
    return {
        "bounded_agreement": 0,
        "mismatch": 1,
        "incomplete": 2,
    }[result["status"]]


def run(
    args: argparse.Namespace, output_root: pathlib.Path | None = None
) -> dict[str, Any]:
    source = args.source
    saved_solc = getattr(args, "solc_attempt", None)
    saved_solar = getattr(args, "solar_attempt", None)
    if bool(saved_solc) != bool(saved_solar):
        raise ValueError("--solc-attempt and --solar-attempt must be supplied together")
    prefix_calldata = tuple(args.prefix_calldata)
    tool_names = (
        ("solc", "forge", "solver")
        if saved_solc
        else ("solc", "solar", "forge", "solver")
    )
    tools = {name: _resolve_executable(getattr(args, name)) for name in tool_names}
    # Forge maps bare solver names such as `bitwuzla` to their own commands and
    # runs any path as a z3-compatible command.
    if not os.path.dirname(args.solver):
        tools["solver"] = args.solver
    if saved_solc:
        source = args.source
        left_input, left_output = read_attempt(saved_solc)
        right_input, right_output = read_attempt(saved_solar)
        if json.dumps(left_input, sort_keys=True) != json.dumps(
            right_input, sort_keys=True
        ):
            raise ValueError("saved compiler attempts used different inputs")
        settings = left_input.get("settings", {})
        if not settings.get("evmVersion"):
            raise ValueError("saved symbolic inputs require an explicit evmVersion")
        args.evm_version = settings["evmVersion"]
        root_source = str(args.source)
        if root_source not in left_input.get("sources", {}):
            raise ValueError("--source must name a source unit in the saved input")
        serialized_input = json.dumps(
            left_input, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        )
        standard_input: dict[str, Any] = {
            "input": left_input,
            "root_source": root_source,
            "settings": settings,
            "sources": left_input["sources"],
            "sha256": hashlib.sha256(serialized_input.encode()).hexdigest(),
        }
        for output in (left_output, right_output):
            deployed = (
                output.get("contracts", {})
                .get(root_source, {})
                .get(args.contract, {})
                .get("evm", {})
                .get("deployedBytecode", {})
            )
            if not all(
                isinstance(deployed.get(key), dict)
                for key in ("immutableReferences", "linkReferences")
            ):
                raise ValueError(
                    "saved artifacts require immutableReferences and linkReferences outputs"
                )
        solc_artifact = _artifact(left_output, root_source, args.contract, "Solc")
        solar_artifact = _artifact(right_output, root_source, args.contract, "Solar")
    else:
        source = pathlib.Path(args.source).resolve()
        if not source.is_file():
            raise ValueError(f"source file does not exist: {source}")
        standard_input = _standard_input(
            tools["solc"],
            source,
            evm_version=args.evm_version,
            optimize=args.optimize,
            optimizer_runs=args.optimizer_runs,
            via_ir=args.via_ir,
            project_root=args.project_root,
            include_paths=tuple(args.include_path),
            remappings=tuple(args.remapping),
            timeout=args.timeout,
        )
        serialized_input = json.dumps(
            standard_input["input"],
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        )
        solc_artifact = _compile(
            tools["solc"],
            serialized_input,
            standard_input["root_source"],
            args.contract,
            args.timeout,
            "Solc",
        )
        solar_artifact = _compile(
            tools["solar"],
            serialized_input,
            standard_input["root_source"],
            args.contract,
            args.timeout,
            "Solar",
        )
    arbitrary_storage = getattr(args, "arbitrary_storage", False)
    allow_precompile_addresses = getattr(args, "allow_precompile_addresses", False)
    constructor_args = getattr(args, "constructor_args", None)
    if arbitrary_storage and "enum_members" not in standard_input:
        raise ValueError(
            "--arbitrary-storage needs the Solc AST for enum ranges, which saved "
            "attempts do not carry"
        )
    runtime_only = getattr(args, "runtime_only", False) or not (
        solc_artifact["creation"] and solar_artifact["creation"]
    )
    for artifact, label in ((solc_artifact, "Solc"), (solar_artifact, "Solar")):
        if runtime_only and artifact["has_immutables"]:
            raise ValueError("contracts with immutables require constructor execution")
        if not runtime_only and artifact["creation_links"]:
            raise ValueError(f"{label} creation code has unresolved libraries")
    if runtime_only and constructor_args is not None:
        raise ValueError("--constructor-args requires constructor execution")
    try:
        if not runtime_only and _function_shape(
            solc_artifact["constructor_entry"]
        ) != _function_shape(solar_artifact["constructor_entry"]):
            raise AbiDisagreement("compiler ABI disagreement for the constructor")
        checks, skipped = _plan_checks(
            solc_artifact, solar_artifact, args, prefix_calldata
        )
    except AbiDisagreement as err:
        # Different external interfaces are an observable difference.
        return {
            "schema": SCHEMA,
            "status": "mismatch",
            "differs_in": "abi",
            "reason": str(err),
            "source": str(source),
            "contract": args.contract,
            "signature": args.signature,
        }
    constructor = _harness_constructor(solc_artifact, runtime_only, constructor_args)
    if constructor and not all(_supported_input(item) for item in constructor):
        raise ValueError("the constructor uses an unsupported symbolic input")
    if not checks:
        raise ValueError("no function can be compared symbolically")
    if args.input_length:
        if getattr(args, "all_functions", False):
            raise ValueError("--input-length requires --signature")
        target = checks[0]["steps"][-1]["function"]
        overrides = _normalize_input_lengths(args.input_length, target["inputs"])
        for check in checks:
            for index, step in enumerate(check["steps"]):
                if step.get("function") == target and step["kind"] == "symbolic":
                    for name, lengths in overrides.items():
                        position = name.removeprefix("arg")
                        check["input_lengths"][f"s{index}_a{position}"] = lengths

    if arbitrary_storage:
        layout = solc_artifact.get("storage_layout")
        if not isinstance(layout, dict):
            raise ValueError("--arbitrary-storage requires Solc's storage layout")
        starting = _storage_constraints(layout, standard_input["enum_members"])
        for check in checks:
            check["starting_storage"] = starting
    # The creation code embeds the runtime, so it covers both.
    scanned = (
        (solc_artifact["runtime"], solar_artifact["runtime"])
        if runtime_only
        else (solc_artifact["creation"], solar_artifact["creation"])
    )
    opcodes = set().union(*(_runtime_opcodes(code) for code in scanned))
    limitations = []
    if 0xF5 in opcodes:
        limitations.append("contracts created with CREATE2 are not compared")
    if 0x5D in opcodes and not _forge_semantics(tools, args)["isolated_transactions"]:
        # Without symbolic call isolation, every call and the constructor run
        # in the test's transaction, so transient storage would persist between
        # calls that are separate transactions on chain.
        for check in checks:
            if (
                sum(step["kind"] != "fixed" for step in check["steps"])
                + len(prefix_calldata)
                + (not runtime_only)
                > 1
            ):
                check["unsupported"] = (
                    "transient storage would persist between calls that are "
                    "separate transactions on chain"
                )
    if (limit := _inner_frame_log_limit(*scanned)) and not _forge_semantics(
        tools, args
    )["reverted_logs_dropped"]:
        # A log committed in one build and reverted in the other matches in
        # both recordings, so agreement is qualified too.
        limitations.append(limit)
        for check in checks:
            check["log_limit"] = limit
    codes = [
        artifact[key]
        for artifact in (solc_artifact, solar_artifact)
        for key in ("runtime", "creation")
        if artifact[key]
    ]
    max_dynamic_length = max(
        MAX_DYNAMIC_LENGTH, *((len(code) - 2) // 2 for code in codes)
    )
    # The calldata budget is per call; one test feeds every call and the
    # constructor.
    symbolic_calls = max(
        sum(step["kind"] != "fixed" for step in check["steps"]) for check in checks
    )
    args.max_calldata_bytes *= symbolic_calls + bool(constructor)
    names = {name for check in checks for name, _ in _check_inputs(check, constructor)}
    extras = sorted(
        {
            kind
            for kind in ("caller", "value")
            if any(name.endswith(f"_{kind}") for name in names)
        }
        | ({"observer"} & names)
    )
    bounds = {
        "command_timeout_seconds": args.timeout,
        "solver_timeout_seconds": args.symbolic_timeout,
        "max_paths": args.max_paths,
        "max_solver_queries": args.max_solver_queries,
        "max_calldata_bytes": args.max_calldata_bytes,
        "max_dynamic_length": max_dynamic_length,
        "max_depth": args.max_depth,
        "exploration_order": args.exploration_order,
        "storage_layout": "zero_init",
        "symbolic_inputs": extras,
        "dynamic_lengths": list(args.dynamic_lengths),
        "constructor": (
            "runtime_only"
            if runtime_only
            else "symbolic_args"
            if constructor_args is None
            else "fixed_args"
        ),
        "call_depth": getattr(args, "depth", 1),
        "starting_storage": "arbitrary_fixed_slots" if arbitrary_storage else "empty",
        "address_inputs": (
            "any" if allow_precompile_addresses else "zero_or_above_0x100"
        ),
    }

    if output_root is None:
        output_root = (
            pathlib.Path(__file__).resolve().parents[2] / "target" / "solsymdiff"
        )
    output_root.mkdir(parents=True, exist_ok=True)
    prefix = args.contract if saved_solc else source.stem
    project = pathlib.Path(tempfile.mkdtemp(prefix=f"{prefix}-", dir=output_root))
    (project / "standard-input.json").write_text(
        serialized_input + "\n",
        encoding="utf-8",
    )
    runnable = [check for check in checks if not check.get("unsupported")]
    _write_project(
        project,
        solc_artifact,
        solar_artifact,
        runnable,
        args.evm_version,
        dynamic_lengths=args.dynamic_lengths,
        exploration_order=args.exploration_order,
        max_dynamic_length=max_dynamic_length,
        runtime_only=runtime_only,
        constructor_args=constructor_args,
        allow_precompile_addresses=allow_precompile_addresses,
    )
    report = (
        _run_forge(
            tools["forge"],
            tools["solc"],
            tools["solver"],
            project,
            args,
            max_dynamic_length,
        )
        if runnable
        else {}
    )
    result = _classify(report, checks, bounds, constructor=constructor)
    result["limitations"] = limitations
    result.update(
        {
            "schema": SCHEMA,
            "source": str(source),
            "contract": args.contract,
            "signature": args.signature,
            "checked": [check["label"] for check in checks],
            "skipped": skipped,
            "settings": standard_input["settings"],
            "standard_input_sha256": standard_input["sha256"],
            "sources": sorted(standard_input["sources"]),
            "bounds": bounds,
            "project": str(project),
        }
    )
    if saved_solc and saved_solar:
        result["attempts"] = {
            "solc": str(saved_solc.resolve()),
            "solar": str(saved_solar.resolve()),
        }
    if prefix_calldata:
        result["prefix"] = {"calldata": list(prefix_calldata), "value": 0}
    (project / "result.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return result


def _positive_seconds(value: str) -> float:
    seconds = float(value)
    if not math.isfinite(seconds) or seconds <= 0:
        raise argparse.ArgumentTypeError("timeout must be finite and positive")
    return seconds


def _positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("value must be positive")
    return number


def _nonnegative_int(value: str) -> int:
    number = int(value)
    if number < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return number


def _prefix_calldata(value: str) -> str:
    if not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})*", value):
        raise argparse.ArgumentTypeError(
            "prefix calldata must be 0x-prefixed, even-length hexadecimal"
        )
    return value.lower()


def _dynamic_lengths(value: str) -> tuple[int, ...]:
    parts = value.split(",")
    try:
        lengths = tuple(int(part) for part in parts)
    except ValueError as err:
        raise argparse.ArgumentTypeError(
            "dynamic lengths must be comma-separated integers"
        ) from err
    if (
        not lengths
        or any(
            not part or length < 0 or length > MAX_DYNAMIC_LENGTH
            for part, length in zip(parts, lengths, strict=True)
        )
        or len(set(lengths)) != len(lengths)
    ):
        raise argparse.ArgumentTypeError(
            f"dynamic lengths must be unique integers from 0 through {MAX_DYNAMIC_LENGTH}"
        )
    return lengths


def _input_lengths(value: str) -> tuple[int, tuple[int, ...]]:
    index, separator, lengths = value.partition("=")
    if not separator or not index.isdigit():
        raise argparse.ArgumentTypeError("input lengths must use INDEX=LENGTHS")
    return int(index), _dynamic_lengths(lengths)


def _normalize_input_lengths(
    overrides: list[tuple[int, tuple[int, ...]]],
    inputs: list[dict[str, Any]],
) -> dict[str, tuple[int, ...]]:
    normalized = {}
    for index, lengths in overrides:
        if index >= len(inputs):
            raise ValueError(f"input length override index {index} is out of range")
        abi_type = inputs[index].get("type")
        if not isinstance(abi_type, str) or not (
            abi_type in {"bytes", "string"} or abi_type.endswith("[]")
        ):
            raise ValueError(f"input {index} is not a top-level dynamic input")
        name = f"arg{index}"
        if name in normalized:
            raise ValueError(f"input length override repeats index {index}")
        normalized[name] = lengths
    return normalized


def _resolve_executable(value: str) -> str:
    path = pathlib.Path(value).expanduser()
    if path.parent != pathlib.Path("."):
        if path.is_file():
            return str(path.resolve())
        raise OSError(f"executable was not found: {value}")
    resolved = shutil.which(value)
    if resolved is None:
        raise OSError(f"executable was not found: {value}")
    return str(pathlib.Path(resolved).resolve())


def _standard_input(
    solc: str,
    source: pathlib.Path,
    *,
    evm_version: str,
    optimize: bool,
    optimizer_runs: int,
    via_ir: bool,
    project_root: pathlib.Path | None,
    include_paths: tuple[pathlib.Path, ...],
    remappings: tuple[str, ...],
    timeout: float,
) -> dict[str, Any]:
    source = source.resolve()
    project_root = (project_root or source.parent).resolve()
    include_paths = tuple(path.resolve() for path in include_paths)
    if not project_root.is_dir():
        raise ValueError(f"project root is not a directory: {project_root}")
    if not source.is_relative_to(project_root):
        raise ValueError(f"source is outside project root: {source}")
    if any(not path.is_dir() for path in include_paths):
        raise ValueError("every include path must be a directory")
    if any(
        not remapping or "=" not in remapping or not all(remapping.split("=", 1))
        for remapping in remappings
    ):
        raise ValueError("remappings must use non-empty prefix=target syntax")

    root_source = source.relative_to(project_root).as_posix()
    discovery_input = {
        "language": "Solidity",
        "sources": {root_source: {"content": source.read_text(encoding="utf-8")}},
        "settings": {
            "evmVersion": evm_version,
            "outputSelection": {"*": {"": ["ast"]}},
            **({"remappings": list(remappings)} if remappings else {}),
        },
    }
    command = [solc, "--base-path", str(project_root)]
    for include_path in include_paths:
        command.extend(["--include-path", str(include_path)])
    command.append("--standard-json")
    with tempfile.TemporaryDirectory(prefix="solar-solsymdiff-imports-") as cwd:
        result = subprocess.run(
            command,
            check=False,
            input=json.dumps(discovery_input),
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
        )
    output = _compiler_json(result, "Solc import discovery")
    discovered = output.get("sources")
    if not isinstance(discovered, dict) or root_source not in discovered:
        raise ValueError("Solc import discovery did not return the root source")
    enum_members = {}
    for unit in discovered.values():
        for node in _ast_nodes(unit.get("ast") if isinstance(unit, dict) else None):
            if node.get("nodeType") == "EnumDefinition":
                enum_members[node.get("canonicalName")] = len(node.get("members", []))

    roots = (project_root, *include_paths)
    sources = {}
    for name in sorted(discovered):
        if name == root_source:
            path = source
        else:
            candidates = tuple((root / name).resolve() for root in roots)
            path = next(
                (candidate for candidate in candidates if candidate.is_file()), None
            )
            if path is None:
                raise ValueError(f"could not snapshot imported source unit: {name}")
        sources[name] = {"content": path.read_text(encoding="utf-8")}

    settings = {
        "optimizer": {"enabled": optimize, "runs": optimizer_runs},
        "viaIR": via_ir,
        "evmVersion": evm_version,
        "metadata": {"bytecodeHash": "none"},
        "outputSelection": {
            "*": {
                "*": [
                    "abi",
                    "evm.bytecode.linkReferences",
                    "evm.bytecode.object",
                    "evm.deployedBytecode.immutableReferences",
                    "evm.deployedBytecode.linkReferences",
                    "evm.deployedBytecode.object",
                    "evm.methodIdentifiers",
                    "storageLayout",
                ]
            }
        },
        **({"remappings": list(remappings)} if remappings else {}),
    }
    value = {"language": "Solidity", "sources": sources, "settings": settings}
    serialized = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    return {
        "input": value,
        "root_source": root_source,
        "settings": settings,
        "sources": sources,
        "sha256": hashlib.sha256(serialized.encode()).hexdigest(),
        "enum_members": enum_members,
    }


def _ast_nodes(node: Any) -> list[dict[str, Any]]:
    """Returns every AST node below `node`, depth first."""
    found = []
    stack = [node]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            found.append(item)
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)
    return found


def _compile(
    compiler: str,
    standard_input: str,
    source_name: str,
    contract: str,
    timeout: float,
    label: str,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="solar-solsymdiff-compile-") as cwd:
        result = subprocess.run(
            [compiler, "--standard-json"],
            check=False,
            input=standard_input,
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
        )
    return _artifact(_compiler_json(result, label), source_name, contract, label)


def _artifact(output, source_name, contract, label):
    contracts = output.get("contracts")
    source_contracts = (
        contracts.get(source_name) if isinstance(contracts, dict) else None
    )
    artifact = (
        source_contracts.get(contract) if isinstance(source_contracts, dict) else None
    )
    if not isinstance(artifact, dict):
        raise ValueError(f"{label} did not emit {source_name}:{contract}")

    evm = artifact.get("evm")
    deployed = evm.get("deployedBytecode") if isinstance(evm, dict) else None
    identifiers = evm.get("methodIdentifiers") if isinstance(evm, dict) else None
    abi = artifact.get("abi")
    if (
        not isinstance(deployed, dict)
        or not isinstance(identifiers, dict)
        or not isinstance(abi, list)
    ):
        raise ValueError(f"{label} emitted a malformed contract artifact")
    if deployed.get("linkReferences"):
        raise ValueError("contracts with unresolved libraries are unsupported")

    runtime = deployed.get("object")
    if not isinstance(runtime, str) or not runtime:
        raise ValueError(f"{label} emitted no deployed runtime")
    payload = runtime.removeprefix("0x")
    if len(payload) % 2:
        raise ValueError(f"{label} emitted bytecode with an odd length")
    try:
        bytes.fromhex(payload)
    except ValueError as err:
        raise ValueError(f"{label} emitted non-hex runtime bytecode") from err

    creation = evm.get("bytecode")
    creation_code = creation.get("object") if isinstance(creation, dict) else None
    if creation_code is not None and (
        not isinstance(creation_code, str)
        or not re.fullmatch(r"(?:0x)?(?:[0-9a-fA-F]{2})+", creation_code)
    ):
        raise ValueError(f"{label} emitted invalid creation bytecode")
    constructors = [
        entry
        for entry in abi
        if isinstance(entry, dict) and entry.get("type") == "constructor"
    ]
    constructor = (
        constructors[0]
        if constructors
        else {"inputs": [], "outputs": [], "stateMutability": "nonpayable"}
    )
    constructor = {"outputs": [], **constructor}
    return {
        "abi": abi,
        "method_identifiers": identifiers,
        "runtime": "0x" + payload.lower(),
        "creation": "0x" + creation_code.removeprefix("0x").lower()
        if creation_code
        else None,
        "creation_links": bool(
            isinstance(creation, dict) and creation.get("linkReferences")
        ),
        "has_immutables": bool(deployed.get("immutableReferences")),
        "storage_layout": artifact.get("storageLayout"),
        "constructor_entry": constructor,
        "constructor": constructor["inputs"],
        "payable_fallback": any(
            isinstance(entry, dict)
            and entry.get("type") in {"fallback", "receive"}
            and entry.get("stateMutability") == "payable"
            for entry in abi
        ),
    }


def _compiler_json(
    result: subprocess.CompletedProcess[str],
    label: str,
) -> dict[str, Any]:
    try:
        output = json.loads(result.stdout)
    except json.JSONDecodeError as err:
        detail = result.stderr.strip() or str(err)
        raise ValueError(f"{label} did not emit Standard JSON: {detail}") from err
    if not isinstance(output, dict):
        raise ValueError(f"{label} Standard JSON output must be an object")
    errors = output.get("errors", [])
    failures = [
        error.get("formattedMessage") or error.get("message") or str(error)
        for error in errors
        if isinstance(error, dict) and error.get("severity") == "error"
    ]
    if result.returncode != 0 or failures:
        detail = "\n".join(failures) or result.stderr.strip()
        raise ValueError(f"{label} compilation failed: {detail}")
    return output


def _select_function(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    signature: str,
    *,
    include_view: bool = False,
    include_stateful: bool = False,
    include_payable: bool = False,
) -> dict[str, Any]:
    solc_entry = _function_entry(solc_artifact["abi"], signature, "Solc")
    try:
        solar_entry = _function_entry(solar_artifact["abi"], signature, "Solar")
    except ValueError as err:
        raise AbiDisagreement(str(err)) from err
    if _function_shape(solc_entry) != _function_shape(solar_entry):
        raise AbiDisagreement(f"compiler ABI disagreement for {signature}")
    solc_selector = _selector(solc_artifact["method_identifiers"], signature, "Solc")
    solar_selector = _selector(
        solar_artifact["method_identifiers"],
        signature,
        "Solar",
    )
    if solc_selector != solar_selector:
        raise AbiDisagreement(
            f"compiler selector disagreement for {signature}: "
            f"Solc={solc_selector}, Solar={solar_selector}"
        )
    mutability = solc_entry.get("stateMutability")
    allowed = {"pure"}
    if include_view:
        allowed.add("view")
    if include_stateful:
        allowed.add("nonpayable")
    if include_payable:
        allowed.add("payable")
    if mutability not in allowed:
        choices = ", ".join(sorted(allowed))
        raise ValueError(
            f"{signature} has mutability {mutability!r}; allowed: {choices}"
        )
    if not all(_supported_input(item) for item in solc_entry["inputs"]):
        raise ValueError(f"{signature} uses an unsupported symbolic input")
    return {
        "signature": signature,
        "selector": solc_selector,
        "inputs": solc_entry["inputs"],
        "mutability": mutability,
    }


def _function_entry(
    abi: list[dict[str, Any]],
    signature: str,
    label: str,
) -> dict[str, Any]:
    matches = [
        entry
        for entry in abi
        if isinstance(entry, dict)
        and entry.get("type") == "function"
        and _abi_signature(entry) == signature
    ]
    if len(matches) != 1:
        raise ValueError(f"{label} must contain exactly one {signature} function")
    return matches[0]


def _abi_signature(entry: dict[str, Any]) -> str:
    name = entry.get("name")
    inputs = entry.get("inputs")
    if not isinstance(name, str) or not isinstance(inputs, list):
        raise ValueError("function ABI entry is malformed")
    return f"{name}({','.join(_canonical_type(item) for item in inputs)})"


def _function_shape(entry: dict[str, Any]) -> tuple[Any, ...]:
    inputs = entry.get("inputs")
    outputs = entry.get("outputs")
    if not isinstance(inputs, list) or not isinstance(outputs, list):
        raise ValueError("function ABI entry is malformed")
    return (
        tuple(_canonical_type(item) for item in inputs),
        tuple(_canonical_type(item) for item in outputs),
        entry.get("stateMutability"),
    )


def _selector(identifiers: dict[str, Any], signature: str, label: str) -> str:
    value = identifiers.get(signature)
    if not isinstance(value, str):
        raise ValueError(f"{label} emitted no selector for {signature}")
    payload = value.removeprefix("0x")
    if len(payload) != 8:
        raise ValueError(f"{label} emitted an invalid selector for {signature}")
    try:
        bytes.fromhex(payload)
    except ValueError as err:
        raise ValueError(f"{label} emitted a non-hex selector for {signature}") from err
    return "0x" + payload.lower()


def _supported_input(item: dict[str, Any]) -> bool:
    abi_type = item.get("type")
    if not isinstance(abi_type, str):
        return False
    if abi_type.startswith("tuple"):
        components = item.get("components")
        base_type = "uint256" + abi_type[len("tuple") :]
        return (
            isinstance(components, list)
            and bool(components)
            and _supported_elementary(base_type)
            and all(
                isinstance(component, dict) and _supported_input(component)
                for component in components
            )
        )
    return _supported_elementary(abi_type)


def _supported_elementary(abi_type: str) -> bool:
    while abi_type.endswith("]"):
        start = abi_type.rfind("[")
        if start < 0:
            return False
        length = abi_type[start + 1 : -1]
        if length and (not length.isdigit() or int(length) <= 0):
            return False
        abi_type = abi_type[:start]

    if abi_type in {"address", "bool", "bytes", "string"}:
        return True
    if abi_type.startswith(("uint", "int")):
        width = abi_type[4:] if abi_type.startswith("uint") else abi_type[3:]
        return not width or (width.isdigit() and int(width) in range(8, 257, 8))
    if abi_type.startswith("bytes"):
        width = abi_type[5:]
        return width.isdigit() and 1 <= int(width) <= 32
    return False


def _solidity_type(
    item: dict[str, Any],
    name: str,
    definitions: list[str],
) -> str:
    abi_type = item["type"]
    if not abi_type.startswith("tuple"):
        return abi_type

    fields = []
    for index, component in enumerate(item["components"]):
        field_type = _solidity_type(
            component,
            f"{name}_{index}",
            definitions,
        )
        fields.append(f"        {field_type} field{index};")
    definitions.append(
        "\n".join(
            [
                f"    struct {name} {{",
                *fields,
                "    }",
            ]
        )
    )
    return name + abi_type[len("tuple") :]


def _plan_checks(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    args: argparse.Namespace,
    prefix_calldata: tuple[str, ...],
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Returns the call sequences to compare and the functions left out.

    Raises `AbiDisagreement` when the two builds expose different functions.
    """
    symbolic_caller = getattr(args, "symbolic_caller", False)
    all_functions = getattr(args, "all_functions", False)
    depth = getattr(args, "depth", 1)
    if all_functions or depth > 1:
        solc_signatures = _function_signatures(solc_artifact["abi"])
        solar_signatures = _function_signatures(solar_artifact["abi"])
        if solc_signatures != solar_signatures:
            raise AbiDisagreement(
                "compiler ABI disagreement: only Solc has "
                f"{sorted(solc_signatures - solar_signatures)}, only Solar has "
                f"{sorted(solar_signatures - solc_signatures)}"
            )
    skipped = []
    selectable = []
    for signature in sorted(_function_signatures(solc_artifact["abi"])):
        try:
            selectable.append(
                _select_function(
                    solc_artifact,
                    solar_artifact,
                    signature,
                    include_view=True,
                    include_stateful=True,
                    include_payable=True,
                )
            )
        except AbiDisagreement:
            raise
        except ValueError as err:
            skipped.append({"signature": signature, "reason": str(err)})
    if all_functions:
        functions = selectable
    else:
        if not args.signature:
            raise ValueError("--signature is required without --all-functions")
        functions = [
            _select_function(
                solc_artifact,
                solar_artifact,
                args.signature,
                include_view=args.include_view or depth > 1,
                include_stateful=args.include_stateful or depth > 1,
                include_payable=getattr(args, "include_payable", False) or depth > 1,
            )
        ]
        if depth == 1:
            skipped = []
        if prefix_calldata and functions[0]["mutability"] == "pure":
            raise ValueError("prefix calls require a view or nonpayable target")

    def step(kind: str, function: dict[str, Any] | None = None) -> dict[str, Any]:
        planned: dict[str, Any] = {"kind": kind, "caller": symbolic_caller}
        if function is not None:
            planned["function"] = function
        if kind == "raw":
            # Calldata that matches no selector reaches `fallback` or `receive`.
            planned["known_selectors"] = sorted(
                {
                    "0x" + selector.removeprefix("0x").lower()
                    for artifact in (solc_artifact, solar_artifact)
                    for selector in artifact["method_identifiers"].values()
                }
            )
            planned["lengths"] = RAW_CALLDATA_LENGTHS
        elif kind == "raw_args" and function is not None:
            # Arbitrary argument bytes after a known selector exercise the
            # compilers' ABI decoding: short data, dirty values, bad offsets.
            head = sum(_abi_head(item)[0] for item in function["inputs"])
            planned["lengths"] = tuple(
                sorted(
                    length
                    for length in {0, max(head - 1, 0), head, head + 32}
                    if length <= MAX_DYNAMIC_LENGTH
                )
            )
        return planned

    last_steps = [step("symbolic", function) for function in functions]
    if all_functions:
        last_steps += [step("raw_args", function) for function in functions]
        last_steps.append(step("raw"))
    state_changing = [
        step("symbolic", function)
        for function in (functions if all_functions else selectable)
        if function["mutability"] in {"nonpayable", "payable"}
    ]
    sequences = []
    for length in range(1, depth + 1):
        for earlier in itertools.product(state_changing, repeat=length - 1):
            for last in last_steps:
                sequences.append([*earlier, last])
    max_checks = getattr(args, "max_checks", MAX_CHECKS)
    if len(sequences) > max_checks:
        raise ValueError(
            f"{len(sequences)} call sequences exceed --max-checks {max_checks}"
        )
    fixed = [{"kind": "fixed", "calldata": calldata} for calldata in prefix_calldata]
    checks = []
    for index, sequence in enumerate(sequences):
        steps = [*fixed, *sequence]
        checks.append(
            {
                "name": f"{TEST_NAME}{index}",
                "label": " -> ".join(_step_label(item) for item in sequence),
                "steps": steps,
                "input_lengths": {
                    f"s{position}_raw": item["lengths"]
                    for position, item in enumerate(steps)
                    if "lengths" in item
                },
            }
        )
    return checks, skipped


def _function_signatures(abi: list[dict[str, Any]]) -> set[str]:
    return {
        _abi_signature(entry)
        for entry in abi
        if isinstance(entry, dict) and entry.get("type") == "function"
    }


def _step_label(step: dict[str, Any]) -> str:
    if step["kind"] == "raw":
        return "<other calldata>"
    if step["kind"] == "raw_args":
        return f"{step['function']['signature']} <arbitrary arguments>"
    if step["kind"] == "fixed":
        return step["calldata"]
    return step["function"]["signature"]


def _step_inputs(index: int, step: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    """Names the test parameters that feed one step, in ABI order."""
    if step["kind"] == "fixed":
        return []
    if step["kind"] in {"raw", "raw_args"}:
        inputs = [(f"s{index}_raw", {"name": "raw", "type": "bytes"})]
    else:
        inputs = [
            (f"s{index}_a{position}", item)
            for position, item in enumerate(step["function"]["inputs"])
        ]
    if step["caller"]:
        inputs.append((f"s{index}_caller", {"name": "caller", "type": "address"}))
    # Every call carries a symbolic value, so a missing `callvalue` check on a
    # nonpayable function is a difference too.
    inputs.append((f"s{index}_value", {"name": "value", "type": "uint256"}))
    return inputs


def _check_inputs(
    check: dict[str, Any],
    constructor: list[dict[str, Any]] | None,
) -> list[tuple[str, dict[str, Any]]]:
    """Names every test parameter of one check; `constructor` is None without one."""
    inputs = [(f"c{position}", item) for position, item in enumerate(constructor or [])]
    if constructor is not None:
        inputs.append(("c_value", {"name": "value", "type": "uint256"}))
    for slot, *_ in check.get("starting_storage", ()):
        inputs.append((f"st{slot}", {"name": "word", "type": "uint256"}))
    for index, step in enumerate(check["steps"]):
        inputs.extend(_step_inputs(index, step))
    if any(step["kind"] != "fixed" for step in check["steps"]):
        inputs.append(("observer", {"name": "observer", "type": "address"}))
    return inputs


def _write_project(
    project: pathlib.Path,
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    checks: list[dict[str, Any]],
    evm_version: str,
    *,
    max_dynamic_length: int,
    dynamic_lengths: tuple[int, ...] = DEFAULT_DYNAMIC_LENGTHS,
    exploration_order: str = "bfs",
    runtime_only: bool = False,
    constructor_args: str | None = None,
    allow_precompile_addresses: bool = False,
) -> None:
    (project / "src").mkdir()
    (project / "test").mkdir()

    constructor = _harness_constructor(solc_artifact, runtime_only, constructor_args)
    definitions: list[str] = []
    functions = [
        _render_check(
            check,
            constructor,
            definitions,
            runtime_only=runtime_only,
            constructor_args=constructor_args,
            allow_precompile_addresses=allow_precompile_addresses,
        )
        for check in checks
    ]
    if runtime_only:
        codes = (
            f'    bytes private constant SOLC_RUNTIME = hex"{solc_artifact["runtime"].removeprefix("0x")}";\n'
            f'    bytes private constant SOLAR_RUNTIME = hex"{solar_artifact["runtime"].removeprefix("0x")}";\n'
        )
    else:
        codes = (
            f'    bytes private constant SOLC_INIT = hex"{solc_artifact["creation"].removeprefix("0x")}";\n'
            f'    bytes private constant SOLAR_INIT = hex"{solar_artifact["creation"].removeprefix("0x")}";\n'
        )
    test_source = (
        _TEST_HEADER.format(
            definitions="\n\n".join(definitions),
            codes=codes,
            mirror_base=MIRROR_BASE_ADDRESS,
        )
        + "\n".join(functions)
        + _TEST_FOOTER
    )
    (project / "test" / "SymbolicDifferential.t.sol").write_text(
        test_source,
        encoding="utf-8",
    )
    lengths = ", ".join(str(length) for length in dynamic_lengths)
    (project / "foundry.toml").write_text(
        _FOUNDRY_TOML.format(
            evm_version=evm_version,
            dynamic_lengths=lengths,
            exploration_order=exploration_order,
            max_dynamic_length=max_dynamic_length,
            storage_layout="zero_init",
        ),
        encoding="utf-8",
    )


def _harness_constructor(
    solc_artifact: dict[str, Any],
    runtime_only: bool,
    constructor_args: str | None,
) -> list[dict[str, Any]] | None:
    """Returns the constructor's symbolic inputs, or None when it does not run."""
    if runtime_only:
        return None
    return [] if constructor_args is not None else solc_artifact["constructor"]


def _render_check(
    check: dict[str, Any],
    constructor: list[dict[str, Any]] | None,
    definitions: list[str],
    *,
    runtime_only: bool,
    constructor_args: str | None = None,
    allow_precompile_addresses: bool = False,
) -> str:
    inputs = _check_inputs(check, constructor)
    declarations = []
    for name, item in inputs:
        type_name = _solidity_type(item, f"{check['name']}_{name}", definitions)
        abi_type = item["type"]
        location = (
            " calldata"
            if abi_type.startswith("tuple")
            or abi_type in {"bytes", "string"}
            or "[" in abi_type
            else ""
        )
        declarations.append(f"{type_name}{location} {name}")

    steps = check["steps"]
    lines = []
    if check["input_lengths"]:
        lengths = ", ".join(
            f"{name} = [{', '.join(str(length) for length in values)}]"
            for name, values in sorted(check["input_lengths"].items())
        )
        lines.append(
            f"    /// forge-config: default.symbolic.dynamic_lengths = {{ {lengths} }}"
        )
    lines.append(f"    function {check['name']}({', '.join(declarations)}) public {{")
    if not allow_precompile_addresses:
        # A symbolic address may otherwise be a precompile whose symbolic
        # input Foundry does not model, which leaves the whole run incomplete.
        for name, item in inputs:
            if item["type"] == "address":
                lines.append(
                    f"        vm.assume(uint160({name}) == 0 || uint160({name}) > 0x100);"
                )
    lines.append(f"        Call[] memory calls = new Call[]({len(steps)});")
    for index, step in enumerate(steps):
        sender = f"s{index}_caller" if step.get("caller") else "address(this)"
        prank = "true" if step.get("caller") else "false"
        value = "0" if step["kind"] == "fixed" else f"s{index}_value"
        if step["kind"] == "fixed":
            data = f'hex"{step["calldata"].removeprefix("0x")}"'
        elif step["kind"] == "raw":
            data = f"s{index}_raw"
            lines.append(f"        if (s{index}_raw.length >= 4) {{")
            lines.append(f"            bytes4 selector = bytes4(s{index}_raw[:4]);")
            for selector in step["known_selectors"]:
                lines.append(f"            vm.assume(selector != bytes4({selector}));")
            lines.append("        }")
        elif step["kind"] == "raw_args":
            selector = step["function"]["selector"]
            data = f"abi.encodePacked(bytes4({selector}), s{index}_raw)"
        else:
            function = step["function"]
            arguments = "".join(
                f", s{index}_a{position}" for position in range(len(function["inputs"]))
            )
            data = f"abi.encodeWithSelector(bytes4({function['selector']}){arguments})"
        lines.append(
            f"        calls[{index}] = Call({data}, {sender}, {prank}, {value});"
        )
    watched = ["address(this)"]
    for index, step in enumerate(steps):
        if step.get("caller"):
            watched.append(f"s{index}_caller")
    if any(name == "observer" for name, _ in inputs):
        watched.append("observer")
    lines.append(f"        address[] memory watched = new address[]({len(watched)});")
    for index, account in enumerate(watched):
        lines.append(f"        watched[{index}] = {account};")
    if runtime_only:
        lines.append("        bytes memory initA = _runtimeInit(SOLC_RUNTIME);")
        lines.append("        bytes memory initB = _runtimeInit(SOLAR_RUNTIME);")
        init_value = "0"
    else:
        if constructor_args is not None:
            arguments = f'hex"{constructor_args.removeprefix("0x")}"'
        else:
            names = ", ".join(
                f"c{position}" for position in range(len(constructor or []))
            )
            arguments = f"abi.encode({names})"
        lines.append(
            f"        bytes memory initA = abi.encodePacked(SOLC_INIT, {arguments});"
        )
        lines.append(
            f"        bytes memory initB = abi.encodePacked(SOLAR_INIT, {arguments});"
        )
        init_value = "c_value"
    starting = check.get("starting_storage", ())
    lines.append(f"        Slot[] memory start = new Slot[]({len(starting)});")
    full = (1 << 256) - 1
    for index, (slot, mask, bools, enums) in enumerate(starting):
        # Only validly encoded storage: compilers may treat other words
        # differently, and Solidity code never stores them.
        word = f"st{slot}"
        if mask != full:
            lines.append(f"        vm.assume({word} & {hex(full ^ mask)} == 0);")
        for offset in bools:
            lines.append(f"        vm.assume(({word} >> {8 * offset}) & 0xff <= 1);")
        for offset, members in enums:
            lines.append(
                f"        vm.assume(({word} >> {8 * offset}) & 0xff < {members});"
            )
        lines.append(
            f"        start[{index}] = Slot(bytes32(uint256({slot})), bytes32({word}));"
        )
    compare = "_compareFromStartingStorage" if starting else "_compare"
    lines.append(
        f"        {compare}(initA, initB, {init_value}, start, calls, watched);"
    )
    lines.append("    }")
    return "\n".join(lines) + "\n"


def _storage_constraints(
    layout: Any,
    enum_members: dict[str, int] | None = None,
) -> list[tuple[int, int, list[int], list[tuple[int, int]]]]:
    """Returns each fixed slot's used-bit mask, `bool` offsets, and enum offsets.

    Slots that hold any value other than fixed-size fields are left out.
    """
    if not isinstance(layout, dict):
        return []
    enum_members = enum_members or {}
    types = layout.get("types") or {}
    full = (1 << 256) - 1
    slots: dict[int, tuple[int, list[int], list[tuple[int, int]]]] = {}
    unconstrained = set()

    def use(
        slot: int,
        mask: int,
        bools: list[int],
        enums: list[tuple[int, int]],
    ) -> None:
        current, current_bools, current_enums = slots.get(slot, (0, [], []))
        slots[slot] = (current | mask, current_bools + bools, current_enums + enums)

    def visit(type_id: str, slot: int, offset: int) -> None:
        info = types.get(type_id)
        if not isinstance(info, dict):
            unconstrained.add(slot)
            return
        encoding = info.get("encoding")
        size = int(info.get("numberOfBytes", 32))
        label = info.get("label", "")
        if encoding != "inplace":
            # Mappings, dynamic arrays, `bytes`, and `string` start empty: their
            # contents live at hashed slots that replay cannot materialize.
            unconstrained.add(slot)
        elif "members" in info:
            for member in info["members"]:
                visit(member["type"], slot + int(member["slot"]), int(member["offset"]))
        elif "base" in info:
            count = re.search(r"\[(\d+)\]$", label)
            base = types.get(info["base"], {})
            element = int(base.get("numberOfBytes", 32))
            if count is None:
                unconstrained.add(slot)
                return
            per_slot = 32 // element if element < 32 else 0
            for index in range(int(count.group(1))):
                if per_slot:
                    visit(
                        info["base"],
                        slot + index // per_slot,
                        (index % per_slot) * element,
                    )
                else:
                    visit(info["base"], slot + index * (element // 32), 0)
        elif not _STORAGE_VALUE_LABEL.fullmatch(label):
            # User-defined value types and internal function pointers hide
            # their encoding behind the label: a user-defined `bool` still
            # must hold 0 or 1, and each compiler numbers function pointers
            # on its own.
            unconstrained.add(slot)
        else:
            mask = ((1 << (8 * size)) - 1) << (8 * offset)
            enums = []
            if label.startswith("enum "):
                members = enum_members.get(label.removeprefix("enum "))
                if members is None:
                    raise ValueError(f"unknown member count for storage {label}")
                enums.append((offset, members))
            use(slot, mask & full, [offset] if label == "bool" else [], enums)

    for entry in layout.get("storage") or []:
        if isinstance(entry, dict):
            visit(entry["type"], int(entry["slot"]), int(entry["offset"]))
    return [
        (slot, mask, bools, enums)
        for slot, (mask, bools, enums) in sorted(slots.items())
        if slot not in unconstrained
    ]


def _runtime_opcodes(runtime: str) -> set[int]:
    """Returns the opcodes of runtime or creation code, skipping PUSH data and trailing metadata."""
    code = bytes.fromhex(runtime.removeprefix("0x"))
    if len(code) >= 2:
        metadata = int.from_bytes(code[-2:]) + 2
        if metadata < len(code) and 0xA1 <= code[-metadata] <= 0xA6:
            code = code[:-metadata]
    opcodes = set()
    pc = 0
    while pc < len(code):
        opcode = code[pc]
        opcodes.add(opcode)
        pc += 1 + (opcode - 0x5F if 0x60 <= opcode <= 0x7F else 0)
    return opcodes


def _inner_frame_log_limit(*codes: str) -> str | None:
    """Reports code that can log in an inner frame that later reverts.

    Foundry records the logs of every frame, including inner frames that revert
    inside a successful call, and attributes no log to a frame, so the harness
    cannot drop them. A `logs` difference in such code may come from one.
    """
    for code in codes:
        opcodes = _runtime_opcodes(code)
        if opcodes & _LOG_OPCODES and opcodes & _INNER_FRAME_OPCODES:
            return (
                "logs of reverted inner frames are not told apart from committed logs"
            )
    return None


_SEMANTICS_PROBE = """// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.24;

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}

contract Transient {
    function set() external {
        assembly {
            tstore(0, 1)
        }
    }

    function get() external view returns (uint256 value) {
        assembly {
            value := tload(0)
        }
    }
}

contract Logger {
    event Logged();

    function inner() external {
        emit Logged();
        revert();
    }

    function outer() external {
        try this.inner() {} catch {}
    }
}

contract Probe {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function checkIsolatedTransactions() public {
        Transient t = new Transient();
        t.set();
        require(t.get() == 0);
    }

    function checkRevertedLogsDropped() public {
        Logger logger = new Logger();
        vm.recordLogs();
        logger.outer();
        require(vm.getRecordedLogs().length == 0);
    }
}
"""
_SEMANTICS: dict[tuple[str, int, int], dict[str, bool]] = {}


def _forge_semantics(
    tools: dict[str, str], args: argparse.Namespace
) -> dict[str, bool]:
    """Probes what the symbolic engine of this Forge models, once per binary.

    Older Forge runs the whole symbolic test as one transaction, so transient
    storage persists between the harness's calls (foundry-rs/foundry#17865), and
    records the logs of inner frames that revert (foundry-rs/foundry#17866).
    The harness reports those cases as incomplete only when Forge has the gap.
    """
    forge = tools["forge"]
    stat = os.stat(forge)
    key = (os.path.realpath(forge), stat.st_mtime_ns, stat.st_size)
    if key in _SEMANTICS:
        return _SEMANTICS[key]
    cache_root = pathlib.Path(
        os.environ.get("XDG_CACHE_HOME") or pathlib.Path.home() / ".cache"
    )
    digest = hashlib.sha256(repr(key).encode()).hexdigest()[:16]
    cache = cache_root / "solsymdiff" / f"forge-semantics-{digest}.json"
    with contextlib.suppress(OSError, ValueError):
        _SEMANTICS[key] = json.loads(cache.read_text())
        return _SEMANTICS[key]
    semantics = {}
    with tempfile.TemporaryDirectory(prefix="solsymdiff-probe-") as tmp:
        project = pathlib.Path(tmp)
        (project / "test").mkdir()
        (project / "test" / "Probe.t.sol").write_text(_SEMANTICS_PROBE)
        (project / "foundry.toml").write_text('[profile.default]\ntest = "test"\n')
        env = {
            name: value
            for name, value in os.environ.items()
            if not name.startswith(("FOUNDRY_", "DAPP_"))
        }
        help_output = subprocess.run(
            [forge, "test", "--help"], check=False, text=True, capture_output=True
        ).stdout
        local = (
            ["--allow-local-compiler"]
            if "--allow-local-compiler" in help_output
            else []
        )
        for name, test in (
            ("isolated_transactions", "checkIsolatedTransactions"),
            ("reverted_logs_dropped", "checkRevertedLogsDropped"),
        ):
            result = subprocess.run(
                [
                    forge,
                    "test",
                    "--root",
                    str(project),
                    "--use",
                    tools["solc"],
                    *local,
                    "--evm-version",
                    "cancun",
                    "--offline",
                    "--symbolic",
                    "--symbolic-solver",
                    tools["solver"],
                    "--match-test",
                    f"^{test}",
                ],
                check=False,
                cwd=project,
                env=env,
                text=True,
                capture_output=True,
                timeout=args.timeout,
            )
            semantics[name] = result.returncode == 0 and "[PASS]" in result.stdout
    _SEMANTICS[key] = semantics
    try:
        cache.parent.mkdir(parents=True, exist_ok=True)
        partial = cache.with_suffix(f".{os.getpid()}.tmp")
        partial.write_text(json.dumps(semantics))
        partial.replace(cache)
    except OSError:
        pass
    return semantics


def _run_forge(
    forge: str,
    solc: str,
    solver: str,
    project: pathlib.Path,
    args: argparse.Namespace,
    max_dynamic_length: int,
) -> dict[str, Any]:
    help_output = subprocess.run(
        [forge, "test", "--help"],
        check=False,
        text=True,
        capture_output=True,
        timeout=args.timeout,
    ).stdout
    test_pattern = f"^{TEST_NAME}"
    command = [
        forge,
        "test",
        "--root",
        str(project),
        "--use",
        solc,
        "--evm-version",
        args.evm_version,
        "--symbolic",
        "--match-test",
        test_pattern,
        "--symbolic-solver",
        solver,
        "--symbolic-timeout",
        str(args.symbolic_timeout),
        "--symbolic-max-paths",
        str(args.max_paths),
        "--symbolic-max-solver-queries",
        str(args.max_solver_queries),
        "--symbolic-max-calldata-bytes",
        str(args.max_calldata_bytes),
        "--symbolic-max-dynamic-length",
        str(max_dynamic_length),
        "--json",
    ]
    if "--allow-local-compiler" in help_output:
        command.insert(command.index("--evm-version"), "--allow-local-compiler")
    if args.max_depth is not None:
        command.extend(["--symbolic-max-depth", str(args.max_depth)])
    home = project / ".home"
    home.mkdir()
    env = os.environ.copy()
    svm_home = _host_svm_home(env)
    for name in list(env):
        if name.startswith(("FOUNDRY_", "DAPP_")) or name == "SVM_HOME":
            del env[name]
    env.update(
        {
            "HOME": str(home),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_DATA_HOME": str(home / ".local" / "share"),
        }
    )
    if svm_home.is_dir():
        isolated_svm_home = home / ".local" / "share" / "svm"
        isolated_svm_home.parent.mkdir(parents=True, exist_ok=True)
        isolated_svm_home.symlink_to(svm_home, target_is_directory=True)
    result = subprocess.run(
        command,
        check=False,
        cwd=project,
        env=env,
        text=True,
        capture_output=True,
        timeout=args.timeout,
        preexec_fn=_raise_stack_limit if sys.platform != "win32" else None,
    )
    # Via-IR can run out of stack slots when the harness has a single property
    # and Solc inlines every helper into it; without the optimizer it does not.
    if not result.stdout.strip() and "too deep in the stack" in result.stderr:
        result = subprocess.run(
            command,
            check=False,
            cwd=project,
            env={**env, "FOUNDRY_OPTIMIZER": "false"},
            text=True,
            capture_output=True,
            timeout=args.timeout,
            preexec_fn=_raise_stack_limit if sys.platform != "win32" else None,
        )
    if not result.stdout.strip():
        detail = result.stderr.strip() or "no output"
        raise ValueError(f"Forge did not emit JSON: {detail}")
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError as err:
        raise ValueError(f"Forge emitted invalid JSON: {err}") from err
    if not isinstance(report, dict):
        raise ValueError("Forge JSON output must be an object")
    return report


def _raise_stack_limit() -> None:
    """Raises the stack limit of Forge and the Solc it runs to the hard limit.

    Older Solc releases, such as 0.8.26, recurse deeply on the harness's long
    hex literals and crash with SIGSEGV under the default 8 MiB stack.
    """
    _, hard = resource.getrlimit(resource.RLIMIT_STACK)
    resource.setrlimit(resource.RLIMIT_STACK, (hard, hard))


def _host_svm_home(env: dict[str, str]) -> pathlib.Path:
    if path := env.get("SVM_HOME"):
        return pathlib.Path(path)
    if path := env.get("XDG_DATA_HOME"):
        return pathlib.Path(path) / "svm"
    return pathlib.Path(env["HOME"]) / ".local" / "share" / "svm"


def _validate_bounded_agreement(
    symbolic: dict[str, Any],
    expected: dict[str, Any],
) -> None:
    actual = symbolic.get("bounds")
    if not isinstance(actual, dict):
        raise ValueError("Forge did not report its effective symbolic bounds")
    fields = {
        "timeout_seconds": expected["solver_timeout_seconds"],
        "max_paths": expected["max_paths"],
        "max_solver_queries": expected["max_solver_queries"],
        "max_calldata_bytes": expected["max_calldata_bytes"],
        "max_dynamic_length": expected["max_dynamic_length"],
        "exploration_order": expected["exploration_order"],
        "storage_layout": expected["storage_layout"],
        "default_array_lengths": expected["dynamic_lengths"],
        "default_bytes_lengths": expected["dynamic_lengths"],
        "dynamic_lengths": {
            name: list(lengths) for name, lengths in expected["input_lengths"].items()
        },
    }
    if expected["max_depth"] is not None:
        fields["max_depth"] = expected["max_depth"]
    disagreements = [
        f"{name}={actual.get(name)!r}"
        for name, value in fields.items()
        if actual.get(name) != value
    ]
    assumptions = symbolic.get("assumptions")
    kinds = set()
    if isinstance(assumptions, list):
        kinds = {
            item.get("kind")
            for item in assumptions
            if isinstance(item, dict) and isinstance(item.get("kind"), str)
        }
    if kinds != {"bounded_exploration", "hash_model"}:
        disagreements.append(f"assumptions={sorted(kinds)!r}")
    if disagreements:
        raise ValueError(
            "Forge effective symbolic configuration disagrees with the request: "
            + ", ".join(disagreements)
        )


def _classify(
    report: dict[str, Any],
    checks: list[dict[str, Any]],
    expected_bounds: dict[str, Any] | None = None,
    *,
    constructor: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    by_name = {}
    for suite in report.values():
        if not isinstance(suite, dict):
            continue
        results = suite.get("test_results")
        if not isinstance(results, dict):
            continue
        for test_name, result in results.items():
            symbolic = result.get("symbolic") if isinstance(result, dict) else None
            if isinstance(symbolic, dict):
                by_name[test_name.split("(", 1)[0]] = (result, symbolic)
    unexpected = sorted(set(by_name) - {check["name"] for check in checks})
    if unexpected:
        raise ValueError(f"Forge ran unexpected symbolic tests: {unexpected}")

    outcomes = []
    for check in checks:
        if check.get("unsupported"):
            outcome = {"status": "incomplete", "reason": check["unsupported"]}
        elif check["name"] not in by_name:
            outcome = {
                "status": "incomplete",
                "reason": "Forge did not report this check",
            }
        else:
            result, symbolic = by_name[check["name"]]
            # One check that cannot be classified must not hide the others.
            try:
                outcome = _classify_one(
                    result, symbolic, check, expected_bounds, constructor
                )
            except ValueError as err:
                outcome = {"status": "incomplete", "reason": str(err)}
        outcomes.append({"label": check["label"], **outcome})
    statuses = {outcome["status"] for outcome in outcomes}
    if "mismatch" in statuses:
        status = "mismatch"
    elif "incomplete" in statuses:
        status = "incomplete"
    else:
        status = "bounded_agreement"
    if len(outcomes) == 1:
        single = dict(outcomes[0])
        single.pop("label")
        return single
    summary = {"status": status, "checks": outcomes}
    if status == "mismatch":
        first = next(item for item in outcomes if item["status"] == "mismatch")
        summary["differs_in"] = first["differs_in"]
        summary["counterexample"] = first["counterexample"]
    elif status == "incomplete":
        first = next(item for item in outcomes if item["status"] == "incomplete")
        summary["reason"] = f"{first['label']}: {first['reason']}"
    return summary


def _classify_one(
    result: dict[str, Any],
    symbolic: dict[str, Any],
    check: dict[str, Any],
    expected_bounds: dict[str, Any] | None,
    constructor: list[dict[str, Any]] | None,
) -> dict[str, Any]:
    status = symbolic.get("status")
    replay = symbolic.get("replay")
    if status == "pass":
        if result.get("status") != "Success":
            raise ValueError("Forge emitted an inconsistent symbolic pass")
        if expected_bounds is not None:
            _validate_bounded_agreement(
                symbolic, {**expected_bounds, "input_lengths": check["input_lengths"]}
            )
        return {"status": "bounded_agreement"}

    if status == "incomplete":
        incomplete = symbolic.get("incomplete")
        reason = (
            incomplete.get("reason")
            if isinstance(incomplete, dict)
            else "symbolic execution was incomplete"
        )
        return {"status": "incomplete", "reason": reason}

    if status != "fail_counterexample":
        raise ValueError(f"unsupported Forge symbolic status: {status}")
    if not isinstance(replay, dict) or replay.get("status") != "confirmed":
        raise ValueError("Forge did not concretely replay the counterexample")

    # The harness names the comparison that failed. An `unsupported:` failure
    # marks behavior the comparison cannot model, and any other failure, such
    # as a replay stopped by `vm.assume`, did not reproduce a difference.
    differs_in = result.get("reason") or ""
    if differs_in.startswith(UNSUPPORTED_PREFIX):
        return {
            "status": "incomplete",
            "reason": differs_in.removeprefix(UNSUPPORTED_PREFIX),
        }
    if differs_in not in COMPARISONS:
        return {
            "status": "incomplete",
            "reason": f"counterexample replay failed outside the comparison: {differs_in}",
        }
    if differs_in == "logs" and check.get("log_limit"):
        return {"status": "incomplete", "reason": check["log_limit"]}
    counterexample = symbolic.get("counterexample")
    artifact = symbolic.get("artifact")
    if not isinstance(counterexample, dict) or not isinstance(artifact, dict):
        raise ValueError("Forge did not emit a counterexample artifact")
    artifact_path = artifact.get("path")
    if not isinstance(artifact_path, str) or not artifact_path:
        raise ValueError("Forge emitted an invalid artifact path")
    return {
        "status": "mismatch",
        "differs_in": differs_in,
        "counterexample": {
            **_decode_counterexample(
                counterexample.get("calldata"), check, constructor
            ),
            "forge_artifact": artifact_path,
        },
    }


def _decode_counterexample(
    wrapper_calldata: Any,
    check: dict[str, Any],
    constructor: list[dict[str, Any]] | None,
) -> dict[str, Any]:
    """Splits the test's calldata back into the constructor and each call."""
    if not isinstance(wrapper_calldata, str) or not re.fullmatch(
        r"0x(?:[0-9a-fA-F]{2}){4,}",
        wrapper_calldata,
    ):
        raise ValueError("Forge emitted invalid counterexample calldata")
    inputs = _check_inputs(check, constructor)
    groups = _split_arguments(
        bytes.fromhex(wrapper_calldata[10:]), [item for _, item in inputs]
    )
    values = dict(zip((name for name, _ in inputs), groups, strict=True))
    decoded: dict[str, Any] = {}
    if constructor:
        decoded["constructor_args"] = (
            "0x"
            + _encode_group(
                [values[f"c{position}"] for position in range(len(constructor))]
            ).hex()
        )
    if "c_value" in values:
        decoded["constructor_value"] = str(int.from_bytes(values["c_value"].head))
    starting = {
        str(slot): "0x" + values[f"st{slot}"].head.hex()
        for slot, *_ in check.get("starting_storage", ())
    }
    if starting:
        decoded["starting_storage"] = starting
    calls = []
    for index, step in enumerate(check["steps"]):
        call: dict[str, Any] = {}
        if step["kind"] == "fixed":
            call["calldata"] = step["calldata"]
        elif step["kind"] in {"raw", "raw_args"}:
            raw = values[f"s{index}_raw"].tail or b""
            length = int.from_bytes(raw[:32])
            payload = raw[32 : 32 + length].hex()
            if step["kind"] == "raw_args":
                call["function"] = step["function"]["signature"]
                payload = step["function"]["selector"].removeprefix("0x") + payload
            call["calldata"] = "0x" + payload
        else:
            function = step["function"]
            arguments = [
                values[f"s{index}_a{position}"]
                for position in range(len(function["inputs"]))
            ]
            call["function"] = function["signature"]
            call["calldata"] = function["selector"] + _encode_group(arguments).hex()
        if step.get("caller"):
            call["caller"] = "0x" + values[f"s{index}_caller"].head[12:].hex()
        if f"s{index}_value" in values:
            call["value"] = str(int.from_bytes(values[f"s{index}_value"].head))
        calls.append(call)
    decoded["calls"] = calls
    # The last call is the one a single-function run reports at the top level.
    decoded.update(
        {key: value for key, value in calls[-1].items() if key != "function"}
    )
    if "observer" in values:
        decoded["observer"] = "0x" + values["observer"].head[12:].hex()
    return decoded


class _Argument:
    """One ABI argument: its static head, or the tail a dynamic head points to."""

    def __init__(self, head: bytes, tail: bytes | None) -> None:
        self.head = head
        self.tail = tail


def _split_arguments(data: bytes, items: list[dict[str, Any]]) -> list[_Argument]:
    heads = []
    offset = 0
    for item in items:
        size, dynamic = _abi_head(item)
        word = data[offset : offset + size]
        if len(word) != size:
            raise ValueError("Forge emitted truncated counterexample calldata")
        heads.append((word, dynamic))
        offset += size
    pointers = sorted(int.from_bytes(word) for word, dynamic in heads if dynamic) + [
        len(data)
    ]
    arguments = []
    for word, dynamic in heads:
        if not dynamic:
            arguments.append(_Argument(word, None))
            continue
        start = int.from_bytes(word)
        end = next(pointer for pointer in pointers if pointer > start)
        arguments.append(_Argument(word, data[start:end]))
    return arguments


def _encode_group(arguments: list[_Argument]) -> bytes:
    head_size = sum(len(argument.head) for argument in arguments)
    head = []
    tails = []
    tail_size = 0
    for argument in arguments:
        if argument.tail is None:
            head.append(argument.head)
        else:
            head.append((head_size + tail_size).to_bytes(32))
            tails.append(argument.tail)
            tail_size += len(argument.tail)
    return b"".join(head) + b"".join(tails)


def _abi_head(item: dict[str, Any]) -> tuple[int, bool]:
    """Returns the ABI head size of one parameter and whether it is dynamic."""
    abi_type = item["type"]
    if abi_type.endswith("]"):
        start = abi_type.rfind("[")
        length = abi_type[start + 1 : -1]
        size, dynamic = _abi_head({**item, "type": abi_type[:start]})
        if not length or dynamic:
            return 32, True
        return size * int(length), False
    if abi_type == "tuple":
        sizes = [_abi_head(component) for component in item["components"]]
        if any(dynamic for _, dynamic in sizes):
            return 32, True
        return sum(size for size, _ in sizes), False
    return 32, abi_type in {"bytes", "string"}


_FOUNDRY_TOML = """\
[profile.default]
src = "src"
test = "test"
out = "out"
cache_path = "cache"
libs = []
optimizer = true
optimizer_runs = 200
via_ir = true
evm_version = "{evm_version}"
code_size_limit = 1000000

[symbolic]
exploration_order = "{exploration_order}"
storage_layout = "{storage_layout}"
symbolic_call_targets = true
max_dynamic_length = {max_dynamic_length}
default_array_lengths = [{dynamic_lengths}]
default_bytes_lengths = [{dynamic_lengths}]
"""


_TEST_HEADER = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    struct Log {{
        bytes32[] topics;
        bytes data;
        address emitter;
    }}

    function accesses(address target)
        external
        returns (bytes32[] memory reads, bytes32[] memory writes);
    function assertTrue(bool condition, string calldata error) external pure;
    function assume(bool condition) external pure;
    function computeCreateAddress(address deployer, uint256 nonce)
        external
        pure
        returns (address);
    function deal(address account, uint256 newBalance) external;
    function getNonce(address account) external view returns (uint64 nonce);
    function getRecordedLogs() external returns (Log[] memory logs);
    function load(address target, bytes32 slot) external view returns (bytes32 data);
    function prank(address msgSender) external;
    function record() external;
    function recordLogs() external;
    function revertToState(uint256 snapshotId) external returns (bool success);
    function revertToStateAndDelete(uint256 snapshotId) external returns (bool success);
    function snapshotState() external returns (uint256 snapshotId);
    function store(address target, bytes32 slot, bytes32 value) external;
}}

contract SymbolicDifferentialTest {{
{definitions}
    struct Slot {{
        bytes32 slot;
        bytes32 word;
    }}

    struct Call {{
        bytes data;
        address sender;
        bool prank;
        uint256 value;
    }}

    // The target and every contract it created with CREATE.
    struct Account {{
        address account;
        bytes32[] writes;
        bytes32[] values;
        uint256 balance;
        uint64 nonce;
        bool hasCode;
    }}

    struct Outcome {{
        address target;
        bytes initRet;
        bool destroyed;
        bool[] oks;
        bytes[] rets;
        Vm.Log[] logs;
        Account[] accounts;
        uint256[] balances;
    }}

    Vm private constant vm =
        Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    // Account k's mirror holds Solc's final storage after its state is rolled back.
    uint160 private constant MIRROR_BASE = uint160({mirror_base});
{codes}
"""

_TEST_FOOTER = """\

    // Both builds start from the same snapshot, so they run at the same
    // address with the same nonce, balance, and empty storage.
    function _compare(
        bytes memory initA,
        bytes memory initB,
        uint256 initValue,
        Slot[] memory start,
        Call[] memory calls,
        address[] memory watched
    ) private {
        uint256 snapshot = vm.snapshotState();
        Outcome memory a = _run(initA, initValue, start, calls, watched);
        assert(vm.revertToStateAndDelete(snapshot));
        for (uint256 k; k < a.accounts.length; ++k) {
            address mirror = address(MIRROR_BASE + uint160(k));
            for (uint256 i; i < a.accounts[k].writes.length; ++i) {
                vm.store(mirror, a.accounts[k].writes[i], a.accounts[k].values[i]);
            }
        }
        Outcome memory b = _run(initB, initValue, start, calls, watched);
        _assertSameEffects(a, b);
        for (uint256 k; k < a.accounts.length; ++k) {
            Account memory left = a.accounts[k];
            Account memory right = b.accounts[k];
            address mirror = address(MIRROR_BASE + uint160(k));
            // A slot written by neither build still holds its initial zero.
            for (uint256 i; i < left.writes.length; ++i) {
                _same(vm.load(mirror, left.writes[i]) == vm.load(right.account, left.writes[i]), "storage");
            }
            for (uint256 i; i < right.writes.length; ++i) {
                _same(vm.load(mirror, right.writes[i]) == vm.load(right.account, right.writes[i]), "storage");
            }
        }
    }

    // With symbolic starting storage, a slot only one build wrote holds the
    // other build's starting value, so Solc runs again to read its final
    // value at every slot either build wrote.
    function _compareFromStartingStorage(
        bytes memory initA,
        bytes memory initB,
        uint256 initValue,
        Slot[] memory start,
        Call[] memory calls,
        address[] memory watched
    ) private {
        uint256 snapshot = vm.snapshotState();
        Outcome memory a = _run(initA, initValue, start, calls, watched);
        assert(vm.revertToState(snapshot));
        Outcome memory b = _run(initB, initValue, start, calls, watched);
        _assertSameEffects(a, b);
        bytes32[][] memory slots = new bytes32[][](b.accounts.length);
        bytes32[][] memory values = new bytes32[][](b.accounts.length);
        for (uint256 k; k < b.accounts.length; ++k) {
            bytes32[] memory left = a.accounts[k].writes;
            bytes32[] memory right = b.accounts[k].writes;
            slots[k] = new bytes32[](left.length + right.length);
            values[k] = new bytes32[](slots[k].length);
            for (uint256 i; i < slots[k].length; ++i) {
                slots[k][i] = i < left.length ? left[i] : right[i - left.length];
                values[k][i] = vm.load(b.accounts[k].account, slots[k][i]);
            }
        }
        assert(vm.revertToStateAndDelete(snapshot));
        _run(initA, initValue, start, calls, watched);
        for (uint256 k; k < b.accounts.length; ++k) {
            for (uint256 i; i < slots[k].length; ++i) {
                _same(vm.load(b.accounts[k].account, slots[k][i]) == values[k][i], "storage");
            }
        }
    }

    function _assertSameEffects(Outcome memory a, Outcome memory b) private pure {
        // Every call runs in the test's transaction, where SELFDESTRUCT deletes
        // the target as if it had been created in the same transaction.
        _same(!a.destroyed && !b.destroyed, "unsupported: SELFDESTRUCT within the comparison transaction");
        _same(a.target == b.target, "constructor success");
        _assertSameBytes(a.initRet, b.initRet, "constructor returndata");
        for (uint256 i; i < a.oks.length; ++i) {
            _same(a.oks[i] == b.oks[i], "call success");
            _assertSameBytes(a.rets[i], b.rets[i], "returndata");
        }
        _same(a.logs.length == b.logs.length, "logs");
        for (uint256 i; i < a.logs.length; ++i) {
            _same(a.logs[i].emitter == b.logs[i].emitter, "logs");
            _same(a.logs[i].topics.length == b.logs[i].topics.length, "logs");
            for (uint256 j; j < a.logs[i].topics.length; ++j) {
                _same(a.logs[i].topics[j] == b.logs[i].topics[j], "logs");
            }
            _assertSameBytes(a.logs[i].data, b.logs[i].data, "logs");
        }
        _same(a.accounts.length == b.accounts.length, "nonce");
        for (uint256 k; k < a.accounts.length; ++k) {
            _same(a.accounts[k].account == b.accounts[k].account, "created accounts");
            _same(a.accounts[k].balance == b.accounts[k].balance, "balance");
            _same(a.accounts[k].nonce == b.accounts[k].nonce, "nonce");
            _same(a.accounts[k].hasCode == b.accounts[k].hasCode, "code presence");
        }
        for (uint256 i; i < a.balances.length; ++i) {
            _same(a.balances[i] == b.balances[i], "balance");
        }
    }

    function _run(
        bytes memory init,
        uint256 initValue,
        Slot[] memory start,
        Call[] memory calls,
        address[] memory watched
    ) private returns (Outcome memory outcome) {
        vm.record();
        vm.recordLogs();
        if (initValue != 0) vm.deal(address(this), initValue);
        address target;
        bytes memory initRet;
        assembly ("memory-safe") {
            target := create(initValue, add(init, 0x20), mload(init))
            if iszero(target) {
                initRet := mload(0x40)
                mstore(initRet, returndatasize())
                returndatacopy(add(initRet, 0x20), 0, returndatasize())
                mstore(0x40, add(add(initRet, 0x20), and(add(returndatasize(), 31), not(31))))
            }
        }
        outcome.target = target;
        outcome.initRet = initRet;
        outcome.oks = new bool[](calls.length);
        outcome.rets = new bytes[](calls.length);
        outcome.logs = _committedLogs(new Vm.Log[](0), target != address(0));
        if (target != address(0)) {
            // The starting storage replaces what the constructor left in
            // fixed slots; `vm.store` is not a recorded write.
            for (uint256 i; i < start.length; ++i) {
                vm.store(target, start[i].slot, start[i].word);
            }
            for (uint256 i; i < calls.length; ++i) {
                Call memory call = calls[i];
                if (call.value != 0) vm.deal(call.sender, call.value);
                if (call.prank) vm.prank(call.sender);
                (bool ok, bytes memory ret) = target.call{value: call.value}(call.data);
                outcome.oks[i] = ok;
                outcome.rets[i] = ret;
                outcome.logs = _committedLogs(outcome.logs, ok);
            }
            outcome.destroyed = target.code.length == 0;
            // A contract's nonce starts at one and counts the contracts it
            // created, which CREATE places at the same addresses in both runs.
            uint64 nonce = vm.getNonce(target);
            outcome.accounts = new Account[](nonce == 0 ? 1 : nonce);
            for (uint256 k; k < outcome.accounts.length; ++k) {
                address account = k == 0 ? target : vm.computeCreateAddress(target, k);
                (, bytes32[] memory writes) = vm.accesses(account);
                bytes32[] memory values = new bytes32[](writes.length);
                for (uint256 i; i < writes.length; ++i) {
                    values[i] = vm.load(account, writes[i]);
                }
                outcome.accounts[k] = Account(
                    account,
                    writes,
                    values,
                    account.balance,
                    vm.getNonce(account),
                    account.code.length != 0
                );
            }
        }
        outcome.balances = new uint256[](watched.length);
        for (uint256 i; i < watched.length; ++i) {
            outcome.balances[i] = watched[i].balance;
        }
    }

    // A reverted top-level call discards its logs on chain, but Foundry still
    // records them, so drop them here. The tool does not run code that can
    // revert an inner frame after a log.
    function _committedLogs(
        Vm.Log[] memory committed,
        bool ok
    ) private returns (Vm.Log[] memory result) {
        Vm.Log[] memory logs = vm.getRecordedLogs();
        if (!ok || logs.length == 0) return committed;
        result = new Vm.Log[](committed.length + logs.length);
        for (uint256 i; i < committed.length; ++i) {
            result[i] = committed[i];
        }
        for (uint256 i; i < logs.length; ++i) {
            result[committed.length + i] = logs[i];
        }
    }

    // Names the comparison that failed, so a mismatch reports what differs.
    function _same(bool equal, string memory what) private pure {
        vm.assertTrue(equal, what);
    }

    function _assertSameBytes(
        bytes memory a,
        bytes memory b,
        string memory what
    ) private pure {
        _same(a.length == b.length, what);
        for (uint256 offset; offset < a.length; offset += 32) {
            _same(_word(a, offset) == _word(b, offset), what);
        }
    }

    function _word(
        bytes memory value,
        uint256 offset
    ) private pure returns (uint256 result) {
        assembly ("memory-safe") {
            result := mload(add(add(value, 0x20), offset))
        }
        uint256 remaining = value.length - offset;
        if (remaining < 32) result >>= (32 - remaining) * 8;
    }

    function _runtimeInit(bytes memory runtime) private pure returns (bytes memory) {
        return abi.encodePacked(
            hex"63", uint32(runtime.length), hex"601260003963",
            uint32(runtime.length), hex"6000f3", runtime
        );
    }
}
"""


if __name__ == "__main__":
    raise SystemExit(main())
