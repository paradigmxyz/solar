#!/usr/bin/env python3
"""Negative controls for tools/certify/generic.py that need no Lean: the strict predicate's source
binding and --replay's manifest check. Run: python3 tools/certify/test_generic.py"""

import argparse
import contextlib
import hashlib
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic  # noqa: E402

NAME, CONTRACT = "meter_ca0b7786", "Meter"
BOUND = (f"Certified/{NAME}/Source.lean is the front end's AST of {CONTRACT} in "
         f"Certified/{NAME}/{NAME}.sol (solc {generic.SOLC_VERSION})")


def full_record(source=BOUND, source_ast=BOUND):
    return {
        "name": NAME, "contract": CONTRACT, "final": {"statement": "x"}, "source_ast": source_ast,
        "statuses": {"equivalence": "proved", "termination": "proved", "resources": "proved",
                     "spec": "none", "source": source, "provenance": "clean", "strongest": False},
        "replay": {"closure": 10, "replayed_count": 10, "trusted_count": 0, "unaccounted": []},
    }


class SourceBound(unittest.TestCase):
    def test_bound(self):
        ok, comps, _ = generic.strict(full_record(), "compiled")
        self.assertTrue(ok)
        self.assertTrue(comps["source_bound"])

    def test_refused(self):
        cases = {
            "empty": ("", BOUND),
            "unknown": ("unknown", "unknown"),
            "missing": (None, None),
            "not bound": ("not bound: the checked SolidCore source AST is not regenerated", None),
            "other contract": (BOUND.replace("AST of Meter", "AST of Gate"), BOUND.replace("AST of Meter", "AST of Gate")),
            "other file": (BOUND.replace("Source.lean", "Src.lean"), BOUND.replace("Source.lean", "Src.lean")),
            "record disagrees": (BOUND, ""),
        }
        for label, (src, ast) in cases.items():
            with self.subTest(label):
                rec = full_record(src, ast)
                if src is None:
                    del rec["statuses"]["source"]
                ok, comps, _ = generic.strict(rec, "compiled")
                self.assertFalse(comps["source_bound"], label)
                self.assertFalse(ok, label)


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


