"""Human-readable summaries with links to complete, unmodified artifacts."""

import io
import json
import shlex
import unittest
from pathlib import Path
from unittest.mock import patch


def shorten(text, full=False):
    return text if full or len(text) <= 180 else text[:177] + "..."


def show_attempt(directory, label):
    directory = Path(directory)
    print(f"  {label}: {directory}")
    if not directory.is_dir():
        return
    for filename in ("compiler.json", "compilation.json", "result.json"):
        try:
            record = json.loads((directory / filename).read_text(encoding="utf-8"))
            if filename == "compiler.json":
                print(f"    command: {shlex.join(record['command'])}")
                version = " ".join(record.get("version_stdout", "").split())
                print(
                    f"    version: {version[:180] or 'unavailable'}; sha256: {record.get('sha256')}"
                )
            elif filename == "compilation.json":
                print(f"    target: {record['fully_qualified_name']}")
            else:
                print(
                    f"    exit: {record.get('returncode')}; seconds: {record.get('seconds', 'unknown')}; max rss kb: {record.get('max_rss_kb') or 'unknown'}"
                )
                if record.get("failure"):
                    print(f"    failure: {record['failure']}")
        except OSError, ValueError, KeyError, TypeError, AttributeError:
            print(f"    {filename}: unavailable or malformed")
    print(f"    output: {directory / 'stdout.txt'}")
    print(f"    stderr: {directory / 'stderr.txt'}")
    if (directory / "replay.sh").is_file():
        print(
            f"    replay compiler: {shlex.join(['sh', str(directory / 'replay.sh')])}"
        )


def show_pair(pair, full=False):
    status = (
        "FAIL"
        if pair["failed"]
        else "KNOWN"
        if any(r["differences"] for r in pair["results"])
        else "PASS"
    )
    checks = ", ".join(r["comparator"] + "=" + r["status"] for r in pair["results"])
    print(f"[{status}] {pair['id']}  {checks}", flush=True)
    if status == "PASS":
        return
    for side in ("left", "right"):
        if pair[side]:
            show_attempt(pair[side], side)
        else:
            print(f"  {side}: missing compiler attempt")
    for result in pair["results"]:
        if result.get("error"):
            print(f"  {result['comparator']}: {result['error']}")
        if result["status"] == "unsupported":
            print(
                f"  {result['comparator']}: requested outputs are missing on both sides"
            )
        differences = result["differences"]
        for difference in differences if full else differences[:5]:
            kind = "KNOWN" if "expected" in difference else "DIFF"
            print(
                f"  [{kind}] {result['comparator']} {difference['path']} ({difference['kind']})"
            )
            values = {
                side: json.dumps(difference[side], ensure_ascii=True, sort_keys=True)
                for side in ("left", "right")
            }
            print(f"    left:  {shorten(values['left'], full)}")
            print(f"    right: {shorten(values['right'], full)}")
            if not full and any(len(value) > 180 for value in values.values()):
                print("    values shortened; use --full or open the report")
            if "expected" in difference:
                print(f"    reason: {difference['expected']}")
        if not full and len(differences) > 5:
            print(
                f"    ... {len(differences) - 5} more differences; use --full or open the report"
            )
    if pair.get("bundle"):
        print(f"  bundle: {pair['bundle']}")
    if pair.get("replay"):
        print(f"  replay comparison: {shlex.join(['sh', pair['replay']])}")


