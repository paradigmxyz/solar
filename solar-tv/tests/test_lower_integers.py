"""Check compiler integer widening and a changed callee result mask."""

import re
import unittest

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state


class LowerIntegersTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_wrap_preserved_through_widened_callee(self):
        before = """@module Width
fn @f() -> i256 {
  bb0:
    v0 = calldataload 0
    v1 = trunc i256 v0 to i8
    v2 = icall @inc, v1
    v3 = zext i8 v2 to i256
    ret v3
}
fn @inc(arg0: i8) -> i8 {
  bb0:
    v0 = add arg0, i8 1
    ret v0
}
"""
        after = self.lower(before, "lower-integers")
        report = self.automatic(before, after, 0)
        self.assertEqual(report["evidence"], "kernel-proof")
        self.assertEqual(
            report["region"]["proof"], "kernel-checked-integer-width-effects"
        )

        entry, callee = after.split("fn @inc", 1)
        callee, changed = re.subn(
            r"(?m)^(\s*v\d+ = and [^,]+), (?:255|0xff)$", r"\1, 127", callee
        )
        self.assertEqual(changed, 1)
        damaged = entry + "fn @inc" + callee
        self.automatic(before, damaged, 2)
        fixture = recursive.calldata((127).to_bytes(32, "big"))
        _, replay = self.pair(after, damaged, fixture=fixture, status=1)
        self.assertEqual(
            replay["witness"]["observations"], replay["witness"]["native_observations"]
        )
