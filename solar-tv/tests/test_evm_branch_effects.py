"""Decoder CSE preserves conditional exits and independently replayed memory."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evm_effects, native
from validator.common import Unsupported


class BranchEffectTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.example = ROOT / "examples/evmir/decoder-branches"
        self.paths = [self.example / f"{side}.evmir" for side in ("before", "after")]

    def cli(self, paths, *flags):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--solar",
                SOLAR,
                "--report",
                report,
                "--timeout",
                "30",
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=100,
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_block_cse_and_all_conditional_exits(self):
        compiled = subprocess.run(
            [SOLAR, self.paths[0], "-Ogas", "-Zevm-ir-pipeline=block-cse"],
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )
        self.assertEqual(compiled.returncode, 0, compiled.stderr)
        normalize = lambda text: [
            line.strip()
            for line in text.splitlines()
            if line.strip() and not line.lstrip().startswith(";")
        ]
        self.assertEqual(
            normalize(compiled.stdout), normalize(self.paths[1].read_text())
        )
        for flags in (("--prefix", "bb0"), ("--window", "bb0", "--stack-height", "2")):
            with self.subTest(flags=flags):
                result, record = self.cli(self.paths, *flags)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    record["region"]["proof"], "universal-branching-effect-prefix"
                )
                self.assertIsNone(record["region"]["stack_height"])
                self.assertFalse(record["whole_pass"])
                self.assertNotIn("world", record)
                self.assertEqual(
                    record["import_correspondence"][0]["conditional_transfers"], 4
                )
                self.assertIn(
                    "universally quantified shared per-label",
                    " ".join(record["assumptions"]),
                )

    def entry(self, source, *, broken=False):
        body = source.read_text().split("bb0 [stack=2]:\n", 1)[1].split("bb1:", 1)[0]
        body = body.replace("push bb1", "push bb3")
        if broken:
            body = body.replace(
                "calldataload\n  dup 2\n  dup 3", "calldataload\n  push 129\n  dup 3"
            )
        path = self.directory / ("broken.evmir" if broken else source.name)
        path.write_text(
            "@module DecoderEntry\nbb0:\n  push 0\n  push bb1\n  jump bb2\n"
            "bb1:\n  push 256\n  mload\n  push 0\n  mstore\n  push 32\n  push 0\n  return\n"
            "bb2 [stack=2]:\n" + body + "bb3:\n  push 0\n  push 0\n  revert\n"
        )
        return path

    def test_broken_decoder_offset_is_replayed_independently(self):
        before, after = [self.entry(path) for path in self.paths]
        world = self.directory / "world.json"
        data = bytes(132) + (123).to_bytes(32, "big")
        world.write_text(
            json.dumps({"version": 1, "gas": 500000, "calldata": "0x" + data.hex()})
        )
        valid, _ = self.cli([before, after], "--world", world, "--gas", "refine")
        self.assertEqual(valid.returncode, 0, valid.stderr)
        broken = self.entry(self.paths[1], broken=True)
        result, record = self.cli([before, broken], "--world", world)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_changed_exit_cannot_claim_a_universal_proof(self):
        for old, new, diagnostic in (
            ("push bb1", "push bb0", "continuation changed"),
            (
                "push 100\n  calldataload\n  dup 1\n  push 160\n  shr",
                "push 100\n  calldataload\n  dup 1\n  push 160\n  shl",
                "no reachable counterexample established",
            ),
        ):
            with self.subTest(change=new):
                broken = self.directory / "wrong-exit.evmir"
                broken.write_text(self.paths[1].read_text().replace(old, new, 1))
                result, record = self.cli([self.paths[0], broken], "--prefix", "bb0")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(record["status"], "unsupported")
                self.assertIn(diagnostic, result.stderr)

    def test_live_pass_keeps_output(self):
        out = self.directory / "audit"
        command = [SOLAR, self.paths[0], "-Ogas", "-Zevm-ir-pipeline=block-cse"]
        results = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=100,
            )
            for flags in ([], [f"-Zsolar-tv={out}"])
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (out / "audits").glob("*.json")
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["status"], "verified")
        self.assertEqual(
            records[0]["region"]["proof"], "universal-branching-effect-prefix"
        )

    def test_unpaired_and_missing_labels_stay_unsupported(self):
        catalog = {
            row["name"]: row
            for row in native.catalog(native.default(SOLAR), 20)["evmir"]
        }
        for lines in (["push bb1", "jumpi"], ["push bb0"], ["jumpi"]):
            with self.subTest(lines=lines), self.assertRaises(Unsupported):
                evm_effects.instructions(lines, catalog, "cancun", {"bb0"})

    def test_peephole_preserves_branch_truth_and_replays_a_wrong_guard(self):
        before, after, broken, world = (
            self.directory / name
            for name in ("before.evmir", "after.evmir", "broken.evmir", "world.json")
        )
        source = """@module Guard
bb0:
  calldatasize
  iszero
  iszero
  push bb2
  jumpi
  jump bb1
bb1:
  stop
bb2:
  push 0
  push 0
  revert
"""
        before.write_text(source)
        after.write_text(source.replace("  iszero\n  iszero\n", ""))
        checked, record = self.cli([before, after], "--prefix", "bb0")
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertEqual(
            record["region"]["proof"], "kernel-checked-contextual-peephole"
        )

        broken.write_text(after.read_text().replace("calldatasize", "push 0"))
        rejected, record = self.cli([before, broken], "--prefix", "bb0")
        self.assertEqual(rejected.returncode, 2, rejected.stdout + rejected.stderr)
        self.assertEqual(record["status"], "unsupported")

        world.write_text('{"version":1,"gas":100000,"calldata":"0x01"}')
        replay, record = self.cli([before, broken], "--world", world)
        self.assertEqual(replay.returncode, 1, replay.stdout + replay.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )


if __name__ == "__main__":
    unittest.main()
