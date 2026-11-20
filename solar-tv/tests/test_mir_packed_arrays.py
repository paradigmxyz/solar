"""Packed arrays against real lowering, including cleanup, strides and wrapped slices."""

import json
import unittest

import test_mir_abi as abi
import test_mir_packed_refs as refs
import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import ROOT

PIPELINE = refs.PIPELINE


class MirPackedArrayTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_memory_array_leaf_cleanup(self):
        for element, values in (
            ("word", [17, 19]),
            ("word<uint8>", [257, 258]),
            ("word<int8>", [255, 254]),
            ("word<bool>", [0, 7]),
            ("word<bytes2>", [2**256 - 1, 0]),
            ("function", [(17 << 64) | 99, (19 << 64) | 3]),
            ("word<enum 3>", [1, 2]),
            ("word<enum 3>", [1, 3]),
        ):
            setup = (
                f"mstore 256, 2\nmstore 288, {values[0]}\nmstore 320, {values[1]}\n"
                "v0 = inttoptr i256 256 to ptr<memory>\n"
            )
            with self.subTest(element=element, values=values):
                self.lowered_pair(
                    refs.source(f"array<{element}> memoryarray v0", setup), PIPELINE
                )

    def test_fixed_array_stride_is_part_of_the_layout(self):
        setup = (
            "mstore 256, 17\nmstore 288, 999\nmstore 320, 19\n"
            "v0 = inttoptr i256 256 to ptr<memory>\n"
        )
        before = refs.source("array<word> memoryfixedarray<2, 2> v0", setup)
        self.lowered_pair(before, PIPELINE)
        self.pair(
            before,
            before.replace("memoryfixedarray<2, 2>", "memoryfixedarray<2, 1>"),
            fixture=storage.fixture(),
            status=1,
        )

    def test_nested_memory_rows_and_contiguous_calldata(self):
        setup = (
            "mstore 256, 2\nmstore 288, 384\nmstore 320, 448\n"
            "mstore 384, 17\nmstore 416, 19\nmstore 448, 23\nmstore 480, 29\n"
            "v0 = inttoptr i256 256 to ptr<memory>\n"
        )
        self.lowered_pair(
            refs.source("array<array<2, word>> memoryarray v0", setup), PIPELINE
        )
        initial = storage.fixture()
        initial["calldata"] = (
            "0x"
            + b"".join(value.to_bytes(32, "big") for value in [17, 19, 23, 29]).hex()
        )
        setup = (
            "v0 = inttoptr i256 0 to ptr<calldata>\nv1 = make_calldata_slice v0, 2\n"
        )
        for element in ("word", "word<uint8>", "array<2, word>"):
            with self.subTest(element=element):
                self.lowered_pair(
                    refs.source(f"array<{element}> calldata v1", setup),
                    PIPELINE,
                    initial,
                )

    def test_wordwise_wrap_and_unguarded_copy_are_distinct(self):
        setup = f"v0 = inttoptr i256 {2**256 - 32} to ptr<calldata>\nv1 = make_calldata_slice v0, 2\n"
        initial = storage.fixture()
        initial["calldata"] = "0x" + (17).to_bytes(32, "big").hex()
        before = refs.source("array<word, wordwise> calldata v1", setup)
        self.lowered_pair(before, PIPELINE, initial)
        self.lowered_pair(
            refs.source("array<word> calldata v1", setup), PIPELINE, initial
        )
        self.pair(
            before,
            before.replace("word, wordwise", "word"),
            fixture=initial,
            status=1,
        )

    def test_empty_arrays_slices_and_ordered_overlapping_cleanup(self):
        for layout, pointer, length in (
            ("memoryarray", 96, 2),
            ("memoryarray", 256, 0),
            ("memoryfixedarray<0, 1>", 256, 0),
        ):
            setup = (
                f"mstore {pointer}, {length}\nmstore {pointer + 32}, 257\n"
                f"mstore {pointer + 64}, 258\nv0 = inttoptr i256 {pointer} to ptr<memory>\n"
            )
            with self.subTest(layout=layout, pointer=pointer, length=length):
                self.lowered_pair(
                    refs.source(f"array<word<uint8>> {layout} v0", setup), PIPELINE
                )
        setup = (
            "mstore 256, 17\nmstore 288, 19\n"
            "v0 = inttoptr i256 256 to ptr<memory>\nv1 = make_memory_slice v0, 2\n"
        )
        self.lowered_pair(refs.source("array<word> memory v1", setup), PIPELINE)

    def test_array_size_overflow_and_work_limits(self):
        for length in (2**255, 2049):
            setup = f"mstore 256, {length}\nv0 = inttoptr i256 256 to ptr<memory>\n"
            before = refs.source("array<word> memoryarray v0", setup)
            if length == 2**255:
                self.lowered_pair(before, PIPELINE)
            else:
                result, _ = self.pair(
                    before, before, fixture=storage.fixture(), status=2
                )
                self.assertIn("work limit", result.stderr)

    def test_documented_strided_array_pair(self):
        folder = ROOT / "examples/mir/packed-arrays"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        initial = json.loads((folder / "world.json").read_text())
        actual = self.lower(before, "lower-builtins,lower-slices,lower-memory-objects")
        self.assertEqual(actual.splitlines()[1:], after.splitlines())
        self.pair(before, after, fixture=initial)
        self.pair(
            before, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )


if __name__ == "__main__":
    unittest.main()
