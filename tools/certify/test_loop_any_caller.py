#!/usr/bin/env python3
"""loop_any_caller.py. Without binaries: the module is built from the export and bytes it is given
(both inlined, nothing fixed imported, the selector and manifest guards stated), the sender label is
per selector, what it refuses before Lean (a wrong manifest digest, a mis-tagged selector), export_check
verifies the bytes' digest, and certify (with stand-in solar, solar-verify and Lean) hashes its own
snapshot, refuses a wrong-digest export before Lean and runs the fixed-caller witness probe only on a
sender reference. With LOOP_ANY_CALLER_LEAN (a Lean checker command): the kernel refuses a mis-tagged
selector and a wrong manifest digest in the module. With LOOP_ANY_CALLER_SOLAR and
LOOP_ANY_CALLER_VERIFY as well, the default path (certify: compile, export, generate, check) on the
canonical Counter.sol and on mutations of it: the canonical source is checked and its module is the
fixture; ensures false is refused as an every-caller failure, ensures msg.sender == 0x2000 as
caller-dependent with the witness 0x2000, ensures false && msg.sender == msg.sender as an
every-caller failure without a witness; changed bytes and a wrong-digest exporter are refused; a
sender reference in another selector's spec keeps bump certified.
Run: python3 tools/certify/test_loop_any_caller.py"""

import hashlib
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import typing
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import loop_any_caller as L

DATA = HERE / "testdata" / "loop_any_caller"
SOURCE = (DATA / "Counter.sol").read_text()
SPEC = (DATA / "Spec.lean").read_text()
HEX = (DATA / "runtime.hex").read_text().strip()
SRC_SHA = hashlib.sha256((DATA / "Counter.sol").read_bytes()).hexdigest()
BUMP_ENSURES = "(.bin .eq (.load (.slot 0) (.lit 0) 32 (.uint 256)) (.bin .add (.old (.load (.slot 0) (.lit 0) 32 (.uint 256))) (.arg 0)))"
BUMP_BLOCK_START = '{ name := "Counter.bump",'
INC_BLOCK_START = '{ name := "Counter.increment",'
SENDER_EQ = "(.bin .eq (.env .sender) (.lit 8192))"
HEX_SHA = hashlib.sha256(bytes.fromhex(HEX)).hexdigest()
ZEROS = "0" * 64
SPEC_NS = "Verify.Phase2.CounterCaller.Spec"

SOLAR = os.environ.get("LOOP_ANY_CALLER_SOLAR")
VERIFY = os.environ.get("LOOP_ANY_CALLER_VERIFY")
LEAN = os.environ.get("LOOP_ANY_CALLER_LEAN")


def edit_block(old, new, start=BUMP_BLOCK_START, text=SPEC):
    i = text.index(start)
    j = text.index("succeedsIf", i)
    block = text[i:j]
    assert old in block, old
    return text[:i] + block.replace(old, new) + text[j:]


def pair(spec=SPEC, hexcode=HEX, **kw):
    kw.setdefault("source_sha256", SRC_SHA)
    return L.any_caller_pair(spec, hexcode, **kw)


def with_manifest(spec, field, value):
    out, n = re.subn(
        rf'{field} := "[0-9a-f]{{64}}"', f'{field} := "{value}"', spec, count=1
    )
    assert n == 1
    return out


def statement(text, name):
    i = text.index(f"theorem {name} ")
    return text[i : text.index(":= by", i)]


