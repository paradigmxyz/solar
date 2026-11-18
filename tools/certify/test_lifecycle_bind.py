"""The lifecycle certificate bound to an expected theorem (generic.py --lifecycle-bind): the
binding's checks, and the golden text of the bound Lifecycle module, SourceSame and Composed. No
Lean; testdata/lifecycle_bind/counter-bind.json names solidity-lean's Counter expected theorem
(Verify/LoopSource/Equiv/CounterCtx.lean counter_expected, n = 8).

  python3 tools/certify/test_lifecycle_bind.py
"""

import copy
import json
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

BIND = json.loads(
    (HERE / "testdata" / "lifecycle_bind" / "counter-bind.json").read_text()
)
NAME = "counter_f1c87a33"
SIGS = [{"sig": "bump(uint256)", "sel": 0xB20EB4C4}]


def binding(b, ws=(8,)):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(b, f)
    return generic.lifecycle_binding(f.name, list(ws))


def life(bind):
    return generic.lifecycle_lean(
        f"Certified.{NAME}.Lifecycle",
        f"{NAME}.Source",
        f"Certified.{NAME}.Src.importedSourceUnit",
        "Counter",
        SIGS,
        bytes.fromhex("6080"),
        bytes.fromhex("6001"),
        bytes.fromhex("6001"),
        [8],
        [(1, 8)],
        bind=bind,
    )


class Binding(unittest.TestCase):
    def test_counter_binding_reads(self):
        b = binding(BIND)
        self.assertEqual(b["ws"], [8])
        self.assertEqual(generic.bind_modules(b)[0], "Contracts.VerifyCounter.Source")

    def test_refused(self):
        cases = {}
        for k, v in [
            ("fuel", "2 ^ 256 + 8"),
            ("contract", "x y"),
            ("init", "a\nb"),
            ("unit", ""),
        ]:
            b = copy.deepcopy(BIND)
            b[k] = v
            cases[f"{k} not a name"] = (b, (8,))
        b = copy.deepcopy(BIND)
        del b["runtime"]
        cases["missing key"] = (b, (8,))
        b = copy.deepcopy(BIND)
        b["extra"] = 1
        cases["extra key"] = (b, (8,))
        cases["other words"] = (BIND, (5,))
        for k, v in [
            ("term", "by sorry"),
            ("term", "by native_decide"),
            ("binders", "(h : False)\n"),
            ("prop", "True"),
            ("prop", "SolidCore.Verify.Generic.Expected a b other"),
            ("term", "#exit"),
            ("module", "A B"),
        ]:
            b = copy.deepcopy(BIND)
            b["expected"][k] = v
            cases[f"expected.{k} {v!r}"] = (b, (8,))
        b = copy.deepcopy(BIND)
        b["modules"] = []
        cases["no modules"] = (b, (8,))
        for label, (b, ws) in cases.items():
            with self.subTest(label), self.assertRaises(generic.Failed):
                binding(b, ws)


