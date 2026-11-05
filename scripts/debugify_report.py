#!/usr/bin/env python3
"""Summarize per-pass debug-info audits from the UI suite, optionally against a baseline.

The headline is LLVM's original-mode `not-generate` count: instructions and terminators with no
source origin and no reason for lacking one ("gaps"). Lost origins, the analogue of LLVM's
synthetic-mode `Missing line` warning, are reported as information.
"""

import argparse
import json
from collections import Counter, defaultdict
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
                if not isinstance(record, dict) or record.get("kind") not in (
                    "pass",
                    "gaps",
                ):
                    raise ValueError("expected a pass or gaps record")
                if record.get("ir") not in ("MIR", "EVM IR"):
                    raise ValueError("unknown IR layer")
                for field in ("pass", "module"):
                    if not isinstance(record.get(field), str) or not record[field]:
                        raise ValueError(f"missing {field}")
                if record["kind"] == "gaps":
                    check_gaps(record)
                    count += 1
                    yield record
                    continue
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
            raise ValueError(f"{path}: invocation has no audit records")


def check_gaps(record: dict) -> None:
    """The count and the opcode histogram are exact; the listed sites may be capped."""
    count = record.get("count")
    if type(count) is not int or count <= 0:
        raise ValueError("invalid gap count")
    opcodes = record.get("opcodes")
    if (
        not isinstance(opcodes, dict)
        or any(type(value) is not int or value <= 0 for value in opcodes.values())
        or sum(opcodes.values()) != count
    ):
        raise ValueError("inconsistent gap opcodes")
    sites = record.get("sites")
    if not isinstance(sites, list) or record.get("omitted") != count - len(sites):
        raise ValueError("inconsistent gap sites")


def row() -> dict:
    return {
        "runs": 0,
        "losses": 0,
        "lost_origins": 0,
        "gaps": 0,
        "gap_runs": 0,
    }


def summarize(root: Path) -> dict:
    passes = defaultdict(row)
    opcodes = defaultdict(Counter)
    tests = defaultdict(row)
    files = sorted(root.rglob("*.jsonl"))
    if not files:
        raise ValueError(f"no debugify audits found in {root}")
    for path in files:
        test = path.parent.relative_to(root).as_posix()
        for record in records(path):
            key = (record["ir"], record["pass"])
            if record["kind"] == "gaps":
                opcodes[key].update(record["opcodes"])
                for counts in (passes[key], tests[test]):
                    counts["gap_runs"] += 1
                    counts["gaps"] += record["count"]
                continue
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
        "gap_runs": sum(row["gap_runs"] for row in passes.values()),
        "gaps": sum(row["gaps"] for row in passes.values()),
        "by_pass": [
            {
                "ir": ir,
                "pass": name,
                **row,
                "opcodes": opcodes[(ir, name)].most_common(5),
            }
            for (ir, name), row in sorted(
                passes.items(),
                key=lambda item: (-item[1]["gaps"], -item[1]["lost_origins"], item[0]),
            )
        ],
        "by_test": [
            {"test": name, **row}
            for name, row in sorted(
                tests.items(),
                key=lambda item: (-item[1]["gaps"], -item[1]["lost_origins"], item[0]),
            )
        ],
    }


def cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def has_gap_data(summary: dict) -> bool:
    """A baseline from before gaps existed has no gap counts."""
    return "gaps" in summary


def load_baseline(path: Path) -> dict:
    try:
        baseline = json.loads(path.read_text())
        if baseline["version"] != 1:
            raise ValueError("unsupported summary version")
        for key in ("lost_origins", "losses", "pass_runs", "audited_tests"):
            if type(baseline[key]) is not int:
                raise ValueError(f"invalid {key}")
        gap_keys = ("gaps", "gap_runs") if has_gap_data(baseline) else ()
        for key in gap_keys:
            if type(baseline[key]) is not int:
                raise ValueError(f"invalid {key}")
        for field, rows in (("by_pass", ("ir", "pass")), ("by_test", ("test",))):
            for item in baseline[field]:
                for key in (*rows, "losses", "lost_origins", *gap_keys):
                    item[key]
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{path}: invalid baseline summary: {error!r}") from error
    return baseline


