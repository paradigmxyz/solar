"""Check real CFG rewrites across calls and independently replay a changed request."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR
from test_world import LEAF, MID, bytecode


def source(kind):
    value = "  push 0\n" if kind == "call" else ""
    return f"""@module Interactions
bb0 [stack=0]:
  push bb2
  jump bb1
bb1 [ret=1, inputs=1, outputs=1]:
  push {MID}
  extcodesize
  pop
  push 32
  push 0
  push 0
  push 0
{value}  push {MID}
  gas
  {kind}
  pop
  push 0
  mload
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


class InteractionTraceTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.before = self.root / "before.evmir"
        self.after = self.root / "after.evmir"

    def rewrite(self, kind):
        self.before.write_text(source(kind))
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
            timeout=30,
        )
        self.assertIn("push bb3", output)
        self.after.write_text(output)
        return output

    def check(self, status, *flags):
        report = self.root / "report.json"
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
                report,
                *flags,
            ],
            text=True,
            capture_output=True,
            timeout=90,
            check=False,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return json.loads(report.read_text())

    def test_requires_explicit_shared_responses(self):
        for kind in ("call", "staticcall"):
            with self.subTest(kind=kind):
                self.rewrite(kind)
                self.check(2, "--region=bb0")
                self.check(2, "--region=bb0", "--interaction-trace", "--gas=exact")
                record = self.check(0, "--region=bb0", "--interaction-trace")
                self.assertEqual(
                    record["region"]["proof"], "kernel-checked-interaction-trace"
                )
                self.assertEqual(record["gas_policy"], "unpriced-shared-interactions")
                for evidence in record["import_correspondence"]:
                    self.assertEqual(
                        evidence["interactions"], sorted([kind, "extcodesize", "gas"])
                    )
                    self.assertEqual(evidence["resolved_returns"], 1)

    def test_changed_destination_replays_with_real_child_code(self):
        output = self.rewrite("call")
        changed = output.replace(
            f"push {MID:#x}\n  gas\n  call", f"push {LEAF:#x}\n  gas\n  call"
        )
        self.assertNotEqual(output, changed)
        self.after.write_text(changed)
        self.check(2, "--region=bb0", "--interaction-trace")

        fixture = self.root / "world.json"
        fixture.write_text(
            json.dumps(
                {
                    "version": 1,
                    "accounts": [
                        {
                            "address": address,
                            "code": bytecode(
                                f"push {value}\npush 0\nmstore\npush 32\npush 0\nreturn"
                            ),
                        }
                        for address, value in ((MID, 7), (LEAF, 8))
                    ],
                }
            )
        )
        record = self.check(1, "--world", fixture)
        witness = record["witness"]
        self.assertEqual(witness["observations"], witness["native_observations"])

    def test_request_includes_zero_extended_input_and_memory_ranges(self):
        proof = self.root / "Requests.lean"
        proof.write_text("""import SolarTV
open SolarTV EVM.InteractionTrace

example :
    request .call ⟨[5000,196608,7,1,2,32,16,999], [222,173]⟩ =
      some (.message .call 5000 196608 7 ⟨1,2⟩ ⟨32,16⟩ [173,0]) := by decide +kernel

example :
    request .staticcall ⟨[5000,196608,1,2,32,16,999], [222,173]⟩ =
      some (.message .staticcall 5000 196608 0 ⟨1,2⟩ ⟨32,16⟩ [173,0]) := by decide +kernel
""")
        checked = subprocess.run(
            ["lake", "env", "lean", "-j1", proof],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)

    def test_code_size_query_cannot_observe_a_continuation_label(self):
        self.rewrite("staticcall")
        for path, address in ((self.before, str(MID)), (self.after, f"{MID:#x}")):
            text = path.read_text()
            changed = text.replace(
                f"  push {address}\n  extcodesize", "  dup 1\n  extcodesize"
            )
            self.assertNotEqual(text, changed)
            path.write_text(changed)
        record = self.check(2, "--region=bb0", "--interaction-trace")
        self.assertIn("continuation label escapes", record["diagnostic"])
