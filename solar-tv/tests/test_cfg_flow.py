"""Check CFG coalescing through live state and reject an unsafe jump contraction."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR


class CfgFlowTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.before = self.root / "before.evmir"
        self.after = self.root / "after.evmir"

    def check(self, flags):
        report = self.root / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                self.before,
                self.after,
                "--solar",
                SOLAR,
                "--evm-version=cancun",
                "--timeout=30",
                "--report",
                report,
                *flags,
            ],
            text=True,
            capture_output=True,
            timeout=90,
            check=False,
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_cfg_simplify_preserves_the_effect_and_retains_the_stack_guard(self):
        self.before.write_text("""@module Coalesce
bb0 [stack=0]:
  push 0
  sload
  jump bb1
bb1:
  push 0
  mstore
  push 32
  push 0
  return
""")
        args = [
            SOLAR,
            self.before,
            "-j1",
            "-Ogas",
            "-Zevm-ir-pipeline=cfg-simplify-early",
        ]
        self.after.write_text(subprocess.check_output(args, text=True, timeout=20))
        result, report = self.check(["--region=bb0"])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            report["region"]["proof"], "kernel-checked-jump-chain-projection"
        )
        self.assertEqual(report["gas_policy"], "unpriced-structural")
        self.assertLess(
            report["import_correspondence"][1]["source_points"],
            report["import_correspondence"][0]["source_points"],
        )
        world = self.root / "world.json"
        world.write_text(
            '{"version":1,"accounts":[{"address":65536,"storage":{"0":"73"}}]}'
        )
        self.after.write_text(self.after.read_text().replace("sload", "pop\n  push 0"))
        result, report = self.check(["--world", world])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(report["status"], "counterexample")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_unsafe_temporary_slot_is_not_erased(self):
        self.before.write_text("""@module Limit
bb0 [stack=1]:
  sload
  jump bb2
bb1:
  invalid
bb2:
  stop
""")
        self.after.write_text("""@module Limit
bb0 [stack=1]:
  sload
  stop
""")
        result, report = self.check(["--region=bb0"])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(report["status"], "unsupported")

    def test_live_terminal_dedup_checks_shared_reverts_from_entry(self):
        fixture = (
            ROOT.parent / "tests/ui/codegen/evm-ir/late-structural/shared_reverts.evmir"
        )
        command = [
            SOLAR,
            fixture,
            "-j1",
            "-Ogas",
            "--emit=abi,bin,bin-runtime",
            "-Zevm-ir-pipeline=terminal-dedup",
        ]
        baseline = subprocess.run(
            command, text=True, capture_output=True, timeout=30, check=False
        )
        audits = self.root / "audits"
        verified = subprocess.run(
            [*command, f"-Zsolar-tv={audits}"],
            env={**os.environ, "SOLAR": SOLAR, "SOLAR_TV": CLI},
            text=True,
            capture_output=True,
            timeout=60,
            check=False,
        )
        self.assertEqual(baseline.returncode, 0, baseline.stderr)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertEqual(verified.stdout, baseline.stdout)
        (artifact,) = json.loads(verified.stdout.splitlines()[-1])["contracts"].values()
        self.assertIn("abi", artifact)
        self.assertTrue(artifact["bin"])
        self.assertTrue(artifact["bin-runtime"])
        (report,) = list((audits / "audits").glob("*.json"))
        entry = json.loads(report.read_text())
        self.assertEqual(entry["status"], "verified")
        self.assertEqual(entry["region"]["proof"], "kernel-checked-entry-layout")
        self.assertEqual(entry["region"]["kind"], "entry")
        before, after = entry["import_correspondence"]
        self.assertTrue(before["physical_entry"])
        self.assertTrue(after["physical_entry"])

    def test_shared_tail_checks_both_incoming_edges(self):
        self.before.write_text("""@module SharedTail
bb0 [stack=0]:
  push 1
  push 0
  sload
  jumpi bb3, bb1
bb1:
  push 2
  push 1
  sload
  jumpi bb4, bb2
bb2:
  invalid
bb3:
  jump bb5
bb4:
  jump bb5
bb5:
  push 0
  mstore
  push 32
  push 0
  return
""")
        output = subprocess.check_output(
            [
                SOLAR,
                self.before,
                "-j1",
                "-Ogas",
                "--evm-version=cancun",
                "-Zevm-ir-pipeline=cfg-simplify-early",
            ],
            text=True,
            timeout=20,
        )
        self.after.write_text(output)
        result, report = self.check(["--region=bb0"])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            report["region"]["proof"], "kernel-checked-jump-chain-projection"
        )
        self.assertEqual(
            [side["reused_points"] for side in report["import_correspondence"]],
            [0, 1],
        )

        # The first branch must send a nonzero condition to the shared tail.
        self.after.write_text(output.replace("jumpi bb5, bb1", "jumpi bb2, bb1"))
        self.assertNotEqual(self.after.read_text(), output)
        result, report = self.check(["--region=bb0"])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(report["status"], "unsupported")
        world = self.root / "world.json"
        world.write_text(
            '{"version":1,"accounts":[{"address":65536,"storage":{"0":"1"}}]}'
        )
        result, report = self.check(["--world", world])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(report["status"], "counterexample")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )
