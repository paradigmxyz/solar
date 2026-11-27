"""Exact scalar islands preserve shared state/gas contexts and early exits."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import interior, native, regions
from validator.check import load
from validator.common import Unsupported


class InteriorTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/evmir/interior-cse"
        self.paths = [self.folder / f"{name}.evmir" for name in ("before", "after")]

    def cli(self, paths=None, *, world=False, flags=()):
        report = self.tmp / "report.json"
        command = [
            CLI,
            "evmir",
            *(paths or self.paths),
            "--solar",
            SOLAR,
            "--report",
            report,
            "--timeout=30",
            *flags,
        ]
        command += (
            ["--world", self.folder / "world.json", "--gas=exact"]
            if world
            else ["--prefix", "bb0"]
        )
        result = subprocess.run(
            command, text=True, capture_output=True, check=False, timeout=120
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_block_cse_pair_preserves_exact_gas(self):
        output = subprocess.check_output(
            [SOLAR, self.paths[0], "-j1", "-Ogas", "-Zevm-ir-pipeline=block-cse"],
            text=True,
        )
        self.assertEqual(
            regions.blocks(output), regions.blocks(self.paths[1].read_text())
        )
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "exact")
        self.assertEqual(record["region"]["proof"], "universal-exact-scalar-island")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        self.assertNotIn("sufficient gas", record["assumptions"])
        self.assertTrue(
            all(
                p["shared_prefix_operations"] > 0
                for p in record["import_correspondence"]
            )
        )
        self.assertTrue(
            all(
                p["shared_suffix_operations"] == 1
                for p in record["import_correspondence"]
            )
        )

    def test_broken_constant_requires_a_reachable_witness(self):
        paths = [self.paths[0], self.folder / "broken.evmir"]
        result, record = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("no reachable counterexample established", result.stderr)
        reduced = [
            self.folder / f"counterexample-{side}.evmir"
            for side in ("before", "broken")
        ]
        result, record = self.cli(reduced, world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_exact_middle_can_precede_shared_cancun_recovery(self):
        paths = [self.tmp / f"{side}.evmir" for side in ("before", "after")]
        for path, op in zip(paths, ("push 32", "dup 1"), strict=True):
            path.write_text(
                f"@module Recovery\nbb0 [stack=0]:\n  push 32\n  {op}\n"
                "  push 0\n  push 0\n  push 1\n  gas\n  staticcall\n  pop\n  stop\n"
            )
        result, record = self.cli(paths, flags=("--evm-version=cancun",))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["region"]["proof"], "universal-exact-scalar-island")
        self.assertEqual(record["import_correspondence"][0]["cancun_recovery_calls"], 1)

        for path in paths:
            path.write_text(path.read_text().replace("push 1\n", "push 2\n"))
        result, record = self.cli(paths, flags=("--evm-version=cancun",))
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_native_execution_retains_state_gas_and_pre_island_failures(self):
        replay = native.default(SOLAR)
        initial = json.loads((self.folder / "world.json").read_text())
        for gas, static in (
            (1, False),
            (2300, False),
            (1000000, True),
            (1000000, False),
        ):
            with self.subTest(gas=gas, static=static):
                world = self.tmp / "world.json"
                world.write_text(json.dumps({**initial, "gas": gas, "static": static}))
                pair = [
                    load(
                        "evmir",
                        path,
                        SOLAR,
                        30,
                        world_path=world,
                        gas_mode="exact",
                        replay_executable=replay,
                    )
                    for path in self.paths
                ]
                result = native.replay(*pair, [], executable=replay, timeout=30)
                self.assertEqual(result["observations"][0], result["observations"][1])
        broken = load(
            "evmir",
            self.folder / "broken.evmir",
            SOLAR,
            30,
            world_path=world,
            gas_mode="exact",
            replay_executable=replay,
        )
        observed = native.replay(pair[0], broken, [], executable=replay, timeout=30)
        self.assertNotEqual(observed["observations"][0], observed["observations"][1])

    def test_changed_effects_guards_and_terminals_are_not_hidden(self):
        source = self.paths[1].read_text()
        for old, new in (
            ("sstore", "mstore"),
            ("log3", "log2"),
            ("  stop", "  invalid"),
            ("push bb1", "push bb0"),
        ):
            with self.subTest(change=old):
                changed = self.tmp / "changed.evmir"
                changed.write_text(source.replace(old, new, 1))
                result, record = self.cli([self.paths[0], changed])
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(record["status"], "unsupported")

    def test_cheaper_middle_cannot_cross_a_storage_sentry(self):
        paths = [self.tmp / f"{side}.evmir" for side in ("before", "after")]
        for path, middle in zip(
            paths,
            ("pop\nswap 1\npop\nswap 1\npop", "pop\nswap 2\npop\npop"),
            strict=True,
        ):
            path.write_text(
                "@module Sentry\nbb0 [stack=7]:\nsstore\n" + middle + "\nsstore\nstop\n"
            )
        result, record = self.cli(paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("gas-sensitive continuation", result.stderr)

    def test_context_admission_keeps_code_and_call_boundaries(self):
        catalog = {
            row["name"]: row
            for row in native.catalog(native.default(SOLAR), 30)["evmir"]
        }
        for context in (
            ["pc"],
            ["codesize"],
            ["extcodehash"],
            ["staticcall"],
            ["call"],
            ["jump"],
            ["push bb1"],
            ["push bb2", "jumpi"],
        ):
            with self.subTest(context=context), self.assertRaises(Unsupported):
                interior.context(context, {"bb0", "bb1"}, catalog, "cancun")
        interior.context(
            ["sload", "sstore", "gas", "log3", "push bb1", "jumpi"],
            {"bb1"},
            catalog,
            "cancun",
        )
        result, record = self.cli(flags=("--evm-version=amsterdam",))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("two-resource gas", record["diagnostic"])

    def test_live_unknown_height_audit_keeps_bytecode(self):
        source = self.tmp / "hook.evmir"
        text = (
            self.paths[0]
            .read_text()
            .replace("bb1", "bb2")
            .replace("bb0 [stack=0]:", "bb1:")
        )
        text = text.replace(
            "@module InteriorCSE",
            "@module InteriorCSE\nbb0 [stack=5]:\n  push bb1\n  swap 1\n  jump",
        )
        source.write_text(text)
        command = [
            SOLAR,
            source,
            "-j1",
            "-Ogas",
            "-Zevm-ir-pipeline=block-cse",
            "--emit=bin",
        ]
        folder = self.tmp / "audits"
        output = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                text=True,
                capture_output=True,
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
                and r.get("region", {}).get("proof") == "universal-exact-scalar-island"
                for r in records
            )
        )

    def test_kernel_retains_early_exits_and_exact_gas_observations(self):
        proof = self.tmp / "Context.lean"
        proof.write_text("""import SolarTV
