"""Prove the exact emitted CALL budget and replay its bound MIR check."""

import hashlib
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import (
    builtin_lowering,
    call_budget,
    call_tail,
    caller_entry,
    caller_replay,
    callers,
    entry_proof,
    native,
    world,
)
from validator.common import Unsupported


class CallBudgetTests(unittest.TestCase):
    def test_actual_morpho_budget_and_replay(self):
        with tempfile.TemporaryDirectory() as work:
            output = Path(work) / "call-budget"
            worlds = Path(work) / "worlds"
            worlds.mkdir()
            failures = {
                "repay": ("602a5f5563deadbeef5f526004601cfd", b"transferFrom reverted"),
                "supplyCollateral": (
                    "602a5f555f5f5260205ff3",
                    b"transferFrom returned false",
                ),
            }
            for offset, name in enumerate(callers.SELECTORS):
                fixture = caller_entry.default_fixture(name, SOLAR, 30)
                fixture["gas"] = 330000 + offset * 1000
                fixture["warm_addresses"] = [caller_replay.TOKEN]
                fixture["accounts"][0]["storage"]["0xbeef"] = str(10 + offset)
                if name in failures:
                    fixture["accounts"][1]["code"] = "0x" + failures[name][0]
                if name == "flashLoan":
                    data = caller_replay.calldata(name)
                    fixture["calldata"] = (
                        "0x" + (data[:36] + caller_replay.word(9) + data[68:]).hex()
                    )
                (worlds / f"{name}.world.json").write_text(json.dumps(fixture))
            result = subprocess.run(
                [
                    CLI,
                    "call-budget",
                    "--solar",
                    SOLAR,
                    "--output",
                    output,
                    "--caller-worlds",
                    worlds,
                ],
                capture_output=True,
                text=True,
                timeout=1500,
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
            self.assertEqual(entry_report["reachability"], "lean-kernel")
            self.assertEqual(entry_report["dispatcher"], "symbolic-lean-kernel")
            self.assertEqual(entry_report["independent_replay"], "lean-and-rust-replay")
            self.assertEqual(entry_report["snapshot_live_ins"], "lean-kernel")
            self.assertEqual(entry_report["end_to_end"], "lean-kernel")
            self.assertTrue((output / "entry" / entry_report["proof"]).is_file())
            entry = entry_report["paths"]
            self.assertEqual(set(entry), set(report["caller_replays"]))
            for site in report["callers"]:
                name = site["name"]
                proof = output / f"entry/{name}.lean"
                self.assertTrue(proof.is_file())
                self.assertTrue((output / "entry" / entry[name]["proof"]).is_file())
                self.assertGreater(entry[name]["segments"], 0)
                dispatch = entry[name]["dispatch"]
                self.assertTrue((output / "entry" / dispatch["proof"]).is_file())
                self.assertEqual(dispatch["pc"], site["abi_pc"])
                end = entry[name]["exit"]
                self.assertTrue((output / "entry" / end["proof"]).is_file())
                self.assertGreater(end["segments"], 0)
                observation = json.loads(
                    (output / f"entry/{name}-exit.json").read_text()
                )
                self.assertEqual(end["gas"], observation[2])
                self.assertEqual(
                    end["status"], "revert" if name in failures else "success"
                )
                if name in failures:
                    self.assertIn(failures[name][1], bytes(observation[1]))
                    accounts = {int(account[0]): account for account in observation[4]}
                    self.assertEqual(accounts[caller_replay.TOKEN][4], [])
                    self.assertEqual(observation[5], [])
                    initial = json.loads((worlds / f"{name}.world.json").read_text())
                    self.assertEqual(
                        {
                            int(slot): int(value)
                            for slot, value in accounts[caller_replay.MORPHO][4]
                        },
                        {
                            int(slot, 16): int(value)
                            for slot, value in initial["accounts"][0]["storage"].items()
                        },
                    )
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
                        "9" if name == "flashLoan" else "7",
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

            # The first physical segment must reject a different checkpoint PC.
            kernel = output / "entry/kernel"
            segment = next(kernel.glob("EntrySupply*physical0.lean")).read_text()
            modified, replacements = re.subn(
                r"(\bpc := )(\d+)",
                lambda match: f"{match[1]}{int(match[2]) + 1}",
                segment,
                count=1,
            )
            self.assertEqual(replacements, 1)
            rejected = entry_proof.lean(kernel, "WrongCheckpoint", modified, 60)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("(kernel) declaration type mismatch", rejected.stdout)

            supply_exit = entry["supply"]["exit"]
            wrong_gas, replacements = re.subn(
                r"(theorem bytecode_gas : physicalResult.gas = )(\d+)",
                lambda match: f"{match[1]}{int(match[2]) + 1}",
                (output / "entry" / supply_exit["price_proof"]).read_text(),
            )
            self.assertEqual(replacements, 1)
            rejected = entry_proof.lean(kernel, "WrongExitGas", wrong_gas, 60)
            self.assertNotEqual(rejected.returncode, 0)
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
            self.check_symbolic_dispatch(output, runtime, report)

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

            # A child must never answer MIR reentry by running the final bytecode
            # on both sides of the comparison.
            state = world.read(output / "entry/supply.world.json", SOLAR, "exact")
            accounts = {
                int(account["address"]): account for account in state["accounts"]
            }
            accounts[caller_replay.MORPHO]["code"] = list(runtime)
            accounts[caller_replay.TOKEN]["code"] = list(
                bytes.fromhex("5f5f5f5f620500005afa5060015f5260205ff3")
            )
            executable = native.default(SOLAR)
            _, models, _ = builtin_lowering.load_pair(
                [output / "before.mir", output / "after.mir"],
                SOLAR,
                executable,
                "cancun",
                30,
            )
            source = caller_entry.replay_source(
                state,
                runtime,
                report["entry_pc"],
                callers.Site(**report["callers"][0]),
                list(map(builtin_lowering.functions, models)),
                SOLAR,
                executable,
                30,
            )
            reentry = Path(work) / "Reentry.lean"
            reentry.write_text(
                source.split("#eval")[0]
                + """
def throughHelper : EVM.State.Run := do
  let cut ← priced
  let physical ← runByteExit tvWorldConfig 1024 cut
  let entered ← before cut
  runMirExit mirCfg beforeFunctions 1024 cut physical entered
#eval IO.println (EVM.State.describe tvWorldConfig .effects throughHelper)
"""
            )
            result = subprocess.run(
                ["lake", "env", "lean", "-j1", reentry],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                result.stdout.strip(), "unsupported: MIR root runtime code is unknown"
            )

            low_gas = Path(work) / "low-gas.world.json"
            fixture = caller_entry.default_fixture("supply", SOLAR, 30)
            fixture["gas"] = 35000
            fixture["accounts"][1]["code"] = "0x5a5f5560015f5260205ff3"
            low_gas.write_text(json.dumps(fixture))
            state = world.read(low_gas, SOLAR, "exact")
            next(
                account
                for account in state["accounts"]
                if account["address"] == state["address"]
            )["code"] = list(runtime)
            cut = caller_entry.replay(state, report["entry_pc"], executable, 30)
            self.assertEqual(cut["gas"], 1343)
            exit_ = caller_entry.replay(state, None, executable, 30)
            self.assertEqual(exit_[:4], ["out-of-gas", [], 0, 0])
            source = caller_entry.replay_source(
                state,
                runtime,
                report["entry_pc"],
                callers.Site(**report["callers"][0]),
                list(map(builtin_lowering.functions, models)),
                SOLAR,
                executable,
                30,
            )
            through_helper = reentry.read_text().split("def throughHelper", 1)[1]
            reentry.write_text(
                source.split("#eval")[0] + "def throughHelper" + through_helper
            )
            result = subprocess.run(
                ["lake", "env", "lean", "-j1", reentry],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                result.stdout.strip(), "unsupported: missing physical GAS sample"
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

    def check_symbolic_dispatch(self, output, runtime, report):
        directory = output / "entry"
        kernel = directory / "kernel"
        executable = native.default(SOLAR)
        for index, site in enumerate(report["callers"]):
            name = site["name"]
            evidence = report["abi_entry"]["paths"][name]["dispatch"]
            state = world.read(directory / f"{name}.world.json", SOLAR, "exact")
            accounts = {
                int(account["address"]): account for account in state["accounts"]
            }
            accounts[caller_replay.MORPHO]["code"] = list(runtime)
            accounts[caller_replay.MORPHO]["storage"] = []
            accounts[caller_replay.TOKEN]["code"] = [0]
            state["gas"] = evidence["gas"] + index
            tag = site["selector"].to_bytes(4, "big")
            original = caller_replay.calldata(name)
            words = [
                int.from_bytes(original[start : start + 32], "big")
                for start in range(4, len(original) - 64, 32)
            ]
            split = 0 if name == "flashLoan" else 5
            market, parameters = words[:split], words[split:]
            index_of_amount = 1 if name in {"liquidate", "flashLoan"} else 0
            parameters[index_of_amount] += 11
            payload = [index, 255, 0]
            head = (*market, *parameters, 32 * (len(words) + 1), len(payload))
            input_ = (
                tag
                + b"".join(map(caller_replay.word, head))
                + bytes(payload).ljust(32, b"\0")
            )
            state["calldata"] = list(input_)
            observed = caller_entry.replay(state, site["abi_pc"], executable, 30)
            self.assertEqual(observed["gas"], index)
            self.assertEqual(observed["stack"], [str(site["selector"])])
            self.assertEqual(len(observed["pcs"]), evidence["steps"])

            module = Path(evidence["proof"]).stem
            namespace = module.removesuffix("Dispatch")
            source = f"""import {module}
{entry_proof.HEADER}
namespace {namespace}
open MIR.TransferFromABI
def variant : Calldata := ⟨{list(tag)},{market},{parameters},{payload}⟩
def changedEnv : World.Environment := {{env with input := {list(input_)}}}
def changedCfg : MIR.State.Config := {{mirCfg with environment := changedEnv}}
theorem selectedVariant :
    Execution.Trace.segment (chunkedByteStep {report["entry_pc"]} tvWorldConfig code) {evidence["steps"]} 2048
      ⟨dispatchFrame changedEnv {state["gas"]} 0 [],checkpoint,[],[],[]⟩ =
        .ok (.inr (dispatched changedEnv checkpoint {index})) :=
  symbolic_dispatch changedEnv checkpoint {index}
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
theorem decodedVariant :
    mirStart changedCfg beforeFunctions beforeEntry (MIR.TransferFromDispatch.selector variant.tag)
      caller checkpoint mirGas =
    (do
      let frame ← entryFrame caller checkpoint variant.market
        ((parameterWidths caller).zip variant.parameters) (4 + variant.offset + 32) variant.payload.length
      let started ← MIR.State.start beforeFunctions beforeEntry frame.arguments frame
      return ⟨⟨{{function := beforeEntry}},[],started⟩,mirGas.samples⟩) :=
  start_encoding caller changedCfg checkpoint variant beforeFunctions beforeEntry mirGas
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by decide +kernel) (by decide +kernel)
end {namespace}
"""
            result = entry_proof.lean(kernel, module + "Variant", source, 45)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
