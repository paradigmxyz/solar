"""Layout certificates prove jump safety using opcode-schema stack effects."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import native, regions
from validator.check import load


class StackLayoutTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/evmir/stack-layout"
        self.paths = [self.folder / f"{side}.evmir" for side in ("before", "after")]

    def cli(self, paths=None, *, world=False, flags=()):
        report = self.tmp / "report.json"
        command = [
            CLI,
            "evmir",
            *(paths or self.paths),
            "--solar",
            SOLAR,
            "--timeout=30",
            "--report",
            report,
            *flags,
        ]
        if world:
            command += ["--world", self.folder / "world.json", "--gas=exact"]
        elif (
            "--entry-trace" not in flags
            and "--world" not in flags
            and not any(flag.startswith("--region=") for flag in flags)
        ):
            command += ["--region=bb0"]
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=120
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_layout_proves_renaming_and_changed_growth(self):
        output = subprocess.check_output(
            [SOLAR, self.paths[0], "-j1", "-Ogas", "-Zevm-ir-pipeline=block-layout"],
            text=True,
        )
        self.assertEqual(
            regions.blocks(output), regions.blocks(self.paths[1].read_text())
        )
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-schema-stack-layout"
        )
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        evidence = record["import_correspondence"][0]
        self.assertNotEqual(
            evidence["source_to_target_nodes"], list(range(evidence["nodes"]))
        )
        self.assertEqual(len(evidence["changed_transfers"]), 2)
        self.assertTrue(
            all(row["height_bound"] <= 1023 for row in evidence["changed_transfers"])
        )
        self.assertEqual(evidence["source_height_bounds"][0], 1024)

    def test_changed_store_has_a_kernel_and_native_counterexample(self):
        broken = self.folder / "broken.evmir"
        for flags in ((), ("--entry-trace",)):
            with self.subTest(flags=flags):
                result, record = self.cli([self.paths[0], broken], flags=flags)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(record["status"], "unsupported")
        # Compare the same layout: the witness must expose the changed value,
        # rather than merely a legitimate difference in physical jump charges.
        result, record = self.cli([self.paths[1], broken], world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_entry_trace_checks_the_real_layout_with_an_empty_stack(self):
        result, record = self.cli(flags=("--entry-trace",))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["kind"], "entry")
        self.assertEqual(record["region"]["proof"], "kernel-checked-entry-layout")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        for side in record["import_correspondence"]:
            self.assertEqual(side["block_labels"], [0, 2, 4])
            self.assertEqual(side["entry_height_bound"], 0)
            self.assertEqual(len(side["changed_growth_checks"]), 2)
            self.assertLess(max(side["stack_height_bounds"]), 1024)

    def test_entry_trace_resolves_both_edges_of_stack_carried_jumpi(self):
        before, after, broken = (
            self.tmp / f"{name}.evmir" for name in ("before", "after", "broken")
        )
        entry = """@module DynamicBranch
bb0 [stack=0]:
  push bb3
  push 0
  calldataload
  swap 1
  jump bb1
"""
        branch = """bb1:
  jumpi
  push 11
  jump bb4
"""
        invalid = """bb2:
  invalid
"""
        body = """bb3:
  push 29
  jump bb4
bb4:
  push 0
  mstore
  push 32
  push 0
  return
"""
        before.write_text(entry + invalid + branch + body)
        after.write_text(entry + branch + invalid + body)
        broken.write_text(after.read_text().replace("push bb3", "push bb2"))
        result, record = self.cli([before, after], flags=("--entry-trace",))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "kernel-checked-entry-layout")
        self.assertEqual(
            [side["resolved_branches"] for side in record["import_correspondence"]],
            [1, 1],
        )
        result, record = self.cli([before, broken], flags=("--entry-trace",))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

        fixture = self.tmp / "destination-world.json"
        fixture.write_text('{"version":1,"calldata":"0x01"}')
        result, record = self.cli([after, broken], flags=("--world", fixture))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_region_layout_uses_proved_caller_return_slot(self):
        before, after = (self.tmp / f"{name}.evmir" for name in ("before", "after"))
        entry = """@module Caller
bb0 [stack=0]:
  push bb9
  jump bb1
