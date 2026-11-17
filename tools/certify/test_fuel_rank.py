#!/usr/bin/env python3
"""fuel_rank.py's rank against Verify/Gas/FixedFuel/Acyclic.lean's `targets` and `rankOk`, and
shape_gen.py's use of it in NAME.AnyCaller. The Lean side checks the rank on the exact export;
these tests pin the reading and the emitted text. Run under Python 3.14, as generic.py needs:
python3.14 tools/certify/test_fuel_rank.py"""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import asm_cert
import fuel_rank
import test_any_caller_emit as emit_test

D = HERE / "testdata" / "any_caller_emit"


def prog(*blocks):
    return "\n".join(
        f"    -- bb{i}\n    [" + ",\n     ".join(b) + "]" for i, b in enumerate(blocks)
    )


class Rank(unittest.TestCase):
    def test_export(self):
        text = (D / "gen" / "SimpleStorage" / "Evm6.lean").read_text()
        rk = fuel_rank.of_text(text)
        self.assertEqual(rk, [2, 0, 0, 1])
        self.assertEqual(fuel_rank.fuel(rk), 3)

    def test_ranks_check(self):
        # The rank check of Acyclic.rankOk, over Acyclic.targets.
        text = (D / "gen" / "SimpleStorage" / "Evm6.lean").read_text()
        blocks = asm_cert.parse_program(text)
        rk = fuel_rank.rank(blocks)
        n = len(blocks)
        for b, instrs in enumerate(blocks):
            for t in fuel_rank.targets(b, instrs):
                self.assertLess(rk[t] if t < n else 0, rk[b])

    def test_fall_through_and_past_the_end(self):
        rk = fuel_rank.of_text(prog([".op (.stack (.push 0))"], [".jump 7"], [".stop"]))
        self.assertEqual(rk, [2, 1, 0])

    def test_jumpi_and_table(self):
        rk = fuel_rank.of_text(
            prog([".jumpi 2", ".jumpTable [1, 3]"], [".jump 3"], [".ret"], [".invalid"])
        )
        self.assertEqual(rk, [2, 1, 0, 0])

    def test_loop_has_none(self):
        self.assertIsNone(fuel_rank.of_text(prog([".jumpi 1", ".jump 0"], [".stop"])))
        self.assertIsNone(fuel_rank.of_text(prog([".jump 1"], [".jump 0"])))

    def test_dynamic_jump_has_none(self):
        self.assertIsNone(fuel_rank.of_text(prog([".jumpDyn"])))
        self.assertIsNone(fuel_rank.of_text(prog([".jumpiDyn", ".stop"])))

    def test_unknown_instruction_has_none(self):
        self.assertIsNone(fuel_rank.of_text(prog([".frob 3", ".stop"])))


class Emit(unittest.TestCase):
    def test_any_caller_uses_the_rank(self):
        rc, log, out = emit_test.emit(emit_test.any_shape())
        self.assertEqual(rc, 0, log)
        a = out["AnyCaller.lean"]
        R = "Verify.Gas.FixedFuel.Acyclic"
        last = "SolarTV.Export.SimpleStorage.evm6"
        for want in (
            f"import {R}\n",
            f"theorem fuel_rank_ok : {R}.rankOk {last} [2, 0, 0, 1] = true := by decide +kernel",
            f"theorem fuel_rank_le : {R}.rankFuel {last} [2, 0, 0, 1] ≤ 4 := by decide +kernel",
            f"  {R}.ends_of_rank fuel_rank_ok fuel_rank_le o call",
            "      exact ends_rank o call\n",
        ):
            self.assertIn(want, a)
        self.assertNotIn("ends_of_runs", a)

    def test_fixed_route_unchanged(self):
        rc, log, out = emit_test.emit(emit_test.FIXED_SHAPE)
        self.assertEqual(rc, 0, log)
        self.assertEqual(out["Shape.lean"], (D / "fixed.Shape.lean").read_text())


if __name__ == "__main__":
    unittest.main()
