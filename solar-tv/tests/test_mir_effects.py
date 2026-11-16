"""Compositional requests retain effects, boundaries and source typing."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import mir_effects, mir_symbolic, native
from validator.common import Unsupported


class MirEffectTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.replay = native.default(SOLAR)

    def model(self, body):
        return mir_symbolic.translate(body, "f", self.replay, "cancun", 20)

    def cli(self, before, after, *args):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                before,
                after,
                "--solar",
                SOLAR,
                "--timeout",
                "30",
                "--function",
                "transferFrom",
                "--report",
                report,
                *args,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=100,
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_weth_width_pass(self):
        folder = ROOT / "examples/mir/weth-width-guard"
        emitted = self.directory / "emitted.mir"
        changed = subprocess.run(
            [SOLAR, folder / "before.mir", "-Ogas", "-Zmir-pipeline=spell-width-tests"],
            text=True,
            capture_output=True,
            check=False,
            timeout=20,
        )
        self.assertEqual(changed.returncode, 0, changed.stderr)
        emitted.write_text(changed.stdout)
        outputs = []
        for path in (emitted, folder / "after.mir"):
            result = subprocess.run(
                [SOLAR, path, "-Ogas", "-Zmir-pipeline=none"],
                text=True,
                capture_output=True,
                check=False,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            outputs.append(
                "\n".join(
                    line
                    for line in result.stdout.splitlines()
                    if not line.startswith(";")
                )
            )
        self.assertEqual(outputs[0], outputs[1])
        result, record = self.cli(folder / "before.mir", folder / "after.mir")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["region"]["proof"], "compositional-effect-tree")
        self.assertFalse(record["whole_pass"])
        self.assertEqual(record["gas_policy"], "semantic-effects")
        for side in record["import_correspondence"]:
            self.assertEqual(side["kind"], "independent-scalar-effect-tree")
            self.assertEqual(len(side["effect_tree_sha256"]), 64)
        self.assertNotIn("world", record)

    def test_broken_width_is_independently_replayed(self):
        folder = ROOT / "examples/mir/weth-width-guard"
        result, record = self.cli(
            folder / "before.mir",
            folder / "broken.mir",
            "--world",
            folder / "world.json",
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")

    def test_live_pass_keeps_output_and_records_proof_kind(self):
        source = ROOT / "examples/mir/weth-width-guard/before.mir"
        output = self.directory / "audit"
        command = [SOLAR, source, "-Ogas", "-Zmir-pipeline=spell-width-tests"]
        runs = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=100,
            )
            for flags in ([], [f"-Zsolar-tv={output}"])
        ]
        for result in runs:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(runs[0].stdout, runs[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (output / "audits").glob("*.json")
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["status"], "verified")
        self.assertEqual(records[0]["region"]["proof"], "compositional-effect-tree")
        self.assertEqual([path.name for path in output.iterdir()], ["audits"])

    def test_symbolic_addresses_and_effect_operands(self):
        before = """@module Effects
fn @f(arg0: i256) -> i256 {
bb0:
  v0 = add arg0, 0
  sstore v0, 7
  v1 = sload arg0
  ret v1
}
"""
        after = before.replace("sstore v0, 7", "sstore arg0, 7")
        pair = [self.model(source) for source in (before, after)]
        result = mir_effects.check(*pair, 20, self.replay)
        self.assertEqual(result.status, "verified")
        # Neither a changed value/key nor an omitted store is a scalar rewrite.
        for changed in (
            after.replace(", 7", ", 8"),
            after.replace("sstore arg0, 7", ""),
        ):
            with (
                self.subTest(source=changed),
                self.assertRaisesRegex(Unsupported, "effect proof did not close"),
            ):
                mir_effects.check(pair[0], self.model(changed), 20, self.replay)

    def test_effect_order_and_terminal_are_observable(self):
        source = """@module Effects
fn @f() {
bb0:
  sstore 0, 7
  sstore 0, 8
  stop
}
"""
        before = self.model(source)
        for changed in (
            source.replace("sstore 0, 7\n  sstore 0, 8", "sstore 0, 8\n  sstore 0, 7"),
            source.replace("stop", "revert 0, 0"),
        ):
            with (
                self.subTest(source=changed),
                self.assertRaisesRegex(Unsupported, "effect proof did not close"),
            ):
                mir_effects.check(before, self.model(changed), 20, self.replay)

    def test_undefined_and_unsupported_operations_do_not_disappear(self):
        definitions = {
            row["name"]: row for row in native.catalog(self.replay, 20)["evmir"]
        }
        for operation in ("v0 = mload undef i256", "v0 = alloc 32"):
            with self.subTest(operation=operation):
                program = self.model(
                    f"@module Effects\nfn @f() {{\nbb0:\n {operation}\n ret\n}}\n"
                )
                with self.assertRaises(Unsupported):
                    mir_effects.render(program.symbolic_mir["model"], definitions)
        gas = self.model(
            "@module Effects\nfn @f() -> i256 {\nbb0:\n v0 = gas\n ret v0\n}\n"
        )
        with self.assertRaises(Unsupported):
            mir_effects.render(gas.symbolic_mir["model"], definitions)


if __name__ == "__main__":
    unittest.main()
