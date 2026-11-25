"""Charged shared prologues, real reordering and conservative window fallback."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR


class MemoryPrefixTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)
        self.examples = ROOT / "examples/evmir/memory-prologue"

    def check(self, before, after, *flags, status=0):
        report = self.folder / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                before,
                after,
                "--solar",
                SOLAR,
                "--timeout",
                "20",
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=150,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_example_is_actual_reorder_expressions_output(self):
        result = subprocess.run(
            [
                SOLAR,
                self.examples / "before.evmir",
                "-Ogas",
                "-Zevm-ir-pipeline=reorder-expressions",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        normalize = lambda text: [
            line.strip()
            for line in text.splitlines()
            if line.strip() and not line.startswith(";")
        ]
        self.assertEqual(
            normalize(result.stdout),
            normalize((self.examples / "after.evmir").read_text()),
        )

    def test_prologue_executes_before_the_universal_scalar_proof(self):
        _, record = self.check(
            self.examples / "before.evmir",
            self.examples / "after.evmir",
            "--prefix",
            "bb1",
        )
        self.assertEqual(record["region"]["proof"], "universal-memory-prefix")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertEqual(
            [
                row["shared_prologue_operations"]
                for row in record["import_correspondence"]
            ],
            [3, 3],
        )
        self.assertIn(
            "identical memory prologue executes with dynamic and expansion charges",
            " ".join(record["assumptions"]),
        )

    def test_fixed_window_falls_back_without_defaulting_calldata(self):
        _, record = self.check(
            self.examples / "before.evmir",
            self.examples / "after.evmir",
            "--window",
            "bb1",
            "--stack-height",
            "1",
        )
        self.assertEqual(record["region"]["proof"], "universal-memory-prefix")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertEqual(record["region"]["requested_stack_height"], 1)
        self.assertEqual(record["gas_policy"], "refine")

    def test_exact_gas_policy_is_not_silently_weakened(self):
        _, record = self.check(
            self.examples / "before.evmir",
            self.examples / "after.evmir",
            "--prefix",
            "bb1",
            "--gas",
            "exact",
            status=2,
        )
        self.assertIn("requested gas policy", record["diagnostic"])

    def test_changed_calldata_does_not_verify(self):
        _, record = self.check(
            self.examples / "before.evmir",
            self.examples / "broken.evmir",
            "--window",
            "bb1",
            "--stack-height",
            "1",
            status=2,
        )
        self.assertEqual(record["status"], "unsupported")
        _, record = self.check(
            self.examples / "before.evmir",
            self.examples / "broken.evmir",
            "--world",
            self.examples / "world.json",
            status=1,
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_changed_store_and_continuation_are_not_erased(self):
        text = (self.examples / "after.evmir").read_text()
        for before, after in (
            ("push 160", "push 161"),
            ("mstore", "mstore8"),
            ("push bb2", "push bb3"),
        ):
            with self.subTest(after=after):
                changed = self.folder / "changed.evmir"
                changed.write_text(text.replace(before, after, 1))
                _, record = self.check(
                    self.examples / "before.evmir", changed, "--prefix", "bb1", status=2
                )
                self.assertEqual(record["status"], "unsupported")

    def test_live_hook_keeps_compiler_output_identical(self):
        audit = self.folder / "audits"
        command = [
            SOLAR,
            self.examples / "before.evmir",
            "-Ogas",
            "-Zevm-ir-pipeline=reorder-expressions",
            "--emit=bin",
        ]
        results = [
            subprocess.run(
                [*command, *flags],
                capture_output=True,
                text=True,
                check=False,
                timeout=180,
                env={**os.environ, "SOLAR": SOLAR, "SOLAR_TV": CLI},
            )
            for flags in ([], [f"-Zsolar-tv={audit}"])
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                record["status"] == "verified"
                and record.get("region", {}).get("proof") == "universal-memory-prefix"
                for record in records
            ),
            results[1].stderr,
        )

    def test_dynamic_shared_memory_uses_effect_correspondence(self):
        text = (self.examples / "before.evmir").read_text()
        before, after = self.folder / "before.evmir", self.folder / "after.evmir"
        for path, side in ((before, "before"), (after, "after")):
            text = (self.examples / f"{side}.evmir").read_text()
            path.write_text(
                text.replace("  push 160\n  push 64\n  mstore\n", "  mload\n")
            )
        _, record = self.check(before, after, "--prefix", "bb1")
        self.assertEqual(record["region"]["proof"], "universal-effectful-stack-prefix")
        self.assertIsNone(record["region"]["stack_height"])

    def test_exact_shared_prologue_keeps_early_faults(self):
        before, after = self.folder / "before.evmir", self.folder / "after.evmir"
        text = "@module Exact\nbb0:\n push 160\n push 64\n mstore\n push 1\n push 2\n add\n push 0\n mstore\n push 32\n push 0\n return\n"
        before.write_text(text)
        after.write_text(text.replace(" push 1\n push 2\n", " push 2\n push 1\n"))
        _, record = self.check(before, after, "--prefix", "bb0", "--gas", "exact")
        self.assertEqual(record["gas_policy"], "exact")
        for gas in (0, 11, 18, 19, 100000):
            with self.subTest(gas=gas):
                fixture = self.folder / "world.json"
                fixture.write_text(json.dumps({"version": 1, "gas": gas}))
                self.check(before, after, "--world", fixture, "--gas", "exact")

    def test_common_scalar_values_stay_in_the_scalar_proof(self):
        source = ROOT / "examples/evmir/calldata-prefix"
        paths = [self.folder / f"{side}.evmir" for side in ("before", "after")]
        for path, side in zip(paths, ("before", "after"), strict=True):
            text = (source / f"{side}.evmir").read_text()
            path.write_text(
                text.replace("bb0:\n", "bb0:\n  push 160\n  push 64\n  mstore\n")
            )
        _, record = self.check(*paths, "--prefix", "bb0")
        self.assertEqual(
            [
                row["shared_prologue_operations"]
                for row in record["import_correspondence"]
            ],
            [3, 3],
        )


if __name__ == "__main__":
    unittest.main()
