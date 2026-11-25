"""Compiler-generated check messages preserve the configured revert-string mode."""

import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import SOLAR


class MirDebugRevertTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def converted(self, source, debug):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "checks.mir"
            path.write_text(source)
            result = subprocess.run(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    "--revert-strings=" + ("debug" if debug else "default"),
                    "-Zmir-pipeline=lower-checks,lower-abi-encode,lower-slices,lower-memory-objects",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_real_check_lowering_in_both_modes(self):
        for name in (
            "empty",
            "ether_sent_to_non_payable",
            "tuple_data_too_short",
            "invalid_calldata_access_offset",
        ):
            for debug in (False, True):
                with self.subTest(name=name, debug=debug):
                    before = state.module(f"icall revert_if<{name}>, true\nret 7")
                    initial = {**storage.fixture(), "debug_reverts": debug}
                    self.pair(before, self.converted(before, debug), fixture=initial)

    def test_debug_strings_differ_from_an_empty_revert(self):
        before = state.module("icall revert_if<tuple_data_too_short>, true\nret 7")
        self.pair(before, state.module("revert 0, 0"), fixture=storage.fixture())
        self.pair(
            before,
            state.module("revert 0, 0"),
            fixture={**storage.fixture(), "debug_reverts": True},
            status=1,
        )

    def test_passing_check_does_not_encode_a_message(self):
        before = state.module("icall revert_if<tuple_data_too_short>, false\nret 7")
        self.pair(before, state.module("ret 7"), fixture={"debug_reverts": True})


if __name__ == "__main__":
    unittest.main()
