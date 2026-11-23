"""Check the compiler's real recipes, including their stack and gas boundaries."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import compact_pushes, native, prefix
from validator.common import Program

MAX = "0x" + "ff" * 32


class CompactPushesTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.before = self.root / "before.evmir"
        self.after = self.root / "after.evmir"
        self.replay = native.default(SOLAR)

    def command(self, fork):
        return [
            SOLAR,
            self.before,
            "-j1",
            "-Osize",
            f"--evm-version={fork}",
            "-Zevm-ir-pipeline=compact-pushes",
        ]

    def compact(self, fork):
        after = subprocess.check_output(self.command(fork), text=True, timeout=30)
        self.after.write_text(after)
        return after

    def check(self, fork, *flags):
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
                f"--evm-version={fork}",
                "--report",
                report,
                *flags,
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def set_word_output(self, word):
        self.before.write_text(f"""@module Compact
bb0 [stack=0]:
  push 0
  sload
  pop
  push {word}
  push 0
  mstore
  push 32
  push 0
  return
""")

    def test_live_word_and_fork_specific_charge(self):
        self.set_word_output(MAX)
        for fork, charge in (("london", 6), ("cancun", 5)):
            with self.subTest(fork=fork):
                self.compact(fork)
                result, record = self.check(fork, "--window=bb0", "--stack-height=0")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(
                    record["region"]["proof"], "kernel-checked-compact-pushes"
                )
                self.assertEqual(record["gas_policy"], "sufficient")
                recipes = [
                    side["recipes"][0] for side in record["import_correspondence"]
                ]
                self.assertEqual([item["gas"] for item in recipes], [3, charge])
                self.assertEqual([item["peak"] for item in recipes], [1, 1])
                self.assertEqual(
                    [item["word"] for item in recipes], [str(int(MAX, 16))] * 2
                )

    def test_wrong_word_replays_against_both_models(self):
        self.set_word_output("0x11223344" + "00" * 28)
        after = self.compact("cancun")
        wrong = after.replace("push 224\n  shl", "push 223\n  shl")
        self.assertNotEqual(wrong, after)
        self.after.write_text(wrong)
        result, record = self.check("cancun", "--window=bb0", "--stack-height=0")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

        world = self.root / "world.json"
        world.write_text('{"version":1}')
        result, record = self.check("cancun", "--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_adjacent_push_reaches_same_peak_at_all_entry_heights(self):
        self.set_word_output("0x11223344" + "00" * 28)
        self.compact("cancun")
        result, record = self.check("cancun", "--region=bb0")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for side in record["import_correspondence"]:
            self.assertEqual(side["entry_height_bound"], 1024)
            self.assertEqual(side["recipes"][0]["shared_pushes"], 1)
            self.assertEqual(side["recipes"][0]["peak"], 2)

    def test_emitted_recipe_overflows_the_real_stack_limit_fixture(self):
        fixture = (
            ROOT.parent / "tests/ui/codegen/evm-ir/compact-pushes/stack_limit.evmir"
        )
        self.before.write_text(fixture.read_text())
        fork = "cancun"
        after = self.compact(fork)
        result, record = self.check(fork, "--region=bb0", "--stack-height=1023")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("overflow the 1024-word stack", record["diagnostic"])

        catalog = {row["name"]: row for row in native.catalog(self.replay, 30)["evmir"]}
        exported = [
            prefix.export(text, "bb0", fork, self.replay, 30)["instructions"]
            for text in (self.before.read_text(), after)
        ]
        self.assertEqual([steps[-1] for steps in exported], ["pop", "pop"])
        recipes = [
            ",".join(
                compact_pushes.instruction(step, catalog, fork)[0]
                for step in steps[:-1]
            )
            for steps in exported
        ]
        word = compact_pushes.instruction(exported[0][0], catalog, fork)[
            0
        ].removeprefix(".push ")
        theorem = self.root / "Limit.lean"
        theorem.write_text(f"""import SolarTV
open SolarTV EvmRules EVM.CompactPushes
def before : List Instruction := [{recipes[0]}]
def after : List Instruction := [{recipes[1]}]
def ctx : EVM.Context := ⟨0,0,0,[],0⟩
theorem oldWord : summarize before [] = some ⟨[{word}],1⟩ := by decide +kernel
theorem newWord : summarize after [] = some ⟨[{word}],2⟩ := by decide +kernel
theorem beforeFits (incoming : EVM.Stack) (height : incoming.length = 1023) :
    EVM.Prefix.charged ctx .cancun (before.map Instruction.op) incoming 100 ≠ none := by
  rw [charged_words _ _ _ _ _ _ _ oldWord (by simp)]
  simp [height,charge,EVM.Prefix.cost,before,Instruction.op,EVM.Gas.pureCost]
theorem afterOverflows (incoming : EVM.Stack) (height : incoming.length = 1023) :
    EVM.Prefix.charged ctx .cancun (after.map Instruction.op) incoming 100 = none := by
  rw [charged_words _ _ _ _ _ _ _ newWord (by simp)]
  simp [height]
#print axioms beforeFits
#print axioms afterOverflows
""")
        checked = subprocess.run(
            ["lake", "env", "lean", theorem],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertNotIn("sorryAx", checked.stdout)
        programs = [
            Program(
                ("evm-window", "1023"),
                ["calldatasize", "returndatasize", *(f"stack{i}" for i in range(1023))],
                "",
                "",
                replay_ir=text,
                evm_version=fork,
            )
            for text in (self.before.read_text(), after)
        ]
        witness = native.replay(
            *programs, [0] * 1025, executable=self.replay, timeout=30
        )
        self.assertEqual(
            witness["observations"], ["success(empty)", "exception(stack-overflow)"]
        )

    def test_gas_observer_rejects_a_more_expensive_recipe(self):
        self.set_word_output(MAX)
        self.before.write_text(
            self.before.read_text().replace(
                f"  push {MAX}\n", f"  push {MAX}\n  pop\n  gas\n"
            )
        )
        self.compact("cancun")
        result, record = self.check("cancun", "--window=bb0", "--stack-height=0")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_checking_the_pass_keeps_bytecode(self):
        self.set_word_output(MAX)
        directory = self.root / "audit"
        base = [*self.command("cancun"), "--emit=bin-runtime"]
        outputs = [
            subprocess.check_output(
                [*base, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                text=True,
                timeout=90,
            )
            for flags in ([], [f"-Zsolar-tv={directory}"])
        ]
        self.assertEqual(*outputs)
        records = [
            json.loads(path.read_text())
            for path in (directory / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                record["status"] == "verified"
                and record.get("region", {}).get("proof")
                == "kernel-checked-compact-pushes"
                for record in records
            )
        )
