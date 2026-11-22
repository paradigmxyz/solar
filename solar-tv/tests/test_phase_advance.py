"""Check a real lower-evm-shaped phase advance and enforce its boundary."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate


class PhaseAdvanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.directory = Path(temporary.name)
        cls.source = {
            "language": "Solidity",
            "sources": {"C.sol": {"content": "contract C {}"}},
            "settings": {
                "optimizer": {"enabled": True},
                "outputSelection": {
                    "*": {"*": ["abi", "evm.bytecode", "evm.deployedBytecode"]}
                },
            },
        }
        cls.baseline, _ = evaluate.compile_input(SOLAR, cls.source, cls.directory)
        cls.captured, _ = evaluate.compile_input(
            SOLAR, cls.source, cls.directory, capture=True
        )
        (cls.phase_change,) = (
            row
            for row in evaluate.captures(cls.directory)
            if row["pass"] == "lower-evm-shaped"
        )

    def check_pair(self, paths, status):
        report = self.directory / "phase-report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *paths,
                "--prune",
                "--solar",
                SOLAR,
                "--report",
                report,
                "--timeout=30",
                "--verbose",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return json.loads(report.read_text())

    def test_real_capture_proves_all_resolved_bodies(self):
        self.assertEqual(self.baseline, self.captured)
        self.assertIsNone(self.phase_change["region"])
        record = self.check_pair(self.phase_change["paths"], 0)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertFalse(record["whole_pass"])
        self.assertEqual(
            record["region"],
            {
                "kind": "module",
                "proof": "resolved-phase-advance",
                "phases": ["semantic", "lowered"],
                "functions": ["entry"],
            },
        )
        before, after = record["import_correspondence"]
        self.assertNotEqual(before["source_sha256"], after["source_sha256"])
        self.assertEqual(before["resolved_sha256"], after["resolved_sha256"])

    def test_live_pass_runs_certificate_without_changing_output(self):
        audit = self.directory / "audit"
        result = subprocess.run(
            [SOLAR, "--standard-json", "-j1", f"-Zsolar-tv={audit}"],
            input=json.dumps(self.source),
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        contracts = json.loads(result.stdout)["contracts"]
        self.assertEqual(evaluate.digest(evaluate.stable(contracts)), self.baseline)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        (record,) = (
            row
            for row in records
            if row["context"].startswith("solar-tv: mir pass `lower-evm-shaped`")
        )
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["region"]["proof"], "resolved-phase-advance")

    def test_phase_label_does_not_allow_other_edits(self):
        before, after = self.phase_change["paths"]
        text = after.read_text()
        broken = self.directory / "changed.mir"
        for old, new in (("revert 0, 0", "stop"), ("@module C", "@module Different")):
            with self.subTest(edit=new):
                self.assertIn(old, text)
                broken.write_text(text.replace(old, new))
                record = self.check_pair([before, broken], 2)
                self.assertEqual(record["status"], "unsupported")
                self.assertIn("phase advance changed executable", record["diagnostic"])
        self.assertEqual(self.check_pair([after, before], 2)["status"], "unsupported")

    def test_lowered_phase_contract_is_validated(self):
        before, after = (
            self.directory / name for name in ("narrow.mir", "bad-phase.mir")
        )
        text = """@module P
fn @f(arg0: i8) -> i8 {
  bb0:
    ret arg0
}
"""
        before.write_text(text)
        after.write_text(text.replace("@module P\n", "@module P\n@phase lowered\n"))
        record = self.check_pair([before, after], 2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("non-word type", record["diagnostic"])
