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
    caller_context,
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
                if name == "supply":
                    fixture["gas"] = 36520
                    fixture["warm_addresses"] = []
                    fixture["accounts"][1]["code"] = "0x5a5f5560015f5260205ff3"
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
            self.assertEqual(entry_report["decoder"], "symbolic-lean-kernel")
            self.assertEqual(entry_report["independent_replay"], "lean-and-rust-replay")
            self.assertEqual(entry_report["snapshot_live_ins"], "lean-kernel")
            self.assertEqual(entry_report["end_to_end"], "lean-kernel")
            self.assertTrue((output / "entry" / entry_report["proof"]).is_file())
            context = entry_report["shared_mir_context"]
            self.assertEqual(
                set(context["replacements"]),
                {"safeTransfer", "safeTransferFrom", "setAuthorizationWithSig"},
            )
            self.assertTrue((output / "entry" / context["proof"]).is_file())
            for name in ("safe_transfer_call", "authorization_with_sig"):
                self.assertTrue(
                    (output / "entry" / entry_report[name]["proof"]).is_file()
                )
            for name in (
                "prehelper_calls",
                "authorization_entry",
                "recovery_boundary",
                "reentrant_owner",
            ):
                self.assertTrue(
                    (output / "entry" / entry_report[name]["proof"]).is_file()
                )
            entry = entry_report["paths"]
            collateral = entry["supplyCollateral"]["collateral_write"]
            self.assertEqual(
                collateral["empty_callback_mir"],
                ["before_abi_collateral_helper", "after_abi_collateral_helper"],
            )
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
                decoder = entry[name]["decoder"]
                self.assertTrue(decoder["applied"])
                self.assertTrue((output / "entry" / decoder["proof"]).is_file())
                decoded = json.loads(
                    (output / f"entry/{name}-decoder.json").read_text()
                )
                self.assertEqual(decoded["pc"], decoder["pc"])
                routed = json.loads(
                    (output / f"entry/{name}-dispatch.json").read_text()
                )
                self.assertEqual(routed["gas"] - decoded["gas"], decoder["gas"])
                self.assertEqual(
                    len(decoded["pcs"]), dispatch["steps"] + decoder["steps"]
                )
                end = entry[name]["exit"]
                self.assertTrue((output / "entry" / end["proof"]).is_file())
                self.assertGreater(end["segments"], 0)
                observation = json.loads(
                    (output / f"entry/{name}-exit.json").read_text()
                )
                self.assertEqual(end["gas"], observation[2])
                if name == "supply":
                    self.assertEqual(end["status"], "out-of-gas")
                    self.assertEqual(observation[:4], ["out-of-gas", [], 0, 0])
                elif name in failures:
                    self.assertEqual(end["status"], "revert")
                    self.assertIn(failures[name][1], bytes(observation[1]))
                else:
                    self.assertEqual(end["status"], "success")
                if name == "supply" or name in failures:
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
                if name == "supply":
                    self.assertEqual(snapshot["gas"], 2863)
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
            pricing = f"""import EntrySupplyProofbeforeLink
import EntrySupplyProofafterLink
{entry_proof.HEADER}
open EntrySupply
theorem supply_paid_call : byteTailCut.budgets = [100] :=
  by decide +kernel
example : (paidHelperBudget (helperCallPrice mirCfg beforeFunctions physicalCut beforeCut)).toList = [100] :=
  before_paid_call.trans supply_paid_call
example : (paidHelperBudget (helperCallPrice mirCfg afterFunctions physicalCut afterCut)).toList = [100] :=
  after_paid_call.trans supply_paid_call
example :
    byteTailCut.result.stop = .outOfGas ∧ byteTailCut.samples = [102] ∧ byteTailCut.unpaidCall = none :=
  by decide +kernel
example : helperSamples mirCfg beforeFunctions physicalCut beforeCut = [102] :=
  before_samples.trans (by decide +kernel)
example : helperSamples mirCfg afterFunctions physicalCut afterCut = [102] :=
  after_samples.trans (by decide +kernel)
example : beforeTailCut.gas = 0 :=
  (congrArg EVM.State.Observation.gas before_observation).trans bytecode_gas
example : afterTailCut.gas = 0 :=
  (congrArg EVM.State.Observation.gas after_observation).trans bytecode_gas
#print axioms supply_paid_call
"""
            proved = entry_proof.lean(kernel, "SupplyPaidCall", pricing, 60)
            self.assertEqual(proved.returncode, 0, proved.stdout + proved.stderr)
            entry_proof.check_axioms(proved.stdout.splitlines(), 1)
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
            self.check_symbolic_abi_prefix(output, runtime, report)

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
            state["gas"] = 330000
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
            tables = list(map(builtin_lowering.functions, models))
            sites = [callers.Site(**row) for row in report["callers"]]
            invalid = {
                **tables[1],
                "safeTransfer": {
                    **tables[1]["safeTransfer"],
                    "returns": [builtin_lowering.word()],
                },
            }
            catalog = {op["name"]: op for op in native.catalog(executable, 30)["evmir"]}
            mismatch, _ = caller_context.source([tables[0], invalid], sites, catalog)
            rejected_context = entry_proof.lean(
                Path(work), "ChangedCallSignature", mismatch, 60
            )
            self.assertNotEqual(rejected_context.returncode, 0)
            self.assertIn("signature", rejected_context.stdout)
            source = caller_entry.replay_source(
                state,
                runtime,
                report["entry_pc"],
                callers.Site(**report["callers"][0]),
                tables,
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
  let entered ← before cut
  let result ← runMirExit mirCfg beforeFunctions .supply 1024 cut entered
  return finishByte checkpoint result
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

            state = world.read(output / "entry/supply.world.json", SOLAR, "exact")
            next(
                account
                for account in state["accounts"]
                if account["address"] == state["address"]
            )["code"] = list(runtime)
            # Find the minimum root gas for GAS from the independent PC probe.
            sample_pc = report["call_pc"] - 1
            gas_fee = 2
            funded = state | {"gas": 300000}
            sample = caller_entry.replay(funded, sample_pc, executable, 30)
            minimum = funded["gas"] - sample["gas"] + gas_fee
            call_pc = report["call_pc"]
            call_fee = 100
            charge = caller_entry.replay(funded, call_pc, executable, 30)
            call_minimum = funded["gas"] - charge["gas"] + call_fee
            amounts = [35000, minimum - 1, minimum, call_minimum - 1, call_minimum]
            near = [state | {"gas": amount} for amount in amounts]
            self.assertEqual(
                [
                    caller_entry.replay(case, sample_pc, executable, 30)["gas"]
                    for case in near[1:3]
                ],
                [gas_fee - 1, gas_fee],
            )
            self.assertEqual(
                [
                    caller_entry.replay(case, call_pc, executable, 30)["gas"]
                    for case in near[-2:]
                ],
                [call_fee - 1, call_fee],
            )
            with self.assertRaisesRegex(Unsupported, "bytecode probe was not reached"):
                caller_entry.replay(near[-2], call_pc + 1, executable, 30)
            resumed = caller_entry.replay(near[-1], call_pc + 1, executable, 30)
            self.assertEqual((resumed["stack"][0], resumed["gas"]), ("0", 0))
            exits = [caller_entry.replay(case, None, executable, 30) for case in near]
            self.assertTrue(
                all(exit_[:4] == ["out-of-gas", [], 0, 0] for exit_ in exits)
            )
            gas_check = f"""
def checkGas (gas : Nat) : Except String Lean.Json := do
  let cut ← runBytes {report["entry_pc"]} 2048 tvWorldConfig {{initial with frame.gas := gas}}
  let terminal ← runByteExit tvWorldConfig 1024 cut
  let entered ← before cut
  let unpaid := unpaidHelperBudget (helperCallPrice mirCfg beforeFunctions cut entered)
  let physical := EVM.State.describe tvWorldConfig .exact (.ok (finishByte checkpoint terminal.result))
  let mir := EVM.State.describe tvWorldConfig .exact
    ((runMirExit mirCfg beforeFunctions .supply 1024 cut entered).map (finishByte checkpoint))
  return Lean.Json.arr #[Lean.toJson terminal.samples, Lean.toJson terminal.budgets,
    Lean.toJson terminal.unpaidCall, Lean.toJson unpaid, Lean.toJson physical, Lean.toJson mir]

def printGas (gas : Nat) : IO Unit :=
  match checkGas gas with
  | .ok result => IO.println result.compress
  | .error reason => throw (IO.userError reason)

"""
            site = callers.Site(**report["callers"][0])
            tables = list(map(builtin_lowering.functions, models))

            def check_gas(fixture, amounts):
                source = caller_entry.replay_source(
                    fixture,
                    runtime,
                    report["entry_pc"],
                    site,
                    tables,
                    SOLAR,
                    executable,
                    30,
                )
                reentry.write_text(
                    source.split("#eval")[0]
                    + gas_check
                    + f"#eval for gas in {amounts} do printGas gas\n"
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
                return list(map(json.loads, result.stdout.splitlines()))

            checks = check_gas(state, amounts)
            expected_samples = [
                [],
                [],
                [gas_fee],
                [gas_fee + call_fee - 1],
                [gas_fee + call_fee],
            ]
            self.assertEqual(len(checks), len(amounts))
            for index, (samples, paid, unpaid, predicted, physical, mir) in enumerate(
                checks
            ):
                self.assertEqual(samples, expected_samples[index])
                self.assertEqual(paid, [call_fee] if index == len(amounts) - 1 else [])
                self.assertEqual(predicted, {2: 0, 3: call_fee - 1}.get(index))
                self.assertEqual(
                    unpaid, [call_pc, predicted] if predicted is not None else None
                )
                self.assertEqual(json.loads(physical), exits[index])
                self.assertEqual(json.loads(mir), exits[index])

            # A paid CALL can exhaust its child's gas and leave its parent
            # enough gas to revert normally. MSTORE tries to expand memory at
            # an offset whose expansion charge exceeds the entire root budget.
            child = state | {
                "gas": 132000,
                "accounts": [
                    account | {"code": list(bytes.fromhex("5f63ffffffff52"))}
                    if int(account["address"]) == caller_replay.TOKEN
                    else account
                    for account in state["accounts"]
                ],
            }
            child_exit = caller_entry.replay(child, None, executable, 30)
            self.assertEqual(child_exit[0], "revert")
            self.assertIn(b"transferFrom reverted", bytes(child_exit[1]))
            after_child = caller_entry.replay(child, call_pc + 1, executable, 30)
            self.assertEqual(after_child["stack"][0], "0")
            self.assertGreater(after_child["gas"], 0)
            [(samples, paid, unpaid, predicted, physical, mir)] = check_gas(
                child, [child["gas"]]
            )
            self.assertEqual(paid, [samples[0] - gas_fee])
            self.assertIsNone(unpaid)
            self.assertIsNone(predicted)
            self.assertEqual(json.loads(physical), child_exit)
            self.assertEqual(json.loads(mir), child_exit)

            # An empty successful child returns all forwarded gas. Find the
            # ABI publication threshold independently on the emitted runtime.
            empty = state | {
                "accounts": [
                    account | {"code": [0]}
                    if int(account["address"]) == caller_replay.TOKEN
                    else account
                    for account in state["accounts"]
                ],
            }
            low, high = call_minimum, call_minimum + 4096
            while high - low > 1:
                middle = (low + high) // 2
                observation = caller_entry.replay(
                    empty | {"gas": middle}, None, executable, 30
                )
                if observation[0] == "success":
                    high = middle
                else:
                    low = middle
            publication = [empty | {"gas": amount} for amount in (low, high)]
            self.assertEqual(
                [
                    caller_entry.replay(case, site.return_pc, executable, 30)["gas"]
                    for case in publication
                ],
                [site.gas - 1, site.gas],
            )
            observations = [
                caller_entry.replay(case, None, executable, 30) for case in publication
            ]
            self.assertEqual(
                [row[0] for row in observations], ["out-of-gas", "success"]
            )
            for check, expected in zip(
                check_gas(empty, [low, high]), observations, strict=True
            ):
                self.assertEqual(
                    list(map(json.loads, check[-2:])), [expected, expected]
                )

            # The token with no code reaches a different revert handler.
            # Probe its entry independently before testing the exact gas edge.
            no_code = state | {
                "accounts": [
                    account | {"code": []}
                    if int(account["address"]) == caller_replay.TOKEN
                    else account
                    for account in state["accounts"]
                ],
            }
            error_entry = caller_entry.replay(
                no_code, report["no_code_pc"], executable, 30
            )
            self.assertGreaterEqual(len(error_entry["memory"]), 128)
            threshold = no_code["gas"] - error_entry["gas"] + 60
            amounts = [threshold - 1, threshold, threshold + 1]
            failed = [
                caller_entry.replay(no_code | {"gas": amount}, None, executable, 30)
                for amount in amounts
            ]
            self.assertEqual(
                [row[0] for row in failed], ["out-of-gas", "revert", "revert"]
            )
            self.assertEqual([row[2] for row in failed], [0, 0, 1])
            self.assertIn(b"no code", bytes(failed[-1][1]))
            for check, expected in zip(
                check_gas(no_code, amounts), failed, strict=True
            ):
                self.assertEqual(check[:4], [[], [], None, None])
                self.assertEqual(
                    list(map(json.loads, check[-2:])), [expected, expected]
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

    def check_symbolic_abi_prefix(self, output, runtime, report):
        directory = output / "entry"
        kernel = directory / "kernel"
        executable = native.default(SOLAR)
        for index, site in enumerate(report["callers"]):
            name = site["name"]
            path = report["abi_entry"]["paths"][name]
            evidence, decoder = path["dispatch"], path["decoder"]
            state = world.read(directory / f"{name}.world.json", SOLAR, "exact")
            accounts = {
                int(account["address"]): account for account in state["accounts"]
            }
            accounts[caller_replay.MORPHO]["code"] = list(runtime)
            accounts[caller_replay.MORPHO]["storage"] = []
            accounts[caller_replay.TOKEN]["code"] = [0]
            state["gas"] = evidence["gas"] + decoder["gas"] + index
            tag = site["selector"].to_bytes(4, "big")
            original = caller_replay.calldata(name)
            words = [
                int.from_bytes(original[start : start + 32], "big")
                for start in range(4, len(original) - 64, 32)
            ]
            split = 0 if name == "flashLoan" else 5
            market, parameters = words[:split], words[split:]
            if market:
                market[-1] += index + 1
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
            self.assertEqual(observed["gas"], decoder["gas"] + index)
            self.assertEqual(observed["stack"], [str(site["selector"])])
            self.assertEqual(len(observed["pcs"]), evidence["steps"])

            observed = caller_entry.replay(state, decoder["pc"], executable, 30)
            self.assertEqual(observed["gas"], index)
            start = 36 + head[-2]
            self.assertEqual(
                observed["stack"],
                list(
                    map(
                        str,
                        (
                            start,
                            len(payload),
                            128 if market else parameters[1],
                            site["selector"],
                        ),
                    )
                ),
            )
            arena = [0, 0, 288, 0, *market] if market else [0, 0, 128]
            self.assertEqual(
                bytes(observed["memory"]), b"".join(map(caller_replay.word, arena))
            )
            self.assertEqual(len(observed["pcs"]), decoder["steps"] + evidence["steps"])
            length_at = start - 32
            changed = state["calldata"].copy()
            changed[length_at:start] = list(caller_replay.word(len(input_) - start + 1))
            rejected_input = {**state, "calldata": changed, "gas": state["gas"] + 1000}
            with self.assertRaisesRegex(Unsupported, "bytecode probe was not reached"):
                caller_entry.replay(rejected_input, decoder["pc"], executable, 30)
            self.assertEqual(
                caller_entry.replay(rejected_input, None, executable, 30)[0], "revert"
            )

            prefix_gas = evidence["gas"] + decoder["gas"]
            for budget in (0, evidence["gas"] - 1, prefix_gas - 1):
                stopped = caller_entry.replay(
                    {**state, "gas": budget}, None, executable, 30
                )
                self.assertEqual(stopped[:4], ["out-of-gas", [], 0, 0])
                self.assertEqual(stopped[5], [])

            module = Path(decoder["proof"]).stem
            namespace = module.removesuffix("Decoder")
            address_count = (4 if market else 0) + 1
            cases = " | ".join("rfl" for _ in range(address_count))
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
        .ok (.inr (dispatched changedEnv checkpoint {decoder["gas"] + index})) :=
  symbolic_dispatch changedEnv checkpoint {decoder["gas"] + index}
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
theorem reachedVariant :
    Execution.Trace.segment (chunkedByteStep {report["entry_pc"]} tvWorldConfig code)
      {decoder["steps"]} {2048 - evidence["steps"]}
      (dispatched changedEnv checkpoint {decoder["gas"] + index}) =
        .ok (.inr (decodedFrame changedEnv checkpoint {index})) :=
  symbolic_decoder changedEnv checkpoint {index}
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by intro i h; simp only [List.mem_cons,List.not_mem_nil,or_false] at h
        rcases h with {cases} <;> decide +kernel)
theorem shortVariant :
    ∃ spent ≤ {evidence["steps"] + decoder["steps"]},
      EVM.State.execute {evidence["steps"] + decoder["steps"]} tvWorldConfig checkpoint
        (dispatchFrame changedEnv {prefix_gas - 1} 0 []) =
        .ok ⟨.outOfGas,[],checkpoint,0,spent⟩ :=
  abi_prefix_out_of_gas changedEnv checkpoint 0
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by intro i h; simp only [List.mem_cons,List.not_mem_nil,or_false] at h
        rcases h with {cases} <;> decide +kernel)
    (by decide +kernel) (by decide +kernel) 0 {prefix_gas - 1} (by decide)
theorem replayedVariant :
    (decodedFrame changedEnv checkpoint {index}).frame.stack = {list(map(int, observed["stack"]))} ∧
    (decodedFrame changedEnv checkpoint {index}).frame.memory = {observed["memory"]} := by
  decide +kernel
theorem memoryMatches :
    (decode selector caller changedCfg checkpoint).map (·.memory) =
      .ok (decodedFrame changedEnv checkpoint {index}).frame.memory :=
  decoded_mir_memory changedEnv checkpoint {index}
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by intro i h; simp only [List.mem_cons,List.not_mem_nil,or_false] at h
        rcases h with {cases} <;> decide +kernel)
    mirCfg (by decide +kernel)
