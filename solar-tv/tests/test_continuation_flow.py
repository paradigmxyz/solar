"""Verify real continuation rewrites without treating label words as data."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, SOLAR

INPUT = """@module Continuation
bb0 [stack=0]:
  push bb2
  push 7
  jump bb1
bb1 [ret=1, inputs=2, outputs=1]:
  dup 2
  pop
  swap 1
  jump
bb2 [continuation]:
  jump bb3
bb3 [continuation]:
  push 0
  mstore
  push 32
  push 0
  return
"""


class ContinuationFlowTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.before = self.root / "before.evmir"
        self.after = self.root / "after.evmir"
        self.before.write_text(INPUT)
        self.compile_args = [
            SOLAR,
            self.before,
            "-j1",
            "-Ogas",
            "--evm-version=cancun",
            "-Zevm-ir-pipeline=cfg-simplify-early",
        ]

    def compile(self):
        output = subprocess.check_output(self.compile_args, text=True, timeout=30)
        self.after.write_text(output)
        return output

    def check(self, *flags):
        path = self.root / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                self.before,
                self.after,
                "--solar",
                SOLAR,
                "--timeout=30",
                "--evm-version=cancun",
                "--report",
                path,
                *flags,
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertTrue(path.exists(), result.stdout + result.stderr)
        return result, json.loads(path.read_text())

    def test_redirected_label_reaches_its_return(self):
        output = self.compile()
        self.assertIn("push bb3", output)
        self.assertNotIn("bb2 [continuation]", output)
        result, record = self.check("--region=bb0")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "kernel-checked-continuation-flow")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(
            [side["resolved_returns"] for side in record["import_correspondence"]],
            [1, 1],
        )

        self.after.write_text(
            output.replace("push bb3", "push bb99")
            + "\nbb99 [continuation]:\n  invalid\n"
        )
        result, record = self.check("--region=bb0")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

        world = self.root / "world.json"
        world.write_text('{"version":1}')
        result, record = self.check("--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_label_used_as_data_is_outside_the_certificate(self):
        # Copy the tag into storage while retaining the original return operand.
        self.before.write_text(
            INPUT.replace("  swap 1\n", "  dup 2\n  push 0\n  sstore\n  swap 1\n")
        )
        self.compile()
        result, record = self.check("--region=bb0")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_live_check_keeps_bytecode(self):
        directory = self.root / "audit"
        args = [*self.compile_args, "--emit=bin-runtime"]
        results = [
            subprocess.check_output(
                [*args, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                text=True,
                timeout=90,
            )
            for flags in ([], [f"-Zsolar-tv={directory}"])
        ]
        self.assertEqual(*results)
        reports = [
            json.loads(path.read_text())
            for path in (directory / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                record["status"] == "verified"
                and record.get("region", {}).get("proof")
                == "kernel-checked-continuation-flow"
                for record in reports
            )
        )
