#!/usr/bin/env python3
"""final.py's caller route (caller_route, expected_mode): the fixed route's published statement is
the fixed-frame statement byte for byte and claims no other caller; the "any" route adds the
equivalence in every real context and proves it with the shape's any_caller proof; every
inconsistent shape fails closed. Runs final.py as the driver does, on a small shape, with no Lean.
Run: python3 tools/certify/test_final_caller.py"""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
FINAL = HERE / "final.py"
NS = "Certified.c_1.Final"
L = "Certified.c_1.Lowering"

# A shape.json as the driver writes it for a generic-stage contract (SimpleStorage's, renamed c_1 and C).
BASE = json.loads((HERE / "testdata" / "final_caller_shape.json").read_text())
FIXED_GENERIC = dict(BASE["generic"], caller="0x2000")
ANY_GENERIC = dict(
    BASE["generic"],
    caller="any",
    check_sym="check_sym_stage0",
    check_value_sym="checkValueSym_stage0",
)
ANY_PROOF = {
    "module": "c_1.AnyCaller",
    "proof": "Certified.c_1.AnyCaller.published_any",
    "gas": "fun _ => 2000",
}
FORALL = "∀ ctx : _root_.SolidCore.Verify.Generic.Context, ctx.Ok →"


def run(shape):
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
        out = {
            k: (d / f).read_text() if (d / f).exists() else None
            for k, f in (
                ("final", "Final.lean"),
                ("statement", "Statement.lean"),
                ("record", "record.json"),
            )
        }
        return p.returncode, p.stdout + p.stderr, out


def shape(**kw):
    s = json.loads(json.dumps(BASE))
    s.update(kw)
    return s


class Route(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixed = run(shape(generic=FIXED_GENERIC))
        cls.absent = run(shape())  # no generic.caller: the fixed route
        cls.anyr = run(shape(generic=ANY_GENERIC, any_caller=ANY_PROOF))

    def test_inputs_accepted(self):
        for rc, log, _ in (self.fixed, self.absent, self.anyr):
            self.assertEqual(rc, 0, log)

    def test_fixed_route_claims_one_caller(self):
        _, _, out = self.fixed
        for k in ("statement", "final", "record"):
            self.assertNotIn("ctx.Ok", out[k])
            self.assertNotIn("AnyCaller", out[k])
            self.assertNotIn("any_caller", out[k])
            self.assertNotIn("semAt", out[k])
        self.assertIn(
            "Contexts ctx := ctx = _root_.SolidCore.Verify.Generic.Context.default",
            out["statement"],
        )

    def test_fixed_route_is_the_base_output(self):
        # The pinned fixed-route output: every caller-free shape emits these bytes.
        _, _, out = self.fixed
        for k, f in (("statement", "Statement.lean"), ("final", "Final.lean")):
            self.assertEqual(
                out[k], (HERE / "testdata" / f"final_caller_fixed.{f}").read_text()
            )

    def test_fixed_route_is_the_absent_route(self):
        # generic.caller 0x2000 and no caller at all emit the same files, byte for byte.
        self.assertEqual(self.fixed[2]["statement"], self.absent[2]["statement"])
        self.assertEqual(self.fixed[2]["final"], self.absent[2]["final"])

    def test_any_route_adds_the_claim(self):
        _, _, out = self.anyr
        st, fx = out["statement"], self.fixed[2]["statement"]
        self.assertIn(FORALL, st)
        self.assertIn(f"∃ c, _root_.{NS}.src.checked = .ok c ∧", st)
        self.assertIn("∀ gas, (fun _ => 2000) call ≤ gas →", st)
        self.assertIn(
            f"_root_.SolidCore.Verify.Generic.runAt ctx gas (_root_.SolidCore.Verify.Generic.runtimeCode _root_.{NS}.runtimeBytes) call =",
            st,
        )
        self.assertIn(f"(_root_.{NS}.src.semAt ctx).run c call", st)
        # The claim is added to the fixed statement, never in place of it.
        self.assertIn(
            f"_root_.SolidCore.Verify.Generic.Expected _root_.{NS}.src _root_.{NS}.spec _root_.{NS}.runtimeBytes ∧\n    {FORALL}",
            st,
        )
        head = fx.split("/-- **The published statement.**")[0]
        self.assertTrue(st.startswith(head))
        self.assertIn("import c_1.AnyCaller\n", out["final"])
        self.assertIn("(Certified.c_1.AnyCaller.published_any) ctx hctx", out["final"])
        self.assertNotIn(
            "AnyCaller", st
        )  # the statement module never imports the proof
        self.assertIn(FORALL, json.loads(out["record"])["statement"])

    def test_any_route_keeps_the_fixed_proof(self):
        fx, an = self.fixed[2]["final"], self.anyr[2]["final"]
        tail = fx.split(":= by\n", 1)[1].split("\n", 1)[1]
        self.assertIn(tail, an)


class FailsClosed(unittest.TestCase):
    def refused(self, s, why):
        rc, log, out = run(s)
        self.assertNotEqual(rc, 0, log)
        self.assertIn(why, log)
        self.assertIsNone(out["statement"])
        self.assertIsNone(out["final"])

    def test_fixed_route_with_any_proof(self):
        # A fixed-route shape that carries an any-caller proof would publish a claim it did not check.
        self.refused(
            shape(generic=FIXED_GENERIC, any_caller=ANY_PROOF),
            "the fixed route claims one caller",
        )
        self.refused(shape(any_caller=ANY_PROOF), "the fixed route claims one caller")

    def test_any_without_check_sym(self):
        g = {
            k: v
            for k, v in ANY_GENERIC.items()
            if k not in ("check_sym", "check_value_sym")
        }
        self.refused(
            shape(generic=g, any_caller=ANY_PROOF), "needs the check for every caller"
        )

    def test_any_with_wrong_check_name(self):
        g = dict(ANY_GENERIC, check_sym="check_stage0")
        self.refused(
            shape(generic=g, any_caller=ANY_PROOF), "needs the check for every caller"
        )

    def test_any_without_proof(self):
        self.refused(
            shape(generic=ANY_GENERIC), "needs any_caller {module, proof, gas}"
        )

    def test_any_without_gas(self):
        p = {k: v for k, v in ANY_PROOF.items() if k != "gas"}
        self.refused(
            shape(generic=ANY_GENERIC, any_caller=p),
            "needs any_caller {module, proof, gas}",
        )

    def test_unknown_caller(self):
        self.refused(
            shape(generic=dict(BASE["generic"], caller="0xbeef")),
            "generic.caller must be 0x2000 or any",
        )

    def test_bad_module_name(self):
        p = dict(ANY_PROOF, module="c_1.AnyCaller\nimport Evil")
        self.refused(
            shape(generic=ANY_GENERIC, any_caller=p),
            "any_caller.module must be a module name",
        )


if __name__ == "__main__":
    unittest.main()
