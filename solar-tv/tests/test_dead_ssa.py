"""Cyclic DCE certificates keep all live data, effects and supplied callees."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import test_mir_state as state
from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import dead_ssa, native, prune
from validator.common import Unsupported


class DeadSsaTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.folder = ROOT / "examples/mir/cyclic-dce"
        self.paths = [self.folder / f"{side}.mir" for side in ("before", "after")]
        self.replay = native.default(SOLAR)

    def cli(self, paths=None, extra=()):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *(paths or self.paths),
                "--solar",
                SOLAR,
                "--function=f",
                "--timeout=30",
                "--report",
                report,
                *extra,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=90,
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def check(self, before, after):
        paths = [self.directory / f"{side}.mir" for side in ("before", "after")]
        for path, text in zip(paths, (before, after), strict=True):
            path.write_text(text)
        return dead_ssa.check(paths, SOLAR, self.replay, "cancun", 30, "f")

    def test_real_pass_and_cyclic_contextual_report(self):
        emitted = subprocess.run(
            [
                SOLAR,
                self.paths[0],
                "-j1",
                "-Ogas",
                "-Zmir-pipeline=dce",
                "-Zvalidate-ir=true",
            ],
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        self.assertEqual(
            prune.declarations(emitted.stdout),
            prune.declarations(self.paths[1].read_text()),
        )
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "cyclic-dead-ssa-simulation")
        self.assertEqual(len(record["region"]["erased_results"]), 3)
        self.assertEqual(record["region"]["blocks"], 4)
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertFalse(record["whole_pass"])
        self.assertEqual(
            record["import_correspondence"][0]["unchanged_callees"], ["sink"]
        )
        self.assertTrue(
            any(
                "every logical block-transfer bound" in item
                for item in record["assumptions"]
            )
        )
        self.assertFalse(
            any(item == "sufficient gas" for item in record["assumptions"])
        )
        self.assertNotIn("world", record)

    def test_broken_write_rejected_and_independently_replayed(self):
        broken = self.folder / "broken.mir"
        result, record = self.cli([self.paths[0], broken])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        _, witness = state.MirStateTests.pair(
            self,
            self.paths[0].read_text(),
            broken.read_text(),
            status=1,
        )
        self.assertEqual(witness["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )
        self.assertNotEqual(*witness["witness"]["observations"])

    def test_effects_traps_and_gas_cannot_disappear(self):
        before = self.paths[0].read_text()
        after = self.paths[1].read_text()
        for instruction in (
            "v0 = mload 65536",
            "v0 = sload 0",
            "v0 = checked_sub uint256, 0, 1",
            "v0 = gas",
        ):
            with self.subTest(instruction=instruction):
                changed = before.replace("v0 = add 1, 2", instruction)
                with self.assertRaises(Unsupported):
                    self.check(changed, after)
        # A checked underflow is an actual revert, not a dead pure definition.
        changed = before.replace("v0 = add 1, 2", "v0 = checked_sub uint256, 0, 1")
        _, witness = state.MirStateTests.pair(self, changed, after, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )

    def test_real_dce_erases_unused_slices_in_every_address_space(self):
        before = self.paths[0].read_text()
        slices = []
        for index, location in enumerate(("memory", "calldata", "returndata")):
            ptr, pair, head, length = range(10 + 4 * index, 14 + 4 * index)
            slices.extend(
                [
                    f"v{ptr} = inttoptr i256 {2**256 - 1} to ptr<{location}>",
                    f"v{pair} = make_{location}_slice v{ptr}, 32",
                    f"v{head} = extract_value slice<{location}>, v{pair}, 0",
                    f"v{length} = extract_value slice<{location}>, v{pair}, 1",
                ]
            )
        before = before.replace("v5 = sub v3, 2", "\n    ".join(slices))
        source = self.directory / "slices.mir"
        source.write_text(before)
        after = subprocess.check_output(
            [SOLAR, source, "-j1", "-Ogas", "-Zmir-pipeline=dce", "-Zvalidate-ir=true"],
            text=True,
            timeout=30,
        )
        optimized = self.directory / "optimized.mir"
        optimized.write_text(after)
        result, record = self.cli([source, optimized])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "cyclic-dead-ssa-simulation")
        self.assertEqual(len(record["region"]["erased_results"]), 14)

        # An invalid memory range is harmless until an operation accesses it.
        # DCE must not accept a pair that also removes the access.
        reads_memory = before.replace(
            "icall @sink, v2", "v30 = mload v12\n    icall @sink, v2"
        )
        with self.assertRaisesRegex(Unsupported, "trapping or stateful"):
            self.check(reads_memory, after)
        with self.assertRaisesRegex(Unsupported, "undefined operand"):
            self.check(
                before.replace(
                    "make_memory_slice v10, 32", "make_memory_slice v10, undef i256"
                ),
                after,
            )

    def test_slice_projection_in_a_retained_store_cannot_be_erased(self):
        before = self.paths[0].read_text()
        before = before.replace(
            "icall @sink, v2",
            "v10 = inttoptr i256 128 to ptr<memory>\n"
            "    v11 = make_memory_slice v10, v2\n"
            "    v12 = extract_value slice<memory>, v11, 1\n"
            "    icall @sink, v12",
        )
        after = self.paths[1].read_text().replace("icall @sink, v2", "icall @sink, v4")
        with self.assertRaisesRegex(Unsupported, "changed a retained action"):
            self.check(before, after)
        _, witness = state.MirStateTests.pair(self, before, after, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )

    def test_phi_backedge_and_kept_operands_remain_exact(self):
        before = self.paths[0].read_text()
        after = self.paths[1].read_text()
        for old, new in (
            ("[bb0: 0]", "[bb0: 1]"),
            ("[bb2: v4]", "[bb2: v2]"),
            ("lt v2, 4", "lt v2, 3"),
            ("icall @sink, v2", "icall @sink, v4"),
        ):
            with self.subTest(change=new):
                self.assertIn(old, after)
                with self.assertRaises(Unsupported):
                    self.check(before, after.replace(old, new))

    def test_module_and_supplied_callee_changes_rejected(self):
        before, after = [p.read_text() for p in self.paths]
        for old, new in (
            ("@module CyclicDce", "@module Other"),
            ("sstore 0, arg0", "sstore 1, arg0"),
            ("sstore 0, arg0", "v8 = icall @f"),
        ):
            with self.subTest(change=new), self.assertRaises(Unsupported):
                self.check(before, after.replace(old, new))
        for instruction in ("v8 = codesize", "v8 = gas"):
            changed = [
                text.replace("sstore 0, arg0", instruction + "\n    sstore 0, arg0")
                for text in (before, after)
            ]
            with self.subTest(observer=instruction), self.assertRaises(Unsupported):
                self.check(*changed)

    def test_alias_correspondence_checks_complete_resolved_data(self):
        models = [
            dead_ssa.export(
                subprocess.check_output([SOLAR, p, "-Zmir-pipeline=none"], text=True),
                "f",
                self.replay,
                30,
            )
            for p in self.paths
        ]
        bodies = [
            next(f for f in model["functions"] if f["id"] == model["entry_id"])
            for model in models
        ]
        aligned, dead, mapping = dead_ssa.align(*bodies)
        self.assertEqual(len(mapping), len(set(mapping.values())))
        self.assertFalse(set(dead) & set(mapping.values()))
        self.assertFalse(set(dead) & set(dead_ssa.reads(aligned)))
        wrong = copy.deepcopy(bodies[1])
        wrong["blocks"][1]["phis"][0]["incoming"][0][1]["value"] = "1"
        with self.assertRaisesRegex(Unsupported, "retained action, phi or transfer"):
            dead_ssa.align(bodies[0], wrong)
        # Repeated definitions cannot make an aliasing substitution injective.
        wrong = copy.deepcopy(bodies[1])
        wrong["blocks"][1]["instructions"][0]["result"] = wrong["blocks"][1]["phis"][0][
            "result"
        ]
        with self.assertRaisesRegex(Unsupported, "not injective"):
            dead_ssa.align(bodies[0], wrong)

    def test_kernel_rechecks_erasure_instead_of_trusting_alignment(self):
        render = dead_ssa.render
        calls = 0

        def corrupt(body, definitions):
            nonlocal calls
            calls += 1
            result = render(body, definitions)
            if calls == 2:
                result = result.replace(
                    '\\"kind\\":\\"return\\"', '\\"kind\\":\\"invalid\\"'
                )
            return result

        with (
            patch.object(dead_ssa, "render", side_effect=corrupt),
            self.assertRaisesRegex(Unsupported, "simulation did not close"),
        ):
            self.check(*(p.read_text() for p in self.paths))

    def test_live_audit_is_bytecode_neutral(self):
        args = [
            SOLAR,
            self.paths[0],
            "-j1",
            "-Ogas",
            "-Zmir-pipeline=dce,lower-evm-shaped",
            "--emit=bin",
        ]
        folder = self.directory / "audit"
        outputs = []
        for flags in ([], [f"-Zsolar-tv={folder}"]):
            result = subprocess.run(
                [*args, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=120,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            outputs.append(result.stdout)
        self.assertEqual(*outputs)
        records = [
            json.loads(path.read_text()) for path in (folder / "audits").glob("*.json")
        ]
        self.assertTrue(records, list(folder.rglob("*")))
        self.assertTrue(
            any(
                record.get("region", {}).get("proof") == "cyclic-dead-ssa-simulation"
                for record in records
            )
        )

    def test_kernel_rejects_live_phi_reads_and_covers_failure(self):
        code = """import SolarTV
open SolarTV
open MIR.DeadSSA
def block : Block := ⟨[⟨"pure",[],[1],true⟩,⟨"phi",[1],[2],false⟩],"loop",[]⟩
example : safe [1] block = true := by decide
"""
        path = self.directory / "Bad.lean"
        path.write_text(code)
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("false", result.stdout)
        # A failed effect after a deleted definition must still stop immediately.
        path.write_text("""import SolarTV
open SolarTV
open MIR.DeadSSA
def handlers : Handlers Nat Nat Nat := ⟨(fun _ _ _ => fun _ => 9),
  (fun _ _ _ _ => .error 17),(fun _ _ _ _ => .inr 0)⟩
def block : Block := ⟨[⟨"pure",[],[1],true⟩,⟨"store",[],[],false⟩],"loop",[]⟩
example : run handlers [block] 100 0 none (fun _ => 0) 0 = .stopped 17 := by rfl
example : run handlers [eraseBlock [1] block] 100 0 none (fun _ => 0) 0 = .stopped 17 := by rfl
#print axioms simulation
""")
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("sorryAx", result.stdout)


if __name__ == "__main__":
    unittest.main()
