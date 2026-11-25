"""Allocation lowering before unchanged, possibly cyclic storage helpers."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import coalesce, native, prune
from validator.check import load
from validator.common import Unsupported


class AllocationLoweringTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/mir/allocation-lowering"
        self.before = self.folder / "before.mir"
        self.after = self.folder / "after.mir"
        self.replay = native.default(SOLAR)

    def pair(self, after=None, world=False):
        report = self.tmp / "audit.json"
        command = [
            CLI,
            "mir",
            self.before,
            after or self.after,
            "--solar",
            SOLAR,
            "--function=constructor",
            "--timeout=30",
            "--report",
            report,
        ]
        if world:
            command += ["--world", self.folder / "world.json"]
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=120
        )
        return result, json.loads(report.read_text())

    def test_real_lower_alloc_prefix_has_a_conditional_proof(self):
        output = subprocess.check_output(
            [SOLAR, self.before, "-j1", "-Ogas", "-Zmir-pipeline=lower-alloc"],
            text=True,
        )
        self.assertEqual(
            prune.declarations(output), prune.declarations(self.after.read_text())
        )
        result, report = self.pair()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["evidence"], "kernel-proof")
        self.assertEqual(report["region"]["proof"], "producer-valid-allocation-prefix")
        self.assertEqual(report["gas_policy"], "unpriced-structural")
        self.assertEqual(
            [p["lowered_reservations"] for p in report["import_correspondence"]],
            [0, 1],
        )
        self.assertFalse(report["whole_pass"])
        self.assertNotIn("world", report)

    def test_wrong_bump_store_has_an_independent_counterexample(self):
        broken = self.folder / "broken.mir"
        result, report = self.pair(broken)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(report["status"], "unsupported")
        result, report = self.pair(broken, world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_complete_execution_at_different_cursors_and_storage_states(self):
        for base, active in ((128, 96), (256, 352), (4096, 4096)):
            with self.subTest(base=base):
                memory = bytearray(b"\xa5" * active)
                memory[64:96] = base.to_bytes(32, "big")
                world = self.tmp / "world.json"
                world.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "memory": {"data": "0x" + memory.hex(), "size": active},
                            "accounts": [
                                {"address": 65536, "storage": {"3": "65", "4": "129"}}
                            ],
                        }
                    )
                )
                pair = [
                    load(
                        "mir",
                        path,
                        SOLAR,
                        30,
                        function="constructor",
                        world_path=world,
                        replay_executable=self.replay,
                    )
                    for path in (self.before, self.after)
                ]
                result = native.replay(*pair, [], executable=self.replay, timeout=30)
                self.assertEqual(result["observations"][0], result["observations"][1])
                state = json.loads(result["observations"][0])
                self.assertEqual(
                    int.from_bytes(bytes(state["memory"][64:96]), "big"), base + 128
                )
                self.assertEqual(len(state["memory"]), max(active, base + 128))
                self.assertEqual(len(state["warm_slots"]), 5)

    def test_near_miss_bumps_are_not_normalized(self):
        response = subprocess.check_output(
            [self.replay, "--mir-model"],
            input=json.dumps(
                {
                    "version": 1,
                    "text": self.after.read_text(),
                    "function": "constructor",
                }
            ),
            text=True,
        )
        model = json.loads(response)["model"]
        original = next(f for f in model["functions"] if f["id"] == model["entry_id"])[
            "blocks"
        ][1]["instructions"]
        # Address, width, increment, stored SSA value, and uninterrupted ordering
        # all participate in the primitive correspondence.
        for change in ("load", "width", "size", "stored", "order", "extra_alloc"):
            with self.subTest(change=change):
                changed = copy.deepcopy(model)
                instructions = next(
                    f for f in changed["functions"] if f["id"] == changed["entry_id"]
                )["blocks"][1]["instructions"]
                if change == "load":
                    instructions[0]["operation"]["args"][0]["value"] = "96"
                elif change == "width":
                    instructions[2]["type"]["bits"] = 160
                elif change == "size":
                    instructions[2]["operation"]["args"][1]["value"] = "127"
                elif change == "stored":
                    instructions[3]["operation"]["args"][1] = copy.deepcopy(
                        original[2]["operation"]["args"][0]
                    )
                elif change == "order":
                    instructions[2], instructions[3] = instructions[3], instructions[2]
                else:
                    instructions.insert(
                        4,
                        {
                            "operation": {
                                "kind": "allocate",
                                "checked": False,
                                "zeroed": False,
                                "args": [original[2]["operation"]["args"][1]],
                            },
                            "result": 999,
                            "type": original[1]["type"],
                        },
                    )
                with self.assertRaises(Unsupported):
                    coalesce.project(changed)

    def test_changed_continuation_is_not_hidden(self):
        text = self.after.read_text()
        for old, new in (
            ("icall @store_storage_bytes, 4,", "icall @store_storage_bytes, 5,"),
            ("sstore arg0, v25", "sstore arg0, 7"),
            ("mload 64", "heap_floor"),
        ):
            with self.subTest(change=old):
                changed = self.tmp / "changed.mir"
                changed.write_text(text.replace(old, new, 1))
                self.assertNotEqual(changed.read_text(), text)
                result, report = self.pair(changed)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(report["status"], "unsupported")

    def test_live_pass_keeps_output(self):
        command = [SOLAR, self.before, "-j1", "-Ogas", "-Zmir-pipeline=lower-alloc"]
        audit = self.tmp / "live"
        environment = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        output = [
            subprocess.run(
                [*command, *flags],
                env=environment,
                capture_output=True,
                text=True,
                check=True,
                timeout=90,
            )
            for flags in ([], [f"-Zsolar-tv={audit}"])
        ]
        self.assertEqual(output[0].stdout, output[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertTrue(records)
        self.assertTrue(all(r["status"] == "verified" for r in records))

    def test_unreachable_runtime_changes_are_outside_the_claim(self):
        sources = (self.before, self.after)
        self.before = self.tmp / "before.mir"
        self.after = self.tmp / "after.mir"
        for index, (path, source) in enumerate(
            zip((self.before, self.after), sources, strict=True)
        ):
            path.write_text(
                source.read_text()
                + f"\nfn @runtime_only() [entry] {{\nbb0:\n  sstore 55, {index}\n  stop\n}}\n"
            )
        result, report = self.pair()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(report["whole_pass"])
        for side in report["import_correspondence"]:
            self.assertEqual(side["unrelated_changed_bodies"], ["runtime_only"])
            self.assertEqual(
                side["unchanged_callees"],
                ["clear_storage_words", "store_storage_bytes"],
            )

    def test_word_wrap_requires_the_producer_contract(self):
        proof = self.tmp / "Wrap.lean"
        proof.write_text(
            """import SolarTV
open SolarTV EvmRules MIR.CoalesceAlloc MIR.LowerAllocPrefix
def base : Nat := 2^256 - 32
def state : Result := ⟨base, write (fun _ => 0) 64 (BitVec.ofNat 256 base),96⟩
example : (loweredReserve 64 state).cursor ≠ (run base [.reserve 64] state).cursor := by
  simp [loweredReserve,state,readCursor_write,base,run]
#print axioms MIR.LowerAllocPrefix.contextual
"""
        )
        result = subprocess.run(
            ["lake", "env", "lean", proof],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("sorryAx", result.stdout)

    def test_lowered_source_is_also_checked_in_reverse(self):
        self.before, self.after = self.after, self.before
        result, report = self.pair()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            [p["lowered_reservations"] for p in report["import_correspondence"]],
            [1, 0],
        )
