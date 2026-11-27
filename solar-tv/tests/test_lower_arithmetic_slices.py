"""Verify captured checked arithmetic and slice projection lowerings."""

import re
import unittest

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state


class LowerArithmeticSlicesTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_checked_add_preserves_overflow_payload(self):
        before = state.module(
            "v0 = calldataload 0\nv1 = checked_add uint256, v0, 1\nret v1"
        )
        after = self.lower(before, "lower-arithmetic")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["region"]["proof"], "kernel-checked-unsigned-arithmetic")
        changed, count = re.subn(r"(mstore 32, )17\b", r"\g<1>18", after)
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        self.pair(before, after, fixture=recursive.calldata(bytes(32)))
        overflow = recursive.calldata(bytes([255]) * 32)
        self.pair(before, after, fixture=overflow)
        _, witness = self.pair(after, changed, fixture=overflow, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )

    def test_slice_projections_preserve_pointer_and_length(self):
        before = state.module(
            """v0 = calldataload 0
v1 = make_memory_slice ptr<memory> 128, v0
v2 = extract_value slice<memory>, v1, 0
v3 = extract_value slice<memory>, v1, 1
v4 = mload v2
v5 = add v4, v3
ret v5"""
        )
        after = self.lower(before, "lower-slices")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["region"]["proof"], "kernel-checked-slice-projections")
        changed, count = re.subn(r"\bret v\d+\b", "ret 0", after)
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        fixture = recursive.calldata((9).to_bytes(32, "big"))
        self.pair(before, after, fixture=fixture)
        _, witness = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )
