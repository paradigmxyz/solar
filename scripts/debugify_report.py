#!/usr/bin/env python3
"""Summarize per-pass debug-info audits from the UI suite, optionally against a baseline.

The headline is LLVM's original-mode `not-generate` count: instructions and terminators with no
source origin and no reason for lacking one ("gaps"). Lost origins, the analogue of LLVM's
synthetic-mode `Missing line` warning, are reported as information.
Explicitly explained drops stay in raw sidecars but are excluded from loss tables.
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
                intentional = record.get("intentional", [])
                if not isinstance(intentional, list) or any(
                    not isinstance(span, list)
                    or len(span) != 2
                    or any(type(value) is not int for value in span)
                    or not 0 <= span[0] <= span[1]
                    for span in intentional
                ):
                    raise ValueError("invalid intentional spans")
                explained = {tuple(span) for span in intentional}
                if len(explained) != len(intentional):
                    raise ValueError("duplicate intentional spans")
                if not explained.issubset(tuple(span) for span in lost):
                    raise ValueError("intentional spans are not lost origins")
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
        "intentional_lost_origins": 0,
        "gaps": 0,
        "gap_runs": 0,
    }


def summarize(root: Path) -> dict:
    passes = defaultdict(row)
    opcodes = defaultdict(Counter)
    tests = defaultdict(row)
    test_passes = defaultdict(Counter)
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
            intentional = len(record.get("intentional", []))
            unexplained = len(record["lost"]) - intentional
            for counts in (passes[key], tests[test]):
                counts["runs"] += 1
                counts["losses"] += bool(unexplained)
                counts["lost_origins"] += unexplained
                counts["intentional_lost_origins"] += intentional
            if unexplained:
                counts = test_passes[(test, *key)]
                counts["losses"] += 1
                counts["lost_origins"] += unexplained
    return {
        "version": 1,
        "invocations": len(files),
        "audited_tests": len(tests),
        "pass_runs": sum(row["runs"] for row in passes.values()),
        "losses": sum(row["losses"] for row in passes.values()),
        "lost_origins": sum(row["lost_origins"] for row in passes.values()),
        "intentional_lost_origins": sum(
            row["intentional_lost_origins"] for row in passes.values()
        ),
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
        # Zero-loss rows are implicit, so this index stays small even for long pipelines.
        "by_test_pass": [
            {"test": test, "ir": ir, "pass": name, **counts}
            for (test, ir, name), counts in sorted(test_passes.items())
        ],
    }


def cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def has_gap_data(summary: dict) -> bool:
    """A baseline from before gaps existed has no gap counts."""
    return "gaps" in summary


def load_baseline(path: Path, audits: Path | None = None) -> dict:
    try:
        baseline = json.loads(path.read_text())
        if baseline["version"] != 1:
            raise ValueError("unsupported summary version")
        for key in ("lost_origins", "losses", "pass_runs", "audited_tests"):
            if type(baseline[key]) is not int:
                raise ValueError(f"invalid {key}")
        check_intentional_count(baseline)
        gap_keys = ("gaps", "gap_runs") if has_gap_data(baseline) else ()
        for key in gap_keys:
            if type(baseline[key]) is not int:
                raise ValueError(f"invalid {key}")
        for field, rows in (("by_pass", ("ir", "pass")), ("by_test", ("test",))):
            for item in baseline[field]:
                check_intentional_count(item)
                for key in (*rows, "losses", "lost_origins", *gap_keys):
                    item[key]
        check_test_pass_counts(baseline)
        if audits is not None:
            rebuilt = summarize(audits)
            for key in (
                "invocations",
                "audited_tests",
                "pass_runs",
                "losses",
                "lost_origins",
                "intentional_lost_origins",
                *gap_keys,
            ):
                if key in baseline and baseline[key] != rebuilt[key]:
                    raise ValueError(f"raw baseline audits disagree on {key}")
            for field, key in (("by_test", by_test), ("by_pass", by_pass)):
                expected = {key(item): item for item in baseline[field]}
                actual = {key(item): item for item in rebuilt[field]}
                if expected.keys() != actual.keys() or any(
                    old[metric] != actual[name][metric]
                    for name, old in expected.items()
                    for metric in row()
                    if metric in old
                ):
                    raise ValueError(f"raw baseline audits disagree on {field}")
            baseline = rebuilt
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{path}: invalid baseline summary: {error!r}") from error
    return baseline


def check_test_pass_counts(summary: dict) -> None:
    """The optional sparse index must account for every unexplained loss exactly once."""
    if "by_test_pass" not in summary:
        return
    if not isinstance(summary["by_test_pass"], list):
        raise ValueError("invalid by_test_pass index")
    tests = defaultdict(Counter)
    passes = defaultdict(Counter)
    seen = set()
    for item in summary["by_test_pass"]:
        if not isinstance(item, dict) or any(
            not isinstance(item.get(field), str) or not item[field]
            for field in ("test", "ir", "pass")
        ):
            raise ValueError("invalid per-test pass identity")
        key = (item["test"], item["ir"], item["pass"])
        if item["ir"] not in ("MIR", "EVM IR") or key in seen:
            raise ValueError("invalid or duplicate per-test pass identity")
        seen.add(key)
        for metric in ("losses", "lost_origins"):
            count = item.get(metric)
            if type(count) is not int or count <= 0:
                raise ValueError(f"invalid per-test pass {metric}")
            tests[item["test"]][metric] += count
            passes[(item["ir"], item["pass"])][metric] += count
        if item["losses"] > item["lost_origins"]:
            raise ValueError("more loss runs than lost origins")
    for field, key, counts in (
        ("by_test", by_test, tests),
        ("by_pass", by_pass, passes),
    ):
        expected = {
            key(item): {metric: item[metric] for metric in ("losses", "lost_origins")}
            for item in summary[field]
            if item["losses"] or item["lost_origins"]
        }
        if counts != expected:
            raise ValueError(f"per-test pass index disagrees with {field}")
    for metric in ("losses", "lost_origins"):
        if sum(item[metric] for item in tests.values()) != summary[metric]:
            raise ValueError(f"per-test pass index disagrees with {metric}")


def check_intentional_count(counts: dict) -> None:
    """Older summaries predate explicit explanations and count none."""
    if not isinstance(counts, dict):
        raise ValueError("invalid summary counts")
    value = counts.setdefault("intentional_lost_origins", 0)
    if type(value) is not int or value < 0:
        raise ValueError("invalid intentional_lost_origins count")


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
    rows: list[dict],
    baseline: list[dict],
    key,
    metric: str,
    secondary: str | None = None,
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
        or (
            secondary is not None
            and after.get(name, empty)[secondary] != before.get(name, empty)[secondary]
        )
    ]
    return sorted(changed, key=lambda item: -abs(item[0][metric] - item[1][metric]))


def by_pass(row: dict) -> tuple[str, str]:
    return (row["ir"], row["pass"])


def by_test(row: dict) -> str:
    return row["test"]


def passes_for_tests(summary: dict, selected: set[str]) -> list[dict] | None:
    """Older summaries can compare the full test set, but cannot split it by pass."""
    if not selected:
        return []
    if "by_test_pass" not in summary:
        tests = {item["test"] for item in summary["by_test"]}
        return (
            [
                item
                for item in summary["by_pass"]
                if item["losses"] or item["lost_origins"]
            ]
            if selected == tests
            else None
        )
    passes = defaultdict(Counter)
    for item in summary["by_test_pass"]:
        if item["test"] in selected:
            counts = passes[by_pass(item)]
            counts["losses"] += item["losses"]
            counts["lost_origins"] += item["lost_origins"]
    return [
        {"ir": ir, "pass": name, **counts}
        for (ir, name), counts in sorted(passes.items())
    ]


def has_changes(summary: dict, baseline: dict) -> bool:
    """Whether to comment: gaps changed, or there is no gap baseline yet, or lost origins changed."""
    lost = compare(
        summary["by_pass"], baseline["by_pass"], by_pass, "lost_origins", "losses"
    ) or compare(
        summary["by_test"], baseline["by_test"], by_test, "lost_origins", "losses"
    )
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
    head_tests = {item["test"]: item for item in summary["by_test"]}
    base_tests = (
        {item["test"]: item for item in baseline["by_test"]} if baseline else {}
    )
    shared = head_tests.keys() & base_tests.keys()
    added = [head_tests[name] for name in sorted(head_tests.keys() - base_tests.keys())]
    removed = [
        base_tests[name] for name in sorted(base_tests.keys() - head_tests.keys())
    ]
    different_tests = baseline is not None and bool(added or removed)
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
        label = (
            "lost origins (all tests)"
            if different_tests
            else "lost origins (information)"
        )
        comparison = (
            "test sets differ"
            if different_tests
            else delta(baseline["lost_origins"], summary["lost_origins"])
        )
        lost = (
            f"| {label} | {baseline['lost_origins']:,} | "
            f"{summary['lost_origins']:,} | "
            f"{comparison} |"
        )
        if not gap_baseline:
            lost = (
                f"| {label} | {summary['lost_origins']:,} "
                f"(base {baseline['lost_origins']:,}, "
                f"{comparison}) |"
            )
        if different_tests:
            before = sum(base_tests[name]["lost_origins"] for name in shared)
            after = sum(head_tests[name]["lost_origins"] for name in shared)
            comparison = delta(before, after) if shared else "no shared tests"
            overview.append(
                f"| lost origins (shared tests) | {before:,} | {after:,} | {comparison} |"
                if gap_baseline
                else f"| lost origins (shared tests) | {after:,} (base {before:,}, {comparison}) |"
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
            f"{summary['pass_runs']:,} pass runs."
        ),
        "",
    ]
    if different_tests:
        lines.extend(
            [
                (
                    f"The shared-test comparison covers {len(shared):,} test/revision pairs. "
                    f"Added tests: {len(added):,} "
                    f"({sum(item['lost_origins'] for item in added):,} lost origins). "
                    f"Removed tests: {len(removed):,} "
                    f"({sum(item['lost_origins'] for item in removed):,} lost origins). "
                    "Lost-origin pass deltas use shared tests only; gap counts include all tests."
                ),
                "",
            ]
        )
    if intentional := summary.get("intentional_lost_origins", 0):
        lines.extend(
            [
                (
                    f"{intentional:,} intentionally dropped origins are recorded in the raw "
                    "audits and excluded from the loss tables."
                ),
                "",
            ]
        )
    lines.extend(["### Gaps by record", ""])
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
        shared_head = passes_for_tests(summary, shared)
        shared_base = passes_for_tests(baseline, shared)
        passes = (
            compare(shared_head, shared_base, by_pass, "lost_origins", "losses")
            if shared_head is not None and shared_base is not None
            else None
        )
        tests = compare(
            [head_tests[name] for name in sorted(shared)],
            [base_tests[name] for name in sorted(shared)],
            by_test,
            "lost_origins",
            "losses",
        )
        if passes is None:
            lines.extend(
                [
                    (
                        "Shared-test per-pass comparison unavailable: an audit summary lacks "
                        "per-test pass counts. Rebuild missing summaries from their raw audits."
                    ),
                    "",
                ]
            )
        if passes:
            lines.extend(
                [
                    f"Changed passes vs `{base_ref}` (shared tests only):",
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
                    f"Changed shared tests vs `{base_ref}`:",
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
        if passes is not None and not passes and not tests and not different_tests:
            lines.extend(
                [f"No pass or test changed its lost origins vs `{base_ref}`.", ""]
            )
        elif not tests and shared:
            if passes is not None and not passes:
                lines.extend(
                    [f"No shared pass changed its lost origins vs `{base_ref}`.", ""]
                )
            lines.extend(
                [f"No shared test changed its lost origins vs `{base_ref}`.", ""]
            )
        elif not shared:
            lines.extend(["No shared test/revision pairs to compare.", ""])
        for label, items, audit in (
            ("Added tests", added, summary),
            ("Removed tests", removed, baseline),
        ):
            if not items:
                continue
            cohort_passes = passes_for_tests(audit, {item["test"] for item in items})
            if cohort_passes is None:
                lines.extend(
                    [f"{label} per-pass totals unavailable in this older summary.", ""]
                )
            elif cohort_passes:
                lines.extend(
                    [
                        f"{label} pass totals (no same-test comparison):",
                        "",
                        "| Layer | Pass | Runs with losses | Lost origins |",
                        "| --- | --- | ---: | ---: |",
                    ]
                )
                for item in sorted(
                    cohort_passes,
                    key=lambda r: (-r["lost_origins"], r["ir"], r["pass"]),
                )[:25]:
                    if item["lost_origins"]:
                        lines.append(
                            f"| {cell(item['ir'])} | {cell(item['pass'])} | "
                            f"{item['losses']:,} | {item['lost_origins']:,} |"
                        )
                lines.append("")
            lines.extend(
                [
                    f"{label} (no same-test comparison):",
                    "",
                    "| Test / revision | Runs with losses | Lost origins |",
                    "| --- | ---: | ---: |",
                ]
            )
            for item in sorted(items, key=lambda r: (-r["lost_origins"], r["test"]))[
                :25
            ]:
                lines.append(
                    f"| {cell(item['test'])} | {item['losses']:,} | {item['lost_origins']:,} |"
                )
            lines.append("")
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
    parser.add_argument(
        "--baseline-audits",
        type=Path,
        help="rebuild an older baseline from its exact raw audits and verify its totals",
    )
    parser.add_argument("--base-ref", default="", help="label for the base commit")
    args = parser.parse_args()
    try:
        summary = summarize(args.input)
        if args.baseline_audits and not args.baseline:
            raise ValueError("--baseline-audits requires --baseline")
        baseline = (
            load_baseline(args.baseline, args.baseline_audits)
            if args.baseline
            else None
        )
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
