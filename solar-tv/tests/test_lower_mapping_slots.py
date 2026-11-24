"""Validate one- and two-word slot materialization and replay a changed extent."""

import re
import unittest

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state


class LowerMappingSlotsTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_keeps_scratch_words_and_hash_extent(self):
        before = state.module(
            """v0 = calldataload 0
v1 = storage_array_data_slot v0
v2 = mapping_slot v0, v1
ret v2"""
        )
        after = self.lower(before, "lower-mapping-slots")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertEqual(proof["region"]["proof"], "kernel-checked-fixed-storage-slots")
        changed, count = re.subn(r"(?m)(keccak256 0, )64\b", r"\g<1>32", after)
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        fixture = recursive.calldata((9).to_bytes(32, "big"))
        self.pair(before, after, fixture=fixture)
        _, witness = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )
