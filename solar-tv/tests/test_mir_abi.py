"""Scalar ABI operations, real conversion pairs and rejected missing obligations."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import ROOT, SOLAR


def decode(layout, word=17, *, location="object", length=32, marker=False):
    prefix = f"mstore 128, {length}\nmstore 160, {word}\n"
    if location == "object":
        prefix += "v0 = inttoptr i256 128 to ptr<memory>\n"
        operand = "v0"
    else:
        place = "calldata" if location == "calldata" else "memory"
        prefix += f"v0 = inttoptr i256 {0 if place == 'calldata' else 160} to ptr<{place}>\nv1 = make_{place}_slice v0, {length}\n"
        operand = "v1"
    if layout == "":
        return state.module(prefix + f"abi_decode [], {operand}\nret 0")
    result = "v2"
    body = prefix + f"v2 = abi_decode [{layout}], {operand}\n"
    if marker:
        body += "validate_abi v2\n"
    if layout == "bool":
        body += "v3 = zext i1 v2 to i256\n"
        result = "v3"
    if layout == "address":
        body += "v3 = zext i160 v2 to i256\n"
        result = "v3"
    if layout in {"uint8", "int8", "enum<3, uint8>"}:
        body += f"v3 = {'sext' if layout == 'int8' else 'zext'} i8 v2 to i256\n"
        result = "v3"
    return state.module(body + f"ret {result}")


class MirAbiTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def lower(self, source, pipeline):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "abi.mir"
            path.write_text(source)
            result = subprocess.run(
                [
                    SOLAR,
                    path,
                    "-Ogas",
                    f"-Zmir-pipeline={pipeline}",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def lowered_pair(self, source, pipeline, world=None):
        after = self.lower(source, pipeline)
        self.pair(source, after, fixture=world or storage.fixture())
        return after

    def test_real_static_encoders_all_placement_modes(self):
        selector = 0x11223344 << 224
        for mode in ("", ", object", ", scratch", ", scratch, base 193"):
            for selected in (False, True):
                with self.subTest(mode=mode, selector=selected):
                    options = mode.replace("base 193", "base v9") + (
                        f", selector {selector}" if selected else ""
                    )
                    body = (
                        "v9 = inttoptr i256 193 to ptr<memory>\n"
                        if "base" in mode
                        else ""
                    ) + f"v0 = abi_encode [word, function]{options}, args 7, 4660\n"
                    if mode == ", object":
                        body += "v1 = ptrtoint ptr<memory> v0 to i256\nret v1"
                    else:
                        body += "v1 = extract_value slice<memory>, v0, 1\nret v1"
                    self.lowered_pair(
                        state.module(body),
                        "lower-abi-encode,lower-slices,lower-memory-objects",
                    )

    def test_empty_encodings_keep_header_and_selector_traffic(self):
        for mode in ("", ", object", ", scratch"):
            with self.subTest(mode=mode):
                body = f"v0 = abi_encode []{mode}\nret 0"
                self.lowered_pair(
                    state.module(body),
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                )
        before = state.module("v0 = abi_encode [word], object, args 17\nret 0")
        self.pair(
            before,
            before.replace("args 17", "args 18"),
            fixture=storage.fixture(),
            status=1,
        )

    def test_decoder_canonicality_and_exact_reverts(self):
        for layout, clean, dirty in (
            ("uint8", 255, 256),
            ("int8", 2**256 - 2, 255),
            ("bool", 1, 2),
            ("address", 17, 2**160),
            ("bytes2", 0xABCD << 240, (0xABCD << 240) | 1),
            ("function", 0x1234 << 64, (0x1234 << 64) | 1),
            ("enum<3, uint8>", 2, 3),
        ):
            with self.subTest(layout=layout):
                self.lowered_pair(
                    decode(layout, clean), "lower-abi,lower-slices,lower-memory-objects"
                )
                before = decode(layout, dirty)
                self.pair(
                    before, state.module("revert 0, 0"), fixture=storage.fixture()
                )
        self.pair(
            decode("uint8", 17),
            decode("uint8", 18),
            fixture=storage.fixture(),
            status=1,
        )

    def test_empty_and_signed_tuple_length_checks(self):
        for layout, length in (
            ("", 0),
            ("", 2**255),
            ("uint256", 31),
            ("uint256", 2**255),
        ):
            with self.subTest(layout=layout, length=length):
                self.lowered_pair(
                    decode(layout, length=length),
                    "lower-abi,lower-slices,lower-memory-objects",
                )

    def test_decode_memory_and_calldata_slices(self):
        self.lowered_pair(
            decode("uint256", location="memory"),
            "lower-abi,lower-slices,lower-memory-objects",
        )
        world = storage.fixture()
        world["calldata"] = "0x" + (17).to_bytes(32, "big").hex()
        self.lowered_pair(
            decode("uint256", location="calldata"),
            "lower-abi,lower-slices,lower-memory-objects",
            world,
        )
        self.pair(
            decode("uint256", location="calldata"),
            decode("uint256", location="calldata").replace("ret v2", "ret 18"),
            fixture=world,
            status=1,
        )

    def test_validation_marker_needs_decoder_evidence(self):
        before = decode("function", 17 << 64, marker=True)
        self.pair(
            before, before.replace("validate_abi v2\n", ""), fixture=storage.fixture()
        )
        result, _ = self.pair(
            state.module("validate_abi 17\nret 0"), state.module("ret 0"), status=2
        )
        self.assertIn("decoder evidence", result.stderr)
        self.lowered_pair(before, "lower-abi,lower-slices,lower-memory-objects")

    def test_unchecked_normalization_cannot_invent_a_revert(self):
        raw = (17 << 64) | 1
        before = state.module(f"v0 = shr {raw}, 64\nvalidate_abi v0\nret v0")
        result, _ = self.pair(
            before, state.module("revert 0, 0"), fixture=storage.fixture(), status=2
        )
        self.assertIn("ABI evidence", result.stderr)

    def test_packed_scalars_literals_and_hashes(self):
        for hash in (False, True):
            with self.subTest(hash=hash):
                op = "keccak256_packed" if hash else "abi_encode_packed"
                body = f'v0 = {op} (data hex"ab", int16 i16 65534, bytes2 {0xCDEF << 240})\n'
                body += (
                    "ret v0" if hash else "v1 = ptrtoint ptr<memory> v0 to i256\nret v1"
                )
                source = state.module(body)
                self.lowered_pair(
                    source, "lower-builtins,lower-slices,lower-memory-objects"
                )
                self.pair(
                    source,
                    source.replace('hex"ab"', 'hex"ac"'),
                    fixture=storage.fixture(),
                    status=1,
                )

    def test_packed_alignment_zero_tail_and_word_scratch_boundary(self):
        for parts in (
            "",
            'data hex""',
            "uint256 17, uint256 19",
            "uint256 17, uint256 19, uint256 23",
        ):
            with self.subTest(parts=parts):
                for hash in (False, True):
                    op = "keccak256_packed" if hash else "abi_encode_packed"
                    body = f"v0 = {op} ({parts})\n" + (
                        "ret v0"
                        if hash
                        else "v1 = ptrtoint ptr<memory> v0 to i256\nret v1"
                    )
                    self.lowered_pair(
                        state.module(body),
                        "lower-builtins,lower-slices,lower-memory-objects",
                    )
        world = storage.fixture()
        world["memory"] = {
            "data": "0x"
            + (
                bytes(64) + (128).to_bytes(32, "big") + bytes(32) + bytes([255]) * 128
            ).hex(),
            "size": 256,
        }
        self.lowered_pair(
            state.module('v0 = abi_encode_packed (data hex"0102")\nret 0'),
            "lower-builtins,lower-slices,lower-memory-objects",
            world,
        )

    def test_recursive_layout_budget_remains_explicit(self):
        layout = "array<1, " * 34 + "uint256" + ">" * 34
        source = state.module(
            f"v0 = inttoptr i256 128 to ptr<memory>\nv1 = abi_decode [{layout}], v0\nret 0"
        )
        result, _ = self.pair(source, source, fixture=storage.fixture(), status=2)
        self.assertIn("layout limit", result.stderr)

    def test_symbolic_words_survive_encoding_and_decoding(self):
        for body, pipeline in (
            (
                (
                    "v0 = abi_encode [word], object, args arg0\n"
                    "v1 = ptrtoint ptr<memory> v0 to i256\nret v1"
                ),
                "lower-abi-encode,lower-slices,lower-memory-objects",
            ),
            (
                (
                    "mstore 128, 32\nmstore 160, arg0\n"
                    "v0 = inttoptr i256 128 to ptr<memory>\n"
                    "v1 = abi_decode [uint256], v0\nret v1"
                ),
                "lower-abi,lower-slices,lower-memory-objects",
            ),
        ):
            with self.subTest(pipeline=pipeline):
                source = state.module(body, "fn @f(arg0: i256) -> i256")
                self.lowered_pair(source, pipeline)

    def test_documented_real_encoder_pair(self):
        folder = ROOT / "examples/mir/abi-scalar"
        source = (folder / "before.mir").read_text()
        expected = (folder / "after.mir").read_text()
        after = self.lower(source, "lower-abi-encode,lower-slices,lower-memory-objects")
        self.assertEqual(after.splitlines()[1:], expected.splitlines()[1:])
        initial = json.loads((folder / "world.json").read_text())
        self.pair(source, expected, fixture=initial)
        self.pair(
            source, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )
