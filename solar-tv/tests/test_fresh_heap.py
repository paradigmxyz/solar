"""Fresh-heap proofs retain the complete SSA suffix and expose their conditions."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import fresh_heap, mir_symbolic, native, prune
from validator.check import load
from validator.common import Unsupported


class FreshHeapTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/mir/fresh-heap"
        self.paths = [self.folder / f"{side}.mir" for side in ("before", "after")]
        self.replay = native.default(SOLAR)

    def cli(self, paths=None, world=None):
        report = self.tmp / "report.json"
        command = [
            CLI,
            "mir",
            *(paths or self.paths),
            "--function=name.body",
            "--solar",
            SOLAR,
            "--timeout=30",
            "--report",
            report,
        ]
        if world is not None:
            path = self.tmp / "world.json"
            path.write_text(json.dumps(world))
            command += ["--world", path]
        result = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=120
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def pair(self):
        return [
            load(
                "mir",
                path,
                SOLAR,
                30,
                function="name.body",
                replay_executable=self.replay,
            )
            for path in self.paths
        ]

    def fixture(self, floor=128, active=96):
        memory = bytearray(b"\xa5" * active)
        memory[64:96] = floor.to_bytes(32, "big")
        return {
            "version": 1,
            "memory": {"data": "0x" + memory.hex(), "size": active},
            "mir_layout": {"functions": {"name.body": {"heap_floor": floor}}},
        }

    def test_real_pass_has_a_complete_conditional_ssa_certificate(self):
        output = subprocess.check_output(
            [SOLAR, self.paths[0], "-j1", "-Ogas", "-Zmir-pipeline=lower-alloc"],
            text=True,
        )
        self.assertEqual(
            prune.declarations(output), prune.declarations(self.paths[1].read_text())
        )
        result, report = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["region"]["proof"], "producer-valid-fresh-heap-ssa")
        self.assertEqual(report["gas_policy"], "semantic-effects")
        self.assertFalse(report["whole_pass"])
        self.assertNotIn("world", report)
        self.assertEqual(report["import_correspondence"][0]["reserved_bytes"], 64)
        self.assertEqual(
            report["import_correspondence"][0]["kind"], "independent-typed-fresh-heap"
        )
        self.assertTrue(
            any("complete function" in line for line in report["assumptions"])
        )
        self.assertTrue(
            any("no proof that every caller" in line for line in report["assumptions"])
        )
        self.assertTrue(
            any("equals the live FMP" in line for line in report["assumptions"])
        )

    def test_broken_suffix_has_a_kernel_and_native_counterexample(self):
        paths = [self.paths[0], self.folder / "broken.mir"]
        result, report = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(report["status"], "unsupported")
        result, report = self.cli(paths, self.fixture())
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_missing_and_disagreeing_bindings_are_not_implicit_initialization(self):
        missing = self.fixture()
        del missing["mir_layout"]
        result, report = self.cli(world=missing)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(report["status"], "unsupported")
        # This is an explicit local-window input outside the producer contract,
        # not a claimed reachable input of the compiler's runtime entry.
        mismatch = self.fixture(256)
        mismatch["mir_layout"]["functions"]["name.body"]["heap_floor"] = 128
        result, report = self.cli(world=mismatch)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")

    def test_complete_executions_with_multiple_dirty_heaps(self):
        for floor, active in ((128, 96), (256, 352), (512, 640)):
            with self.subTest(floor=floor):
                world = self.tmp / "heap.json"
                world.write_text(json.dumps(self.fixture(floor, active)))
                pair = [
                    load(
                        "mir",
                        path,
                        SOLAR,
                        30,
                        function="name.body",
                        world_path=world,
                        replay_executable=self.replay,
                    )
                    for path in self.paths
                ]
                replay = native.replay(*pair, [], executable=self.replay, timeout=30)
                self.assertEqual(replay["observations"][0], replay["observations"][1])
                observation = json.loads(replay["observations"][0])
                self.assertEqual(observation["values"], [str(floor)])
                self.assertEqual(
                    int.from_bytes(bytes(observation["memory"][64:96]), "big"),
                    floor + 64,
                )
                self.assertEqual(
                    int.from_bytes(
                        bytes(observation["memory"][floor : floor + 32]), "big"
                    ),
                    13,
                )
                self.assertEqual(observation["returndata"], [])
        result, report = self.cli(world=self.fixture())
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(report["evidence"], "kernel-proof")

    def test_wrong_bump_and_return_cannot_hide_behind_the_contract(self):
        text = self.paths[1].read_text()
        for old, new in (
            ("add v3, 64", "add v3, 96"),
            ("mstore 64, v4", "mstore 96, v4"),
            ("ret v0", "v99 = ptradd v0, 32\n    ret v99"),
        ):
            with self.subTest(change=old):
                path = self.tmp / "broken.mir"
                path.write_text(text.replace(old, new))
                self.assertNotEqual(path.read_text(), text)
                result, report = self.cli([self.paths[0], path])
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(report["status"], "unsupported")

    def test_prefix_admission_rejects_zeroing_checked_and_empty_contracts(self):
        programs = self.pair()
        for field in ("zeroed", "checked"):
            changed = copy.deepcopy(programs[0])
            inst = changed.symbolic_mir["model"]["functions"][0]["blocks"][0][
                "instructions"
            ][0]
            inst["operation"][field] = True
            with self.assertRaisesRegex(Unsupported, "infallible uninitialized"):
                fresh_heap.extent(changed, programs[1])
        changed = copy.deepcopy(programs[0])
        inst = changed.symbolic_mir["model"]["functions"][0]["blocks"][0][
            "instructions"
        ][0]
        inst["operation"]["args"][0]["value"] = "65536"
        with self.assertRaisesRegex(Unsupported, "infallible uninitialized"):
            fresh_heap.extent(changed, programs[1])

    def test_cached_source_is_rechecked_before_using_the_new_route(self):
        a, b = self.pair()
        with self.assertRaisesRegex(Unsupported, "independent source export"):
            mir_symbolic.check(
                replace(a, body="untrusted cached execution"),
                b,
                30,
                replay_command="",
                replay_executable=self.replay,
                verbose=False,
            )

    def test_live_audit_keeps_bytecode(self):
        folder = self.tmp / "audit"
        command = [
            SOLAR,
            self.paths[0],
            "-j1",
            "-Ogas",
            "-Zmir-pipeline=lower-alloc,lower-memory-objects,lower-evm-shaped",
            "--emit=bin",
        ]
        results = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=120,
            )
            for flags in ([], [f"-Zsolar-tv={folder}"])
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (folder / "audits").glob("*.json")
        ]
        self.assertTrue(
            any(
                row["status"] == "verified"
                and row.get("region", {}).get("proof")
                == "producer-valid-fresh-heap-ssa"
                for row in records
            )
        )

    def test_kernel_proves_initialization_and_nonvacuity(self):
        path = self.tmp / "Contract.lean"
        path.write_text("""import SolarTV
open SolarTV EvmRules
example (frame : MIR.State.Frame) :
    ∃ memory, MIR.FreshHeap.Contract {frame with memory} (128 : Word) 64 :=
  MIR.FreshHeap.inhabited frame 64 (by decide)
example (frame : MIR.State.Frame) (floor : Word) (extent : Nat)
    (memory : MIR.Universal.MemoryValid frame) (active : 96 ≤ frame.memory.length)
    (lower : 128 ≤ floor.toNat) (aligned : floor.toNat % 32 = 0)
    (range : floor.toNat + extent ≤ 65536) :
    MIR.FreshHeap.Contract {frame with memory := Execution.writeBytes frame.memory 64 (Execution.wordBytes floor)} floor extent :=
  MIR.FreshHeap.initialized frame floor extent memory active lower aligned range
#print axioms MIR.FreshHeap.reservation
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
