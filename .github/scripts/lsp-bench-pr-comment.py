#!/usr/bin/env python3
"""Render a validated LSP benchmark comparison for a PR comment."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import unicodedata
from pathlib import Path
from typing import Any


COMPARISON_SCHEMA_VERSION = 2
SUMMARY_SCHEMA_VERSION = 5
MAX_INPUT_BYTES = 2 * 1024 * 1024
MAX_COMMENT_BYTES = 60_000
MAX_ROWS = 200
MAX_TEXT_LENGTH = 256
MAX_PATH_LENGTH = 4_096
MAX_COUNT = 1_000_000
FLOAT_REL_TOLERANCE = 1e-9
FLOAT_ABS_TOLERANCE = 1e-9
REVISION_RE = re.compile(r"[0-9a-f]{40}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
IDENTIFIER_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._/:+\-]*")
SOURCE_PATHS = {
    "baseline": "target/lsp-bench/pr/baseline/summary.json",
    "candidate": "target/lsp-bench/pr/current/summary.json",
}
PR_SERVER_ID = "solar"
PR_FIXTURE_ID = "synthetic"

TOP_LEVEL_KEYS = {
    "schema_version",
    "baseline",
    "candidate",
    "threshold_pct",
    "min_samples",
    "compatible",
    "blockers",
    "comparable_metrics",
    "regressions",
    "improvements",
    "stable",
    "inconclusive",
    "rows",
}
ROW_KEYS = {
    "server",
    "fixture",
    "workload",
    "metric",
    "baseline_status",
    "candidate_status",
    "expected_runs",
    "baseline_successful_runs",
    "candidate_successful_runs",
    "baseline_count",
    "candidate_count",
    "baseline_mean",
    "candidate_mean",
    "mean_delta_pct",
    "baseline_p50",
    "candidate_p50",
    "p50_delta_pct",
    "baseline_p95",
    "candidate_p95",
    "p95_delta_pct",
    "verdict",
    "reason",
}
STATS_KEYS = {"count", "mean", "p50", "p95", "p99", "max"}
SOURCE_KEYS = {
    "path",
    "summary_sha256",
    "source_url",
    "revision",
    "executable_sha256",
}
SUMMARY_STATUSES = {"pass", "partial", "unsupported", "unavailable", "failed"}
RUN_STATUSES = {
    "pass",
    "unsupported",
    "incorrect",
    "incompatible",
    "timeout",
    "crash",
    "unavailable",
    "harness-error",
}
VERDICTS = {"regression", "improvement", "stable", "inconclusive"}
PROCESS_ACCOUNTING_BACKENDS = {
    "cgroup-v2-process-tree",
    "rusage-direct-child",
    "unavailable",
}
MEMORY_ACCOUNTING_BACKENDS = {
    "cgroup-v2-total",
    "rusage-max-rss-direct-child",
    "unavailable",
}
RowKey = tuple[str, str, str, str]
GroupKey = tuple[str, str, str]
MARKDOWN_ESCAPES = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "@": "&#64;",
    "\\": "&#92;",
    "`": "&#96;",
    "*": "&#42;",
    "_": "&#95;",
    "[": "&#91;",
    "]": "&#93;",
    "(": "&#40;",
    ")": "&#41;",
    "#": "&#35;",
    "!": "&#33;",
    "|": "&#124;",
    "~": "&#126;",
    ":": "&#58;",
    ".": "&#46;",
    "/": "&#47;",
}

INCONCLUSIVE_REPORT = (
    "# Solar LSP PR benchmark\n\n"
    "**INCONCLUSIVE**\n\n"
    "The benchmark comparison artifact was missing or invalid. "
    "Inspect the workflow run logs and retained artifacts.\n"
)


class ValidationError(ValueError):
    """The comparison artifact does not match the trusted comment schema."""


def reject_constant(value: str) -> None:
    raise ValidationError(f"non-finite JSON number `{value}` is not allowed")


def parse_json_integer(value: str) -> int | float:
    # serde_json represents the JSON integer `-0` as a floating-point value.
    return -0.0 if value == "-0" else int(value)


def parse_json_float(value: str) -> float:
    number = float(value)
    if not math.isfinite(number):
        raise ValidationError(f"non-finite JSON number `{value}` is not allowed")
    return number


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValidationError(f"duplicate JSON key `{key}`")
        result[key] = value
    return result


def require_exact_keys(value: Any, expected: set[str], name: str) -> dict[str, Any]:
    if type(value) is not dict:
        raise ValidationError(f"{name} must be an object")
    if set(value) != expected:
        raise ValidationError(f"{name} has unexpected or missing fields")
    return value


def require_bool(value: Any, name: str) -> bool:
    if type(value) is not bool:
        raise ValidationError(f"{name} must be a boolean")
    return value


def require_count(value: Any, name: str, *, minimum: int = 0) -> int:
    if type(value) is not int or not minimum <= value <= MAX_COUNT:
        raise ValidationError(f"{name} must be an integer from {minimum} to {MAX_COUNT}")
    return value


def optional_count(value: Any, name: str, *, minimum: int = 0) -> int | None:
    if value is None:
        return None
    return require_count(value, name, minimum=minimum)


def require_number(value: Any, name: str, *, minimum: float | None = None) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValidationError(f"{name} must be a number")
    number = float(value)
    if not math.isfinite(number) or abs(number) > 1e15:
        raise ValidationError(f"{name} must be finite and bounded")
    if minimum is not None and number < minimum:
        raise ValidationError(f"{name} must be at least {minimum}")
    return number


def optional_number(
    value: Any, name: str, *, minimum: float | None = None
) -> float | None:
    if value is None:
        return None
    return require_number(value, name, minimum=minimum)


def require_text(value: Any, name: str, *, maximum: int = MAX_TEXT_LENGTH) -> str:
    if type(value) is not str or not value or len(value) > maximum:
        raise ValidationError(
            f"{name} must be a non-empty string of at most {maximum} characters"
        )
    if value != unicodedata.normalize("NFC", value):
        raise ValidationError(f"{name} is not normalized Unicode text")
    disallowed_categories = {"Cc", "Cf", "Cs", "Zl", "Zp"}
    if any(unicodedata.category(character) in disallowed_categories for character in value):
        raise ValidationError(f"{name} contains a control or formatting character")
    return value


def optional_text(value: Any, name: str) -> str | None:
    if value is None:
        return None
    return require_text(value, name)


def require_identifier(value: Any, name: str) -> str:
    identifier = require_text(value, name)
    if IDENTIFIER_RE.fullmatch(identifier) is None:
        raise ValidationError(f"{name} is not a valid identifier")
    return identifier


def require_revision(value: Any, name: str) -> str:
    revision = require_text(value, name)
    if REVISION_RE.fullmatch(revision) is None:
        raise ValidationError(f"{name} must be a lowercase 40-character Git revision")
    return revision


def require_sha256(value: Any, name: str) -> str:
    digest = require_text(value, name)
    if SHA256_RE.fullmatch(digest) is None:
        raise ValidationError(f"{name} must be a lowercase SHA-256 digest")
    return digest


def optional_status(value: Any, name: str) -> str | None:
    if value is None:
        return None
    status = require_text(value, name)
    if status not in SUMMARY_STATUSES:
        raise ValidationError(f"{name} is not a recognized summary status")
    return status


def require_string_list(value: Any, name: str) -> tuple[str, ...]:
    if type(value) is not list:
        raise ValidationError(f"{name} must be an array")
    return tuple(require_text(item, f"{name}[{index}]") for index, item in enumerate(value))


def require_enum_list(value: Any, name: str, allowed: set[str]) -> tuple[str, ...]:
    items = require_string_list(value, name)
    if any(item not in allowed for item in items):
        raise ValidationError(f"{name} contains an unsupported value")
    return items


def require_string_map(value: Any, name: str) -> tuple[tuple[str, str], ...]:
    if type(value) is not dict:
        raise ValidationError(f"{name} must be an object")
    return tuple(
        sorted(
            (
                require_text(key, f"{name} key"),
                require_text(item, f"{name}[{key}]")
            )
            for key, item in value.items()
        )
    )


def normalize_json_contract(value: Any, name: str) -> tuple[Any, ...]:
    """Preserve JSON types when comparing untrusted runtime configuration."""
    if value is None:
        return ("null",)
    if type(value) is bool:
        return ("boolean", value)
    if type(value) is int:
        return ("integer", value)
    if type(value) is float:
        if not math.isfinite(value):
            raise ValidationError(f"{name} contains a non-finite number")
        return ("float", value)
    if type(value) is str:
        try:
            value.encode("utf-8")
        except UnicodeEncodeError as error:
            raise ValidationError(f"{name} contains invalid Unicode text") from error
        return ("string", value)
    if type(value) is list:
        return (
            "array",
            tuple(
                normalize_json_contract(item, f"{name}[{index}]")
                for index, item in enumerate(value)
            ),
        )
    if type(value) is dict:
        normalized = []
        for key, item in value.items():
            if type(key) is not str:
                raise ValidationError(f"{name} contains a non-string object key")
            normalized.append(
                (
                    normalize_json_contract(key, f"{name} object key"),
                    normalize_json_contract(item, f"{name}[{key}]"),
                )
            )
        return ("object", tuple(sorted(normalized)))
    raise ValidationError(f"{name} contains a value that is not valid JSON")


def normalize_required_json_contract(
    value: dict[str, Any], field: str, name: str
) -> tuple[Any, ...]:
    if field not in value:
        raise ValidationError(f"{name} is missing")
    return normalize_json_contract(value[field], name)


def validate_stats(value: Any, name: str) -> dict[str, float | int]:
    stats = require_exact_keys(value, STATS_KEYS, name)
    validated: dict[str, float | int] = {
        "count": require_count(stats["count"], f"{name}.count", minimum=1),
    }
    for field in ("mean", "p50", "p95", "p99", "max"):
        validated[field] = require_number(stats[field], f"{name}.{field}", minimum=0.0)
    if not validated["p50"] <= validated["p95"] <= validated["p99"] <= validated["max"]:
        raise ValidationError(f"{name} percentiles are not ordered")
    if validated["mean"] > validated["max"]:
        raise ValidationError(f"{name}.mean exceeds its maximum")
    return validated


def load_json(path: Path, name: str) -> tuple[Any, str]:
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"{name} is not a regular file")
    with path.open("rb") as file:
        contents = file.read(MAX_INPUT_BYTES + 1)
    if len(contents) > MAX_INPUT_BYTES:
        raise ValidationError(f"{name} is too large")
    text = contents.decode("utf-8")
    value = json.loads(
        text,
        object_pairs_hook=unique_object,
        parse_constant=reject_constant,
        parse_float=parse_json_float,
        parse_int=parse_json_integer,
    )
    return value, hashlib.sha256(contents).hexdigest()


def validate_summary(
    value: Any,
    role: str,
    expected_revision: str,
    expected_source_url: str,
) -> dict[str, str]:
    if type(value) is not dict:
        raise ValidationError(f"{role} summary must be an object")
    if require_count(value.get("schema_version"), f"{role} summary schema_version", minimum=1) != (
        SUMMARY_SCHEMA_VERSION
    ):
        raise ValidationError(f"{role} summary schema_version is not supported")
    if require_text(value.get("profile"), f"{role} summary profile") != "pr":
        raise ValidationError(f"{role} summary profile is not `pr`")
    require_sha256(
        value.get("servers_lock_sha256"), f"{role} summary server lock"
    )
    servers = value.get("servers")
    if type(servers) is not list or len(servers) != 1:
        raise ValidationError(f"{role} summary must contain only the Solar server")
    if type(servers[0]) is not dict or servers[0].get("id") != PR_SERVER_ID:
        raise ValidationError(f"{role} summary server is not Solar")
    solar = servers[0]

    source = require_exact_keys(
        solar.get("source"), {"url", "revision"}, f"{role} summary Solar source"
    )
    source_url = require_text(
        source["url"], f"{role} summary Solar source URL", maximum=MAX_PATH_LENGTH
    )
    revision = require_revision(source["revision"], f"{role} summary Solar revision")
    executable_sha256 = require_sha256(
        solar.get("executable_sha256"), f"{role} summary Solar executable"
    )
    artifact_expected_sha256 = require_sha256(
        solar.get("artifact_expected_sha256"),
        f"{role} summary Solar expected artifact",
    )
    artifact_sha256 = require_sha256(
        solar.get("artifact_sha256"), f"{role} summary Solar artifact"
    )
    if require_text(solar.get("status"), f"{role} summary Solar status") != "available":
        raise ValidationError(f"{role} summary Solar server is not available")
    if not (
        executable_sha256 == artifact_expected_sha256 == artifact_sha256
    ):
        raise ValidationError(
            f"{role} summary Solar executable and artifact digests are inconsistent"
        )
    if source_url != expected_source_url:
        raise ValidationError(f"{role} summary Solar source URL does not match the workflow")
    if revision != expected_revision:
        raise ValidationError(f"{role} summary Solar revision does not match the workflow")
    return {
        "source_url": source_url,
        "revision": revision,
        "executable_sha256": executable_sha256,
    }


def validate_summary_contract(
    value: Any, role: str
) -> tuple[dict[GroupKey, dict[str, Any]], dict[str, int]]:
    """Return normalized groups and workload repetitions from a PR summary."""
    workloads = value.get("workloads") if type(value) is dict else None
    if type(workloads) is not list:
        raise ValidationError(f"{role} summary workloads must be an array")

    workload_fixtures: dict[str, str] = {}
    workload_repetitions: dict[str, int] = {}
    for index, workload in enumerate(workloads):
        name = f"{role} workloads[{index}]"
        if type(workload) is not dict:
            raise ValidationError(f"{name} must be an object")
        workload_id = require_identifier(workload.get("id"), f"{name}.id")
        fixture = require_identifier(workload.get("fixture"), f"{name}.fixture")
        if fixture != PR_FIXTURE_ID:
            raise ValidationError(f"{name}.fixture is not the PR fixture")
        repetitions = require_count(workload.get("repetitions"), f"{name}.repetitions", minimum=1)
        if workload_id in workload_fixtures:
            raise ValidationError(f"{role} summary contains duplicate workload `{workload_id}`")
        workload_fixtures[workload_id] = fixture
        workload_repetitions[workload_id] = repetitions
    if not workload_fixtures:
        raise ValidationError(f"{role} summary contains no selected workloads")

    summaries = value.get("summaries") if type(value) is dict else None
    if type(summaries) is not list:
        raise ValidationError(f"{role} summary summaries must be an array")

    groups: dict[GroupKey, dict[str, Any]] = {}
    for index, group in enumerate(summaries):
        name = f"{role} summaries[{index}]"
        if type(group) is not dict:
            raise ValidationError(f"{name} must be an object")
        server = require_identifier(group.get("server"), f"{name}.server")
        fixture = require_identifier(group.get("fixture"), f"{name}.fixture")
        workload = require_identifier(group.get("workload"), f"{name}.workload")
        if server != PR_SERVER_ID:
            raise ValidationError(f"{name}.server is not Solar")
        if fixture != PR_FIXTURE_ID:
            raise ValidationError(f"{name}.fixture is not the PR fixture")
        if workload not in workload_fixtures:
            raise ValidationError(f"{name}.workload is not declared")
        if fixture != workload_fixtures[workload]:
            raise ValidationError(f"{name}.fixture does not match its workload")
        status = optional_status(group.get("status"), f"{name}.status")
        if status is None:
            raise ValidationError(f"{name}.status is missing")
        successful_runs = require_count(
            group.get("successful_runs"), f"{name}.successful_runs"
        )
        raw_status_counts = group.get("status_counts")
        if type(raw_status_counts) is not dict or not raw_status_counts:
            raise ValidationError(f"{name}.status_counts must be a non-empty object")
        status_counts = {
            require_text(raw_status, f"{name}.status_counts key"): require_count(
                count, f"{name}.status_counts[{raw_status}]", minimum=1
            )
            for raw_status, count in raw_status_counts.items()
        }
        if any(raw_status not in RUN_STATUSES for raw_status in status_counts):
            raise ValidationError(f"{name}.status_counts contains an unsupported status")
        if sum(status_counts.values()) != workload_repetitions[workload]:
            raise ValidationError(f"{name}.status_counts does not match configured repetitions")
        if successful_runs != status_counts.get("pass", 0):
            raise ValidationError(f"{name}.successful_runs does not match passing samples")
        if status != aggregate_summary_status(status_counts, successful_runs):
            raise ValidationError(f"{name}.status does not match status_counts")
        raw_metrics = group.get("metrics")
        if type(raw_metrics) is not dict:
            raise ValidationError(f"{name}.metrics must be an object")
        group_key = (server, fixture, workload)
        if group_key in groups:
            raise ValidationError(f"{name} duplicates an earlier summary group")
        metrics = {
            require_identifier(metric, f"{name}.metrics key"): validate_stats(
                stats, f"{name}.metrics[{metric}]"
            )
            for metric, stats in raw_metrics.items()
        }
        groups[group_key] = {
            "status": status,
            "successful_runs": successful_runs,
            "metrics": metrics,
        }

    expected_groups = {
        (PR_SERVER_ID, fixture, workload)
        for workload, fixture in workload_fixtures.items()
    }
    if set(groups) != expected_groups:
        raise ValidationError(
            f"{role} summary groups do not match the selected workload contract"
        )

    return groups, workload_repetitions


def aggregate_summary_status(
    status_counts: dict[str, int], successful_runs: int
) -> str:
    def has(status: str) -> bool:
        return status_counts.get(status, 0) != 0

    if any(
        has(status)
        for status in ("incorrect", "incompatible", "timeout", "crash", "harness-error")
    ):
        return "failed"
    if has("unavailable"):
        return "unavailable"
    if has("unsupported") and successful_runs == 0:
        return "unsupported"
    if has("unsupported"):
        return "partial"
    return "pass"


def comparison_row_keys(
    baseline_groups: dict[GroupKey, dict[str, Any]],
    candidate_groups: dict[GroupKey, dict[str, Any]],
) -> set[RowKey]:
    """Mirror the Rust comparator's per-group metric union and empty fallback."""
    row_keys: set[RowKey] = set()
    group_keys = baseline_groups.keys() | candidate_groups.keys()
    for server, fixture, workload in group_keys:
        baseline_metrics = baseline_groups.get((server, fixture, workload), {}).get("metrics", {})
        candidate_metrics = candidate_groups.get((server, fixture, workload), {}).get(
            "metrics", {}
        )
        metrics = baseline_metrics.keys() | candidate_metrics.keys()
        if not metrics:
            metrics = {"status"}
        row_keys.update((server, fixture, workload, metric) for metric in metrics)
    return row_keys


