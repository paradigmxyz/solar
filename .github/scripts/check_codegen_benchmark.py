#!/usr/bin/env python3
"""Report Solar codegen benchmark JSON emitted by solar_bench.py.

This script is intentionally non-gating: runtime benchmarks are useful CI
signals, but benchmark deltas should be reviewed rather than fail PRs.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
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


def pct_change(current: int | None, baseline: int | None) -> float | None:
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


def fmt_value_with_delta(
    value: int | None, current: int | None, baseline: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_improvement(current, baseline)})"


def fmt_value_with_size_delta(
    value: int | None, current: int | None, baseline: int | None, suffix: str = ""
) -> str:
    return f"{fmt_int(value, suffix)} ({fmt_pct_change_lower_is_better(current, baseline)})"


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
                    fmt_value_with_size_delta(solar_size, solar_size, base_solar_size, "B"),
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


def format_report(markdown: str, has_changes: bool) -> str:
    if has_changes:
        return markdown
    return (
        "> [!NOTE]\n"
        "> Codegen benchmark output is unchanged from `main`.\n\n"
        f"{markdown}"
    )


def metric(value: int | float, unit: str, statistic: str) -> dict[str, Any]:
    return {"value": value, "unit": unit, "statistic": statistic}


def load_timing(path: Path | None, label: str) -> dict[str, Any] | None:
    if path is None or not path.exists():
        warning(f"{label} benchmark timing not found: {path}")
        return None
    with path.open() as f:
        timing = json.load(f)
    if not isinstance(timing, dict):
        warning(f"{label} benchmark timing has unexpected shape: expected object")
        return None
    return timing


def common_benchmark(
    name: str, results: list[dict[str, Any]], timing: dict[str, Any] | None
) -> dict[str, Any] | None:
    if not results or timing is None:
        return None

    wall_time = timing.get("wall_time_seconds")
    if not isinstance(wall_time, (int, float)):
        warning(f"{name} benchmark timing is missing wall time")
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
        "wall_time": metric(wall_time, "second", "total"),
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
    micro_results: list[dict[str, Any]],
    repo_results: list[dict[str, Any]],
    micro_timing: dict[str, Any] | None,
    repo_timing: dict[str, Any] | None,
) -> None:
    benchmarks = [
        benchmark
        for benchmark in (
            common_benchmark("micro", micro_results, micro_timing),
            common_benchmark("repo", repo_results, repo_timing),
        )
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--micro", type=Path)
    parser.add_argument("--repo", type=Path)
    parser.add_argument("--baseline-micro", type=Path)
    parser.add_argument("--baseline-repo", type=Path)
    parser.add_argument("--micro-timing", type=Path)
    parser.add_argument("--repo-timing", type=Path)
    parser.add_argument("--common-output", type=Path)
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

    should_comment = has_baseline_changes(
        micro_results, baseline_micro
    ) or has_baseline_changes(repo_results, baseline_repo)
    markdown = format_report("\n".join(sections), should_comment)
    print(markdown)
    append_step_summary(markdown)
    append_github_output("should_comment", "true" if should_comment else "false")
    if args.common_output is not None:
        write_common_result(
            args.common_output,
            micro_results,
            repo_results,
            load_timing(args.micro_timing, "micro"),
            load_timing(args.repo_timing, "repository"),
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
