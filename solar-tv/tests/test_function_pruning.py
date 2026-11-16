"""Rooted function pruning preserves live declarations, calls and entry points."""

import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, prune


class FunctionPruningTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.folder = ROOT / "examples/mir/function-dce"
        self.paths = [self.folder / f"{side}.mir" for side in ("before", "after")]

    def cli(self, paths=None, *flags):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *(paths or self.paths),
                "--solar",
                SOLAR,
                "--timeout",
                "30",
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=100,
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_pass_with_recursive_calls_and_cfg_cycles(self):
        emitted = subprocess.run(
            [SOLAR, self.paths[0], "-Ogas", "-Zmir-pipeline=function-dce"],
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )
        self.assertEqual(emitted.returncode, 0, emitted.stderr)
        self.assertEqual(
            prune.declarations(emitted.stdout),
            prune.declarations(self.paths[1].read_text()),
        )
        result, record = self.cli(None, "--prune")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["region"]["kind"], "module")
        self.assertEqual(
            record["region"]["proof"], "rooted-function-table-correspondence"
        )
        self.assertEqual(record["region"]["removed"], ["unused", "unused_leaf"])
        self.assertEqual(
            set(record["region"]["external_entries"]),
            {"constructor", "f", "spin", "loop"},
        )
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)

    def test_changed_body_is_rejected_and_has_an_independent_counterexample(self):
        paths = [self.paths[0], self.folder / "broken.mir"]
        result, record = self.cli(paths, "--prune")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("retained body or interface", record["diagnostic"])
        result, record = self.cli(paths, "--function", "f")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_external_roots_and_shared_declarations_cannot_disappear(self):
        _, functions = prune.declarations(self.paths[1].read_text())
        for name in ("constructor", "f", "spin", "loop"):
            with self.subTest(root=name):
                changed = self.directory / "removed-root.mir"
                changed.write_text(
                    "@module FunctionDce\n"
                    + "\n".join(
                        "\n".join(body["text"])
                        for key, body in functions.items()
                        if key != name
                    )
                )
                # Recursive spin must be removed as a complete declaration, not
                # replaced by a dangling call that only the parser would reject.
                result, record = self.cli([self.paths[0], changed], "--prune")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("external entry point", record["diagnostic"])
        changed = self.directory / "module.mir"
        changed.write_text(
            self.paths[1]
            .read_text()
            .replace("@module FunctionDce", "@module Different")
        )
        result, record = self.cli([self.paths[0], changed], "--prune")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("shared declarations", record["diagnostic"])

    def test_interfaces_numeric_dispatchers_and_unrooted_modules_are_rejected(self):
        after = self.directory / "changed.mir"
        for old, new, message in (
            (
                "selector=0x01020304",
                "selector=0x01020305",
                "retained body or interface",
            ),
            (
                "selector=0x01020304,",
                "function_pointer_dispatcher, selector=0x01020304,",
                "function-pointer",
            ),
        ):
            with self.subTest(change=new):
                after.write_text(self.paths[1].read_text().replace(old, new))
                result, record = self.cli([self.paths[0], after], "--prune")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(message, record["diagnostic"])
        before = self.directory / "before.mir"
        before.write_text(
            "@module None\nfn @f() {\nbb0:\nret\n}\nfn @dead() {\nbb0:\nret\n}\n"
        )
        after.write_text("@module None\nfn @f() {\nbb0:\nret\n}\n")
        result, record = self.cli([before, after], "--prune")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("explicit module entry points", record["diagnostic"])

    def test_quotes_comments_and_reference_extraction(self):
        text = """@module Quotes
fn @f() [entry] {
bb0:
  literal "@dead; \\"quoted\\"" ; @not_a_reference
  icall @live
  ret
}
fn @live() {
bb0:
  ret
}
"""
        _, functions = prune.declarations(text)
        self.assertEqual(functions["f"]["targets"], ["live"])
        self.assertIn('literal "@dead; \\"quoted\\""', functions["f"]["text"])
        for role in (
            "constructor",
            "entry",
            "receive",
            "fallback",
            "selector=0x12345678",
        ):
            with self.subTest(role=role):
                _, table = prune.declarations(
                    f"@module Roots\nfn @f() [{role}] {{\nbb0:\nret\n}}\n"
                )
                self.assertTrue(table["f"]["root"])

    def test_live_pass_records_module_scope_and_keeps_capture_selection(self):
        command = [SOLAR, self.paths[0], "-Ogas", "-Zmir-pipeline=function-dce"]
        audit, capture = [self.directory / name for name in ("audit", "capture")]
        environment = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        results = [
            subprocess.run(
                [*command, *flags],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
                timeout=100,
            )
            for flags in (
                [],
                [f"-Zsolar-tv={audit}"],
                [f"-Zsolar-tv={capture}", "-Zsolar-tv-capture"],
            )
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        self.assertEqual(results[0].stdout, results[2].stdout)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertTrue(records)
        self.assertTrue(
            all(
                record["status"] == "verified" and record["region"]["kind"] == "module"
                for record in records
            )
        )
        rows = evaluate.captures(capture)
        self.assertTrue(rows)
        self.assertTrue(
            all(
                row["rejection"] == "function was removed or its identity changed"
                for row in rows
            )
        )
        self.assertEqual(
            {row["region"]["name"] for row in rows},
            {"unused", "unused_leaf", "live", "spin", "loop"},
        )

    def test_structural_certificate_does_not_accept_world_or_gas_claims(self):
        for flags in (("--gas", "refine"), ("--world", self.folder / "before.mir")):
            with self.subTest(flags=flags):
                result, record = self.cli(None, "--prune", *flags)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("unpriced MIR without", record["diagnostic"])

    def test_kernel_axiom_policy(self):
        path = self.directory / "Axioms.lean"
        path.write_text(
            "import SolarTV\n"
            + "\n".join(
                f"#print axioms SolarTV.MIR.FunctionDCE.{name}"
                for name in ("preserve", "certificate", "roots_preserved")
            )
        )
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(result.stdout.splitlines()), 3)
        for line in result.stdout.splitlines():
            if "does not depend on any axioms" in line:
                continue
            axioms = re.search(r"depends on axioms: \[(.*?)\]", line)
            self.assertIsNotNone(axioms, line)
            self.assertLessEqual(
                set(axioms[1].split(", ")),
                {"propext", "Quot.sound", "Classical.choice"},
            )


if __name__ == "__main__":
    unittest.main()