bb1:
  push 0
  sload
  pop
  jump bb2
"""
        invalid = """bb3:
  invalid
"""
        helper_exit = """bb2:
  jump
"""
        epilogue = """bb9:
  stop
"""
        before.write_text(entry + invalid + helper_exit + epilogue)
        after.write_text(entry + helper_exit + invalid + epilogue)
        flags = ("--region=bb1", "--entry-stack")
        result, record = self.cli([before, after], flags=flags)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["name"], "bb1")
        self.assertEqual(record["region"]["proof"], "kernel-checked-entry-bound-layout")
        for evidence in record["import_correspondence"]:
            self.assertEqual(evidence["caller_return_slots"], [0])
            self.assertEqual(evidence["entry_height_bound"], 2)
            self.assertEqual(len(evidence["changed_growth_checks"]), 1)
        after.write_text(after.read_text().replace("bb2:\n  jump", "bb2:\n  stop"))
        result, record = self.cli([before, after], flags=flags)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_real_layout_checks_comparison_and_swapped_successors(self):
        before = ROOT.parent / "tests/ui/codegen/evm-ir/jumpi-fallthrough/equal.evmir"
        after = self.tmp / "after.evmir"
        output = subprocess.check_output(
            [
                SOLAR,
                before,
                "-j1",
                "-Ogas",
                "--evm-version=cancun",
                "-Zevm-ir-pipeline=jumpi-fallthrough",
            ],
            text=True,
            timeout=20,
        )
        after.write_text(output)
        result, record = self.cli([before, after], flags=("--entry-trace",))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "kernel-checked-entry-layout")
        self.assertEqual(
            [side["checked_branch_tests"] for side in record["import_correspondence"]],
            [1, 1],
        )
        broken = self.tmp / "broken.evmir"
        broken.write_text(output.replace("\n  sub\n", "\n  eq\n", 1))
        result, record = self.cli([before, broken], flags=("--entry-trace",))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

        fixture = self.tmp / "comparison-world.json"
        calldata = bytes(4) + (7).to_bytes(32, "big") + (8).to_bytes(32, "big")
        fixture.write_text(
            json.dumps({"version": 1, "calldata": "0x" + calldata.hex()})
        )
        result, record = self.cli([after, broken], flags=("--world", fixture))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_large_entry_layout_checks_every_row_in_batches(self):
        before, after = (self.tmp / f"{name}.evmir" for name in ("before", "after"))
        body = "".join(f"  push {word}\n  pop\n" for word in range(1, 1025))
        before.write_text(
            self.paths[0].read_text().replace("  sload\n", f"  sload\n{body}", 1)
        )
        after.write_text(
            subprocess.check_output(
                [
                    SOLAR,
                    before,
                    "-j1",
                    "-Ogas",
                    "--evm-version=cancun",
                    "-Zevm-ir-pipeline=block-layout",
                ],
                text=True,
                timeout=20,
            )
        )
        result, record = self.cli([before, after], flags=("--entry-trace",))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "kernel-checked-entry-layout")
        for evidence in record["import_correspondence"]:
            self.assertGreater(evidence["logical_actions"], 2048)
            self.assertEqual(len(evidence["changed_growth_checks"]), 2)

    def test_unproved_growth_is_not_hidden_by_an_entry_hint(self):
        paths = [self.tmp / f"{side}.evmir" for side in ("before", "after")]
        text = "@module Unsafe\nbb0 [stack=1]:\n sload\n jump bb2\n"
        paths[0].write_text(text + "bb2:\n stop\nbb1:\n stop\n")
        paths[1].write_text(text + "bb1:\n stop\nbb2:\n stop\n")
        for flags in ((), ("--stack-height=1",)):
            with self.subTest(flags=flags):
                result, record = self.cli(paths, flags=flags)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(record["status"], "unsupported")

    def test_dynamic_jump_is_a_full_live_payload_boundary(self):
        path = self.tmp / "dynamic.evmir"
        path.write_text("@module Exit\nbb0 [stack=1]:\n jump\n")
        result, record = self.cli([path, path])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-schema-stack-layout"
        )
        self.assertEqual(record["import_correspondence"][0]["dynamic_exits"], [0])
        self.assertTrue(
            any("shared logical continuation" in line for line in record["assumptions"])
        )

    def test_layout_does_not_claim_physical_gas(self):
        result, record = self.cli(flags=("--gas=exact",))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("physical transfer pricing", record["diagnostic"])

    def test_native_sufficient_gas_observations_preserve_values_and_effects(self):
        replay = native.default(SOLAR)
        for value, gas, static in (
            (0, 1000000, False),
            (1, 1000000, False),
            (0, 1, False),
            (0, 1000000, True),
        ):
            with self.subTest(value=value, gas=gas, static=static):
                world = self.tmp / "world.json"
                world.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "gas": gas,
                            "static": static,
                            "accounts": [
                                {"address": 65536, "storage": {"0": str(value)}}
                            ],
                        }
                    )
                )
                pair = [
                    load(
                        "evmir",
                        path,
                        SOLAR,
                        30,
                        world_path=world,
                        replay_executable=replay,
                    )
                    for path in self.paths
                ]
                observed = native.replay(*pair, [], executable=replay, timeout=30)
                self.assertEqual(
                    observed["observations"][0], observed["observations"][1]
                )

    def test_live_audit_keeps_bytecode(self):
        fallback = self.tmp / "code-observer.evmir"
        header, body = self.paths[0].read_text().split("\n", 1)
        fallback.write_text(
            f"{header}\nbb99 [stack=0]:\n  codesize\n  pop\n  jump bb0\n{body}"
        )
        env = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        env.pop("SOLAR_TV_GAS", None)
        for source, proof in (
            (self.paths[0], "kernel-checked-entry-layout"),
            (fallback, "kernel-checked-schema-stack-layout"),
        ):
            with self.subTest(source=source):
                folder = self.tmp / source.stem
                command = [
                    SOLAR,
                    source,
                    "-j1",
                    "-Ogas",
                    "-Zevm-ir-pipeline=block-layout",
                    "--emit=bin",
                ]
                outputs = [
                    subprocess.run(
                        [*command, *flags],
                        env=env,
                        capture_output=True,
                        text=True,
                        check=True,
                        timeout=120,
                    )
                    for flags in ([], [f"-Zsolar-tv={folder}"])
                ]
                self.assertEqual(outputs[0].stdout, outputs[1].stdout)
                records = [
                    json.loads(path.read_text())
                    for path in (folder / "audits").glob("*.json")
                ]
                verified = [r["region"] for r in records if r["status"] == "verified"]
                self.assertIn(proof, [r.get("proof") for r in verified])
                if source == self.paths[0]:
                    self.assertEqual(len(records), 1)
                    self.assertEqual(verified[0]["kind"], "entry")

    def test_kernel_rejects_forged_bounds_and_exhibits_overflow(self):
        path = self.tmp / "Bounds.lean"
        path.write_text("""import SolarTV
