"""Exact require payloads against lower-checks, ABI lowering and independent replay."""

import unittest

import test_mir_abi as abi
import test_mir_abi_encode_recursive as encoding
import test_mir_state as state

PIPELINE = "lower-checks,lower-abi-encode,lower-slices,lower-memory-objects"


class MirRequireTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_empty_string_is_not_an_empty_revert(self):
        source = state.module("icall require<empty_string>, false\nret 0")
        self.lowered_pair(source, PIPELINE)
        self.pair(source, state.module("revert 0, 0"), status=1)

    def test_short_and_literal_payloads(self):
        for length in (1, 3, 32):
            with self.subTest(length=length):
                data = int.from_bytes(bytes(range(length)).ljust(32, b"\xff"), "big")
                source = state.module(
                    f"icall require<short_string>, false, {length}, {data}\nret 0"
                )
                self.lowered_pair(source, PIPELINE)
        for payload in (b"abc" * 11, bytes(range(64))):
            with self.subTest(payload=payload):
                source = state.module(
                    f'icall require<long_string hex"{payload.hex()}">, false\nret 0'
                )
                self.lowered_pair(source, PIPELINE)

    def test_dynamic_error_and_custom_error(self):
        initial = encoding.memory(encoding.words(3) + b"abc" + bytes(29))
        for kind, args in (
            ("error_string", "false, v0"),
            ("custom_error [memory_bytes]", f"false, {0x11223344 << 224}, v0"),
            ("custom_error [word, word]", f"false, {0x11223344 << 224}, 7, 9"),
        ):
            with self.subTest(kind=kind):
                source = state.module(
                    "v0 = inttoptr i256 128 to ptr<memory>\n"
                    + f"icall require<{kind}>, {args}\nret 0"
                )
                self.lowered_pair(source, PIPELINE, initial)

    def test_passing_path_and_argument_evaluation(self):
        for kind, args in (
            ("empty_string", "true"),
            ("short_string", "true, 3, 0"),
            ("error_string", "true, v0"),
            ("custom_error [memory_bytes]", f"true, {0x11223344 << 224}, v0"),
        ):
            with self.subTest(kind=kind):
                source = state.module(
                    "v0 = inttoptr i256 65536 to ptr<memory>\n"
                    + f"icall require<{kind}>, {args}\nret 7"
                )
                self.pair(source, state.module("ret 7"))
                self.lowered_pair(source, PIPELINE)
        source = state.module(
            "icall require<empty_string>, arg0\nret 7", "fn @f(arg0: i1) -> i256"
        )
        self.lowered_pair(source, PIPELINE)

    def test_raw_panic_custom_error_has_the_same_bytes(self):
        before = state.module(
            f"icall require<custom_error [word]>, false, {0x4E487B71 << 224}, 17\nret 0"
        )
        self.pair(before, state.module("icall panic_if<0x11>, true\nret 0"))

    def test_symbolic_boolean_product_is_exhaustive(self):
        source = state.module(
            "v0 = and arg0, arg1\nicall require<empty_string>, v0\nret 7",
            "fn @f(arg0: i1, arg1: i1) -> i256",
        )
        self.lowered_pair(source, PIPELINE)
