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


def baseline_regression_details(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> list[str]:
    details = []
    baseline = by_test_id(baseline_results)
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        base = baseline.get(test_id)
        if base is None:
            continue

        solar_gas = total_gas(result, "solar")
        base_solar_gas = total_gas(base, "solar")
        if solar_gas is not None and base_solar_gas is not None and solar_gas > base_solar_gas:
            details.append(
                f"{test_id} solar gas regressed vs previous Solar run: "
                f"{base_solar_gas:,} -> {solar_gas:,} "
                f"({absolute_delta(solar_gas, base_solar_gas)}, "
                f"{pct_increase(solar_gas, base_solar_gas)} worse)"
            )

        solar_size = runtime_size(result, "solar")
        base_solar_size = runtime_size(base, "solar")
        if solar_size is not None and base_solar_size is not None and solar_size > base_solar_size:
            details.append(
                f"{test_id} solar runtime size regressed vs previous Solar run: "
                f"{base_solar_size:,}B -> {solar_size:,}B "
                f"({absolute_delta(solar_size, base_solar_size)}B, "
                f"{pct_increase(solar_size, base_solar_size)} worse)"
            )

    return details


def has_baseline_changes(
    results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> bool:
    baseline = by_test_id(baseline_results)
    for result in results:
        test_id = str(result.get("test_id", "<unknown>"))
        base = baseline.get(test_id)
        if base is None:
            continue

        solar_gas = total_gas(result, "solar")
        base_solar_gas = total_gas(base, "solar")
        if solar_gas is not None and base_solar_gas is not None and solar_gas != base_solar_gas:
            return True

        solar_size = runtime_size(result, "solar")
        base_solar_size = runtime_size(base, "solar")
        if solar_size is not None and base_solar_size is not None and solar_size != base_solar_size:
            return True

    return False


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


def pct_improvement(current: int | None, baseline: int | None) -> float | None:
    if current is None or baseline in (None, 0):
        return None
    return (baseline - current) / baseline * 100


def fmt_pct_improvement(current: int | None, baseline: int | None) -> str:
    delta = pct_improvement(current, baseline)
    if delta is None:
        return "n/a"
    return fmt_pct(delta)


def pct_vs_current(current: int | None, comparison: int | None) -> float | None:
    if current in (None, 0) or comparison is None:
        return None
    return (comparison - current) / current * 100


def fmt_pct_vs_current(current: int | None, comparison: int | None) -> str:
    delta = pct_vs_current(current, comparison)
    if delta is None:
        return "n/a"
    return fmt_pct(delta)


def fmt_pct(delta: float) -> str:
    rounded = round(delta, 2)
    if rounded == 0:
        return "~0%"
    emoji = "✅" if rounded > 0 else "❌"
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


def fmt_value_with_delta(
    value: int | None, current: int | None, baseline: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_improvement(current, baseline)})"


def fmt_value_with_delta_vs_current(
    value: int | None, current: int | None, comparison: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_vs_current(current, comparison)})"


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
                    fmt_value_with_delta(solar_gas, solar_gas, base_solar_gas),
                    fmt_value_with_delta_vs_current(solc_gas, solar_gas, solc_gas),
                    fmt_value_with_delta(solar_size, solar_size, base_solar_size, "B"),
                    fmt_value_with_delta_vs_current(solc_size, solar_size, solc_size, "B"),
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
    if not baseline:
        lines.extend(["No `main` baseline artifact was available for comparison.", ""])

    lines.extend(
        [
            "| bench | gas (vs main) | solc | size (vs main) | solc |",
            "| ----- | ------------- | ---- | -------------- | ---- |",
            *benchmark_rows(results, baseline),
            "",
        ]
    )
    return "\n".join(lines)


def emit_warnings(
    label: str, results: list[dict[str, Any]], baseline_results: list[dict[str, Any]]
) -> None:
    for failure in compiler_failures(results):
        warning(f"{label} compiler failure recorded: {failure}")
    for detail in runtime_issue_details(results):
        warning(f"{label} runtime mismatch recorded: {detail}")
    for detail in baseline_regression_details(results, baseline_results):
        warning(f"{label} benchmark regression recorded: {detail}")


def append_step_summary(markdown: str) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return
    with open(summary_path, "a") as f:
        f.write(markdown)
        f.write("\n")


def append_github_output(name: str, value: str) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        return
    with open(output_path, "a") as f:
        f.write(f"{name}={value}\n")


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

    emit_warnings("micro", micro_results, baseline_micro)
    emit_warnings("repository", repo_results, baseline_repo)

    sections = []
    if args.micro is not None:
        sections.append(
            report_section("Micro codegen benchmark", micro_results, baseline_micro)
        )
    if args.repo is not None:
        sections.append(
            report_section("Repository codegen benchmark", repo_results, baseline_repo)
        )
    if not sections:
        sections.append("## Codegen benchmark\n\nNo benchmark inputs were configured.\n")

    markdown = "\n".join(sections)
    print(markdown)
    append_step_summary(markdown)
    should_comment = has_baseline_changes(
        micro_results, baseline_micro
    ) or has_baseline_changes(repo_results, baseline_repo)
    append_github_output("should_comment", "true" if should_comment else "false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