class ReplayManifest(unittest.TestCase):
    def make(self, root):
        d = root / "run"
        (d / "inputs/Certified" / NAME).mkdir(parents=True)
        (d / "Meter.sol").write_text("contract Meter {}\n")
        (d / "inputs/Certified" / NAME / "shape.json").write_text("{}\n")
        (d / "certify.log").write_text("log\n")
        files = {str(q.relative_to(d)): sha(q) for q in d.rglob("*") if q.is_file()}
        rec = {"status": "COMPILED", "source": {"sha256": files["Meter.sol"]}, "run_files": files}
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def replay(self, d):
        # --solidity-lean points nowhere: a run directory that passes the checks fails later, at
        # the fresh run, with a different message.
        p = subprocess.run([sys.executable, str(HERE / "generic.py"), "--replay", str(d), "--solidity-lean", "/nonexistent",
                            "--work", str(d.parent / "w")], capture_output=True, text=True)
        return p.returncode, p.stdout.strip().splitlines()[-1] if p.stdout.strip() else ""

    def test_stray_outside_inputs(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "stray.txt").write_text("x\n")
            code, line = self.replay(d)
            self.assertEqual(code, 1)
            self.assertIn("REJECTED: replay: files the record's manifest does not list: ['stray.txt']", line)

    def test_stray_nested(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "logs").mkdir()
            (d / "logs" / "extra.lean").write_text("x\n")
            code, line = self.replay(d)
            self.assertEqual(code, 1)
            self.assertIn("logs/extra.lean", line)

    def test_modified_outside_inputs(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "certify.log").write_text("edited\n")
            code, line = self.replay(d)
            self.assertEqual(code, 1)
            self.assertIn("REJECTED: replay: certify.log differs from the record", line)

    def test_modified_source(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "Meter.sol").write_text("contract Meter { uint x; }\n")
            code, line = self.replay(d)
            self.assertEqual(code, 1)
            self.assertIn("REJECTED: replay: Meter.sol differs from the record", line)

    def test_clean_passes_the_checks(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            code, line = self.replay(d)
            self.assertNotIn("differs from the record", line)
            self.assertNotIn("manifest does not list", line)


def claims_record():
    """A certified run's certify-record.json (scripts/certify's record) and the driver's record.json."""
    cr = full_record()
    cr["statuses"].update(spec="proved", strongest=True, excluded=["deployment", "events"])
    cr["final"] = {"theorem": f"Certified.{NAME}.Final.final", "statement": "code = runtime ∧ Expected src spec runtime",
                   "shape": "SolidCore.Verify.Generic.Expected", "assumptions": [], "excluded": ["deployment", "events"],
                   "nonvacuous": f"Certified.{NAME}.Final.NonVacuous"}
    cr["replay"].update(replayed=[{"module": f"M{i}", "olean_sha256": f"{i:064x}"} for i in range(10)],
                        trusted={}, replayed_beyond_plan=[], checker={"path": "/x/leanchecker", "sha256": "c" * 64})
    cr["caveats"] = []
    cr["end_to_end_statement"] = {}
    ok, need_c, comps = generic.strict(cr, "certified")
    rec = {"name": NAME, "contract": CONTRACT, "requested_tier": "certified", "status": "CERTIFIED",
           "strict": {"tier": "certified", "holds": ok, "components": need_c},
           "tier_reached": generic.reached_tier(comps), "published_statement": cr["end_to_end_statement"],
           **{k: json.loads(json.dumps(cr[k])) for k in generic.CLAIM_KEYS}}
    return cr, rec


class ReplayClaims(unittest.TestCase):
    """record.json is not hashed: --replay must hold its claims to certify-record.json and the fresh run."""

    def make(self, root, edit=None):
        cr, rec = claims_record()
        d = root / "run"
        d.mkdir()
        (d / "Meter.sol").write_text("contract Meter {}\n")
        (d / "certify.log").write_text("log\n")
        (d / "certify-record.json").write_text(json.dumps(cr))
        (d / "inputs/Certified" / NAME).mkdir(parents=True)
        (d / "inputs/Certified" / NAME / "Source.lean").write_text("-- generated\n")
        (d / "inputs/lakefile.lean").write_text("-- lakefile\n")
        files = {str(q.relative_to(d)): sha(q) for q in d.rglob("*") if q.is_file()}
        ins = {r[len("inputs/"):]: v for r, v in files.items() if r.startswith("inputs/")}
        rec.update(source={"sha256": files["Meter.sol"]}, run_files=files,
                   solidity_lean={"inputs_commit": "a" * 40, "inputs": sorted(ins), "inputs_sha256": ins})
        rec["caveats"] = cr["caveats"] = [f"solidity-lean at {'a' * 40}, not the pinned {'b' * 40}"]
        (d / "certify-record.json").write_text(json.dumps(cr))
        files["certify-record.json"] = sha(d / "certify-record.json")
        if edit:
            edit(rec)
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def replay(self, d, fresh_edit=None):
        """replay_main in this process, its fresh run replaced by one that writes the record's own
        claims back (edited by fresh_edit): the checks run, no Lean does."""
        orig = json.loads((d / "record.json").read_text())

        def fake_run(cmd, *args, **kw):
            fresh = json.loads(json.dumps(orig))
            if fresh_edit:
                fresh_edit(fresh)
            out = pathlib.Path(cmd[cmd.index("--record-dir") + 1])
            out.mkdir(parents=True)
            (out / "record.json").write_text(json.dumps(fresh))
            return subprocess.CompletedProcess(cmd, 0)

        a = argparse.Namespace(replay=str(d), work=str(d.parent / "w"), solidity_lean="/nonexistent",
                               packages_from=None, probe_cache=None)
        buf = io.StringIO()
        saved = generic.subprocess.run
        generic.subprocess.run = fake_run
        try:
            with contextlib.redirect_stdout(buf), self.assertRaises(SystemExit) as e:
                generic.replay_main(a)
        finally:
            generic.subprocess.run = saved
        return e.exception.code, buf.getvalue().strip().splitlines()[-1]

    def test_control_certified(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(self.make(pathlib.Path(t)))
            self.assertEqual(code, 0, line)
            self.assertTrue(line.startswith("CERTIFIED: replay of "), line)

    def test_tampered_record_rejected(self):
        def drop_module(r):
            del r["replay"]["replayed"][3]

        def empty_excluded(r):
            r["final"]["excluded"] = []
            r["statuses"]["excluded"] = []

        cases = {
            "final.theorem renamed": (lambda r: r["final"].update(theorem="Certified.other.Final.final"), "final.theorem"),
            "final.statement edited": (lambda r: r["final"].update(statement="True"), "final.statement"),
            "replay.closure raised": (lambda r: r["replay"].update(closure=11), "replay.closure"),
            "replayed module dropped": (drop_module, "replay.replayed"),
            "excluded emptied": (empty_excluded, "final.excluded"),
            "statuses.excluded emptied": (lambda r: r["statuses"].update(excluded=[]), "statuses.excluded"),
            "caveats emptied": (lambda r: r.update(caveats=None), "caveats"),
            "strict holds forged": (lambda r: r["strict"].update(holds=False), "strict.holds"),
        }
        for label, (edit, field) in cases.items():
            with self.subTest(label), tempfile.TemporaryDirectory() as t:
                code, line = self.replay(self.make(pathlib.Path(t), edit))
                self.assertEqual(code, 1, line)
                self.assertTrue(line.startswith(f"REJECTED: replay: {field}"), line)

    def test_certify_record_missing(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            rec = json.loads((d / "record.json").read_text())
            del rec["run_files"]["certify-record.json"]
            (d / "certify-record.json").unlink()
            (d / "record.json").write_text(json.dumps(rec))
            code, line = self.replay(d)
            self.assertEqual(code, 1, line)
            self.assertIn("without a certify-record.json", line)

    def test_fresh_run_differs(self):
        cases = {
            "final.statement": lambda r: r["final"].update(statement="True"),
            "statuses.excluded": lambda r: r["statuses"].update(excluded=[]),
            "replay.closure": lambda r: r["replay"].update(closure=11, replayed_count=11),
            "replay.replayed": lambda r: r["replay"]["replayed"].pop(),
        }
        for field, edit in cases.items():
            with self.subTest(field), tempfile.TemporaryDirectory() as t:
                code, line = self.replay(self.make(pathlib.Path(t)), edit)
                self.assertEqual(code, 1, line)
                self.assertTrue(line.startswith(f"REJECTED: replay: {field}"), line)

    def test_archived_input_edited_and_rehashed(self):
        """An archived generated input edited, with only its run_files digest updated: the pin
        inputs_sha256 (which the fresh run reproduces) still names the generated one."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            f = d / "inputs/Certified" / NAME / "Source.lean"
            f.write_text("-- edited\n")
            rec = json.loads((d / "record.json").read_text())
            rec["run_files"][f"inputs/Certified/{NAME}/Source.lean"] = sha(f)
            (d / "record.json").write_text(json.dumps(rec))
            code, line = self.replay(d)
            self.assertEqual(code, 1, line)
            self.assertIn(f"REJECTED: replay: inputs/Certified/{NAME}/Source.lean is not the generated input", line)

    def test_archived_input_added(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            f = d / "inputs/Extra.lean"
            f.write_text("-- extra\n")
            rec = json.loads((d / "record.json").read_text())
            rec["run_files"]["inputs/Extra.lean"] = sha(f)
            (d / "record.json").write_text(json.dumps(rec))
            code, line = self.replay(d)
            self.assertEqual(code, 1, line)
            self.assertIn("REJECTED: replay: inputs/Extra.lean is not the generated input", line)

    def test_fresh_caveats_differ(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(self.make(pathlib.Path(t)), lambda r: r.update(caveats=[]))
            self.assertEqual(code, 1, line)
            self.assertTrue(line.startswith("REJECTED: replay: caveats"), line)

    def test_fresh_inputs_commit_may_differ(self):
        """A fresh run commits the same inputs at another time: only that hash differs in its caveats."""
        def recommit(r):
            r["solidity_lean"]["inputs_commit"] = "c" * 40
            r["caveats"] = [x.replace("a" * 40, "c" * 40) for x in r["caveats"]]

        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(self.make(pathlib.Path(t)), recommit)
            self.assertEqual(code, 0, line)

    def test_fresh_olean_digests_may_differ(self):
        def redigest(r):
            r["replay"]["replayed"][0]["olean_sha256"] = "f" * 64

        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(self.make(pathlib.Path(t)), redigest)
            self.assertEqual(code, 0, line)


SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"


def run_driver(src_text, spec, root):
    """The driver up to its first refusal; its envelope and spec checks run before any build."""
    src = root / "C.sol"
    src.write_text(src_text)
    p = subprocess.run([sys.executable, str(HERE / "generic.py"), str(src), spec, "--solidity-lean", "/nonexistent",
                        "--work", str(root / "w"), "--record-dir", str(root / "r"), "--solc", str(SOLC)],
                       capture_output=True, text=True)
    return p.returncode, (p.stdout.strip().splitlines() or [""])[-1]


HEAD = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class SpecGates(unittest.TestCase):
    def test_environment_clause_blocked(self):
        src = HEAD + """contract C { uint256 public x;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:requires block.chainid == 424242
    /// @custom:ensures x == 8
    function f() external { x = 1; } }"""
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertEqual(code, 3)
            self.assertTrue(line.startswith("BLOCKED: environment"), line)

    def test_msg_value_clause_allowed(self):
        src = HEAD + """contract C { uint256 public x;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures x == 1
    function f() external { x = 1; } }"""
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertNotIn("BLOCKED", line)
            self.assertNotIn("succeeds-if", line)

    def test_no_succeeds_if_refused(self):
        src = HEAD + """contract C { uint256 public x;
    /// @custom:ensures x == 1
    function f() external { x = 1; } }"""
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertEqual(code, 1)
            self.assertIn("REJECTED: spec: no function has @custom:succeeds-if", line)

    def test_spec_none_needs_no_succeeds_if(self):
        src = HEAD + """contract C { uint256 public x;
    function f() external { x = 1; } }"""
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "none", pathlib.Path(t))
            self.assertNotIn("succeeds-if", line)


def final_py(shape, root, hexcode="6001"):
    sp = root / "shape.json"
    sp.write_text(json.dumps(shape))
    p = subprocess.run([sys.executable, str(HERE / "final.py"), str(sp), "--name", "c_1", "--contract", "C", "--hex", hexcode,
                        "--out", str(root / "F.lean"), "--statement-out", str(root / "S.lean")], capture_output=True, text=True)
    return p.returncode, p.stderr.strip(), (root / "S.lean").read_text() if (root / "S.lean").exists() else ""


def expected_shape(**over):
    s = {"kind": "certified", "statement": "expected", "contract": "L.contract?", "fuel": "L.fuel",
         "valid": "V", "allowed_export": True, "spec_export": {"ctx": "X.ctxOfCall", "step": "X.idStep"},
         "nonvacuous": "P.nonvacuous", "gas": "fun _ => 7", "env": [], "spec": "s", "spec_modules": ["L"],
         "proof_module": "P", "proof": "P.published", "source_unit": "Src.importedSourceUnit",
         "source_contract": "C", "storage": "L.storageLit",
         "entries": [{"signature": "f()", "fn": "L.fn0", "sel": 1, "arity": 0}]}
    s.update(over)
    return s


class ExpectedStatement(unittest.TestCase):
    def test_writes_expected(self):
        with tempfile.TemporaryDirectory() as t:
            code, err, stmt = final_py(expected_shape(), pathlib.Path(t))
            self.assertEqual(code, 0, err)
            self.assertIn("_root_.SolidCore.Verify.Generic.Expected _root_.Certified.c_1.Final.src", stmt)
            self.assertIn("Contexts ctx := ctx = _root_.SolidCore.Verify.Generic.Context.default", stmt)
            self.assertIn("assumptions := []", stmt)

    def test_refused(self):
        cases = {
            "assumptions": expected_shape(env=["True"]),
            "no non-vacuity": {k: v for k, v in expected_shape().items() if k != "nonvacuous"},
            "compiled kind": expected_shape(kind="compiled", allowed_export=None),
            "post not from the export": {**{k: v for k, v in expected_shape().items() if k != "spec_export"}, "post": "Q"},
            "no entries": expected_shape(entries=[]),
            "tactic in an input": expected_shape(gas="by decide"),
            "other statement": expected_shape(statement="shapes"),
        }
        for label, shape in cases.items():
            with self.subTest(label):
                with tempfile.TemporaryDirectory() as t:
                    code, err, _ = final_py({k: v for k, v in shape.items() if v is not None}, pathlib.Path(t))
                    self.assertNotEqual(code, 0, label)


class SpecConnectEmission(unittest.TestCase):
    """A succeeds-if over the call's environment alone leaves some of tree_succeeds's steps
    nothing to rewrite; the emitted proof must allow that, not call the macro."""

    def test_tolerant_succeeds_proof(self):
        import spec_connect

        lines = spec_connect.succeeds_block(
            "set", 1, "", "  tree_meets_fixed [set_tree, set_spec,]", [], [], "TreeMeetsFixed", "M", "B"
        )
        text = "\n".join(lines)
        self.assertNotIn("tree_succeeds [", text)
        self.assertIn("try simp only [set_tree, set_spec, M.spec, B, M.ctx, *", text)
        self.assertIn("first | omega | (simp_all; done) | (simp_all; omega)", text)
        self.assertNotIn("@DS@", text)


class ResourceFailures(unittest.TestCase):
    """A kernel that runs out of memory is reported as resources, never as a refuted proof."""

    def test_kernel_memory(self):
        log = ["X.lean:97:4: (kernel) excessive memory consumption detected", "error: build failed"]
        self.assertEqual(len(generic.resource_failures(log)), 1)

    def test_proof_failure_is_not_resources(self):
        log = ["X.lean:70:2: `simp` made no progress", "error: build failed"]
        self.assertEqual(generic.resource_failures(log), [])


class Utf8Boundary(unittest.TestCase):
    """A message cut at 160 bytes inside a multibyte character does not decide the verdict."""

    HERE = pathlib.Path(__file__).resolve().parent

    def boundary_line(self):
        # A long work path that puts the 3-byte hygienic x\u271d of a Lean message at bytes 159-161.
        msg = "error: Tactic \x60rcases\x60 failed: \x60x\u271d : ?m.318\x60 is not an inductive datatype"
        head = "/Final.lean:24:11: " + msg.split("\u271d")[0]
        work = "/w/" + "p" * (159 - len(head.encode()) - 3)
        line = work + "/Final.lean:24:11: " + msg
        self.assertEqual(line.encode().index("\u271d".encode()), 159)
        return line

    def test_byte_cut_streams(self):
        line = self.boundary_line()
        with tempfile.TemporaryDirectory() as td:
            src = pathlib.Path(td) / "final.out"
            src.write_text(line + "\n")
            logf = pathlib.Path(td) / "certify.log"
            cmd = ["bash", "-c", 'echo "neg: refused ($(cut -c1-160 "$1"))"; exit 3', "-", str(src)]
            with contextlib.redirect_stdout(io.StringIO()) as out:
                rc = generic.stream(cmd, logf)
            self.assertEqual(rc, 3)
            self.assertIn("\ufffd", logf.read_text())
            self.assertEqual(out.getvalue(), logf.read_text())

    def test_run_replaces(self):
        rc, out, _ = generic.run(["bash", "-c", "printf 'a\\342\\234b'"])
        self.assertEqual((rc, out), (0, "a\ufffdb"))

    def test_final_neg_clip(self):
        line = self.boundary_line()
        src = (self.HERE / "final_neg.sh").read_text()
        fn = next(l for l in src.splitlines() if l.startswith("clip() {"))
        r = subprocess.run(["bash", "-c", fn + "\nclip"], input=line.encode(), capture_output=True)
        self.assertEqual(r.returncode, 0)
        self.assertEqual(r.stdout.decode("utf-8"), line.encode()[:159].decode())
        r = subprocess.run(["bash", "-c", fn + "\nclip"], input=b"x" * 200, capture_output=True)
        self.assertEqual(r.stdout, b"x" * 160)
        self.assertNotIn("cut -c1-160", src)


class SpecConnectCheck(unittest.TestCase):
    """spec_connect.py names the theorem a false spec fails in, before anything says connected."""

    def test_failing_theorem_named(self):
        import spec_connect

        out = ["namespace M", "theorem clear_meets0 : P := by", "  first", "  | (a)", "  | (b)", "",
               "theorem clear_succeeds : Q := by", "  omega"]
        text = ("SpecConnect.lean:5:5: error: omega could not prove the goal:\n"
                "SpecConnect.lean:8:2: error: omega could not prove the goal:\n")
        self.assertEqual(spec_connect.failing_theorems(text, out),
                         [("clear_meets0", 5, "omega could not prove the goal:"),
                          ("clear_succeeds", 8, "omega could not prove the goal:")])


class SpecRejectionPath(unittest.TestCase):
    """A rejected spec ends with spec_connect's REJECTED line, and scripts/certify's guard on the
    captured output refuses cleanly even when the capture is empty (no bad array subscript)."""

    def test_rejected_line_is_the_verdict(self):
        log = ["spec_connect: REJECTED: spec: set_meets0 does not check: omega could not prove the goal: (SpecConnect.lean:67)",
               "certify: error: spec_connect.py refuses functions of C (above): their spec is not proved"]
        self.assertEqual(generic.spec_rejection(log),
                         ["REJECTED: spec: set_meets0 does not check: omega could not prove the goal: (SpecConnect.lean:67)"])

    def test_guard_on_empty_capture(self):
        script = (HERE.parent.parent / "scripts" / "certify").read_text()
        guard = next(l.strip() for l in script.splitlines() if "spec_connect.py refuses functions of" in l and "exit 0" in l)
        for capture in ("true", "echo 'exit 1'"):
            prog = ("set -euo pipefail; contract=C; die() { echo \"certify: error: $*\" >&2; exit 1; }\n"
                    f"mapfile -t holds < <({capture})\n{guard}\necho passed\n")
            p = subprocess.run(["bash", "-c", prog], capture_output=True, text=True)
            self.assertEqual(p.returncode, 1, capture)
            self.assertIn("their spec is not proved", p.stderr)
            self.assertNotIn("bad array subscript", p.stderr)
            self.assertNotIn("passed", p.stdout)


class FrontendLocation(unittest.TestCase):
    """Source.lean must not depend on where the source file sits (a replay reads it from the
    record directory): the same source at two paths gives the same front-end body."""

    def test_same_body_from_two_paths(self):
        import os
        lean = os.environ.get("SOLIDITY_LEAN_DIR")
        solc = os.environ.get("SOLC", str(pathlib.Path.home() / ".cache/solc/solc-0.8.35"))
        if not lean or not pathlib.Path(solc).exists():
            self.skipTest("needs SOLIDITY_LEAN_DIR and solc")
        src = (HERE / "examples" / "counter.sol").read_bytes()
        sha = hashlib.sha256(src).hexdigest()
        bodies = []
        for sub in ("a", "b/c"):
            with tempfile.TemporaryDirectory() as d:
                p = pathlib.Path(d) / sub / "counter.sol"
                p.parent.mkdir(parents=True)
                p.write_bytes(src)
                rc, body, err = generic.frontend(lean, p, sha, "counter_x", "Counter", solc)
                self.assertEqual(rc, 0, err)
                bodies.append(body)
        self.assertEqual(bodies[0], bodies[1])
        self.assertIn("counter_x.sol", bodies[0])


class LeanMemory(unittest.TestCase):
    """--lean-memory: the default stays 6000 MB and a value that is not a cap is refused."""

    def test_values(self):
        self.assertEqual(generic.LEAN_MEMORY, "6000")
        self.assertEqual(generic.lean_memory_mb("12000"), "12000")
        for bad in ("", "6G", "-1", "999"):
            with self.assertRaises(generic.argparse.ArgumentTypeError):
                generic.lean_memory_mb(bad)


class SpecConnectWordProof(unittest.TestCase):
    """A branch-free tree's spec without succeeds-if conditions tries the returned-word proof first,
    keeps both earlier proofs as fallbacks, and names every definition of the simp set in it."""

    def test_order(self):
        import spec_connect
        ds = "f_tree, f_spec, SolidCore.Spec.FnSpec.noSucc, M.spec, B, M.ctx"
        block = spec_connect.succeeds_block("f", 1, "", "X", [], [], "TreeMeetsFixed", "M", "B")[3]
        alts = [l for l in block.splitlines() if l.startswith("  | ")]
        self.assertEqual(len(alts), 3)
        self.assertTrue(alts[0].startswith("  | (intro args s iargs pre"))
        self.assertIn("tree_meets_map' [" + ds + "]", alts[1])
        self.assertEqual(alts[2], "  | (tree_meets_fixed [" + ds + "])")
        self.assertEqual(block.count(ds), 6)
        self.assertNotIn("@DS@", block)


class SpecConnectAnyCaller(unittest.TestCase):
    """--any-caller: the every-caller block names no fixed context, refuses what it cannot state, and
    reads the probe's symbolic-caller lines."""

    def test_block_has_no_fixed_context(self):
        import spec_connect
        for nsi in (0, 1):
            block = "\n".join(spec_connect.from_block("f", "C.f", 2, nsi, "NS", "M", "B", "[256, 256]", "NS.selector_eq"))
            for fixed in (" ctx ", "(ctx :=", "ctxOf ", "0x2000", "callerWord", "callerAddress", "Generic.ctx"):
                self.assertNotIn(fixed, block)
            self.assertIn("∀ a, a < 2 ^ 160 → a ≠ selfAddress →", block)
            self.assertIn("HoldsFn (solidCoreFrom a NS.fuel) c (valueScope fn_f deployed) spec", block)
            self.assertIn("(SolidCore.Verify.AnyCaller.ctxOfCallFrom a) step f_spec", block)
            self.assertIn("eraseFnSym (envOf c.core) NS.fuel f", block)
            self.assertIn("#print axioms f_holds_from", block)
            self.assertEqual("f_succeeds_from" in block, bool(nsi))

    def test_refusals(self):
        import spec_connect
        r = spec_connect.any_caller_refusal
        self.assertIsNone(r((False, "[256]", "t"), ([], [])))
        self.assertIn("payable", r((True, "[256]", "t"), ([], [])))
        self.assertIn("eraseFnSym", r(("noerase", None, None), ([], [])))
        self.assertIn("no function", r(None, ([], [])))
        self.assertIn("mappings", r((False, "[256]", "t"), ([0], [])))

    def test_parse_sym(self):
        import spec_connect
        self.assertEqual(spec_connect.parse_sym("sym C.f false some [256, 160] TREE (Tree.halt x)"),
                         ("C.f", False, "[256, 160]", "(Tree.halt x)"))
        self.assertEqual(spec_connect.parse_sym("sym C.f noerase"), ("C.f", "noerase", None, None))


if __name__ == "__main__":
    unittest.main(verbosity=2)
