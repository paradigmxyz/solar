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


def shorten(value: Any, limit: int = 160) -> str:
    text = str(value).replace("\n", " ")
    if len(text) <= limit:
        return text
    return text[: limit - 1] + "…"


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
    print(f"::warning::{escaped}")


def markdown_cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", "<br>")


def runtime_summary(result: dict[str, Any]) -> str:
    status = result.get("runtime_status")
    if status in (None, "skipped", "ok"):
        return ""

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


def append_summary(title: str, results: list[dict[str, Any]]) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return

    lines = [
        f"### {title}",
        "",
        "| Test | Compile | Runtime | Details |",
        "| ---- | ------- | ------- | ------- |",
    ]
    for result in results:
        compilers = result.get("compilers", {})
        compile_ok = all(data.get("status") == "ok" for data in compilers.values())
        runtime_status = result.get("runtime_status") or "not run"
        lines.append(
            f"| {result.get('test_id', '<unknown>')} | "
            f"{'ok' if compile_ok else 'failed'} | {runtime_status} | "
            f"{runtime_summary(result)} |"
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
    failures.extend(f"micro {failure}" for failure in runtime_issue_details(micro_results))
    failures.extend(f"repo {failure}" for failure in compiler_failures(repo_results))

    repo_runtime = runtime_issue_details(repo_results)
    for detail in repo_runtime:
        warning(f"repository runtime mismatch recorded: {detail}")
    failures.extend(f"repo {failure}" for failure in repo_runtime)

    if failures:
        print("codegen benchmark failures:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("codegen benchmark checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
