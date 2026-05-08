#!/usr/bin/env python3
"""Validate Solar codegen benchmark JSON emitted by solar_bench.py."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any


def load_results(path: Path) -> list[dict[str, Any]]:
    with path.open() as f:
        data = json.load(f)
    if not isinstance(data, list):
        raise SystemExit(f"{path}: expected a list of benchmark results")
    return data


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


def runtime_failures(results: list[dict[str, Any]]) -> list[str]:
    failures = []
    for result in results:
        status = result.get("runtime_status")
        if status in (None, "skipped", "ok"):
            continue
        failures.append(f"{result.get('test_id', '<unknown>')}: runtime_status={status}")
    return failures


def warning(message: str) -> None:
    print(f"::warning::{message}")


def append_summary(title: str, results: list[dict[str, Any]]) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return

    lines = [
        f"### {title}",
        "",
        "| Test | Compile | Runtime |",
        "| ---- | ------- | ------- |",
    ]
    for result in results:
        compilers = result.get("compilers", {})
        compile_ok = all(data.get("status") == "ok" for data in compilers.values())
        runtime_status = result.get("runtime_status") or "not run"
        lines.append(
            f"| {result.get('test_id', '<unknown>')} | "
            f"{'ok' if compile_ok else 'failed'} | {runtime_status} |"
        )
    lines.append("")

    with open(summary_path, "a") as f:
        f.write("\n".join(lines))
        f.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--micro", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    args = parser.parse_args()

    micro_results = load_results(args.micro)
    repo_results = load_results(args.repo)

    append_summary("Solar micro codegen benchmark", micro_results)
    append_summary("Solar repository codegen benchmark", repo_results)

    failures = []
    failures.extend(f"micro {failure}" for failure in compiler_failures(micro_results))
    failures.extend(f"micro {failure}" for failure in runtime_failures(micro_results))
    failures.extend(f"repo {failure}" for failure in compiler_failures(repo_results))

    repo_runtime = runtime_failures(repo_results)
    for failure in repo_runtime:
        warning(f"repository runtime mismatch recorded: {failure}")

    if failures:
        print("codegen benchmark failures:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("codegen benchmark checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
