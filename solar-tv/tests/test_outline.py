"""Check the outlined continuation against the compiler and raw Rust replay."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import heap_entry, native, world

PREFIX = """  push 3
  push 32
  mstore
  push 7
  push 0
  sload
  push bb2
"""
EXITS = """bb1:
  push 42
  push 0
  mstore
  push 32
  push 0
  return
bb2:
  push 43
  push 0
  mstore
  push 32
  push 0
  return
"""
BEFORE = "@module Outline\nbb0:\n" + PREFIX + "  jumpi\n  jump bb1\n" + EXITS
AFTER = (
    "@module Outline\nbb0:\n  push bb4\n  jump bb3\n"
    + EXITS
    + "bb3 [ret=0, inputs=0, outputs=3]:\n"
    + PREFIX
    + "  swap 1\n  swap 2\n  swap 3\n  jump\nbb4 [continuation]:\n  jumpi\n  jump bb1\n"
)


class OutlineTests(unittest.TestCase):
    def test_guarded_outline_preserves_effects_and_resume(self):
        replay = native.default(SOLAR)
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            before, after = (
                directory / f"{name}.evmir" for name in ("before", "after")
            )
            before.write_text(BEFORE)
            after.write_text(AFTER)
            report = directory / "report.json"
            command = [
                CLI,
                "evmir",
                before,
                after,
                "--region",
                "bb0",
                "--solar",
                SOLAR,
                "--replay",
                replay,
                "--evm-version",
                "cancun",
                "--timeout",
                "60",
                "--report",
                report,
            ]
            checked = subprocess.run(
                command, capture_output=True, text=True, timeout=120, check=False
            )
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
            self.assertEqual(
                json.loads(report.read_text())["region"]["proof"],
                "kernel-checked-outlined-prefix",
            )

            def image(path):
                value = subprocess.check_output(
                    [
                        SOLAR,
                        path,
                        "-j1",
                        "-Zevm-ir-pipeline=none",
                        "--evm-version=cancun",
                        "--emit=bin-runtime",
                    ],
                    text=True,
                    timeout=30,
                )
                (contract,) = json.loads(value)["contracts"].values()
                return bytes.fromhex(contract["bin-runtime"])

            left, right = image(before), image(after)
            fixture = directory / "world.json"
            for slot in (0, 1):
                fixture.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "gas": 10000,
                            "accounts": [
                                {"address": 65536, "storage": {"0": str(slot)}}
                            ],
                        }
                    )
                )
                world_state = world.read(fixture, SOLAR, "exact")
                observations = [
                    heap_entry.observe(code, world_state, "cancun", replay, 30)
                    for code in (left, right)
                ]
                self.assertEqual(
                    [r[:2] for r in observations],
                    [["success", [0] * 31 + [42 + slot]]] * 2,
                )
                self.assertEqual(observations[0][3:], observations[1][3:])

            after.write_text(AFTER.replace("swap 3", "swap 2"))
            checked = subprocess.run(
                command, capture_output=True, text=True, timeout=120, check=False
            )
            self.assertNotEqual(checked.returncode, 0)
            site = right.index(bytes.fromhex("90919256")) + 2
            changed = right[:site] + b"\x91" + right[site + 1 :]
            self.assertEqual(
                heap_entry.observe(changed, world_state, "cancun", replay, 30)[0],
                "exception",
            )
