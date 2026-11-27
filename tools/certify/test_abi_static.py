#!/usr/bin/env python3
"""The static ABI envelope of tools/certify/generic.py, with no Lean: static_words flattens a static
parameter into the head words SolidCore/Refinement/AbiStatic.lean's Static.flat lists (bytes32 as
.uint 256, whose clean words are the same: Abi.Word.clean_bytes32), return_ok admits static results
and one bytes or string, and every dynamic type, every aggregate holding one and every bytesN below
32 bytes stays refused, each with its own reason. Run: python3 tools/certify/test_abi_static.py"""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

ENUMS = {"Mode": 3}


def comp(t, name="x", **kw):
    return {"type": t, "name": name, **kw}


class StaticWords(unittest.TestCase):
    def words(self, p):
        ws, why = generic.static_words(p, ENUMS)
        self.assertIsNone(why)
        return ws

    def refused(self, p):
        ws, why = generic.static_words(p, ENUMS)
        self.assertIsNone(ws)
        return why[0]

    def test_words(self):
        self.assertEqual(self.words(comp("uint256")), [".uint 256"])
        self.assertEqual(self.words(comp("uint8")), [".uint 8"])
        self.assertEqual(self.words(comp("int16")), [".int 16"])
        self.assertEqual(self.words(comp("address")), [".uint 160"])
        self.assertEqual(self.words(comp("bool")), [".bool"])
        self.assertEqual(self.words(comp("uint8", internalType="enum Mode")), [".enum 3"])

    def test_bytes32(self):
        self.assertEqual(self.words(comp("bytes32")), [".uint 256"])

    def test_fixed_array(self):
        self.assertEqual(self.words(comp("uint256[3]")), [".uint 256"] * 3)
        # uint8[2][3]: three uint8[2], six words.
        self.assertEqual(self.words(comp("uint8[2][3]")), [".uint 8"] * 6)
        self.assertEqual(self.words(comp("uint256[0]")), [])

    def test_struct(self):
        s = comp("tuple", internalType="struct S", components=[comp("uint256", "a"), comp("address", "b")])
        self.assertEqual(self.words(s), [".uint 256", ".uint 160"])
        arr = comp("tuple[2]", internalType="struct S[2]", components=s["components"])
        self.assertEqual(self.words(arr), [".uint 256", ".uint 160"] * 2)
        nested = comp("tuple", internalType="struct T", components=[s, comp("bytes32", "c")])
        self.assertEqual(self.words(nested), [".uint 256", ".uint 160", ".uint 256"])

    def test_dynamic_refused(self):
        self.assertEqual(self.refused(comp("bytes")), "ABI types: dynamic parameter")
        self.assertEqual(self.refused(comp("string")), "ABI types: dynamic parameter")
        self.assertEqual(self.refused(comp("uint256[]")), "ABI types: dynamic parameter")
        s = comp("tuple[]", internalType="struct S[]", components=[comp("uint256", "a")])
        self.assertEqual(self.refused(s), "ABI types: dynamic parameter")

    def test_nested_dynamic_refused(self):
        self.assertEqual(self.refused(comp("bytes[]")), "ABI types: nested dynamic type")
        self.assertEqual(self.refused(comp("string[2]")), "ABI types: nested dynamic type")
        self.assertEqual(self.refused(comp("uint256[][2]")), "ABI types: nested dynamic type")
        s = comp("tuple", internalType="struct S", components=[comp("uint256", "a"), comp("bytes", "b")])
        self.assertEqual(self.refused(s), "ABI types: nested dynamic type")
        deep = comp("tuple", internalType="struct T", components=[comp("tuple", "s", internalType="struct S", components=s["components"])])
        self.assertEqual(self.refused(deep), "ABI types: nested dynamic type")
        arr = comp("tuple[]", internalType="struct S[]", components=s["components"])
        self.assertEqual(self.refused(arr), "ABI types: nested dynamic type")

    def test_other_refused(self):
        self.assertEqual(self.refused(comp("bytes4")), "ABI types: bytesN below 32 bytes")
        self.assertEqual(self.refused(comp("bytes4[2]")), "ABI types: bytesN below 32 bytes")
        self.assertEqual(self.refused(comp("function")), "ABI types")
        self.assertEqual(self.refused(comp("uint8", internalType="enum Unknown")), "ABI types")
        self.assertEqual(self.refused(comp("tuple", internalType="struct E", components=[])), "ABI types")


class Returns(unittest.TestCase):
    def test_admitted(self):
        for t in ("bytes", "string", "bytes32", "uint256[2]", "uint256"):
            self.assertIsNone(generic.return_ok(comp(t), ENUMS), t)
        s = comp("tuple", internalType="struct P", components=[comp("uint256", "a"), comp("uint128", "b")])
        self.assertIsNone(generic.return_ok(s, ENUMS))

    def test_refused(self):
        self.assertEqual(generic.return_ok(comp("uint256[]"), ENUMS)[0], "ABI types: dynamic return")
        self.assertEqual(generic.return_ok(comp("bytes[]"), ENUMS)[0], "ABI types: nested dynamic type")
        self.assertEqual(generic.return_ok(comp("bytes4"), ENUMS)[0], "ABI types: bytesN below 32 bytes")


class Canon(unittest.TestCase):
    def test_tuple_signature(self):
        s = comp("tuple", internalType="struct A.Item", components=[comp("uint256", "amount")])
        self.assertEqual(generic.abi_canon(s), "(uint256)")
        arr = comp("tuple[2]", components=[comp("uint8", "a"), comp("tuple", "b", components=[comp("address", "c")])])
        self.assertEqual(generic.abi_canon(arr), "(uint8,(address))[2]")
        self.assertEqual(generic.abi_canon(comp("uint256[3]")), "uint256[3]")


if __name__ == "__main__":
    unittest.main()
