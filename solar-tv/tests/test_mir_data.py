"""Constant data is logical bytes; unresolved layouts are never guessed."""

import unittest

import test_mir_state as state


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

    def test_unlinked_data_and_relocations_stay_unsupported(self):
        for body, reason in (
            ("v0 = datasize child_0\nret v0", "missing linked data extent"),
            (
                "v0 = datasize child_0\ndatacopy child_0, 0, v0\nret 0",
                "datacopy of unlinked data",
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
        self.assertIn("unlinked data", result.stderr)
