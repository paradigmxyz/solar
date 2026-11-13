"""Recursive ABI shape semantics against real conversion passes and original-MIR replay."""

import unittest

import test_mir_abi as abi
import test_mir_state as state


def world(payload=b"", *, calldata=False):
    data = bytes(64) + (
        128 if calldata else (128 + len(payload) + 31) // 32 * 32
    ).to_bytes(32, "big")
    if not calldata:
        data = data.ljust(96, b"\0") + len(payload).to_bytes(32, "big") + payload
    result = {"memory": {"data": "0x" + data.hex()}}
    if calldata:
        result["calldata"] = "0x" + payload.hex()
    return result


def words(*values):
    return b"".join(value.to_bytes(32, "big") for value in values)


def decode(layout, result, *, calldata=False, input_length=None):
    if calldata:
        length = input_length if input_length is not None else 32
        prefix = f"v0 = inttoptr i256 0 to ptr<calldata>\nv1 = make_calldata_slice v0, {length}\n"
        source = "v1"
    else:
        prefix = "v0 = inttoptr i256 96 to ptr<memory>\n"
        source = "v0"
    return state.module(prefix + f"v2 = abi_decode [{layout}], {source}\n" + result)


class MirRecursiveAbiTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def check_decode(self, layout, payload, result, *, calldata=False):
        source = decode(layout, result, calldata=calldata, input_length=len(payload))
        self.lowered_pair(
            source,
            "lower-abi,lower-structs,lower-slices,lower-memory-objects",
            world(payload, calldata=calldata),
        )
        return source

    def test_multiple_scalar_results_and_widths(self):
        payload = words(255, 1, 17)
        # The parser interns the three-field result as struct0.
        self.check_decode(
            "uint8, bool, uint256",
            payload,
            "v3 = extract_value struct0, v2, 0\nv4 = zext i8 v3 to i256\nret v4",
        )
        self.check_decode(
            "uint8, bool, uint256",
            payload,
            "v3 = extract_value struct0, v2, 2\nret v3",
        )

    def test_bytes_all_lengths_and_locations(self):
        for length in (0, 1, 31, 32, 33):
            payload = words(32, length) + bytes(range(length)).ljust(
                (length + 31) // 32 * 32, b"\xff"
            )
            for calldata in (False, True):
                with self.subTest(length=length, calldata=calldata):
                    self.check_decode(
                        "bytes", payload, "v3 = mload v2\nret v3", calldata=calldata
                    )

    def test_arrays_and_nested_tuples(self):
        cases = (
            ("array<2, uint8>", words(7, 9)),
            ("array<_, uint8>", words(32, 2, 7, 9)),
            ("array<_, function>", words(32, 1, 17 << 64)),
            ("tuple<uint8, array<2, bool>>", words(7, 1, 0)),
            ("tuple<uint256, bytes>", words(32, 7, 64, 3) + b"abc" + bytes(29)),
            (
                "array<2, bytes>",
                words(32, 64, 128, 1) + b"a" + bytes(31) + words(2) + b"bc" + bytes(30),
            ),
            (
                "array<_, bytes>",
                words(32, 2, 64, 128, 1)
                + b"a"
                + bytes(31)
                + words(2)
                + b"bc"
                + bytes(30),
            ),
        )
        for layout, payload in cases:
            for calldata in (False, True):
                with self.subTest(layout=layout, calldata=calldata):
                    self.check_decode(
                        layout,
                        payload,
                        "v3 = ptrtoint ptr<memory> v2 to i256\nret v3",
                        calldata=calldata,
                    )

    def test_offsets_lengths_and_dirty_words_revert(self):
        for layout, payload in (
            ("bytes", words(2**256 - 1, 0)),
            ("bytes", words(32, 33)),
            ("array<_, uint8>", words(32, 2, 7, 256)),
            ("tuple<uint256, bytes>", words(32, 7, 2**256 - 1)),
            ("uint8, bool", words(1, 2)),
        ):
            with self.subTest(layout=layout):
                source = decode(layout, "ret 0")
                initial = world(payload)
                self.pair(source, state.module("revert 0, 0"), fixture=initial)
                self.lowered_pair(
                    source,
                    "lower-abi,lower-structs,lower-slices,lower-memory-objects",
                    initial,
                )

    def test_corrupt_offset_is_a_counterexample(self):
        payload = words(32, 3) + b"abc" + bytes(29)
        initial = world(payload)
        source = decode("bytes", "v3 = mload v2\nret v3")
        bad = source.replace("v2 = abi_decode", "mstore 128, 64\nv2 = abi_decode")
        self.pair(source, bad, fixture=initial, status=1)