theorem decodedVariant :
    mirStart changedCfg beforeFunctions beforeEntry (MIR.TransferFromDispatch.selector variant.tag)
      caller checkpoint mirGas =
    (do
      let frame ← entryFrame caller checkpoint variant.market
        ((parameterWidths caller).zip variant.parameters) (4 + variant.offset + 32) variant.payload.length
      let started ← MIR.State.start beforeFunctions beforeEntry frame.arguments frame
      return ⟨⟨{{function := beforeEntry}},[],started⟩,mirGas.samples,false,none⟩) :=
  start_encoding caller changedCfg checkpoint variant beforeFunctions beforeEntry mirGas
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by decide +kernel) (by decide +kernel)
end {namespace}
"""
            result = entry_proof.lean(
                kernel, module + "Variant", source, 45, module=bool(market)
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            if market:
                self.check_symbolic_market(
                    kernel,
                    namespace,
                    module,
                    path["market_read"],
                    state,
                    state["gas"] - index,
                    executable,
                )
            if name == "supplyCollateral":
                self.check_collateral_write(
                    kernel, path, state, state["gas"] - index, executable
                )

    def check_collateral_write(self, kernel, path, state, decode_gas, executable):
        proof = path["collateral_write"]
        self.assertTrue((kernel / Path(proof["proof"]).name).is_file())
        market = path["market_read"]
        state = {**state, "gas": 300_000, "warm_slots": []}
        owner = int(state["address"])
        market_cut = caller_entry.replay(state, market["load_pc"], executable, 30)
        market_key = market_cut["stack"][0]

        def with_position(
            value, position_key=None, warm=False, child=b"\x5a\x5f\x55\x00"
        ):
            slots = [[market_key, "1"]]
            if position_key is not None:
                slots.append([position_key, str(value)])
            return {
                **state,
                "warm_slots": [(str(owner), position_key)] if warm else [],
                "accounts": [
                    {**account, "storage": slots}
                    if int(account["address"]) == owner
                    else {**account, "code": list(child)}
                    if int(account["address"]) == caller_replay.CALLER
                    else {**account, "code": list(bytes.fromhex("60015f5260205ff3"))}
                    if int(account["address"]) == caller_replay.TOKEN
                    else account
                    for account in state["accounts"]
                ],
            }

        def storage(receipt, address=owner):
            account = next(
                account for account in receipt[4] if int(account[0]) == address
            )
            return dict(account[4])

        load = caller_entry.replay(with_position(0), proof["load_pc"], executable, 30)
        position_key = load["stack"][0]
        assets = int.from_bytes(bytes(state["calldata"][164:196]), "big")
        static_gas = (
            decode_gas
            + market["gas_before_load"]
            + market["guard"]["gas"]
            + proof["fixed_gas"]
        )
        for read_fee, write_fee, old in (
            (2100, 20000, 0),
            (100, 2900, (29 << 128) | 0xDEADBEEF),
        ):
            warmed = read_fee == 100
            fixture = with_position(old, position_key, warmed)
            gas = static_gas + 2100 + read_fee + write_fee
            keyed = {**fixture, "gas": gas}
            old_high, old_low = divmod(old, 1 << 128)
            packed = ((old_high + assets) << 128) | old_low
            before = caller_entry.replay(keyed, proof["store_pc"], executable, 30)
            self.assertEqual(before["stack"][:2], [position_key, str(packed)])
            self.assertEqual(before["gas"], write_fee)
            after = caller_entry.replay(keyed, proof["pc"], executable, 30)
            self.assertEqual(after["stack"], before["stack"][2:])
            self.assertEqual(after["memory"], before["memory"])
            self.assertEqual(after["gas"], 0)
            unpaid = caller_entry.replay(
                {**keyed, "gas": gas - 1}, None, executable, 30
            )
            self.assertEqual(unpaid[0], "out-of-gas")
            self.assertEqual(int(storage(unpaid).get(position_key, "0")), old)
            empty = fixture["calldata"].copy()
            empty[260:292] = [0] * 32
            entry_gas = gas + proof["empty_callback_gas"]
            paid = {**fixture, "calldata": empty, "gas": entry_gas}
            helper_cut = caller_entry.replay(paid, path["pc"], executable, 30)
            self.assertEqual(helper_cut["gas"], 0)
            self.assertEqual((helper_cut["samples"], helper_cut["budgets"]), ([], []))
            self.assertEqual(
                helper_cut["stack"][1:5],
                [
                    str(caller_replay.TOKEN),
                    str(caller_replay.CALLER),
                    str(owner),
                    str(assets),
                ],
            )
            short = {**paid, "gas": entry_gas - 1}
            with self.assertRaisesRegex(Unsupported, "bytecode probe was not reached"):
                caller_entry.replay(short, path["pc"], executable, 30)
            rolled_back = caller_entry.replay(short, None, executable, 30)
            self.assertEqual(rolled_back[0], "out-of-gas")
            self.assertEqual(int(storage(rolled_back).get(position_key, "0")), old)
            self.assertEqual(rolled_back[5], [])

            success = caller_entry.replay(fixture, None, executable, 30)
            self.assertEqual(success[0], "success")
            self.assertEqual(int(storage(success)[position_key]), packed)
            self.assertEqual(len(success[5]), 1)
            helper = caller_entry.replay(fixture, path["pc"], executable, 30)
            (budget,) = helper["budgets"]
            # EXTCODESIZE warms the zero-value CALL, whose input is already in memory.
            left = budget - 100
            forwarded = left - left // 64
            child_gas = int(storage(success, caller_replay.CALLER)["0"])
            self.assertEqual(child_gas, forwarded - 2)
            failed = caller_entry.replay(
                with_position(
                    old, position_key, warmed, bytes.fromhex("60015f526004601cfd")
                ),
                None,
                executable,
                30,
            )
            self.assertEqual(failed[0], "revert")
            self.assertEqual(bytes(failed[1]), b"\x00\x00\x00\x01")
            self.assertEqual(int(storage(failed).get(position_key, "0")), old)
            self.assertEqual(failed[5], [])

        high = ((1 << 128) - 1) << 128
        overflow = caller_entry.replay(
            with_position(high, position_key), None, executable, 30
        )
        self.assertEqual(overflow[0], "revert")
        self.assertEqual(
            bytes(overflow[1]), bytes.fromhex("4e487b71") + caller_replay.word(0x11)
        )
        self.assertEqual(int(storage(overflow)[position_key]), high)

    def check_symbolic_market(
        self, kernel, namespace, decoder_module, market, state, decode_gas, executable
    ):
        module = Path(market["proof"]).stem
        self.assertTrue(market["applied"])
        self.assertTrue((kernel / f"{module}.lean").is_file())
        load_gas = decode_gas + market["gas_before_load"]
        before = caller_entry.replay(
            {**state, "gas": load_gas}, market["load_pc"], executable, 30
        )
        self.assertEqual(before["gas"], 0)
        key = int(before["stack"][0])
        identity = int.from_bytes(bytes(before["memory"][:32]), "big")
        owner = int(state["address"])
        value = (1 << 128) + 19
        accounts = [
            {**account, "storage": [[str(key), str(value)]]}
            if int(account["address"]) == owner
            else account
            for account in state["accounts"]
        ]
        for warm, price in ((False, 2100), (True, 100)):
            world_state = {
                **state,
                "accounts": accounts,
                "warm_slots": [(str(owner), str(key))] if warm else [],
                "gas": load_gas + price,
            }
            reached = caller_entry.replay(world_state, market["pc"], executable, 30)
            self.assertEqual(reached["stack"], [str(value), *before["stack"][1:]])
            self.assertEqual(reached["memory"], before["memory"])
            self.assertEqual(reached["gas"], 0)
            underpaid = {**world_state, "gas": world_state["gas"] - 1}
            self.assertEqual(
                caller_entry.replay(underpaid, None, executable, 30)[0], "out-of-gas"
            )

        guard = market["guard"]
        guard_cuts = []
        for loaded, successor in (
            (0, "missing"),
            (1 << 128, "missing"),
            (value, "present"),
        ):
            guarded = {
                **state,
                "accounts": [
                    {**account, "storage": [[str(key), str(loaded)]]}
                    if int(account["address"]) == owner
                    else account
                    for account in accounts
                ],
                "warm_slots": [(str(owner), str(key))],
                "gas": load_gas + 100 + guard["gas"],
            }
            cut = caller_entry.replay(guarded, guard[successor], executable, 30)
            self.assertEqual(cut["pcs"][-1], guard["pc"])
            self.assertEqual(cut["gas"], 0)
            self.assertEqual(cut["memory"], before["memory"])
            guard_cuts.append((loaded, guard[successor], cut))
            underpaid = {**guarded, "gas": guarded["gas"] - 1}
            self.assertEqual(
                caller_entry.replay(underpaid, None, executable, 30)[0], "out-of-gas"
            )
            if successor == "missing":
                funded = {**guarded, "gas": guarded["gas"] + 1_000}
                rejected = caller_entry.replay(funded, None, executable, 30)
                self.assertEqual(rejected[0], "revert")
                self.assertIn(b"market not created", bytes(rejected[1]))

        guard_checks = "".join(
            f"""
