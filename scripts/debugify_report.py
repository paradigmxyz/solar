#!/usr/bin/env python3
"""Summarize per-pass source-origin audits from the UI suite."""

import argparse
import json
from collections import defaultdict
from pathlib import Path


def records(path: Path):
    with path.open() as stream:
        header = json.loads(next(stream, "null"))
        if not isinstance(header, dict) or (
            header.get("kind"),
            header.get("version"),
        ) != ("invocation", 1):
            raise ValueError(f"{path}: missing version-1 invocation header")
        count = 0
        for line_number, line in enumerate(stream, start=2):
            try:
                record = json.loads(line)
                if not isinstance(record, dict) or record.get("kind") != "pass":
                    raise ValueError("expected a pass record")
                if record.get("ir") not in ("MIR", "EVM IR"):
                    raise ValueError("unknown IR layer")
                for field in ("pass", "module"):
                    if not isinstance(record.get(field), str) or not record[field]:
                        raise ValueError(f"missing {field}")
                for field in ("before", "retained"):
                    if type(record.get(field)) is not int or record[field] < 0:
                        raise ValueError(f"invalid {field} count")
                lost = record.get("lost")
                if not isinstance(lost, list) or any(
                    not isinstance(span, list)
                    or len(span) != 2
                    or any(type(value) is not int for value in span)
                    or not 0 <= span[0] <= span[1]
                    for span in lost
                ):
                    raise ValueError("invalid lost spans")
                if record["before"] != record["retained"] + len(lost):
                    raise ValueError("inconsistent origin counts")
            except (ValueError, TypeError) as error:
                raise ValueError(f"{path}:{line_number}: {error}") from error
            count += 1
            yield record
        if not count:
            raise ValueError(f"{path}: invocation has no pass records")


def summarize(root: Path) -> dict:
    passes = defaultdict(lambda: {"runs": 0, "losses": 0, "lost_origins": 0})
    tests = defaultdict(lambda: {"runs": 0, "losses": 0, "lost_origins": 0})
    files = sorted(root.rglob("*.jsonl"))
    if not files:
        raise ValueError(f"no debugify audits found in {root}")
    for path in files:
        test = path.parent.relative_to(root).as_posix()
        for record in records(path):
            key = (record["ir"], record["pass"])
            for counts in (passes[key], tests[test]):
                counts["runs"] += 1
                counts["losses"] += bool(record["lost"])
                counts["lost_origins"] += len(record["lost"])
    return {
        "version": 1,
        "invocations": len(files),
        "audited_tests": len(tests),
        "pass_runs": sum(row["runs"] for row in passes.values()),
        "losses": sum(row["losses"] for row in passes.values()),
        "lost_origins": sum(row["lost_origins"] for row in passes.values()),
        "by_pass": [
            {"ir": ir, "pass": name, **row}
            for (ir, name), row in sorted(
                passes.items(), key=lambda item: (-item[1]["lost_origins"], item[0])
            )
        ],
        "by_test": [
            {"test": name, **row}
            for name, row in sorted(
                tests.items(), key=lambda item: (-item[1]["lost_origins"], item[0])
            )
        ],
    }


def cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def markdown(summary: dict) -> str:
    lines = [
        "## Debugify UI audit",
        "",
        (
            f"Audited {summary['audited_tests']:,} test/revision pairs, "
            f"{summary['invocations']:,} compiler invocations and "
            f"{summary['pass_runs']:,} pass runs."
        ),
        (
            f"{summary['lost_origins']:,} origins were lost across "
            f"{summary['losses']:,} pass runs."
        ),
        "",
        (
            "Losses are investigation candidates: deleting dead code can legitimately "
            "remove its last origin. Frontend-only and rejected inputs never reach IR passes. "
            "The test log includes those tests; counts above cover audited compilations only."
        ),
        "",
        (
            "Strict preservation tests still fail CI. The artifact contains the full JSON "
            "summary and raw per-invocation reports, including arguments, source-file "
            "bounds and lost spans."
        ),
        "",
        "### Passes with the most losses",
        "",
        "| Layer | Pass | Runs with losses | Lost origins |",
        "| --- | --- | ---: | ---: |",
    ]
    for row in summary["by_pass"][:25]:
        if row["lost_origins"]:
            lines.append(
                f"| {cell(row['ir'])} | {cell(row['pass'])} | "
                f"{row['losses']:,} | {row['lost_origins']:,} |"
            )
    lines.extend(
        [
            "",
            "### Tests with the most losses",
            "",
            "| Test / revision | Runs with losses | Lost origins |",
            "| --- | ---: | ---: |",
        ]
    )
    for row in summary["by_test"][:25]:
        if row["lost_origins"]:
            lines.append(
                f"| {cell(row['test'])} | {row['losses']:,} | {row['lost_origins']:,} |"
            )
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        summary = summarize(args.input)
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        report = markdown(summary)
        (args.output / "report.md").write_text(report)
    except (ValueError, OSError) as error:
        parser.exit(1, f"debugify: {error}\n")
    print(report)


if __name__ == "__main__":
    main()
