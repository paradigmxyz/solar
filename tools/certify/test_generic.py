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


LIFE = HERE / "examples" / "lifecycle"
# solar 0479b90a's bin-runtime of the three examples (-Ogas --evm-version osaka).
LIFE_RUNTIME = {
    "NoArgCtor": "346026575f3560e01c80630c55699c14601d5780636d4ce63c036026575b5f545f5260205ff35b5f5ffd",
    "ArgCtor": "346027575f3560e01c80630c55699c14602b5780636d4ce63c14602b5780638da5cb5b146034575b5f5ffd5b5f545f5260205ff3"
               "5b60015460601b60601c5f5260205ff3",
    "ImmutableCtor": "346027575f3560e01c80636d4ce63c14602b578063a4d66daf14602b578063a56dfe4a146053575b5f5ffd5b7f" + "00" * 32 +
                     "5f5260205ff35b5f545f5260205ff3",
}


def life_probe(example, storage, runtime=None, ethlib_storage=None, ethlib="success true", constructor="returns", logs="0"):
    """A lifecycle probe result as run_lifecycle_probe parses it (PROBE lines)."""
    pairs = lambda xs: "[" + ", ".join(f"({k}, {v})" for k, v in xs) + "]"
    return {"typecheck": "ok", "constructor": constructor, "storage": pairs(storage), "ethlib": ethlib,
            "runtime": LIFE_RUNTIME[example] if runtime is None else runtime,
            "ethlib storage": pairs(storage if ethlib_storage is None else ethlib_storage), "ethlib logs": logs}


# What the real probe printed for the three examples with solar 0479b90a and solidity-lean 49d1d7f5:
# NoArgCtor sets slot 0 to 7; ArgCtor(42, 0xbeef) sets slots 0 and 1; ImmutableCtor(1000) sets
# slot 0 to 1001 and fills the immutable's PUSH32 placeholder with 1000.
REAL = {
    "NoArgCtor": ("", life_probe("NoArgCtor", [(0, 7)])),
    "ArgCtor": ("42,0xbeef", life_probe("ArgCtor", [(0, 42), (1, 0xbeef)])),
    "ImmutableCtor": ("1000", life_probe("ImmutableCtor", [(0, 1001)], runtime=LIFE_RUNTIME["ImmutableCtor"].replace(
        "7f" + "00" * 32, "7f" + (1000).to_bytes(32, "big").hex()))),
}


class LifecycleHelpers(unittest.TestCase):
    """The lifecycle tier's argument, runtime and storage checks."""

    def test_words(self):
        self.assertEqual(generic.constructor_words([], ""), [])
        self.assertEqual(generic.constructor_words(["uint256", "address"], "42, 0xbeef"), [42, 0xbeef])
        self.assertEqual(generic.constructor_words(["bool", "uint8"], "1,255"), [1, 255])

    def test_words_refused(self):
        for types, text, needle in [(["uint256", "address"], "42", "takes 2 argument"), ([], "1", "takes 0 argument"),
                                    (["address"], "0x" + "1" * 41, "outside address"), (["uint8"], "256", "outside uint8"),
                                    (["bool"], "2", "outside bool"), (["uint256"], "-1", "outside uint256"),
                                    (["int256"], "1", "static uintN, address and bool"), (["string"], "1", "static uintN"),
                                    (["uint256"], "x", "not a number")]:
            with self.assertRaises(generic.Blocked) as e:
                generic.constructor_words(types, text)
            self.assertIn(needle, str(e.exception.reasons), (types, text))

    def test_runtime(self):
        rt = LIFE_RUNTIME["ImmutableCtor"]
        filled = REAL["ImmutableCtor"][1]["runtime"]
        self.assertIsNone(generic.runtime_problem(rt, rt, []))
        self.assertIsNone(generic.runtime_problem(filled, rt, ["limit"]))
        self.assertIn("not solar's bin-runtime", generic.runtime_problem(filled, rt, []))
        self.assertIn("outside an immutable placeholder", generic.runtime_problem("ff" + rt[2:], rt, ["limit"]))
        self.assertIn("solar's bin-runtime has", generic.runtime_problem(filled + "00", rt, ["limit"]))

    def test_storage_pairs(self):
        self.assertEqual(generic.storage_pairs("[(1, 48879), (0, 42)]"), [(0, 42), (1, 48879)])
        self.assertEqual(generic.storage_pairs("[]"), [])


