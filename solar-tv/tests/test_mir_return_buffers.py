"""Lowered memory-return calls use explicit placements, not compiled caller images."""

import json
import unittest

import test_mir_abi as abi
import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import ROOT

PAIR = "fn @pair() [return_abi=[i256, i256]] {\nbb0:\njump bb1\nbb1:\nret 17, 19\n}\n"
CALL = "v0 = icall @pair\nv1 = mload 32\nv2 = add v1, 32\nv3 = mload v2\nv4 = add v0, v3\nret v4"


def fixture(**functions):
    return {**storage.fixture(), "mir_layout": {"functions": functions}}


class MirReturnBufferTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_real_cfg_pair_and_broken_extra_component(self):
        before = state.module(CALL) + PAIR
        initial = fixture(pair={"return_buffer": 256})
        after = self.lower(before, "cfg-simplify")
        self.assertNotEqual(before.splitlines()[1:], after.splitlines()[1:])
        self.pair(before, after, fixture=initial)
        expected = state.module("mstore 288, 19\nmstore 32, 256\nret 36")
        self.pair(before, expected, fixture=initial)
        self.pair(before, before.replace("17, 19", "17, 20"), fixture=initial, status=1)

    def test_ignored_results_still_publish_the_buffer(self):
        before = state.module("icall @pair\nret 7") + PAIR
        self.pair(
            before,
            state.module("mstore 288, 19\nmstore 32, 256\nret 7"),
            fixture=fixture(pair={"return_buffer": 256}),
        )

    def test_tail_calls_keep_the_original_return_owner(self):
        pair = PAIR.replace("jump bb1", "tail_call @other").replace(
            "bb1:\nret 17, 19\n", ""
        )
        other = PAIR.replace("@pair", "@other")
        initial = fixture(pair={"return_buffer": 256}, other={"return_buffer": 512})
        before = state.module(CALL) + pair + other
        expected = state.module("mstore 288, 19\nmstore 32, 256\nret 36")
        self.pair(before, expected, fixture=initial)
        self.pair(before, self.lower(before, "cfg-simplify"), fixture=initial)
        self.pair(
            before,
            expected.replace("288", "544").replace("32, 256", "32, 512"),
            fixture=initial,
            status=1,
        )

    def test_narrow_components_and_live_memory_aliases(self):
        helper = "fn @pair() [return_abi=[i8, i1, ptr<memory>]] {\nbb0:\nv0 = inttoptr i256 128 to ptr<memory>\nret i8 255, i1 1, v0\n}\n"
        before = (
            state.module("v0 = icall @pair\nv1 = zext i8 v0 to i256\nret v1") + helper
        )
        self.pair(
            before,
            state.module("mstore 160, 1\nmstore 192, 128\nmstore 32, 128\nret 255"),
            fixture=fixture(pair={"return_buffer": 128}),
        )

    def test_missing_or_invalid_placements_do_not_verify(self):
        before = state.module(CALL) + PAIR
        result, _ = self.pair(before, before, status=2)
        self.assertIn("missing MIR function placement", result.stderr)
        result, _ = self.pair(
            before, before, fixture=fixture(pair={"return_buffer": 129}), status=2
        )
        self.assertIn("aligned", result.stderr)

    def test_documented_real_cfg_pair(self):
        folder = ROOT / "examples/mir/return-buffers"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        initial = json.loads((folder / "world.json").read_text())
        actual = self.lower(before, "cfg-simplify")
        self.assertEqual(actual.splitlines()[1:], after.splitlines())
        self.pair(before, after, fixture=initial)
        self.pair(
            before, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )


if __name__ == "__main__":
    unittest.main()
