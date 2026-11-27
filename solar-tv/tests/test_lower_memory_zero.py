"""Exercise calldata padding and the memory word zero-store fast path."""

import re
import unittest

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state


class LowerMemoryZeroTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_calldata_end_and_changed_source(self):
        before = state.module(
            """v0 = calldataload 0
v1 = inttoptr i256 128 to ptr<memory>
mstore v1, 7
memory_zero v1, v0
v2 = mload v1
ret v2"""
        )
        after = self.lower(before, "lower-memory-zero")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["region"]["proof"], "kernel-checked-memory-zero")
        changed, count = re.subn(
            r"(?m)^(\s*calldatacopy [^,]+), [^,]+,",
            r"\1, ptr<calldata> 0,",
            after,
        )
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        fixture = recursive.calldata((32).to_bytes(32, "big"))
        self.pair(before, after, fixture=fixture)
        _, record = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_zero_word_store(self):
        before = state.module(
            """v0 = inttoptr i256 128 to ptr<memory>
mstore v0, 7
memory_zero v0, 32
v1 = mload v0
ret v1"""
        )
        after = self.lower(before, "lower-memory-zero")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["region"]["proof"], "kernel-checked-memory-zero")
        self.pair(before, after)
