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
        elif "--entry-trace" not in flags:
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
        folder = self.tmp / "audit"
        command = [
            SOLAR,
            self.paths[0],
            "-j1",
            "-Ogas",
            "-Zevm-ir-pipeline=block-layout",
            "--emit=bin",
        ]
        outputs = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=True,
                timeout=120,
            )
            for flags in ([], [f"-Zsolar-tv={folder}"])
        ]
        self.assertEqual(outputs[0].stdout, outputs[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (folder / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                r["status"] == "verified"
                and r.get("region", {}).get("proof")
                == "kernel-checked-schema-stack-layout"
                for r in records
            )
        )

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