open SolarTV EvmRules EVM
def ctx : Context := ⟨0,0,0,[], 0⟩
def before : List Op := [.push 32,.push 32]
def after : List Op := [.push 32,.dup 0]
def tail : Suffix.Tail := ⟨[],"stop",none⟩
def enter (gas : Nat) (_ : List String) : Suffix.PrefixResult Nat (Nat × Nat) :=
  .next ctx [] gas 7
-- A gas-sensitive observer deliberately distinguishes the sentry boundary.
def leave (_ : Suffix.Tail) (_ : Context) (_ : Stack) (gas world : Nat) : Nat × Nat :=
  (if gas ≤ 2300 then 0 else 1,world)
example : Suffix.scalar (enter 2306) leave (99,99) .cancun [] tail before = (0,7) := by decide +kernel
example : Suffix.scalar (enter 2306) leave (99,99) .cancun [] tail after = (0,7) := by decide +kernel
example : Suffix.scalar (enter 2307) leave (99,99) .cancun [] tail before = (1,7) := by decide +kernel
example : Suffix.scalar (enter 2307) leave (99,99) .cancun [] tail after = (1,7) := by decide +kernel
example : Suffix.scalar (fun _ => .stopped (42,8)) leave (99,99) .cancun [] tail before = (42,8) := rfl
example : Suffix.scalar (enter 5) leave (99,99) .cancun [] tail before = (99,99) := by decide +kernel
example : Suffix.scalar (enter 5) leave (99,99) .cancun [] tail after = (99,99) := by decide +kernel
#print axioms Suffix.scalar_contextual
""")
        result = subprocess.run(
            ["lake", "env", "lean", proof],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("sorryAx", result.stdout)

    def test_identical_blocks_keep_the_existing_prefix_proof(self):
        result, record = self.cli([self.paths[0], self.paths[0]])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "universal-stack-prefix")
