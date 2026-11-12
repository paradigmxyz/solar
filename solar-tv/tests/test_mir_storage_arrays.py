"""Direct array semantics against builtin expansion, with independent replay."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import ROOT, SOLAR
from test_mir_storage_bytes import BASE, fixture

sys.path.insert(0, str(ROOT))
from validator import native, world
from validator.common import Program


def load(layout, slot=3):
    return state.module(
        f"v0 = load_storage_array {layout}, {slot}\n"
        "v1 = ptrtoint ptr<memory> v0 to i256\nret v1"
    )


def panic(code):
    return state.module(
        "mstore 0, 0x4e487b7100000000000000000000000000000000000000000000000000000000\n"
        f"mstore 4, {code}\nrevert 0, 36"
    )


class MirStorageArraysTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lowered_pair = storage.MirStorageBytesTests.lowered_pair

    def test_real_scalar_encodings_and_partial_last_words(self):
        for layout, bits in [
            ("uint8", 8),
            ("uint24", 24),
            ("uint128", 128),
            ("uint136", 136),
            ("uint256", 256),
            ("int24", 24),
            ("int256", 256),
            ("bytes3", 24),
            ("bytes17", 136),
            ("bytes32", 256),
        ]:
            with self.subTest(layout=layout):
                per_slot = 256 // bits
                length = min(per_slot + 1, 3)
                # High bits are dirty; signed fields have their sign bit set.
                word = 2**256 - 1
                self.lowered_pair(
                    load(layout), fixture({3: length, BASE: word, BASE + 1: word - 1})
                )

    def native_lowered_pair(self, source, initial):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "array.mir"
            path.write_text(source)
            compiled = subprocess.run(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    "-Zmir-pipeline=lower-builtins,lower-slices,lower-memory-objects,lower-mapping-slots",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            fixture_path = Path(directory) / "world.json"
            fixture_path.write_text(json.dumps({"version": 1, **initial}))
            fixture = world.read(fixture_path, SOLAR, None, kind="mir")

            def program(text):
                return Program(
                    ("world-entry", "mir"),
                    [],
                    "MIR.State.Run",
                    "",
                    canonical_ir=text,
                    selector="f",
                    evm_version="cancun",
                    world=fixture,
                )

            result = native.replay(
                program(source),
                program(compiled.stdout),
                [],
                executable=state.REPLAY,
                timeout=30,
            )
        self.assertEqual(*result["observations"])

    def test_native_full_packed_word_and_partial_tail(self):
        # Large loop expansions supplement the smaller kernel-checked pass pairs
        # through independent MIR replay; they are not reported as Lean proofs.
        for bits in (8, 24):
            with self.subTest(bits=bits):
                length = 256 // bits + 1
                word = 2**256 - 1
                self.native_lowered_pair(
                    load(f"uint{bits}"),
                    fixture({3: length, BASE: word, BASE + 1: word - 1}),
                )

    def test_empty_array_still_reserves_header_and_hashes_slot(self):
        source = load("uint8")
        after = state.module(
            "v0 = sload 3\nmstore 64, 160\nmstore 128, 0\nmstore 0, 3\nret 128"
        )
        self.pair(source, after, fixture=fixture())
        self.lowered_pair(source)
        self.pair(source, state.module("ret 128"), fixture=fixture(), status=1)

    def test_packed_signed_and_fixed_bytes_are_not_interchangeable(self):
        for before, after in [("int24", "uint24"), ("bytes3", "uint24")]:
            with self.subTest(before=before):
                self.pair(
                    load(before),
                    load(after),
                    fixture=fixture({3: 1, BASE: 0x800001}),
                    status=1,
                )
        # Dirty padding must not become a phantom element in the last storage word.
        self.pair(
            load("uint8"),
            load("uint16"),
            fixture=fixture({3: 1, BASE: 0x1234}),
            status=1,
        )

    def test_enum_range_failure_is_exact(self):
        for length, word in [(0, 255), (2, 0x0201), (2, 0x0301)]:
            with self.subTest(length=length, word=word):
                self.lowered_pair(load("enum<3>"), fixture({3: length, BASE: word}))
        world = fixture({3: 2, BASE: 0x0301})
        self.pair(load("enum<3>"), panic(0x21), fixture=world)
        self.pair(load("enum<3>"), load("uint8"), fixture=world, status=1)
        self.lowered_pair(load("enum<256>"), fixture({3: 1, BASE: 255}))

    def test_nested_bytes_allocate_in_order_and_validate_each_header(self):
        short = int.from_bytes(b"abc" + bytes(29), "big") | 6
        # Child zero is short, child one is long; untouched data words stay zero.
        self.lowered_pair(load("memorybytes"), fixture({3: 1, BASE: short}))
        self.lowered_pair(load("memorybytes"), fixture({3: 1, BASE: 65}))
        self.native_lowered_pair(
            load("memorybytes"), fixture({3: 2, BASE: short, BASE + 1: 67})
        )
        self.native_lowered_pair(
            load("memorybytes"), fixture({3: 2, BASE: short, BASE + 1: 1})
        )
        self.pair(load("memorybytes"), panic(0x22), fixture=fixture({3: 1, BASE: 1}))
        self.pair(
            load("memorybytes"),
            load("uint256"),
            fixture=fixture({3: 1, BASE: short}),
            status=1,
        )

    def test_allocation_overflow_precedes_slot_hashing(self):
        for length in [2**256 - 1, 2**59, 2**59 - 1]:
            with self.subTest(length=length):
                self.pair(load("uint8"), panic(0x41), fixture=fixture({3: length}))
                self.lowered_pair(load("uint8"), fixture({3: length}))

    def test_parent_can_alias_fmp_and_scratch_without_hidden_setup(self):
        for pointer in (0, 32, 64):
            with self.subTest(pointer=pointer):
                world = fixture({3: 2, BASE: 0x0201})
                world["memory"] = {
                    "data": "0x" + (bytes(64) + pointer.to_bytes(32, "big")).hex()
                }
                self.lowered_pair(load("uint8"), world)

    def test_read_only_static_context_and_explicit_limits(self):
        self.lowered_pair(
            load("uint24"), {**fixture({3: 2, BASE: 0x020001}), "static": True}
        )
        result, _ = self.pair(
            load("uint8"), load("uint8"), fixture=fixture({3: 2049}), status=2
        )
        self.assertTrue(
            "limit" in result.stderr or "unsupported" in result.stderr, result.stderr
        )
