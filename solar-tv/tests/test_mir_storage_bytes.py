"""Byte-string storage semantics against real builtin expansion and native replay."""

import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
from test_cli import ROOT, SOLAR

BASE = int("c2575a0e9e593c00f959f8c92f12db2869c3395a3b0502d05e2516446f71f85b", 16)


def fixture(slots=None):
    return {
        "memory": {
            "data": "0x" + (bytes(64) + (128).to_bytes(32, "big")).hex(),
            "size": 96,
        },
        "accounts": [
            {
                "address": 65536,
                "storage": {str(k): str(v) for k, v in (slots or {}).items()},
            }
        ],
    }


class MirStorageBytesTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def lowered_pair(self, source, world=None):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "storage.mir"
            path.write_text(source)
            result = subprocess.run(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    "-Zmir-pipeline=lower-builtins,lower-slices,lower-memory-objects,lower-mapping-slots",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                capture_output=True,
                check=True,
            )
        self.pair(source, result.stdout, fixture=world or fixture())
        return result.stdout

    def test_header_validation_and_panic_selector(self):
        for header, valid in [
            (0, True),
            (62, True),
            (64, False),
            (1, False),
            (63, False),
            (65, True),
        ]:
            with self.subTest(header=header):
                before = state.module(f"validate_storage_bytes {header}\nret 9")
                after = state.module(
                    "ret 9"
                    if valid
                    else "mstore 0, 0x4e487b7100000000000000000000000000000000000000000000000000000000\nmstore 4, 34\nrevert 0, 36"
                )
                self.pair(before, after)
        self.pair(
            state.module("validate_storage_bytes 64\nret 0"),
            state.module("ret 0"),
            status=1,
        )

    def test_real_loads_short_empty_and_long(self):
        source = state.module(
            "v0 = load_storage_bytes 3\nv1 = ptrtoint ptr<memory> v0 to i256\nret v1"
        )
        for header, slots in [
            (0, {}),
            ((int.from_bytes(b"abc" + bytes(29), "big") | 6), {}),
            (67, {BASE: 17, BASE + 1: 255}),
        ]:
            with self.subTest(header=header):
                self.lowered_pair(source, fixture({3: header, **slots}))
        self.pair(
            source,
            source.replace("load_storage_bytes 3", "load_storage_bytes 4"),
            fixture=fixture({3: 67, BASE: 17}),
            status=1,
        )

    def test_literal_store_clears_old_long_tail(self):
        for data in (b"abc", bytes(range(33))):
            with self.subTest(length=len(data)):
                source = state.module(
                    f'store_storage_bytes_literal 3, hex"{data.hex()}"\nret 0'
                )
                self.lowered_pair(source, fixture({3: 129, BASE: 17, BASE + 1: 19}))
        folder = ROOT / "examples/mir/storage-bytes"
        before = (folder / "before.mir").read_text()
        after = self.lowered_pair(before, fixture({3: 129, BASE: 17, BASE + 1: 19}))
        self.assertEqual(
            after.splitlines()[1:], (folder / "after.mir").read_text().splitlines()[1:]
        )
        self.pair(
            before,
            before.replace('hex"616263"', 'hex"616264"'),
            fixture=fixture({3: 129, BASE: 17, BASE + 1: 19}),
            status=1,
        )

    def test_object_store_masks_dirty_padding_and_preserves_read_order(self):
        prefix = (
            "mstore 128, 33\nmstore 160, 0x"
            + "aa" * 32
            + "\nmstore 192, 0x"
            + "ff" * 32
            + "\nv0 = inttoptr i256 128 to ptr<memory>\n"
        )
        self.lowered_pair(state.module(prefix + "store_storage_bytes 3, v0\nret 0"))
        # Clearing an old long tail hashes in scratch before reading the object's data.
        prefix = (
            "mstore 0, 4\nmstore 32, 0x"
            + "aa" * 32
            + "\nv0 = inttoptr i256 0 to ptr<memory>\n"
        )
        self.lowered_pair(
            state.module(prefix + "store_storage_bytes 3, v0\nret 0"),
            fixture({3: 129, BASE: 17, BASE + 1: 19}),
        )

    def test_clear_ranges_static_failures_and_limits(self):
        for first, last in [(0, 2), (2, 1), (2, 2)]:
            with self.subTest(first=first, last=last):
                self.lowered_pair(
                    state.module(f"clear_storage_words 3, {first}, {last}\nret 0"),
                    fixture({BASE: 17, BASE + 1: 19}),
                )
        self.pair(
            state.module("clear_storage_words 3, 0, 1\nret 0"),
            state.module("invalid"),
            fixture={"static": True},
        )
        self.pair(
            state.module('store_storage_bytes_literal 3, hex"01"\nret 0'),
            state.module("invalid"),
            fixture={"static": True},
        )
        source = state.module("clear_storage_words 3, 0, 2049\nret 0")
        result, _ = self.pair(source, source, status=2)
        self.assertIn("storage work limit", result.stderr)

    def test_invalid_old_header_precedes_static_write_and_allocation(self):
        source = state.module('store_storage_bytes_literal 3, hex"01"\nret 0')
        expected = state.module(
            "mstore 0, 0x4e487b7100000000000000000000000000000000000000000000000000000000\nmstore 4, 34\nrevert 0, 36"
        )
        self.pair(source, expected, fixture={**fixture({3: 1}), "static": True})
        source = state.module("v0 = load_storage_bytes 3\nret 0")
        self.pair(source, expected, fixture=fixture({3: 1}))

    def test_overflow_panics_keep_their_own_codes(self):
        maximum = 2**256 - 1
        for body, code, world in [
            (
                f"mstore 128, {maximum}\nv0 = inttoptr i256 128 to ptr<memory>\nstore_storage_bytes 3, v0\nret 0",
                0x11,
                fixture(),
            ),
            ("v0 = load_storage_bytes 3\nret 0", 0x41, fixture({3: maximum})),
        ]:
            with self.subTest(code=code):
                expected = state.module(
                    f"mstore 0, 0x4e487b7100000000000000000000000000000000000000000000000000000000\nmstore 4, {code}\nrevert 0, 36"
                )
                self.pair(state.module(body), expected, fixture=world)
