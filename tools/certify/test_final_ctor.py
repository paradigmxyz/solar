#!/usr/bin/env python3
"""final.py's statement for every constructor argument list (ctor_args, ctor_returns): the claim
quantifies the argument words, each below its type's bound, against the source deployed with them
(solidCoreFromArgs), with the constructor's return as a premise and a witness when it may revert;
every inconsistent shape fails closed. Runs final.py as the driver does, with no Lean.
Run: python3 tools/certify/test_final_ctor.py"""

import json
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
FINAL = HERE / "final.py"
HEX = "34603b575f3560e01c"
L = "Certified.pair_1.Lowering"

BASE = {
    "kind": "compiled",
    "contract": f"{L}.contract?",
    "fuel": f"{L}.fuel",
    "valid": f"SolidCore.Refinement.Shapes.genericValid ({L}.fns) ({L}.storageOf [A0, A1])",
    "gas": "fun _ => 100",
    "env": [],
    "spec": "none: no specification requested",
    "spec_modules": ["pair_1.Lowering", "Verify.ShapeGeneric"],
    "proof_module": "pair_1.Shape",
    "proof": "Certified.pair_1.Shape.published",
    "ctor_args": [{"var": "A0", "lt": "2 ^ 256"}, {"var": "A1", "lt": "2 ^ 160"}],
}


def run(shape):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        (d / "shape.json").write_text(json.dumps(shape))
        p = subprocess.run(
            [sys.executable, "-I", str(FINAL), str(d / "shape.json"), "--name", "pair_1",
             "--contract", "C", "--hex", HEX, "--out", str(d / "Final.lean"),
             "--statement-out", str(d / "Statement.lean"), "--record", str(d / "record.json")],
            check=False, capture_output=True, text=True,
        )
        st = (d / "Statement.lean").read_text() if (d / "Statement.lean").exists() else None
        rec = json.loads((d / "record.json").read_text()) if (d / "record.json").exists() else None
        return p.returncode, p.stdout + p.stderr, st, rec


def shape(**kw):
    s = json.loads(json.dumps(BASE))
    for k, v in kw.items():
        if v is None:
            s.pop(k, None)
        else:
            s[k] = v
    return s


def claim(st):
    return " ".join(st.split("def Statement : Prop :=", 1)[1].split("set_option maxRecDepth")[0].split())


class CtorArgs(unittest.TestCase):
    def test_every_argument_list(self):
        rc, out, st, rec = run(shape())
        self.assertEqual(rc, 0, out)
        c = claim(st)
        self.assertIn("∀ (A0 A1 : Nat), A0 < 2 ^ 256 → A1 < 2 ^ 160 → ∃ c,", c)
        self.assertIn(
            "_root_.SolidCore.Refinement.Shapes.Compiled (_root_.SolidCore.Refinement.solidCoreFromArgs "
            f"_root_.SolidCore.Refinement.callerAddress ({L}.fuel) [A0, A1]) c", c)
        self.assertNotIn("CtorReturns", c)
        self.assertIn("import SolidCore.Refinement.Generic.StageFrom", st)
        self.assertEqual(rec["ctor_args"], BASE["ctor_args"])

    def test_returns_premise_and_witness(self):
        rc, out, st, rec = run(shape(ctor_returns={"witness": [0, 0]}))
        self.assertEqual(rc, 0, out)
        c = claim(st)
        self.assertIn(
            f"A1 < 2 ^ 160 → _root_.SolidCore.Refinement.Generic.CtorReturns ({L}.fuel) ({L}.contract?) "
            "[A0, A1] → ∃ c,", c)
        self.assertTrue(c.endswith(
            f"∧ _root_.SolidCore.Refinement.Generic.CtorReturns ({L}.fuel) ({L}.contract?) [0, 0]"), c)

    def test_fails_closed(self):
        bad = {
            "wrong name": shape(ctor_args=[{"var": "B0", "lt": "2 ^ 256"}]),
            "wrong bound": shape(ctor_args=[{"var": "A0", "lt": "2 ^ 255"}]),
            "free bound": shape(ctor_args=[{"var": "A0", "lt": "n"}]),
            "empty": shape(ctor_args=[]),
            "extra key": shape(ctor_args=[{"var": "A0", "lt": "2 ^ 256", "x": 1}]),
            "kind": shape(kind="certified", allowed="M.allowed", post="M.post", vacuity="v"),
            "source_sem": shape(source_sem="SolidCore.Refinement.Generic.Loop.solidCoreRuntime"),
            "witness arity": shape(ctor_returns={"witness": [0]}),
            "witness range": shape(ctor_returns={"witness": [0, 2 ** 160]}),
            "witness term": shape(ctor_returns={"witness": ["x", 0]}),
            "env": shape(env=["True"]),
            "pinned": shape(pinned_bytes="M.b", pinned_module="M.B"),
        }
        for why, s in bad.items():
            rc, out, _, _ = run(s)
            self.assertNotEqual(rc, 0, why)
            self.assertTrue(re.search(r"final\.py: shape\.json", out), (why, out))


if __name__ == "__main__":
    unittest.main()
