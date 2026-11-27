"""Tests of loop invariant discovery (tools/certify/loop_infer.py).

  python3 tools/certify/test_loop_infer.py      # Z3 names z3 (default ~/.local/bin/z3)

The encoding and translation cases need nothing. The solver cases (skipped without z3) run Spacer
on the exported leaves of Horn.ok_transfer (testdata/loop_infer/horn_ok_transfer.leaves.json,
from solidity-lean's Verify/Phase3/Infer/HornExport.lean) and check it finds a + b == n, and that
wrong candidates fail with the sequent kind and a counterexample. Nothing here is trusted: the
Lean kernel checks what the tool proposes.
"""

import json
import os
import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import loop_infer as m

DATA = HERE / "testdata" / "loop_infer"
LEAVES = json.loads((DATA / "horn_ok_transfer.leaves.json").read_text())
HAVE_Z3 = os.path.exists(m.Z3)
W = m.W


def a(i):
    return m.arg(i)


class Words(unittest.TestCase):
    def test_eval_wraps_like_wexpr_eval(self):
        self.assertEqual(m.weval(["bin", "sub", m.lit(0), m.lit(1)], []), W - 1)
        self.assertEqual(m.weval(["bin", "add", m.lit(W - 1), m.lit(1)], []), 0)
        self.assertEqual(m.weval(["un", "iszero", ["bin", "eq", a(1), m.lit(0)]], [5, 0]), 0)
        self.assertEqual(m.weval(a(7), [1, 2]), 0)  # past the valuation: getD 0

    def test_norm_folds_constant_bytes(self):
        self.assertEqual(m.bnorm(["byteOf", ["bin", "add", m.lit(1), m.lit(2)], 31]), ["blit", 3])
        self.assertEqual(m.bnorm(["byteOf", a(0), 3]), ["byteOf", a(0), 3])

    def test_sub_replaces_header_words(self):
        hs = [["bin", "sub", a(1), m.lit(1)], ["bin", "add", a(2), m.lit(1)]]
        f = ["lin", a(0), a(1), a(2), m.lit(0)]
        self.assertEqual(m.fsub(1, hs, f), ["lin", a(0), hs[0], hs[1], m.lit(0)])

    def test_smt_word_is_exact(self):
        env = lambda e: f"v{e[1]}"  # noqa: E731
        self.assertEqual(m.smt_word(["bin", "sub", a(1), m.lit(1)], env),
                         f"(ite (>= v1 1) (- v1 1) (+ (- v1 1) {W}))")
        # iszero(eq(x, 0)) != 0 is x != 0, i.e. x >= 1 over words.
        self.assertEqual(m.smt_fact(["un", "iszero", ["bin", "eq", a(1), m.lit(0)]], True, env), "(>= v1 1)")
        self.assertEqual(m.smt_fact(["un", "iszero", ["bin", "eq", a(1), m.lit(0)]], False, env), "(<= v1 0)")


class Goals(unittest.TestCase):
    def test_word_outputs_give_word_goals(self):
        H = ["halt", True, [["byteOf", a(2), i] for i in range(32)], [], 0]
        T = m.target_tree("returns-arg:0")
        self.assertIsNone(m.halt_goals(H, T))  # outside Inv.haltGoals
        self.assertEqual(m.dedup(m.halt_goals_w(H, T)), [["eq", a(2), a(0)]])

    def test_different_success_is_unreachable(self):
        H = ["halt", False, [], [], 0]
        self.assertEqual(m.halt_goals_w(H, m.target_tree("returns-arg:0")), [m.FALSE])

    def test_hyp_slices_like_lean(self):
        S = [["le", a(2), a(0)], ["le", a(3), m.lit(4)]]
        K = [["fact", a(1), True]]
        g = ["eq", a(1), a(2)]
        self.assertEqual(m.hyp(S, K, g), [["le", a(2), a(0)], ["fact", a(1), True]])

    def test_sequent_kinds(self):
        I = [["lin", a(0), a(1), a(2), m.lit(0)]]
        kinds = [k.split(":")[0] for _, _, k in m.all_seqs(1, [[]], m.target_tree("returns-arg:0"),
                                                           LEAVES["le"], LEAVES["li"], I, a(1))]
        self.assertEqual(sorted(set(kinds)), ["cover", "decrease", "initiation", "preservation", "sufficiency"])


class Clauses(unittest.TestCase):
    def test_chc_shape(self):
        text, params, slots, live = m.chc(LEAVES["le"], LEAVES["li"], 1, 5, m.target_tree("returns-arg:0"))
        self.assertEqual(live, [0, 1])  # h2..h4 are read by no leaf
        self.assertEqual(params, ["x0", "h0", "h1"])
        self.assertIn("(Inv x0 x0 0)", text)  # init: a = n, b = 0
        self.assertIn("(>= h0 1)", text)  # the loop guard a != 0
        self.assertIn("(not (and (= h1 x0)))", text)  # the query: b == n at the exit
        self.assertEqual(text.count("(assert"), 3)


class ExitWrites(unittest.TestCase):
    """An exit's storage writes are words it reads: header words that only an exit writes stay in
    Inv's parameters (the while route's source system ends by writing every header word)."""

    def test_exit_writes_are_read(self):
        T = ["halt", True, [], [[["lit", 0], ["arg", 1]], [["lit", 1], ["arg", 2]]], 0]
        self.assertIn(["arg", 2], m.leaf_words(["exit", T]))
        li = [([], ["exit", T]), ([], ["back", [["arg", 1], ["arg", 2]]])]
        self.assertEqual(m.live_header(li, 1, 2), [0, 1])
        target = ["halt", True, [], [[["lit", 0], ["arg", 0]], [["lit", 1], ["lit", 0]]], 0]
        text, *_ = m.chc([([], ["back", [["arg", 0], ["lit", 0]]])], li, 1, 2, target)
        self.assertIn("(declare-fun Inv (Int Int Int) Bool)", text)


class Translate(unittest.TestCase):
    def model(self, body):
        return (["x!0", "x!1", "x!2"], m.sexp_parse(body)[0])

    def test_spacer_style_model(self):
        body = ("(let ((a!1 (not (<= (+ x!0 (* (- 1) x!2)) (- 1))))"
                " (a!2 (not (>= (+ x!0 (* (- 1) x!1) (* (- 1) x!2)) 1)))"
                " (a!3 (not (<= (+ x!0 (* (- 1) x!1) (* (- 1) x!2)) (- 1)))))"
                " (and a!1 a!2 a!3))")
        I, dropped = m.translate(self.model(body), 1, 2, [])
        self.assertEqual(dropped, [])
        self.assertIn(["le", a(2), a(0)], I)
        self.assertIn(["lin", a(0), a(1), a(2), m.lit(0)], I)
        for n0, h0 in [(0, 0), (5, 3), (W - 1, W - 1)]:
            vals = [n0, h0, n0 - h0]
            self.assertTrue(all(m.fholds(f, vals) for f in I))
        self.assertFalse(m.fholds(["lin", a(0), a(1), a(2), m.lit(0)], [5, 3, 1]))

    def test_equation_and_constants(self):
        I, _ = m.translate(self.model("(= x!0 (+ x!1 x!2 3))"), 1, 2, [])
        # x0 - x1 - x2 - 3 = 0 as x0 + 0 = x1 + (x2 + 3).
        self.assertEqual(I, [["lin", a(0), a(1), ["bin", "add", a(2), m.lit(3)], m.lit(0)]])
        self.assertTrue(m.fholds(I[0], [10, 4, 3]))

    def test_trivial_word_bounds_drop(self):
        I, _ = m.translate(self.model("(and (>= x!1 0) (<= x!2 x!0))"), 1, 2, [])
        self.assertEqual(I, [["le", a(2), a(0)]])

    def test_untranslatable_is_reported(self):
        I, dropped = m.translate(self.model("(and (<= (* x!1 x!2) x!0) (<= x!2 x!0))"), 1, 2, [])
        self.assertEqual(I, [["le", a(2), a(0)]])
        self.assertEqual(len(dropped), 1)
        self.assertIn("nonlinear", dropped[0][1])

    def test_lean_terms(self):
        self.assertEqual(m.lean_f(["lin", a(0), a(1), a(2), m.lit(0)]),
                         "(F.lin (WExpr.arg 0) (WExpr.arg 1) (WExpr.arg 2) (WExpr.lit 0))")
        self.assertEqual(m.lean_w(["bin", "sub", a(1), m.lit(1)]), "(WExpr.bin BinOp.sub (WExpr.arg 1) (WExpr.lit 1))")


class Concrete(unittest.TestCase):
    def test_bad_spec_has_a_trace(self):
        cex = m.concrete_cex(1, LEAVES["le"], LEAVES["li"], m.target_tree("returns-arg-plus:0:1"), [[0], [3]])
        self.assertEqual(cex["call"], [0])
        self.assertEqual((cex["mir"]["output"], cex["property"]["output"]), (0, 1))

    def test_good_spec_has_none(self):
        self.assertIsNone(m.concrete_cex(1, LEAVES["le"], LEAVES["li"], m.target_tree("returns-arg:0"),
                                         [[0], [1], [3], [7]]))

    def test_measure_candidates(self):
        self.assertEqual(m.measure_cands(1, 5, LEAVES["li"])[0], a(1))


@unittest.skipUnless(HAVE_Z3, "z3 not found")
class Solver(unittest.TestCase):
    def test_spacer_finds_the_transfer_invariant(self):
        st, model, _, (slots, live) = m.spacer(LEAVES["le"], LEAVES["li"], 1, 5, m.target_tree("returns-arg:0"), 60)
        self.assertEqual(st, "sat")
        I, _ = m.translate(model, 1, 5, slots, live)
        self.assertIn(["lin", a(0), a(1), a(2), m.lit(0)], I)
        _, fails = m.check_all(1, LEAVES["cases"], m.target_tree("returns-arg:0"), LEAVES["le"], LEAVES["li"], I, a(1))
        self.assertEqual(fails, [])

    def test_spacer_refutes_the_bad_spec(self):
        st, model, _, _ = m.spacer(LEAVES["le"], LEAVES["li"], 1, 5, m.target_tree("returns-arg-plus:0:1"), 60)
        self.assertEqual((st, model), ("unsat", None))

    def test_wrong_candidate_names_the_sequent(self):
        I = [["le", a(2), a(1)]]
        _, fails = m.check_all(1, LEAVES["cases"], m.target_tree("returns-arg:0"), LEAVES["le"], LEAVES["li"], I, a(1))
        kinds = [f["kind"].split(":")[0] for f in fails]
        self.assertEqual(kinds, ["preservation", "sufficiency"])
        cex = fails[0]["counterexample"]
        # The counterexample really breaks preservation: b <= a, a != 0, but b + 1 > a - 1.
        self.assertTrue(cex["a2"] <= cex["a1"] and cex["a1"] != 0 and cex["a2"] + 1 > cex["a1"] - 1)



# --- The erased-tree binding, the interval fallback and the diagnosis (agent 179) -------------

COUNT10_LE = [[[], ["back", [m.lit(0)]]]]
COUNT10_LI = [
    [[[["bin", "lt", a(0), m.lit(10)], True]], ["back", [["bin", "add", a(0), m.lit(1)]]]],
    [[[["bin", "lt", a(0), m.lit(10)], False]], ["exit", ["halt", True, [["byteOf", a(0), i] for i in range(32)], [], 0]]],
]
TEN = ["halt", True, [["byteOf", m.lit(10), i] for i in range(32)], [], 0]


def leaves_file(d, T=None, **kw):
    L = {"format": "loop leaves 1", "sel": 1, "n": 0, "nhw": 1, "cases": [[]], "le": COUNT10_LE,
         "li": COUNT10_LI, **kw}
    if T is not None:
        L["T"] = T
        L["tree_source"] = "erasure"
    p = pathlib.Path(d) / "leaves.json"
    p.write_text(json.dumps(L))
    return str(p)


class ErasedTree(unittest.TestCase):
    def test_a_target_for_leaves_with_a_source_tree_is_refused(self):
        import contextlib, io, tempfile
        with tempfile.TemporaryDirectory() as d:
            f = leaves_file(d, TEN)
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                rc = m.main([f, "--target", "returns-arg:0"])
            self.assertEqual(rc, 2)
            self.assertIn("refused", json.loads(out.getvalue().splitlines()[-1]))

    def test_leaves_without_a_tree_need_a_target(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(SystemExit):
                m.main([leaves_file(d)])


class Intervals(unittest.TestCase):
    def test_candidates_bound_the_counter_by_the_exit_test(self):
        c = m.interval_cands(0, 1, COUNT10_LE, COUNT10_LI, [[]])
        self.assertIn(["le", a(0), m.lit(10)], c)
        # A bound refuted by a concrete run of the leaves is not a candidate.
        self.assertNotIn(["le", a(0), m.lit(1)], c)

    def test_run_leaves_follows_the_back_edges(self):
        self.assertEqual([v[0] for v in m.run_leaves(0, COUNT10_LE, COUNT10_LI, [])], list(range(11)))

    @unittest.skipUnless(HAVE_Z3, "needs z3")
    def test_the_interval_engine_alone_finds_the_bound(self):
        import contextlib, io, tempfile
        with tempfile.TemporaryDirectory() as d:
            out, cand = io.StringIO(), pathlib.Path(d) / "cand.json"
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                rc = m.main([leaves_file(d, TEN), "--engines", "interval", "--emit-candidate", str(cand)])
            rep = json.loads(out.getvalue().splitlines()[-1])
            self.assertEqual(rc, 0, rep)
            self.assertEqual(rep["source"], "interval")
            c = json.loads(cand.read_text())
            self.assertIn("x0 <= 10", c["I_math"])
            self.assertIn("F.le (WExpr.arg 0) (WExpr.lit 10)", c["I_lean"])

    @unittest.skipUnless(HAVE_Z3, "needs z3")
    def test_a_wrong_source_tree_is_not_given_an_invariant(self):
        import contextlib, io, tempfile
        bad = ["halt", True, [["byteOf", m.lit(11), i] for i in range(32)], [], 0]
        with tempfile.TemporaryDirectory() as d:
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                rc = m.main([leaves_file(d, bad), "--engines", "interval"])
            rep = json.loads(out.getvalue().splitlines()[-1])
            self.assertEqual(rc, 2)
            self.assertEqual(rep["discovery"], "refuted")  # the concrete run ends with 10, not 11
            self.assertIn("counterexample", rep)


class Diagnosis(unittest.TestCase):
    def run_with(self, status):
        import contextlib, io, tempfile, unittest.mock as um
        with tempfile.TemporaryDirectory() as d, um.patch.object(m, "spacer", lambda *a, **k: (status, None, "", ([], []))):
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                rc = m.main([leaves_file(d, TEN), "--engines", "spacer"])
            return rc, json.loads(out.getvalue().splitlines()[-1])

    def test_unknown_is_a_refusal_to_discover_not_a_refutation(self):
        for st in ("unknown", "timeout", "sat (no model)", "no answer"):
            rc, rep = self.run_with(st)
            self.assertEqual(rc, 2)
            self.assertEqual(rep["engines"]["spacer"]["verdict"], "undecided", st)
            self.assertEqual(rep["discovery"], "undecided", st)

    def test_unsat_is_a_refutation_of_the_encoding(self):
        rc, rep = self.run_with("unsat")
        self.assertEqual(rep["engines"]["spacer"]["verdict"], "refuted")

if __name__ == "__main__":
    unittest.main()
