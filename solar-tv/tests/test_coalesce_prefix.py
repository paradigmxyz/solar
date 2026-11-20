"""Allocation-prefix correspondence without assuming cyclic callees succeed."""

import copy
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import coalesce, native, prune
from validator.check import load


class CoalescePrefixTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/mir/coalesce-allocs"
        self.paths = [self.folder / f"{side}.mir" for side in ("before", "after")]

    def cli(self, paths=None, world=None):
        report = self.tmp / "audit.json"
        command = [
            CLI,
            "mir",
            *(paths or self.paths),
            "--function",
            "constructor",
            "--solar",
            SOLAR,
            "--report",
            report,
            "--timeout",
            "45",
            "--verbose",
        ]
        if world is not None:
            path = self.tmp / "world.json"
            path.write_text(json.dumps(world))
            command += ["--world", path]
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=150
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_pass_and_conditional_certificate(self):
        emitted = subprocess.run(
            [SOLAR, self.paths[0], "-Ogas", "-Zmir-pipeline=coalesce-allocs"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(
            prune.declarations(emitted.stdout),
            prune.declarations(self.paths[1].read_text()),
        )
        result, report = self.cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["evidence"], "kernel-proof")
        self.assertEqual(report["region"]["proof"], "producer-valid-allocation-prefix")
        self.assertEqual(report["gas_policy"], "unpriced-structural")
        self.assertEqual(report["import_correspondence"][0]["reserved_bytes"], 128)
        self.assertFalse(report["whole_pass"])
        self.assertNotIn("world", report)
        self.assertTrue(
            any(
                "not full-state" in line and "producer contract" in line
                for line in report["assumptions"]
            )
        )

    def test_complete_execution_with_multiple_cursors_and_dirty_memory(self):
        for base, active in ((128, 96), (256, 352), (512, 640)):
            with self.subTest(base=base):
                memory = bytearray(b"\xa5" * active)
                memory[64:96] = base.to_bytes(32, "big")
                world = {
                    "version": 1,
                    "memory": {"data": "0x" + memory.hex(), "size": active},
                }
                if base == 512:
                    world["accounts"] = [
                        {"address": 65536, "storage": {"3": "65", "4": "129"}}
                    ]
                result, report = self.cli(world=world)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(report["evidence"], "kernel-proof")
                self.assertEqual(report["gas_policy"], "semantic-effects")

    def test_large_cursor_with_independent_source_replay(self):
        # The universal theorem covers arbitrary memory; use source replay for
        # the larger concrete heap instead of duplicating a large kernel term.
        memory = bytearray(b"\xa5" * 4096)
        memory[64:96] = (4096).to_bytes(32, "big")
        initial = self.tmp / "large.json"
        initial.write_text(
            json.dumps(
                {
                    "version": 1,
                    "memory": {"data": "0x" + memory.hex(), "size": 4096},
                    "accounts": [
                        {"address": 65536, "storage": {"3": "65", "4": "129"}}
                    ],
                }
            )
        )
        replay = str(Path(SOLAR).with_name("solar-tv-replay"))
        pair = [
            load(
                "mir",
                path,
                SOLAR,
                30,
                function="constructor",
                world_path=initial,
                replay_executable=replay,
            )
            for path in self.paths
        ]
        result = native.replay(*pair, [], executable=replay, timeout=30)
        self.assertEqual(result["observations"][0], result["observations"][1])
        state = json.loads(result["observations"][0])
        self.assertEqual(len(state["memory"]), 4096 + 128)
        self.assertEqual(
            int.from_bytes(bytes(state["memory"][64:96]), "big"), 4096 + 128
        )
        self.assertEqual(len(state["warm_slots"]), 5)

    def test_broken_initializer_is_not_hidden_by_the_cut(self):
        paths = [self.paths[0], self.folder / "broken.mir"]
        result, report = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(report["status"], "unsupported")
        initial = json.loads((self.folder / "world.json").read_text())
        result, report = self.cli(paths, initial)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_changed_callee_and_call_arguments_are_rejected(self):
        source = self.paths[1].read_text()
        for old, new in (
            ("sstore arg0, v25", "sstore arg0, 7"),
            ("icall @store_storage_bytes, 4,", "icall @store_storage_bytes, 5,"),
        ):
            with self.subTest(change=old):
                changed = self.tmp / "changed.mir"
                changed.write_text(source.replace(old, new))
                self.assertNotEqual(changed.read_text(), source)
                result, report = self.cli([self.paths[0], changed])
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(report["status"], "unsupported")

    def test_checked_zeroing_and_intermediate_observers_are_not_admitted(self):
        for fragment in (
            "alloc checked 64",
            "alloc zeroed 64",
            "alloc 64\n v999 = mload 64",
        ):
            with self.subTest(fragment=fragment):
                before = self.tmp / "before.mir"
                before.write_text(
                    self.paths[0].read_text().replace("alloc 64", fragment, 1)
                )
                result, report = self.cli([before, self.paths[1]])
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(report["status"], "unsupported")

    def test_recursive_reentry_is_not_an_unchanged_continuation(self):
        paths = [self.tmp / f"{side}.mir" for side in ("before", "after")]
        for path, source in zip(paths, self.paths, strict=True):
            text = source.read_text()
            match = re.search(r"fn @store_storage_bytes[^\n]+\{", text)
            self.assertIsNotNone(match)
            head, body = text[: match.end()], text[match.end() :]
            body = body.replace("bb0:", "bb0:\n icall @constructor", 1)
            path.write_text(head + body)
        result, report = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(report["status"], "unsupported")

    def test_projection_rejects_unknown_ssa_and_out_of_range_stores(self):
        response = subprocess.run(
            [str(Path(SOLAR).with_name("solar-tv-replay")), "--mir-model"],
            input=json.dumps(
                {
                    "version": 1,
                    "text": self.paths[0].read_text(),
                    "function": "constructor",
                }
            ),
            capture_output=True,
            text=True,
            check=True,
        )
        model = json.loads(response.stdout)["model"]
        ranged = copy.deepcopy(model)
        entry = next(
            body for body in model["functions"] if body["id"] == model["entry_id"]
        )
        store = next(
            i
            for i in entry["blocks"][1]["instructions"]
            if i["operation"].get("name") == "mstore"
        )
        store["operation"]["args"][0]["index"] = 99999
        with self.assertRaisesRegex(ValueError, "unknown SSA"):
            coalesce.project(model)
        entry = next(
            body for body in ranged["functions"] if body["id"] == ranged["entry_id"]
        )
        address = next(
            i
            for i in entry["blocks"][1]["instructions"]
            if i["operation"].get("name") == "add"
        )
        address["operation"]["args"][1]["value"] = "96"
        with self.assertRaisesRegex(ValueError, "leaves its reservation"):
            coalesce.project(ranged)

    def test_live_pass_keeps_output_and_reports_the_contract(self):
        command = [SOLAR, self.paths[0], "-Ogas", "-Zmir-pipeline=coalesce-allocs"]
        audit = self.tmp / "live"
        environment = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        outputs = [
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
        self.assertEqual(outputs[0].stdout, outputs[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertTrue(records)
        self.assertTrue(
            all(
                record["status"] == "verified"
                and record["region"]["proof"] == "producer-valid-allocation-prefix"
                for record in records
            )
        )

    def test_kernel_axioms_and_fmp_alias_counterexample(self):
        path = self.tmp / "Laws.lean"
        path.write_text(
            """import SolarTV
open SolarTV EvmRules
open MIR.CoalesceAlloc
def first : List Step := [.reserve 32,.store 0 7,.reserve 32]
def fused : List Step := [.reserve 64,.store 0 7]
def initial : Result := ⟨64,write (fun _ => 0) 64 64,96⟩
example : readCursor (run 64 first initial).memory ≠ readCursor (run 64 fused initial).memory := by
  simp [first,fused,initial,run,readCursor_write]
"""
            + "\n".join(
                f"#print axioms MIR.CoalesceAlloc.{name}"
                for name in (
                    "contextual",
                    "normalize",
                    "readCursor_write",
                    "write_list",
                )
            )
        )
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=45,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for line in result.stdout.splitlines():
            if "does not depend on any axioms" in line:
                continue
            axioms = re.search(r"depends on axioms: \[(.*?)\]", line)
            self.assertIsNotNone(axioms, line)
            self.assertLessEqual(
                set(axioms[1].split(", ")),
                {"propext", "Quot.sound", "Classical.choice"},
            )


if __name__ == "__main__":
    unittest.main()
