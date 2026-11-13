"""Explicit ABI canonicality, path-sensitive evidence and independent replay."""

import unittest

import test_mir_abi as abi
import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import ROOT


class MirAbiObligationTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_all_canonicality_rules_without_definition_shape_inference(self):
        good = (
            ("uint1", 1),
            ("uint8", 255),
            ("uint255", 2**255 - 1),
            ("uint256", 2**256 - 1),
            ("uint192", 17),
            ("int8", 2**256 - 2),
            ("int256", 2**255),
            ("bytes2", 0xABCD << 240),
            ("bytes24", 17 << 64),
            ("bytes32", 17),
            ("bool", 1),
            ("enum 3", 2),
        )
        before = state.module(
            "\n".join(f"validate_abi {rule}, {word}" for rule, word in good)
            + "\nret 17"
        )
        self.pair(before, state.module("ret 17"))

    def test_false_obligations_are_not_assumptions_or_runtime_reverts(self):
        for rule, word in (
            ("uint8", 256),
            ("int8", 255),
            ("bytes2", (0xABCD << 240) | 1),
            ("bool", 2),
            ("enum 3", 3),
        ):
            with self.subTest(rule=rule):
                before = state.module(f"validate_abi {rule}, {word}\nret 0")
                result, _ = self.pair(before, state.module("revert 0, 0"), status=2)
                self.assertIn("canonicality obligation", result.stderr)

    def test_obligation_after_phi_and_internal_call(self):
        before = """@module AbiEvidence
fn @f(arg0: i256) -> i256 {
bb0:
  v0 = eq arg0, 0
  jumpi v0, bb1, bb2
bb1:
  jump bb3
bb2:
  v1 = and arg0, 255
  jump bb3
bb3:
  v2 = phi [bb1: 17], [bb2: v1]
  v3 = icall @identity, v2
  validate_abi uint8, v3
  ret v3
}
fn @identity(arg0: i256) -> i256 {
bb0:
  ret arg0
}
"""
        self.pair(before, before.replace("  validate_abi uint8, v3\n", ""))

    def test_unreached_obligation_needs_no_invented_evidence(self):
        before = """@module AbiPath
fn @f(arg0: i256) -> i256 {
bb0:
  v0 = gt arg0, 255
  jumpi v0, bb1, bb2
bb1:
  revert 0, 0
bb2:
  validate_abi uint8, arg0
  ret arg0
}
"""
        self.pair(before, before.replace("  validate_abi uint8, arg0\n", ""))

    def test_real_lower_abi_pair_and_broken_padding_check(self):
        folder = ROOT / "examples/mir/abi-validation"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        actual = self.lower(before, "lower-abi,lower-slices,lower-memory-objects")
        self.assertEqual(actual[actual.index("@module") :], after)
        self.pair(before, after, fixture=storage.fixture())
        result, record = self.pair(
            before,
            (folder / "broken.mir").read_text(),
            fixture=storage.fixture(),
            status=1,
        )
        self.assertEqual(record["status"], "counterexample")
        witness = record["witness"]
        self.assertEqual(witness["inputs"], {"arg0": "1"})
        self.assertEqual(witness["observations"], witness["native_observations"])
        self.assertEqual(witness["observations"][0]["outcome"], "revert")
        self.assertEqual(witness["observations"][1]["outcome"], "return")
        self.assertEqual(witness["observations"][1]["values"], ["17"])
        self.assertIn("counterexample", result.stdout + result.stderr)
