"""Graph identity retains all operations, edges and transfer stack checks."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import correspondence, evm_region, native, regions, source_identity
from validator.check import load
from validator.common import Unsupported


class SourceIdentityTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/evmir/graph-identity"
        self.paths = [self.folder / f"{side}.evmir" for side in ("before", "after")]
        self.replay = native.default(SOLAR)

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
        command += (
            ["--world", self.folder / "world.json", "--gas=exact"]
            if world
            else ["--region=bb0", "--stack-height=0"]
        )
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=120
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_layout_pair_has_an_explicit_structural_certificate(self):
        output = subprocess.check_output(
            [SOLAR, self.paths[0], "-j1", "-Ogas", "-Zevm-ir-pipeline=block-layout"],
            text=True,
        )
        self.assertNotEqual(
            list(regions.blocks(self.paths[0].read_text())),
            list(regions.blocks(output)),
        )
        self.assertEqual(
            regions.blocks(output), regions.blocks(self.paths[1].read_text())
        )
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["region"]["proof"], "universal-source-graph-identity")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertEqual(record["region"]["requested_stack_height"], 0)
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        evidence = record["import_correspondence"]
        self.assertEqual(evidence[0]["graph_sha256"], evidence[1]["graph_sha256"])
        self.assertEqual(evidence[0]["kind"], "independent-source-graph-identity")
        self.assertNotIn("sufficient gas", record["assumptions"])

    def test_changed_store_is_rejected_and_has_an_independent_witness(self):
        paths = [self.paths[0], self.folder / "broken.evmir"]
        result, record = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        result, record = self.cli(paths, world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_changed_jump_growth_is_not_normalized_away(self):
        paths = [self.tmp / f"{side}.evmir" for side in ("before", "after")]
        prefix = "@module Growth\nbb0 [stack=1]:\n sload\n jump bb2\n"
        paths[0].write_text(prefix + "bb2:\n stop\nbb1:\n stop\n")
        paths[1].write_text(prefix + "bb1:\n stop\nbb2:\n stop\n")
        with self.assertRaisesRegex(Unsupported, "graphs differ"):
            source_identity.check(paths, SOLAR, self.replay, "cancun", 30, "bb0")
        result, record = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_all_execution_bounds_include_stateful_cycles(self):
        text = "@module Cycle\nbb0 [stack=0]:\n push 0\n sload\n pop\n jump bb0\n"
        path = self.tmp / "cycle.evmir"
        path.write_text(text)
        result, record = self.cli([path, path])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "universal-source-graph-identity")
        self.assertTrue(
            any(
                "every initial state and execution bound" in line
                for line in record["assumptions"]
            )
        )

    def test_malformed_graph_data_and_code_observers_are_rejected(self):
        model = correspondence.export(
            self.paths[0].read_text(), "bb0", "cancun", self.replay, 30
        )
        catalog = {row["name"]: row for row in native.catalog(self.replay, 30)["evmir"]}
        for change in ("entry", "edge", "id", "label", "extra", "pc", "call"):
            with self.subTest(change=change):
                mutated = copy.deepcopy(model)
                if change == "entry":
                    mutated["entry"] = 1
                elif change == "edge":
                    mutated["nodes"][0]["node"]["next"] = len(mutated["nodes"])
                elif change == "id":
                    mutated["nodes"][0]["id"] = False
                elif change == "label":
                    mutated["nodes"][0]["label"] = -1
                elif change == "extra":
                    mutated["nodes"][0]["node"]["ignored"] = "effect"
                else:
                    mutated["nodes"][0]["node"]["operation"] = change
                with self.assertRaises(Unsupported):
                    source_identity.render(mutated, catalog, "cancun", "bb0")

    def test_live_layout_audit_keeps_bytecode(self):
        folder = self.tmp / "audits"
        command = [
            SOLAR,
            self.paths[0],
            "-j1",
            "-Ogas",
            "-Zevm-ir-pipeline=block-layout",
            "--emit=bin",
        ]
        output = [
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
        self.assertEqual(output[0].stdout, output[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (folder / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                r["status"] == "verified"
                and r.get("region", {}).get("proof")
                == "universal-source-graph-identity"
                for r in records
            )
        )

    def test_native_full_entries_preserve_effects_and_failures(self):
        for slot, gas, static in (
            (0, 1000000, False),
            (1, 1000000, False),
            (0, 1, False),
            (0, 1000000, True),
        ):
            with self.subTest(slot=slot, gas=gas, static=static):
                fixture = self.tmp / "world.json"
                fixture.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "gas": gas,
                            "static": static,
                            "accounts": [
                                {"address": 65536, "storage": {"0": str(slot)}}
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
                        world_path=fixture,
                        gas_mode="exact",
                        replay_executable=self.replay,
                    )
                    for path in self.paths
                ]
                observed = native.replay(*pair, [], executable=self.replay, timeout=30)
                self.assertEqual(
                    observed["observations"][0], observed["observations"][1]
                )

    def test_kernel_exposes_a_lost_transfer_check(self):
        path = self.tmp / "Growth.lean"
        path.write_text("""import SolarTV
open SolarTV EVM.SourceGraph
def fallthrough : Program := ⟨0,#[.jump 0 0 1,.terminal 0 "stop"]⟩
def transfer : Program := ⟨0,#[.jump 0 1 1,.terminal 0 "stop"]⟩
def handlers : Handlers Nat Nat where
  operation := fun _ _ _ state => .ok state
  jump := fun _ _ growth state => if state + growth > 1024 then .error 0 else .ok state
  branch := fun _ _ state => .ok (false,state)
  terminal := fun _ _ _ state => state
-- This is an abstract-handler counterexample, not a claimed reachable input.
example : Correspondence.evalGraph (graph fallthrough handlers) 2 0 1024 = some 1024 := by decide +kernel
example : Correspondence.evalGraph (graph transfer handlers) 2 0 1024 = some 0 := by decide +kernel
#print axioms EVM.SourceGraph.identity
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
        self.assertIn("does not depend on any axioms", result.stdout)

    def test_structural_identity_cannot_claim_physical_gas_pricing(self):
        result, record = self.cli(flags=("--gas=exact",))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("physical transfer pricing", record["diagnostic"])

    def test_changed_branches_terminals_and_shared_headers_still_fail(self):
        text = self.paths[1].read_text()
        for old, new, diagnostic in (
            ("push bb4", "push bb2", "graphs differ"),
            ("revert", "invalid", "graphs differ"),
            ("@module SourceIdentity", "@module Other", "shared declarations"),
        ):
            with self.subTest(change=old):
                changed = self.tmp / "changed.evmir"
                changed.write_text(text.replace(old, new, 1))
                self.assertNotEqual(changed.read_text(), text)
                with self.assertRaisesRegex(Unsupported, diagnostic):
                    source_identity.check(
                        [self.paths[0], changed],
                        SOLAR,
                        self.replay,
                        "cancun",
                        30,
                        "bb0",
                    )

    def test_failed_import_certificates_cannot_select_identity(self):
        path = self.tmp / "supported.evmir"
        path.write_text("@module Supported\nbb0:\n stop\n")
        with (
            patch.object(
                correspondence,
                "certify",
                side_effect=Unsupported("unmodeled EVM IR instruction: callvalue"),
            ),
            self.assertRaises(Unsupported) as error,
        ):
            load("evmir", path, SOLAR, 30, region="bb0")
        self.assertNotIsInstance(error.exception, evm_region.UnmodeledInstruction)
        with self.assertRaises(evm_region.UnmodeledInstruction):
            load("evmir", self.paths[0], SOLAR, 30, region="bb0", stack_height=0)