open SolarTV EVM.SourceGraph EVM.StackGraph
def before : Program := ⟨0,#[.operation 0 "sload" 1,.jump 0 0 2,.terminal 2 "jump"]⟩
def after : Program := ⟨0,#[.operation 0 "sload" 1,.jump 0 1 2,.terminal 2 "jump"]⟩
def signatures : Signatures := fun _ => some (1,1)
def bound : Nat → Nat := fun _ => 1024
def forged : Nat → Nat := fun i => if i = 1 then 1023 else 1024
example : ¬ Valid before after id bound signatures := by decide +kernel
example : ¬ Valid before after id forged signatures := by decide +kernel
def handlers : EVM.StackGraph.Handlers Nat (Nat × Nat) where
  operation := fun _ _ _ payload => .ok payload
  branch := fun _ _ payload => .ok (false,payload)
  terminal := fun _ _ height payload => (height,payload)
-- This is an abstract architectural state, not a claimed reachable source input.
example : execute before signatures handlers 3 0 ⟨1024,7⟩ = some (.halted (1024,7)) := by decide +kernel
example : execute after signatures handlers 3 0 ⟨1024,7⟩ = some .overflow := by decide +kernel
example : execute before signatures handlers 3 0 ⟨1025,7⟩ = some .overflow := by decide +kernel
#print axioms EVM.StackGraph.contextual
""")
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("sorryAx", result.stdout)
