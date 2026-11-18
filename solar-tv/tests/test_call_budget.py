"""Prove the exact emitted CALL budget and replay its bound MIR check."""

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import call_budget, native
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
            self.assertEqual(report["child_gas_samples"], [95572, 95569])
            self.assertEqual(report["warm_child_gas_samples"], [98033, 98029])
            self.assertTrue((output / "MIRCallBudget.lean").is_file())
            runtime = bytes.fromhex((output / "runtime.bin").read_text())
            self.assertEqual(
                report["runtime_sha256"], hashlib.sha256(runtime).hexdigest()
            )
            self.assertEqual(runtime[report["entry_pc"]], 0x5B)
            self.assertEqual(
                runtime[report["call_pc"] - 1 : report["call_pc"] + 1],
                bytes.fromhex("5af1"),
            )

            mir = json.loads((output / "mir.json").read_text())
            self.assertEqual(mir["status"], "verified")
            self.assertEqual(mir["gas_policy"], "exact-call-boundary")
            self.assertEqual(
                mir["region"]["proof"], "final-bytecode-call-returndata-lowering"
            )
            self.assertEqual(
                mir["import_correspondence"][0]["final_bytecode"]["runtime_sha256"],
                report["runtime_sha256"],
            )

            replay = json.loads((output / "counterexample.json").read_text())
            self.assertEqual(replay["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                replay["witness"]["observations"],
                replay["witness"]["native_observations"],
            )

            payloads = [
                bytes(side[1][32:]) for side in replay["witness"]["observations"]
            ]
            payload = bytes.fromhex("23b872dd") + b"".join(
                word.to_bytes(32, "big") for word in (0x20000, 0x20001, 1)
            )
            self.assertEqual(payloads, [payload, payload])

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

            catalog = {
                row["name"]: row
                for row in native.catalog(native.default(SOLAR), 30)["evmir"]
            }
            location = call_budget.extract_final(runtime, setup, catalog)
            prefix = bytearray(runtime)
            prefix[report["call_pc"] - 1] = 0x59  # MSIZE is not the GAS producer.
            with self.assertRaisesRegex(
                Unsupported, "cannot bind a unique final-bytecode"
            ):
                call_budget.extract_final(prefix, setup, catalog)

            # The native witness keeps the original physically decoded prefix,
            # allowing only its local no-code relocation and the paid mutation.
            fixture = json.loads((output / "world.json").read_text())
            good, bad = (bytes.fromhex(a["code"][2:]) for a in fixture["accounts"][:2])
            self.assertEqual(good, call_budget.replay_image(runtime, location))
            self.assertEqual(
                bad,
                call_budget.replay_image(
                    runtime, location, extra=bytes.fromhex("5f50")
                ),
            )

            changed = Path(work) / "different.mir"
            after = (output / "after.mir").read_text()
            changed.write_text(after.replace("0x23b872dd", "0x23b872dc"))
            self.assertNotEqual(changed.read_text(), after)
            rejected = subprocess.run(
                [
                    CLI,
                    "call-budget",
                    "--solar",
                    SOLAR,
                    "--output",
                    Path(work) / "bad-pair",
                    "--mir-pair",
                    output / "before.mir",
                    changed,
                ],
                capture_output=True,
                text=True,
                timeout=90,
                check=False,
            )
            self.assertEqual(rejected.returncode, 2, rejected.stdout + rejected.stderr)
            self.assertIn("differs from the same-compile", rejected.stderr)
