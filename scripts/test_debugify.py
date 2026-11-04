"""Audit summaries must expose losses and reject absent or damaged compiler output."""

import json
import tempfile
import unittest
from pathlib import Path

from debugify_report import has_changes, load_baseline, markdown, summarize


class DebugifyReportTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def write(self, name, *records):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        header = {"kind": "invocation", "version": 1, "arguments": [], "sources": []}
        path.write_text("\n".join(map(json.dumps, (header, *records))) + "\n")
        return path

    def test_losses_are_grouped_by_pass_and_revision(self):
        self.write(
            "mir/example.mir/report/first.jsonl",
            self.record("MIR", "dce", 3, 1, [[10, 20], [30, 40]]),
            self.record("MIR", "sccp", 1, 1, []),
        )
        self.write(
            "mir/example.mir/check/second.jsonl",
            self.record("MIR", "dce", 2, 1, [[10, 20]]),
        )
        self.write(
            "evm-ir/other.evmir/default/third.jsonl",
            self.record("EVM IR", "dce", 1, 1, []),
        )
        report = summarize(self.root)
        self.assertEqual(report["invocations"], 3)
        self.assertEqual(report["audited_tests"], 3)
        self.assertEqual(report["pass_runs"], 4)
        self.assertEqual(report["lost_origins"], 3)
        self.assertEqual(report["losses"], 2)
        self.assertEqual(
            report["by_pass"][0],
            {
                "ir": "MIR",
                "pass": "dce",
                "runs": 2,
                "losses": 2,
                "lost_origins": 3,
                "gaps": 0,
                "gap_runs": 0,
                "opcodes": [],
            },
        )
        self.assertIn("mir/example.mir/report", markdown(report))

    def test_baseline_comparison_lists_changed_passes_and_tests(self):
        self.write(
            "base/a/default/x.jsonl",
            self.record("MIR", "dce", 3, 1, [[1, 2], [3, 4]]),
            self.record("MIR", "sccp", 1, 1, []),
        )
        base = summarize(self.root / "base")
        self.write(
            "head/a/default/x.jsonl",
            self.record("MIR", "dce", 3, 0, [[1, 2], [3, 4], [5, 6]]),
            self.record("MIR", "sccp", 1, 1, []),
        )
        head = summarize(self.root / "head")
        self.assertTrue(has_changes(head, base))
        self.assertFalse(has_changes(base, base))
        report = markdown(head, base, "abc12345")
        self.assertIn("(LLVM's `not-generate`), against `abc12345`", report)
        self.assertIn("| MIR | dce | 1 | 2 → 3 (❌ +1, +50.00%) |", report)
        self.assertNotIn("sccp", report.split("<details>")[0])
        self.assertIn("No pass or test changed", markdown(base, base, "abc12345"))
        self.assertIn("no baseline audit", markdown(head))

    def test_gaps_are_the_headline_and_lost_origins_information(self):
        self.write(
            "base/a/default/x.jsonl",
            self.record("MIR", "dce", 3, 1, [[1, 2], [3, 4]]),
            self.gaps("MIR", "input", {"add": 2}),
        )
        base = summarize(self.root / "base")
        self.write(
            "head/a/default/x.jsonl",
            self.record("MIR", "dce", 3, 3, []),
            self.gaps("MIR", "input", {"add": 2}),
            self.gaps("EVM IR", "stackify", {"jump": 3, "push": 1}),
        )
        head = summarize(self.root / "head")
        self.assertEqual((base["gaps"], head["gaps"]), (2, 6))
        self.assertEqual(head["gap_runs"], 2)
        self.assertEqual(head["by_pass"][0]["opcodes"], [("jump", 3), ("push", 1)])
        report = markdown(head, base, "abc12345")
        self.assertIn("| **gaps** | 2 | 6 | ❌ +4, +200.00% |", report)
        self.assertIn("| EVM IR stackify | 4 | 1 |", report)
        self.assertIn(
            "| lost origins (information) | 2 | 0 | ✅ -2, -100.00% |", report
        )
        # Lost origins have their own collapsed table and never reach the gap tables.
        self.assertNotIn("dce", report.split("<summary>Lost origins")[0])
        self.assertTrue(has_changes(head, base))
        self.assertFalse(has_changes(base, base))
        self.assertTrue(has_changes(base, {**base, "gaps": 1, "by_pass": []}))

    def test_baseline_without_gap_data_still_renders(self):
        self.write("head/a/default/x.jsonl", self.record("MIR", "dce", 2, 1, [[1, 2]]))
        head = summarize(self.root / "head")
        old = {
            "version": 1,
            "lost_origins": 5,
            "losses": 2,
            "pass_runs": 9,
            "audited_tests": 3,
            "by_pass": [{"ir": "MIR", "pass": "dce", "losses": 2, "lost_origins": 5}],
            "by_test": [{"test": "a/default", "losses": 2, "lost_origins": 5}],
        }
        path = self.root / "summary.json"
        path.write_text(json.dumps(old))
        baseline = load_baseline(path)
        report = markdown(head, baseline, "abc12345")
        self.assertIn("No gap baseline", report)
        self.assertIn("audited before gaps existed", report)
        self.assertIn(
            "| lost origins (information) | 1 (base 5, ✅ -4, -80.00%) |", report
        )
        self.assertTrue(has_changes(head, baseline))

    def test_input_gaps_alone_are_an_audit(self):
        # `-Zdebugify=check` stops at the input gaps, before any pass record exists.
        self.write("only/a/default/x.jsonl", self.gaps("MIR", "input", {"add": 2}))
        report = summarize(self.root / "only")
        self.assertEqual((report["gaps"], report["pass_runs"]), (2, 0))

    def test_inconsistent_gap_records_fail(self):
        record = self.gaps("MIR", "dce", {"add": 2})
        self.write("bad.jsonl", {**record, "count": 3})
        with self.assertRaisesRegex(ValueError, "inconsistent gap opcodes"):
            summarize(self.root)
        # A negative entry that offsets an overcount is not a histogram.
        self.write(
            "bad.jsonl", {**record, "opcodes": {"add": 3, "sub": -1}, "count": 2}
        )
        with self.assertRaisesRegex(ValueError, "inconsistent gap opcodes"):
            summarize(self.root)

    def test_invalid_baseline_fails(self):
        path = self.root / "summary.json"
        path.write_text('{"version": 2}')
        with self.assertRaisesRegex(ValueError, "invalid baseline summary"):
            load_baseline(path)

    def test_no_output_does_not_pass_as_zero_losses(self):
        with self.assertRaisesRegex(ValueError, "no debugify audits"):
            summarize(self.root)

    def test_header_only_and_truncated_records_fail(self):
        path = self.write("empty.jsonl")
        with self.assertRaisesRegex(ValueError, "no audit records"):
            summarize(self.root)
        with path.open("a") as stream:
            stream.write('{"kind": "pass",')
        with self.assertRaisesRegex(ValueError, "empty.jsonl:2"):
            summarize(self.root)

    def test_inconsistent_counts_fail(self):
        self.write("bad.jsonl", self.record("MIR", "dce", 2, 2, [[10, 20]]))
        with self.assertRaisesRegex(ValueError, "inconsistent origin counts"):
            summarize(self.root)

    @staticmethod
    def gaps(ir, name, opcodes):
        return {
            "kind": "gaps",
            "ir": ir,
            "pass": name,
            "module": "test",
            "count": sum(opcodes.values()),
            "opcodes": opcodes,
            "sites": [],
            "omitted": sum(opcodes.values()),
        }

    @staticmethod
    def record(ir, name, before, retained, lost):
        return {
            "kind": "pass",
            "ir": ir,
            "pass": name,
            "module": "test",
            "before": before,
            "retained": retained,
            "lost": lost,
        }


if __name__ == "__main__":
    unittest.main()