class LifecycleStages(unittest.TestCase):
    """Every lifecycle run ends at a stage that fails: the kernel when the observed stages agree
    (UNSUPPORTED-lifecycle), else the first stage that disagrees (REJECTED)."""

    def first(self, example, res, imm=()):
        st = generic.lifecycle_stages(res, LIFE_RUNTIME[example], list(imm))
        self.assertEqual([q["stage"] for q in st], ["source", "initcode", "runtime bytes", "storage", "kernel"])
        self.assertFalse(st[-1]["ok"], "the kernel stage must never pass yet")
        return next(q for q in st if not q["ok"])

    def test_real_examples_agree(self):
        for ex, (_, res) in REAL.items():
            self.assertEqual(self.first(ex, res, ["limit"] if ex == "ImmutableCtor" else [])["stage"], "kernel", ex)

    def test_immutable_without_immutables(self):
        self.assertEqual(self.first("ImmutableCtor", REAL["ImmutableCtor"][1])["stage"], "runtime bytes")

    def test_storage_disagrees(self):
        res = life_probe("ArgCtor", [(0, 42), (1, 0xbeef)], ethlib_storage=[(0, 42), (1, 0)])
        q = self.first("ArgCtor", res)
        self.assertEqual(q["stage"], "storage")
        self.assertIn("EthLib's run", q["detail"])

    def test_init_reverts(self):
        self.assertEqual(self.first("ArgCtor", life_probe("ArgCtor", [], ethlib="success false"))["stage"], "initcode")
        self.assertEqual(self.first("ArgCtor", life_probe("ArgCtor", [], ethlib="none"))["stage"], "initcode")

    def test_constructor_fails_or_logs(self):
        self.assertEqual(self.first("ArgCtor", life_probe("ArgCtor", [], constructor="undefined"))["stage"], "storage")
        self.assertEqual(self.first("ArgCtor", life_probe("ArgCtor", [(0, 1)], constructor="returns, with logs"))["stage"], "storage")
        self.assertEqual(self.first("ArgCtor", life_probe("ArgCtor", [(0, 1)], logs="1"))["stage"], "storage")

    def test_source_rejected(self):
        self.assertEqual(self.first("NoArgCtor", dict(life_probe("NoArgCtor", [(0, 7)]), typecheck="error"))["stage"], "source")


