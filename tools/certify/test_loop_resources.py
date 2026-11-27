"""Tests of tools/certify/loop_resources.py.

The offline tests run the generator on checked-in exports (Counter's and Bump's). The shape tests run
it on the final EVM IR of solar's own exports (with SOLAR set), one per loop layout the generator
covers (a counter word stepped up to zero with a calldata, storage or literal limit; the counter
compared with its limit; a rotated loop whose body writes storage), and on hand-mutated copies: a
body that steps the counter by two must be refused by name, and so must a nested loop. The Lean
side (the generated module building, and refusing mutated modules for their proof reason) is
tools/certify/loop_resources_check.sh.
"""

import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import loop_resources

SOLAR = os.environ.get("SOLAR", str(HERE.parents[1] / "target" / "debug" / "solar"))

COUNTER = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C {
    uint256 public count;
    function bump(uint256 a) external {
        for (uint256 i = 0; i < a; ++i) {
            count += 1;
        }
    }
}
"""


def export(src_text, tmp):
    src = pathlib.Path(tmp) / "C.sol"
    src.write_text(src_text)
    out = subprocess.run(
        [
            SOLAR,
            "C.sol",
            "-Ogas",
            "--evm-version",
            "osaka",
            "--emit=bin-runtime",
            "-Zemit-lean-stages=gen",
        ],
        cwd=tmp,
        check=False,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        raise AssertionError(out.stderr[-400:])
    return pathlib.Path(tmp) / "gen" / "C"


def generate(gen, contract="C", sel=0xB20EB4C4, nargs=1):
    bt = (gen / "Bytecode.lean").read_text()
    plain = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', bt).group(1)
    ob = loop_resources.obligation(gen, plain)
    last = ob["final_evm_ir"]
    return loop_resources.generate(
        ns="T.LoopResources",
        contract=contract,
        last=last[0].lower() + last[1:],
        gen_dir=gen,
        ob=ob,
        code_term="SolarTV.Export.C.code",
        code_import="T",
        sel=sel,
        nargs=nargs,
    )


def mutate(gen, old, new, count=1):
    """Replace OLD by NEW in the final EVM IR (exactly COUNT times)."""
    f = max(gen.glob("Evm*.lean"), key=lambda p: int(p.stem[3:]))
    t = f.read_text()
    assert t.count(old) >= count, (old, t.count(old))
    f.write_text(t.replace(old, new, count))


class Words(unittest.TestCase):
    def test_render(self):
        e = ("op", "add", loop_resources.lit(1), ("var", "w0"))
        self.assertEqual(loop_resources.render(e), "EvmRules.Evm.add 1 w0")
        self.assertEqual(
            loop_resources.render(("cdw", loop_resources.lit(4))), "calldataWord call 4"
        )

    def test_hyp(self):
        c = ("op", "eq", ("var", "a"), loop_resources.lit(5))
        self.assertEqual(loop_resources.hyp(c, True), "a = 5")
        self.assertEqual(loop_resources.hyp(c, False), "¬a = 5")
        self.assertEqual(loop_resources.hyp(("var", "w"), True), "¬w = 0")

    def test_evaluate(self):
        f = {"cv": 0, "cds": 36, "sel": 7}
        sel = ("op", "shr", loop_resources.lit(224), ("cdw", loop_resources.lit(0)))
        self.assertEqual(
            loop_resources.evaluate(("op", "eq", sel, loop_resources.lit(7)), f), 1
        )
        self.assertIsNone(loop_resources.evaluate(("cdw", loop_resources.lit(4)), f))


def sstore(key, value):
    """SSTORE(KEY, VALUE): KEY and VALUE are pushed literals, or None for the entry stack's top
    word (a computed slot, or a value the block did not make)."""
    out = []
    for w in (value, key):
        if w is None:
            out.append(("dup", len(out)))
        else:
            out.append(("push", w))
    return out + [("effect", "sstore", 2)]


def sload(key):
    return [("push", key), ("effect", "sload", 1)]


class StoreReads(unittest.TestCase):
    """A storage read after a write in the same block: forwarded only when no write could alias
    the read slot. X is the entry stack's top word, a computed slot."""

    X = ("var", "x")

    def end_stack(self, instrs):
        _, end = loop_resources.run_path(instrs + [("jump", 1)], [self.X], "s", {}, [])
        return end[2]

    def refused(self, instrs):
        with self.assertRaises(loop_resources.Unsupported) as e:
            self.end_stack(instrs)
        self.assertRegex(str(e.exception), "may alias")

    def test_literal_then_computed_write_is_refused(self):
        # SSTORE(3, 11); SSTORE(x, 22); SLOAD(3): at x = 3 the machine reads 22, not 11.
        self.refused(sstore(3, 11) + sstore(None, 22) + sload(3))
        self.refused(sstore(3, 11) + sstore(None, 22) + sload(4))

    def test_computed_then_literal_write(self):
        # SSTORE(x, 22); SSTORE(3, 11): slot 3 holds 11 whatever x is; slot 4 may hold 22.
        pre = sstore(None, 22) + sstore(3, 11)
        self.assertEqual(self.end_stack(pre + sload(3))[0], loop_resources.lit(11))
        self.refused(pre + sload(4))

    def test_literal_write_reestablishes_a_slot(self):
        # SSTORE(3, 11); SSTORE(x, 22); SSTORE(3, 33): slot 3 holds 33; slot 4 may hold 22.
        pre = sstore(3, 11) + sstore(None, 22) + sstore(3, 33)
        self.assertEqual(self.end_stack(pre + sload(3))[0], loop_resources.lit(33))
        self.refused(pre + sload(4))
        self.refused(pre + sload(4) + sload(3))

    def test_literal_slots(self):
        # Literal writes only: the slot written reads the word written, another the entry state.
        pre = sstore(3, 11) + sstore(4, 12) + sstore(3, 13)
        self.assertEqual(
            self.end_stack(pre + sload(3) + sload(5))[:2],
            [("sload", "s", loop_resources.lit(5)), loop_resources.lit(13)],
        )
        self.refused(sstore(None, 22) + [("dup", 0), ("effect", "sload", 1)])
        self.refused(sstore(3, 11) + [("dup", 0), ("effect", "sload", 1)])

    def test_straight_line_fact_resolves_reads_before_splitting(self):
        # A block with no jumpi never splits, so sym_exec_st alone leaves
        # `update s.storage 3 v 3` in the goal: the fact resolves the reads once before splitting.
        blocks = [sstore(3, None) + sload(3) + sload(4) + [("jump", 1)], [("stop",)]]
        old = loop_resources.BLOCKS
        loop_resources.BLOCKS = blocks
        try:
            m = loop_resources.Module("P")
            m.block_lemma(0, [("cdw", loop_resources.lit(4))], [], "goto", {})
            text = m.lemmas[0]
        finally:
            loop_resources.BLOCKS = old
        self.assertIn("some (.inr (1, [s.storage 4, v0, v0], s'))", text)
        self.assertTrue(
            text.endswith(
                ":= ⟨_, by\n"
                "    obtain ⟨cd, hcd⟩ : ∃ cd, cd = calldataWord call := ⟨_, rfl⟩\n"
                "    sym_norm\n"
                "    try simp (config := { decide := true }) only\n"
                "      [Verify.Gas.FixedFuel.Measured.update_apply, ite_true, ite_false] at *\n"
                "    all_goals sym_exec_st⟩\n"
            ),
            text,
        )

    def test_fact_without_reads_after_writes(self):
        blocks = [sload(3) + sstore(3, None) + [("jump", 1)], [("stop",)]]
        old = loop_resources.BLOCKS
        loop_resources.BLOCKS = blocks
        try:
            m = loop_resources.Module("P")
            m.block_lemma(0, [("cdw", loop_resources.lit(4))], [], "goto", {})
        finally:
            loop_resources.BLOCKS = old
        self.assertTrue(m.lemmas[0].endswith(":= ⟨_, by sym_exec⟩\n"), m.lemmas[0])


