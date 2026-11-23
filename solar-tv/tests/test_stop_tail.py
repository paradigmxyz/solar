"""Deleting POPs before STOP requires room on every physical incoming path."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import native, world
from validator.common import Program

SOURCE = """@module StopTail
bb0 [stack=0]:
  push bb3
  push 11
  push 22
  push 33
  push 0
  calldataload
  jumpi bb1, bb2
bb1:
  swap 3
  jump
bb2:
  swap 3
  jump
bb3 [continuation]:
  push 0
  push 0
  log1
  pop
  pop
  stop
"""


class StopTailTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.directory = Path(folder.name)
        self.before = self.directory / "before.evmir"
        self.after = self.directory / "after.evmir"
        self.report = self.directory / "report.json"
        self.replay = native.default(SOLAR)

    def cli(self, *flags):
        result = subprocess.run(
            [
                CLI,
                "evmir",
                self.before,
                self.after,
                "--prefix=bb3",
                "--evm-version=cancun",
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                self.report,
                *flags,
            ],
            timeout=90,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertTrue(self.report.is_file(), result.stderr)
        return result, json.loads(self.report.read_text())

    def test_real_dce_suffix_proves_the_caller_floor(self):
        self.before.write_text(SOURCE)
        output = subprocess.check_output(
            [SOLAR, self.before, "-j1", "-Ogas", "-Zevm-ir-pipeline=dce"],
            timeout=30,
            text=True,
        )
        self.assertNotEqual(
            output.split("@module", 1)[-1], SOURCE.split("@module", 1)[-1]
        )
        self.after.write_text(output)
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-terminal-pop-deletion"
        )
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        for side in record["import_correspondence"]:
            self.assertEqual(side["incoming_minimum"], 2)
            self.assertEqual(side["removed_pops"], 2)
        result, record = self.cli("--gas=exact")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)

        folder = self.directory / "live"
        env = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        env.pop("SOLAR_TV_GAS", None)
        command = [
            SOLAR,
            self.before,
            "-j1",
            "-Ogas",
            "-Zevm-ir-pipeline=dce",
            "--evm-version=cancun",
            "--emit=bin-runtime",
            "--allow=2264",
        ]
        plain = subprocess.check_output(command, env=env, text=True, timeout=30)
        audited = subprocess.run(
            [*command, f"-Zsolar-tv={folder}"],
            env=env,
            text=True,
            timeout=90,
            capture_output=True,
            check=False,
        )
        self.assertEqual(audited.returncode, 0, audited.stderr)
        self.assertEqual(plain, audited.stdout)
        (receipt,) = list((folder / "audits").glob("*.json"))
        record = json.loads(receipt.read_text())
        self.assertEqual(record["status"], "verified")
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-terminal-pop-deletion"
        )

    def test_short_caller_cannot_prove_terminal_pop_deletion(self):
        short = SOURCE.replace("bb2:\n  swap 3", "bb2:\n  pop\n  swap 2")
        self.before.write_text(short)
        self.after.write_text(short.replace("  pop\n  pop\n  stop", "  stop"))
        result, record = self.cli()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn(
            "caller paths do not establish enough stack words", record["diagnostic"]
        )

        fixture = self.directory / "world.json"
        fixture.write_text('{"version":1}')
        state = world.read(fixture, SOLAR, "exact")
        for path, expected in ((self.before, "exception"), (self.after, "success")):
            code = world.runtime(path.read_text(), "evmir", SOLAR, "cancun", 30)
            program = Program(
                ("world-entry", "evmir"),
                [],
                "",
                "",
                world=world.with_runtime(state, code),
                evm_version="cancun",
                replay_ir=SOURCE,
                gas="exact",
            )
            result = native.replay(
                program,
                program,
                [],
                raw_runtime=True,
                executable=self.replay,
                timeout=30,
            )
            self.assertEqual(json.loads(result["observations"][0])[0], expected)
