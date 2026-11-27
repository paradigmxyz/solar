"""Unit tests of tools/certify/loop_final.py's pure parts: refusals, the parsed resources, the generated
join and shape.json. The Lean side is checked by running the loop route (see loop_final.py)."""

import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import loop_final

RES = """namespace Certified.n.LoopResources
def Kmax : Nat := 13042098468400

def Valid (K : Nat) (call : Call) : Prop :=
  ∃ σ a0, a0 < 2 ^ 256 ∧ a0 ≤ K ∧ call = { calldata := Abi.calldata 2987307204 [a0], callvalue := 0, storage := σ }

theorem loops (o : Oracle) (K : Nat) (call : Call) (hv : Valid K call) :
    Loops (EVM.Program.block o call SolarTV.Export.C.evm7) (0, [], State.initial call) record 2 2 3 K) := by
"""

CONST = (
    RES.replace("def Valid (K : Nat)", "def Valid")
    .replace(" ∧ a0 ≤ K", "")
    .replace("record 2 2 3 K)", "record 2 2 3 M)")
    + "abbrev M : Nat := 10\n"
)

STAGES = """namespace SolarTV.Milestone.X
theorem n_mir_to_evmir (o : Oracle) : Stage (mirSem o) (evmirSem o) stage0 evm7 anyCall :=
"""


def facts(text=RES):
    f = loop_final.parse_resources(text)
    f.update(final_evm_ir="evm7", per_iteration_gas=1414400, exit_gas=3536012)
    return f


class Refusals(unittest.TestCase):
    def refused(self, step, fn, *a):
        with self.assertRaises(loop_final.Refused) as c:
            fn(*a)
        self.assertEqual(c.exception.step, step)
        return c.exception.why

    def test_route(self):
        rec = {"loop_cuts": {"bump": {"route": "summary"}}, "loop_cuts_blocked": {}}
        self.assertIn("summary", self.refused("route", loop_final.loop_function, rec))

    def test_blocked(self):
        rec = {
            "loop_cuts": {},
            "loop_cuts_blocked": {"f": {"step": "shape", "why": "a while loop"}},
        }
        self.assertIn(
            "a while loop", self.refused("route", loop_final.loop_function, rec)
        )

    def test_functions(self):
        # One loop function for the single statement; several take loop_functions and parts.
        rec = {"loop_cuts": {"a": {"route": "erasure"}, "b": {"route": "erasure"}}}
        self.refused("functions", loop_final.loop_function, rec)
        self.refused("functions", loop_final.loop_function, {"loop_cuts": {}})

    def test_abi(self):
        self.refused("abi", loop_final.resources, "n", "C", None, "", 0, 1, ["uint8"])

    def test_stages(self):
        self.refused(
            "stages", loop_final.compose_of, "namespace SolarTV.Milestone.X\n", facts()
        )
        self.refused(
            "stages",
            loop_final.compose_of,
            STAGES.replace("evm7 anyCall", "evm6 anyCall"),
            facts(),
        )
        self.refused(
            "stages",
            loop_final.compose_of,
            STAGES.replace("stage0 evm7", "sched evm7"),
            facts(),
        )

    def test_resources_text(self):
        self.refused("resources", loop_final.parse_resources, "def Kmax : Nat := 1\n")


class Generated(unittest.TestCase):
    def test_parse(self):
        f = facts()
        self.assertTrue(f["bounded"])
        self.assertEqual(
            (f["Kmax"], f["E"], f["B"], f["X"], f["M"]), (13042098468400, 2, 2, 3, None)
        )
        g = facts(CONST)
        self.assertFalse(g["bounded"])
        self.assertEqual(g["M"], 10)

    def test_valid_is_the_same_body(self):
        v = loop_final.valid_module("n", facts())
        self.assertIn(
            "def valid (K : Nat) (call : Call) : Prop :=\n  ∃ σ a0, a0 < 2 ^ 256 ∧ a0 ≤ K ∧ call =",
            v,
        )

    def test_join(self):
        st = loop_final.compose_of(STAGES, facts())
        j = loop_final.join_module("n", "C", "bump", 2987307204, 1, st, facts())
        self.assertIn("theorem join {K : Nat} (hK : K ≤ 13042098468400) :", j)
        self.assertIn("(fun _ => K * 1414400 + 3536012)", j)
        self.assertIn(
            "(SolarTV.Milestone.X.n_mir_to_evmir SolidCore.Refinement.Shapes.oracle)", j
        )
        self.assertIn("kernel_rfl sig_eq sigV (some ([256], true))", j)
        self.assertIn("rintro call ⟨σ, a0, h0, -, rfl⟩", j)
        self.assertIn(loop_final.JOIN_LEMMA, j)
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, j)
        c = loop_final.join_module("n", "C", "run", 1, 0, st, facts(CONST))
        self.assertIn("theorem join :", c)
        self.assertIn("rintro call ⟨σ, rfl⟩", c)
        self.assertIn("(fun _ => 10 * 1414400 + 3536012)", c)

    def test_shape(self):
        s = loop_final.shape("n", "C", "bump", "bump(uint256)", facts())
        self.assertEqual(s["iteration_bound"], {"var": "K", "max": 13042098468400})
        self.assertEqual(
            s["source_ast"],
            {
                "file": "Certified/n/Source.lean",
                "namespace": "Certified.n.Src",
                "contract": "C",
            },
        )
        self.assertTrue(s["excluded"][0].startswith("runtime only"))
        self.assertIn("vacuity", s)
        self.assertEqual(s["fuel"], "2 ^ 256 + 8")
        json.dumps(s)
        c = loop_final.shape("n", "C", "run", "run()", facts(CONST))
        self.assertNotIn("iteration_bound", c)
        self.assertEqual(c["valid"], "Certified.n.LoopValid.valid")


SPEC = {
    "loop_spec": "Certified.n.LoopSpec.bump_loop_spec",
    "index": 1,
    "verify_sha256": "0" * 64,
    "receipt": None,
}


class SpecBridge(unittest.TestCase):
    def test_route_switch(self):
        rec = {"loop_cuts": {"bump": {"route": "erasure-inferred"}}}
        self.assertEqual(loop_final.loop_function(rec), "bump")
        self.assertEqual(loop_final.loop_route(rec, "bump"), "erasure-inferred")
        st = loop_final.compose_of(STAGES, facts())
        j = loop_final.join_module(
            "n", "C", "bump", 2987307204, 1, st, facts(), "erasure-inferred"
        )
        self.assertIn(f"import {loop_final.ROUTE_MODULE}\n", j)
        self.assertIn(f"  {loop_final.JOIN_W} (by decide)", j)
        s = loop_final.spec_join_module(
            "n",
            "C",
            "bump",
            2987307204,
            1,
            st,
            facts(),
            "erasure-inferred",
            SPEC,
            False,
        )
        self.assertIn(f"    {loop_final.JOIN_W_SPEC} (bd :=", s)
        self.assertIn(
            "Certified.n.Loops.bump.leaves Certified.n.Loops.bump.seqs Certified.n.Loops.bump.valid",
            s,
        )

    def test_spec_join(self):
        st = loop_final.compose_of(STAGES, facts())
        j = loop_final.join_module(
            "n", "C", "bump", 2987307204, 1, st, facts(), "erasure", SPEC, True
        )
        self.assertIn("import n.LoopSpec\nimport n.LoopNonVac\n", j)
        self.assertIn(
            f"    {loop_final.JOIN_OBL_SPEC} (bd := Certified.n.gen.C.Spec.C)", j
        )
        self.assertIn(
            f"({loop_final.OBL_OF_SEQS} Certified.n.Loops.bump.seqs Certified.n.Loops.bump.valid)",
            j,
        )
        self.assertIn(
            "(fun c hc => Certified.n.LoopSpec.bump_loop_spec (core_eq.trans hc))", j
        )
        self.assertIn(
            "theorem core_eq : Certified.n.LoopCore.contract? = "
            'SolidCore.Solidity.TypeCheck.CheckedInput.contract Certified.n.Src.importedSourceUnit "C" := rfl',
            j,
        )
        # The statement's spec: the export in the fixed frame, named.
        self.assertIn(
            "(SolidCore.Refinement.Shapes.bundleAllowed Certified.n.gen.C.Spec.C (fun _ => SolidCore.Spec.TreeLoop.ctx0))",
            j,
        )
        self.assertIn(
            "SolidCore.Spec.TreeLoop.certified_ctx0 (by rintro call ⟨σ, a0, -, -, rfl⟩; rfl) h⟩",
            j,
        )
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, j)
        nv = loop_final.join_module(
            "n", "C", "bump", 2987307204, 1, st, facts(), "erasure", SPEC, False
        )
        self.assertNotIn("LoopNonVac", nv)

    def test_witness(self):
        w = loop_final.nonvac_module("n", "C", 2987307204, 1, facts(), 1)
        self.assertIn("theorem post_rejects {K : Nat} (_hK : K ≤ 13042098468400) :", w)
        self.assertIn(
            "{ calldata := Abi.calldata 2987307204 [0], callvalue := 0, storage := [(1, 0)] }",
            w,
        )
        self.assertIn("(Certified.n.gen.C.Spec.C.spec.specs[1]'(by decide))", w)
        self.assertIn("⟨true, [], [], fun _ => 1⟩", w)
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, w)
        c = loop_final.nonvac_module("n", "C", 1, 0, facts(CONST), 0)
        self.assertIn("theorem post_rejects :", c)
        with self.assertRaises(loop_final.Refused):
            loop_final.witness_call("∃ σ a0, call = f σ", 1)

    def test_spec_shape(self):
        s = loop_final.shape("n", "C", "bump", "bump(uint256)", facts(), SPEC, True)
        for k in ("allowed", "post", "vacuity"):
            self.assertNotIn(k, s)
        self.assertTrue(s["allowed_export"])
        self.assertEqual(
            s["spec_export"],
            {
                "ctx": "fun _ => SolidCore.Spec.TreeLoop.ctx0",
                "step": "SolidCore.Refinement.Shapes.idStep",
            },
        )
        self.assertEqual(s["nonvacuous"], "Certified.n.LoopNonVac.post_rejects")
        self.assertIn("succeeds-if", s["allowed_vacuity"])
        self.assertIn("Verify.Spec.TreeLoop", s["spec_modules"])
        self.assertTrue(
            any(e.startswith("environment: the fixed frame") for e in s["excluded"])
        )
        self.assertFalse(any(e.startswith("spec:") for e in s["excluded"]))
        w = loop_final.shape(
            "n",
            "C",
            "bump",
            "bump(uint256)",
            facts(),
            SPEC,
            "the witness does not check",
        )
        self.assertNotIn("nonvacuous", w)
        self.assertIn("the witness does not check", w["vacuity"])
        r = loop_final.shape(
            "n", "C", "bump", "bump(uint256)", facts(), "no spec index"
        )
        self.assertEqual(r["post"], "fun _ _ => True")
        self.assertIn("the spec bridge refused: no spec index", r["vacuity"])

    def test_core_module(self):
        body = (
            "namespace Certified\nnamespace n\nnamespace LoopCore\n\n"
            'def importedSourceName : String := "/tmp/x/n.sol"\n\n'
            "def importedSourceUnit : SourceUnit := { items := [] }\n\n"
            "end LoopCore\nend n\nend Certified\n"
        )
        m = loop_final.core_module(
            "n", "C", body, "Certified/n/n.sol", [("bump(uint256)", 2987307204)]
        )
        self.assertIn('def importedSourceName : String := "Certified/n/n.sol"', m)
        self.assertNotIn("/tmp/x", m)
        self.assertIn("def fuel : Nat := 2 ^ 256 + 8", m)
        self.assertIn("theorem selector_eq :", m)
        self.assertNotIn(chr(0xA7), m)
        with self.assertRaises(loop_final.Refused):
            loop_final.core_module("n", "C", "namespace X\nend X\n", "a.sol", [])

    def test_selector_entries(self):
        fn = {
            "nodeType": "FunctionDefinition",
            "name": "bump",
            "functionSelector": "b20eb4c4",
            "parameters": {
                "parameters": [{"typeDescriptions": {"typeString": "uint256"}}]
            },
        }
        getter = {
            "nodeType": "VariableDeclaration",
            "name": "count",
            "functionSelector": "06661abd",
            "typeName": {"nodeType": "ElementaryTypeName"},
        }
        mapping = {
            "nodeType": "VariableDeclaration",
            "name": "m",
            "functionSelector": "aaaaaaaa",
            "typeName": {"nodeType": "Mapping"},
        }
        ast = {
            "nodes": [
                {
                    "nodeType": "ContractDefinition",
                    "name": "C",
                    "nodes": [fn, getter, mapping],
                }
            ]
        }
        self.assertEqual(
            loop_final.selector_entries(ast, "C"),
            [("bump(uint256)", 0xB20EB4C4), ("count()", 0x06661ABD)],
        )


