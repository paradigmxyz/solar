"""Check gas admission on WETH CFG pass pairs and replay a dispatcher gas difference."""

import json
import sys
import tempfile
import unittest
from itertools import pairwise
from pathlib import Path

from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import (
    call_budget,
    caller_entry,
    entry_proof,
    evaluate,
    native,
    world,
)
from validator.check import build_model

FORK = "cancun"
ROOT_ACCOUNT = 0x10000
CALLER = 0x30000
AMOUNT = 17
GAS = 100_000
WARM_VALUE_CHARGE = 9100
PAID_SAMPLE = WARM_VALUE_CHARGE + 2
WITHDRAW = "2e1a7d4d"
# keccak256 of the packed caller and Solady's 12-byte balance-slot seed.
BALANCE_SLOT = "0xf1a8496ce45cb02d05cd26eda19aeb21f31c429a41e70666b1a6d436dcda84ea"
CALL_SEQUENCE = "\n  gas\n  call\n"
MODULE = "SolarTV.EVMIR.Transforms.SampledCall"


class SampledCallTests(unittest.TestCase):
    def test_weth_withdrawal_gas(self):
        with tempfile.TemporaryDirectory(prefix="solar-tv-weth-") as folder:
            self.check_weth(Path(folder))

    def check_weth(self, folder):
        case = next(
            row
            for row in json.loads((ROOT / "evaluation.json").read_text())["cases"]
            if row["name"] == "solady-weth"
        )
        inputs = evaluate.inputs(case)
        baseline, _ = evaluate.compile_input(SOLAR, inputs, folder)
        captured, _ = evaluate.compile_input(SOLAR, inputs, folder, capture=True)
        self.assertEqual(baseline, captured)

        pairs = {
            tuple(path.read_text() for path in row["paths"])
            for row in evaluate.captures(folder)
            if row["module"] == "WETH_runtime"
            and row["pass"] == "cfg-simplify-early"
            and all(CALL_SEQUENCE in path.read_text() for path in row["paths"])
        }
        self.assertTrue(pairs)
        fixture = folder / "world.json"
        fixture.write_text(
            json.dumps(
                {
                    "version": 1,
                    "address": ROOT_ACCOUNT,
                    "caller": CALLER,
                    "gas": GAS,
                    "calldata": f"0x{WITHDRAW}{AMOUNT:064x}",
                    "accounts": [
                        {
                            "address": ROOT_ACCOUNT,
                            "balance": 100,
                            "storage": {BALANCE_SLOT: 100, "0x5345cdf77eb68f44c": 100},
                        },
                        {
                            "address": CALLER,
                            "nonce": 1,
                            # Return the child's remaining gas after its GAS opcode.
                            "code": "0x5a5f5260205ff3",
                        },
                    ],
                }
            )
        )
        state = world.read(fixture, SOLAR, "exact")
        replay = native.default(SOLAR)
        catalog = {row["name"]: row for row in native.catalog(replay, 30)["evmir"]}

        def observe(text):
            image = world.runtime(text, "evmir", SOLAR, FORK, 30)
            decoded = list(call_budget.decode_runtime(image, catalog).items())
            (pc,) = (
                pc
                for (pc, gas), (_, call) in pairwise(decoded)
                if gas == ("gas", None) and call == ("call", None)
            )
            runtime = world.with_runtime(state, image)
            live = caller_entry.replay(runtime, pc, replay, 30)
            self.assertEqual(
                live["stack"][:6], [str(CALLER), str(AMOUNT), "0", "0", "0", "0"]
            )
            continuation = caller_entry.replay(runtime, pc + 2, replay, 30)
            self.assertEqual(continuation["stack"][0], "1")
            supplied = int.from_bytes(bytes(continuation["returndata"]), "big") + 2
            return image, pc, live["gas"], supplied

        # Distinct invocations of this pass rewrite different routes through the
        # dispatcher. Probe actual code to retain that distinction.
        matches, changes = [], []
        for before, after in sorted(pairs):
            self.assertNotEqual(before, after)
            left, right = observe(before), observe(after)
            if left[2] == right[2]:
                self.assertEqual(left[3], right[3])
                matches.append((left, right))
            else:
                changes.append((left, right))
        self.assertTrue(matches)
        self.assertTrue(changes)
        left, right = matches[0]
        longer, shorter = changes[0]
        self.assertEqual(shorter[2] - longer[2], 12)
        self.assertEqual(shorter[3] - longer[3], 12)

        short = GAS - left[2] + PAID_SAMPLE - 1
        for image, pc, _, _ in (left, right):
            runtime = world.with_runtime({**state, "gas": short}, image)
            call = caller_entry.replay(runtime, pc + 1, replay, 30)
            self.assertEqual(call["gas"], WARM_VALUE_CHARGE - 1)
            receipt = caller_entry.replay(runtime, None, replay, 30)
            self.assertEqual(receipt[:3], ["out-of-gas", [], 0])
        threshold = GAS - shorter[2] + PAID_SAMPLE
        outcomes = []
        for image, pc, sample, _ in (longer, shorter):
            runtime = world.with_runtime({**state, "gas": threshold}, image)
            cut = caller_entry.replay(runtime, pc, replay, 30)
            self.assertEqual(cut["gas"], sample - (GAS - threshold))
            outcomes.append(caller_entry.replay(runtime, None, replay, 30)[0])
        self.assertEqual(outcomes, ["out-of-gas", "success"])
        self.certify(folder, state, replay, left, right, longer, shorter)

    def certify(self, folder, state, replay, left, right, longer, shorter):
        before, old_pc, _, _ = left
        after, new_pc, _, _ = right
        model = build_model(60, MODULE)
        self.assertEqual(model.returncode, 0, model.stdout + model.stderr)
        config = world.configuration(SOLAR, FORK, state, 30, replay)
        source = f"""import {MODULE}
open SolarTV EvmRules EVM.State EVM.SampledCall
set_option maxRecDepth 16384
set_option maxHeartbeats 3000000
{config}
def before : List Execution.Byte := {list(before)}
def after : List Execution.Byte := {list(after)}
theorem gasSchema : available tvWorldConfig 0x5a = ⟨true,2,0,0,1⟩ := by decide +kernel
theorem callSchema : available tvWorldConfig 0xf1 = ⟨true,2600,0,7,1⟩ := by decide +kernel
theorem weth_pair (world : Execution.Journal) (frame : Frame) (target amount : Word)
    (rest : List Word) (stack : frame.stack = arguments target amount rest)
    (space : rest.length+6 < 1024) (bounded : frame.memory.length ≤ 65536)
    (word : frame.gas < 2^256) :
    admit tvWorldConfig world {{frame with code := before, pc := {old_pc}}} =
      admit tvWorldConfig world {{frame with code := after, pc := {new_pc}}} :=
  paired tvWorldConfig world frame before after {old_pc} {new_pc} target amount rest
    rfl gasSchema callSchema (by decide +kernel) (by decide +kernel)
    (by decide +kernel) (by decide +kernel) stack space bounded (by simp [tvWorldConfig]) word
#print axioms weth_pair

def journal : Execution.Journal := ⟨{world.accounts_expression(state)}, [], [{CALLER}], [], [], 0⟩
def env : World.Environment := {world.environment_expression(state)}
def cut (code : List Execution.Byte) (pc gas : Nat) : Frame :=
  ⟨env,code,pc,arguments {CALLER} {AMOUNT} [],[],[],gas⟩
theorem lowGas :
    (admit tvWorldConfig journal (cut before {old_pc} 1)).toOption = some (.failed .outOfGas) ∧
    (admit tvWorldConfig journal (cut after {new_pc} {PAID_SAMPLE - 1})).toOption = some (.failed .outOfGas) :=
  by decide +kernel
#print axioms lowGas
"""
        for i, (code, pc, sample, child) in enumerate((left, right, longer, shorter)):
            source += f"""
def replayImage{i} : List Execution.Byte := {list(code)}
theorem witness{i} :
    (admit tvWorldConfig journal (cut replayImage{i} {pc} {sample})).toOption =
      some (.entered {child - 2300} {child}) := by decide +kernel
#print axioms witness{i}
"""
        source += f"""
theorem thresholdDifference :
    (admit tvWorldConfig journal (cut replayImage2 {longer[1]} {PAID_SAMPLE - (shorter[2] - longer[2])})).toOption =
      some (.failed .outOfGas) ∧
    (admit tvWorldConfig journal (cut replayImage3 {shorter[1]} {PAID_SAMPLE})).toOption =
      some (.entered 0 2300) := by decide +kernel
#print axioms thresholdDifference
"""
        proof = entry_proof.lean(folder, "WethCall", source, 60)
        self.assertEqual(proof.returncode, 0, proof.stdout + proof.stderr)
        entry_proof.check_axioms(
            proof.stdout.splitlines(), source.count("#print axioms")
        )
