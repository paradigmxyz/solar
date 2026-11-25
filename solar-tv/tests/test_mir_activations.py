"""Explicit per-activation frame bases retain caller locals and recursive aliases."""

import unittest

import test_mir_abi as abi
import test_mir_state as state

PIPELINE = "lower-frame-slots,lower-slices,lower-memory-objects"


def fixture(**functions):
    return {"mir_layout": {"functions": functions}}


class MirActivationTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_real_internal_frame_lowering(self):
        before = state.module(
            "frame_store internal_frame, word, 0, 17\n"
            "v0 = frame_load internal_frame, word, 0\nret v0"
        )
        initial = fixture(f={"frame_base": 256})
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(before, state.module("mstore 352, 17\nret 17"), fixture=initial)
        self.pair(
            before,
            before.replace("word, 0, 17", "word, 0, 19"),
            fixture=initial,
            status=1,
        )

    def test_slice_store_order_and_unaligned_alias(self):
        for location in ("memory", "calldata", "returndata"):
            before = state.module(
                f"v0 = inttoptr i256 2 to ptr<{location}>\n"
                f"v1 = make_{location}_slice v0, 4\n"
                f"frame_store internal_frame, {location}, 1, v1\n"
                "mstore 385, 7\n"
                f"v2 = frame_load internal_frame, {location}, 1\n"
                f"v3 = extract_value slice<{location}>, v2, 1\nret v3"
            )
            with self.subTest(location=location):
                self.pair(
                    before,
                    self.lower(before, PIPELINE),
                    fixture=fixture(f={"frame_base": 256}),
                )

    def test_callees_get_their_own_explicit_frame(self):
        helper = "fn @g(arg0: i256) -> i256 {\nbb0:\nframe_store internal_frame, word, 0, arg0\nret 0\n}\n"
        before = (
            state.module(
                "frame_store internal_frame, word, 0, 17\nv0 = icall @g, 23\n"
                "v1 = frame_load internal_frame, word, 0\nret v1"
            )
            + helper
        )
        initial = fixture(f={"frame_base": 256}, g={"frame_base": 512})
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(
            before,
            state.module("mstore 352, 17\nmstore 640, 23\nret 17"),
            fixture=initial,
        )
        self.pair(
            before,
            before.replace("ret v1", "ret 17"),
            fixture=fixture(f={"frame_base": 256}, g={"frame_base": 224}),
            status=1,
        )

    def test_recursive_calls_keep_live_ancestor_slots(self):
        helper = (
            "fn @sum(arg0: i256) -> i256 {\nbb0:\n"
            "frame_store internal_frame, word, 0, arg0\n"
            "v0: i1 = eq arg0, 0\njumpi v0, bb1, bb2\nbb1:\nret 0\n"
            "bb2:\nv1 = sub arg0, 1\nv2 = icall @sum, v1\n"
            "v3 = frame_load internal_frame, word, 0\nv4 = add v2, v3\nret v4\n}\n"
        )
        before = state.module("v0 = icall @sum, 3\nret v0") + helper
        initial = fixture(sum={"frame_bases": [256, 512, 768, 1024]})
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(
            before,
            state.module(
                "mstore 384, 3\nmstore 640, 2\nmstore 896, 1\nmstore 1152, 0\nret 6"
            ),
            fixture=initial,
        )
        result, _ = self.pair(
            before, before, fixture=fixture(sum={"frame_base": 256}), status=2
        )
        self.assertIn("activation", result.stderr)

    def test_tail_call_replaces_the_current_activation(self):
        helper = (
            "fn @g() -> i256 {\nbb0:\nframe_store internal_frame, word, 0, 17\ntail_call @h\n}\n"
            "fn @h() -> i256 {\nbb0:\nframe_store internal_frame, word, 0, 23\n"
            "v0 = internal_frame_addr 96\nv1 = mload v0\nret v1\n}\n"
        )
        before = state.module("v0 = icall @g\nret v0") + helper
        initial = fixture(g={"frame_base": 256}, h={"frame_bases": [512]})
        self.pair(
            before,
            state.module("mstore 352, 17\nmstore 608, 23\nret 23"),
            fixture=initial,
        )

    def test_ambiguous_or_unbounded_activation_inputs_are_rejected(self):
        before = state.module(
            "v0 = internal_frame_addr 0\nv1 = ptrtoint ptr<memory> v0 to i256\nret v1"
        )
        for fields in (
            {"frame_base": 256, "frame_bases": [512]},
            {"frame_bases": [129]},
            {"frame_bases": [256] * 129},
        ):
            with self.subTest(fields=fields):
                self.pair(before, before, fixture=fixture(f=fields), status=2)


if __name__ == "__main__":
    unittest.main()
