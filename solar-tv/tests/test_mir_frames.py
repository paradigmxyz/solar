"""Convention-defined frame slots and explicit unsupported backend placement."""

import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
from test_cli import ROOT, SOLAR


class MirFrameTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def lowered_pair(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frames.mir"
            path.write_text(source)
            result = subprocess.run(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    "-Zmir-pipeline=lower-frame-slots,lower-slices,lower-memory-objects",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.pair(source, result.stdout)
        return result.stdout

    def test_real_unaligned_word_and_memory_aliases(self):
        for offset in (0, 1, 32):
            with self.subTest(offset=offset):
                self.lowered_pair(
                    state.module(
                        f"frame_store scratch, word, {offset}, 17\n"
                        f"v0 = mload {128 + offset}\n"
                        f"mstore {128 + offset}, 23\n"
                        f"v1 = frame_load scratch, word, {offset}\n"
                        "v2 = add v0, v1\nret v2"
                    )
                )

    def test_slice_locations_and_overlapping_stores(self):
        for location in ("memory", "calldata", "returndata"):
            with self.subTest(location=location):
                self.lowered_pair(
                    state.module(
                        f"v0 = inttoptr i256 2 to ptr<{location}>\n"
                        f"v1 = make_{location}_slice v0, 4\n"
                        f"frame_store scratch, {location}, 1, v1\n"
                        "mstore 161, 7\n"
                        f"v2 = frame_load scratch, {location}, 1\n"
                        f"v3 = extract_value slice<{location}>, v2, 1\nret v3"
                    )
                )

    def test_multi_return_word_is_pointer_slot_not_buffer_contents(self):
        source = state.module(
            "frame_store multi_return, word, 0, 256\n"
            "mstore 256, 19\nv0 = frame_load multi_return, word, 0\nret v0"
        )
        self.lowered_pair(source)
        self.pair(source, state.module("mstore 32, 256\nmstore 256, 19\nret 256"))
        self.pair(source, source.replace("ret v0", "ret 19"), status=1)

    def test_callers_share_external_scratch_region(self):
        source = (
            state.module(
                "frame_store scratch, word, 0, 11\nv0 = icall @g\n"
                "v1 = frame_load scratch, word, 0\nret v1"
            )
            + "fn @g() -> i256 {\nbb0:\nframe_store scratch, word, 0, 23\nret 0\n}\n"
        )
        self.lowered_pair(source)
        self.pair(source, source.replace("ret v1", "ret 11"), status=1)

    def test_wrong_offset_and_slice_length_have_counterexamples(self):
        source = state.module(
            "frame_store scratch, word, 0, 17\nv0 = frame_load scratch, word, 0\nret v0"
        )
        self.pair(
            source,
            source.replace(
                "frame_load scratch, word, 0", "frame_load scratch, word, 32"
            ),
            status=1,
        )
        source = state.module(
            "v0 = inttoptr i256 2 to ptr<memory>\nv1 = make_memory_slice v0, 4\nframe_store scratch, memory, 0, v1\nret 0"
        )
        self.pair(source, source.replace("v0, 4", "v0, 5"), status=1)

    def test_missing_placements_and_invalid_slots_stay_inconclusive(self):
        for body in (
            "v0 = frame_load internal_frame, word, 0\nret v0",
            "frame_store internal_frame, word, 0, 7\nret 0",
            "v0 = frame_load multi_return, memory, 0\nret 0",
            "frame_store multi_return, word, 1, 7\nret 0",
        ):
            with self.subTest(body=body):
                result, _ = self.pair(state.module(body), state.module(body), status=2)
                self.assertIn(
                    "missing MIR function placement"
                    if "internal_frame" in body
                    else "multi-return frame slot requires a word at offset zero",
                    result.stderr,
                )

    def test_memory_bounds_are_not_valid_frame_results(self):
        source = state.module("v0 = frame_load scratch, word, 65535\nret v0")
        result, _ = self.pair(source, source, status=2)
        self.assertIn("memory", result.stderr.lower())

    def test_documented_examples_match_real_pass_output(self):
        for name, pipeline in (
            (
                "storage-arrays",
                "lower-builtins,lower-slices,lower-memory-objects,lower-mapping-slots",
            ),
            ("frames", "lower-frame-slots,lower-slices,lower-memory-objects"),
            (
                "address-calls",
                "lower-builtins,lower-slices,lower-memory-objects,lower-mapping-slots",
            ),
        ):
            with self.subTest(name=name):
                folder = ROOT / "examples/mir" / name
                result = subprocess.run(
                    [
                        SOLAR,
                        folder / "before.mir",
                        "-Ogas",
                        f"-Zmir-pipeline={pipeline}",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    result.stdout.splitlines()[1:],
                    (folder / "after.mir").read_text().splitlines()[1:],
                )