def optional_sha256(value: Any, name: str) -> str | None:
    if value is None:
        return None
    return require_sha256(value, name)


def optional_bounded_text(value: Any, name: str, maximum: int) -> str | None:
    if value is None:
        return None
    return require_text(value, name, maximum=maximum)


def normalize_transport(value: Any, name: str) -> tuple[Any, ...]:
    if type(value) is not dict:
        raise ValidationError(f"{name} must be an object")
    kind = require_text(value.get("kind"), f"{name}.kind")
    if kind == "stdio":
        require_exact_keys(value, {"kind"}, name)
        return (kind,)
    if kind == "tcp":
        require_exact_keys(value, {"kind", "address"}, name)
        return (kind, require_text(value["address"], f"{name}.address"))
    raise ValidationError(f"{name}.kind is unsupported")


def normalize_compiler(
    value: Any,
    *,
    name: str,
    native_actual_sha256: Any,
    soljson_actual_sha256: Any,
) -> tuple[Any, ...] | None:
    if value is None:
        if native_actual_sha256 is not None or soljson_actual_sha256 is not None:
            raise ValidationError(f"{name} has artifact digests without compiler metadata")
        return None
    if type(value) is not dict:
        raise ValidationError(f"{name} must be an object")
    native = optional_bounded_text(value.get("native"), f"{name}.native", MAX_PATH_LENGTH)
    soljson = optional_bounded_text(value.get("soljson"), f"{name}.soljson", MAX_PATH_LENGTH)
    native_pin = optional_sha256(value.get("native_sha256"), f"{name}.native_sha256")
    soljson_pin = optional_sha256(value.get("soljson_sha256"), f"{name}.soljson_sha256")
    native_actual = optional_sha256(native_actual_sha256, f"{name}.native_actual_sha256")
    soljson_actual = optional_sha256(soljson_actual_sha256, f"{name}.soljson_actual_sha256")
    if native is not None and native_actual is None:
        raise ValidationError(f"{name} native artifact digest is unavailable")
    if soljson is not None and soljson_actual is None:
        raise ValidationError(f"{name} soljson artifact digest is unavailable")
    if native_pin is not None and native_actual is not None and native_pin != native_actual:
        raise ValidationError(f"{name} native artifact digest does not match its pin")
    if soljson_pin is not None and soljson_actual is not None and soljson_pin != soljson_actual:
        raise ValidationError(f"{name} soljson artifact digest does not match its pin")
    return (
        require_text(value.get("version"), f"{name}.version"),
        optional_bounded_text(value.get("native_url"), f"{name}.native_url", MAX_PATH_LENGTH),
        native_pin,
        native_actual,
        optional_bounded_text(value.get("soljson_url"), f"{name}.soljson_url", MAX_PATH_LENGTH),
        soljson_pin,
        soljson_actual,
        optional_bounded_text(value.get("archive_url"), f"{name}.archive_url", MAX_PATH_LENGTH),
        optional_sha256(value.get("archive_sha256"), f"{name}.archive_sha256"),
    )


