#!/usr/bin/env python3
"""final.py's fresh-byte binding of a pinned artifact (pinned_bytes, pinned_module) and its iteration
bound (iteration_bound): the statement binds the byte list the proof was built on to the bytes this
run emitted (runtime_is_fresh, decided by the kernel) and states the shape of those bytes; a run whose
bytes differ changes only the literal the kernel compares, and every inconsistent shape fails closed.
Runs final.py as the driver does, with no Lean (the kernel's refusal of a mutated byte is a Lean
check of the generated modules).
Run: python3 tools/certify/test_final_pinned.py"""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
FINAL = HERE / "final.py"
NS = "Certified.loop_1.Final"
HEX = "34603b575f3560e01c"

# A loop contract whose proof is stated over a pinned stage export's bytes, for every iteration
# bound K below a literal maximum.
BASE = {
    "kind": "certified",
    "contract": "M.Source.contract?",
    "fuel": "M.Source.fuel",
    "source_sem": "M.Sem.runtime",
    "iteration_bound": {"var": "K", "max": 1000},
    "valid": "M.Calls.bounded K",
    "allowed": "fun _ _ => True",
    "post": "M.Spec.post",
    "vacuity": "every status is allowed",
    "gas": "fun _ => K * 7 + 3",
    "env": [],
    "spec": "a Lean specification",
    "spec_modules": ["M.Source", "M.Spec", "M.Calls"],
    "pinned_bytes": "M.Export.C.bytecode",
    "pinned_module": "M.Export.C.Bytecode",
    "proof_module": "M.Join",
    "proof": "M.Join.certified",
}


def run(shape, hexs=HEX):
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
                "loop_1",
                "--contract",
                "C",
                "--hex",
                hexs,
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
    for k, v in kw.items():
        if v is None:
            s.pop(k, None)
        else:
            s[k] = v
    return s


