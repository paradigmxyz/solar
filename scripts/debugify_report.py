#!/usr/bin/env python3
"""Summarize per-pass source-origin audits from the UI suite, optionally against a baseline."""

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


def load_baseline(path: Path) -> dict:
    try:
        baseline = json.loads(path.read_text())
        if baseline["version"] != 1:
            raise ValueError("unsupported summary version")
        for key in ("lost_origins", "losses", "pass_runs", "audited_tests"):
            if type(baseline[key]) is not int:
                raise ValueError(f"invalid {key}")
        for field, rows in (("by_pass", ("ir", "pass")), ("by_test", ("test",))):
            for row in baseline[field]:
                for key in (*rows, "losses", "lost_origins"):
                    row[key]
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{path}: invalid baseline summary: {error!r}") from error
    return baseline


def delta(base: int, head: int) -> str:
    """Lower is better, so fewer lost origins is an improvement."""
    if base == head:
        return "~0%"
    sign = "+" if head > base else "-"
    change = f"{sign}{abs(head - base):,}"
    if base:
        change += f", {sign}{abs(head - base) / base * 100:.2f}%"
    return f"{'❌' if head > base else '✅'} {change}"


def change(base: int, head: int) -> str:
    return f"{head:,}" if base == head else f"{base:,} → {head:,} ({delta(base, head)})"


def compare(
    rows: list[dict], baseline: list[dict], key
) -> list[tuple[dict, dict, dict]]:
    """Pair rows by key, ordering by the size of the change in lost origins."""
    empty = {"losses": 0, "lost_origins": 0}
    before = {key(row): row for row in baseline}
    after = {key(row): row for row in rows}
    changed = [
        (
            after.get(name, empty),
            before.get(name, empty),
            after.get(name) or before[name],
        )
        for name in sorted(before.keys() | after.keys())
        if after.get(name, empty)["lost_origins"]
        != before.get(name, empty)["lost_origins"]
    ]
    return sorted(
        changed,
        key=lambda item: -abs(item[0]["lost_origins"] - item[1]["lost_origins"]),
    )


def has_changes(summary: dict, baseline: dict) -> bool:
    return bool(
        compare(
            summary["by_pass"],
            baseline["by_pass"],
            lambda row: (row["ir"], row["pass"]),
        )
        or compare(summary["by_test"], baseline["by_test"], lambda row: row["test"])
    )


def markdown(summary: dict, baseline: dict | None = None, base_ref: str = "") -> str:
    if baseline:
        intro = f"Source-origin loss changes against `{base_ref}`."
        overview = [
            "| Metric | Base | PR | Change |",
            "| --- | ---: | ---: | ---: |",
        ]
        for label, key in (
            ("lost origins", "lost_origins"),
            ("pass runs with losses", "losses"),
        ):
            overview.append(
                f"| {label} | {baseline[key]:,} | {summary[key]:,} | "
                f"{delta(baseline[key], summary[key])} |"
            )
        for label, key in (
            ("pass runs", "pass_runs"),
            ("audited tests", "audited_tests"),
        ):
            overview.append(f"| {label} | {baseline[key]:,} | {summary[key]:,} | |")
    else:
        intro = "Source-origin losses; no baseline audit was found for the base commit."
        overview = [
            "| Metric | PR |",
            "| --- | ---: |",
            f"| lost origins | {summary['lost_origins']:,} |",
            f"| pass runs with losses | {summary['losses']:,} |",
            f"| pass runs | {summary['pass_runs']:,} |",
            f"| audited tests | {summary['audited_tests']:,} |",
        ]
    lines = [
        "## Debug info audit",
        "",
        intro,
        "",
        "### Overview",
        "",
        *overview,
        "",
        (
            f"Audited {summary['audited_tests']:,} test/revision pairs, "
            f"{summary['invocations']:,} compiler invocations and "
            f"{summary['pass_runs']:,} pass runs; lower is better."
        ),
    ]
    if baseline:
        passes = compare(
            summary["by_pass"],
            baseline["by_pass"],
            lambda row: (row["ir"], row["pass"]),
        )
        tests = compare(
            summary["by_test"], baseline["by_test"], lambda row: row["test"]
        )
        if passes:
            lines.extend(
                [
                    "",
                    f"### Changed passes vs `{base_ref}`",
                    "",
                    "| Layer | Pass | Runs with losses | Lost origins |",
                    "| --- | --- | ---: | ---: |",
                ]
            )
            for head, base, row in passes[:25]:
                lines.append(
                    f"| {cell(row['ir'])} | {cell(row['pass'])} | "
                    f"{change(base['losses'], head['losses'])} | "
                    f"{change(base['lost_origins'], head['lost_origins'])} |"
                )
        if tests:
            lines.extend(
                [
                    "",
                    f"### Changed tests vs `{base_ref}`",
                    "",
                    "| Test / revision | Runs with losses | Lost origins |",
                    "| --- | ---: | ---: |",
                ]
            )
            for head, base, row in tests[:25]:
                lines.append(
                    f"| {cell(row['test'])} | "
                    f"{change(base['losses'], head['losses'])} | "
                    f"{change(base['lost_origins'], head['lost_origins'])} |"
                )
        if not passes and not tests:
            lines.extend(
                ["", f"No pass or test changed its lost origins vs `{base_ref}`."]
            )
    lines.extend(
        [
            "",
            "<details>",
            "<summary>Passes with the most losses</summary>",
            "",
            "| Layer | Pass | Runs with losses | Lost origins |",
            "| --- | --- | ---: | ---: |",
        ]
    )
    for row in summary["by_pass"][:25]:
        if row["lost_origins"]:
            lines.append(
                f"| {cell(row['ir'])} | {cell(row['pass'])} | "
                f"{row['losses']:,} | {row['lost_origins']:,} |"
            )
    lines.extend(
        [
            "",
            "</details>",
            "",
            "<details>",
            "<summary>Tests with the most losses</summary>",
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
    lines.extend(
        [
            "",
            "</details>",
            "",
            (
                "Losses are investigation candidates: deleting dead code can legitimately "
                "remove its last origin. Frontend-only and rejected inputs never reach IR "
                "passes. Strict preservation tests still fail. The output directory contains "
                "the full JSON summary and raw per-invocation reports."
            ),
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--baseline", type=Path, help="summary.json from the base commit"
    )
    parser.add_argument("--base-ref", default="", help="label for the base commit")
    args = parser.parse_args()
    try:
        summary = summarize(args.input)
        baseline = load_baseline(args.baseline) if args.baseline else None
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        report = markdown(summary, baseline, args.base_ref)
        (args.output / "report.md").write_text(report)
        changed = baseline is not None and has_changes(summary, baseline)
        (args.output / "should-comment").write_text(f"{str(changed).lower()}\n")
    except (ValueError, OSError) as error:
        parser.exit(1, f"debugify: {error}\n")
    print(report)


if __name__ == "__main__":
    main()
