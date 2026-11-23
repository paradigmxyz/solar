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
from validator import compact_pushes, native, prefix, world
from validator.check import build_model
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

    def test_recipe_preserves_requests_across_an_internal_return(self):
        self.before.write_text(f"""@module CallRecipe
bb0 [stack=0]:
  push bb2
  jump bb1
bb1 [ret=0, inputs=1, outputs=0]:
  push {MAX}
  push 0
  mstore
  push 0
  push 0
  push 32
  push 0
  push 0
  push 0x30000
  gas
  call
  pop
  jump
bb2 [continuation]:
  push 32
  push 0
  return
""")
        after = self.compact("cancun")
        result, record = self.check("cancun", "--region=bb0")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        result, record = self.check("cancun", "--region=bb0", "--interaction-trace")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-compact-pushes-interaction-trace"
        )
        self.assertEqual(record["gas_policy"], "unpriced-shared-interactions")
        left, right = record["import_correspondence"]
        for side in (left, right):
            self.assertEqual(side["interactions"], ["call", "gas"])
            self.assertEqual(side["resolved_returns"], 1)
        self.assertEqual([side["recipes"][0]["gas"] for side in (left, right)], [3, 5])

        wrong = after.replace("  not\n", "  iszero\n")
        self.assertNotEqual(wrong, after)
        self.after.write_text(wrong)
        result, _ = self.check("cancun", "--region=bb0", "--interaction-trace")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        world = self.root / "world.json"
        world.write_text('{"version":1}')
        result, record = self.check("cancun", "--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

    def test_relocation_placeholder_cannot_be_a_recipe_literal(self):
        built = build_model(30)
        self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
        proof = self.root / "Relocation.lean"
        proof.write_text("""import SolarTV
open SolarTV EVM
def original : ProofTable.Tree ContinuationFlow.Node := .leaf [.pushLabel 1 1]
def points : ProofTable.Tree ContinuationFlow.Point := .leaf [⟨0,[]⟩]
def projected : ProofTable.Tree SourceGraph.Node := .leaf [.operation 0 "push 0" 1]
def relocated := ContinuationFlow.pushesLabel original.view points.view
example : relocated 0 = true := by decide +kernel
example : CompactPushes.scan projected.view .cancun relocated [.push 0] 0 = none :=
  by decide +kernel
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

    def test_physical_entry_supplies_stack_bound_and_return(self):
        self.before.write_text(f"""@module EntryRecipe
bb51:
  calldatasize
  push bb52
  jumpi
  push bb4
  jump bb3
bb52:
  push bb5
  jump bb3
bb3 [ret=0, inputs=1, outputs=0]:
  push 0x11223344{"00" * 28}
  dup 1
  push 0
  mstore
  pop
  jump
bb4 [continuation]:
  push 32
  push 0
  return
bb5 [continuation]:
  push 0
  push 0
  return
""")
        after = self.compact("cancun")
        result, record = self.check("cancun", "--region=bb3")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["kind"], "entry")
        self.assertEqual(record["region"]["name"], "bb51")
        for side in record["import_correspondence"]:
            self.assertEqual(side["block_labels"], [3, 4, 5, 51, 52])
            self.assertEqual(side["entry_height_bound"], 0)
            self.assertEqual(side["resolved_returns"], 2)
            self.assertLess(side["max_projected_stack_bound"], 1024)
        result, record = self.check("cancun", "--region=bb3", "--entry-stack")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for side in record["import_correspondence"]:
            self.assertEqual(side["caller_return_slots"], [0])
            self.assertEqual(side["inherited_returns"], 1)
        wrong = after.replace("push 224\n  shl", "push 223\n  shl")
        self.assertNotEqual(wrong, after)
        self.after.write_text(wrong)
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("--entry-trace", record["diagnostic"])
        world = self.root / "world.json"
        world.write_text('{"version":1}')
        result, record = self.check("cancun", "--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

        wrong_return = after.replace("  pop\n  jump\n", "  pop\n  pop\n  jump bb4\n")
        self.assertNotEqual(wrong_return, after)
        self.after.write_text(wrong_return)
        result, record = self.check("cancun", "--region=bb3", "--entry-stack")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        world.write_text('{"version":1,"calldata":"0xaa"}')
        result, record = self.check("cancun", "--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

    def test_entry_trace_preserves_immutable_slot_and_width(self):
        self.before.write_text(f"""@module ImmutableRecipe
bb5:
  push_immutable 3, 32
  push {MAX}
  and
  push 0
  mstore
  push 32
  push 0
  return
""")
        after = self.compact("cancun")
        result, _ = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for change in ("push_immutable 4, 32", "push_immutable 3, 20", "push 3"):
            with self.subTest(change=change):
                self.after.write_text(after.replace("push_immutable 3, 32", change))
                result, record = self.check("cancun", "--entry-trace")
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn(
                    "changes a reachable nonliteral action", record["diagnostic"]
                )

    def test_region_uses_bounds_from_both_callers(self):
        self.before.write_text(f"""@module CallerStack
bb10:
  push 42
  jump bb8
bb8:
  push 0x11223344{"00" * 28}
  pop
  stop
""")
        after = self.compact("cancun")
        result, record = self.check("cancun", "--region=bb8")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        result, record = self.check("cancun", "--region=bb8", "--entry-stack")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-compact-pushes-entry-bound"
        )
        left, right = record["import_correspondence"]
        self.assertEqual(left["caller_stack"]["max_height"], 2)
        self.assertEqual(right["caller_stack"]["max_height"], 3)

        grown = after.replace("  push 42\n", "  push 42\n" * 1023)
        self.assertNotEqual(grown, after)
        self.after.write_text(grown)
        result, record = self.check("cancun", "--region=bb8", "--entry-stack")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("may overflow", record["diagnostic"])

    def test_address_fold_checks_each_reaching_stack_definition(self):
        source = """@module CarriedAddress
bb0:
  calldatasize
  push bb2
  jumpi
  push 128
  jump bb3
bb2:
  push 128
  jump bb3
bb3:
  push 32
  dup 2
  add
  push 0
  mstore
  pop
  push 32
  push 0
  return
"""
        self.before.write_text(source)
        folded = source.replace("  push 32\n  dup 2\n  add\n", "  push 160\n")
        self.after.write_text(folded)
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["import_correspondence"][0]["address_folds"], 1)

        self.after.write_text(folded.replace("  push 160\n", "  push 159\n"))
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        world = self.root / "world.json"
        world.write_text('{"version":1}')
        result, record = self.check("cancun", "--world", world)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

        for path, text in ((self.before, source), (self.after, folded)):
            path.write_text(text.replace("bb2:\n  push 128\n", "bb2:\n  push 129\n"))
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("unproved stack word", record["diagnostic"])

    def test_constructor_reads_bind_to_relocated_data_and_arguments(self):
        self.before.write_text(f"""@module ConstructorReads
@data
  blob_0: hex"11223344"

bb0:
  push_data blob_0+4
  codesize
  sub
  dup 1
  push_data blob_0+4
  push 64
  codecopy
  pop
  push_data blob_0+4
  codesize
  sub
  pop
  push {MAX}
  pop
  push 4
  dup 1
  push_data blob_0
  push 0
  codecopy
  pop
  push 68
  push 0
  return
""")
        after = self.compact("cancun")
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        left, right = [side["code_layout"] for side in record["import_correspondence"]]
        self.assertEqual(left["declared_data_sha256"], right["declared_data_sha256"])
        self.assertNotEqual(left["argument_position"], right["argument_position"])
        self.assertEqual(
            left["relocated_reads"],
            ["copy-code-data 0 4 0", "copy-init-tail 64", "size-init-tail"],
        )
        fixture_path = self.root / "world.json"
        fixture_path.write_text('{"version":1}')
        fixture = world.read(fixture_path, SOLAR, None)
        arguments = bytes.fromhex("55667788")
        for text in (self.before.read_text(), after):
            image = world.runtime(text, "evmir", SOLAR, "cancun", 30)
            program = Program(
                ("world-entry", "evmir"),
                [],
                "",
                "",
                world=world.with_runtime(fixture, image + arguments),
                evm_version="cancun",
            )
            observed = native.replay(
                program,
                program,
                [],
                executable=self.replay,
                timeout=30,
                raw_runtime=True,
            )
            self.assertEqual(
                json.loads(observed["observations"][0])[1],
                [0x11, 0x22, 0x33, 0x44] + [0] * 60 + list(arguments),
            )
        wrong = after.replace(
            "  push_data blob_0\n  push 0\n", "  push_data blob_0\n  push 1\n"
        )
        self.assertNotEqual(wrong, after)
        self.after.write_text(wrong)
        result, record = self.check("cancun", "--entry-trace")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        result, record = self.check("cancun", "--world", fixture_path)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

    def test_kernel_rejects_an_understated_caller_bound(self):
        checked = build_model(30)
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        proof = self.root / "Height.lean"
        proof.write_text("""import SolarTV
open SolarTV.EVM StackReachability
def rows : Array SourceGraph.Node := #[.operation 0 "push 42" 1, .terminal 0 "stop"]
def graph := ProofTable.ofArray rows
def signatures : StackGraph.Signatures :=
  fun name => if name == "push 42" then some (0,1) else none
def heights (index : Nat) := if index = 0 then 0 else 1
example : (List.range graph.size).all (closedAt graph signatures heights 1) = true :=
  by decide +kernel
example : closedAt graph signatures (fun _ => 0) 0 0 = false := by decide +kernel
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