def summary_compatibility_contract(value: Any, role: str) -> dict[str, Any]:
    environment = value.get("environment")
    if type(environment) is not dict:
        raise ValidationError(f"{role} summary environment must be an object")
    server = value["servers"][0]
    source = server["source"]
    server_contract = (
        server["id"],
        require_string_list(server.get("args"), f"{role} summary Solar args"),
        normalize_transport(server.get("transport"), f"{role} summary Solar transport"),
        require_string_list(
            server.get("version_args"), f"{role} summary Solar version arguments"
        ),
        optional_text(server.get("locked_version"), f"{role} summary Solar locked version"),
        optional_text(server.get("expected_version"), f"{role} summary Solar expected version"),
        require_bool(server.get("enabled"), f"{role} summary Solar enabled"),
        require_string_map(server.get("env"), f"{role} summary Solar environment"),
        normalize_required_json_contract(
            server,
            "initialization_options",
            f"{role} summary Solar initialization options",
        ),
        normalize_required_json_contract(
            server, "configuration", f"{role} summary Solar configuration"
        ),
        require_bool(server.get("required"), f"{role} summary Solar required"),
    )

    workloads = []
    selected_fixtures = set()
    for index, workload in enumerate(value["workloads"]):
        name = f"{role} workloads[{index}]"
        workload_id = require_identifier(workload.get("id"), f"{name}.id")
        fixture = require_identifier(workload.get("fixture"), f"{name}.fixture")
        selected_fixtures.add(fixture)
        workloads.append(
            (
                workload_id,
                fixture,
                require_string_list(workload.get("methods"), f"{name}.methods"),
                require_count(workload.get("step_count"), f"{name}.step_count"),
                require_count(workload.get("repetitions"), f"{name}.repetitions", minimum=1),
            )
        )
    workloads.sort()

    raw_fixtures = value.get("fixtures")
    if type(raw_fixtures) is not list:
        raise ValidationError(f"{role} summary fixtures must be an array")
    fixture_records = []
    for index, fixture in enumerate(raw_fixtures):
        name = f"{role} fixtures[{index}]"
        if type(fixture) is not dict:
            raise ValidationError(f"{name} must be an object")
        fixture_records.append(
            (require_identifier(fixture.get("id"), f"{name}.id"), fixture)
        )
    fixtures = []
    for fixture_id in sorted(selected_fixtures):
        matches = [fixture for item_id, fixture in fixture_records if item_id == fixture_id]
        if len(matches) != 1:
            raise ValidationError(
                f"{role} fixture `{fixture_id}` metadata is missing or duplicated"
            )
        fixture = matches[0]
        name = f"{role} fixture `{fixture_id}`"
        fixtures.append(
            (
                fixture_id,
                require_sha256(fixture.get("content_sha256"), f"{name} content"),
                require_count(fixture.get("source_file_count"), f"{name} source_file_count"),
                require_count(fixture.get("source_line_count"), f"{name} source_line_count"),
                require_count(fixture.get("source_byte_count"), f"{name} source_byte_count"),
                normalize_compiler(
                    fixture.get("solc"),
                    name=f"{name} solc",
                    native_actual_sha256=fixture.get("solc_native_sha256"),
                    soljson_actual_sha256=fixture.get("solc_soljson_sha256"),
                ),
                normalize_compiler(
                    fixture.get("foundry"),
                    name=f"{name} foundry",
                    native_actual_sha256=fixture.get("foundry_native_sha256"),
                    soljson_actual_sha256=None,
                ),
                require_string_map(fixture.get("dependencies"), f"{name} dependencies"),
            )
        )

    repeat_override = value.get("repeat_override")
    if repeat_override is not None:
        repeat_override = require_count(
            repeat_override, f"{role} summary repeat_override", minimum=1
        )
    return {
        "config schema": require_count(
            value.get("config_schema_version"), f"{role} summary config schema", minimum=1
        ),
        "benchmark profile": require_text(value.get("profile"), f"{role} summary profile"),
        "benchmark config": require_sha256(
            value.get("config_sha256"), f"{role} summary benchmark config"
        ),
        "fixture lock": require_sha256(
            value.get("fixtures_lock_sha256"), f"{role} summary fixture lock"
        ),
        "timeout": require_count(value.get("timeout_ms"), f"{role} summary timeout", minimum=1),
        "repeat override": repeat_override,
        "harness version": require_text(
            value.get("harness_version"), f"{role} summary harness version"
        ),
        "harness contract": require_sha256(
            value.get("harness_contract_sha256"), f"{role} summary harness contract"
        ),
        "Rust compiler": require_text(
            value.get("rustc_version"), f"{role} summary Rust compiler", maximum=MAX_PATH_LENGTH
        ),
        "operating system": require_text(
            environment.get("os"), f"{role} summary operating system"
        ),
        "architecture": require_text(
            environment.get("architecture"), f"{role} summary architecture"
        ),
        "logical CPU count": require_count(
            environment.get("logical_cpus"), f"{role} summary logical CPU count", minimum=1
        ),
        "process accounting backends": require_enum_list(
            environment.get("accounting_backends"),
            f"{role} summary process accounting backends",
            PROCESS_ACCOUNTING_BACKENDS,
        ),
        "memory accounting backends": require_enum_list(
            environment.get("memory_accounting_backends"),
            f"{role} summary memory accounting backends",
            MEMORY_ACCOUNTING_BACKENDS,
        ),
        "network isolation": require_bool(
            environment.get("network_isolated"), f"{role} summary network isolation"
        ),
        "server contract": server_contract,
        "workload contract": tuple(workloads),
        "fixture contents": tuple(fixtures),
    }