theorem guardVariant{index} :
    let cut := (marketGuardCut changedEnv warm 0 {loaded}).frame
    (cut.pc,cut.gas,cut.stack) = ({pc},{cut["gas"]},{list(map(int, cut["stack"]))}) := by
  simp only [marketGuardCut,decoderFrame,sameId]
  decide +kernel
"""
            for index, (loaded, pc, cut) in enumerate(guard_cuts)
        )
        source = f"""import {module}
import {decoder_module}Variant
{entry_proof.HEADER}
namespace {namespace}
open MIR.TransferFromMarket
def testAccount : Execution.Account :=
  {{(Execution.account checkpoint changedEnv.address) with storage := [({key},{value})]}}
def cold : Execution.Journal :=
  Execution.putAccount {{checkpoint with warmSlots := []}} testAccount
def warm : Execution.Journal := Execution.warmSlot cold changedEnv.address {key}

theorem marketInput : MIR.TransferFromABI.wordArena (argumentWords changedEnv) =
    {before["memory"][128:]} := by decide +kernel
theorem sameId : marketId changedEnv = {identity} := by
  rw [marketId,marketHash,marketInput]
  decide +kernel
theorem sameSlot : marketSlot changedEnv = {key} := by
  change SolarTV.Keccak.hash (MIR.TransferFromABI.wordArena [marketId changedEnv,3]) + 2 = _
  rw [sameId]
  change SolarTV.Keccak.hash {before["memory"][:64]} + 2 = _
  decide +kernel
theorem coldPrice : marketPrice changedEnv cold = 2100 := by
  simp [marketPrice,sameSlot,cold,Execution.putAccount,SolarTV.Storage.readPrice,Fork.atLeast,Fork.ordinal]
theorem warmPrice : marketPrice changedEnv warm = 100 := by
  simp [marketPrice,sameSlot,warm,cold,Execution.putAccount,Execution.warmSlot,
    SolarTV.Storage.readPrice,Fork.atLeast,Fork.ordinal]
theorem readVariantValue :
    (readMarket changedEnv warm 0).frame.stack.head? = some {value} := by
  change some (marketValue changedEnv warm) = _
  rw [marketValue,sameSlot]
  decide +kernel
theorem readVariantMemory :
    (readMarket changedEnv warm 0).frame.memory = {before["memory"]} := by
  change MIR.TransferFromABI.wordArena ([marketId changedEnv,3,288,0] ++ argumentWords changedEnv) = _
  rw [sameId,MIR.TransferFromABI.wordArena_append,marketInput]
  decide +kernel
{guard_checks}end {namespace}
"""
        result = entry_proof.lean(kernel, module + "Variant", source, 60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
