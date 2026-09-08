#!/usr/bin/env python3
"""Compare benchmark.py runs, inspect artifacts, and generate the CI report.

This script is intentionally non-gating: runtime benchmarks are useful CI
signals, but benchmark deltas should be reviewed rather than fail PRs.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import math
import os
import platform
import subprocess
import sys
import uuid
from contextlib import nullcontext
from pathlib import Path
from typing import Any
from urllib.parse import urlencode

from benchmark import workload_signature

PERF_SITE_URL = "https://getfoundry.sh/perf/solar/"

METRICS = {
    "total_gas": "runtime gas",
    "runtime_size": "runtime bytes",
    "bytecode_size": "creation bytes",
    "deploy_gas": "deployment gas",
    "compile_time_seconds": "compile seconds",
    "peak_rss_bytes": "peak RSS bytes",
}
ARTIFACT_KINDS = {
    "mir": (".mir",),
    "evm-ir": (".evmir",),
    "disasm": (".disasm",),
    "bytecode": (".hex",),
    "json": (".json",),
}


def numeric(value: Any) -> bool:
    return type(value) in (int, float) and math.isfinite(value)


def delta(before: Any, after: Any, reason: str | None = None) -> dict[str, Any]:
    comparable = reason is None and numeric(before) and numeric(after)
    return {
        "before": before,
        "after": after,
        "delta": after - before if comparable else None,
        "percent": pct_change(after, before) if comparable else None,
        "reason": reason or (None if comparable else "measurement missing"),
    }


def compare_runs(
    results: list[dict[str, Any]],
    baseline_results: list[dict[str, Any]],
    compiler: str = "solar",
) -> dict[str, Any]:
    current = by_test_id(results)
    baseline = by_test_id(baseline_results)
    rows = []
    for key in sorted(current.keys() | baseline.keys()):
        after = current.get(key, {})
        before = baseline.get(key, {})
        new = compiler_data(after, compiler)
        old = compiler_data(before, compiler)
        issues = []
        input_reason = None
        if not old or not new:
            input_reason = "compiler result missing from " + (
                "baseline" if not old else "candidate"
            )
        elif old.get("status") != "ok" or new.get("status") != "ok":
            input_reason = "compilation failed"
        elif not old.get("input_fingerprint") or not new.get("input_fingerprint"):
            input_reason = "input fingerprint missing"
        elif old["input_fingerprint"] != new["input_fingerprint"]:
            input_reason = "compiler inputs differ"
        if input_reason:
            issues.append(input_reason)

        gas_reason = input_reason
        if gas_reason is None:
            if not before.get("gas_profile") or before.get("gas_profile") != after.get(
                "gas_profile"
            ):
                gas_reason = "gas profiles differ or are missing"
            elif workload_signature(old) != workload_signature(new):
                gas_reason = "ordered runtime workloads differ"
            elif old.get("gas_status") != "ok" or new.get("gas_status") != "ok":
                gas_reason = "gas run failed or was not measured"
            elif any(runtime_failed(case, compiler) for case in (before, after)):
                gas_reason = "runtime checks failed"
        if (
            gas_reason
            and gas_reason not in issues
            and any(numeric(data.get("total_gas")) for data in (old, new))
        ):
            issues.append(gas_reason)

        runtime_changes = []
        if input_reason is None and workload_signature(old) == workload_signature(new):
            for old_check, new_check in zip(
                old.get("runtime_results") or [],
                new.get("runtime_results") or [],
                strict=True,
            ):
                if old_check.get("value") != new_check.get("value"):
                    runtime_changes.append(
                        {
                            "label": new_check.get("label"),
                            "before": old_check.get("value"),
                            "after": new_check.get("value"),
                        }
                    )
            if runtime_changes:
                gas_reason = "runtime observations differ"
                issues.append(gas_reason)

        for label, data in (("baseline", old), ("candidate", new)):
            for field in ("error", "artifact_error", "deploy_error"):
                if data.get(field):
                    issues.append(f"{label} {field}: {data[field]}")
            if data.get("runtime_status") in ("failed", "mismatch"):
                issues.append(f"{label} runtime checks failed")
            if data.get("gas_status") == "failed":
                issues.append(f"{label} gas run failed")
        for label, case in (("baseline", before), ("candidate", after)):
            if case.get("benchmark_error"):
                issues.append(f"{label}: {case['benchmark_error']}")
            if runtime_mismatches(case, compiler):
                issues.append(f"{label} cross-compiler runtime checks failed")

        measurements = {}
        for metric_name in METRICS:
            reason = input_reason
            if metric_name in ("total_gas", "deploy_gas"):
                reason = gas_reason
            elif (
                metric_name in ("compile_time_seconds", "peak_rss_bytes")
                and reason is None
            ):
                old_build = compiler_build_fingerprint(before, compiler)
                new_build = compiler_build_fingerprint(after, compiler)
                if "unknown" in (old_build[1], new_build[1]) or old_build != new_build:
                    reason = "compiler build profiles or labels differ or are unknown"
            measurements[metric_name] = delta(
                old.get(metric_name), new.get(metric_name), reason
            )

        calls = []
        if gas_reason is None:
            for old_call, new_call in zip(
                old.get("gas_results") or [], new.get("gas_results") or [], strict=True
            ):
                calls.append(
                    {
                        "label": new_call.get("label"),
                        "call": new_call.get("call"),
                        "args": new_call.get("args"),
                        **delta(old_call.get("gas"), new_call.get("gas")),
                    }
                )
        old_output = old.get("output_fingerprint")
        new_output = new.get("output_fingerprint")
        rows.append(
            {
                "suite": key[0],
                "test_id": key[1],
                "compiler": compiler,
                "compile_only": before.get("contract_name")
                == after.get("contract_name")
                == "*",
                "status_before": old.get("status"),
                "status_after": new.get("status"),
                "input_fingerprint_before": old.get("input_fingerprint"),
                "input_fingerprint_after": new.get("input_fingerprint"),
                "compiler_build_before": compiler_build_fingerprint(before, compiler),
                "compiler_build_after": compiler_build_fingerprint(after, compiler),
                "issues": issues,
                "metrics": measurements,
                "gas_calls": calls,
                "runtime_changes": runtime_changes,
                "compile_samples_before": old.get("compile_time_samples", []),
                "compile_samples_after": new.get("compile_time_samples", []),
                "compiler_output_changed": old_output != new_output
                if old_output and new_output
                else None,
            }
        )

    totals = {}
    for metric_name in METRICS:
        paired = [
            row for row in rows if row["metrics"][metric_name]["delta"] is not None
        ]
        totals[metric_name] = {
            "tests": [f"{row['suite']}/{row['test_id']}" for row in paired],
            **delta(
                sum(row["metrics"][metric_name]["before"] for row in paired)
                if paired
                else None,
                sum(row["metrics"][metric_name]["after"] for row in paired)
                if paired
                else None,
            ),
        }
    summary = {}
    for name in METRICS:
        paired = [
            row["metrics"][name]
            for row in rows
            if row["metrics"][name]["delta"] is not None
        ]
        positive = [
            value for value in paired if value["before"] > 0 and value["after"] > 0
        ]
        summary[name] = {
            "paired": len(paired),
            "ratio_pairs": len(positive),
            "percent": math.expm1(
                math.fsum(
                    math.log(value["after"] / value["before"]) for value in positive
                )
                / len(positive)
            )
            * 100
            if positive
            else None,
            "improved": sum(value["delta"] < 0 for value in paired),
            "regressed": sum(value["delta"] > 0 for value in paired),
            "unchanged": sum(value["delta"] == 0 for value in paired),
        }
    return {
        "format_version": 1,
        "compiler": compiler,
        "rows": rows,
        "totals": totals,
        "summary": summary,
    }


def artifact_files(root: Path | None, test_id: str, compiler: str) -> dict[str, Path]:
    if root is None:
        return {}
    directory = root / test_id / compiler
    if not directory.resolve().is_relative_to(root.resolve()):
        raise ValueError(f"artifact path escapes its root: {directory}")
    files = {}
    if directory.is_dir():
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                if not path.resolve().is_relative_to(root.resolve()):
                    raise ValueError(f"artifact symlink escapes its root: {path}")
                files[path.relative_to(directory).as_posix()] = path
    return files


def compare_artifacts(
    comparison: dict[str, Any],
    baseline_root: Path | None,
    current_root: Path | None,
    kinds: list[str],
    diff_output: Path | None = None,
) -> None:
    suffixes = tuple(suffix for kind in kinds for suffix in ARTIFACT_KINDS[kind])
    if diff_output is not None:
        diff_output.parent.mkdir(parents=True, exist_ok=True)
    with diff_output.open("w") if diff_output is not None else nullcontext() as patch:
        for row in comparison["rows"]:
            before = artifact_files(baseline_root, row["test_id"], row["compiler"])
            after = artifact_files(current_root, row["test_id"], row["compiler"])
            row["artifacts_available"] = {"before": bool(before), "after": bool(after)}
            row["artifacts"] = []
            for name in sorted(before.keys() | after.keys()):
                if not name.endswith(suffixes):
                    continue
                old = before[name].read_bytes() if name in before else None
                new = after[name].read_bytes() if name in after else None
                status = (
                    "added"
                    if old is None
                    else "removed"
                    if new is None
                    else "unchanged"
                    if old == new
                    else "changed"
                )
                row["artifacts"].append(
                    {
                        "name": name,
                        "status": status,
                        "before": str(before[name]) if name in before else None,
                        "after": str(after[name]) if name in after else None,
                        "bytes_before": len(old) if old is not None else None,
                        "bytes_after": len(new) if new is not None else None,
                        "sha256_before": hashlib.sha256(old).hexdigest()
                        if old is not None
                        else None,
                        "sha256_after": hashlib.sha256(new).hexdigest()
                        if new is not None
                        else None,
                    }
                )
                if patch is not None and status != "unchanged":
                    relative = f"{row['test_id']}/{row['compiler']}/{name}"
                    patch.writelines(
                        difflib.unified_diff(
                            (old or b"")
                            .decode("utf-8", errors="replace")
                            .splitlines(keepends=True),
                            (new or b"")
                            .decode("utf-8", errors="replace")
                            .splitlines(keepends=True),
                            fromfile=f"before/{relative}"
                            if old is not None
                            else "/dev/null",
                            tofile=f"after/{relative}"
                            if new is not None
                            else "/dev/null",
                        )
                    )


def comparison_report(comparison: dict[str, Any]) -> str:
    rows = comparison["rows"]
    lines = [
        "### Run comparison",
        "",
        f"Compiler: `{comparison['compiler']}`. Deltas are candidate minus baseline; lower is better.",
        "Change is the geometric mean of candidate/baseline ratios, with equal weight per benchmark. Only positive, comparable pairs enter the mean.",
        "Runtime gas is the sum of measured calls within each benchmark. Timing and RSS are noisy.",
        "",
        "| Metric | Change | Pairs in mean | Improved | Regressed | Unchanged | Excluded from mean |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for name, values in comparison["summary"].items():
        change = (
            fmt_pct(values["percent"], positive_is_good=False)
            if values["percent"] is not None
            else "n/a"
        )
        lines.append(
            f"| {METRICS[name]} | {change} | {values['ratio_pairs']} | {values['improved']} | {values['regressed']} | {values['unchanged']} | {len(rows) - values['ratio_pairs']} |"
        )
    if compile_only := sum(bool(row.get("compile_only")) for row in rows):
        lines.extend(
            [
                "",
                f"{compile_only} compilation-only benchmarks do not measure gas, per-contract size, or artifacts.",
            ]
        )
    if comparison.get("artifact_warning"):
        lines.extend(["", f"> {comparison['artifact_warning']}"])

    issues = [
        f"- `{row['test_id']}`: {markdown_cell(issue)}"
        for row in rows
        for issue in row["issues"]
    ]
    if issues:
        lines.extend(["", "#### Incomplete or incompatible comparisons", "", *issues])
    changed = []
    for name, metric_label in METRICS.items():
        if comparison["compiler"] == "solar":
            continue
        for row in sorted(
            rows, key=lambda row: row["metrics"][name]["percent"] or 0, reverse=True
        ):
            values = row["metrics"][name]
            if values["delta"] not in (None, 0):
                changed.append(
                    f"| {markdown_cell(row['test_id'])} | {metric_label} | {comparison_cells(values, name)} |"
                )
    if changed:
        lines.extend(
            [
                "",
                "<details>",
                "<summary>Per-case metric changes (regressions first)</summary>",
                "",
                "| Case | Metric | Baseline | Candidate | Delta | Change |",
                "| --- | --- | ---: | ---: | ---: | ---: |",
                *changed,
                "",
                "</details>",
            ]
        )
    calls = [
        f"| {markdown_cell(row['test_id'])} | {markdown_cell(call['label'])} | {comparison_cells(call)} |"
        for row in rows
        for call in sorted(
            row["gas_calls"], key=lambda call: call["percent"] or 0, reverse=True
        )
        if call["delta"] not in (None, 0)
    ]
    if calls:
        lines.extend(
            [
                "",
                "<details>",
                "<summary>Per-call gas changes</summary>",
                "",
                "| Case | Call | Baseline | Candidate | Delta | Change |",
                "| --- | --- | ---: | ---: | ---: | ---: |",
                *calls,
                "",
                "</details>",
            ]
        )
    artifacts = []
    for row in rows:
        files = row.get("artifacts", [])
        changes = [
            f"{item['name']} ({item['status']})"
            for item in files
            if item["status"] != "unchanged"
        ]
        available = row.get("artifacts_available", {})
        if not row.get("compile_only") and (
            not available.get("before") or not available.get("after")
        ):
            changes.append(
                "artifacts unavailable on "
                + (
                    "both sides"
                    if not any(available.values())
                    else "baseline"
                    if not available.get("before")
                    else "candidate"
                )
            )
        if row["compiler_output_changed"]:
            changes.append("compiler output fingerprint changed")
        if changes:
            artifacts.append(
                f"| {markdown_cell(row['test_id'])} | {markdown_cell(', '.join(changes))} |"
            )
    if artifacts:
        lines.extend(
            [
                "",
                "<details>",
                "<summary>Artifact changes and availability</summary>",
                "",
                "| Case | Artifacts |",
                "| --- | --- |",
                *artifacts,
                "",
                "</details>",
                "",
            ]
        )
    return "\n".join(lines)


def comparison_cells(values: dict[str, Any], metric_name: str | None = None) -> str:
    def fmt(value: Any) -> str:
        if numeric(value) and metric_name == "peak_rss_bytes":
            return ("-" if value < 0 else "") + fmt_bytes(abs(value))
        if numeric(value) and metric_name == "compile_time_seconds":
            return ("-" if value < 0 else "") + fmt_duration(abs(value))
        if type(value) is int:
            return f"{value:,}"
        return f"{value:,.6g}" if numeric(value) else "n/a"

    change = (
        fmt_pct(values["percent"], positive_is_good=False)
        if values["percent"] is not None
        else "n/a"
    )
    return " | ".join(
        (fmt(values["before"]), fmt(values["after"]), fmt(values["delta"]), change)
    )


def normalize_timings(timings: Any) -> dict[str, int | float]:
    if not isinstance(timings, dict):
        return {}
    normalized = {}
    for name, value in timings.items():
        if isinstance(value, dict):
            value = value.get("wall_time_seconds")
        if isinstance(value, (int, float)):
            normalized["repository" if name == "repo" else str(name)] = value
    return normalized


def load_document(path: Path | None, label: str) -> dict[str, Any]:
    empty = {"results": [], "timings": {}}
    if path is None:
        return empty
    if not path.exists():
        warning(f"{label} benchmark results not found: {path}")
        return empty
    with path.open() as f:
        data = json.load(f)
    if isinstance(data, list):
        return {"results": data, "timings": {}}
    if not isinstance(data, dict) or not isinstance(data.get("results"), list):
        warning(
            f"{label} benchmark results have unexpected shape: expected result document"
        )
        return empty
    return {
        "results": data["results"],
        "timings": normalize_timings(data.get("timings")),
    }


def suite_name(result: dict[str, Any]) -> str:
    suite = str(result.get("suite", "repository"))
    return "repository" if suite == "repo" else suite


def suite_key(result: dict[str, Any]) -> tuple[str, str]:
    return (suite_name(result), str(result.get("test_id", "<unknown>")))


def by_test_id(results: list[dict[str, Any]]) -> dict[tuple[str, str], dict[str, Any]]:
    indexed = {}
    for result in results:
        key = suite_key(result)
        if key in indexed:
            raise ValueError(f"duplicate benchmark result: {'/'.join(key)}")
        indexed[key] = result
    return indexed


def compiler_failures(results: list[dict[str, Any]]) -> list[str]:
    failures = []
    for result in results:
        test_id = "/".join(suite_key(result))
        if error := result.get("benchmark_error"):
            failures.append(f"{test_id}: {error}")
            continue
        compilers = result.get("compilers", {})
        for compiler_id, data in compilers.items():
            if compiler_id == "solar" and data.get("status") != "ok":
                error_lines = str(data.get("error") or "").strip().splitlines()
                error = error_lines[0] if error_lines else "compiler failed"
                failures.append(f"{test_id} {compiler_id}: {error}")
    return failures


def shorten(value: Any, limit: int = 160) -> str:
    text = str(value).replace("\n", " ")
    if len(text) <= limit:
        return text
    return text[: limit - 1] + "..."


def format_values(values: dict[str, Any]) -> str:
    return ", ".join(
        f"{compiler}={shorten(value)}" for compiler, value in values.items()
    )


def runtime_mismatches(result: dict[str, Any], compiler: str) -> list[dict[str, Any]]:
    return [
        mismatch
        for mismatch in result.get("runtime_mismatches") or []
        if (value := (mismatch.get("values") or {}).get(compiler)) is not None
        and any(
            other is not None and other != value
            for other in mismatch["values"].values()
        )
    ]


def runtime_failed(result: dict[str, Any], compiler: str) -> bool:
    return compiler_data(result, compiler).get("runtime_status") in (
        "failed",
        "mismatch",
    ) or bool(runtime_mismatches(result, compiler))


def runtime_issue_details(results: list[dict[str, Any]]) -> list[str]:
    details = []
    for result in results:
        if not runtime_failed(result, "solar"):
            continue
        test_id = "/".join(suite_key(result))
        before = len(details)

        for mismatch in runtime_mismatches(result, "solar"):
            label = mismatch.get("label", "<unknown>")
            values = mismatch.get("values") or {}
            details.append(f"{test_id} {label}: {format_values(values)}")

        for compiler_id, data in (result.get("compilers") or {}).items():
            if compiler_id != "solar":
                continue
            for check in data.get("runtime_results") or []:
                if check.get("status") == "ok":
                    continue
                label = check.get("label", "<unknown>")
                error = check.get("error") or check.get("status")
                details.append(f"{test_id} {compiler_id} {label}: {shorten(error)}")

        if len(details) == before:
            status = compiler_data(result, "solar").get("runtime_status")
            details.append(f"{test_id} solar: runtime_status={status}")

    return details


def baseline_regression_details(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> list[str]:
    details = []
    baseline = by_test_id(baseline_results)
    for result in results:
        test_id = "/".join(suite_key(result))
        base = baseline.get(suite_key(result))
        if base is None:
            continue

        solar_gas = total_gas(result, "solar")
        base_solar_gas = total_gas(base, "solar")
        if (
            solar_gas is not None
            and base_solar_gas is not None
            and solar_gas > base_solar_gas
        ):
            details.append(
                f"{test_id} solar gas regressed vs previous Solar run: "
                f"{base_solar_gas:,} -> {solar_gas:,} "
                f"({absolute_delta(solar_gas, base_solar_gas)}, "
                f"{pct_increase(solar_gas, base_solar_gas)} worse)"
            )

        solar_size = runtime_size(result, "solar")
        base_solar_size = runtime_size(base, "solar")
        if (
            solar_size is not None
            and base_solar_size is not None
            and solar_size > base_solar_size
        ):
            details.append(
                f"{test_id} solar runtime size regressed vs previous Solar run: "
                f"{base_solar_size:,}B -> {solar_size:,}B "
                f"({absolute_delta(solar_size, base_solar_size)}B, "
                f"{pct_increase(solar_size, base_solar_size)} worse)"
            )

        solar_deploy_gas = deploy_gas(result, "solar")
        base_solar_deploy_gas = deploy_gas(base, "solar")
        if (
            solar_deploy_gas is not None
            and base_solar_deploy_gas is not None
            and solar_deploy_gas > base_solar_deploy_gas
        ):
            details.append(
                f"{test_id} solar deployment gas regressed vs previous Solar run: "
                f"{base_solar_deploy_gas:,} -> {solar_deploy_gas:,} "
                f"({absolute_delta(solar_deploy_gas, base_solar_deploy_gas)}, "
                f"{pct_increase(solar_deploy_gas, base_solar_deploy_gas)} worse)"
            )

        solar_creation_size = creation_size(result, "solar")
        base_solar_creation_size = creation_size(base, "solar")
        if (
            solar_creation_size is not None
            and base_solar_creation_size is not None
            and solar_creation_size > base_solar_creation_size
        ):
            details.append(
                f"{test_id} solar creation size regressed vs previous Solar run: "
                f"{base_solar_creation_size:,}B -> {solar_creation_size:,}B "
                f"({absolute_delta(solar_creation_size, base_solar_creation_size)}B, "
                f"{pct_increase(solar_creation_size, base_solar_creation_size)} worse)"
            )

    return details


# Wall-clock compile times jitter between CI runners. Require both a relative
# and absolute change before posting a fresh PR comment.
COMPILE_TIME_BENCH_CHANGE = 0.20
COMPILE_TIME_BENCH_ABSOLUTE_CHANGE = 0.010
COMPILE_TIME_TOTAL_CHANGE = 0.10
COMPILE_TIME_TOTAL_ABSOLUTE_CHANGE = 1.0


def compiler_status(result: dict[str, Any] | None, compiler: str) -> str | None:
    if result is None:
        return None
    data = compiler_data(result, compiler)
    if not data:
        return None
    return "ok" if data.get("status") == "ok" else "n/a"


def compilation_failure_report(
    results: list[dict[str, Any]],
    baseline_results: list[dict[str, Any]],
    baseline_ref: str,
) -> list[str]:
    current = by_test_id(results)
    baseline = by_test_id(baseline_results)
    keys = [*current, *(key for key in baseline if key not in current)]
    rows = []
    for key in keys:
        current_status = compiler_status(current.get(key), "solar")
        baseline_status = compiler_status(baseline.get(key), "solar")
        if "n/a" not in (current_status, baseline_status):
            continue

        statuses = []
        if baseline_status is not None:
            statuses.append(f"`{baseline_ref}` = `{baseline_status}`")
        if current_status is not None:
            statuses.append(f"branch = `{current_status}`")
        rows.append(f"> - `{'/'.join(key)}`: {', '.join(statuses)}")

    if not rows:
        return []
    return [
        "> [!NOTE]",
        "> The compiler failed on these benchmarks; `n/a` marks the failed revision:",
        ">",
        *rows,
        "",
    ]


def has_codegen_changes(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> bool:
    baseline = by_test_id(baseline_results)
    for result in results:
        base = baseline.get(suite_key(result))
        if base is None:
            continue

        current_status = compiler_status(result, "solar")
        baseline_status = compiler_status(base, "solar")
        if (
            current_status is not None
            and baseline_status is not None
            and current_status != baseline_status
        ):
            return True

        solar_gas = total_gas(result, "solar")
        base_solar_gas = total_gas(base, "solar")
        if (
            solar_gas is not None
            and base_solar_gas is not None
            and solar_gas != base_solar_gas
        ):
            return True

        solar_size = runtime_size(result, "solar")
        base_solar_size = runtime_size(base, "solar")
        if (
            solar_size is not None
            and base_solar_size is not None
            and solar_size != base_solar_size
        ):
            return True

        solar_deploy_gas = deploy_gas(result, "solar")
        base_solar_deploy_gas = deploy_gas(base, "solar")
        if (
            solar_deploy_gas is not None
            and base_solar_deploy_gas is not None
            and solar_deploy_gas != base_solar_deploy_gas
        ):
            return True

        solar_creation_size = creation_size(result, "solar")
        base_solar_creation_size = creation_size(base, "solar")
        if (
            solar_creation_size is not None
            and base_solar_creation_size is not None
            and solar_creation_size != base_solar_creation_size
        ):
            return True

    return False


def has_compile_time_changes(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> bool:
    baseline = by_test_id(baseline_results)
    time_sum = 0.0
    base_time_sum = 0.0
    for result in results:
        base = baseline.get(suite_key(result))
        if base is None:
            if successful_compile_time(result, "solar") is not None:
                return True
            continue

        solar_time = successful_compile_time(result, "solar")
        base_solar_time = baseline_compile_time(result, base, "solar")
        if solar_time is not None and base_solar_time is None:
            return True
        if solar_time is not None and base_solar_time is not None:
            time_delta = abs(solar_time - base_solar_time)
            if (
                time_delta > COMPILE_TIME_BENCH_ABSOLUTE_CHANGE
                and time_delta > base_solar_time * COMPILE_TIME_BENCH_CHANGE
            ):
                return True
            time_sum += solar_time
            base_time_sum += base_solar_time

    return (
        abs(time_sum - base_time_sum) > COMPILE_TIME_TOTAL_ABSOLUTE_CHANGE
        and abs(time_sum - base_time_sum) > base_time_sum * COMPILE_TIME_TOTAL_CHANGE
    )


def has_baseline_changes(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> bool:
    return has_codegen_changes(results, baseline_results) or has_compile_time_changes(
        results, baseline_results
    )


def warning(message: str) -> None:
    escaped = message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::warning::{escaped}", file=sys.stderr)


def markdown_cell(value: Any) -> str:
    return str(value).replace("|", "\\|").replace("\n", "<br>")


def perf_link(
    label: str, benchmark: str | None = None, section: str = "benchmarks"
) -> str:
    base = os.environ.get("BENCHMARK_BASE_SHA")
    head = os.environ.get("BENCHMARK_PR_HEAD_SHA")
    if not base or not head:
        return label
    query = {"base": base[:8], "head": head[:8]}
    if benchmark is not None:
        query["benchmark"] = benchmark
        section = "artifacts"
    site = os.environ.get("BENCHMARK_SITE_URL") or PERF_SITE_URL
    return f"[{label}]({site}?{urlencode(query)}#{section})"


def compiler_data(result: dict[str, Any], compiler: str) -> dict[str, Any]:
    data = result.get("compilers") or {}
    value = data.get(compiler)
    return value if isinstance(value, dict) else {}


def total_gas(result: dict[str, Any], compiler: str) -> int | None:
    return compiler_metric(result, compiler, "total_gas")


def deploy_gas(result: dict[str, Any], compiler: str) -> int | None:
    return compiler_metric(result, compiler, "deploy_gas")


def creation_size(result: dict[str, Any], compiler: str) -> int | None:
    return compiler_metric(result, compiler, "bytecode_size")


def runtime_size(result: dict[str, Any], compiler: str) -> int | None:
    return compiler_metric(result, compiler, "runtime_size")


def compiler_metric(result: dict[str, Any], compiler: str, metric: str) -> int | None:
    data = compiler_data(result, compiler)
    if data.get("status") != "ok":
        return None
    value = data.get(metric)
    return value if isinstance(value, int) else None


def peak_rss(result: dict[str, Any], compiler: str) -> int | None:
    data = compiler_data(result, compiler)
    value = data.get("peak_rss_bytes")
    if data.get("status") != "ok":
        return None
    return value if isinstance(value, int) else None


def compile_time(result: dict[str, Any], compiler: str) -> float | None:
    data = compiler_data(result, compiler)
    if data.get("status") != "ok":
        return None
    value = data.get("compile_time_seconds")
    if isinstance(value, (int, float)) and value > 0:
        return float(value)
    return None


def successful_compile_time(result: dict[str, Any], compiler: str) -> float | None:
    if compiler_data(result, compiler).get("status") != "ok":
        return None
    return compile_time(result, compiler)


def compiler_build_fingerprint(
    result: dict[str, Any], compiler: str
) -> tuple[str, str]:
    data = compiler_data(result, compiler)
    command = str(data.get("command") or "")
    if "target/release/" in command or "target\\release\\" in command:
        profile = "release"
    elif "target/debug/" in command or "target\\debug\\" in command:
        profile = "debug"
    else:
        profile = "unknown"
    return str(data.get("label") or ""), profile


def baseline_compile_time(
    result: dict[str, Any], baseline: dict[str, Any], compiler: str
) -> float | None:
    current_fingerprint = compiler_build_fingerprint(result, compiler)
    baseline_fingerprint = compiler_build_fingerprint(baseline, compiler)
    if current_fingerprint[1] == "unknown" or baseline_fingerprint[1] == "unknown":
        return None
    if current_fingerprint != baseline_fingerprint:
        return None
    current_input = compiler_data(result, compiler).get("input_fingerprint")
    baseline_input = compiler_data(baseline, compiler).get("input_fingerprint")
    if not current_input or current_input != baseline_input:
        return None
    return successful_compile_time(baseline, compiler)


def fmt_duration(seconds: float | None) -> str:
    if seconds is None:
        return "n/a"
    if seconds >= 1.0:
        return f"{seconds:.3f} s"
    return f"{seconds * 1000:.1f} ms"


def fmt_int(value: int | None, suffix: str = "") -> str:
    if value is None:
        return "n/a"
    return f"{value:,}{suffix}"


def fmt_bytes(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value / (1024 * 1024):,.1f} MiB"


def pct_change(current: float | None, baseline: float | None) -> float | None:
    if current is None or baseline in (None, 0):
        return None
    return (current - baseline) / baseline * 100


def fmt_pct_change_lower_is_better(current: int | None, baseline: int | None) -> str:
    delta = pct_change(current, baseline)
    if delta is None:
        return "n/a"
    return fmt_pct(delta, positive_is_good=False)


def pct_vs_current(current: int | None, comparison: int | None) -> float | None:
    if current in (None, 0) or comparison is None:
        return None
    return (comparison - current) / current * 100


def fmt_pct_vs_current(current: int | None, comparison: int | None) -> str:
    delta = pct_vs_current(current, comparison)
    if delta is None:
        return "n/a"
    return fmt_pct(delta)


def fmt_pct(delta: float, positive_is_good: bool = True) -> str:
    rounded = round(delta, 2)
    if rounded == 0:
        return "~0%"
    emoji = "✅" if (rounded > 0) == positive_is_good else "❌"
    return f"{emoji} {rounded:+.2f}%"


def pct_increase(current: int, baseline: int) -> str:
    if baseline == 0:
        return "n/a"
    delta = (current - baseline) / baseline * 100
    return f"{delta:+.2f}%"


def absolute_delta(current: int | None, baseline: int | None) -> str:
    if current is None or baseline is None:
        return "n/a"
    delta = current - baseline
    return f"{delta:+,}"


def fmt_value_with_lower_is_better_delta(
    value: int | None, current: int | None, baseline: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_change_lower_is_better(current, baseline)})"


def fmt_value_with_delta_vs_current(
    value: int | None, current: int | None, comparison: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_vs_current(current, comparison)})"


def benchmark_rows(
    results: list[dict[str, Any]],
    baseline: dict[tuple[str, str], dict[str, Any]],
    compared: dict | None = None,
) -> list[str]:
    return metric_rows(results, baseline, "total_gas", "runtime_size", compared)


def deployment_rows(
    results: list[dict[str, Any]],
    baseline: dict[tuple[str, str], dict[str, Any]],
    compared: dict | None = None,
) -> list[str]:
    return metric_rows(results, baseline, "deploy_gas", "bytecode_size", compared)


def compared_baseline(
    result: dict, base: dict, metric_name: str, compared: dict | None
) -> Any:
    if compared is not None:
        values = compared[suite_key(result)]["metrics"][metric_name]
        return values["before"] if values["reason"] is None else None
    if metric_name == "compile_time_seconds":
        return baseline_compile_time(result, base, "solar") if base else None
    return compiler_metric(base, "solar", metric_name)


def comparison_has_changes(
    comparison: dict[str, Any], ignore_compile_time: bool
) -> bool:
    for row in comparison["rows"]:
        if (
            row["issues"]
            or row["compiler_output_changed"]
            or any(
                row["metrics"][metric]["delta"] not in (None, 0)
                for metric in (
                    "total_gas",
                    "runtime_size",
                    "bytecode_size",
                    "deploy_gas",
                )
            )
            or any(call["delta"] not in (None, 0) for call in row["gas_calls"])
            or (
                row.get("artifacts_available", {}).get("before")
                and row.get("artifacts_available", {}).get("after")
                and any(
                    item["status"] != "unchanged" for item in row.get("artifacts", [])
                )
            )
        ):
            return True
        timing = row["metrics"]["compile_time_seconds"]
        if (
            not ignore_compile_time
            and timing["delta"] is not None
            and abs(timing["delta"])
            > max(
                COMPILE_TIME_BENCH_ABSOLUTE_CHANGE,
                timing["before"] * COMPILE_TIME_BENCH_CHANGE,
            )
        ):
            return True
    timing = comparison["totals"]["compile_time_seconds"]
    return (
        not ignore_compile_time
        and timing["delta"] is not None
        and abs(timing["delta"])
        > max(
            COMPILE_TIME_TOTAL_ABSOLUTE_CHANGE,
            timing["before"] * COMPILE_TIME_TOTAL_CHANGE,
        )
    )


def metric_rows(
    results: list[dict[str, Any]],
    baseline: dict[tuple[str, str], dict[str, Any]],
    gas_metric: str,
    size_metric: str,
    compared: dict | None = None,
) -> list[str]:
    rows = []
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        base = baseline.get(suite_key(result), {})
        solar_gas = compiler_metric(result, "solar", gas_metric)
        reference_gas = [
            compiler_metric(result, name, gas_metric)
            for name in reference_compiler_ids(results)
        ]
        base_solar_gas = compared_baseline(result, base, gas_metric, compared)
        solar_size = compiler_metric(result, "solar", size_metric)
        reference_size = [
            compiler_metric(result, name, size_metric)
            for name in reference_compiler_ids(results)
        ]
        base_solar_size = compared_baseline(result, base, size_metric, compared)

        if all(
            value is None
            for value in (solar_gas, *reference_gas, solar_size, *reference_size)
        ):
            continue

        rows.append(
            "| "
            + " | ".join(
                [
                    perf_link(markdown_cell(test_id), test_id),
                    fmt_value_with_lower_is_better_delta(
                        solar_gas, solar_gas, base_solar_gas
                    ),
                    *(
                        fmt_value_with_delta_vs_current(value, solar_gas, value)
                        for value in reference_gas
                    ),
                    fmt_value_with_lower_is_better_delta(
                        solar_size, solar_size, base_solar_size, "B"
                    ),
                    *(
                        fmt_value_with_delta_vs_current(value, solar_size, value, "B")
                        for value in reference_size
                    ),
                ]
            )
            + " |"
        )
    return rows


def compiler_ids(results: list[dict[str, Any]]) -> list[str]:
    ids = []
    for result in results:
        for compiler_id in result.get("compilers") or {}:
            if compiler_id not in ids:
                ids.append(compiler_id)
    return ids


def reference_compiler_ids(results: list[dict[str, Any]]) -> list[str]:
    return [
        "solc",
        *(name for name in compiler_ids(results) if name not in ("solar", "solc")),
    ]


def memory_summary_rows(results: list[dict[str, Any]]) -> list[str]:
    rows = []
    for compiler_id in compiler_ids(results):
        values = [
            (str(result.get("test_id", "<unknown>")), value)
            for result in results
            if (value := peak_rss(result, compiler_id)) is not None
        ]
        if not values:
            continue
        max_bench, max_value = max(values, key=lambda item: item[1])
        average = sum(value for _, value in values) / len(values)
        rows.append(
            f"| {markdown_cell(compiler_id)} | {len(values)} | {fmt_bytes(average)} | "
            f"{fmt_bytes(max_value)} | {markdown_cell(max_bench)} |"
        )
    return rows


def memory_benchmark_rows(
    results: list[dict[str, Any]], compared: dict | None = None
) -> list[str]:
    ids = compiler_ids(results)
    rows = []
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        values = {compiler_id: peak_rss(result, compiler_id) for compiler_id in ids}
        cells = [
            perf_link(markdown_cell(test_id), test_id),
            *(fmt_bytes(values[compiler_id]) for compiler_id in ids),
        ]
        if "solar" in values and "solc" in values:
            cells.append(
                fmt_pct_change_lower_is_better(values["solar"], values["solc"])
            )
        if compared is not None:
            measurement = compared[suite_key(result)]["metrics"]["peak_rss_bytes"]
            cells.append(
                fmt_pct(measurement["percent"], positive_is_good=False)
                if measurement["percent"] is not None
                else "n/a"
            )
        rows.append("| " + " | ".join(cells) + " |")
    return rows


def memory_report(
    results: list[dict[str, Any]], compared: dict | None = None
) -> list[str]:
    ids = compiler_ids(results)
    summary_rows = memory_summary_rows(results)
    if not summary_rows:
        return []

    headers = ["bench", *(f"{compiler_id} peak" for compiler_id in ids)]
    if "solar" in ids and "solc" in ids:
        headers.append("Solar vs solc")
    if compared is not None:
        headers.append("Solar vs baseline")

    return [
        "<details>",
        "<summary>Peak RSS</summary>",
        "",
        "| compiler | benches | average peak RSS | maximum peak RSS | maximum bench |",
        "| -------- | ------- | ---------------- | ---------------- | ------------- |",
        *summary_rows,
        "",
        "#### Per-benchmark peak RSS",
        "",
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
        *memory_benchmark_rows(results, compared),
        "",
        "</details>",
        "",
    ]


def compile_time_rows(
    results: list[dict[str, Any]],
    baseline: dict[tuple[str, str], dict[str, Any]],
    compared: dict | None = None,
) -> list[str]:
    rows = []
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        solar_time = compile_time(result, "solar")
        base = baseline.get(suite_key(result), {})
        base_solar_time = compared_baseline(
            result, base, "compile_time_seconds", compared
        )
        rows.append(
            "| "
            + " | ".join(
                [
                    perf_link(markdown_cell(test_id), test_id),
                    (
                        f"{fmt_duration(solar_time)} "
                        f"({fmt_pct_change_lower_is_better(solar_time, base_solar_time)})"
                    ),
                    *(
                        f"{fmt_duration(value)} ({fmt_pct_vs_current(solar_time, value)})"
                        for name in reference_compiler_ids(results)
                        for value in [compile_time(result, name)]
                    ),
                ]
            )
            + " |"
        )
    return rows


def compile_time_report(
    results: list[dict[str, Any]],
    baseline: dict[tuple[str, str], dict[str, Any]],
    baseline_label: str,
    compared: dict | None = None,
) -> list[str]:
    # Aggregate only tests where all compilers succeeded, so a new failure
    # cannot make the Solar total look faster.
    ids = ["solar", *reference_compiler_ids(results)]
    paired = [[compile_time(result, name) for name in ids] for result in results]
    paired = [values for values in paired if all(value is not None for value in values)]
    if not any(compile_time(result, "solar") is not None for result in results):
        return []

    sums = [sum(values[index] for values in paired) for index in range(len(ids))]
    solar_sum = sums[0]
    reference_headers = " | ".join(ids[1:])

    return [
        "<details>",
        "<summary>Compilation time</summary>",
        "",
        f"| bench | time (vs {baseline_label}) | {reference_headers} |",
        "| ----- | --------------------- |" + " ---- |" * (len(ids) - 1),
        *compile_time_rows(results, baseline, compared),
        *(
            [
                (
                    f"| **sum of medians** | **{fmt_duration(solar_sum)}** | "
                    + " | ".join(
                        f"**{fmt_duration(value)} ({fmt_pct_vs_current(solar_sum, value)})**"
                        for value in sums[1:]
                    )
                    + " |"
                )
            ]
            if paired
            else []
        ),
        "",
        "</details>",
        "",
    ]


def report_section(
    title: str,
    results: list[dict[str, Any]],
    baseline_results: list[dict[str, Any]],
    baseline_ref: str = "main",
    compared: dict | None = None,
) -> str:
    lines = [f"## {perf_link(title)}", ""]
    if not results:
        lines.extend(["No benchmark results were produced.", ""])
        return "\n".join(lines)

    baseline = by_test_id(baseline_results)
    baseline_label = markdown_cell(baseline_ref)
    if not baseline:
        lines.extend(
            [f"No `{baseline_ref}` baseline artifact was available for comparison.", ""]
        )
    lines.extend(compilation_failure_report(results, baseline_results, baseline_ref))
    references = reference_compiler_ids(results)
    reference_headers = " | ".join(references)
    metric_header = f"| bench | gas (vs {baseline_label}) | {reference_headers} | size (vs {baseline_label}) | {reference_headers} |"
    metric_separator = (
        "| ----- | ------------- |"
        + " ---- |" * len(references)
        + " -------------- |"
        + " ---- |" * len(references)
    )

    rows = benchmark_rows(results, baseline, compared)
    if rows:
        lines.extend(
            [
                metric_header,
                metric_separator,
                *rows,
                "",
            ]
        )
    deployment = deployment_rows(results, baseline, compared)
    if deployment:
        lines.extend(
            [
                f"### {perf_link('Deployment')}",
                "",
                metric_header,
                metric_separator,
                *deployment,
                "",
            ]
        )
    lines.extend(compile_time_report(results, baseline, baseline_label, compared))
    lines.extend(memory_report(results, compared))
    return "\n".join(lines)


def codegen_report(
    results: list[dict[str, Any]],
    baseline_results: list[dict[str, Any]],
    baseline_ref: str = "main",
    compared: dict | None = None,
) -> str:
    return report_section(
        "Codegen benchmark", results, baseline_results, baseline_ref, compared
    )


def emit_warnings(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> None:
    for failure in compiler_failures(results):
        warning(f"compiler failure recorded: {failure}")
    for detail in runtime_issue_details(results):
        warning(f"runtime mismatch recorded: {detail}")
    for detail in baseline_regression_details(results, baseline_results):
        warning(f"benchmark regression recorded: {detail}")


def append_github_output(name: str, value: str) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        return
    delimiter = f"benchmark_{uuid.uuid4().hex}"
    with open(output_path, "a") as f:
        f.write(f"{name}<<{delimiter}\n{value}\n{delimiter}\n")


def branch_is_behind(base_ref: str = "main") -> bool:
    head_sha = os.environ.get("BENCHMARK_PR_HEAD_SHA")
    if not head_sha:
        return False
    try:
        count = subprocess.check_output(
            ["git", "rev-list", "--count", f"{head_sha}..origin/{base_ref}"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError):
        warning(f"could not determine whether the branch is behind {base_ref}")
        return False
    return int(count) > 0


def format_report(
    markdown: str,
    has_changes: bool,
    behind_base: bool,
    base_ref: str = "main",
    comparison: str = "",
) -> str:
    summary = comparison + "\n\n" if comparison else ""
    if has_changes and not behind_base:
        return summary + markdown
    notices = ""
    if behind_base:
        notices += (
            "> [!WARNING]\n"
            f"> This branch is behind `{base_ref}`, so these benchmark results may be incorrect.\n\n"
        )
    if not has_changes:
        notices += (
            f"> [!NOTE]\n> Codegen benchmark output is unchanged from `{base_ref}`.\n\n"
        )
    details = (
        "<details>\n"
        "<summary>Codegen benchmark output</summary>\n\n"
        f"{summary}{markdown}\n\n"
        "</details>\n"
    )
    return notices + details


def metric(value: float, unit: str, statistic: str) -> dict[str, Any]:
    return {"value": value, "unit": unit, "statistic": statistic}


def common_benchmark(
    name: str,
    results: list[dict[str, Any]],
    timing: float | None,
) -> dict[str, Any] | None:
    if not results or timing is None:
        return None

    successful = []
    failed = 0
    for result in results:
        compiler = compiler_data(result, "solar")
        if compiler.get("status") == "ok":
            successful.append(compiler)
        else:
            failed += 1

    benchmark = {
        "name": f"codegen_runtime_suite/{name}",
        "wall_time": metric(timing, "second", "total"),
        "counters": {
            "tests": metric(len(results), "count", "total"),
            "successful_compilations": metric(len(successful), "count", "total"),
            "failed_compilations": metric(failed, "count", "total"),
        },
    }

    def complete_values(key: str) -> list[int] | None:
        values = [compiler.get(key) for compiler in successful]
        if failed or not values or any(type(value) is not int for value in values):
            return None
        return values

    gas = {}
    total_gas_values = complete_values("total_gas")
    deploy_gas_values = complete_values("deploy_gas")
    if total_gas_values is not None:
        gas["runtime"] = metric(sum(total_gas_values), "gas", "total")
    if deploy_gas_values is not None:
        gas["deployment"] = metric(sum(deploy_gas_values), "gas", "total")
    if gas:
        benchmark["gas"] = gas

    compiler_metrics = {}
    creation_sizes = complete_values("bytecode_size")
    runtime_sizes = complete_values("runtime_size")
    if creation_sizes is not None:
        compiler_metrics["creation_bytecode_size"] = metric(
            sum(creation_sizes), "byte", "total"
        )
    if runtime_sizes is not None:
        compiler_metrics["runtime_bytecode_size"] = metric(
            sum(runtime_sizes), "byte", "total"
        )
    if compiler_metrics:
        benchmark["compiler"] = compiler_metrics
    peak_rss_values = complete_values("peak_rss_bytes")
    if peak_rss_values is not None:
        benchmark["memory"] = metric(max(peak_rss_values), "byte", "max")
    return benchmark


def git_commit() -> str:
    commit = os.environ.get("GITHUB_SHA")
    if commit:
        return commit
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL
    ).strip()


def runner_metadata() -> dict[str, Any]:
    runner = {
        "os": platform.system().lower(),
        "arch": platform.machine(),
        "logical_cpus": os.cpu_count() or 1,
    }
    image = os.environ.get("ImageOS")
    if image:
        runner["image"] = image
    return runner


def write_common_result(
    output: Path,
    results: list[dict[str, Any]],
    timings: dict[str, Any],
) -> None:
    timings = normalize_timings(timings)
    by_suite: dict[str, list[dict[str, Any]]] = {}
    for result in results:
        by_suite.setdefault(suite_name(result), []).append(result)
    benchmarks = [
        benchmark
        for suite, suite_results in by_suite.items()
        for benchmark in (common_benchmark(suite, suite_results, timings.get(suite)),)
        if benchmark is not None
    ]
    if not benchmarks:
        warning("common benchmark result has no measurements; not writing output")
        return

    result = {
        "schema_version": 1,
        "repo": os.environ.get("GITHUB_REPOSITORY", "paradigmxyz/solar"),
        "commit": git_commit(),
        "runner": runner_metadata(),
        "benchmarks": benchmarks,
    }
    pr = os.environ.get("BENCHMARK_PR_NUMBER")
    if pr:
        result["pr"] = int(pr)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w") as f:
        json.dump(result, f, indent=2)
        f.write("\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "runs",
        nargs="*",
        type=Path,
        metavar="RUN",
        help="Baseline and candidate JSON files or run directories",
    )
    parser.add_argument("--results", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--common-output", type=Path)
    parser.add_argument("--report-output", type=Path)
    parser.add_argument(
        "--json-output",
        type=Path,
        help="Write per-case deltas, eligibility, samples, and artifact hashes",
    )
    parser.add_argument(
        "--diff-output", type=Path, help="Write unified diffs of selected artifacts"
    )
    parser.add_argument("--baseline-artifacts", type=Path)
    parser.add_argument(
        "--artifacts",
        type=Path,
        help="Candidate artifact directory (default: artifacts beside results)",
    )
    parser.add_argument(
        "--artifact",
        nargs="+",
        choices=ARTIFACT_KINDS,
        default=list(ARTIFACT_KINDS),
        help="Artifact kinds to inspect or diff",
    )
    parser.add_argument("--tests", nargs="+", help="Select test IDs from either run")
    parser.add_argument(
        "--compiler", choices=("solar", "solc", "solx"), default="solar"
    )
    parser.add_argument(
        "--comment-output", type=Path, help="Write CI should-comment metadata"
    )
    parser.add_argument("--ignore-compile-time-changes", action="store_true")
    args = parser.parse_args(argv)
    if args.runs:
        if len(args.runs) != 2 or args.results is not None or args.baseline is not None:
            parser.error("provide two runs, or use --baseline and --results")
        args.baseline, args.results = args.runs
    if args.results is None:
        parser.error("provide baseline and candidate runs, or --results")
    if args.common_output and (args.tests or args.compiler != "solar"):
        parser.error("--common-output requires the complete run and --compiler solar")
    if args.compiler != "solar" and args.baseline is None:
        parser.error(f"--compiler {args.compiler} requires a baseline comparison")
    for name in ("baseline", "results"):
        path = getattr(args, name)
        if path is not None and path.is_dir():
            setattr(args, name, path / "results.json")
    if args.baseline is None and (args.diff_output or args.json_output):
        parser.error("comparison JSON and artifact diffs require a baseline")
    baseline_root = args.baseline_artifacts or (
        args.baseline.parent / "artifacts" if args.baseline else None
    )
    current_root = args.artifacts or args.results.parent / "artifacts"
    for output in (
        args.report_output,
        args.json_output,
        args.diff_output,
        args.common_output,
        args.comment_output,
    ):
        if output is not None and (
            output.resolve()
            in {
                path.resolve()
                for path in (args.baseline, args.results)
                if path is not None
            }
            or any(
                output.resolve().is_relative_to(root.resolve())
                for root in (baseline_root, current_root)
                if root is not None
            )
        ):
            parser.error(f"output would overwrite run inputs or artifacts: {output}")
    try:
        document = load_document(args.results, "benchmark")
        baseline_document = load_document(args.baseline, "baseline")
        by_test_id(document["results"])
        by_test_id(baseline_document["results"])
    except (OSError, ValueError) as error:
        parser.error(str(error))
    results = document["results"]
    baseline_results = baseline_document["results"]
    if args.tests:
        selected = set(args.tests)
        missing = selected - {
            row.get("test_id") for row in [*results, *baseline_results]
        }
        if missing:
            parser.error(f"unknown test IDs: {', '.join(sorted(missing))}")
        results = [row for row in results if row.get("test_id") in selected]
        baseline_results = [
            row for row in baseline_results if row.get("test_id") in selected
        ]
    base_ref = os.environ.get("BENCHMARK_BASE_REF") or (
        args.baseline.parent.name
        if args.baseline and args.baseline.name == "results.json"
        else args.baseline.stem
        if args.baseline
        else "main"
    )

    comparison = compare_runs(results, baseline_results, args.compiler)
    comparison["baseline"] = str(args.baseline) if args.baseline else None
    comparison["candidate"] = str(args.results)
    if (
        args.baseline
        and args.baseline.resolve() != args.results.resolve()
        and baseline_root.resolve() == current_root.resolve()
    ):
        comparison["artifact_warning"] = (
            "Both runs use the same artifact directory; supply separate --baseline-artifacts and --artifacts paths to compare independent captures."
        )
    try:
        compare_artifacts(
            comparison, baseline_root, current_root, args.artifact, args.diff_output
        )
    except (OSError, ValueError) as error:
        parser.error(str(error))
    eligible_rows = {(row["suite"], row["test_id"]): row for row in comparison["rows"]}
    emit_warnings(results, [])
    for row in comparison["rows"]:
        for name in ("total_gas", "runtime_size", "bytecode_size", "deploy_gas"):
            values = row["metrics"][name]
            if values["delta"] is not None and values["delta"] > 0:
                warning(
                    f"{row['test_id']} {row['compiler']} {METRICS[name]} regressed: {values['before']} -> {values['after']}"
                )
    report = (
        codegen_report(results, baseline_results, base_ref, eligible_rows)
        if args.compiler == "solar"
        else "## Codegen benchmark\n"
    )
    should_comment = not baseline_results or comparison_has_changes(
        comparison, args.ignore_compile_time_changes
    )
    markdown = format_report(
        report,
        should_comment,
        branch_is_behind(base_ref),
        base_ref,
        comparison_report(comparison) if args.baseline is not None else "",
    )
    print(markdown)
    append_github_output("report", markdown)
    append_github_output("should_comment", "true" if should_comment else "false")
    if args.report_output is not None:
        args.report_output.parent.mkdir(parents=True, exist_ok=True)
        args.report_output.write_text(markdown)
    if args.json_output is not None:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(
            json.dumps(comparison, indent=2, allow_nan=False) + "\n"
        )
    if args.comment_output is not None:
        args.comment_output.parent.mkdir(parents=True, exist_ok=True)
        args.comment_output.write_text("true\n" if should_comment else "false\n")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as stream:
            stream.write(markdown + "\n")
    if args.common_output is not None:
        write_common_result(args.common_output, results, document["timings"])
    return 0 if results and (args.baseline is None or args.baseline.is_file()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