class Module(unittest.TestCase):
    def test_counter_is_the_checked_module(self):
        route, text, notes = pair()
        self.assertEqual(route, "no-sender-reference")
        self.assertEqual(notes, [])
        self.assertEqual(text, (DATA / "CounterCaller.lean").read_text())
        lean = os.environ.get("SOLIDITY_LEAN")
        if lean:
            self.assertEqual(
                text,
                (pathlib.Path(lean) / "Verify/Phase2/CounterCaller.lean").read_text(),
            )

    def test_export_is_inlined_and_nothing_fixed_is_imported(self):
        _, text, _ = pair()
        self.assertIn(L.inline_export(SPEC), text)
        imports = [l for l in text.splitlines() if l.startswith("import ")]
        self.assertEqual(
            imports,
            [
                "import Verify.Spec.Contract",
                "import Verify.Phase2.CallerFrom",
                "import Verify.Phase2.CounterRun",
                "import Verify.AllValue.CounterValue",
            ],
        )
        self.assertIn(
            "abbrev B : Bundle := Verify.Phase2.CounterCaller.Spec.Counter\n", text
        )

    def test_mutated_export_is_what_the_module_states(self):
        # The review's reproduction: a mutated spec must change the module, never emit the old one.
        _, base, _ = pair()
        for new in ("(.lit 0)", SENDER_EQ):
            _, text, _ = pair(edit_block(BUMP_ENSURES, new))
            self.assertNotEqual(text, base)
            b = text[
                text.index(BUMP_BLOCK_START) : text.index(
                    "succeedsIf", text.index(BUMP_BLOCK_START)
                )
            ]
            self.assertIn(new, b)
            self.assertNotIn(BUMP_ENSURES, b)

    def test_bytes_are_the_runs(self):
        _, text, _ = pair()
        self.assertIn(
            "theorem runtime_is_fresh : SolarTV.Export.Counter.bytecode = runtimeBytes := by decide +kernel",
            text,
        )
        self.assertIn("abbrev code : ByteArray := runtimeCode runtimeBytes", text)
        lit = text[
            text.index("def runtimeBytes : List Nat :=") : text.index(
                "/-- **The translation proofs"
            )
        ]
        self.assertEqual(
            bytes(
                int(b, 16)
                for b in lit.split(":=", 1)[1]
                .replace("[", "")
                .replace("]", "")
                .split(",")
            ),
            bytes.fromhex(HEX),
        )
        flipped = HEX[:-2] + ("00" if HEX[-2:] != "00" else "01")
        fsha = hashlib.sha256(bytes.fromhex(flipped)).hexdigest()
        _, other, _ = pair(with_manifest(SPEC, "bytecodeSha256", fsha), hexcode=flipped)
        self.assertNotEqual(other, text)
        self.assertIn(f'"{fsha}" := ⟨rfl, rfl⟩', other)
        # No statement names the pinned bytes; only runtime_is_fresh and code_eq do.
        for thm in (
            "bump_translation_from",
            "bump_value_translation_from",
            "bump_bytes_meet_from",
        ):
            self.assertNotIn("SolarTV.Export.Counter.bytecode", statement(text, thm))
        with self.assertRaisesRegex(L.Refused, "not hex"):
            pair(hexcode="zz")

    def test_pair_is_two_theorems_over_every_caller(self):
        _, text, _ = pair()
        for thm in ("bump_translation_from", "bump_property_from"):
            self.assertIn(
                f"theorem {thm} : ∀ c, contract? = .ok c →\n    ∀ x : Nat, x < 2 ^ 160 → x ≠ selfAddress →",
                text,
            )
        prop = text[
            text.index("theorem bump_property_from") : text.index(
                "/-! ## Translation preservation -/"
            )
        ]
        trans = text[
            text.index("theorem bump_translation_from") : text.index(
                "/-! ## Calls with a value -/"
            )
        ]
        self.assertNotIn("runAt", prop)
        self.assertNotIn("code", prop)
        self.assertNotIn("bundlePost", trans)
        self.assertNotIn("0x2000", prop + trans)
        # T1's proof is the property-free run theorem, not the theorem with BumpPost in it.
        self.assertIn("Verify.Phase2.CounterRun.counter_run_any", trans)
        self.assertNotIn("counter_partial_any", text)

    def test_guards_are_stated(self):
        _, text, _ = pair()
        self.assertIn(
            "theorem bump_spec_selector : bump_spec.selector = bumpSel := by decide\n",
            text,
        )
        self.assertIn(
            f'theorem manifest_is_the_runs :\n    {SPEC_NS}.manifest.sourceSha256 =\n        "{SRC_SHA}" ∧\n'
            f'      {SPEC_NS}.manifest.bytecodeSha256 =\n        "{HEX_SHA}" := ⟨rfl, rfl⟩\n',
            text,
        )
        # The guards precede the property section, so the witness probe has them too.
        w = L.witness_module(text, L.COUNTER, L.COUNTER["ns"])
        self.assertIn("theorem bump_spec_selector", w)
        self.assertIn("theorem bump_meets_at_witness : ∀ a, a = 0x2000 →", w)
        self.assertNotIn("theorem bump_meets_from", w)
        self.assertNotIn("theorem bump_property_from", w)
        self.assertTrue(
            w.rstrip().endswith(
                "#print axioms Verify.Phase2.CounterCaller.bump_meets_at_witness"
            )
        )

    def test_value_pair_for_nonpayable(self):
        _, text, _ = pair()
        self.assertIn("theorem bump_value_translation_from", text)
        self.assertIn("theorem bump_value_property_from", text)

    def test_other_specs_by_selector(self):
        _, text, _ = pair()
        self.assertIn(
            "  rcases hfs with rfl | rfl | rfl\n  · exact absurd hs (by decide)\n  · exact bump_meets_from a ha hne\n  · exact absurd hs (by decide)\n",
            text,
        )
        self.assertIn(
            "(the other two,\n`increment` and `max`, have other selectors)", text
        )