def compatibility_blockers(
    baseline: dict[str, Any], candidate: dict[str, Any]
) -> list[str]:
    return [
        f"{name} differs between baseline and candidate"
        for name, baseline_value in baseline.items()
        if baseline_value != candidate[name]
    ]


def validate_source(
    value: Any,
    role: str,
    summary_digest: str,
    summary_provenance: dict[str, str],
) -> dict[str, Any]:
    source = require_exact_keys(value, SOURCE_KEYS, role)
    path = require_text(source["path"], f"{role}.path", maximum=MAX_PATH_LENGTH)
    if path != SOURCE_PATHS[role]:
        raise ValidationError(f"{role}.path does not match the expected artifact path")
    digest = require_sha256(source["summary_sha256"], f"{role}.summary_sha256")
    source_url = require_text(
        source["source_url"], f"{role}.source_url", maximum=MAX_PATH_LENGTH
    )
    revision = require_revision(source["revision"], f"{role}.revision")
    executable_sha256 = require_sha256(
        source["executable_sha256"], f"{role}.executable_sha256"
    )
    if digest != summary_digest:
        raise ValidationError(f"{role} summary digest does not match the downloaded summary")
    for field, expected in summary_provenance.items():
        if source[field] != expected:
            raise ValidationError(f"{role}.{field} does not match the downloaded summary")
    return {
        "path": path,
        "summary_sha256": digest,
        "source_url": source_url,
        "revision": revision,
        "executable_sha256": executable_sha256,
    }