class Offline(unittest.TestCase):
    """The generator on a checked-in export of Counter (`bump(n)`'s loop): its final EVM IR and
    runtime bytes, so these run without a solar binary."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gen = pathlib.Path(self.tmp) / "Counter"
        shutil.copytree(HERE / "testdata" / "loop_resources" / "Counter", self.gen)

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def test_counter_generates(self):
        text = generate(self.gen, "Counter")
        for want in (
            "abbrev record : LoopRecord := Measured.record",
            "refine Measured.loops_of (H := 8) (n := 4) Measured.always (Measured.down 1)",
            "Counted.meas_add_one hk",
            "Measured.meas_neg_le (Measured.arg_le ha hK)",
            "∃ σ a0, a0 < 2 ^ 256 ∧ a0 ≤ K ∧ call =",
            "theorem check_ok",
            "theorem verdict",
            "theorem fit {K : Nat} (hK : K ≤ Kmax)",
            "theorem premises {K : Nat} (hK : K ≤ Kmax)",
            "def Kmax : Nat := 13042098468400",
            "theorem resources_evmir",
        ):
            self.assertIn(want, text)
        # one fact per block and path: the header's three, the body's three, the exit's two, the
        # entry's six
        self.assertEqual(len(re.findall(r"^theorem b\d+_\d+ ", text, re.MULTILINE)), 14)
        self.assertNotIn("sorry", text)
        self.assertNotIn("native_decide", text)
        self.assertNotIn("axiom ", text)

    def test_bump_generates(self):
        # tools/certify/testdata/loop_equiv/Bump.sol, the corpus's second counted loop.
        gen = pathlib.Path(self.tmp) / "Bump"
        shutil.copytree(HERE / "testdata" / "loop_resources" / "Bump", gen)
        text = generate(gen, "Bump")
        for want in (
            "Header block 4 (depth 4; header word 1 steps by one up to zero)",
            "refine Measured.loops_of (H := 4) (n := 4) Measured.always (Measured.down 1)",
            "theorem premises {K : Nat} (hK : K ≤ Kmax)",
        ):
            self.assertIn(want, text)
        self.assertNotIn("sorry", text)

    def test_step_by_two_is_refused(self):
        mutate(
            self.gen,
            "     .op (.stack (.push 1)),\n     .op (.stack .add),\n     .op (.stack (.push 1)),\n     .jump 8]",
            "     .op (.stack (.push 2)),\n     .op (.stack .add),\n     .op (.stack (.push 1)),\n     .jump 8]",
        )
        with self.assertRaises(loop_resources.Unsupported) as e:
            generate(self.gen, "Counter")
        self.assertRegex(str(e.exception), "stepping the counter by one")


@unittest.skipUnless(os.access(SOLAR, os.X_OK), "no solar binary (set SOLAR)")
class Shapes(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gen = export(COUNTER, self.tmp)

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def test_counter_generates(self):
        text = generate(self.gen)
        for want in (
            "theorem resources_evmir",
            "refine Measured.loops_of",
            "theorem premises",
            "theorem fit",
            "theorem loops",
            "theorem verdict",
            "theorem check_ok",
            "theorem inhabited",
            "theorem total",
        ):
            self.assertIn(want, text)
        self.assertNotIn("sorry", text)
        self.assertNotIn("native_decide", text)
        self.assertNotIn("axiom ", text)

    def test_step_by_two_is_refused(self):
        # The body's increment of the counter word (add 1) becomes add 2.
        f = max(self.gen.glob("Evm*.lean"), key=lambda p: int(p.stem[3:]))
        t = f.read_text()
        hb = re.search(r"-- bb(\d+)\n((?:.|\n)*?)(?=\n    -- bb|\n  \])", t)
        self.assertIsNotNone(hb)
        n = t.count(".op (.stack (.push 1))")
        self.assertGreater(n, 0)
        f.write_text(
            t.replace(
                ".op (.stack (.push 1)),\n     .op (.stack .add)",
                ".op (.stack (.push 2)),\n     .op (.stack .add)",
            )
        )
        with self.assertRaises(loop_resources.Unsupported) as e:
            generate(self.gen)
        self.assertRegex(str(e.exception), "stepping the counter by one|counter word")

    def shape(self, src, wants, sel=0xB20EB4C4, nargs=1):
        tmp = tempfile.mkdtemp()
        try:
            text = generate(export(src, tmp), sel=sel, nargs=nargs)
            for want in wants:
                self.assertIn(want, text)
            self.assertNotIn("sorry", text)
        finally:
            shutil.rmtree(tmp)

    def test_literal_limit(self):
        # A literal limit: a constant iteration bound, no K.
        self.shape(
            COUNTER.replace("bump(uint256 a)", "bump()").replace("i < a", "i < 10"),
            ["abbrev M : Nat := 10", "theorem premises :", "Measured.call_facts0"],
            sel=0x68110B2F,
            nargs=0,
        )

    def test_storage_limit(self):
        # A limit read from storage: Valid K bounds the slot's value.
        src = COUNTER.replace(
            "uint256 public count;", "uint256 count;\n    uint256 limit;"
        )
        src = src.replace("bump(uint256 a)", "bump()").replace("i < a", "i < limit")
        self.shape(
            src,
            ["SolarTV.slotValue σ 1 ≤ K", "Measured.meas_neg_le (hst)"],
            sel=0x68110B2F,
            nargs=0,
        )

    def test_counter_compared_with_limit(self):
        # A counter that starts at 2: the header compares it with the limit.
        self.shape(
            COUNTER.replace("uint256 i = 0", "uint256 i = 2"),
            ["(Measured.up ", "Measured.upM_step hk", "Measured.up_le"],
        )

    def test_rotated_loop_writing_storage(self):
        # Two slots stepped each iteration: the test ends the body block, which writes storage.
        src = COUNTER.replace(
            "uint256 public count;", "uint256 count;\n    uint256 other;"
        ).replace("count += 1;", "count++;\n            other++;")
        self.shape(src, ["(Measured.nonzero 0)", "sym_exec_st"])

    def test_nested_loop_is_refused(self):
        tmp = tempfile.mkdtemp()
        try:
            src = COUNTER.replace(
                "count += 1;",
                "for (uint256 j = 0; j < a; ++j) {\n                count += 1;\n            }",
            )
            with self.assertRaises(loop_resources.Unsupported):
                generate(export(src, tmp))
        finally:
            shutil.rmtree(tmp)


if __name__ == "__main__":
    unittest.main()