class LifecycleRun(unittest.TestCase):
    """lifecycle_run end to end with solar, git, the front end and the probe replaced: it exits 4
    with UNSUPPORTED-lifecycle when every observed stage agrees and 1 at a stage that does not, and
    never 0."""

    def drive(self, example, args, res, ast=None):
        src = LIFE / f"{example}.sol"
        bins = {"bin": "6080" + LIFE_RUNTIME[example], "bin-runtime": LIFE_RUNTIME[example]}
        if ast is None:
            ast = {"nodes": [{"nodeType": "ContractDefinition", "name": example, "nodes": []}]}
        saved = (generic.run, generic.git, generic.frontend, generic.run_lifecycle_probe, generic.log)
        seen = {}

        def fake_run(cmd, *a, **kw):
            return 0, json.dumps({"contracts": {f"{src}:{example}": bins}}), ""

        def fake_probe(lean_dir, work, name, contract, source_text, creation, ws, cache):
            seen["creation"], seen["ws"] = creation, ws
            return res

        def finish(status, code, line, **extra):
            seen.update(status=status, code=code, line=line)
            raise SystemExit(code)
        generic.run, generic.git = fake_run, (lambda *a, **kw: "b" * 40)
        generic.frontend = lambda *a, **kw: (0, "-- body", "")
        generic.run_lifecycle_probe, generic.log = fake_probe, (lambda *a, **kw: None)
        record = {}
        try:
            with tempfile.TemporaryDirectory() as t, self.assertRaises(SystemExit):
                a = argparse.Namespace(constructor_args=args, solidity_lean=t, base="b" * 40, solc="solc", probe_cache=None)
                generic.lifecycle_run(a, record, "solar", src, "0" * 64, "x", example, ast, pathlib.Path(t) / "w", finish)
        finally:
            generic.run, generic.git, generic.frontend, generic.run_lifecycle_probe, generic.log = saved
        self.assertNotEqual(seen["code"], 0)
        return seen, record

    def ctor_ast(self, example, types, imm=()):
        params = [{"typeDescriptions": {"typeString": t}} for t in types]
        nodes = [{"nodeType": "FunctionDefinition", "kind": "constructor", "parameters": {"parameters": params}}]
        nodes += [{"nodeType": "VariableDeclaration", "name": n, "mutability": "immutable"} for n in imm]
        return {"nodes": [{"nodeType": "ContractDefinition", "name": example, "nodes": nodes}]}

    def test_no_arg_constructor(self):
        seen, rec = self.drive("NoArgCtor", None, REAL["NoArgCtor"][1], self.ctor_ast("NoArgCtor", []))
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertTrue(seen["line"].startswith("UNSUPPORTED-lifecycle: kernel: "))
        self.assertEqual(seen["ws"], [])
        self.assertEqual(rec["lifecycle"]["storage"], [(0, 7)])

    def test_constructor_with_args(self):
        seen, rec = self.drive("ArgCtor", "42,0xbeef", REAL["ArgCtor"][1], self.ctor_ast("ArgCtor", ["uint256", "address"]))
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        init = bytes.fromhex("6080" + LIFE_RUNTIME["ArgCtor"])
        self.assertEqual(seen["creation"], init + (42).to_bytes(32, "big") + (0xbeef).to_bytes(32, "big"))
        self.assertEqual(rec["lifecycle"]["args"], ["0x2a", "0xbeef"])
        self.assertEqual(rec["lifecycle"]["creation_sha256"], hashlib.sha256(seen["creation"]).hexdigest())

    def test_immutables(self):
        seen, rec = self.drive("ImmutableCtor", "1000", REAL["ImmutableCtor"][1], self.ctor_ast("ImmutableCtor", ["uint256"], ["limit"]))
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertEqual(rec["lifecycle"]["immutables"], ["limit"])
        self.assertNotEqual(rec["lifecycle"]["runtime_sha256"], rec["lifecycle"]["bin_runtime_sha256"])

    def test_disagreement_rejected(self):
        res = life_probe("ArgCtor", [(0, 42), (1, 0xbeef)], ethlib_storage=[(0, 41), (1, 0xbeef)])
        seen, _ = self.drive("ArgCtor", "42,0xbeef", res, self.ctor_ast("ArgCtor", ["uint256", "address"]))
        self.assertEqual((seen["status"], seen["code"]), ("REJECTED", 1))
        self.assertTrue(seen["line"].startswith("REJECTED: lifecycle: storage: "), seen["line"])

    def test_wrong_arity_blocked(self):
        with self.assertRaises(generic.Blocked):
            self.drive("ArgCtor", "42", REAL["ArgCtor"][1], self.ctor_ast("ArgCtor", ["uint256", "address"]))


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class LifecycleEnvelope(unittest.TestCase):
    """The examples through solc: constructor parameters stay outside the other tiers' envelope,
    and the lifecycle tier reads their types and the immutables."""

    def test_examples(self):
        want = {"NoArgCtor": ([], []), "ArgCtor": (["uint256", "address"], []), "ImmutableCtor": (["uint256"], ["limit"])}
        for ex, (types, imm) in want.items():
            src = LIFE / f"{ex}.sol"
            ast, contracts = generic.solc_ast(str(SOLC), src)
            self.assertEqual(generic.constructor_params(ast, ex), types, ex)
            self.assertEqual(generic.immutables(ast, ex), imm, ex)
            generic.envelope(src.read_text(), ast, contracts, None, None, False, True)
            if types:
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(src.read_text(), ast, contracts, None, None, False, False)
                self.assertIn("--tier lifecycle", str(e.exception.reasons))

    def test_cli(self):
        with tempfile.TemporaryDirectory() as t:
            src = LIFE / "ArgCtor.sol"
            p = subprocess.run([sys.executable, str(HERE / "generic.py"), str(src), "none", "--constructor-args", "1,2",
                                "--solidity-lean", "/nonexistent", "--work", t + "/w", "--record-dir", t + "/r"],
                               capture_output=True, text=True, check=False)
            self.assertEqual(p.returncode, 2)
            self.assertIn("--constructor-args needs --tier lifecycle", p.stderr)
            p = subprocess.run([sys.executable, str(HERE / "generic.py"), "--help"], capture_output=True, text=True, check=False)
            self.assertIn("lifecycle", p.stdout)
            self.assertIn("UNSUPPORTED-lifecycle", p.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