def percentage_delta(baseline: float | None, candidate: float | None) -> float | None:
    if baseline is None or candidate is None or baseline == 0.0:
        return None
    delta = (candidate - baseline) / abs(baseline) * 100.0
    if not math.isfinite(delta) or abs(delta) > 1e15:
        raise ValidationError("computed percentage delta is not finite and bounded")
    return delta


def require_same_number(reported: float | None, expected: float | None, name: str) -> None:
    if reported is None or expected is None:
        if reported is not expected:
            raise ValidationError(f"{name} does not match the recomputed delta")
        return
    if not math.isclose(
        reported,
        expected,
        rel_tol=FLOAT_REL_TOLERANCE,
        abs_tol=FLOAT_ABS_TOLERANCE,
    ):
        raise ValidationError(f"{name} does not match the recomputed delta")


def validate_row(
    value: Any,
    index: int,
    *,
    compatible: bool,
    threshold_pct: float,
    min_samples: int,
    baseline_groups: dict[GroupKey, dict[str, Any]],
    candidate_groups: dict[GroupKey, dict[str, Any]],
    candidate_workload_repetitions: dict[str, int],
) -> dict[str, Any]:
    name = f"rows[{index}]"
    row = require_exact_keys(value, ROW_KEYS, name)
    for field in ("server", "fixture", "workload", "metric"):
        require_identifier(row[field], f"{name}.{field}")

    baseline_status = optional_status(row["baseline_status"], f"{name}.baseline_status")
    candidate_status = optional_status(row["candidate_status"], f"{name}.candidate_status")
    expected_runs = optional_count(row["expected_runs"], f"{name}.expected_runs", minimum=1)
    baseline_successful_runs = optional_count(
        row["baseline_successful_runs"], f"{name}.baseline_successful_runs"
    )
    candidate_successful_runs = optional_count(
        row["candidate_successful_runs"], f"{name}.candidate_successful_runs"
    )
    baseline_group_present = baseline_status is not None
    candidate_group_present = candidate_status is not None
    if baseline_group_present != (baseline_successful_runs is not None):
        raise ValidationError(f"{name} has inconsistent baseline group fields")
    if candidate_group_present != (candidate_successful_runs is not None):
        raise ValidationError(f"{name} has inconsistent candidate group fields")

    baseline_count = optional_count(row["baseline_count"], f"{name}.baseline_count")
    candidate_count = optional_count(row["candidate_count"], f"{name}.candidate_count")
    baseline_mean = optional_number(
        row["baseline_mean"], f"{name}.baseline_mean", minimum=0.0
    )
    candidate_mean = optional_number(
        row["candidate_mean"], f"{name}.candidate_mean", minimum=0.0
    )
    baseline_p50 = optional_number(
        row["baseline_p50"], f"{name}.baseline_p50", minimum=0.0
    )
    candidate_p50 = optional_number(
        row["candidate_p50"], f"{name}.candidate_p50", minimum=0.0
    )
    baseline_p95 = optional_number(
        row["baseline_p95"], f"{name}.baseline_p95", minimum=0.0
    )
    candidate_p95 = optional_number(
        row["candidate_p95"], f"{name}.candidate_p95", minimum=0.0
    )
    baseline_stats = (baseline_count, baseline_mean, baseline_p50, baseline_p95)
    candidate_stats = (candidate_count, candidate_mean, candidate_p50, candidate_p95)
    if any(value is None for value in baseline_stats) != all(
        value is None for value in baseline_stats
    ):
        raise ValidationError(f"{name} has incomplete baseline metric statistics")
    if any(value is None for value in candidate_stats) != all(
        value is None for value in candidate_stats
    ):
        raise ValidationError(f"{name} has incomplete candidate metric statistics")
    baseline_metric_present = baseline_count is not None
    candidate_metric_present = candidate_count is not None
    if not baseline_group_present and baseline_metric_present:
        raise ValidationError(f"{name} has a baseline metric without a baseline group")
    if not candidate_group_present and candidate_metric_present:
        raise ValidationError(f"{name} has a candidate metric without a candidate group")

    trusted = bind_row_to_summaries(
        row,
        index,
        baseline_groups,
        candidate_groups,
        candidate_workload_repetitions,
    )
    baseline_status = trusted["baseline_status"]
    candidate_status = trusted["candidate_status"]
    expected_runs = trusted["expected_runs"]
    baseline_successful_runs = trusted["baseline_successful_runs"]
    candidate_successful_runs = trusted["candidate_successful_runs"]
    baseline_count = trusted["baseline_count"]
    candidate_count = trusted["candidate_count"]
    baseline_mean = trusted["baseline_mean"]
    candidate_mean = trusted["candidate_mean"]
    baseline_p50 = trusted["baseline_p50"]
    candidate_p50 = trusted["candidate_p50"]
    baseline_p95 = trusted["baseline_p95"]
    candidate_p95 = trusted["candidate_p95"]
    baseline_group_present = baseline_status is not None
    candidate_group_present = candidate_status is not None
    baseline_metric_present = baseline_count is not None
    candidate_metric_present = candidate_count is not None

    deltas = {
        "mean_delta_pct": percentage_delta(baseline_mean, candidate_mean),
        "p50_delta_pct": percentage_delta(baseline_p50, candidate_p50),
        "p95_delta_pct": percentage_delta(baseline_p95, candidate_p95),
    }
    for field, expected in deltas.items():
        reported = optional_number(row[field], f"{name}.{field}")
        require_same_number(reported, expected, f"{name}.{field}")

    reason = None
    if not compatible:
        reason = "run metadata is incompatible"
    elif not baseline_group_present:
        reason = "metric group is missing from the baseline"
    elif not candidate_group_present:
        reason = "metric group is missing from the candidate"
    elif baseline_status != "pass":
        reason = "baseline group did not pass"
    elif candidate_status != "pass":
        reason = "candidate group did not pass"
    elif expected_runs is None:
        reason = "workload repetition contract is missing"
    elif baseline_successful_runs != expected_runs:
        reason = "baseline group did not complete every configured repetition"
    elif candidate_successful_runs != expected_runs:
        reason = "candidate group did not complete every configured repetition"
    elif not baseline_metric_present:
        reason = "metric is missing from the baseline"
    elif not candidate_metric_present:
        reason = "metric is missing from the candidate"
    elif baseline_count < min_samples or candidate_count < min_samples:
        reason = f"metric has fewer than {min_samples} samples"
    elif baseline_count != candidate_count:
        reason = "baseline and candidate sample counts differ"
    elif any(delta is None for delta in deltas.values()):
        reason = "baseline metric contains a zero or non-finite value"

    reported_reason = optional_text(row["reason"], f"{name}.reason")
    if reported_reason != reason:
        raise ValidationError(f"{name}.reason does not match the recomputed reason")

    if reason is not None:
        verdict = "inconclusive"
    elif deltas["p50_delta_pct"] >= threshold_pct and deltas["p95_delta_pct"] >= (
        threshold_pct
    ):
        verdict = "regression"
    elif deltas["p50_delta_pct"] <= -threshold_pct and deltas["p95_delta_pct"] <= (
        -threshold_pct
    ):
        verdict = "improvement"
    else:
        verdict = "stable"
    reported_verdict = require_text(row["verdict"], f"{name}.verdict")
    if reported_verdict not in VERDICTS or reported_verdict != verdict:
        raise ValidationError(f"{name}.verdict does not match the recomputed verdict")

    validated = dict(row)
    validated.update(trusted)
    validated.update(deltas)
    validated["reason"] = reason
    validated["verdict"] = verdict
    return validated


