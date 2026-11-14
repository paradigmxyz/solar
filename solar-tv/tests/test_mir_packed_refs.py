"""Packed byte references retain aliases, storage words and restored hash headers."""

import unittest

import test_mir_abi as abi
import test_mir_state as state
import test_mir_storage_bytes as storage

PIPELINE = "lower-builtins,lower-slices,lower-memory-objects,lower-mapping-slots"


def source(parts, setup="", hashed=False):
    op = "keccak256_packed" if hashed else "abi_encode_packed"
    result = "ret v9" if hashed else "v10 = ptrtoint ptr<memory> v9 to i256\nret v10"
    return state.module(setup + f"v9 = {op} ({parts})\n" + result)


class MirPackedReferenceTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_memory_objects_slices_and_borrowed_headers(self):
        setup = (
            "mstore 256, 3\nmstore 288, "
            + str(int.from_bytes(b"abc" + b"\xff" * 29, "big"))
            + "\nv0 = inttoptr i256 256 to ptr<memory>\n"
        )
        for parts in (
            "memory_bytes v0",
            'data hex"1122", memory_bytes v0',
            'memory_bytes v0, data hex"1122"',
        ):
            for hashed in (False, True):
                with self.subTest(parts=parts, hashed=hashed):
                    self.lowered_pair(source(parts, setup, hashed), PIPELINE)
        slice_setup = (
            setup
            + "v1 = inttoptr i256 288 to ptr<memory>\nv2 = make_memory_slice v1, 3\n"
        )
        self.lowered_pair(source("memory_bytes v2", slice_setup, True), PIPELINE)
        before = source('data hex"1122", memory_bytes v0', setup, True)
        self.pair(
            before,
            before.replace('hex"1122"', 'hex"1123"'),
            fixture=storage.fixture(),
            status=1,
        )

    def test_calldata_slices_and_zero_fill(self):
        initial = storage.fixture()
        initial["calldata"] = "0x010203"
        for offset, length in ((0, 3), (2, 4), (99, 0)):
            setup = f"v0 = inttoptr i256 {offset} to ptr<calldata>\nv1 = make_calldata_slice v0, {length}\n"
            for hashed in (False, True):
                with self.subTest(offset=offset, length=length, hashed=hashed):
                    self.lowered_pair(
                        source(
                            'data hex"ab", calldata_bytes v1, uint8 i8 7', setup, hashed
                        ),
                        PIPELINE,
                        initial,
                    )

    def test_output_aliases_payload_and_header(self):
        for pointer, length in ((96, 3), (128, 3), (160, 0), (64, 128)):
            setup = f"mstore {pointer}, {length}\nv0 = inttoptr i256 {pointer} to ptr<memory>\n"
            with self.subTest(pointer=pointer, length=length):
                self.lowered_pair(
                    source('data hex"abcdef", memory_bytes v0', setup), PIPELINE
                )

    def test_short_empty_long_and_invalid_storage_headers(self):
        for header, slots in (
            (0, {}),
            ((0x616263 << 232) | 6, {}),
            (67, {storage.BASE: 17, storage.BASE + 1: 255}),
            (64, {}),
            (63, {}),
        ):
            for hashed in (False, True):
                with self.subTest(header=header, hashed=hashed):
                    self.lowered_pair(
                        source(
                            'data hex"ab", storage_bytes 3, uint8 i8 7', hashed=hashed
                        ),
                        PIPELINE,
                        storage.fixture({3: header, **slots}),
                    )

    def test_empty_borrow_restores_header_and_reference_limit(self):
        setup = "mstore 256, 0\nv0 = inttoptr i256 256 to ptr<memory>\n"
        self.lowered_pair(
            source('data hex"ab", memory_bytes v0', setup, True), PIPELINE
        )
        before = source("memory_bytes v0", setup.replace("256, 0", "256, 65537"))
        result, _ = self.pair(before, before, fixture=storage.fixture(), status=2)
        self.assertIn("64 KiB", result.stderr)

    def test_invalid_returndata_shape_is_rejected(self):
        setup = (
            "v0 = inttoptr i256 0 to ptr<returndata>\n"
            "v1 = make_returndata_slice v0, 0\n"
        )
        before = source("memory_bytes v1", setup)
        result, _ = self.pair(before, before, status=2)
        self.assertIn("incompatible scalar types", result.stderr)


if __name__ == "__main__":
    unittest.main()
