#!/usr/bin/env python3
"""The succeeds witness of a spec without succeeds-if conditions (succeeds_witness.py): the driver's
choice (generic.spec_witness, generic.synth_witness), shape.json's field (generic.shape_json) and the
published statement's witness clause (final.py), which only strengthens the exported allowed
statuses and postcondition. No Lean: that the witness succeeds and meets its precondition is the
Shape module's kernel check. Run: python3 tools/certify/test_succeeds_witness.py"""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic
import succeeds_witness

FINAL = HERE / "final.py"
BASE = json.loads((HERE / "testdata" / "final_caller_shape.json").read_text())
SW = {"function": "set", "sel": 1627429335, "arity": 1, "args": [0], "storage": []}
B = "Certified.c_1.gen.C.Spec.C"
CTX = "SolidCore.Refinement.Shapes.ctxOfCall"


def fn(name, mutates=True, specified=True, succeeds=False, params=("uint256",), sel=1):
    return {
        "name": name,
        "sig": f"{name}({','.join(params)})",
        "params": list(params),
        "mutates": mutates,
        "specified": specified,
        "succeeds": succeeds,
        "sel": sel,
    }


def run_final(shape):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        (d / "shape.json").write_text(json.dumps(shape))
        p = subprocess.run(
            [
                sys.executable,
                "-I",
                str(FINAL),
                str(d / "shape.json"),
                "--name",
                "c_1",
                "--contract",
                "C",
                "--hex",
                "6080604052",
                "--out",
                str(d / "Final.lean"),
                "--statement-out",
                str(d / "Statement.lean"),
                "--record",
                str(d / "record.json"),
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        get = lambda f: (d / f).read_text() if (d / f).exists() else None
        return (
            p.returncode,
            p.stdout + p.stderr,
            get("Statement.lean"),
            get("record.json"),
        )


class Choice(unittest.TestCase):
    def test_succeeds_if_function_first(self):
        fns = [fn("a"), fn("b", succeeds=True)]
        self.assertEqual(generic.spec_witness(fns), "b")

    def test_synthesized_without_succeeds_if(self):
        self.assertEqual(
            generic.spec_witness([fn("a"), fn("v", mutates=False)]),
            generic.SYNTH_WITNESS,
        )

    def test_refused_without_any_spec(self):
        with self.assertRaises(generic.Failed) as e:
            generic.spec_witness([fn("a", specified=False)])
        self.assertIn("no function has @custom:succeeds-if", str(e.exception))

    def test_search_takes_a_succeeding_mutating_spec_function(self):
        fns = [
            fn("v", mutates=False, sel=7),
            fn("u", specified=False, sel=8),
            fn("m", sel=9),
        ]
        res = {
            "run fn v(uint256)": "success",
            "run fn u(uint256)": "success",
            "run fn m(uint256)": "success",
        }
        w = generic.synth_witness(res, fns)
        self.assertEqual(
            w, {"function": "m", "sel": 9, "arity": 1, "args": [0], "storage": []}
        )
        res["run fn m(uint256)"] = "revert"
        self.assertEqual(generic.synth_witness(res, fns)["function"], "v")

    def test_search_refuses_when_no_zero_call_succeeds(self):
        fns = [fn("m"), fn("u", specified=False)]
        res = {"run fn m(uint256)": "revert", "run fn u(uint256)": "success"}
        with self.assertRaises(generic.Failed) as e:
            generic.synth_witness(res, fns)
        self.assertTrue(str(e.exception).startswith("spec: no succeeds witness"))
        self.assertIn("m(uint256): revert", str(e.exception))

    def test_shape_json_names_the_witness(self):
        fns = [dict(fn("set", sel=SW["sel"]), succeeds=False)]
        s = generic.shape_json("c_1", "C", fns, 100, "spec", "set", succeeds_witness=SW)
        self.assertEqual(s["succeeds_witness"], SW)
        with self.assertRaises(generic.Failed):
            generic.shape_json(
                "c_1", "C", fns, 100, "spec", "other", succeeds_witness=SW
            )


class Clause(unittest.TestCase):
    def test_strengthen_is_a_statement_term(self):
        t = succeeds_witness.strengthen("X", SW, "S", B, CTX)
        self.assertNotRegex(t, r"[:\n;]")
        self.assertTrue(t.startswith("fun call obs => (X) call obs ∧ (call = "))
        self.assertIn("obs.success = true", t)

    def test_only_the_zero_call(self):
        for bad in (
            {**SW, "args": [1]},
            {**SW, "storage": [5]},
            {**SW, "arity": 2},
            {**SW, "extra": 1},
        ):
            with self.assertRaises(ValueError):
                succeeds_witness.strengthen("X", bad, "S", B, CTX)


class Statement(unittest.TestCase):
    def test_both_halves_strengthened(self):
        rc, err, stmt, rec = run_final(dict(BASE, succeeds_witness=SW))
        self.assertEqual(rc, 0, err)
        allowed = next(
            l for l in stmt.splitlines() if l.strip().startswith("Allowed _ :=")
        )
        post = next(l for l in stmt.splitlines() if l.strip().startswith("Post _ :="))
        clause = "(call = ⟨_root_.SolidCore.Refinement.Abi.calldata 1627429335 (List.replicate 1 0), 0,"
        for line, base in ((allowed, "bundleAllowed"), (post, "bundlePost")):
            self.assertIn(base, line)
            self.assertIn(clause, line)
            self.assertIn("→ obs.success = true)", line)
        r = json.loads(rec)
        self.assertEqual(r["succeeds_witness"], SW)
        self.assertIn("succeeds witness", r["statuses"]["vacuity"])

    def test_without_the_witness_unchanged(self):
        rc, err, stmt, _ = run_final(BASE)
        self.assertEqual(rc, 0, err)
        self.assertNotIn("obs.success = true)", stmt)

    def test_refusals(self):
        no_nonvac = {k: v for k, v in BASE.items() if k != "nonvacuous"}
        for s in (
            dict(no_nonvac, succeeds_witness=SW, vacuity="none"),
            dict(BASE, succeeds_witness={**SW, "args": [3]}),
            dict(BASE, succeeds_witness=["set"]),
        ):
            rc, err, _, _ = run_final(s)
            self.assertNotEqual(rc, 0)
            self.assertIn("final.py:", err)


if __name__ == "__main__":
    unittest.main()