def _receipt_fixture(d, source_bytes=b"contract C {}\n", exporter="e" * 64):
    """A run directory D with Certified/n/n.sol, a LoopSpec.lean stating the digests and the receipt
    spec_connect.py would write for them. Returns (src, receipt, module, record)."""
    import hashlib

    d = pathlib.Path(d)
    src = d / "C.sol"
    src.write_bytes(source_bytes)
    s = hashlib.sha256(source_bytes).hexdigest()
    c = d / "Certified" / "n"
    c.mkdir(parents=True)
    (c / "n.sol").write_bytes(source_bytes)
    digests = {
        "sourceSha256": s,
        "exportedSha256": "b" * 64,
        "exporterSha256": exporter,
        "astToolSha256": "c" * 64,
        "astToolCacheSha256": "d" * 64,
        "solcSha256": "f" * 64,
    }
    module = c / "LoopSpec.lean"
    module.write_text(
        "".join(f'def {k} : String := "{v}"\n' for k, v in digests.items())
    )
    rec = {
        "module": "Certified.n.LoopSpec",
        "contract": "C",
        "source": "Certified/n/n.sol",
        **digests,
        "moduleSha256": hashlib.sha256(module.read_bytes()).hexdigest(),
    }
    receipt = c / "LoopSpec.lean.receipt.json"
    receipt.write_text(json.dumps(rec))
    return src, receipt, module, {"source": {"file": str(src), "sha256": s}}


