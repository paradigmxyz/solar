"""Literal bytes and explicit linked images retain exact copy boundaries."""

import json
import unittest

import test_mir_state as state
from test_cli import ROOT
from test_mir_layout import fixture


def module(body, data='payload_0: hex"11223344"'):
    return (
        "@module Data\n@data\n" + data + "\nfn @f() -> i256 {\nbb0:\n" + body + "\n}\n"
    )


class MirDataTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def test_exact_copy_subrange_and_wrong_offset(self):
        self.pair(
            module("datacopy payload_0+1, 1, 3\nv0 = mload 0\nret v0"),
            module(
                "mstore 0, 0x0022334400000000000000000000000000000000000000000000000000000000\nv0 = mload 0\nret v0"
            ),
        )
        self.pair(
            module("datacopy payload_0, 0, 2\nret 0"),
            module("datacopy payload_0+1, 0, 2\nret 0"),
            status=1,
        )

    def test_zero_copy_and_neighboring_data_are_not_conflated(self):
        maximum = 2**256 - 1
        self.pair(module(f"datacopy payload_0+4, {maximum}, 0\nret 0"), module("ret 0"))
        source = module(
            "datacopy payload_0, 0, 5\nret 0",
            data='payload_0: hex"11223344"\nnext_1: hex"55"',
        )
        result, _ = self.pair(source, source, status=2)
        self.assertIn("datacopy range 0..5 exceeds data size 4", result.stderr)

    def test_link_inputs_are_required_for_deferred_and_relocated_data(self):
        for body, reason in (
            ("v0 = datasize child_0\nret v0", "missing linked data extent"),
            (
                "v0 = datasize child_0\ndatacopy child_0, 0, v0\nret 0",
                "missing linked data extent",
            ),
        ):
            with self.subTest(body=body):
                source = module(body, data='child_0: runtime_code "child.sol:Child"')
                result, _ = self.pair(source, source, status=2)
                self.assertIn(reason, result.stderr)
        source = module(
            "datacopy payload_0, 0, 20\nret 0",
            data='payload_0: hex"' + "00" * 20 + '" library_relocations [0: L_0]',
        )
        source = source.replace("@data", '@libraries\nL_0: "lib.sol:Lib"\n@data')
        result, _ = self.pair(source, source, status=2)
        self.assertIn("missing library relocation", result.stderr)

    def test_deferred_code_copies_use_explicit_extents(self):
        for kind in ("runtime_code", "creation_code"):
            with self.subTest(kind=kind):
                data = f'child_0: {kind} "child.sol:Child"'
                before = module(
                    "v0 = datasize child_0\ndatacopy child_0, 1, v0\nv1 = mload 0\nret v1",
                    data=data,
                )
                after = module(
                    f"mstore 0, {0x00112233 << 224}\nv0 = mload 0\nret v0", data=data
                )
                initial = fixture(
                    code="0xaa112233bb", data={"0": {"offset": 1, "size": 3}}
                )
                self.pair(before, after, fixture=initial)
                self.pair(
                    before,
                    before.replace("child_0, 1", "child_0, 2"),
                    fixture=initial,
                    status=1,
                )

    def test_linked_copies_keep_neighbor_bytes_and_code_padding(self):
        data = 'child_0: runtime_code "child.sol:Child"'
        initial = fixture(code="0xaa1122bb", data={"0": {"offset": 1, "size": 2}})
        for offset, expected in ((1, 0x1122BB00), (3, 0xBB000000), (2**256 - 1, 0)):
            with self.subTest(offset=offset):
                before = module(
                    f"codecopy 0, {offset}, 4\nv0 = mload 0\nret v0", data=data
                )
                after = module(
                    f"mstore 0, {expected << 224}\nv0 = mload 0\nret v0", data=data
                )
                self.pair(before, after, fixture=initial)

    def test_library_patches_preserve_surrounding_bytes(self):
        data = 'payload_0: hex"aa' + "00" * 20 + 'bb" library_relocations [1: L_0]'
        before = module("datacopy payload_0, 0, 22\nv0 = mload 0\nret v0", data=data)
        before = before.replace("@data", '@libraries\nL_0: "lib.sol:Lib"\n@data')
        address = int("112233445566778899aabbccddeeff0011223344", 16)
        word = (0xAA << 248) | (address << 88) | (0xBB << 80)
        after = before.replace("datacopy payload_0, 0, 22", f"mstore 0, {word}")
        self.pair(before, after, fixture=fixture(libraries={"0": str(address)}))
        image = "0x00aa" + address.to_bytes(20, "big").hex() + "bbcc"
        initial = fixture(
            code=image,
            data={"0": {"offset": 1, "size": 22}},
            libraries={"0": str(address)},
        )
        self.pair(before, after, fixture=initial)
        mismatched = {
            **initial,
            "mir_layout": {
                **initial["mir_layout"],
                "libraries": {"0": str(address + 1)},
            },
        }
        result, _ = self.pair(before, before, fixture=mismatched, status=2)
        self.assertIn("linked data disagrees", result.stderr)
        self.pair(
            before,
            after.replace(f"mstore 0, {word}", f"mstore 0, {word ^ (1 << 88)}"),
            fixture=initial,
            status=1,
        )

    def test_known_data_must_agree_with_linked_image(self):
        before = module("datacopy payload_0, 0, 2\nret 0")
        for image, size in (("0x11223345", 4), ("0x11223344", 3)):
            with self.subTest(image=image, size=size):
                initial = fixture(code=image, data={"0": {"offset": 0, "size": size}})
                result, _ = self.pair(before, before, fixture=initial, status=2)
                self.assertIn("linked data disagrees", result.stderr)

    def test_empty_copies_do_not_expand_memory(self):
        data = 'child_0: runtime_code "child.sol:Child"'
        self.pair(
            module(
                f"v0 = datasize child_0\ndatacopy child_0, {2**256 - 1}, v0\nret 7",
                data=data,
            ),
            module("ret 7", data=data),
            fixture=fixture(code="0x", data={"0": {"offset": 0, "size": 0}}),
        )
        self.pair(
            module(f"codecopy {2**256 - 1}, {2**256 - 1}, 0\nret 7"),
            module("ret 7"),
        )

    def test_invalid_deferred_subranges_stay_invalid(self):
        data = 'child_0: runtime_code "child.sol:Child"'
        for body, reason in (
            ("datacopy child_0+1, 0, 0\nret 0", "data offset 1 exceeds data size 0"),
            (
                "datacopy child_0, 0, 4\nret 0",
                "datacopy size of deferred data must be its `datasize`",
            ),
        ):
            with self.subTest(body=body):
                source = module(body, data=data)
                result, _ = self.pair(source, source, status=2)
                self.assertIn(reason, result.stderr)

    def test_documented_link_boundary(self):
        folder = ROOT / "examples/mir/linked-data"
        before = (folder / "before.mir").read_text()
        initial = json.loads((folder / "world.json").read_text())
        self.pair(before, (folder / "after.mir").read_text(), fixture=initial)
        self.pair(
            before, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )
