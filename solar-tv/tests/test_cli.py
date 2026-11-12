"""End-to-end checks using the pass outputs of the compiler under test."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOLAR = os.environ.get("SOLAR", str(ROOT.parent / "target/debug/solar"))
CLI = str(ROOT / "solar-tv")


def check(layer, before, after):
    return subprocess.run(
        [CLI, layer, before, after, "--solar", SOLAR],
        text=True,
        check=False,
        capture_output=True,
        timeout=120,
    )


class PairTests(unittest.TestCase):
    def test_pass_outputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            for layer, example, transform, flag in [
                ("mir", "dce", "dce", "mir"),
                ("mir", "switch", "cfg-simplify", "mir"),
                ("mir", "phi", "cfg-simplify", "mir"),
                ("mir", "casts", "integer-cleanup", "mir"),
                ("mir", "checked", "check-elim", "mir"),
                ("mir", "typed-branch", "cfg-simplify", "mir"),
                ("evmir", "peephole", "peephole", "evm-ir"),
                ("evmir", "symbolic-stack", "peephole", "evm-ir"),
                ("mir", "memory", "memory-dse", "mir"),
                ("evmir", "memory", "peephole", "evm-ir"),
            ]:
                with self.subTest(example=example):
                    folder = ROOT / "examples" / layer / example
                    before = folder / f"before.{layer}"
                    after = Path(tmp) / f"after.{layer}"
                    output = subprocess.check_output(
                        [
                            SOLAR,
                            before,
                            "-Ogas",
                            f"-Z{flag}-pipeline={transform}",
                            "-Zvalidate-ir=true",
                            "-Zcheck-analyses=false",
                        ],
                        text=True,
                    )
                    # The first line includes the path of the source file.
                    self.assertEqual(
                        output.splitlines()[1:],
                        (folder / after.name).read_text().splitlines()[1:],
                    )
                    after.write_text(output)
                    result = check(layer, before, after)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, "verified\n")

    def test_recorded_facts_are_skipped(self):
        # Debug builds record the facts a pass relies on as assume lines.
        folder = ROOT / "examples/mir/checked"
        before = folder / "before.mir"
        with tempfile.TemporaryDirectory() as tmp:
            after = Path(tmp) / "after.mir"
            output = subprocess.check_output(
                [
                    SOLAR,
                    before,
                    "-Ogas",
                    "-Zmir-pipeline=check-elim",
                    "-Zvalidate-ir=true",
                    "-Zcheck-analyses=true",
                ],
                text=True,
            )
            self.assertIn("assume ", output)
            after.write_text(output)
            result = check("mir", before, after)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "verified\n")

    def test_bad_pairs(self):
        for layer, example, expected in [
            ("mir", "dce", "  arg0 = 0\n  before = 1\n  after  = 0"),
            ("mir", "phi", "  arg0 = 0\n  before = 11\n  after  = 21"),
            (
                "mir",
                "checked",
                f"  arg0 = {2**256 - 1}\n  before = revert Panic(17)\n  after  = 0",
            ),
            ("mir", "casts", "  arg0 = 1\n  before = 1\n  after  = " + str(2**256 - 1)),
            (
                "evmir",
                "peephole",
                "  before = fallthrough, stack=[]\n  after  = bb1, stack=[]",
            ),
        ]:
            folder = ROOT / "examples" / layer / example
            result = check(
                layer, folder / f"before.{layer}", folder / f"broken.{layer}"
            )
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn(expected, result.stdout)

    def test_effect_is_unsupported(self):
        before = ROOT / "examples/mir/dce/before.mir"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "write.mir"
            path.write_text(
                before.read_text().replace("ret v1", "sstore 0, v1\n    ret v1")
            )
            result = check("mir", path, before)
            self.assertEqual(result.returncode, 2)
            self.assertIn("unmodeled MIR instruction", result.stderr)

    def test_counterexample_away_from_zero(self):
        before = ROOT / "examples/mir/switch/before.mir"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bad.mir"
            path.write_text(before.read_text().replace("arg0 => bb1, ", ""))
            result = check("mir", before, path)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("arg0 = 5\n  before = 1\n  after  = 2", result.stdout)

    def test_live_stack_is_observable(self):
        folder = ROOT / "examples/evmir/peephole"
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, value in [("before", 1), ("after", 2)]:
                path = Path(tmp) / f"{name}.evmir"
                text = (folder / path.name).read_text()
                path.write_text(text.replace("bb0:\n", f"bb0:\n  push {value}\n"))
                paths.append(path)
            result = check("evmir", *paths)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("stack=[1]", result.stdout)
            self.assertIn("stack=[2]", result.stdout)

    def test_phi_incoming_order_is_irrelevant(self):
        before = ROOT / "examples/mir/phi/before.mir"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reordered.mir"
            path.write_text(
                before.read_text()
                .replace("[bb1: 1], [bb2: v0]", "[bb2: v0], [bb1: 1]")
                .replace("[bb1: 10], [bb2: 20]", "[bb2: 20], [bb1: 10]")
            )
            result = check("mir", before, path)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "verified\n")

    def test_loop_with_phis_is_inconclusive(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "loop.mir"
            path.write_text(
                "@module Loop\nfn @loop(arg0: i256) -> i256 {\n"
                "bb0:\n  jump bb1\n"
                "bb1:\n  v0 = phi [bb0: arg0], [bb1: v1]\n"
                "  v1 = sub v0, 1\n"
                "  switch v0, default bb1, [0 => bb2]\n"
                "bb2:\n  ret v0\n}\n"
            )
            result = check("mir", path, path)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("cyclic MIR control flow", result.stderr)

    def test_path_expansion_is_bounded(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "diamonds.mir"
            blocks = []
            for i in range(12):
                entry = 3 * i
                blocks.append(
                    f"bb{entry}:\n"
                    f"  switch arg0, default bb{entry + 1}, [0 => bb{entry + 2}]\n"
                    f"bb{entry + 1}:\n  jump bb{entry + 3}\n"
                    f"bb{entry + 2}:\n  jump bb{entry + 3}\n"
                )
            path.write_text(
                "@module Diamonds\nfn @f(arg0: i256) -> i256 {\n"
                + "".join(blocks)
                + "bb36:\n  ret arg0\n}\n"
            )
            result = check("mir", path, path)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("MIR path expansion limit", result.stderr)