class Pinned(unittest.TestCase):
    def test_binds_the_pinned_bytes_to_the_run(self):
        rc, err, out = run(shape())
        self.assertEqual(rc, 0, err)
        st, fin = out["statement"], out["final"]
        # The statement imports the pinned data module, not the run's export or the proof module.
        self.assertIn("import M.Export.C.Bytecode\n", st)
        self.assertNotIn("loop_1.gen.C.Bytecode", st)
        self.assertNotIn("import M.Join", st)
        # code is the run's bytes themselves.
        self.assertIn(
            f"def code : ByteArray := ⟨(_root_.{NS}.runtimeBytes.map UInt8.ofNat).toArray⟩",
            st,
        )
        self.assertIn(
            "def runtimeBytes : List Nat :=\n  [0x34, 0x60, 0x3b, 0x57, 0x5f, 0x35, 0x60, 0xe0, 0x1c]",
            st,
        )
        # The first conjunct is the fresh-byte binding, then the claim of code for every K ≤ 1000.
        self.assertIn(
            f"def Statement : Prop :=\n  (M.Export.C.bytecode) = _root_.{NS}.runtimeBytes ∧\n"
            "    (∀ (K : Nat), K ≤ 1000 →\n      (∃ c, M.Source.contract? = .ok c ∧\n"
            f"      _root_.SolidCore.Refinement.Shapes.Certified (_root_.M.Sem.runtime (M.Source.fuel)) c _root_.{NS}.code\n"
            "        (M.Calls.bounded K) (fun _ _ => True) (M.Spec.post) (fun _ => K * 7 + 3)))",
            st,
        )
        self.assertIn(
            f"theorem runtime_is_fresh : (M.Export.C.bytecode) = _root_.{NS}.runtimeBytes :=\n"
            "  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)",
            st,
        )
        self.assertIn(
            f"theorem code_eq : (⟨((M.Export.C.bytecode).map UInt8.ofNat).toArray⟩ : ByteArray) = _root_.{NS}.code",
            st,
        )
        self.assertIn(
            f"  refine ⟨_root_.{NS}.runtime_is_fresh, ?_⟩\n  rw [← _root_.{NS}.code_eq]\n"
            "  exact fun K hK => (M.Join.certified) hK\n",
            fin,
        )
        rec = json.loads(out["record"])
        self.assertEqual(rec["fresh_binding"]["theorem"], f"{NS}.runtime_is_fresh")
        self.assertEqual(rec["iteration_bound"], {"var": "K", "max": 1000})
        self.assertFalse(rec["strongest"])
        self.assertTrue(
            rec["statement"].startswith(
                f"(M.Export.C.bytecode) = _root_.{NS}.runtimeBytes ∧"
            )
        )
        self.assertNotIn("runtime_is_fresh", rec["statement"])

    def test_a_mutated_byte_changes_only_the_decided_literal(self):
        flip = HEX[:8] + format(int(HEX[8:10], 16) ^ 1, "02x") + HEX[10:]
        _, _, good = run(shape())
        rc, err, bad = run(shape(), flip)
        self.assertEqual(rc, 0, err)
        a, b = good["statement"].splitlines(), bad["statement"].splitlines()
        diff = [(x, y) for x, y in zip(a, b, strict=True) if x != y]
        # The hex for the reader and the literal runtime_is_fresh compares, nothing else: the kernel
        # refuses the mutated run's binding (decide finds the equality false).
        self.assertEqual(len(diff), 2, diff)
        self.assertIn("0x5e", diff[1][1])
        self.assertEqual(good["final"], bad["final"])

    def test_env_premises_pass_through(self):
        rc, err, out = run(shape(env=["M.Env.ok"]))
        self.assertEqual(rc, 0, err)
        self.assertIn(
            "(∀ (K : Nat), K ≤ 1000 →\n      ((M.Env.ok) →\n    ∃ c,", out["statement"]
        )
        self.assertIn("exact fun K hK _h0 => (M.Join.certified) hK _h0\n", out["final"])

    def test_iteration_bound_on_the_fresh_export(self):
        rc, err, out = run(shape(pinned_bytes=None, pinned_module=None))
        self.assertEqual(rc, 0, err)
        st = out["statement"]
        self.assertIn("import loop_1.gen.C.Bytecode\n", st)
        self.assertIn(
            "def code : ByteArray := ⟨(_root_.SolarTV.Export.C.bytecode.map UInt8.ofNat).toArray⟩",
            st,
        )
        self.assertIn(
            f"theorem bound : _root_.{NS}.code.data.toList.map (·.toNat) = _root_.{NS}.runtimeBytes",
            st,
        )
        self.assertNotIn("runtime_is_fresh", st)
        self.assertIn(
            f"  refine ⟨_root_.{NS}.bound, ?_⟩\n  exact fun K hK => (M.Join.certified) hK\n",
            out["final"],
        )

    def test_pinned_without_a_bound(self):
        rc, err, out = run(
            shape(iteration_bound=None, valid="M.Calls.all", gas="fun _ => 9")
        )
        self.assertEqual(rc, 0, err)
        self.assertNotIn("∀ (K : Nat)", out["statement"])
        self.assertIn("  exact (M.Join.certified)\n", out["final"])

    def test_plain_shapes_are_unchanged(self):
        rc, err, out = run(
            shape(
                iteration_bound=None,
                pinned_bytes=None,
                pinned_module=None,
                valid="M.Calls.all",
                gas="fun _ => 9",
            )
        )
        self.assertEqual(rc, 0, err)
        self.assertNotIn("runtime_is_fresh", out["statement"])
        self.assertIn(
            f"theorem {NS}.final : _root_.{NS}.Statement :=\n  ⟨", out["final"]
        )
        self.assertNotIn("fresh_binding", json.loads(out["record"]))

    def test_bound_with_nonvacuous(self):
        # Non-vacuity at every bound: NonVacuous takes K, inside the bound's binder.
        rc, err, out = run(
            shape(
                vacuity=None, allowed="M.Spec.allowed", nonvacuous="M.Spec.nonvacuous"
            )
        )
        self.assertEqual(rc, 0, err)
        st = out["statement"]
        self.assertIn("def NonVacuous (K : Nat) : Prop :=\n  (∃ (call", st)
        self.assertIn("¬ (M.Spec.allowed) call obs)", st)
        self.assertIn(
            f"    (∀ (K : Nat), K ≤ 1000 →\n      (_root_.{NS}.NonVacuous K ∧ (∃ c, M.Source.contract? = .ok c ∧",
            st,
        )
        self.assertIn(
            "  exact fun K hK => ⟨(M.Spec.nonvacuous) hK, (M.Join.certified) hK⟩\n",
            out["final"],
        )
        self.assertNotIn("refine ⟨M.Spec.nonvacuous", out["final"])
        rec = json.loads(out["record"])
        self.assertEqual(rec["nonvacuous"], f"{NS}.NonVacuous")
        self.assertIsNone(rec["vacuity"])

    def test_allowed_vacuity_narrows_nonvacuous(self):
        rc, err, out = run(
            shape(
                vacuity=None,
                allowed="M.Spec.allowed",
                nonvacuous="M.Spec.nonvacuous",
                allowed_vacuity="no succeeds-if",
            )
        )
        self.assertEqual(rc, 0, err)
        st = out["statement"]
        self.assertIn(
            "def NonVacuous (K : Nat) : Prop :=\n  ∃ (call : _root_.SolarTV.Call) (obs : _root_.SolarTV.Observable), "
            "(M.Calls.bounded K) call ∧ ¬ (M.Spec.post) call obs\n",
            st,
        )
        self.assertNotIn("obs.success = false", st)
        rec = json.loads(out["record"])
        self.assertEqual(rec["allowed_vacuity"], "no succeeds-if")
        self.assertFalse(rec["strongest"])

    def test_refusals(self):
        cases = {
            "no pinned_module": shape(pinned_module=None),
            "pinned_module without pinned_bytes": shape(pinned_bytes=None),
            "pinned bytes from the proof module": shape(pinned_module="M.Join"),
            "pinned bytes with a tactic": shape(pinned_bytes="by decide"),
            "open_functional": shape(
                kind="open_functional", env_ok="E", wide="W", admit="A"
            ),
            "expected statement": shape(statement="expected"),
            "bound with a witness but a trivial allowed": shape(
                vacuity=None, nonvacuous="M.Spec.nonvacuous"
            ),
            "allowed_vacuity without a witness": shape(
                allowed="M.Spec.allowed", allowed_vacuity="no succeeds-if"
            ),
            "allowed_vacuity that says nothing": shape(
                vacuity=None,
                allowed="M.Spec.allowed",
                nonvacuous="M.Spec.nonvacuous",
                allowed_vacuity=" ",
            ),
            "lowercase bound variable": shape(
                iteration_bound={"var": "k", "max": 1000}
            ),
            "bound variable that is a module": shape(
                iteration_bound={"var": "M", "max": 1000}
            ),
            "bound maximum not a literal": shape(
                iteration_bound={"var": "K", "max": "M.Kmax"}
            ),
            "bound maximum negative": shape(iteration_bound={"var": "K", "max": -1}),
            "bound with extra keys": shape(
                iteration_bound={"var": "K", "max": 1, "min": 0}
            ),
        }
        for label, s in cases.items():
            with self.subTest(label):
                rc, err, out = run(s)
                self.assertNotEqual(rc, 0, label)
                self.assertIn("final.py:", err)
                self.assertIsNone(out["final"], label)


if __name__ == "__main__":
    unittest.main()