class Receipt(unittest.TestCase):
    """Review 189: the receipt is required, and its source digest is the run's."""

    def check(self, d, receipt, module, src_sha, exporter="e" * 64):
        return loop_final.receipt_problem(
            receipt,
            module="Certified.n.LoopSpec",
            contract="C",
            source="Certified/n/n.sol",
            source_sha=src_sha,
            exporter_sha=exporter,
            module_file=module,
        )

    def test_accepts_the_runs_receipt(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            src, receipt, module, record = _receipt_fixture(d)
            s = record["source"]["sha256"]
            self.assertIsNone(self.check(d, receipt, module, s))
            self.assertIsNone(
                loop_final.acceptance_problem(record, src, d, "n", "C", "e" * 64)
            )

    def test_refusals(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            src, receipt, module, record = _receipt_fixture(d)
            s = record["source"]["sha256"]
            # Another source digest, another exporter.
            self.assertIn("sourceSha256", self.check(d, receipt, module, "0" * 64))
            self.assertIn(
                "exporterSha256", self.check(d, receipt, module, s, exporter="1" * 64)
            )
            # The module changed after the receipt.
            module.write_text(module.read_text() + "-- edited\n")
            self.assertIn("moduleSha256", self.check(d, receipt, module, s))
        with tempfile.TemporaryDirectory() as d:
            src, receipt, module, record = _receipt_fixture(d)
            s = record["source"]["sha256"]
            # A coherent receipt of a module whose own digests are not the receipt's.
            r = json.loads(receipt.read_text())
            r["exportedSha256"] = "9" * 64
            receipt.write_text(json.dumps(r))
            self.assertIn("exportedSha256", self.check(d, receipt, module, s))
            receipt.write_text("{")
            self.assertIn("does not parse", self.check(d, receipt, module, s))
            receipt.unlink()
            self.assertIn("no receipt", self.check(d, receipt, module, s))

    def test_acceptance_binds_the_runs_source(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            src, receipt, module, record = _receipt_fixture(d)
            self.assertIn(
                "no source digest",
                loop_final.acceptance_problem({}, src, d, "n", "C", "e" * 64),
            )
            other = {"source": {"sha256": "0" * 64}}
            self.assertIn(
                "not the run's",
                loop_final.acceptance_problem(other, src, d, "n", "C", "e" * 64),
            )
            # A stale coherent pair: the receipt and module of other bytes than the run's.
            src.write_bytes(b"contract C { uint x; }\n")
            fresh = {"source": {"sha256": loop_final.sha(src)}}
            self.assertIn(
                "Certified/n/n.sol",
                loop_final.acceptance_problem(fresh, src, d, "n", "C", "e" * 64),
            )
            (pathlib.Path(d) / "Certified/n/n.sol").write_bytes(src.read_bytes())
            self.assertIn(
                "sourceSha256",
                loop_final.acceptance_problem(fresh, src, d, "n", "C", "e" * 64),
            )


class Exporter(unittest.TestCase):
    """Review 189: the spec is exported by the pinned solar-verify with O6's fixes, never the
    shared checkout's."""

    def fake(self, missing=()):
        def run(cmd, cwd=None, env=None):
            if cmd[:2] == ["git", "-C"] and cmd[3] == "merge-base":
                return (1 if cmd[5] in missing else 0), "", ""
            raise AssertionError(f"spec_exporter runs only git merge-base: {cmd}")

        return run

    def test_pin(self):
        import hashlib
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            exe = pathlib.Path(d) / "solar-verify-pinned"
            exe.write_bytes(b"binary")
            good = hashlib.sha256(b"binary").hexdigest()
            pin = pathlib.Path(d) / "pin"

            def write(sha, binary=exe):
                pin.write_text(
                    json.dumps(
                        {"commit": "a" * 40, "sha256": sha, "tree": d, "binary": str(binary)}
                    )
                )

            write(good)
            path, prov = loop_final.spec_exporter(self.fake(), pin)
            self.assertEqual(path, exe)
            self.assertEqual(prov["sha256"], good)
            self.assertEqual(prov["fixes"], list(loop_final.VERIFY_FIXES))
            for sha, binary, run, why in (
                (good, exe, self.fake((loop_final.VERIFY_FIXES[3],)), "lacks"),
                ("0" * 64, exe, self.fake(), "not the pinned"),
                (good, pathlib.Path(d) / "gone", self.fake(), "missing"),
            ):
                write(sha, binary)
                with self.assertRaises(loop_final.Refused) as e:
                    loop_final.spec_exporter(run, pin)
                self.assertEqual(e.exception.step, "exporter")
                self.assertIn(why, e.exception.why)
            pin.write_text("{}")
            with self.assertRaises(loop_final.Refused):
                loop_final.spec_exporter(self.fake(), pin)

    def test_pinned_commit_has_the_fixes(self):
        import subprocess

        pin = json.loads(loop_final.VERIFY_PIN.read_text())
        root = pathlib.Path(loop_final.HERE).parent.parent
        for c in loop_final.VERIFY_FIXES:
            r = subprocess.run(
                ["git", "-C", str(root), "merge-base", "--is-ancestor", c, pin["commit"]],
                capture_output=True,
            )
            if r.returncode == 128:
                self.skipTest("the pinned commit is not in this repository")
            self.assertEqual(r.returncode, 0, c)


class SeveralLoops(unittest.TestCase):
    """Several loop functions: one part each, and final.py's parts statement."""

    def test_functions(self):
        rec = {"loop_cuts": {"a": {"route": "erasure"}, "b": {"route": "while-inferred"}}}
        self.assertEqual(loop_final.loop_functions(rec), ["a", "b"])
        with self.assertRaises(loop_final.Refused) as c:
            loop_final.loop_functions({"loop_cuts": {}})
        self.assertEqual(c.exception.step, "functions")

    def test_modules(self):
        self.assertEqual(
            loop_final.modules("n"),
            {
                "resources": "n.LoopResources",
                "valid": "n.LoopValid",
                "join": "n.LoopJoin",
                "nonvac": "n.LoopNonVac",
            },
        )
        m = loop_final.modules("n", "ok_transfer")
        self.assertEqual(m["join"], "n.Loop.ok_transfer.Join")
        self.assertEqual(
            loop_final.module_file("/r", m["valid"]),
            pathlib.Path("/r/n/Loop/ok_transfer/Valid.lean"),
        )

    def test_part_modules(self):
        m = loop_final.modules("n", "a")
        st = loop_final.compose_of(STAGES, facts())
        j = loop_final.join_module("n", "C", "a", 1, 1, st, facts(), m=m)
        self.assertIn("namespace Certified.n.Loop.a.Join", j)
        self.assertIn("import n.Loop.a.Resources\nimport n.Loop.a.Valid\n", j)
        self.assertIn("Certified.n.Loop.a.Resources.check_ok", j)
        self.assertIn("(Certified.n.Loop.a.Valid.valid K)", j)
        v = loop_final.valid_module("n", facts(), m["valid"])
        self.assertIn("namespace Certified.n.Loop.a.Valid", v)

    def test_shape_parts(self):
        import final

        shapes = []
        for f, sp in (("a", {"loop_spec": "Certified.n.LoopSpec.a_loop_spec", "index": 0}), ("b", None)):
            m = loop_final.modules("n", f)
            shapes.append(
                loop_final.shape("n", "C", f, f"{f}(uint256)", dict(facts(), E=2, B=2, X=3), sp, True, m)
            )
        sh = loop_final.shape_parts("n", shapes)
        self.assertEqual(sh["proof_module"], "n.LoopJoin")
        self.assertEqual([p["function"] for p in sh["parts"]], ["a", "b"])
        self.assertEqual(sh["parts"][0]["proof"], "Certified.n.Loop.a.Join.join")
        self.assertEqual(sh["parts"][0]["nonvacuous"], "Certified.n.Loop.a.NonVac.post_rejects")
        self.assertIn("vacuity", sh["parts"][1])
        self.assertIn("n.Loop.a.Valid", sh["spec_modules"])
        self.assertIn("n.Loop.b.Valid", sh["spec_modules"])
        for p in sh["parts"]:
            self.assertTrue(set(p) <= set(final.PART_KEYS))
        for k in final.PART_KEYS:
            if k not in ("spec", "excluded"):
                self.assertNotIn(k, sh)
        # final.py states one conjunct per part, each with its own bound.
        import subprocess
        import tempfile

        d = pathlib.Path(tempfile.mkdtemp())
        (d / "shape.json").write_text(json.dumps(sh))
        r = subprocess.run(
            [sys.executable, str(loop_final.HERE / "final.py"), str(d / "shape.json"), "--name", "n",
             "--contract", "C", "--hex", "6001", "--out", str(d / "F.lean"),
             "--statement-out", str(d / "S.lean"), "--record", str(d / "r.json")],
            capture_output=True, text=True,
        )
        self.assertEqual(r.returncode, 0, r.stderr)
        stmt = (d / "S.lean").read_text()
        self.assertEqual(stmt.count("∀ (K : Nat), K ≤ 13042098468400 →"), 2)
        self.assertIn("def NonVacuous_a (K : Nat) : Prop :=", stmt)
        self.assertNotIn("NonVacuous_b", stmt)
        self.assertIn("(Certified.n.Loop.b.Valid.valid K) (fun _ _ => True) (fun _ _ => True)", stmt)
        fin = (d / "F.lean").read_text()
        self.assertIn("fun K hK => ⟨(Certified.n.Loop.a.NonVac.post_rejects) hK, (Certified.n.Loop.a.Join.join) hK⟩", fin)
        self.assertIn("fun K hK => (Certified.n.Loop.b.Join.join) hK", fin)
        rec = json.loads((d / "r.json").read_text())
        self.assertFalse(rec["strongest"])
        self.assertEqual([p["function"] for p in rec["parts"]], ["a", "b"])

    def test_parts_refusals(self):
        import subprocess
        import tempfile

        base = {"kind": "certified", "contract": "M.c", "fuel": "1", "proof_module": "M.J",
                "parts": [{"function": "a", "valid": "M.V.v", "allowed": "fun _ _ => True",
                           "post": "fun _ _ => True", "vacuity": "none", "gas": "fun _ => 1",
                           "proof": "M.J.a"}]}
        bads = [
            dict(base, valid="M.V.v"),
            dict(base, parts=[]),
            dict(base, parts=base["parts"] * 2),
            dict(base, parts=[dict(base["parts"][0], pinned_bytes="M.b")]),
            dict(base, pinned_bytes="M.b"),
            dict(base, kind="open_functional"),
        ]
        for sh in bads:
            d = pathlib.Path(tempfile.mkdtemp())
            (d / "s.json").write_text(json.dumps(sh))
            r = subprocess.run(
                [sys.executable, str(loop_final.HERE / "final.py"), str(d / "s.json"), "--name", "n",
                 "--contract", "C", "--hex", "6001", "--out", str(d / "F.lean"),
                 "--statement-out", str(d / "S.lean")],
                capture_output=True, text=True,
            )
            self.assertEqual(r.returncode, 1, sh)
            self.assertFalse((d / "S.lean").exists())


class WhileSpec(unittest.TestCase):
    """The while route with the exported spec: final_of_whileW_spec and the bridge's WhileSpec."""

    def test_while_spec_join(self):
        st = loop_final.compose_of(STAGES, facts())
        spec = {"loop_spec": "Certified.n.LoopSpec.count_loop_spec", "index": 0}
        j = loop_final.join_module("n", "C", "count", 1, 1, st, facts(), "while-inferred", spec, True)
        self.assertIn(loop_final.JOIN_WHILE_SPEC, j)
        self.assertIn(f"import {loop_final.WHILE_MODULE}\n", j)
        self.assertIn(
            "Certified.n.Loops.count.whileOf_eq\n      sigOf_eq Certified.n.Loops.count.src_seqs "
            "Certified.n.Loops.count.src_valid Certified.n.Loops.count.leaves", j)
        self.assertIn("(by rw [← core_eq]; exact Certified.n.LoopSpec.count_loop_spec)", j)
        self.assertNotIn("erasedOf_eq", j)
        self.assertIn("SolidCore.Spec.TreeLoop.certified_ctx0", j)

    def test_bridge_block(self):
        import spec_connect as sc

        self.assertEqual(
            sc.parse_while("while C.count 0 123 [256] 1 TREE .halt"),
            (0, 123, "[256]", ".halt", 1),
        )
        self.assertEqual(sc.parse_while("while C.count nowhile"), ("nowhile", None, None, None, None))
        self.assertEqual(
            sc.while_specs(["C.count=n.Loops.count.Obl:Certified.n.Loops.count"]),
            {"C.count": ("n.Loops.count.Obl", "Certified.n.Loops.count")},
        )
        out = "\n".join(
            sc.while_block("count", "C.count", 0, 1, 123, "[256]", ".halt", "NS", "M", "M.exported",
                           "u", False, False, "Certified.n.Loops.count")
        )
        self.assertIn("theorem count_tree_eq : Certified.n.Loops.count.Te = count_tree := rfl", out)
        self.assertIn("Verify.LoopSource.Invariant.Final.whileSpecCheck NS.contract? 123 spec 0", out)
        self.assertIn(
            "theorem count_loop_spec : Verify.LoopSource.Invariant.Final.WhileSpec NS.contract? 123 1 [256] "
            "Certified.n.Loops.count.Te spec := by", out)
        self.assertIn("Verify.LoopSource.Invariant.Final.whileSpec_of_check (by decide) count_while_eq", out)
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, out)


HORN_EXPORT = '''/-- The specs of `Horn`. -/
def Horn : Bundle where
  fuel := 0
  spec :=
  { name := "Horn",
    fns := [
    ],
    invariants := [
    ],
    ghosts := [],
    specs := [
    { name := "Horn.ok_transfer",
      selector := 0xfd0011a5,
      params := [(.uint 256)],
      rets := [(.uint 256)],
      payable := false,
      requires := [],
      ensures := [
        (.bin .eq (.ret 0) (.arg 0))],
      succeedsIf := [] }] }

/-- Every spec function call resolves and nests within the fuel. -/
theorem Horn_wf : Horn.WF = true := by decide +kernel
'''


def _fn(name, sel, doc=None, loop=True, kind="function", vis="public"):
    body = {"nodeType": "Block", "statements": [{"nodeType": "WhileStatement"}] if loop else []}
    n = {"nodeType": "FunctionDefinition", "name": name, "kind": kind, "visibility": vis,
         "functionSelector": f"{sel:08x}", "body": body}
    if doc:
        n["documentation"] = {"nodeType": "StructuredDocumentation", "text": doc}
    return n


def _ast(*fns, cdoc=None, extra=()):
    c = {"nodeType": "ContractDefinition", "id": 1, "name": "Horn", "linearizedBaseContracts": [1],
         "contractKind": "contract", "nodes": [*fns, *extra]}
    if cdoc:
        c["documentation"] = {"nodeType": "StructuredDocumentation", "text": cdoc}
    return {"nodeType": "SourceUnit", "nodes": [c]}


OK, BAD = 0xFD0011A5, 0x12345678
PROVED = {"loop_spec": "Certified.h.LoopSpec.ok_transfer_loop_spec", "index": 0}


class SpecGate(unittest.TestCase):
    """Review 206: a part is True only by the checked no-spec determination; a function with a spec,
    or a tag that may carry one, that the bridge does not prove refuses the whole final."""

    parts = [{"fname": "ok_transfer", "sel": OK}, {"fname": "bad_transfer", "sel": BAD}]

    def ast(self, **kw):
        return _ast(_fn("ok_transfer", OK, " @custom:ensures result == n"), _fn("bad_transfer", BAD, kw.get("bad_doc")),
                    cdoc=kw.get("cdoc"))

    def test_inventory_parses_the_export(self):
        self.assertEqual(loop_final.export_inventory(HORN_EXPORT, "Horn"), [("Horn.ok_transfer", OK)])
        with self.assertRaises(loop_final.Refused) as e:
            loop_final.export_inventory(HORN_EXPORT, "Other")
        self.assertEqual(e.exception.step, "inventory")

    def test_proved_and_checked_none(self):
        inv = loop_final.export_inventory(HORN_EXPORT, "Horn")
        none = loop_final.spec_gate(self.ast(), "Horn", self.parts, {"ok_transfer": PROVED, "bad_transfer": None}, inv)
        self.assertEqual([p["fname"] for p in none], ["bad_transfer"])
        text = loop_final.inventory_module("h", "Horn", "ab" * 32, inv, none)
        self.assertIn('= [("Horn.ok_transfer", 0xfd0011a5)] := by', text)
        self.assertIn("theorem bad_transfer_no_spec", text)
        self.assertIn('s.name != "Horn.bad_transfer" && s.selector != 0x12345678', text)
        self.assertIn("theorem no_invariants", text)
        self.assertNotIn("ok_transfer_no_spec", text)

    def test_refusals_refuse_the_whole_final(self):
        inv = loop_final.export_inventory(HORN_EXPORT, "Horn")
        cases = [
            # the bridge refused a specified function (succeeds-if, a failing theorem)
            (self.ast(), {"ok_transfer": "spec_connect.py --loops refuses: succeeds-if", "bad_transfer": None}),
            # the export lists a function the bridge result calls absent
            (self.ast(), {"ok_transfer": None, "bad_transfer": None}),
            # a tag on the function that the export does not list
            (self.ast(bad_doc=" @custom:succeeds-if n > 0"), {"ok_transfer": PROVED, "bad_transfer": None}),
            (self.ast(bad_doc=" @inheritdoc Base"), {"ok_transfer": PROVED, "bad_transfer": None}),
            # a contract-level tag (an invariant) no part states
            (self.ast(cdoc=" @custom:invariant x <= 1"), {"ok_transfer": PROVED, "bad_transfer": None}),
            # a function missing from the bridge's result
            (self.ast(), {"ok_transfer": PROVED}),
        ]
        for ast, specs in cases:
            with self.subTest(specs=specs), self.assertRaises(loop_final.Refused) as e:
                loop_final.spec_gate(ast, "Horn", self.parts, specs, inv)
            self.assertEqual(e.exception.step, "spec")

    def test_proved_spec_must_be_listed(self):
        with self.assertRaises(loop_final.Refused):
            loop_final.spec_gate(self.ast(), "Horn", self.parts[:1], {"ok_transfer": PROVED}, [])

    def test_witness_is_well_typed(self):
        body = "∃ σ a0, a0 < 2 ^ 256 ∧ call = { calldata := Abi.calldata 1 [a0], callvalue := 0, storage := σ }"
        f = {"valid_body": body, "bounded": False}
        text = loop_final.nonvac_module("h", "Horn", OK, 1, f, 0, nrets=2, word=1)
        out = "[" + ", ".join(map(str, ([0] * 31 + [1]) * 2)) + "]"
        self.assertIn(f"⟨true, {out}, [], fun _ => 1⟩", text)
        self.assertIn(f"(words {out}) = some [1, 1]", text)
        # The decoding is never what refutes the postcondition (review 207).
        self.assertNotIn("revert hdec", text)
        self.assertIn("⟨true, [], [], fun _ => 1⟩", loop_final.nonvac_module("h", "Horn", OK, 1, f, 0))

    def test_source_loops(self):
        ast = _ast(_fn("ok_transfer", OK), _fn("bad_transfer", BAD), _fn("plain", 1, loop=False),
                   _fn("helper", 2, loop=True, vis="internal"))
        self.assertEqual(loop_final.source_loops(ast, "Horn"), {"functions": {"ok_transfer", "bad_transfer", "helper"}, "elsewhere": []})
        mod = {"nodeType": "ModifierDefinition", "name": "m", "body": {"nodeType": "ForStatement"}}
        self.assertEqual(loop_final.source_loops(_ast(extra=[mod]), "Horn")["elsewhere"], ["Horn.m"])


MANIFEST = """-- solar-lean-stage: kind=manifest index=0 pass=none
namespace SolarTV.Export.n.C.Manifest

def sources : List (String × String) :=
  [("c.sol", "@SRC@")]

def compiler : String := "0.8.38+commit.unknown.solar.0.3.0"

def arguments : List String :=
  ["c.sol",
   "-Ogas",
   "--evm-version",
   "osaka",
   "--emit=bin-runtime",
   "-Zemit-lean-stages=gen"]

def evmVersion : String := "osaka"

def optimization : String := "Gas"

def stages : List (String × String × String × String) :=
  [("Stage0", "stage0", "lower-evm-shaped", "@H0@"),
   ("Evm1", "evm1", "cfg-simplify-early", "@H1@"),
   ("Bytecode", "bytecode", "assemble", "@H2@")]

def bytecodeSha256 : String := "00"

end SolarTV.Export.n.C.Manifest
"""

TOOLCHAIN = {
    "compiler": "0.8.38+commit.unknown.solar.0.3.0",
    "arguments": ["c.sol", "-Ogas", "--evm-version", "osaka", "--emit=bin-runtime", "-Zemit-lean-stages=gen"],
    "evm_version": "osaka",
    "optimization": "Gas",
}


class ExpectedStatement(unittest.TestCase):
    """Term 6 (review 216, F2): the loop final is final.py's expected statement, written from data
    only, which binds in its type the fresh source and the export's inventory, the compiler, options,
    MIR and stage digests, the bytes, and per function the inhabited valid calls, Shapes.Certified and
    the well-typed NonVacuous, or the checked absence of a spec (the inventory module in the Final's
    closure)."""

    def setUp(self):
        import hashlib
        import tempfile

        self.d = pathlib.Path(tempfile.mkdtemp())
        self.src = self.d / "c.sol"
        self.src.write_text("contract C { function a(uint256 n) public {} }\n")
        self.gen = self.d / "gen" / "C"
        self.gen.mkdir(parents=True)
        self.hashes = []
        for f in ("Stage0", "Evm1", "Bytecode"):
            (self.gen / f"{f}.lean").write_text(f"-- {f}\n")
            self.hashes.append(hashlib.sha256((self.gen / f"{f}.lean").read_bytes()).hexdigest())
        self.src_sha = hashlib.sha256(self.src.read_bytes()).hexdigest()
        m = MANIFEST.replace("@SRC@", self.src_sha)
        for i, h in enumerate(self.hashes):
            # The manifest's own digests are what the kernel compares with the file hashes.
            m = m.replace(f"@H{i}@", "f" * 64)
        (self.gen / "Manifest.lean").write_text(m)
        (self.gen / "Spec.lean").write_text("-- spec\n")

    def tearDown(self):
        import shutil

        shutil.rmtree(self.d, ignore_errors=True)

    def shapes(self, parts=(("a", 0xA0A0A0A0, {"loop_spec": "Certified.n.LoopSpec.a_loop_spec", "index": 0}), ("b", 0xB0B0B0B0, None))):
        out = []
        multi = len(parts) > 1
        for f, sel, sp in parts:
            m = loop_final.modules("n", f if multi else None)
            ns = {"theorem": f"Certified.n.LoopSpecInventory.{f}_no_spec"} if sp is None else None
            out.append(loop_final.shape("n", "C", f, f"{f}(uint256)", dict(facts(), E=2, B=2, X=3), sp, True if sp else False, m,
                                        no_spec=ns, sel=sel, nargs=1))
        return out

    def expected(self, inv=(("C.a", 0xA0A0A0A0),)):
        return {"toolchain": dict(TOOLCHAIN), "inventory": [list(x) for x in inv], "inventory_module": "n.LoopSpecInventory"}

    def run_final(self, sh, *extra, source=True):
        import subprocess

        (self.d / "shape.json").write_text(json.dumps(sh))
        for f in ("S.lean", "F.lean", "r.json"):
            (self.d / f).unlink(missing_ok=True)
        cmd = [sys.executable, str(loop_final.HERE / "final.py"), str(self.d / "shape.json"), "--name", "n",
               "--contract", "C", "--hex", "6001", "--out", str(self.d / "F.lean"),
               "--statement-out", str(self.d / "S.lean"), "--record", str(self.d / "r.json"),
               "--gen", str(self.gen), *extra]
        if source:
            cmd += ["--source", str(self.src)]
        return subprocess.run(cmd, capture_output=True, text=True)

    def test_call_data(self):
        self.assertEqual(loop_final.call_data(facts(), 7, 1),
                         {"selector": 7, "arity": 1, "limit": ["arg", 0], "per_iteration_gas": 1414400,
                          "exit_gas": 3536012, "M": None})
        slot = facts(RES.replace("a0 ≤ K", "SolarTV.slotValue σ 3 ≤ K"))
        self.assertEqual(loop_final.call_data(slot, 7, 1)["limit"], ["slot", 3])
        self.assertEqual(loop_final.call_data(facts(CONST), 7, 1)["limit"], None)
        self.assertEqual(loop_final.call_data(facts(CONST), 7, 1)["M"], 10)
        with self.assertRaises(loop_final.Refused):
            loop_final.call_data(facts(RES.replace("a0 ≤ K", "a0 ≤ 5")), 7, 1)

    def test_witness_states_the_decode(self):
        w = loop_final.nonvac_module("n", "C", 2987307204, 1, facts(), 1, nrets=1)
        self.assertIn("theorem decodes : decodeWords (Certified.n.gen.C.Spec.C.spec.specs[1]'(by decide)).rets", w)
        self.assertIn("(∃ s ∈ Certified.n.gen.C.Spec.C.spec.specs, s.selector = 2987307204 ∧\n"
                      "        ∃ vs, SolidCore.Spec.decodeWords s.rets (SolidCore.Spec.words obs.output) = some vs) ∧", w)
        self.assertIn("decodes⟩, fun h => ?_⟩", w)
        self.assertNotIn("revert hdec", w)

    def test_statement_binds_everything(self):
        sh = loop_final.shape_parts("n", self.shapes(), self.expected())
        self.assertEqual(sh["statement"], "expected")
        for p in sh["parts"]:
            for k in ("valid", "allowed", "post", "gas"):
                self.assertNotIn(k, p)
        r = self.run_final(sh)
        self.assertEqual(r.returncode, 0, r.stderr)
        stmt = (self.d / "S.lean").read_text()
        # No generated proof-side module: only the exports, the source's AST and the library.
        self.assertNotIn("import n.Loop", stmt)
        self.assertNotIn("LoopSpecInventory", stmt)
        self.assertIn("import n.gen.C.Manifest\n", stmt)
        body = stmt.split("def Statement : Prop :=", 1)[1]
        self.assertTrue(body.lstrip().startswith(
            "_root_.Certified.n.Final.code.data.toList.map (·.toNat) = _root_.Certified.n.Final.runtimeBytes ∧"))
        S, M = "_root_.Certified.n.gen.C.Spec", "_root_.SolarTV.Export.n.C.Manifest"
        for want in (
            f'({S}.manifest.sourceSha256 = "{self.src_sha}")',
            f'({S}.C.spec.specs.map (fun s => (s.name, s.selector)) = [("C.a", 0xa0a0a0a0)])',
            f"({S}.C.spec.invariants = [])",
            f'({M}.sources = [("c.sol", "{self.src_sha}")])',
            f'({M}.compiler = "0.8.38+commit.unknown.solar.0.3.0")',
            f'({M}.arguments = ["c.sol", "-Ogas", "--evm-version", "osaka", "--emit=bin-runtime", "-Zemit-lean-stages=gen"])',
            f'({M}.evmVersion = "osaka")',
            f'({M}.optimization = "Gas")',
            f'("Stage0", "stage0", "lower-evm-shaped", "{self.hashes[0]}")',
            f'("Bytecode", "bytecode", "assemble", "{self.hashes[2]}")',
            f'({M}.bytecodeSha256 = "{__import__("hashlib").sha256(bytes.fromhex("6001")).hexdigest()}")',
            # the satisfiable precondition and the well-typed NonVacuous of a, inside its bound
            "∀ (K : Nat), K ≤ 13042098468400 →\n      (∃ (call : _root_.SolarTV.Call), ∃ σ a0, a0 < 2 ^ 256 ∧ a0 ≤ K ∧ call = { calldata := _root_.SolidCore.Refinement.Abi.calldata 2694881440 [a0], callvalue := 0, storage := σ }) ∧",
            f"(∃ s ∈ {S}.C.spec.specs, s.selector = 2694881440 ∧\n          ∃ vs, _root_.SolidCore.Spec.decodeWords s.rets (_root_.SolidCore.Spec.words obs.output) = some vs) ∧",
            "(fun _ => K * 1414400 + 3536012)",
            # b: the kernel-checked absence of a spec, then translation only
            f'(({S}.C.spec.specs.all fun s => s.name != "C.b" && s.selector != 2964369584) = true ∧',
            "(fun _ _ => True) (fun _ _ => True)",
        ):
            self.assertIn(want, stmt)
        # The manifest's own digests are never copied into the statement: the file hashes are.
        self.assertNotIn("f" * 64, stmt)
        fin = (self.d / "F.lean").read_text()
        self.assertIn("import n.LoopSpecInventory\n", fin)
        for want in ("_root_.Certified.n.LoopSpecInventory.source_is_fresh", "_root_.Certified.n.LoopSpecInventory.inventory",
                     "_root_.Certified.n.LoopSpecInventory.no_invariants", "_root_.Certified.n.LoopSpecInventory.b_no_spec",
                     "(Certified.n.Loop.a.NonVac.post_rejects) hK", "(Certified.n.Loop.b.Join.join) hK",
                     "example : _root_.Certified.n.Final.Statement := _root_.Certified.n.Final.final"):
            self.assertIn(want, fin)
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, stmt + fin)
        rec = json.loads((self.d / "r.json").read_text())
        self.assertEqual(rec["statement_kind"], "expected")
        self.assertEqual(rec["expected"]["source_sha256"], self.src_sha)
        self.assertEqual([s["sha256"] for s in rec["expected"]["stages"]], self.hashes)
        self.assertEqual(rec["expected"]["inventory"], [{"name": "C.a", "selector": "0xa0a0a0a0"}])
        self.assertEqual([p["selector"] for p in rec["parts"]], ["0xa0a0a0a0", "0xb0b0b0b0"])
        self.assertFalse(rec["strongest"])

    def test_one_function(self):
        sh = loop_final.shape_parts("n", self.shapes(parts=(("a", 0xA0A0A0A0, {"loop_spec": "x", "index": 0}),)), self.expected())
        self.assertEqual(sh["proof_module"], "n.LoopJoin")
        self.assertEqual(sh["parts"][0]["proof"], "Certified.n.LoopJoin.join")
        r = self.run_final(sh)
        self.assertEqual(r.returncode, 0, r.stderr)
        stmt = (self.d / "S.lean").read_text()
        self.assertNotIn("invariants = []", stmt)
        self.assertIn("(Certified.n.LoopNonVac.post_rejects) hK", (self.d / "F.lean").read_text())

    def test_refusals(self):
        import copy

        good = loop_final.shape_parts("n", self.shapes(), self.expected())

        def m(f):
            s = copy.deepcopy(good)
            f(s)
            return s

        cases = {
            "the spec part's selector is not in the inventory": m(lambda s: s["expected"].update(inventory=[])),
            "a True part the export specifies": m(lambda s: s["expected"]["inventory"].append(["C.b", 0xB0B0B0B0])),
            "a True part named in the export": m(lambda s: s["expected"]["inventory"].append(["C.b", 1])),
            "a part's Lean valid calls": m(lambda s: s["parts"][0].update(valid="M.v K")),
            "no call data": m(lambda s: s["parts"][0].pop("call")),
            "a True part without the checked absence": m(lambda s: s["parts"][1].pop("no_spec")),
            "a no-spec theorem of another module": m(lambda s: s["parts"][1].update(no_spec="Certified.n.Other.b_no_spec")),
            "two parts, one selector": m(lambda s: s["parts"][1]["call"].update(selector=0xA0A0A0A0)),
            "another source semantics": m(lambda s: s.update(source_sem="SolidCore.Refinement.solidCore")),
            "another frame": m(lambda s: s["parts"][0]["spec_export"].update(ctx="fun _ => x")),
            "the compiler ran on another file": m(lambda s: s["expected"]["toolchain"].update(arguments=["d.sol"])),
            "no toolchain": m(lambda s: s["expected"].pop("toolchain")),
            "an iteration bound without the call's limit": m(lambda s: s["parts"][0]["call"].update(limit=None, M=3)),
            "kind compiled": m(lambda s: s.update(kind="compiled")),
        }
        for k, sh in cases.items():
            with self.subTest(k):
                r = self.run_final(sh)
                self.assertEqual(r.returncode, 1, (k, r.stderr))
                self.assertFalse((self.d / "S.lean").exists())
        # Another function's export spec stays in the stated inventory; the statement claims nothing of it.
        r = self.run_final(m(lambda s: s["expected"]["inventory"].append(["C.z", 5])))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn('[("C.a", 0xa0a0a0a0), ("C.z", 0x00000005)]', (self.d / "S.lean").read_text())
        r = self.run_final(good, source=False)
        self.assertEqual(r.returncode, 1)
        self.assertIn("--source", r.stderr)
        # A stage module the manifest does not list, and a listed stage without its file.
        (self.gen / "Evm9.lean").write_text("x")
        r = self.run_final(good)
        self.assertEqual(r.returncode, 1)
        self.assertIn("Evm9", r.stderr)
        (self.gen / "Evm9.lean").unlink()
        (self.gen / "Evm1.lean").unlink()
        r = self.run_final(good)
        self.assertEqual(r.returncode, 1)
        self.assertIn("Evm1", r.stderr)


if __name__ == "__main__":
    unittest.main()
