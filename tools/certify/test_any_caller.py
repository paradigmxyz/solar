#!/usr/bin/env python3
"""The any-caller route of tools/certify/generic.py, with no Lean: which callers the generic source
stage is about (caller_route), what the probe asks (the check for every caller beside the fixed
one), what Lowering.lean states for it (check_sym_stage0, checkValueSym_stage0) and what shape.json
records. Every refusal is a BLOCKED reason (exit 3), never a fixed-caller claim about a contract
that reads msg.sender. Run: python3 tools/certify/test_any_caller.py"""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

WHO = {"sig": "who()", "sel": 0xD6D21DFD, "params": []}
NUM = {"sig": "num()", "sel": 0x4E70B1DC, "params": []}

# A core that reads msg.sender, as SolidCore's repr prints it, and one that does not.
SENDER_CORE = '{ functions := [{ name := "who", body := SolidCore.Solidity.Source.Stmt.block [SolidCore.Solidity.Source.Stmt.returnValues [SolidCore.Solidity.Source.Expr.caller]] }] }'
PLAIN_CORE = '{ functions := [{ name := "num", body := SolidCore.Solidity.Source.Stmt.block [SolidCore.Solidity.Source.Stmt.returnValues [SolidCore.Solidity.Source.Expr.word 7]] }] }'
DIFFER = "trees differ: source (Tree.halt true [OByte.byteOf (WExpr.caller) 0]) MIR (Tree.halt true [OByte.byteOf (WExpr.lit 8192) 0])"


def probe(core, **sym):
    res = {"typecheck": "ok", "core": core, "storage": "[]"}
    for sig, verdict in sym.items():
        res[f"fn {sig}"] = "agree"
        res[f"sym fn {sig}"] = verdict
    return res


class Positive(unittest.TestCase):
    def test_any_caller(self):
        # One check, the caller a symbolic word: the stage holds for 0x2000 and for 0xbeef alike.
        caller, reasons = generic.caller_route(
            probe(SENDER_CORE, **{"who()": "agree"}), [WHO]
        )
        self.assertEqual((caller, reasons), ("any", []))

    def test_lowering_states_the_check(self):
        text = generic.lowering_lean("who_1", "Who", [WHO], [], "core", "[]", sym=True)
        self.assertIn("import SolidCore.Refinement.Generic.StageFrom", text)
        self.assertIn(
            "Generic.checkSym fuel mirFuel coreLit SolarTV.Export.Who.stage0 fns = true",
            text,
        )
        self.assertIn(
            "Generic.checkValue mirFuel coreLit SolarTV.Export.Who.stage0 fns .allSym = true",
            text,
        )
        self.assertIn("theorem check_stage0", text)
        fixed = generic.lowering_lean("who_1", "Who", [WHO], [], "core", "[]")
        self.assertNotIn("check_sym_stage0", fixed)
        self.assertNotIn("StageFrom", fixed)

    def test_shape_records_the_route(self):
        # The route "any" carries the checker with contexts' gas bound, on the published statement.
        shape = generic.shape_json(
            "who_1", "Who", [WHO], 100, "none", "who", any_gas=200
        )
        self.assertEqual(shape["generic"]["caller"], "any")
        self.assertEqual(shape["generic"]["check_sym"], "check_sym_stage0")
        self.assertEqual(shape["generic"]["check_value_sym"], "checkValueSym_stage0")
        self.assertEqual(shape["any_caller"]["gas"], "fun _ => 200")
        # The fixed route names no caller (final.py reads that as 0x2000) and no check for every caller.
        fixed = generic.shape_json("num_1", "Num", [NUM], 100, "none")
        self.assertNotIn("caller", fixed["generic"])
        self.assertNotIn("check_sym", fixed["generic"])
        self.assertNotIn("any_caller", fixed)

    def test_probe_asks_both_checks(self):
        text = generic.probe_lean("who_1", "Who", [WHO, NUM])
        for f in (WHO, NUM):
            self.assertIn(f"PROBE fn {f['sig']}: ", text)
            self.assertIn(f"PROBE sym fn {f['sig']}: ", text)
        self.assertIn("Generic.eraseFnSym (Generic.envOf core) fuel fn", text)
        self.assertIn(".allSym with", text)

    def test_caller_free_contract_keeps_its_route(self):
        # No msg.sender: the fixed-caller route is unchanged, whatever the check for every caller says.
        res = probe(PLAIN_CORE, **{"num()": "source tree undefined (erasure)"})
        self.assertEqual(generic.caller_route(res, [NUM]), ("0x2000", []))


class FailsClosed(unittest.TestCase):
    def blocked(self, res, fns):
        caller, reasons = generic.caller_route(res, fns)
        self.assertEqual(caller, "0x2000")
        self.assertTrue(reasons)
        for r in reasons:
            self.assertEqual(r["feature"], "msg.sender")
        # The driver raises Blocked on any reason: exit 3, a BLOCKED line, nothing claimed.
        with self.assertRaises(generic.Blocked):
            raise generic.Blocked(reasons)
        return reasons

    def test_misuse_constant_sender(self):
        # The module returns the constant 0x2000 for msg.sender: the fixed check agrees, this one not.
        reasons = self.blocked(probe(SENDER_CORE, **{"who()": DIFFER}), [WHO])
        self.assertEqual([r["where"] for r in reasons], ["who()"])
        self.assertIn("WExpr.lit 8192", reasons[0]["detail"])

    def test_aliasing_address_for_sender(self):
        # One function aliases msg.sender to ADDRESS: only it is named, the other still agrees.
        diff = "trees differ: source (Tree.halt true [OByte.byteOf (WExpr.caller) 0]) MIR (Tree.halt true [OByte.byteOf (WExpr.lit 4096) 0])"
        reasons = self.blocked(
            probe(SENDER_CORE, **{"who()": diff, "num()": "agree"}), [WHO, NUM]
        )
        self.assertEqual([r["where"] for r in reasons], ["who()"])

    def test_overflow_truncated_sender(self):
        # The module masks CALLER to 128 bits: a sender of 2^128 or more aliases a smaller one.
        diff = "trees differ: source (Tree.halt true [OByte.byteOf (WExpr.caller) 0]) MIR (Tree.halt true [OByte.byteOf (WExpr.bin BinOp.and WExpr.caller (WExpr.lit 340282366920938463463374607431768211455)) 0])"
        self.blocked(probe(SENDER_CORE, **{"who()": diff}), [WHO])

    def test_symbolic_run_undefined(self):
        self.blocked(
            probe(SENDER_CORE, **{"who()": "MIR symbolic run undefined"}), [WHO]
        )

    def test_no_verdict(self):
        # The probe printed no verdict for the check for every caller: refused, never assumed.
        res = probe(SENDER_CORE)
        res["fn who()"] = "agree"
        reasons = self.blocked(res, [WHO])
        self.assertIn("no verdict", reasons[0]["detail"])

    def test_short_sender_atom_spelling(self):
        # The core's literal may name the constructor without its namespace.
        core = SENDER_CORE.replace(
            "SolidCore.Solidity.Source.Expr.caller", "Expr.caller"
        )
        self.blocked(probe(core, **{"who()": DIFFER}), [WHO])

    def test_field_named_caller_is_not_sender(self):
        # A field or a function named caller is not msg.sender.
        core = PLAIN_CORE.replace(
            'name := "num"', 'name := "caller", caller := 1, Foo.Expr.callerX := 2'
        )
        res = probe(core, **{"num()": DIFFER})
        self.assertEqual(generic.caller_route(res, [NUM]), ("0x2000", []))


if __name__ == "__main__":
    unittest.main()
