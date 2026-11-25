"""Check typed memory address lowering and replay a changed byte offset."""

import re
import unittest

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state


class LowerMemoryObjectsTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_same_address_after_word_casts(self):
        before = state.module(
            """v0 = calldataload 0
v1 = inttoptr i256 v0 to ptr<memory>
v2 = ptradd v1, 32
v3 = mload v2
ret v3"""
        )
        after = self.lower(before, "lower-memory-objects")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertEqual(
            proof["region"]["proof"], "kernel-checked-memory-object-carriers"
        )
        changed, count = re.subn(r"(?m)(\badd v\d+, )32$", r"\g<1>64", after)
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        fixture = recursive.calldata((128).to_bytes(32, "big"))
        memory = bytes(160) + (11).to_bytes(32, "big") + (42).to_bytes(32, "big")
        fixture["memory"] = {"data": "0x" + memory.hex()}
        self.pair(before, after, fixture=fixture)
        _, witness = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )
