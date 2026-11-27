"""spec_connect.py's refutations (no Lean): the probe's proposals are parsed, each refutation is
the negation of the obligation it names with its axioms pinned, and the verdict line of each of the
stage-gate negatives (testdata/fixplan/stages.tsv) matches its registered stage and names its
declaration. An error Lean places at a set_option prefix belongs to the theorem below it.

  python3 tools/certify/test_spec_refute.py
"""

import pathlib
import re
import sys
import tempfile
import types
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import spec_connect

MOD = "c_00000000.SpecConnect"
STAGES = [
    l.split("\t")
    for l in (HERE / "testdata" / "fixplan" / "stages.tsv").read_text().splitlines()
    if l and not l.startswith("#")
]
# The proposal each case's run makes (observed on the cases' exports).
PROPOSALS = {
    "clampwrongspec-r2": ("clamp", 1, ("meets0", "[0, 0, 0]", "[0, 0, 0]", "[]")),
    "siblingfns-r2": ("lo", 1, ("meets0", "[0]", "[0]", "[]")),
    "initbroken-r2": ("@init", 0, ("init", None, None, None)),
    "invbroken-r2": ("reset", 1, ("meets0inv", "[]", "[]", "[(0, 1)]")),
    "flag-succeedsalways": ("lower", 1, ("value", None, "[]", "[]")),
    "vacuoussecond-r2a": ("two", 0, ("unsat", None, None, None)),
    "vacuoussecond-r2b": ("two", 1, ("unsat", None, None, None)),
}


def fake_lean(stdout, rc=0):
    """A --lean command printing STDOUT and exiting RC."""
    with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False) as f:
        f.write(f"#!/bin/sh\ncat <<'EOF'\n{stdout}\nEOF\nexit {rc}\n")
    pathlib.Path(f.name).chmod(0o755)
    return f.name


def args_for(lean, out):
    return types.SimpleNamespace(
        lean=lean,
        cwd=tempfile.gettempdir(),
        out=out,
        module_name=MOD,
        spec_module="c_00000000.gen.C.Spec",
        bundle="B",
    )


FN = (0, "C.clamp", "clamp", 3, 1, "T", ([], []), 1)


class Parse(unittest.TestCase):
    def test_proposals(self):
        out = (
            "SPEC_CONNECT initinv\n"
            "SPEC_CONNECT cex clamp meets0 [0, 0, 0] [0, 0, 0] []\n"
            "SPEC_CONNECT cex reset meets0inv [] [] [(0, 1)]\n"
            "SPEC_CONNECT nocex one\n"
            "SPEC_CONNECT value lower [] []\n"
            "SPEC_CONNECT unsat two"
        )
        got = spec_connect.search_cex(args_for(fake_lean(out), None), [FN], "[]")
        self.assertEqual(got["@init"], [("init", None, None, None)])
        self.assertEqual(got["clamp"], [("meets0", "[0, 0, 0]", "[0, 0, 0]", "[]")])
        self.assertEqual(got["reset"], [("meets0inv", "[]", "[]", "[(0, 1)]")])
        self.assertEqual(got["lower"], [("value", None, "[]", "[]")])
        self.assertEqual(got["two"], [("unsat", None, None, None)])
        self.assertNotIn("one", got)

    def test_failed_probe_proposes_nothing(self):
        out = "SPEC_CONNECT cex clamp meets0 [0, 0, 0] [0, 0, 0] []"
        self.assertEqual(
            spec_connect.search_cex(args_for(fake_lean(out, 1), None), [FN], "[]"), {}
        )


class Blocks(unittest.TestCase):
    def test_statements(self):
        cases = [
            (
                "clamp",
                3,
                1,
                ("meets0", "[0, 0, 0]", "[0, 0, 0]", "[]"),
                "theorem clamp_meets0_refuted : ¬ TreeMeetsFixed spec clamp_spec.noSucc ctx 3 clamp_tree := by",
            ),
            (
                "f",
                1,
                0,
                ("meets0", "[0]", "[0]", "[]"),
                "theorem f_meets_refuted : ¬ TreeMeetsFixed spec f_spec ctx 1 f_tree := by",
            ),
            (
                "g",
                0,
                1,
                ("succeeds", "[]", "[]", "[]"),
                "theorem g_succeeds_refuted : ¬ TreeSucceeds spec g_spec ctx 0 g_tree := by",
            ),
            (
                "lower",
                0,
                1,
                ("value", None, "[]", "[]"),
                "theorem lower_value_refuses_refuted : ¬ ValueRefuses spec lower_spec := by",
            ),
            (
                "two",
                1,
                0,
                ("unsat", None, None, None),
                "theorem two_requires_unsat : ∀ (c : Ctx) (iargs : List Int) (pre : State),",
            ),
            (
                "",
                0,
                0,
                ("init", None, None, None),
                "theorem invariants_initial_refuted : ¬ spec.satAll spec.spec.invariants",
            ),
        ]
        for fid, n, nsi, cex, stmt in cases:
            with self.subTest(fid or "init"):
                t = "\n".join(spec_connect.refutation_block(fid, n, nsi, cex, MOD, "B"))
                self.assertIn(stmt, t)
                thm = stmt.split()[1]
                self.assertIn(
                    f"/-- info: '{MOD}.{thm}' depends on axioms: [propext, Classical.choice, Quot.sound] -/\n"
                    f"#guard_msgs (whitespace := lax) in\n#print axioms {thm}",
                    t,
                )
                for bad in ("sorry", "native_decide", "axiom ", "@[extern"):
                    self.assertNotIn(bad, t)


class Verdicts(unittest.TestCase):
    def test_registered_stage_and_declaration(self):
        out = pathlib.Path(tempfile.mkdtemp()) / "SpecConnect.lean"
        out.write_text("")
        ok = fake_lean("", 0)
        self.assertEqual(len(STAGES), 7)
        for name, _source, stage, decl in STAGES:
            with self.subTest(name):
                fid, nsi, cex = PROPOSALS[name]
                fns = (
                    []
                    if fid == "@init"
                    else [(0, "C." + fid, fid, 1, 1, "T", ([], []), nsi)]
                )
                with self.assertRaises(SystemExit) as e:
                    spec_connect.refutation_verdict(
                        args_for(ok, out), fns, {fid: [cex]}
                    )
                line = str(e.exception.code).removeprefix("spec_connect: ")
                self.assertRegex(line, stage)
                self.assertRegex(
                    line, r"refuted in the kernel by \S*\." + re.escape(decl) + r"\b"
                )

    def test_failing_refutation_returns(self):
        out = pathlib.Path(tempfile.mkdtemp()) / "SpecConnect.lean"
        out.write_text("")
        bad = fake_lean("SpecConnect.lean:3:0: error: unsolved goals", 1)
        self.assertIsNone(
            spec_connect.refutation_verdict(
                args_for(bad, out), [FN], {"clamp": [("meets0", "[0]", "[0]", "[]")]}
            )
        )
        self.assertTrue(out.with_suffix(".refute.log").exists())


class Attribution(unittest.TestCase):
    def test_set_option_prefix(self):
        lines = [
            "theorem deploy_ok : True := trivial",
            "",
            "set_option maxHeartbeats 4000000 in",
            "/-- doc -/",
            "theorem clamp_meets0 : False := by",
            "  simp",
        ]
        for text, want in (
            ("SpecConnect.lean:3:0: error: (deterministic) timeout", "clamp_meets0"),
            ("SpecConnect.lean:6:2: error: unsolved goals", "clamp_meets0"),
            ("SpecConnect.lean:1:30: error: x", "deploy_ok"),
        ):
            self.assertEqual(spec_connect.failing_theorems(text, lines)[0][0], want)


if __name__ == "__main__":
    unittest.main()
