"""Packed word footprints, narrow groups and scratch selection versus real lowering."""

import unittest

import test_mir_abi as abi
import test_mir_state as state
import test_mir_storage_bytes as storage

PIPELINE = "lower-builtins,lower-slices,lower-memory-objects"
EMPTY = {"memory": {"data": "0x", "size": 0}}


def source(parts, hashed=True):
    op = "keccak256_packed" if hashed else "abi_encode_packed"
    return state.module(f"v0 = {op} ({parts})\nv1 = msize\nret v1")


class MirPackedTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_unaligned_literal_stores_expand_an_extra_word(self):
        before = source('data hex"01", data hex"02"')
        self.lowered_pair(before, PIPELINE, EMPTY)
        broken = state.module(
            f"mstore 0, {0x0102 << 240}\nv0 = keccak256 0, 2\nv1 = msize\nret v1"
        )
        result, _ = self.pair(before, broken, fixture=EMPTY, status=1)
        self.assertIn("Returned values changed", result.stdout)

    def test_narrow_groups_and_scratch_limit_follow_every_store(self):
        for parts in (
            'data hex"01", data hex"02"',
            f'data hex"{"11" * 32}", uint8 i8 3',
            f'data hex"{"11" * 33}", uint8 i8 3',
            f'uint8 i8 3, data hex"{"11" * 31}"',
            f'data hex"01", data hex"{"22" * 31}"',
            "uint8 i8 3, uint256 1, uint8 i8 4",
        ):
            for hashed in (False, True):
                with self.subTest(parts=parts, hashed=hashed):
                    self.lowered_pair(
                        source(parts, hashed),
                        PIPELINE,
                        EMPTY if hashed else storage.fixture(),
                    )

    def test_dirty_bytes_above_the_hash_length_are_observable(self):
        initial = storage.fixture()
        data = bytes.fromhex(initial["memory"]["data"].removeprefix("0x"))
        initial["memory"] = {"size": 256, "data": "0x" + data.ljust(256, b"\xff").hex()}
        self.lowered_pair(source('data hex"01", data hex"02"'), PIPELINE, initial)

    def test_empty_pieces_and_limits(self):
        for parts in ('data hex"", uint8 i8 7, data hex""', 'data hex"", data hex""'):
            with self.subTest(parts=parts):
                self.lowered_pair(source(parts), PIPELINE, EMPTY)
        before = source(", ".join(['data hex""'] * 2049))
        result, _ = self.pair(before, before, fixture=EMPTY, status=2)
        self.assertIn("packed part limit", result.stderr)


if __name__ == "__main__":
    unittest.main()