class Text(unittest.TestCase):
    def test_unbound_module_is_unchanged(self):
        t = life(None)
        self.assertIn("def fuel : Nat := 4096\n", t)
        self.assertIn(
            f'TypeCheck.CheckedInput.contract Certified.{NAME}.Src.importedSourceUnit "Counter"',
            t,
        )
        self.assertNotIn("fuel_same", t)

    def test_bound_module_uses_the_theorems_constants(self):
        t = life(binding(BIND))
        self.assertIn(
            "import Contracts.VerifyCounter.Source\nimport Verify.LoopSource.Equiv.CounterCtor\n"
            "import Verify.LoopSource.Equiv.Counter.gen.Counter.Bytecode\nimport Verify.Generic.Expected\n",
            t,
        )
        self.assertNotIn("import Verify.LoopSource.Equiv.CounterCtx", t)
        self.assertIn("def fuel : Nat := Contracts.VerifyCounter.Source.fuel\n", t)
        self.assertIn("def frameFuel : Nat := 4096\n", t)
        self.assertIn("  Contracts.VerifyCounter.Source.contract?\n", t)
        for line in (
            "theorem fuel_same : fuel = Contracts.VerifyCounter.Source.fuel := rfl\n",
            "theorem contract_same : contract? = Contracts.VerifyCounter.Source.contract? := rfl\n",
            "theorem init_same : init = Verify.LoopSource.Equiv.CounterCtor.counterInit := by\n  apply ByteArray.ext; decide +kernel\n",
            "theorem code_same : code = SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode := by\n  apply ByteArray.ext; decide +kernel\n",
            "theorem ws_same : ws = [8] := rfl\n",
        ):
            self.assertIn(line, t)
        self.assertLess(
            t.index("#print axioms statement"), t.index("theorem fuel_same")
        )
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, t.replace("#print axioms", ""))

    def test_source_same(self):
        t = generic.source_same_lean(NAME, binding(BIND))
        self.assertIn(
            f"theorem unit_same : Certified.{NAME}.Src.importedSourceUnit = "
            "SolidCore.Refinement.Source.VerifyCounter.importedSourceUnit := rfl\n",
            t,
        )

    def test_composed(self):
        t = generic.composed_lean(NAME, binding(BIND))
        L = f"Certified.{NAME}.Lifecycle"
        self.assertIn(
            f"import {NAME}.Lifecycle\nimport {NAME}.SourceSame\nimport Verify.LoopSource.Equiv.CounterCtx\n",
            t,
        )
        self.assertIn(
            "theorem composed {K : Nat} (hK : K ≤ Verify.LoopSource.Equiv.CounterCtx.Kmax) :\n"
            "    SolidCore.Verify.Generic.Expected (Verify.LoopSource.Equiv.CounterCtx.src 8) "
            "(Verify.LoopSource.Equiv.CounterCtx.spec 8 K) SolarTV.Export.Counter.bytecode ∧\n",
            t,
        )
        self.assertIn(
            "Verify.Lifecycle.Statement Contracts.VerifyCounter.Source.fuel c "
            "Verify.LoopSource.Equiv.CounterCtor.counterInit "
            f"(SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode) [8] {L}.storageLit ∧\n",
            t,
        )
        self.assertIn(
            "  have hE := Verify.LoopSource.Equiv.CounterCtx.counter_expected (n := 8) (Or.inr rfl) hK\n",
            t,
        )
        self.assertIn(
            f"  rw [{L}.fuel_same, {L}.init_same, {L}.code_same, {L}.ws_same] at hl\n",
            t,
        )
        self.assertIn("  exact ⟨hE.1, c, hc, hl, hE.2 c hc _ hl⟩\n", t)
        self.assertIn("depends on axioms: [propext, Classical.choice, Quot.sound]", t)


class Wiring(unittest.TestCase):
    def test_driver_checks_source_then_composes(self):
        src = (HERE / "generic.py").read_text()
        i = src.index("def lifecycle_run(")
        body = src[i : src.index("\ndef admit_lake_jobs", i)]
        self.assertLess(
            body.index('"SourceSame"'), body.index("kq = lifecycle_kernel(")
        )
        self.assertLess(body.index("kq = lifecycle_kernel("), body.index('"Composed"'))
        self.assertIn("this run's source unit is not the expected theorem's", body)
        self.assertIn("the composed theorem does not build", body)
        self.assertEqual(body.count("build_failure("), 2)

    def test_build_failure_keeps_the_whole_log(self):
        out = "\n".join(
            [f"✔ [{i}/9] Built M{i}" for i in range(80)]
            + [
                "✖ [9/9] Building Contracts.VerifyCounter.Source",
                "INTERNAL PANIC: lean::memory_exception: excessive memory consumption detected at 'interpreter'",
                "error: Lean exited with code 134",
            ]
        )
        with tempfile.TemporaryDirectory() as d:
            log_name, tail = generic.build_failure(
                pathlib.Path(d), NAME, "SourceSame", out
            )
            self.assertEqual(log_name, f"{NAME}.SourceSame.build.log")
            self.assertEqual((pathlib.Path(d) / log_name).read_text(), out)
        self.assertEqual(len(tail.splitlines()), 60)
        self.assertIn("lean::memory_exception", tail)
        self.assertTrue(tail.endswith("error: Lean exited with code 134"))


if __name__ == "__main__":
    unittest.main()
