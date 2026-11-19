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
from validator import call_budget, call_tail, caller_entry, callers, native, world
from validator.common import Unsupported


class CallBudgetTests(unittest.TestCase):
    def test_actual_morpho_budget_and_replay(self):
        with tempfile.TemporaryDirectory() as work:
            output = Path(work) / "call-budget"
            result = subprocess.run(
                [
                    CLI,
                    "call-budget",
                    "--solar",
                    SOLAR,
                    "--output",
                    output,
                    "--check-caller-entry",
                ],
                capture_output=True,
                text=True,
                timeout=420,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            report = json.loads((output / "result.json").read_text())
            self.assertEqual(report["status"], "verified")
            self.assertEqual(report["gas_policy"], "exact")
            self.assertEqual(report["function"], "safeTransferFrom")
            self.assertEqual(report["additional_stack_slots"], 7)
            self.assertEqual(
                {
                    site["name"]: (site["return_pc"], site["live_words"], site["gas"])
                    for site in report["callers"]
                },
                {
                    "supply": (210, [1, 5], 25),
                    "repay": (210, [1, 5], 25),
                    "supplyCollateral": (2161, [], 1),
                    "liquidate": (6252, [5, 2], 28),
                    "flashLoan": (2161, [], 1),
                },
            )
            self.assertEqual(
                {
                    name: evidence["abi_words"]
                    for name, evidence in report["caller_replays"].items()
                },
                {
                    "supply": [7, 7_000_000],
                    "repay": [7, 7_000_000],
                    "supplyCollateral": [],
                    "liquidate": [8, 7],
                    "flashLoan": [],
                },
            )
            for name, evidence in report["caller_replays"].items():
                self.assertEqual(evidence["rollback"], ["revert", "false"])
                observations = json.loads(
                    (output / "callers" / f"{name}.json").read_text()
                )
                self.assertEqual(len(observations), 3)
                self.assertTrue(observations[0][-1])
                self.assertEqual(observations[1][-1], [])
                self.assertEqual(observations[2][-1], [])
                self.assertIn(b"transferFrom reverted", bytes(observations[1][1]))
                self.assertIn(b"transferFrom returned false", bytes(observations[2][1]))
            entry_report = report["abi_entry"]
            self.assertEqual(entry_report["reachability"], "lean-and-rust-replay")
            self.assertEqual(entry_report["snapshot_live_ins"], "lean-kernel")
            entry = entry_report["paths"]
            self.assertEqual(set(entry), set(report["caller_replays"]))
            for site in report["callers"]:
                name = site["name"]
                proof = output / f"entry/{name}.lean"
                self.assertTrue(proof.is_file())
                snapshot = json.loads((output / f"entry/{name}.json").read_text())[
                    "bytecode"
                ]
                self.assertEqual(snapshot["pc"], report["entry_pc"])
                self.assertEqual(
                    snapshot["stack"][:5],
                    [
                        str(site["return_pc"]),
                        "262144",
                        "65536",
                        "327680",
                        "7",
                    ],
                )
                words = [
                    int(snapshot["stack"][5 + index]) for index in site["live_words"]
                ]
                self.assertEqual(words, report["caller_replays"][name]["abi_words"])
                self.assertEqual(snapshot["gas"], entry[name]["gas"])
                self.assertEqual(
                    len(snapshot["budgets"]),
                    2 if name in {"liquidate", "flashLoan"} else 0,
                )
            wrong_live_in = Path(work) / "WrongLiveIn.lean"
            supply_proof = (output / "entry/supply.lean").read_text()
            wrong_live_in.write_text(
                supply_proof.replace(
                    "def other : Word := 7000000", "def other : Word := 7000001"
                )
            )
            rejected = subprocess.run(
                ["lake", "env", "lean", "-j1", wrong_live_in],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertNotEqual(rejected.returncode, 0, rejected.stdout)
            self.assertIn("decide", rejected.stdout)
            self.assertEqual(
                report["tail_fixed_gas"],
                {
                    "child-failed": 137,
                    "empty": 109,
                    "short": 145,
                    "malformed": 173,
                    "false": 261,
                    "true": 218,
                },
            )
            self.assertEqual(report["returndatacopy_word_gas"], 3)
            self.assertEqual(
                report["continuation_replays"],
                ["success-revert", "empty-malformed", "short-false"],
            )
            for case in report["continuation_replays"]:
                continuation = json.loads(
                    (output / "continuation" / case / "counterexample.json").read_text()
                )
                self.assertEqual(continuation["evidence"], "kernel-and-rust-replay")
                self.assertEqual(
                    continuation["witness"]["observations"],
                    continuation["witness"]["native_observations"],
                )
            success, reverted = json.loads(
                (output / "continuation/success-revert/counterexample.json").read_text()
            )["witness"]["observations"]
            self.assertEqual(bytes(success[1][352:384]), (1).to_bytes(32, "big"))
            self.assertEqual(bytes(reverted[1][32:36]), bytes.fromhex("08c379a0"))
            self.assertIn(b"transferFrom reverted", bytes(reverted[1]))
            storage = [
                next(a[4] for a in side[4] if int(a[0]) == call_budget.TOKEN)
                for side in (success, reverted)
            ]
            self.assertEqual(storage, [[["0", "42"]], []])
            self.assertEqual(report["child_gas_samples"], [95572, 95569])
            self.assertEqual(report["warm_child_gas_samples"], [98033, 98029])
            proof = output / "MIRCallBudget.lean"
            self.assertTrue(proof.is_file())
            self.assertEqual(
                report["frame_replay"],
                {"memory_bytes": 384, "fmp": 352, "broken_fmp": 320},
            )
            frame_report = json.loads(
                (output / "frame" / "counterexample.json").read_text()
            )
            self.assertEqual(frame_report["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                frame_report["witness"]["observations"],
                frame_report["witness"]["native_observations"],
            )
            frames = [
                bytes(side[1]) for side in frame_report["witness"]["observations"]
            ]
            self.assertEqual(
                [int.from_bytes(memory[64:96], "big") for memory in frames],
                [352, 320],
            )
            self.assertEqual(
                frames[0][:64] + frames[0][96:], frames[1][:64] + frames[1][96:]
            )
            self.assertEqual(frames[0][324:332], b"\xff" * 8)
            self.assertEqual((frames[0][31], frames[0][-1]), (0x77, 0x99))
            runtime = bytes.fromhex((output / "runtime.bin").read_text())
            self.assertEqual(
                report["runtime_sha256"], hashlib.sha256(runtime).hexdigest()
            )

            # Dispatch with a different selector must never certify a reached cut.
            state = world.read(output / "entry/supply.world.json", SOLAR, "exact")
            next(
                account
                for account in state["accounts"]
                if account["address"] == state["address"]
            )["code"] = list(runtime)
            state["calldata"][:4] = [255] * 4
            with self.assertRaisesRegex(Unsupported, "bytecode probe was not reached"):
                caller_entry.replay(
                    state, report["entry_pc"], native.default(SOLAR), 30
                )

            self.assertEqual(runtime[report["entry_pc"]], 0x5B)
            self.assertEqual(
                runtime[report["call_pc"] - 1 : report["call_pc"] + 1],
                bytes.fromhex("5af1"),
            )

            mir = json.loads((output / "mir.json").read_text())
            self.assertEqual(mir["status"], "verified")
            self.assertEqual(mir["gas_policy"], "exact-caller-continuation")
            self.assertEqual(
                mir["region"]["proof"], "final-bytecode-caller-continuations"
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
            self.assertEqual(frames[0][224:324], payload)

            # The kernel rejects changes to the imported writes, typed prefix
            # and continuation ledger in the generated certificate.
            lines = proof.read_text().splitlines()
            writes = next(
                i for i, line in enumerate(lines) if line.startswith("def finalWrites ")
            )
            setup = next(
                i
                for i, line in enumerate(lines)
                if line.startswith("def importedSetup ")
            )
            prices = next(
                i for i, line in enumerate(lines) if line.startswith("def tailPrices ")
            )
            lines[prices] = lines[prices].replace("137", "136")
            publication = next(
                i
                for i, line in enumerate(lines)
                if line.startswith("  | .liquidate => ⟨")
            )
            lines[publication] = lines[publication].replace(".pair 5 2", ".pair 2 5")
            lines[writes] = lines[writes].replace(".heap 160", ".heap 128")
            lines[setup] = lines[setup].replace(
                str(call_budget.SELECTOR), str(call_budget.SELECTOR + 1)
            )
            broken_proof = Path(work) / "BrokenFrame.lean"
            broken_proof.write_text("\n".join(lines) + "\n")
            rejected_proof = subprocess.run(
                ["lake", "env", "lean", broken_proof],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertNotEqual(rejected_proof.returncode, 0)
            self.assertIn("finalWrites = stores", rejected_proof.stdout)
            self.assertIn("importedSetup = setup", rejected_proof.stdout)
            self.assertIn("tailPrices = referencePrices", rejected_proof.stdout)
            self.assertIn(
                "callerSuffix Caller.liquidate = reference Caller.liquidate",
                rejected_proof.stdout,
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

            # A valid opcode with a different terminal action breaks the
            # actual caller PC binding, even when the helper bytes match.
            finalized = (output / "finalized.evmir").read_text()
            self.assertEqual(
                callers.extract(
                    scheduled=schedule,
                    finalized=finalized,
                    decoded=call_budget.decode_runtime(runtime, catalog),
                    helper=report["entry"],
                    catalog=catalog,
                    fork="cancun",
                )[0].return_pc,
                210,
            )
            wrong_return = bytearray(runtime)
            supply_pc = report["callers"][0]["return_pc"]
            end = next(
                pc
                for pc, row in call_budget.decode_runtime(runtime, catalog).items()
                if pc >= supply_pc and row == ("return", None)
            )
            wrong_return[end] = 0
            with self.assertRaisesRegex(
                Unsupported, "final EVM IR disagrees with bytecode"
            ):
                callers.extract(
                    schedule,
                    finalized,
                    call_budget.decode_runtime(wrong_return, catalog),
                    report["entry"],
                    catalog,
                    "cancun",
                )

            # The return-copy certificate also binds the physical allocation.
            wrong_size = bytearray(runtime)
            immediate = report["call_pc"] + 3
            self.assertEqual(wrong_size[immediate], 63)
            wrong_size[immediate] = 31
            _, _, _, resume_stack = call_budget.project(location[-1], catalog)
            with self.assertRaisesRegex(Unsupported, "different return-buffer effect"):
                call_tail.extract(
                    call_budget.decode_runtime(wrong_size, catalog),
                    report["call_pc"] + 1,
                    resume_stack,
                    catalog,
                    "cancun",
                )

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