def show_ast_summary(summary, worst=10):
    fields = summary["fields"]
    total = {k: sum(row[k] for row in fields) for k in ("compared", "equal", "ignored")}
    reference = [n for n, c in summary["node_types"].items() if c["left"]]
    print(
        f"AST: {total['equal']}/{total['compared']} field comparisons equal"
        f" ({total['equal'] * 100_000 // max(total['compared'], 1) / 1000}%), {total['ignored']} ignored"
    )
    nodes = summary["node_types"].values()
    print(
        f"AST nodes: {sum(c['paired'] for c in nodes)}/{sum(c['left'] for c in nodes)}"
        " of the reference's are paired with a candidate node"
    )
    missing = summary["never_emitted"]
    print(
        f"AST node types: {len(reference) - len(missing)}/{len(reference)} of the reference's are emitted"
        + (f"; never emitted: {shorten(', '.join(missing))}" if missing else "")
    )
    lowest = sorted(
        (row for row in fields if row["equal"] < row["compared"]),
        key=lambda row: row["equal"] / row["compared"],
    )
    for row in lowest[:worst]:
        print(
            f"  {row['nodeType']}.{row['field']}: {row['equal']}/{row['compared']} equal,"
            f" {row['different']} different, {row['missing_left']} missing left,"
            f" {row['missing_right']} missing right"
        )
    if len(lowest) > worst:
        print(f"  ... {len(lowest) - worst} more fields below 100%; see report.json")
    if summary["unused_rules"]:
        print(f"unused AST rules: {summary['unused_rules']}")


class Tests(unittest.TestCase):
    def test_difference_output(self):
        pair = {
            "id": "case",
            "failed": True,
            "left": "reference.json",
            "right": "candidate.json",
            "results": [
                {
                    "comparator": "abi",
                    "status": "different",
                    "differences": [
                        {
                            "path": "/contracts/C.sol/C/f/stateMutability",
                            "kind": "value",
                            "left": "view",
                            "right": "pure",
                        }
                    ],
                }
            ],
            "bundle": "/tmp/bundle",
            "replay": "/tmp/bundle/compare.sh",
        }
        with patch("sys.stdout", new_callable=io.StringIO) as output:
            show_pair(pair)
        self.assertEqual(
            output.getvalue(),
            """[FAIL] case  abi=different
  left: reference.json
  right: candidate.json
  [DIFF] abi /contracts/C.sol/C/f/stateMutability (value)
    left:  "view"
    right: "pure"
  bundle: /tmp/bundle
  replay comparison: sh /tmp/bundle/compare.sh
""",
        )
        pair["failed"] = False
        pair["results"][0]["differences"][0]["expected"] = "tracked issue"
        with patch("sys.stdout", new_callable=io.StringIO) as output:
            show_pair(pair)
        self.assertEqual(
            output.getvalue(),
            """[KNOWN] case  abi=different
  left: reference.json
  right: candidate.json
  [KNOWN] abi /contracts/C.sol/C/f/stateMutability (value)
    left:  "view"
    right: "pure"
    reason: tracked issue
  bundle: /tmp/bundle
  replay comparison: sh /tmp/bundle/compare.sh
""",
        )

    def test_full_values(self):
        self.assertEqual(shorten(json.dumps("x" * 200)), '"' + "x" * 176 + "...")
        self.assertEqual(
            shorten(json.dumps("x" * 200), full=True), '"' + "x" * 200 + '"'
        )

    def test_ast_summary(self):
        row = {"nodeType": "Call", "field": "isPure", "compared": 4, "equal": 3}
        row.update(different=1, missing_left=0, missing_right=0, ignored=2)
        summary = {
            "fields": [row, {**row, "field": "kind", "equal": 4, "different": 0}],
            "node_types": {
                "Call": {"left": 2, "right": 2, "paired": 2},
                "Throw": {"left": 1, "right": 0, "paired": 0},
            },
            "never_emitted": ["Throw"],
            "unused_rules": [1],
        }
        with patch("sys.stdout", new_callable=io.StringIO) as output:
            show_ast_summary(summary)
        self.assertEqual(
            output.getvalue(),
            """AST: 7/8 field comparisons equal (87.5%), 4 ignored
AST nodes: 2/3 of the reference's are paired with a candidate node
AST node types: 1/2 of the reference's are emitted; never emitted: Throw
  Call.isPure: 3/4 equal, 1 different, 0 missing left, 0 missing right
unused AST rules: [1]
""",
        )
