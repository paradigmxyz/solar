"""MIR copies are logical across forks; partial lowering keeps exact footprints."""

import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
from test_cli import CLI, ROOT, SOLAR


class MirCopyLoweringTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def lower(self, source, version="shanghai"):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "copy.mir"
            path.write_text(source)
            return subprocess.check_output(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    f"--evm-version={version}",
                    "-Zmir-pipeline=lower-mcopy",
                    "-Zvalidate-ir=true",
                ],
                text=True,
            )

    def test_pre_cancun_copies_cannot_turn_wrong_returns_into_equal_failures(self):
        before = state.module("mcopy 0, 0, 1\nret 7")
        _, record = self.pair(
            before, before.replace("ret 7", "ret 8"), version="shanghai", status=1
        )
        self.assertEqual(
            [o["values"] for o in record["witness"]["observations"]], [["7"], ["8"]]
        )

    def test_exact_partial_footprints_and_overlaps(self):
        initial = {"memory": {"size": 192, "data": "0x" + bytes(range(192)).hex()}}
        for destination, source, count in (
            (0, 191, 1),
            (0, 190, 2),
            (0, 175, 17),
            (0, 161, 31),
            (190, 0, 2),
            (175, 0, 17),
            (161, 0, 31),
            (0, 191, 2),
            (191, 0, 2),
            (1, 0, 2),
            (0, 1, 2),
            (1, 0, 31),
            (0, 1, 31),
            (1, 0, 33),
            (0, 1, 34),
            (0, 0, 0),
            (0, 0, 31),
            (2, 0, 17),
        ):
            before = state.module(
                f"mcopy {destination}, {source}, {count}\nv0 = msize\nret v0"
            )
            with self.subTest(destination=destination, source=source, count=count):
                self.pair(
                    before, self.lower(before), fixture=initial, version="shanghai"
                )

    def test_old_expanding_tail_has_an_independent_counterexample(self):
        folder = ROOT / "examples/mir/mcopy"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        actual = self.lower(before)
        self.assertEqual(actual.splitlines()[1:], after.splitlines())
        self.pair(before, after, version="shanghai")
        self.pair(
            before, (folder / "broken.mir").read_text(), version="shanghai", status=1
        )

    def test_logical_copies_are_available_across_supported_forks(self):
        # The MIR abstraction has no native-opcode availability check. EVM IR
        # still executes the actual, fork-specific opcode through its own model.
        source = state.module("mcopy 0, 0, 1\nret 7")
        for version in ("istanbul", "paris", "shanghai", "cancun"):
            with self.subTest(version=version):
                self.pair(source, source, version=version)

    def test_scheduled_mcopy_still_requires_cancun(self):
        before = (
            "@module RawCopy\nbb0 [stack=0]:\npush 1\npush 0\npush 0\nmcopy\nstop\n"
        )
        after = "@module RawCopy\nbb0 [stack=0]:\nstop\n"
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            paths = [folder / f"{name}.evmir" for name in ("before", "after")]
            for path, text in zip(paths, (before, after), strict=True):
                path.write_text(text)
            world = folder / "world.json"
            world.write_text('{"version":1}')
            result = subprocess.run(
                [
                    CLI,
                    "evmir",
                    *paths,
                    "--world",
                    world,
                    "--evm-version=shanghai",
                    "--solar",
                    SOLAR,
                    "--verbose",
                ],
                capture_output=True,
                text=True,
                check=False,
                timeout=60,
            )
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("opcode `mcopy` is unavailable", result.stderr)


if __name__ == "__main__":
    unittest.main()
