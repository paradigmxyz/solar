"""Reference ABI encoders, real conversion passes and independent original-MIR replay."""

import json
import unittest

import test_mir_abi as abi
import test_mir_state as state
from test_cli import ROOT
from test_mir_abi_recursive import words


def memory(payload):
    end = (128 + len(payload) + 31) // 32 * 32
    data = (bytes(64) + end.to_bytes(32, "big")).ljust(128, b"\0") + payload
    return {"memory": {"data": "0x" + data.hex()}}


def calldata(payload):
    return {
        "memory": {"data": "0x" + (bytes(64) + (128).to_bytes(32, "big")).hex()},
        "calldata": "0x" + payload.hex(),
    }


def encode(layout, *, mode=", object", selector=False, calldata_length=None):
    setup = "v0 = inttoptr i256 128 to ptr<memory>\n"
    source = "v0"
    if calldata_length is not None:
        setup = f"v0 = inttoptr i256 0 to ptr<calldata>\nv1 = make_calldata_slice v0, {calldata_length}\n"
        source = "v1"
    option = f", selector {0x11223344 << 224}" if selector else ""
    body = setup + f"v2 = abi_encode [{layout}]{mode}{option}, args {source}\n"
    body += (
        "v3 = mload v2\nret v3"
        if mode == ", object"
        else "v3 = extract_value slice<memory>, v2, 1\nret v3"
    )
    return state.module(body)


class MirRecursiveEncodeTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_bytes_placement_and_dirty_padding(self):
        for mode in ("", ", object", ", scratch"):
            for selected in (False, True):
                with self.subTest(mode=mode, selector=selected):
                    self.lowered_pair(
                        encode("memory_bytes", mode=mode, selector=selected),
                        "lower-abi-encode,lower-slices,lower-memory-objects",
                        memory(words(3) + b"abc" + bytes([255]) * 29),
                    )
        for length in (0, 31, 32, 33):
            with self.subTest(length=length):
                data = bytes(range(length)).ljust((length + 31) // 32 * 32, b"\xff")
                self.lowered_pair(
                    encode("memory_bytes"),
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    memory(words(length) + data),
                )

    def test_memory_arrays_and_tuple_children(self):
        for layout, payload in (
            ("memory_array<word>", words(2, 7, 9)),
            ("memory_array<word<uint8>>", words(2, 263, 265)),
            ("array<2, word<uint8>>", words(263, 265)),
            ("tuple<word<uint8>, word<bool>>", words(263, 27)),
            ("tuple<word, memory_bytes>", words(7, 192, 3) + b"abc" + bytes(29)),
            (
                "memory_array<memory_bytes>",
                words(2, 224, 288, 1) + b"a" + bytes(31) + words(2) + b"bc" + bytes(30),
            ),
        ):
            with self.subTest(layout=layout):
                self.lowered_pair(
                    encode(layout),
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    memory(payload),
                )

    def test_calldata_bytes_arrays_and_nested_offsets(self):
        for layout, length, payload in (
            ("calldata_bytes", 3, b"abc"),
            ("calldata_array<word>", 2, words(7, 9)),
            ("calldata_array<word<uint8>>", 2, words(7, 9)),
            ("array<2, word>", 2, words(7, 9)),
            ("tuple<word, calldata_bytes>", 64, words(7, 64, 3) + b"abc" + bytes(29)),
            (
                "calldata_array<calldata_bytes>",
                2,
                words(64, 128, 1) + b"a" + bytes(31) + words(2) + b"bc" + bytes(30),
            ),
        ):
            with self.subTest(layout=layout):
                self.lowered_pair(
                    encode(layout, calldata_length=length),
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    calldata(payload),
                )

    def test_storage_bytes_short_empty_and_long(self):
        for length in (0, 3, 33):
            with self.subTest(length=length):
                payload = bytes(range(length))
                slot = 3
                slots = {str(slot): length * 2 + (length >= 32)}
                if length < 32:
                    slots[str(slot)] = int.from_bytes(
                        payload.ljust(32, b"\0"), "big"
                    ) | (length * 2)
                else:
                    # Keccak(uint256(3)), independent of the encoder implementation.
                    base = 87903029871075914254377627908054574944891091886930582284385770809450030037083
                    for index in range((length + 31) // 32):
                        slots[str(base + index)] = int.from_bytes(
                            payload[index * 32 :][:32].ljust(32, b"\0"), "big"
                        )
                initial = calldata(b"")
                initial["accounts"] = [{"address": 65536, "storage": slots}]
                source = state.module(
                    "v0 = abi_encode [storage_bytes], object, args 3\nv1 = mload v0\nret v1"
                )
                self.lowered_pair(
                    source,
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    initial,
                )

    def test_changed_encoding_length_is_a_counterexample(self):
        source = encode("memory_bytes")
        self.pair(
            source,
            source.replace("ret v3", "ret 0"),
            fixture=memory(words(3) + b"abc" + bytes(29)),
            status=1,
        )

    def test_documented_reference_encoder_pair(self):
        folder = ROOT / "examples/mir/abi-reference"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        initial = json.loads((folder / "world.json").read_text())
        actual = self.lower(
            before, "lower-abi-encode,lower-slices,lower-memory-objects"
        )
        self.assertEqual(actual.splitlines()[1:], after.splitlines()[1:])
        self.pair(before, after, fixture=initial)
        self.pair(
            before, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )

    def test_encoded_memory_slices(self):
        for layout, payload, count in (
            ("array<2, word<uint8>>", words(263, 265), 2),
            ("tuple<word, array<2, word>>", words(7, 9, 11), 96),
            ("tuple<word, memory_bytes>", words(7, 192, 3) + b"abc" + bytes(29), 64),
        ):
            with self.subTest(layout=layout):
                source = (
                    encode(layout)
                    .replace(
                        "v2 = abi_encode",
                        f"v1 = make_memory_slice v0, {count}\nv2 = abi_encode",
                    )
                    .replace("args v0", "args v1")
                )
                self.lowered_pair(
                    source,
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    memory(payload),
                )

    def test_returndata_sources_and_exact_copy_bounds(self):
        for layout, count, payload in (
            ("returndata_bytes", 3, b"abc"),
            ("returndata_array<word>", 2, words(7, 9)),
            ("returndata_array<function>", 1, words(17 << 64)),
        ):
            with self.subTest(layout=layout):
                setup = (
                    f"v9 = staticcall 100000, 4, 128, {len(payload)}, 0, 0\n"
                    f"v0 = inttoptr i256 0 to ptr<returndata>\n"
                    f"v1 = make_returndata_slice v0, {count}\n"
                )
                source = state.module(
                    setup
                    + f"v2 = abi_encode [{layout}], object, args v1\nv3 = mload v2\nret v3"
                )
                initial = memory(payload)
                self.lowered_pair(
                    source,
                    "lower-abi-encode,lower-slices,lower-memory-objects",
                    initial,
                )
                short = source.replace(
                    f"make_returndata_slice v0, {count}",
                    f"make_returndata_slice v0, {count + 1}",
                )
                self.lowered_pair(
                    short, "lower-abi-encode,lower-slices,lower-memory-objects", initial
                )