class SenderLabel(unittest.TestCase):
    def test_sender_in_ensures(self):
        route, text, notes = pair(edit_block(BUMP_ENSURES, SENDER_EQ))
        self.assertEqual(route, "sender-reference")
        self.assertTrue(any("caller-dependence witness" in n for n in notes))
        self.assertFalse(any(L.CALLER_DEPENDENT in n for n in notes))
        self.assertIn("which the statement reads as the quantified caller", text)

    def test_sender_in_invariant(self):
        spec = SPEC.replace(
            "invariants := [\n",
            "invariants := [\n    (.bin .ne (.env .sender) (.lit 0)),\n",
            1,
        )
        self.assertEqual(pair(spec)[0], "sender-reference")

    def test_sender_in_another_selector_only(self):
        # increment's clauses are not bump's: the label is per selector.
        spec = edit_block(
            "(.lit 1)))]", "(.lit 1))), " + SENDER_EQ + "]", start=INC_BLOCK_START
        )
        bundle = L.parse_bundle(spec, "Counter")
        self.assertEqual(L.sender_label(bundle, 0xD09DE08A), "sender-reference")
        self.assertEqual(pair(spec)[0], "no-sender-reference")

    def test_sender_through_a_called_spec_function(self):
        fn = '{ name := "who",\n      params := [],\n      ret := .address,\n      body := (.env .sender),\n      isOpaque := false }'
        spec = SPEC.replace("fns := [\n    ]", "fns := [\n    " + fn + "]", 1)
        self.assertEqual(
            pair(spec)[0], "no-sender-reference"
        )  # defined, not called by bump
        called = edit_block(
            BUMP_ENSURES, '(.bin .ne (.call "who" []) (.lit 0))', text=spec
        )
        self.assertEqual(pair(called)[0], "sender-reference")

    def test_sender_inside_a_string_is_not_a_reference(self):
        spec = edit_block(BUMP_ENSURES, '(.bin .eq (.var ".env .sender") (.lit 0))')
        self.assertEqual(pair(spec)[0], "no-sender-reference")

    def test_undefined_spec_function_refused(self):
        with self.assertRaisesRegex(L.Refused, "does not define"):
            pair(edit_block(BUMP_ENSURES, '(.call "nope" [])'))

    def test_routes_say_what_they_mean(self):
        self.assertEqual(
            L.ROUTES["no-sender-reference"],
            "no sender reference in this selector's clauses",
        )


class FailsClosed(unittest.TestCase):
    def test_payable_keeps_zero_value_only(self):
        route, text, notes = pair(edit_block("payable := false", "payable := true"))
        self.assertEqual(route, "no-sender-reference")
        self.assertTrue(notes and notes[0].startswith("payable"))
        self.assertNotIn("bump_value_", text)
        self.assertNotIn("CounterValue", text)
        self.assertIn("Calls with a value are outside this module (payable", text)

    def test_succeeds_if_refused(self):
        with self.assertRaisesRegex(L.Refused, "succeeds-if"):
            pair(_succeeds())

    def test_two_arguments_refused(self):
        with self.assertRaisesRegex(L.Refused, "one uint256 argument"):
            pair(
                edit_block(
                    "params := [(.uint 256)]", "params := [(.uint 256), (.uint 256)]"
                )
            )

    def test_unknown_function_refused(self):
        with self.assertRaisesRegex(L.Refused, "no single function spec"):
            pair(profile=dict(L.COUNTER, spec_name="Counter.nope"))

    def test_extra_import_refused(self):
        with self.assertRaisesRegex(L.Refused, "imports more"):
            pair(SPEC.replace("\n\n/-!", "\nimport Evil\n\n/-!", 1))

    def test_other_contract_refused(self):
        with self.assertRaisesRegex(L.Refused, "exactly one Bundle"):
            pair(SPEC.replace("def Counter : Bundle where", "def Other : Bundle where"))

    def test_mis_tagged_selector_refused(self):
        with self.assertRaisesRegex(
            L.Refused, "selector 0x12345678, not the source's bumpSel"
        ):
            pair(edit_block("selector := 0xb20eb4c4", "selector := 0x12345678"))

    def test_wrong_manifest_digest_refused(self):
        with self.assertRaisesRegex(
            L.Refused, "bytecode with SHA-256 0{64}, not the run's"
        ):
            pair(with_manifest(SPEC, "bytecodeSha256", ZEROS))
        with self.assertRaisesRegex(L.Refused, "a source with SHA-256 0{64}"):
            pair(with_manifest(SPEC, "sourceSha256", ZEROS))
        with self.assertRaisesRegex(L.Refused, "a source with SHA-256"):
            pair(source_sha256=ZEROS)
        two = SPEC.replace(
            'bytecodeSha256 := "',
            f'bytecodeSha256 := "{HEX_SHA}",\n    bytecodeSha256 := "',
            1,
        )
        with self.assertRaisesRegex(L.Refused, "one bytecodeSha256"):
            pair(two)


class Refusal(unittest.TestCase):
    """kernel_refusal: what a failed Lean run is reported as."""

    def setUp(self):
        _, self.text, _ = pair()
        self.path = pathlib.Path("/w/CounterCaller.lean")

    def log_at(self, decl):
        line = next(
            i
            for i, l in enumerate(self.text.splitlines(), 1)
            if l.startswith(f"theorem {decl} ")
        )
        return f"{self.path}:{line + 1}:2: error: omega could not prove the goal\n"

    def test_every_caller_failure_without_a_probe(self):
        why = L.kernel_refusal(self.text, self.log_at("bump_meets_from"), self.path)
        self.assertTrue(why.startswith(L.EVERY_CALLER_FAILED), why)
        self.assertNotIn("caller-dependent", why)
        self.assertIn("bump_meets_from", why)

    def test_caller_dependent_only_with_a_witness(self):
        log = self.log_at("bump_meets_from")
        why = L.kernel_refusal(self.text, log, self.path, witness=True)
        self.assertTrue(why.startswith(L.CALLER_DEPENDENT), why)
        self.assertIn("witness 0x2000", why)
        why = L.kernel_refusal(self.text, log, self.path, witness=False)
        self.assertTrue(why.startswith(L.NO_WITNESS), why)
        self.assertNotIn("caller-dependent", why)

    def test_guards_are_named(self):
        for decl, want in (
            ("runtime_is_fresh", "not the bytes the translation proofs are about"),
            ("manifest_is_the_runs", "manifest is not of the run's source and bytes"),
            ("bump_spec_selector", "another selector than the source's"),
        ):
            log = self.log_at(decl) + self.log_at("bump_meets_from")
            why = L.kernel_refusal(self.text, log, self.path, witness=True)
            self.assertIn(want, why)
            self.assertNotIn("caller-dependent", why)
            self.assertTrue(L.guard_failed({decl}))


class ExportCheck(unittest.TestCase):
    """export_check.py spec verifies the manifest's bytecode digest against the run's."""

    def check(self, spec_text, *extra):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "Spec.lean"
            f.write_text(spec_text)
            r = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(HERE / "export_check.py"),
                    "spec",
                    str(f),
                    SPEC_NS,
                    "--source",
                    str(DATA / "Counter.sol"),
                    "--contract",
                    "Counter",
                    *extra,
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        return r.returncode, r.stderr

    def test_right_digest_passes(self):
        self.assertEqual(self.check(SPEC, "--bytecode-sha256", HEX_SHA)[0], 0)
        self.assertEqual(self.check(SPEC, "--bytecode-sha256", HEX_SHA.upper())[0], 0)

    def test_wrong_digest_refused(self):
        rc, err = self.check(
            with_manifest(SPEC, "bytecodeSha256", ZEROS), "--bytecode-sha256", HEX_SHA
        )
        self.assertEqual(rc, 1)
        self.assertIn(f"bytecode with SHA-256 {ZEROS}, not the run's ({HEX_SHA})", err)
        rc, err = self.check(SPEC, "--bytecode-sha256", ZEROS)
        self.assertEqual(rc, 1)
        rc, err = self.check(SPEC, "--bytecode-sha256", "xyz")
        self.assertEqual(rc, 1)
        self.assertIn("is not a SHA-256", err)

    def test_digest_read_from_the_manifest_only(self):
        # A digest string outside the manifest (here a trusted item) is not the manifest's.
        decoy = SPEC.replace(
            "def trusted : List String := []",
            f'def trusted : List String := ["bytecodeSha256 := \\"{HEX_SHA}\\""]',
        )
        bad = with_manifest(decoy, "bytecodeSha256", ZEROS)
        self.assertEqual(self.check(bad, "--bytecode-sha256", HEX_SHA)[0], 1)
        two = SPEC.replace(
            'bytecodeSha256 := "',
            f'bytecodeSha256 := "{HEX_SHA}",\n    bytecodeSha256 := "',
            1,
        )
        rc, err = self.check(two, "--bytecode-sha256", HEX_SHA)
        self.assertEqual(rc, 1)
        self.assertIn("bytecodeSha256 2 times", err)

    def test_wrong_source_refused(self):
        rc, err = self.check(with_manifest(SPEC, "sourceSha256", ZEROS))
        self.assertEqual(rc, 1)
        self.assertIn("is of a source with SHA-256", err)


FAKE_SOLAR = r"""#!PY -I
import json, os, pathlib, sys
hexcode = pathlib.Path(os.environ["FAKE_HEX"]).read_text().strip()
print(json.dumps({"version": "0.8.38+commit.unknown.solar.0.3.0",
                  "contracts": {sys.argv[1] + ":Counter": {"bin-runtime": hexcode}}}))
"""

FAKE_VERIFY = r"""#!PY -I
import hashlib, os, pathlib, re, sys
a = sys.argv[1:]
out = a[a.index("--emit-lean-spec") + 1]
bsha = a[a.index("--lean-spec-bytecode-sha256") + 1]
if os.environ.get("FAKE_WRONG_DIGEST"):
    bsha = "0" * 64
src = hashlib.sha256(pathlib.Path(a[-1]).read_bytes()).hexdigest()
t = pathlib.Path(os.environ["FAKE_SPEC"]).read_text()
t = re.sub(r'sourceSha256 := "[0-9a-f]{64}"', f'sourceSha256 := "{src}"', t, count=1)
t = re.sub(r'bytecodeSha256 := "[0-9a-f]{64}"', f'bytecodeSha256 := "{bsha}"', t, count=1)
pathlib.Path(out).write_text(t)
"""

# A stand-in checker: records each file it is given; fails at bump_meets_from on the module, and on
# the witness probe unless FAKE_WITNESS_OK.
FAKE_LEAN = r"""#!PY -I
import os, pathlib, sys
f = pathlib.Path(sys.argv[-1])
with open(os.environ["FAKE_CALLS"], "a") as c:
    c.write(f.name + chr(10))
if os.environ.get("FAKE_PASS"):
    sys.exit(0)
if f.name.endswith("Witness.lean"):
    sys.exit(0 if os.environ.get("FAKE_WITNESS_OK") else 1)
lines = f.read_text().splitlines()
n = next(i for i, l in enumerate(lines, 1) if l.startswith("theorem bump_meets_from "))
print(f"{f}:{n + 1}:2: error: omega could not prove the goal")
sys.exit(1)
"""


class FakeRun(unittest.TestCase):
    """certify end to end with stand-in binaries: what reaches Lean, and how a failure is labeled."""

    def setUp(self):
        self.d = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.d)
        self.bins = {}
        for name, body in (
            ("solar", FAKE_SOLAR),
            ("verify", FAKE_VERIFY),
            ("lean", FAKE_LEAN),
        ):
            f = self.d / f"fake-{name}"
            f.write_text(body.replace("PY", sys.executable, 1))
            f.chmod(f.stat().st_mode | stat.S_IXUSR)
            self.bins[name] = str(f)
        self.calls = self.d / "calls"
        self.env = {
            "FAKE_HEX": str(DATA / "runtime.hex"),
            "FAKE_SPEC": str(DATA / "Spec.lean"),
            "FAKE_CALLS": str(self.calls),
        }

    def run_certify(self, spec=None, **env):
        if spec is not None:
            sf = self.d / "Spec.in.lean"
            sf.write_text(spec)
            env["FAKE_SPEC"] = str(sf)
        src = self.d / "src" / "Counter.sol"
        src.parent.mkdir(exist_ok=True)
        src.write_text(SOURCE)
        old = dict(os.environ)
        os.environ.update(self.env, **env)
        try:
            return L.certify(
                src,
                "Counter",
                solar=self.bins["solar"],
                verify=self.bins["verify"],
                lean=self.bins["lean"],
                work=self.d / "w",
            )
        except L.Refused as e:
            return e.record
        finally:
            os.environ.clear()
            os.environ.update(old)

    def lean_calls(self):
        return self.calls.read_text().split() if self.calls.exists() else []

    def test_wrong_digest_refused_before_lean(self):
        rec = self.run_certify(FAKE_WRONG_DIGEST="1")
        self.assertEqual(rec["status"], "refused")
        self.assertIn("not of this run's source and bytes", rec["why"])
        self.assertIn(
            f"bytecode with SHA-256 {ZEROS}, not the run's ({HEX_SHA})", rec["why"]
        )
        self.assertNotIn("lean_rc", rec)
        self.assertNotIn("module", rec)
        self.assertEqual(self.lean_calls(), [])

    def test_honest_export_reaches_lean(self):
        rec = self.run_certify(FAKE_PASS="1")
        self.assertEqual(rec["status"], "checked", rec.get("why"))
        self.assertEqual(rec["runtime_sha256"], HEX_SHA)
        self.assertEqual(self.lean_calls(), ["CounterCaller.lean"])

    def test_snapshot_is_hashed_not_the_original(self):
        # The original changes right after it is copied: the record (and the manifest check) are
        # of the copy that was compiled and exported.
        real = L.shutil.copyfile

        def copy_then_edit(a, b):
            r = real(a, b)
            pathlib.Path(a).write_text(SOURCE + "// edited\n")
            return r

        L.shutil.copyfile = copy_then_edit
        try:
            rec = self.run_certify(FAKE_PASS="1")
        finally:
            L.shutil.copyfile = real
        self.assertEqual(rec["status"], "checked", rec.get("why"))
        self.assertEqual(rec["source_sha256"], SRC_SHA)
        self.assertNotEqual(
            hashlib.sha256((self.d / "src" / "Counter.sol").read_bytes()).hexdigest(),
            SRC_SHA,
        )

    def test_failure_without_sender_reference_has_no_probe(self):
        rec = self.run_certify()
        self.assertTrue(rec["why"].startswith(L.EVERY_CALLER_FAILED), rec["why"])
        self.assertNotIn("witness", rec)
        self.assertEqual(self.lean_calls(), ["CounterCaller.lean"])

    def test_sender_reference_probes_the_fixed_caller(self):
        spec = edit_block(BUMP_ENSURES, SENDER_EQ)
        rec = self.run_certify(spec, FAKE_WITNESS_OK="1")
        self.assertTrue(rec["why"].startswith(L.CALLER_DEPENDENT), rec["why"])
        self.assertEqual(rec["witness"]["caller"], "0x2000")
        self.assertTrue(rec["witness"]["holds"])
        self.assertEqual(
            self.lean_calls(), ["CounterCaller.lean", "CounterCallerWitness.lean"]
        )
        self.calls.unlink()
        rec = self.run_certify(spec)
        self.assertTrue(rec["why"].startswith(L.NO_WITNESS), rec["why"])
        self.assertFalse(rec["witness"]["holds"])
        self.assertNotIn("caller-dependent", rec["why"])


@unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
class LeanGuards(unittest.TestCase):
    """The kernel refuses a module whose export is mis-tagged or whose manifest is not the run's,
    even when the generator's own checks are bypassed."""

    def test_kernel_refuses_mis_tag_and_wrong_manifest(self):
        spec = edit_block("selector := 0xb20eb4c4", "selector := 0x12345678")
        _, text, _ = pair(spec, profile=dict(L.COUNTER, selector=None))
        good = f'.bytecodeSha256 =\n        "{HEX_SHA}" := ⟨rfl, rfl⟩'
        self.assertIn(good, text)
        text = text.replace(good, good.replace(HEX_SHA, ZEROS))
        with tempfile.TemporaryDirectory() as d:
            mod = pathlib.Path(d) / "CounterCaller.lean"
            mod.write_text(text)
            r = subprocess.run(
                [*L.shlex.split(LEAN), str(mod)],
                check=False,
                capture_output=True,
                text=True,
            )
        log = r.stdout + r.stderr
        self.assertNotEqual(r.returncode, 0, log[-2000:])
        where = {w for w, _ in L.kernel_errors(text, log, mod)}
        self.assertIn("bump_spec_selector", where, log[-2000:])
        self.assertIn("manifest_is_the_runs", where, log[-2000:])
        self.assertNotIn("runtime_is_fresh", where)


def _succeeds():
    i = SPEC.index(BUMP_BLOCK_START)
    j = SPEC.index("succeedsIf := []", i)
    return (
        SPEC[:i]
        + SPEC[i:j]
        + "succeedsIf := [(.lit 1)]"
        + SPEC[j + len("succeedsIf := []") :]
    )


class Hook(unittest.TestCase):
    AST: typing.ClassVar[dict] = {
        "nodes": [
            {
                "nodeType": "ContractDefinition",
                "name": "Counter",
                "contractKind": "contract",
                "nodes": [
                    {
                        "nodeType": "FunctionDefinition",
                        "name": "bump",
                        "body": {"statements": [{"nodeType": "ForStatement"}]},
                    },
                    {
                        "nodeType": "FunctionDefinition",
                        "name": "max",
                        "body": {"statements": []},
                    },
                ],
            }
        ]
    }

    def test_loop_functions(self):
        self.assertEqual(L.loop_functions(self.AST, "Counter"), ["bump"])

    def test_no_checker_records_not_run(self):
        env = os.environ.pop("LOOP_ANY_CALLER_LEAN", None)
        try:
            rec = {}
            built = []
            r = L.from_driver(
                rec,
                "Counter.sol",
                self.AST,
                None,
                "/nonexistent",
                lambda: built.append(1),
                lambda: built.append(1),
                lambda _m: None,
            )
        finally:
            if env is not None:
                os.environ["LOOP_ANY_CALLER_LEAN"] = env
        self.assertEqual(r["status"], "not run")
        self.assertEqual(rec["loop_any_caller"], r)
        self.assertEqual(built, [])

    def test_no_profile_no_entry(self):
        rec = {}
        ast = {"nodes": [dict(self.AST["nodes"][0], name="Other")]}
        self.assertIsNone(
            L.from_driver(rec, "x.sol", ast, None, "/nonexistent", None, None, None)
        )
        self.assertEqual(rec, {})

    def test_build_failure_is_a_refusal(self):
        def boom():
            raise RuntimeError("cargo cannot build solar")

        rec = {}
        old = os.environ.get("LOOP_ANY_CALLER_LEAN")
        os.environ["LOOP_ANY_CALLER_LEAN"] = old or "false"
        try:
            with tempfile.TemporaryDirectory() as d:
                r = L.from_driver(
                    rec,
                    "Counter.sol",
                    self.AST,
                    "Counter",
                    d,
                    boom,
                    boom,
                    lambda _m: None,
                )
        finally:
            if old is None:
                del os.environ["LOOP_ANY_CALLER_LEAN"]
        self.assertEqual(r["status"], "refused")
        self.assertIn("cargo cannot build solar", r["why"])


@unittest.skipUnless(
    SOLAR and VERIFY, "set LOOP_ANY_CALLER_SOLAR and LOOP_ANY_CALLER_VERIFY"
)
class DefaultPath(unittest.TestCase):
    """certify on Counter.sol and its mutations, as the CLI and the driver run it."""

    def run_source(self, text, lean=LEAN, verify=VERIFY):
        with tempfile.TemporaryDirectory() as d:
            src = pathlib.Path(d) / "src" / "Counter.sol"
            src.parent.mkdir()
            src.write_text(text)
            try:
                rec = L.certify(
                    src,
                    "Counter",
                    solar=SOLAR,
                    verify=verify,
                    lean=lean,
                    work=pathlib.Path(d) / "w",
                )
                rec["text"] = pathlib.Path(rec["module"]).read_text()
                return rec
            except L.Refused as e:
                return e.record

    def mutate(self, old, new):
        assert old in SOURCE, old
        return SOURCE.replace(old, new, 1)

    def test_without_a_checker_nothing_is_certified(self):
        rec = self.run_source(SOURCE, lean=None)
        self.assertEqual(rec["status"], "refused")
        self.assertIn("no Lean checker", rec["why"])
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "Counter.sol"
            f.write_text(SOURCE)
            env = {k: v for k, v in os.environ.items() if k != "LOOP_ANY_CALLER_LEAN"}
            r = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "loop_any_caller.py"),
                    str(f),
                    "--contract",
                    "Counter",
                    "--solar",
                    SOLAR,
                    "--verify",
                    VERIFY,
                ],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )
        self.assertEqual(r.returncode, 4, r.stderr)

    def test_wrong_digest_exporter_refused_before_lean(self):
        # The review's control: solar-verify run with 64 zeroes for the bytes' digest.
        with tempfile.TemporaryDirectory() as d:
            w = pathlib.Path(d) / "verify-zeroes"
            w.write_text(
                f"#!{sys.executable} -I\nimport os, sys\na = sys.argv[1:]\n"
                "a[a.index('--lean-spec-bytecode-sha256') + 1] = '0' * 64\n"
                f"os.execv({VERIFY!r}, [{VERIFY!r}, *a])\n"
            )
            w.chmod(0o755)
            calls = pathlib.Path(d) / "calls"
            rec = self.run_source(SOURCE, lean=f"touch {calls}", verify=str(w))
            self.assertFalse(calls.exists())
        self.assertEqual(rec["status"], "refused")
        self.assertIn(f"bytecode with SHA-256 {ZEROS}, not the run's", rec["why"])
        self.assertNotIn("lean_rc", rec)

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_canonical_is_checked(self):
        rec = self.run_source(SOURCE)
        self.assertEqual(rec["status"], "checked", rec.get("why"))
        self.assertEqual(rec["route"], "no-sender-reference")
        if rec["runtime_sha256"] == hashlib.sha256(bytes.fromhex(HEX)).hexdigest():
            self.assertEqual(rec["text"], (DATA / "CounterCaller.lean").read_text())

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_ensures_false_refused(self):
        rec = self.run_source(
            self.mutate(
                "/// @custom:ensures count == old(count) + n",
                "/// @custom:ensures false",
            )
        )
        self.assertEqual(rec["status"], "refused")
        self.assertIn("bump_meets_from", rec["why"])
        self.assertTrue(rec["why"].startswith(L.EVERY_CALLER_FAILED), rec["why"])
        self.assertNotIn("caller-dependent", rec["why"])
        self.assertNotIn("witness", rec)

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_caller_dependent_refused(self):
        rec = self.run_source(
            self.mutate(
                "/// @custom:ensures count == old(count) + n",
                "/// @custom:ensures msg.sender == address(0x2000)",
            )
        )
        self.assertEqual(rec["status"], "refused")
        self.assertEqual(rec["route"], "sender-reference")
        self.assertTrue(rec["why"].startswith(L.CALLER_DEPENDENT), rec["why"])
        self.assertEqual(rec["witness"]["caller"], "0x2000")
        self.assertTrue(rec["witness"]["holds"])
        self.assertEqual(rec["witness"]["lean_rc"], 0)

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_false_with_sender_tautology_is_no_witness(self):
        # False for every caller: a sender reference alone never makes it caller-dependent.
        rec = self.run_source(
            self.mutate(
                "/// @custom:ensures count == old(count) + n",
                "/// @custom:ensures false && msg.sender == msg.sender",
            )
        )
        self.assertEqual(rec["status"], "refused")
        self.assertEqual(rec["route"], "sender-reference")
        self.assertTrue(rec["why"].startswith(L.NO_WITNESS), rec["why"])
        self.assertNotIn("caller-dependent", rec["why"])
        self.assertFalse(rec["witness"]["holds"])

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_other_bytes_refused(self):
        rec = self.run_source(self.mutate('"over"', '"over!"'))
        self.assertEqual(rec["status"], "refused")
        self.assertIn("runtime_is_fresh", rec["why"])

    @unittest.skipUnless(LEAN, "set LOOP_ANY_CALLER_LEAN to a Lean checker command")
    def test_sender_in_another_selector_still_checked(self):
        rec = self.run_source(
            self.mutate(
                "/// @custom:ensures count == old(count) + 1",
                "/// @custom:ensures count == old(count) + 1 && msg.sender != address(0)",
            )
        )
        self.assertEqual(rec["status"], "checked", rec.get("why"))
        self.assertEqual(rec["route"], "no-sender-reference")


if __name__ == "__main__":
    unittest.main()