def delta(base: int, head: int) -> str:
    """Lower is better, so fewer gaps or lost origins is an improvement."""
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
    rows: list[dict], baseline: list[dict], key, metric: str
) -> list[tuple[dict, dict, dict]]:
    """Pair rows by key, ordering by the size of the change in `metric`."""
    empty = {"losses": 0, "lost_origins": 0, "gaps": 0, "gap_runs": 0}
    before = {key(item): item for item in baseline}
    after = {key(item): item for item in rows}
    changed = [
        (
            after.get(name, empty),
            before.get(name, empty),
            after.get(name) or before[name],
        )
        for name in sorted(before.keys() | after.keys())
        if after.get(name, empty)[metric] != before.get(name, empty)[metric]
    ]
    return sorted(changed, key=lambda item: -abs(item[0][metric] - item[1][metric]))


def by_pass(row: dict) -> tuple[str, str]:
    return (row["ir"], row["pass"])


def by_test(row: dict) -> str:
    return row["test"]


def has_changes(summary: dict, baseline: dict) -> bool:
    """Whether to comment: gaps changed, or there is no gap baseline yet, or lost origins changed."""
    lost = compare(
        summary["by_pass"], baseline["by_pass"], by_pass, "lost_origins"
    ) or compare(summary["by_test"], baseline["by_test"], by_test, "lost_origins")
    if not has_gap_data(baseline):
        return True
    return bool(
        lost
        or compare(summary["by_pass"], baseline["by_pass"], by_pass, "gaps")
        or compare(summary["by_test"], baseline["by_test"], by_test, "gaps")
    )


def opcodes(row: dict) -> str:
    return ", ".join(f"`{cell(op)}` {count:,}" for op, count in row.get("opcodes", []))


