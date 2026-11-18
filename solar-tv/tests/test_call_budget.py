"""Exact CALL pricing for the pinned, compiler-scheduled Morpho runtime."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import call_budget
from validator.common import Unsupported


class CallBudgetTests(unittest.TestCase):
    def test_actual_morpho_budget_and_replay(self):
        with tempfile.TemporaryDirectory() as work:
            output = Path(work) / "call-budget"
            result = subprocess.run(
                [CLI, "call-budget", "--solar", SOLAR, "--output", output],
                capture_output=True,
                text=True,
                timeout=180,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            report = json.loads((output / "result.json").read_text())
            self.assertEqual(report["status"], "verified")
            self.assertEqual(report["gas_policy"], "exact")
            self.assertEqual(report["function"], "safeTransferFrom")
            self.assertEqual(report["child_gas_samples"], [95578, 95574])

            replay = json.loads((output / "counterexample.json").read_text())
            self.assertEqual(replay["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                replay["witness"]["observations"],
                replay["witness"]["native_observations"],
            )

            schedule = (output / "scheduled.evmir").read_text()
            disasm = (output / "scheduled.disasm").read_text()
            entry, _, setup, _ = call_budget.extract(schedule, disasm)
            self.assertEqual(entry, report["entry"])
            self.assertEqual(setup[-2:], ["gas", "call"])
            # A second assembly fingerprint alone cannot certify changed operands.
            mutated = disasm.replace(
                "PUSH1 0x64\nDUP8\nPUSH0\nDUP7\nGAS\nCALL",
                "PUSH1 0x63\nDUP8\nPUSH0\nDUP7\nGAS\nCALL",
            )
            self.assertNotEqual(mutated, disasm)
            with self.assertRaisesRegex(
                Unsupported, "scheduled CALL and assembly disagree"
            ):
                call_budget.extract(schedule, mutated)