def bind_row_to_summaries(
    row: dict[str, Any],
    index: int,
    baseline_groups: dict[GroupKey, dict[str, Any]],
    candidate_groups: dict[GroupKey, dict[str, Any]],
    candidate_workload_repetitions: dict[str, int],
) -> dict[str, Any]:
    name = f"rows[{index}]"
    key = (row["server"], row["fixture"], row["workload"])
    trusted = {
        "expected_runs": candidate_workload_repetitions.get(row["workload"]),
    }
    for role, groups in (("baseline", baseline_groups), ("candidate", candidate_groups)):
        group = groups.get(key)
        stats = None if group is None else group["metrics"].get(row["metric"])
        trusted.update(
            {
                f"{role}_status": None if group is None else group["status"],
                f"{role}_successful_runs": (
                    None if group is None else group["successful_runs"]
                ),
                f"{role}_count": None if stats is None else stats["count"],
                f"{role}_mean": None if stats is None else stats["mean"],
                f"{role}_p50": None if stats is None else stats["p50"],
                f"{role}_p95": None if stats is None else stats["p95"],
            }
        )
    for field, expected_value in trusted.items():
        if row[field] != expected_value:
            raise ValidationError(f"{name}.{field} does not match the downloaded summaries")
    return trusted


def validate_comparison(
    value: Any,
    *,
    baseline_digest: str,
    candidate_digest: str,
    baseline_provenance: dict[str, str],
    candidate_provenance: dict[str, str],
    baseline_groups: dict[GroupKey, dict[str, Any]],
    candidate_groups: dict[GroupKey, dict[str, Any]],
    trusted_blockers: list[str],
    expected_row_keys: set[RowKey],
    candidate_workload_repetitions: dict[str, int],
    expected_threshold_pct: float,
    expected_min_samples: int,
) -> dict[str, Any]:
    report = require_exact_keys(value, TOP_LEVEL_KEYS, "comparison")
    if (
        require_count(report["schema_version"], "schema_version", minimum=1)
        != COMPARISON_SCHEMA_VERSION
    ):
        raise ValidationError("schema_version is not supported")
    baseline = validate_source(
        report["baseline"], "baseline", baseline_digest, baseline_provenance
    )
    candidate = validate_source(
        report["candidate"], "candidate", candidate_digest, candidate_provenance
    )
    threshold_pct = require_number(report["threshold_pct"], "threshold_pct", minimum=0.0)
    min_samples = require_count(report["min_samples"], "min_samples", minimum=1)
    if threshold_pct != expected_threshold_pct:
        raise ValidationError("threshold_pct does not match the trusted workflow value")
    if min_samples != expected_min_samples:
        raise ValidationError("min_samples does not match the trusted workflow value")
    compatible = not trusted_blockers
    if require_bool(report["compatible"], "compatible") != compatible:
        raise ValidationError("compatible does not match the downloaded summary contracts")

    blockers = report["blockers"]
    if type(blockers) is not list or len(blockers) > 32:
        raise ValidationError("blockers must be an array with at most 32 entries")
    reported_blockers = [
        require_text(blocker, f"blockers[{index}]") for index, blocker in enumerate(blockers)
    ]
    if compatible != (not reported_blockers):
        raise ValidationError("compatible does not match the blocker list")

    rows = report["rows"]
    if type(rows) is not list or len(rows) > MAX_ROWS:
        raise ValidationError(f"rows must be an array with at most {MAX_ROWS} entries")
    validated_rows = []
    row_keys = set()
    for index, row in enumerate(rows):
        validated = validate_row(
            row,
            index,
            compatible=compatible,
            threshold_pct=threshold_pct,
            min_samples=min_samples,
            baseline_groups=baseline_groups,
            candidate_groups=candidate_groups,
            candidate_workload_repetitions=candidate_workload_repetitions,
        )
        key = tuple(validated[field] for field in ("server", "fixture", "workload", "metric"))
        if key in row_keys:
            raise ValidationError(f"rows[{index}] duplicates an earlier metric row")
        row_keys.add(key)
        validated_rows.append(validated)

    if row_keys != expected_row_keys:
        raise ValidationError("comparison rows do not match the summary metric keys")

    verdict_counts = {verdict: 0 for verdict in VERDICTS}
    for row in validated_rows:
        verdict_counts[row["verdict"]] += 1
    totals = {
        "regressions": verdict_counts["regression"],
        "improvements": verdict_counts["improvement"],
        "stable": verdict_counts["stable"],
        "inconclusive": verdict_counts["inconclusive"],
    }
    totals["comparable_metrics"] = (
        totals["regressions"] + totals["improvements"] + totals["stable"]
    )
    for field, expected in totals.items():
        reported = require_count(report[field], field)
        if reported != expected:
            raise ValidationError(f"{field} does not match the recomputed row count")

    validated_report = dict(report)
    validated_report.update(totals)
    validated_report["baseline"] = baseline
    validated_report["candidate"] = candidate
    validated_report["blockers"] = trusted_blockers
    validated_report["rows"] = validated_rows
    return validated_report


def markdown_text(value: str) -> str:
    return "".join(MARKDOWN_ESCAPES.get(character, character) for character in value)


def format_number(value: Any) -> str:
    return "-" if value is None else f"{float(value):.2f}"


def format_percentage(value: Any) -> str:
    return "-" if value is None else f"{float(value):+.2f}%"


def overall_verdict(report: dict[str, Any]) -> str:
    if not report["compatible"]:
        return "INCONCLUSIVE"
    if report["regressions"]:
        return "REGRESSION"
    if report["inconclusive"] or not report["comparable_metrics"]:
        return "INCONCLUSIVE"
    if report["improvements"]:
        return "NO REGRESSION (improvements detected)"
    return "STABLE"


def render_comparison(report: dict[str, Any]) -> str:
    output = [
        "# Solar LSP PR benchmark",
        "",
        f"**{overall_verdict(report)}**",
        "",
        "This is a portable same-runner-class signal, not an authoritative "
        "cross-server comparison.",
        "",
        "| Field | Value |",
        "|---|---:|",
    ]
    metadata = (
        ("Baseline revision", report["baseline"]["revision"]),
        ("Baseline summary", report["baseline"]["summary_sha256"]),
        ("Baseline executable", report["baseline"]["executable_sha256"]),
        ("Candidate revision", report["candidate"]["revision"]),
        ("Candidate summary", report["candidate"]["summary_sha256"]),
        ("Candidate executable", report["candidate"]["executable_sha256"]),
        ("Noise threshold", f"{float(report['threshold_pct']):.2f}%"),
        ("Minimum samples", str(report["min_samples"])),
        ("Compatible", "yes" if report["compatible"] else "no"),
        ("Comparable metrics", str(report["comparable_metrics"])),
        ("Regressions", str(report["regressions"])),
        ("Improvements", str(report["improvements"])),
        ("Stable", str(report["stable"])),
        ("Inconclusive", str(report["inconclusive"])),
    )
    output.extend(f"| {name} | {value} |" for name, value in metadata)

    if report["blockers"]:
        output.extend(("", "## Compatibility blockers", ""))
        output.extend(f"- {markdown_text(blocker)}" for blocker in report["blockers"])

    if report["rows"]:
        output.extend(
            (
                "",
                "## Metric deltas",
                "",
                "Higher values are worse. A regression or improvement requires both p50 and "
                "p95 to cross the noise threshold in the same direction.",
                "",
                "| Workload | Metric | Samples | Baseline p50 | Candidate p50 | p50 delta | "
                "p95 delta | Verdict | Reason |",
                "|---|---|---:|---:|---:|---:|---:|---|---|",
            )
        )
        for row in report["rows"]:
            workload = markdown_text("/".join((row["server"], row["fixture"], row["workload"])))
            baseline_count = row["baseline_count"]
            candidate_count = row["candidate_count"]
            if baseline_count == candidate_count and baseline_count is not None:
                samples = str(baseline_count)
            elif baseline_count is None and candidate_count is None:
                samples = "-"
            else:
                baseline_count_text = baseline_count if baseline_count is not None else "-"
                candidate_count_text = candidate_count if candidate_count is not None else "-"
                samples = f"{baseline_count_text}/{candidate_count_text}"
            values = (
                workload,
                markdown_text(row["metric"]),
                samples,
                format_number(row["baseline_p50"]),
                format_number(row["candidate_p50"]),
                format_percentage(row["p50_delta_pct"]),
                format_percentage(row["p95_delta_pct"]),
                row["verdict"],
                markdown_text(row["reason"] or ""),
            )
            output.append(f"| {' | '.join(values)} |")
    rendered = "\n".join(output) + "\n"
    if len(rendered.encode("utf-8")) > MAX_COMMENT_BYTES:
        raise ValidationError("rendered comment is too large")
    return rendered


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--baseline-summary", type=Path)
    parser.add_argument("--candidate-summary", type=Path)
    parser.add_argument("--expected-baseline-revision", required=True)
    parser.add_argument("--expected-candidate-revision", required=True)
    parser.add_argument("--expected-baseline-source-url", required=True)
    parser.add_argument("--expected-candidate-source-url", required=True)
    parser.add_argument("--expected-threshold-pct", type=float, required=True)
    parser.add_argument("--expected-min-samples", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    rendered = INCONCLUSIVE_REPORT
    artifact_paths = (args.input, args.baseline_summary, args.candidate_summary)
    if any(path is not None for path in artifact_paths) and not all(
        path is not None for path in artifact_paths
    ):
        print("warning: comparison artifact was missing or invalid", file=sys.stderr)
    elif args.input is not None:
        try:
            expected_baseline_revision = require_revision(
                args.expected_baseline_revision, "expected baseline revision"
            )
            expected_candidate_revision = require_revision(
                args.expected_candidate_revision, "expected candidate revision"
            )
            expected_baseline_source_url = require_text(
                args.expected_baseline_source_url,
                "expected baseline source URL",
                maximum=MAX_PATH_LENGTH,
            )
            expected_candidate_source_url = require_text(
                args.expected_candidate_source_url,
                "expected candidate source URL",
                maximum=MAX_PATH_LENGTH,
            )
            expected_threshold_pct = require_number(
                args.expected_threshold_pct, "expected threshold_pct", minimum=0.0
            )
            if expected_threshold_pct == 0.0:
                raise ValidationError("expected threshold_pct must be greater than zero")
            expected_min_samples = require_count(
                args.expected_min_samples, "expected min_samples", minimum=1
            )
            baseline_value, baseline_digest = load_json(
                args.baseline_summary, "baseline summary"
            )
            candidate_value, candidate_digest = load_json(
                args.candidate_summary, "candidate summary"
            )
            baseline_provenance = validate_summary(
                baseline_value,
                "baseline",
                expected_baseline_revision,
                expected_baseline_source_url,
            )
            candidate_provenance = validate_summary(
                candidate_value,
                "candidate",
                expected_candidate_revision,
                expected_candidate_source_url,
            )
            baseline_groups, _ = validate_summary_contract(baseline_value, "baseline")
            candidate_groups, candidate_workload_repetitions = validate_summary_contract(
                candidate_value, "candidate"
            )
            trusted_blockers = compatibility_blockers(
                summary_compatibility_contract(baseline_value, "baseline"),
                summary_compatibility_contract(candidate_value, "candidate"),
            )
            comparison_value, _ = load_json(args.input, "comparison artifact")
            report = validate_comparison(
                comparison_value,
                baseline_digest=baseline_digest,
                candidate_digest=candidate_digest,
                baseline_provenance=baseline_provenance,
                candidate_provenance=candidate_provenance,
                baseline_groups=baseline_groups,
                candidate_groups=candidate_groups,
                trusted_blockers=trusted_blockers,
                expected_row_keys=comparison_row_keys(baseline_groups, candidate_groups),
                candidate_workload_repetitions=candidate_workload_repetitions,
                expected_threshold_pct=expected_threshold_pct,
                expected_min_samples=expected_min_samples,
            )
            rendered = render_comparison(report)
        except (OSError, UnicodeError, ValueError, OverflowError, RecursionError):
            print("warning: comparison artifact was missing or invalid", file=sys.stderr)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