def markdown(summary: dict, baseline: dict | None = None, base_ref: str = "") -> str:
    gap_baseline = baseline if baseline and has_gap_data(baseline) else None
    if gap_baseline:
        intro = (
            "Instructions without a source location or a reason for lacking one "
            f"(LLVM's `not-generate`), against `{base_ref}`."
        )
        overview = [
            "| Metric | Base | PR | Change |",
            "| --- | ---: | ---: | ---: |",
        ]
        for label, key in (
            ("**gaps**", "gaps"),
            ("pass runs with gaps", "gap_runs"),
        ):
            overview.append(
                f"| {label} | {gap_baseline[key]:,} | {summary[key]:,} | "
                f"{delta(gap_baseline[key], summary[key])} |"
            )
    else:
        intro = (
            "Instructions without a source location or a reason for lacking one "
            "(LLVM's `not-generate`). No gap baseline: "
            + (
                f"the base commit `{base_ref}` was audited before gaps existed."
                if baseline
                else "no baseline audit was found for the base commit."
            )
        )
        overview = [
            "| Metric | PR |",
            "| --- | ---: |",
            f"| **gaps** | {summary['gaps']:,} |",
            f"| pass runs with gaps | {summary['gap_runs']:,} |",
        ]
    if baseline:
        for label, key in (
            ("pass runs", "pass_runs"),
            ("audited tests", "audited_tests"),
        ):
            overview.append(
                f"| {label} | {baseline[key]:,} | {summary[key]:,} | |"
                if gap_baseline
                else f"| {label} | {summary[key]:,} |"
            )
        lost = (
            f"| lost origins (information) | {baseline['lost_origins']:,} | "
            f"{summary['lost_origins']:,} | "
            f"{delta(baseline['lost_origins'], summary['lost_origins'])} |"
        )
        if not gap_baseline:
            lost = (
                f"| lost origins (information) | {summary['lost_origins']:,} "
                f"(base {baseline['lost_origins']:,}, "
                f"{delta(baseline['lost_origins'], summary['lost_origins'])}) |"
            )
    else:
        overview.extend(
            [
                f"| pass runs | {summary['pass_runs']:,} |",
                f"| audited tests | {summary['audited_tests']:,} |",
            ]
        )
        lost = f"| lost origins (information) | {summary['lost_origins']:,} |"
    overview.append(lost)
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
        "",
        "### Gaps by record",
        "",
    ]
    if gap_baseline:
        records_changed = compare(
            summary["by_pass"], gap_baseline["by_pass"], by_pass, "gaps"
        )
        lines.extend(
            [
                "| Record | Gaps | Runs with gaps | Top opcodes |",
                "| --- | ---: | ---: | --- |",
            ]
        )
        for head, base, item in records_changed[:25]:
            lines.append(
                f"| {cell(item['ir'])} {cell(item['pass'])} | "
                f"{change(base['gaps'], head['gaps'])} | "
                f"{change(base['gap_runs'], head['gap_runs'])} | "
                f"{opcodes(head) if head['gaps'] else ''} |"
            )
        if not records_changed:
            lines.extend(["", f"No record changed its gaps vs `{base_ref}`."])
        tests_changed = compare(
            summary["by_test"], gap_baseline["by_test"], by_test, "gaps"
        )
        if tests_changed:
            lines.extend(
                [
                    "",
                    f"### Changed tests vs `{base_ref}`",
                    "",
                    "| Test / revision | Runs with gaps | Gaps |",
                    "| --- | ---: | ---: |",
                ]
            )
            for head, base, item in tests_changed[:25]:
                lines.append(
                    f"| {cell(item['test'])} | "
                    f"{change(base['gap_runs'], head['gap_runs'])} | "
                    f"{change(base['gaps'], head['gaps'])} |"
                )
        lines.extend(
            ["", "<details>", "<summary>Records with the most gaps</summary>", ""]
        )
    else:
        lines.extend(
            ["<details open>", "<summary>Records with the most gaps</summary>", ""]
        )
    lines.extend(
        [
            "| Record | Gaps | Runs with gaps | Top opcodes |",
            "| --- | ---: | ---: | --- |",
        ]
    )
    for item in summary["by_pass"][:25]:
        if item["gaps"]:
            lines.append(
                f"| {cell(item['ir'])} {cell(item['pass'])} | {item['gaps']:,} | "
                f"{item['gap_runs']:,} | {opcodes(item)} |"
            )
    lines.extend(
        [
            "",
            "</details>",
            "",
            "<details>",
            "<summary>Lost origins (information)</summary>",
            "",
        ]
    )
    if baseline:
        passes = compare(
            summary["by_pass"], baseline["by_pass"], by_pass, "lost_origins"
        )
        tests = compare(
            summary["by_test"], baseline["by_test"], by_test, "lost_origins"
        )
        if passes:
            lines.extend(
                [
                    f"Changed passes vs `{base_ref}`:",
                    "",
                    "| Layer | Pass | Runs with losses | Lost origins |",
                    "| --- | --- | ---: | ---: |",
                ]
            )
            for head, base, item in passes[:25]:
                lines.append(
                    f"| {cell(item['ir'])} | {cell(item['pass'])} | "
                    f"{change(base['losses'], head['losses'])} | "
                    f"{change(base['lost_origins'], head['lost_origins'])} |"
                )
            lines.append("")
        if tests:
            lines.extend(
                [
                    f"Changed tests vs `{base_ref}`:",
                    "",
                    "| Test / revision | Runs with losses | Lost origins |",
                    "| --- | ---: | ---: |",
                ]
            )
            for head, base, item in tests[:25]:
                lines.append(
                    f"| {cell(item['test'])} | "
                    f"{change(base['losses'], head['losses'])} | "
                    f"{change(base['lost_origins'], head['lost_origins'])} |"
                )
            lines.append("")
        if not passes and not tests:
            lines.extend(
                [f"No pass or test changed its lost origins vs `{base_ref}`.", ""]
            )
    lines.extend(
        [
            "Passes with the most lost origins:",
            "",
            "| Layer | Pass | Runs with losses | Lost origins |",
            "| --- | --- | ---: | ---: |",
        ]
    )
    for item in sorted(summary["by_pass"], key=lambda r: -r["lost_origins"])[:25]:
        if item["lost_origins"]:
            lines.append(
                f"| {cell(item['ir'])} | {cell(item['pass'])} | "
                f"{item['losses']:,} | {item['lost_origins']:,} |"
            )
    lines.extend(
        [
            "",
            "Tests with the most lost origins:",
            "",
            "| Test / revision | Runs with losses | Lost origins |",
            "| --- | ---: | ---: |",
        ]
    )
    for item in sorted(summary["by_test"], key=lambda r: -r["lost_origins"])[:25]:
        if item["lost_origins"]:
            lines.append(
                f"| {cell(item['test'])} | {item['losses']:,} | {item['lost_origins']:,} |"
            )
    lines.extend(
        [
            "",
            "</details>",
            "",
            (
                "A gap is a bug to fix or to annotate; a lost origin is not, since deleting dead "
                "code or merging two locations legitimately removes one (LLVM prints the same "
                "finding as a `Missing line` warning). Frontend-only and rejected inputs never "
                "reach IR passes. The record `input` counts the gaps lowering left before the first "
                "pass. Each gap is reported once, at the pass that left it. The output directory "
                "contains the full JSON summary and raw per-invocation reports, whose gap "
                "records list at most 100 sites each; counts are exact."
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
