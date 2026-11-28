#!/usr/bin/env python3
"""The driver's caller route "any", end to end without Lean: generic.py's route decision
(decide_route: caller_route on the probe's check for every caller, ctx_route on the checker with
contexts), the Lowering and shape.json it writes, shape_gen.py's NAME.CtxGas and NAME.AnyCaller,
and final.py's statement. testdata/any_caller_emit/ is a generic-stage run's export
(SimpleStorage: gen/, Proofs/Stages.lean, SpecConnect.lean, its core literal) with its fixed-route
Lowering.lean, shape.json and Shape.lean, which every caller-free change must keep byte for byte.
Run under the Python generic.py needs (3.14): python3.14 tools/certify/test_any_caller_emit.py"""

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import gen_ctxgas
import generic

D = HERE / "testdata" / "any_caller_emit"
NAME, CONTRACT = "simplestorage_928cc159", "SimpleStorage"
CORE = (D / "core.txt").read_text().rstrip("\n")
FIXED_SHAPE = json.loads((D / "fixed.shape.json").read_text())
FNS = [
    {"sig": e["signature"], "sel": e["sel"], "params": [".uint 256"] * e["arity"]}
    for e in FIXED_SHAPE["entries"]
]
GAS = int(FIXED_SHAPE["gas"].split()[-1])
BOUND = (
    2121603  # the checker with contexts' bound on this export (gasBoundL at 4 blocks)
)
SENDER = "SolidCore.Solidity.Source.Expr.caller"
SENDER_CORE = CORE.replace("SolidCore.Solidity.Source.Expr.var", SENDER, 1)
DIFFER = "trees differ: source (Tree.halt true [OByte.byteOf (WExpr.caller) 0]) MIR (Tree.halt true [OByte.byteOf (WExpr.lit 8192) 0])"


def probe(core=CORE, verdict="agree", only=None):
    res = {"core": core, "storage": "[]"}
    for f in FNS:
        res[f"fn {f['sig']}"] = "agree"
        res[f"sym fn {f['sig']}"] = verdict if only in (None, f["sig"]) else "agree"
    return res


def export_dir():
    t = pathlib.Path(tempfile.mkdtemp())
    for p in ("gen", "Proofs"):
        shutil.copytree(D / p, t / p)
    return t


def route(res, witness="set", reasons=()):
    d = export_dir()
    try:
        return generic.decide_route(res, FNS, list(reasons), witness, d, CONTRACT)
    finally:
        shutil.rmtree(d)


def rename(text, name):
    """TEXT about contract module NAME instead (its snake and its milestone spellings)."""
    camel = "".join(w.capitalize() for w in name.split("_"))
    return text.replace(NAME, name).replace("Simplestorage928cc159", camel)


