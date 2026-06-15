#!/usr/bin/env python3
"""Report Solar codegen benchmark JSON emitted by solar_bench.py.

This script is intentionally non-gating: runtime benchmarks are useful CI
signals, but benchmark deltas should be reviewed rather than fail PRs.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any


def load_results(path: Path | None, label: str) -> list[dict[str, Any]]:
    if path is None:
        return []
    if not path.exists():
        warning(f"{label} benchmark results not found: {path}")
        return []
    with path.open() as f:
        data = json.load(f)
    if not isinstance(data, list):
        warning(f"{label} benchmark results have unexpected shape: expected list")
        return []
    return data


def by_test_id(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {str(result.get("test_id", "<unknown>")): result for result in results}


def compiler_failures(results: list[dict[str, Any]]) -> list[str]:
    failures = []
    for result in results:
        test_id = result.get("test_id", "<unknown>")
        compilers = result.get("compilers", {})
        for compiler_id, data in compilers.items():
            if data.get("status") != "ok":
                error = str(data.get("error") or "").splitlines()[0]
                failures.append(f"{test_id} {compiler_id}: {error}")
    return failures


def shorten(value: Any, limit: int = 160) -> str:
    text = str(value).replace("\n", " ")
    if len(text) <= limit:
        return text
    return text[: limit - 1] + "..."


def format_values(values: dict[str, Any]) -> str:
    return ", ".join(f"{compiler}={shorten(value)}" for compiler, value in values.items())


def runtime_issue_details(results: list[dict[str, Any]]) -> list[str]:
    details = []
    for result in results:
        status = result.get("runtime_status")
        if status in (None, "skipped", "ok"):
            continue
        test_id = result.get("test_id", "<unknown>")
        before = len(details)

        for mismatch in result.get("runtime_mismatches") or []:
            label = mismatch.get("label", "<unknown>")
            values = mismatch.get("values") or {}
            details.append(f"{test_id} {label}: {format_values(values)}")

        for compiler_id, data in (result.get("compilers") or {}).items():
            for check in data.get("runtime_results") or []:
                if check.get("status") == "ok":
                    continue
                label = check.get("label", "<unknown>")
                error = check.get("error") or check.get("status")
                details.append(f"{test_id} {compiler_id} {label}: {shorten(error)}")

        if len(details) == before:
            details.append(f"{test_id}: runtime_status={status}")

    return details


def warning(message: str) -> None:
    escaped = message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::warning::{escaped}", file=sys.stderr)


def markdown_cell(value: Any) -> str:
    return str(value).replace("|", "\\|").replace("\n", "<br>")


def compiler_data(result: dict[str, Any], compiler: str) -> dict[str, Any]:
    data = result.get("compilers") or {}
    value = data.get(compiler)
    return value if isinstance(value, dict) else {}


def total_gas(result: dict[str, Any], compiler: str) -> int | None:
    value = compiler_data(result, compiler).get("total_gas")
    return value if isinstance(value, int) else None


def runtime_size(result: dict[str, Any], compiler: str) -> int | None:
    value = compiler_data(result, compiler).get("runtime_size")
    return value if isinstance(value, int) else None


def fmt_int(value: int | None, suffix: str = "") -> str:
    if value is None:
        return "n/a"
    return f"{value:,}{suffix}"


def pct_delta(current: int | None, baseline: int | None) -> str:
    if current is None or baseline in (None, 0):
        return "n/a"
    delta = (baseline - current) / baseline * 100
    return f"{delta:+.2f}%"


def absolute_delta(current: int | None, baseline: int | None) -> str:
    if current is None or baseline is None:
        return "n/a"
    delta = current - baseline
    return f"{delta:+,}"


def runtime_summary(result: dict[str, Any]) -> str:
    status = result.get("runtime_status")
    if status in (None, "skipped", "ok"):
        return str(status or "not run")

    parts = []
    for mismatch in result.get("runtime_mismatches") or []:
        label = mismatch.get("label", "<unknown>")
        values = mismatch.get("values") or {}
        parts.append(f"{label}: {format_values(values)}")

    for compiler_id, data in (result.get("compilers") or {}).items():
        for check in data.get("runtime_results") or []:
            if check.get("status") == "ok":
                continue
            label = check.get("label", "<unknown>")
            error = check.get("error") or check.get("status")
            parts.append(f"{compiler_id} {label}: {shorten(error)}")

    return "<br>".join(markdown_cell(part) for part in parts) or str(status)


def benchmark_rows(
    results: list[dict[str, Any]], baseline: dict[str, dict[str, Any]]
) -> list[str]:
    rows = []
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        base = baseline.get(test_id, {})
        solar_gas = total_gas(result, "solar")
        solc_gas = total_gas(result, "solc")
        base_solar_gas = total_gas(base, "solar") if base else None
        solar_size = runtime_size(result, "solar")
        solc_size = runtime_size(result, "solc")
        base_solar_size = runtime_size(base, "solar") if base else None

        rows.append(
            "| "
            + " | ".join(
                [
                    markdown_cell(test_id),
                    markdown_cell(runtime_summary(result)),
                    fmt_int(solar_gas),
                    pct_delta(solar_gas, solc_gas),
                    absolute_delta(solar_gas, base_solar_gas),
                    pct_delta(solar_gas, base_solar_gas),
                    fmt_int(solar_size, "B"),
                    pct_delta(solar_size, solc_size),
                    absolute_delta(solar_size, base_solar_size),
                    pct_delta(solar_size, base_solar_size),
                ]
            )
            + " |"
        )
    return rows


def report_section(
    title: str,
    results: list[dict[str, Any]],
    baseline_results: list[dict[str, Any]],
) -> str:
    lines = [f"## {title}", ""]
    if not results:
        lines.extend(["No benchmark results were produced.", ""])
        return "\n".join(lines)

    baseline = by_test_id(baseline_results)
    if baseline:
        lines.extend(
            [
                "Gas and size deltas are informational. Positive percentages mean "
                "Solar used less gas or smaller runtime bytecode.",
                "",
            ]
        )
    else:
        lines.extend(["No `main` baseline artifact was available for comparison.", ""])

    lines.extend(
        [
            "| Test | Runtime | Solar gas | Solar vs solc gas | Gas Δ vs main | Gas % vs main | Solar size | Solar vs solc size | Size Δ vs main | Size % vs main |",
            "| ---- | ------- | --------- | ----------------- | ------------- | ------------- | ---------- | ------------------ | -------------- | -------------- |",
            *benchmark_rows(results, baseline),
            "",
        ]
    )
    return "\n".join(lines)


def emit_warnings(label: str, results: list[dict[str, Any]]) -> None:
    for failure in compiler_failures(results):
        warning(f"{label} compiler failure recorded: {failure}")
    for detail in runtime_issue_details(results):
        warning(f"{label} runtime mismatch recorded: {detail}")


def append_step_summary(markdown: str) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return
    with open(summary_path, "a") as f:
        f.write(markdown)
        f.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--micro", type=Path)
    parser.add_argument("--repo", type=Path)
    parser.add_argument("--baseline-micro", type=Path)
    parser.add_argument("--baseline-repo", type=Path)
    args = parser.parse_args()

    micro_results = load_results(args.micro, "micro")
    repo_results = load_results(args.repo, "repository")
    baseline_micro = load_results(args.baseline_micro, "baseline micro")
    baseline_repo = load_results(args.baseline_repo, "baseline repository")

    emit_warnings("micro", micro_results)
    emit_warnings("repository", repo_results)

    sections = []
    if args.micro is not None:
        sections.append(
            report_section("Solar micro codegen benchmark", micro_results, baseline_micro)
        )
    if args.repo is not None:
        sections.append(
            report_section("Solar repository codegen benchmark", repo_results, baseline_repo)
        )
    if not sections:
        sections.append("## Solar codegen benchmark\n\nNo benchmark inputs were configured.\n")

    markdown = "\n".join(sections)
    print(markdown)
    append_step_summary(markdown)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