def emit(shape, name=NAME):
    """shape_gen.py on the export with SHAPE as contract module NAME: (exit, log, {file: text})."""
    t = pathlib.Path(tempfile.mkdtemp())
    try:
        root = t / name
        root.mkdir()
        for p in ("gen", "Proofs"):
            shutil.copytree(D / p, root / p)
        # The same export under module NAME is under NAME's namespace (export_ns.py).
        for f in (root / "gen").rglob("*"):
            if f.suffix in (".lean", ".json"):
                f.write_text(f.read_text().replace(NAME, name))
        st = root / "Proofs" / "Stages.lean"
        st.write_text(rename(st.read_text(), name))
        sc = (D / "SpecConnect.lean").read_text().replace(NAME, name)
        (root / "SpecConnect.lean").write_text(sc)
        (root / "shape.json").write_text(
            json.dumps(shape, indent=2).replace(NAME, name) + "\n"
        )
        p = subprocess.run(
            [
                sys.executable,
                "-I",
                str(HERE / "shape_gen.py"),
                str(root / "shape.json"),
                "--name",
                name,
                "--certified",
                str(t),
                "--out",
                str(root / "Shape.lean"),
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        out = {
            f: (root / f).read_text()
            for f in ("Shape.lean", "CtxGas.lean", "AnyCaller.lean")
            if (root / f).exists()
        }
        return p.returncode, p.stdout + p.stderr, out
    finally:
        shutil.rmtree(t)


def any_shape():
    return generic.shape_json(
        NAME, CONTRACT, FNS, GAS, FIXED_SHAPE["spec"], "set", [], BOUND
    )


class FixedRoute(unittest.TestCase):
    def test_lowering_identical(self):
        self.assertEqual(
            generic.lowering_lean(NAME, CONTRACT, FNS, [], CORE, "[]"),
            (D / "fixed.Lowering.lean").read_text(),
        )

    def test_shape_json_identical(self):
        s = generic.shape_json(NAME, CONTRACT, FNS, GAS, FIXED_SHAPE["spec"], "set", [])
        self.assertEqual(
            json.dumps(s, indent=2) + "\n", (D / "fixed.shape.json").read_text()
        )

    def test_shape_lean_identical_and_no_any_modules(self):
        rc, log, out = emit(FIXED_SHAPE)
        self.assertEqual(rc, 0, log)
        self.assertEqual(out["Shape.lean"], (D / "fixed.Shape.lean").read_text())
        self.assertEqual(set(out), {"Shape.lean"})


class AnyRoute(unittest.TestCase):
    def test_decided_from_the_probe_and_the_export(self):
        caller, gas, reasons, _ = route(probe())
        self.assertEqual((caller, gas, reasons), ("any", BOUND, []))

    def test_bound_is_the_checkers(self):
        _, info = gen_ctxgas.generate(D / "gen" / CONTRACT, "p", CONTRACT, "evm6")
        self.assertEqual(gen_ctxgas.gas_bound(info, 4), BOUND)
        self.assertGreater(BOUND, GAS)  # the loop checker's bound would not do

    def test_lowering_states_the_checks(self):
        low = generic.lowering_lean(NAME, CONTRACT, FNS, [], CORE, "[]", sym=True)
        self.assertIn("import SolidCore.Refinement.Generic.StageFrom\n", low)
        self.assertIn(
            "theorem check_sym_stage0 :\n    Generic.checkSym fuel mirFuel coreLit SolarTV.Export.simplestorage_928cc159.SimpleStorage.stage0 fns = true",
            low,
        )
        self.assertIn(
            "theorem checkValueSym_stage0 :\n    Generic.checkValue mirFuel coreLit SolarTV.Export.simplestorage_928cc159.SimpleStorage.stage0 fns .allSym = true",
            low,
        )

    def test_probe_asks_the_check_for_every_caller(self):
        text = generic.probe_lean(NAME, CONTRACT, FNS)
        for f in FNS:
            self.assertIn(f"PROBE sym fn {f['sig']}: ", text)
        self.assertIn(".allSym with", text)

    def test_emitter_writes_the_modules(self):
        rc, log, out = emit(any_shape())
        self.assertEqual(rc, 0, log)
        self.assertEqual(set(out), {"Shape.lean", "CtxGas.lean", "AnyCaller.lean"})
        ctx, _ = gen_ctxgas.generate(
            D / "gen" / CONTRACT, f"{NAME}.gen.{CONTRACT}", CONTRACT, "evm6"
        )
        self.assertEqual(out["CtxGas.lean"], ctx)
        a = out["AnyCaller.lean"]
        for want in (
            f"namespace Certified.{NAME}.AnyCaller",
            "theorem published_any : ∀ ctx : SolidCore.Verify.Generic.Context, ctx.Ok →",
            f"theorem gas_le : AssembleLoop.gasBoundL SolarTV.Export.{NAME}.{CONTRACT}.CtxGas.cx.base 4 ≤ {BOUND}",
            f"rw [hk]; exact Certified.{NAME}.Lowering.check_sym_stage0",
            f"rw [hk]; exact Certified.{NAME}.Lowering.checkValueSym_stage0",
            "SolidCore.Verify.AnyCaller.bytes_eq_source_from",
            "depends on axioms: [propext, Classical.choice, Quot.sound]",
        ):
            self.assertIn(want, a)
        for bad in (
            "sorry",
            "native_decide",
            "axiom ",
            "@[extern",
            "implemented_by",
            "SimpleStorage.get",
        ):
            self.assertNotIn(bad, a)

    def test_same_text_for_every_contract(self):
        # No dispatch on the contract: AnyCaller.lean comes from the shape and the stage names only,
        # so the same export under another module name gives the same text, renamed.
        _, _, a = emit(any_shape())
        rc, log, b = emit(any_shape(), "other_7")
        self.assertEqual(rc, 0, log)
        self.assertEqual(rename(a["AnyCaller.lean"], "other_7"), b["AnyCaller.lean"])
        for sel in ("6d4ce63c", "4e70b1dc", "60fe47b1", "SimpleStorage.sol"):
            self.assertNotIn(sel, a["AnyCaller.lean"])

    def test_final_statement(self):
        with tempfile.TemporaryDirectory() as t:
            t = pathlib.Path(t)
            (t / "shape.json").write_text(json.dumps(any_shape()))
            p = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(HERE / "final.py"),
                    str(t / "shape.json"),
                    "--name",
                    NAME,
                    "--contract",
                    CONTRACT,
                    "--hex",
                    "6080604052",
                    "--out",
                    str(t / "Final.lean"),
                    "--statement-out",
                    str(t / "Statement.lean"),
                    "--record",
                    str(t / "r.json"),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(p.returncode, 0, p.stderr)
            st = (t / "Statement.lean").read_text()
            self.assertIn(f"∀ gas, (fun _ => {BOUND}) call ≤ gas →", st)
            self.assertIn(f"import {NAME}.AnyCaller\n", (t / "Final.lean").read_text())


class FailsClosed(unittest.TestCase):
    def test_checksym_failure_on_a_sender_contract_is_blocked(self):
        caller, gas, reasons, _ = route(probe(SENDER_CORE, DIFFER, only="get()"))
        self.assertEqual((caller, gas), ("0x2000", None))
        self.assertEqual(
            [(r["feature"], r["where"]) for r in reasons], [("msg.sender", "get()")]
        )

    def test_checksym_failure_without_sender_stays_fixed(self):
        caller, gas, reasons, _ = route(probe(CORE, DIFFER, only="get()"))
        self.assertEqual((caller, gas, reasons), ("0x2000", None, []))
        self.assertEqual(
            generic.shape_json(
                NAME, CONTRACT, FNS, GAS, FIXED_SHAPE["spec"], "set", [], gas
            ),
            FIXED_SHAPE,
        )

    def test_caller_read_that_is_not_msg_sender(self):
        # The module reads ORIGIN (or ADDRESS) where the source reads msg.sender.
        diff = "trees differ: source (Tree.halt true [OByte.byteOf (WExpr.caller) 0]) MIR (Tree.halt true [OByte.byteOf (WExpr.lit 4096) 0])"
        _, _, reasons, _ = route(probe(SENDER_CORE, diff))
        self.assertEqual({r["feature"] for r in reasons}, {"msg.sender"})
        self.assertEqual(len(reasons), len(FNS))

    def test_environment_read_the_context_checker_does_not_bind(self):
        real = gen_ctxgas.generate

        def reads(*a, **k):
            t, info = real(*a, **k)
            return t, dict(info, reads=True)

        with mock.patch.object(gen_ctxgas, "generate", reads):
            caller, gas, reasons, note = route(probe())
        self.assertEqual((caller, gas, reasons), ("0x2000", None, []))
        self.assertIn("environment words", note)

    def test_context_checker_refuses_the_export(self):
        def refuse(*a, **k):
            raise gen_ctxgas.CtxGasError("error: block 3 is outside the fragment")

        with mock.patch.object(gen_ctxgas, "generate", refuse):
            caller, gas, _, note = route(probe())
        self.assertEqual((caller, gas), ("0x2000", None))
        self.assertIn("block 3 is outside the fragment", note)

    def test_no_spec_witness(self):
        caller, gas, _, _ = route(probe(), witness=None)
        self.assertEqual((caller, gas), ("0x2000", None))

    def test_probe_refused(self):
        self.assertEqual(
            route(probe(), reasons=[{"feature": "x"}])[:3], ("0x2000", None, [])
        )

    def emit_refused(self, shape, why):
        rc, log, out = emit(shape)
        self.assertNotEqual(rc, 0, log)
        self.assertIn(why, log)
        self.assertEqual(out, {})

    def test_missing_gas_bound(self):
        s = any_shape()
        del s["any_caller"]["gas"]
        self.emit_refused(s, "needs any_caller {module, proof, gas}")

    def test_gas_bound_below_the_checker(self):
        s = any_shape()
        s["any_caller"]["gas"] = f"fun _ => {GAS}"
        self.emit_refused(s, f"is below the checker's {BOUND}")

    def test_gas_bound_not_constant(self):
        s = any_shape()
        s["any_caller"]["gas"] = "fun c => c.callvalue"
        self.emit_refused(s, "must be a constant bound")

    def test_missing_check_sym(self):
        s = any_shape()
        del s["generic"]["check_sym"]
        self.emit_refused(s, "needs check_sym and check_value_sym")

    def test_fixed_route_with_any_proof(self):
        s = json.loads(json.dumps(FIXED_SHAPE))
        s["any_caller"] = any_shape()["any_caller"]
        self.emit_refused(s, "the fixed route claims one caller")

    def test_foreign_proof_module(self):
        s = any_shape()
        s["any_caller"]["module"] = "other_1.AnyCaller"
        self.emit_refused(s, "must name")

    def test_any_needs_the_expected_statement(self):
        with self.assertRaises(generic.Failed):
            generic.shape_json(NAME, CONTRACT, FNS, GAS, "none", None, [], BOUND)


if __name__ == "__main__":
    unittest.main()
