#!/usr/bin/env python3
"""Negative controls for tools/certify/generic.py that need no Lean: the strict predicate's source
binding and --replay's manifest check. Run: python3 tools/certify/test_generic.py"""

import argparse
import contextlib
import hashlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import typing
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

NAME, CONTRACT = "meter_ca0b7786", "Meter"
BOUND = (
    f"Certified/{NAME}/Source.lean is the front end's AST of {CONTRACT} in "
    f"Certified/{NAME}/{NAME}.sol (solc {generic.SOLC_VERSION})"
)


def full_record(source=BOUND, source_ast=BOUND):
    return {
        "name": NAME,
        "contract": CONTRACT,
        "final": {"statement": "x"},
        "source_ast": source_ast,
        "statuses": {
            "equivalence": "proved",
            "termination": "proved",
            "resources": "proved",
            "spec": "none",
            "source": source,
            "provenance": "clean",
            "strongest": False,
        },
        "replay": {
            "closure": 10,
            "replayed_count": 10,
            "trusted_count": 0,
            "unaccounted": [],
        },
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
            "not bound": (
                "not bound: the checked SolidCore source AST is not regenerated",
                None,
            ),
            "other contract": (
                BOUND.replace("AST of Meter", "AST of Gate"),
                BOUND.replace("AST of Meter", "AST of Gate"),
            ),
            "other file": (
                BOUND.replace("Source.lean", "Src.lean"),
                BOUND.replace("Source.lean", "Src.lean"),
            ),
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
        rec = {
            "status": "COMPILED",
            "source": {"sha256": files["Meter.sol"]},
            "run_files": files,
        }
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def replay(self, d):
        # --solidity-lean points nowhere: a run directory that passes the checks fails later, at
        # the fresh run, with a different message.
        p = subprocess.run(
            [
                sys.executable,
                str(HERE / "generic.py"),
                "--replay",
                str(d),
                "--solidity-lean",
                "/nonexistent",
                "--work",
                str(d.parent / "w"),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        return p.returncode, p.stdout.strip().splitlines()[
            -1
        ] if p.stdout.strip() else ""

    def test_stray_outside_inputs(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "stray.txt").write_text("x\n")
            code, line = self.replay(d)
            self.assertEqual(code, 1)
            self.assertIn(
                "REJECTED: replay: files the record's manifest does not list: ['stray.txt']",
                line,
            )

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
            _code, line = self.replay(d)
            self.assertNotIn("differs from the record", line)
            self.assertNotIn("manifest does not list", line)


def git_init(d, files):
    d.mkdir(parents=True, exist_ok=True)
    for rel, text in files.items():
        (d / rel).parent.mkdir(parents=True, exist_ok=True)
        (d / rel).write_text(text)
    g = [
        "git",
        "-C",
        str(d),
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
    ]
    subprocess.run([*g[:3], "init", "-q"], check=True)
    subprocess.run([*g, "add", "-A"], check=True)
    subprocess.run([*g, "commit", "-q", "-m", "base"], check=True)
    return subprocess.run(
        ["git", "-C", str(d), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def lean_fixture(root):
    """A solidity-lean base commit that pins one git package, and a package cache holding that
    package at its pinned revision with one built olean."""
    cache = root / "cache"
    pkg = cache / ".lake/packages/Pkg"
    rev = git_init(
        pkg,
        {
            "Pkg/Foo.lean": "def foo := 1\n",
            "Pkg/Bar.lean": "def bar := 2\n",
            ".gitignore": ".lake\n",
        },
    )
    (pkg / ".lake/build/lib/lean/Pkg").mkdir(parents=True)
    (pkg / ".lake/build/lib/lean/Pkg/Foo.olean").write_bytes(b"olean foo")
    manifest = json.dumps(
        {
            "version": "1.1.0",
            "packages": [
                {"name": "Pkg", "type": "git", "rev": rev},
                {"name": "local", "type": "path", "rev": None},
            ],
        }
    )
    repo = root / "sl"
    base = git_init(
        repo,
        {
            "lakefile.lean": "-- lakefile\n",
            "lean-toolchain": "leanprover/lean4:v4.35.0\n",
            "lake-manifest.json": manifest,
            "SolidCore/A.lean": "-- a\n",
        },
    )
    return repo, base, cache


def claims_record():
    """A certified run's certify-record.json (scripts/certify's record) and the driver's record.json."""
    cr = full_record()
    cr["statuses"].update(
        spec="proved", strongest=True, excluded=["deployment", "events"]
    )
    cr["final"] = {
        "theorem": f"Certified.{NAME}.Final.final",
        "statement": "code = runtime ∧ Expected src spec runtime",
        "shape": "SolidCore.Verify.Generic.Expected",
        "assumptions": [],
        "excluded": ["deployment", "events"],
        "nonvacuous": f"Certified.{NAME}.Final.NonVacuous",
    }
    cr["replay"].update(
        replayed=[{"module": f"M{i}", "olean_sha256": f"{i:064x}"} for i in range(10)],
        trusted={},
        replayed_beyond_plan=[],
        checker={"path": "/x/leanchecker", "sha256": "c" * 64},
    )
    cr["caveats"] = []
    cr["end_to_end_statement"] = {}
    ok, need_c, comps = generic.strict(cr, "certified")
    rec = {
        "name": NAME,
        "contract": CONTRACT,
        "requested_tier": "certified",
        "status": "CERTIFIED",
        "strict": {"tier": "certified", "holds": ok, "components": need_c},
        "tier_reached": generic.reached_tier(comps),
        "published_statement": cr["end_to_end_statement"],
        **{k: json.loads(json.dumps(cr[k])) for k in generic.CLAIM_KEYS},
    }
    return cr, rec


class ReplayClaims(unittest.TestCase):
    """record.json is not hashed: --replay must hold its claims to certify-record.json and the fresh run."""

    def make(self, root, edit=None):
        cr, rec = claims_record()
        self.repo, base, self.cache = lean_fixture(root)
        d = root / "run"
        d.mkdir()
        man = generic.cache_manifest(self.cache / ".lake/packages")
        (d / "packages-cache.sha256").write_text(man)
        (d / "Meter.sol").write_text("contract Meter {}\n")
        (d / "certify.log").write_text("log\n")
        (d / "certify-record.json").write_text(json.dumps(cr))
        (d / "inputs/Certified" / NAME).mkdir(parents=True)
        (d / "inputs/Certified" / NAME / "Source.lean").write_text("-- generated\n")
        (d / "inputs/lakefile.lean").write_text("-- lakefile\n")
        files = {str(q.relative_to(d)): sha(q) for q in d.rglob("*") if q.is_file()}
        ins = {
            r[len("inputs/") :]: v for r, v in files.items() if r.startswith("inputs/")
        }
        pin, digest = generic.pinned_packages(self.repo, base)
        rec.update(
            source={"sha256": files["Meter.sol"]},
            run_files=files,
            solidity_lean={
                "base": base,
                "base_tree": generic.git(self.repo, "rev-parse", base + "^{tree}"),
                "executed_tree": generic.inputs_tree(
                    self.repo, base, d / "inputs", sorted(ins)
                ),
                "inputs_commit": "a" * 40,
                "inputs": sorted(ins),
                "inputs_sha256": ins,
            },
            packages={
                "pinned": pin,
                "digest": digest,
                "cache_sha256": hashlib.sha256(man.encode()).hexdigest(),
            },
        )
        rec["caveats"] = cr["caveats"] = [
            f"solidity-lean at {'a' * 40}, not the pinned {'b' * 40}"
        ]
        (d / "certify-record.json").write_text(json.dumps(cr))
        files["certify-record.json"] = sha(d / "certify-record.json")
        if edit:
            edit(rec)
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def replay(self, d, fresh_edit=None, packages_from=True):
        """replay_main in this process, its fresh run replaced by one that writes the record's own
        claims back (edited by fresh_edit): the checks run, no Lean does."""
        orig = json.loads((d / "record.json").read_text())

        def fake_run(cmd, *args, **kw):
            if "--record-dir" not in cmd:
                # git and the checks' own commands run.
                return saved(cmd, *args, **kw)
            fresh = json.loads(json.dumps(orig))
            if fresh_edit:
                fresh_edit(fresh)
            out = pathlib.Path(cmd[cmd.index("--record-dir") + 1])
            out.mkdir(parents=True)
            (out / "record.json").write_text(json.dumps(fresh))
            return subprocess.CompletedProcess(cmd, 0)

        a = argparse.Namespace(
            replay=str(d),
            work=str(d.parent / "w"),
            solidity_lean=str(self.repo),
            packages_from=str(self.cache) if packages_from else None,
            probe_cache=None,
        )
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
            "final.theorem renamed": (
                lambda r: r["final"].update(theorem="Certified.other.Final.final"),
                "final.theorem",
            ),
            "final.statement edited": (
                lambda r: r["final"].update(statement="True"),
                "final.statement",
            ),
            "replay.closure raised": (
                lambda r: r["replay"].update(closure=11),
                "replay.closure",
            ),
            "replayed module dropped": (drop_module, "replay.replayed"),
            "excluded emptied": (empty_excluded, "final.excluded"),
            "statuses.excluded emptied": (
                lambda r: r["statuses"].update(excluded=[]),
                "statuses.excluded",
            ),
            "caveats emptied": (lambda r: r.update(caveats=None), "caveats"),
            "strict holds forged": (
                lambda r: r["strict"].update(holds=False),
                "strict.holds",
            ),
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
            "replay.closure": lambda r: r["replay"].update(
                closure=11, replayed_count=11
            ),
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
            self.assertIn(
                f"REJECTED: replay: inputs/Certified/{NAME}/Source.lean is not the generated input",
                line,
            )

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
            self.assertIn(
                "REJECTED: replay: inputs/Extra.lean is not the generated input", line
            )

    def test_fresh_caveats_differ(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(
                self.make(pathlib.Path(t)), lambda r: r.update(caveats=[])
            )
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


class ExecutedTree(unittest.TestCase):
    """--replay holds the run to the solidity-lean tree it executed (the base plus the listed
    generated inputs, rebuilt) and to the package cache it started from, not only to the nominal
    keys."""

    make = ReplayClaims.make
    replay = ReplayClaims.replay

    def rewrite(self, d, fn):
        rec = json.loads((d / "record.json").read_text())
        fn(rec, d)
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def rejected(self, d, needle, **kw):
        code, line = self.replay(d, **kw)
        self.assertEqual(code, 1, line)
        self.assertTrue(line.startswith("REJECTED: replay: "), line)
        self.assertIn(needle, line)
        return line

    def test_inputs_tree_is_the_input_commit(self):
        """The rebuilt tree is git's own: commit the inputs on the base and compare trees."""
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            repo, base, _ = lean_fixture(root)
            ins = root / "ins"
            (ins / "Certified/x").mkdir(parents=True)
            (ins / "Certified/x/S.lean").write_text("-- s\n")
            (ins / "lakefile.lean").write_text("-- lakefile\nlean_lib x\n")
            for q in ("Certified/x/S.lean", "lakefile.lean"):
                (repo / q).parent.mkdir(parents=True, exist_ok=True)
                (repo / q).write_bytes((ins / q).read_bytes())
            g = [
                "git",
                "-C",
                str(repo),
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ]
            subprocess.run([*g, "add", "-A"], check=True)
            subprocess.run([*g, "commit", "-q", "-m", "inputs"], check=True)
            want = generic.git(repo, "rev-parse", "HEAD^{tree}")
            objs = sorted((repo / ".git/objects").rglob("*"))
            self.assertEqual(
                generic.inputs_tree(
                    repo, base, ins, ["Certified/x/S.lean", "lakefile.lean"]
                ),
                want,
            )
            self.assertEqual(
                sorted((repo / ".git/objects").rglob("*")),
                objs,
                "inputs_tree wrote into the repository",
            )
            self.assertNotEqual(
                generic.inputs_tree(repo, base, ins, ["Certified/x/S.lean"]), want
            )

    def test_control_passes(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(self.make(pathlib.Path(t)))
            self.assertEqual(code, 0, line)

    def test_tampered_generated_file(self):
        """A generated input edited with every digest the record holds updated to match."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            q = f"Certified/{NAME}/Source.lean"
            (d / "inputs" / q).write_text("-- edited\n")

            def rehash(rec, d):
                rec["run_files"]["inputs/" + q] = sha(d / "inputs" / q)
                rec["solidity_lean"]["inputs_sha256"][q] = sha(d / "inputs" / q)

            self.rewrite(d, rehash)
            self.rejected(
                d,
                "is not base",
                fresh_edit=lambda r: r["solidity_lean"]["inputs_sha256"].update(
                    {q: sha(d / "inputs" / q)}
                ),
            )

    def test_unlisted_file_in_executed_tree(self):
        """A record whose executed tree holds a file beyond the listed inputs (an edited base file)."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            extra = pathlib.Path(t) / "extra"
            (extra / "SolidCore").mkdir(parents=True)
            (extra / "SolidCore/A.lean").write_text("-- edited base\n")
            for q in json.loads((d / "record.json").read_text())["solidity_lean"][
                "inputs"
            ]:
                (extra / q).parent.mkdir(parents=True, exist_ok=True)
                (extra / q).write_bytes((d / "inputs" / q).read_bytes())

            def forge(rec, d):
                sl = rec["solidity_lean"]
                sl["executed_tree"] = generic.inputs_tree(
                    self.repo, sl["base"], extra, [*sl["inputs"], "SolidCore/A.lean"]
                )

            self.rewrite(d, forge)
            self.rejected(d, "is not base")

    def test_no_executed_tree(self):
        """A record from before this check (such as rc2-acc5-siblingok's) is refused, not passed on its keys."""
        with tempfile.TemporaryDirectory() as t:
            d = self.rewrite(
                self.make(pathlib.Path(t)),
                lambda r, d: r["solidity_lean"].pop("executed_tree"),
            )
            self.rejected(d, "pins no executed tree")

    def test_base_tree_forged(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.rewrite(
                self.make(pathlib.Path(t)),
                lambda r, d: r["solidity_lean"].update(base_tree="0" * 40),
            )
            self.rejected(d, "base_tree")

    def test_package_set_not_pinned(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.rewrite(
                self.make(pathlib.Path(t)),
                lambda r, d: r["packages"]["pinned"]["packages"][0].__setitem__(
                    1, "f" * 40
                ),
            )
            self.rejected(d, "pinned toolchain and packages")

    def test_cache_olean_added(self):
        """An olean added to the cache after the run, of a module the pinned package has (H1's
        24 EthLib oleans are this case): only the cache manifest sees it."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (
                self.cache / ".lake/packages/Pkg/.lake/build/lib/lean/Pkg/Bar.olean"
            ).write_bytes(b"olean bar")
            line = self.rejected(
                d,
                "the --packages-from cache is not the run's: 1 added, 0 removed, 0 changed",
            )
            self.assertIn("Pkg/.lake/build/lib/lean/Pkg/Bar.olean", line)

    def test_cache_olean_changed(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (
                self.cache / ".lake/packages/Pkg/.lake/build/lib/lean/Pkg/Foo.olean"
            ).write_bytes(b"other")
            self.rejected(d, "0 added, 0 removed, 1 changed")

    def test_cache_foreign_olean(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (
                self.cache / ".lake/packages/Pkg/.lake/build/lib/lean/Pkg/Evil.olean"
            ).write_bytes(b"x")
            self.rejected(d, "have no source at their package's pinned revision")

    def test_cache_package_other_revision(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            pkg = self.cache / ".lake/packages/Pkg"
            (pkg / "Pkg/Foo.lean").write_text("def foo := 3\n")
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(pkg),
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qam",
                    "other",
                ],
                check=True,
            )
            self.rejected(d, "not the pinned")

    def test_cache_package_edited(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (self.cache / ".lake/packages/Pkg/Pkg/Foo.lean").write_text(
                "def foo := 9\n"
            )
            self.rejected(d, "edited or untracked")

    def test_cache_extra_package(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            git_init(self.cache / ".lake/packages/Other", {"O.lean": "-- o\n"})
            self.rejected(d, "not the pinned set")

    def test_cache_manifest_file_edited(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            m = d / "packages-cache.sha256"
            m.write_text(m.read_text() + "0" * 64 + "  ./Pkg/x\n")
            self.rewrite(
                d, lambda r, d: r["run_files"].update({"packages-cache.sha256": sha(m)})
            )
            self.rejected(d, "not the manifest packages.cache_sha256 pins")

    def test_record_without_cache_manifest(self):
        """A record that pins no cache manifest (H1's) is refused under --packages-from, never
        passed on its nominal keys."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            (d / "packages-cache.sha256").unlink()

            def drop(rec, d):
                rec["run_files"].pop("packages-cache.sha256")
                rec["packages"]["cache_sha256"] = None

            self.rewrite(d, drop)
            self.rejected(
                d,
                "pins no package cache manifest",
                fresh_edit=lambda r: r["packages"].update(cache_sha256=None),
            )

    def test_cache_run_replayed_without_cache(self):
        with tempfile.TemporaryDirectory() as t:
            self.rejected(
                self.make(pathlib.Path(t)),
                "replay with --packages-from",
                packages_from=False,
            )

    def test_fresh_cache_digest_differs(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(
                self.make(pathlib.Path(t)),
                lambda r: r["packages"].update(cache_sha256="e" * 64),
            )
            self.assertEqual(code, 1, line)
            self.assertIn("packages", line)


SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"


def run_driver(src_text, spec, root):
    """The driver up to its first refusal; its envelope and spec checks run before any build."""
    src = root / "C.sol"
    src.write_text(src_text)
    p = subprocess.run(
        [
            sys.executable,
            str(HERE / "generic.py"),
            str(src),
            spec,
            "--solidity-lean",
            "/nonexistent",
            "--work",
            str(root / "w"),
            "--record-dir",
            str(root / "r"),
            "--solc",
            str(SOLC),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    return p.returncode, (p.stdout.strip().splitlines() or [""])[-1]


HEAD = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class SpecGates(unittest.TestCase):
    def test_environment_clause_blocked(self):
        src = (
            HEAD
            + """contract C { uint256 public x;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:requires block.chainid == 424242
    /// @custom:ensures x == 8
    function f() external { x = 1; } }"""
        )
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertEqual(code, 3)
            self.assertTrue(line.startswith("BLOCKED: environment"), line)

    def test_msg_value_clause_allowed(self):
        src = (
            HEAD
            + """contract C { uint256 public x;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures x == 1
    function f() external { x = 1; } }"""
        )
        with tempfile.TemporaryDirectory() as t:
            _code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertNotIn("BLOCKED", line)
            self.assertNotIn("succeeds-if", line)

    def test_no_succeeds_if_refused(self):
        src = (
            HEAD
            + """contract C { uint256 public x;
    /// @custom:ensures x == 1
    function f() external { x = 1; } }"""
        )
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(src, "natspec", pathlib.Path(t))
            self.assertEqual(code, 1)
            self.assertIn("REJECTED: spec: no function has @custom:succeeds-if", line)

    def test_spec_none_needs_no_succeeds_if(self):
        src = (
            HEAD
            + """contract C { uint256 public x;
    function f() external { x = 1; } }"""
        )
        with tempfile.TemporaryDirectory() as t:
            _code, line = run_driver(src, "none", pathlib.Path(t))
            self.assertNotIn("succeeds-if", line)


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class RecursionBoundary(unittest.TestCase):
    """Recursion is outside the envelope: the driver refuses it as BLOCKED (exit 3) before any build."""

    def blocked(self, body):
        with tempfile.TemporaryDirectory() as t:
            return run_driver(
                HEAD + "contract C {\n" + body + "\n}", "none", pathlib.Path(t)
            )

    def test_self_recursion(self):
        code, line = self.blocked("""    uint256 public x;
    function decrement(uint256 n) public { if (n > 0) { x = n; decrement(n - 1); } }""")
        self.assertEqual(code, 3, line)
        self.assertIn(
            "BLOCKED: recursion (1x, first line 5: unsupported: decrement can call itself",
            line,
        )

    def test_mutual_recursion(self):
        code, line = (
            self.blocked("""    function even(uint256 n) internal pure returns (bool) { return n == 0 ? true : odd(n - 1); }
    function odd(uint256 n) internal pure returns (bool) { return n == 0 ? false : even(n - 1); }
    function f(uint256 n) external pure returns (bool) { return even(n); }""")
        )
        self.assertEqual(code, 3, line)
        self.assertIn(
            "BLOCKED: recursion (1x, first line 4: unsupported: even and odd can call each other",
            line,
        )

    def test_recursion_through_a_modifier(self):
        code, line = self.blocked("""    uint256 public x;
    modifier m() { g(); _; }
    function g() internal { x += 1; h(); }
    function h() internal m { x += 2; }
    function f() external { g(); }""")
        self.assertEqual(code, 3, line)
        self.assertIn("unsupported: g and h and m can call each other", line)

    def test_call_through_a_function_value(self):
        code, line = (
            self.blocked("""    function g(uint256 n) internal pure returns (uint256) { return n + 1; }
    function f(uint256 n) external pure returns (uint256) {
        function (uint256) internal pure returns (uint256) h = g;
        return h(n);
    }""")
        )
        self.assertEqual(code, 3, line)
        self.assertIn("unsupported: a call through an internal function value", line)

    def test_acyclic_calls_pass(self):
        _code, line = self.blocked("""    uint256 public x;
    modifier m() { require(x < 10); _; }
    function g(uint256 n) internal pure returns (uint256) { return n + 1; }
    function h(uint256 n) internal pure returns (uint256) { return g(g(n)); }
    function f(uint256 n) external m { x = h(n) + uint256(keccak256(abi.encode(n))) % 2; }""")
        self.assertNotIn("recursion", line)
        self.assertNotIn("BLOCKED", line)


def final_py(shape, root, hexcode="6001"):
    sp = root / "shape.json"
    sp.write_text(json.dumps(shape))
    p = subprocess.run(
        [
            sys.executable,
            str(HERE / "final.py"),
            str(sp),
            "--name",
            "c_1",
            "--contract",
            "C",
            "--hex",
            hexcode,
            "--out",
            str(root / "F.lean"),
            "--statement-out",
            str(root / "S.lean"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    return (
        p.returncode,
        p.stderr.strip(),
        (root / "S.lean").read_text() if (root / "S.lean").exists() else "",
    )


def expected_shape(**over):
    s = {
        "kind": "certified",
        "statement": "expected",
        "contract": "L.contract?",
        "fuel": "L.fuel",
        "valid": "V",
        "allowed_export": True,
        "spec_export": {"ctx": "X.ctxOfCall", "step": "X.idStep"},
        "nonvacuous": "P.nonvacuous",
        "gas": "fun _ => 7",
        "env": [],
        "spec": "s",
        "spec_modules": ["L"],
        "proof_module": "P",
        "proof": "P.published",
        "source_unit": "Src.importedSourceUnit",
        "source_contract": "C",
        "storage": "L.storageLit",
        "entries": [{"signature": "f()", "fn": "L.fn0", "sel": 1, "arity": 0}],
    }
    s.update(over)
    return s


class ExpectedStatement(unittest.TestCase):
    def test_writes_expected(self):
        with tempfile.TemporaryDirectory() as t:
            code, err, stmt = final_py(expected_shape(), pathlib.Path(t))
            self.assertEqual(code, 0, err)
            self.assertIn(
                "_root_.SolidCore.Verify.Generic.Expected _root_.Certified.c_1.Final.src",
                stmt,
            )
            self.assertIn(
                "Contexts ctx := ctx = _root_.SolidCore.Verify.Generic.Context.default",
                stmt,
            )
            self.assertIn("assumptions := []", stmt)

    def test_refused(self):
        cases = {
            "assumptions": expected_shape(env=["True"]),
            "no non-vacuity": {
                k: v for k, v in expected_shape().items() if k != "nonvacuous"
            },
            "compiled kind": expected_shape(kind="compiled", allowed_export=None),
            "post not from the export": {
                **{k: v for k, v in expected_shape().items() if k != "spec_export"},
                "post": "Q",
            },
            "no entries": expected_shape(entries=[]),
            "tactic in an input": expected_shape(gas="by decide"),
            "other statement": expected_shape(statement="shapes"),
        }
        for label, shape in cases.items():
            with self.subTest(label), tempfile.TemporaryDirectory() as t:
                code, _err, _ = final_py(
                    {k: v for k, v in shape.items() if v is not None},
                    pathlib.Path(t),
                )
                self.assertNotEqual(code, 0, label)


class SpecConnectEmission(unittest.TestCase):
    """A succeeds-if over the call's environment alone leaves some of tree_succeeds's steps
    nothing to rewrite; the emitted proof must allow that, not call the macro."""

    def test_tolerant_succeeds_proof(self):
        import spec_connect

        lines = spec_connect.succeeds_block(
            "set",
            1,
            "",
            "  tree_meets_fixed [set_tree, set_spec,]",
            [],
            [],
            "TreeMeetsFixed",
            "M",
            "B",
        )
        text = "\n".join(lines)
        self.assertNotIn("tree_succeeds [", text)
        self.assertIn("try simp only [set_tree, set_spec, M.spec, B, M.ctx, *", text)
        self.assertIn("first | omega | (simp_all; done) | (simp_all; omega)", text)
        self.assertNotIn("@DS@", text)


class ResourceFailures(unittest.TestCase):
    """A kernel that runs out of memory is reported as resources, never as a refuted proof."""

    def test_kernel_memory(self):
        log = [
            "X.lean:97:4: (kernel) excessive memory consumption detected",
            "error: build failed",
        ]
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
            cmd = [
                "bash",
                "-c",
                'echo "neg: refused ($(cut -c1-160 "$1"))"; exit 3',
                "-",
                str(src),
            ]
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
        r = subprocess.run(
            ["bash", "-c", fn + "\nclip"],
            input=line.encode(),
            capture_output=True,
            check=False,
        )
        self.assertEqual(r.returncode, 0)
        self.assertEqual(r.stdout.decode("utf-8"), line.encode()[:159].decode())
        r = subprocess.run(
            ["bash", "-c", fn + "\nclip"],
            input=b"x" * 200,
            capture_output=True,
            check=False,
        )
        self.assertEqual(r.stdout, b"x" * 160)
        self.assertNotIn("cut -c1-160", src)


class SpecConnectCheck(unittest.TestCase):
    """spec_connect.py names the theorem a false spec fails in, before anything says connected."""

    def test_failing_theorem_named(self):
        import spec_connect

        out = [
            "namespace M",
            "theorem clear_meets0 : P := by",
            "  first",
            "  | (a)",
            "  | (b)",
            "",
            "theorem clear_succeeds : Q := by",
            "  omega",
        ]
        text = (
            "SpecConnect.lean:5:5: error: omega could not prove the goal:\n"
            "SpecConnect.lean:8:2: error: omega could not prove the goal:\n"
        )
        self.assertEqual(
            spec_connect.failing_theorems(text, out),
            [
                ("clear_meets0", 5, "omega could not prove the goal:"),
                ("clear_succeeds", 8, "omega could not prove the goal:"),
            ],
        )


class SpecRejectionPath(unittest.TestCase):
    """A rejected spec ends with spec_connect's REJECTED line, and scripts/certify's guard on the
    captured output refuses cleanly even when the capture is empty (no bad array subscript)."""

    def test_rejected_line_is_the_verdict(self):
        log = [
            "spec_connect: REJECTED: spec: set_meets0 does not check: omega could not prove the goal: (SpecConnect.lean:67)",
            "certify: error: spec_connect.py refuses functions of C (above): their spec is not proved",
        ]
        self.assertEqual(
            generic.spec_rejection(log),
            [
                "REJECTED: spec: set_meets0 does not check: omega could not prove the goal: (SpecConnect.lean:67)"
            ],
        )

    def test_guard_on_empty_capture(self):
        script = (HERE.parent.parent / "scripts" / "certify").read_text()
        guard = next(
            l.strip()
            for l in script.splitlines()
            if "spec_connect.py refuses functions of" in l and "exit 0" in l
        )
        for capture in ("true", "echo 'exit 1'"):
            prog = (
                'set -euo pipefail; contract=C; die() { echo "certify: error: $*" >&2; exit 1; }\n'
                f"mapfile -t holds < <({capture})\n{guard}\necho passed\n"
            )
            p = subprocess.run(
                ["bash", "-c", prog], capture_output=True, text=True, check=False
            )
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
        solc = os.environ.get(
            "SOLC", str(pathlib.Path.home() / ".cache/solc/solc-0.8.35")
        )
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
                rc, body, err = generic.frontend(
                    lean, p, sha, "counter_x", "Counter", solc
                )
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
    "ImmutableCtor": "346027575f3560e01c80636d4ce63c14602b578063a4d66daf14602b578063a56dfe4a146053575b5f5ffd5b7f"
    + "00" * 32
    + "5f5260205ff35b5f545f5260205ff3",
}


def life_probe(
    example,
    storage,
    runtime=None,
    ethlib_storage=None,
    ethlib="success true",
    constructor="returns",
    logs="0",
):
    """A lifecycle probe result as run_lifecycle_probe parses it (PROBE lines)."""
    pairs = lambda xs: "[" + ", ".join(f"({k}, {v})" for k, v in xs) + "]"
    return {
        "typecheck": "ok",
        "constructor": constructor,
        "storage": pairs(storage),
        "ethlib": ethlib,
        "runtime": LIFE_RUNTIME[example] if runtime is None else runtime,
        "ethlib storage": pairs(storage if ethlib_storage is None else ethlib_storage),
        "ethlib logs": logs,
    }


# What the real probe printed for the three examples with solar 0479b90a and solidity-lean 49d1d7f5:
# NoArgCtor sets slot 0 to 7; ArgCtor(42, 0xbeef) sets slots 0 and 1; ImmutableCtor(1000) sets
# slot 0 to 1001 and fills the immutable's PUSH32 placeholder with 1000.
REAL = {
    "NoArgCtor": ("", life_probe("NoArgCtor", [(0, 7)])),
    "ArgCtor": ("42,0xbeef", life_probe("ArgCtor", [(0, 42), (1, 0xBEEF)])),
    "ImmutableCtor": (
        "1000",
        life_probe(
            "ImmutableCtor",
            [(0, 1001)],
            runtime=LIFE_RUNTIME["ImmutableCtor"].replace(
                "7f" + "00" * 32, "7f" + (1000).to_bytes(32, "big").hex()
            ),
        ),
    ),
}


class LifecycleHelpers(unittest.TestCase):
    """The lifecycle tier's argument, runtime and storage checks."""

    def test_words(self):
        self.assertEqual(generic.constructor_words([], ""), [])
        self.assertEqual(
            generic.constructor_words(["uint256", "address"], "42, 0xbeef"),
            [42, 0xBEEF],
        )
        self.assertEqual(
            generic.constructor_words(["bool", "uint8"], "1,255"), [1, 255]
        )

    def test_words_refused(self):
        for types, text, needle in [
            (["uint256", "address"], "42", "takes 2 argument"),
            ([], "1", "takes 0 argument"),
            (["address"], "0x" + "1" * 41, "outside address"),
            (["uint8"], "256", "outside uint8"),
            (["bool"], "2", "outside bool"),
            (["uint256"], "-1", "outside uint256"),
            (["int256"], "1", "static uintN, address and bool"),
            (["string"], "1", "static uintN"),
            (["uint256"], "x", "not a number"),
        ]:
            with self.assertRaises(generic.Blocked) as e:
                generic.constructor_words(types, text)
            self.assertIn(needle, str(e.exception.reasons), (types, text))

    def test_runtime(self):
        rt = LIFE_RUNTIME["ImmutableCtor"]
        filled = REAL["ImmutableCtor"][1]["runtime"]
        self.assertIsNone(generic.runtime_problem(rt, rt, []))
        self.assertIsNone(generic.runtime_problem(filled, rt, ["limit"]))
        self.assertIn(
            "not solar's bin-runtime", generic.runtime_problem(filled, rt, [])
        )
        self.assertIn(
            "outside an immutable placeholder",
            generic.runtime_problem("ff" + rt[2:], rt, ["limit"]),
        )
        self.assertIn(
            "solar's bin-runtime has",
            generic.runtime_problem(filled + "00", rt, ["limit"]),
        )

    def test_storage_pairs(self):
        self.assertEqual(
            generic.storage_pairs("[(1, 48879), (0, 42)]"), [(0, 42), (1, 48879)]
        )
        self.assertEqual(generic.storage_pairs("[]"), [])


class LifecycleStages(unittest.TestCase):
    """The observed stages before the kernel: all agree for the real examples, else the first that
    disagrees is the run's REJECTED stage."""

    def first(self, example, res, imm=()):
        st = generic.lifecycle_stages(res, LIFE_RUNTIME[example], list(imm))
        self.assertEqual(
            [q["stage"] for q in st], ["source", "initcode", "runtime bytes", "storage"]
        )
        return next((q for q in st if not q["ok"]), {"stage": "kernel"})

    def test_real_examples_agree(self):
        for ex, (_, res) in REAL.items():
            self.assertEqual(
                self.first(ex, res, ["limit"] if ex == "ImmutableCtor" else [])[
                    "stage"
                ],
                "kernel",
                ex,
            )

    def test_immutable_without_immutables(self):
        self.assertEqual(
            self.first("ImmutableCtor", REAL["ImmutableCtor"][1])["stage"],
            "runtime bytes",
        )

    def test_storage_disagrees(self):
        res = life_probe(
            "ArgCtor", [(0, 42), (1, 0xBEEF)], ethlib_storage=[(0, 42), (1, 0)]
        )
        q = self.first("ArgCtor", res)
        self.assertEqual(q["stage"], "storage")
        self.assertIn("EthLib's run", q["detail"])

    def test_init_reverts(self):
        self.assertEqual(
            self.first("ArgCtor", life_probe("ArgCtor", [], ethlib="success false"))[
                "stage"
            ],
            "initcode",
        )
        self.assertEqual(
            self.first("ArgCtor", life_probe("ArgCtor", [], ethlib="none"))["stage"],
            "initcode",
        )

    def test_constructor_fails_or_logs(self):
        self.assertEqual(
            self.first("ArgCtor", life_probe("ArgCtor", [], constructor="undefined"))[
                "stage"
            ],
            "storage",
        )
        self.assertEqual(
            self.first(
                "ArgCtor",
                life_probe("ArgCtor", [(0, 1)], constructor="returns, with logs"),
            )["stage"],
            "storage",
        )
        self.assertEqual(
            self.first("ArgCtor", life_probe("ArgCtor", [(0, 1)], logs="1"))["stage"],
            "storage",
        )

    def test_source_rejected(self):
        self.assertEqual(
            self.first(
                "NoArgCtor", dict(life_probe("NoArgCtor", [(0, 7)]), typecheck="error")
            )["stage"],
            "source",
        )


class LifecycleRun(unittest.TestCase):
    """lifecycle_run end to end with solar, git, the front end and the probe replaced: it exits 4
    with UNSUPPORTED-lifecycle when every observed stage agrees but the kernel stage has nothing to
    prove (a base without the generic creation frame, a constructor outside its shape), 1 at a
    stage that disagrees or a creation the kernel refutes, and 0 only when the kernel builds the
    generated lifecycle module."""

    def drive(self, example, args, res, ast=None, bins=None, frame=False, build=None):
        src = LIFE / f"{example}.sol"
        if bins is None:
            bins = {
                "bin": "6080" + LIFE_RUNTIME[example],
                "bin-runtime": LIFE_RUNTIME[example],
            }
        if ast is None:
            ast = {
                "nodes": [
                    {"nodeType": "ContractDefinition", "name": example, "nodes": []}
                ]
            }
        saved = (
            generic.run,
            generic.git,
            generic.frontend,
            generic.run_lifecycle_probe,
            generic.log,
            generic.lifecycle_build,
        )
        seen = {}

        def fake_git(cwd, *args, **kw):
            # The clone of a base with the generic creation frame, when FRAME.
            if frame and args[:1] == ("checkout",):
                (pathlib.Path(cwd) / "Verify" / "Lifecycle").mkdir(
                    parents=True, exist_ok=True
                )
                (
                    pathlib.Path(cwd) / "Verify" / "Lifecycle" / "CreationFrame.lean"
                ).write_text("")
            return "b" * 40

        def fake_build(lean_dir, name, source_text, mod, text, cache):
            seen.setdefault("built", []).append(mod)
            return (build or (lambda mod, text: (False, "error: no Lean here")))(
                mod, text
            )

        def fake_run(cmd, *a, **kw):
            return 0, json.dumps({"contracts": {f"{src}:{example}": bins}}), ""

        def fake_probe(
            lean_dir, work, name, contract, source_text, creation, ws, cache
        ):
            seen["creation"], seen["ws"] = creation, ws
            return res

        def finish(status, code, line, **extra):
            seen.update(status=status, code=code, line=line)
            raise SystemExit(code)

        generic.run, generic.git, generic.lifecycle_build = (
            fake_run,
            fake_git,
            fake_build,
        )
        generic.frontend = lambda *a, **kw: (0, "-- body", "")
        generic.run_lifecycle_probe, generic.log = fake_probe, (lambda *a, **kw: None)
        record = {}
        try:
            with tempfile.TemporaryDirectory() as t, self.assertRaises(SystemExit):
                a = argparse.Namespace(
                    constructor_args=args,
                    solidity_lean=t,
                    base="b" * 40,
                    solc="solc",
                    probe_cache=None,
                )
                rec_dir = pathlib.Path(t) / "rec"
                rec_dir.mkdir()
                generic.lifecycle_run(
                    a,
                    record,
                    "solar",
                    src,
                    "0" * 64,
                    "x",
                    example,
                    ast,
                    pathlib.Path(t) / "w",
                    finish,
                    [],
                    rec_dir,
                )
                seen["rec_files"] = sorted(q.name for q in rec_dir.iterdir())
        finally:
            (
                generic.run,
                generic.git,
                generic.frontend,
                generic.run_lifecycle_probe,
                generic.log,
                generic.lifecycle_build,
            ) = saved
        return seen, record

    def ctor_ast(self, example, types, imm=()):
        params = [{"typeDescriptions": {"typeString": t}} for t in types]
        nodes = [
            {
                "nodeType": "FunctionDefinition",
                "kind": "constructor",
                "parameters": {"parameters": params},
            }
        ]
        nodes += [
            {"nodeType": "VariableDeclaration", "name": n, "mutability": "immutable"}
            for n in imm
        ]
        return {
            "nodes": [
                {"nodeType": "ContractDefinition", "name": example, "nodes": nodes}
            ]
        }

    def test_no_arg_constructor(self):
        seen, rec = self.drive(
            "NoArgCtor", None, REAL["NoArgCtor"][1], self.ctor_ast("NoArgCtor", [])
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertTrue(seen["line"].startswith("UNSUPPORTED-lifecycle: kernel: "))
        self.assertEqual(seen["ws"], [])
        self.assertEqual(rec["lifecycle"]["storage"], [(0, 7)])

    def test_constructor_with_args(self):
        seen, rec = self.drive(
            "ArgCtor",
            "42,0xbeef",
            REAL["ArgCtor"][1],
            self.ctor_ast("ArgCtor", ["uint256", "address"]),
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        init = bytes.fromhex("6080" + LIFE_RUNTIME["ArgCtor"])
        self.assertEqual(
            seen["creation"],
            init + (42).to_bytes(32, "big") + (0xBEEF).to_bytes(32, "big"),
        )
        self.assertEqual(rec["lifecycle"]["args"], ["0x2a", "0xbeef"])
        self.assertEqual(
            rec["lifecycle"]["creation_sha256"],
            hashlib.sha256(seen["creation"]).hexdigest(),
        )

    def test_immutables(self):
        seen, rec = self.drive(
            "ImmutableCtor",
            "1000",
            REAL["ImmutableCtor"][1],
            self.ctor_ast("ImmutableCtor", ["uint256"], ["limit"]),
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertEqual(rec["lifecycle"]["immutables"], ["limit"])
        self.assertNotEqual(
            rec["lifecycle"]["runtime_sha256"], rec["lifecycle"]["bin_runtime_sha256"]
        )

    def test_disagreement_rejected(self):
        res = life_probe(
            "ArgCtor", [(0, 42), (1, 0xBEEF)], ethlib_storage=[(0, 41), (1, 0xBEEF)]
        )
        seen, _ = self.drive(
            "ArgCtor",
            "42,0xbeef",
            res,
            self.ctor_ast("ArgCtor", ["uint256", "address"]),
        )
        self.assertEqual((seen["status"], seen["code"]), ("REJECTED", 1))
        self.assertTrue(
            seen["line"].startswith("REJECTED: lifecycle: storage: "), seen["line"]
        )

    def test_kernel_proves(self):
        # Kernel YES only when the generated module builds: then CERTIFIED, exit 0, and only then.
        seen, rec = self.drive(
            "NoArgCtor",
            None,
            REAL["NoArgCtor"][1],
            self.ctor_ast("NoArgCtor", []),
            frame=True,
            build=lambda mod, text: (mod == "Lifecycle", ""),
        )
        self.assertEqual((seen["status"], seen["code"]), ("CERTIFIED", 0))
        self.assertEqual(seen["built"], ["Lifecycle"])
        self.assertEqual(rec["lifecycle"]["stages"][-1]["stage"], "kernel")
        self.assertTrue(rec["lifecycle"]["stages"][-1]["ok"])
        self.assertIn("Certified.x.Lifecycle.statement", rec["lifecycle"]["statement"])

    def test_kernel_refutes(self):
        seen, rec = self.drive(
            "NoArgCtor",
            None,
            REAL["NoArgCtor"][1],
            self.ctor_ast("NoArgCtor", []),
            frame=True,
            build=lambda mod, text: (mod == "LifecycleRefute", ""),
        )
        self.assertEqual((seen["status"], seen["code"]), ("REJECTED", 1))
        self.assertTrue(
            seen["line"].startswith("REJECTED: lifecycle: kernel: "), seen["line"]
        )
        self.assertFalse(rec["lifecycle"]["stages"][-1]["ok"])

    def test_kernel_neither(self):
        seen, _ = self.drive(
            "NoArgCtor",
            None,
            REAL["NoArgCtor"][1],
            self.ctor_ast("NoArgCtor", []),
            frame=True,
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertEqual(seen["built"], ["Lifecycle", "LifecycleRefute"])

    def test_payable_outside(self):
        ast = self.ctor_ast("NoArgCtor", [])
        ast["nodes"][0]["nodes"][0]["stateMutability"] = "payable"
        seen, _ = self.drive(
            "NoArgCtor",
            None,
            REAL["NoArgCtor"][1],
            ast,
            frame=True,
            build=lambda mod, text: (True, ""),
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertIn("payable", seen["line"])
        self.assertNotIn("built", seen)

    def test_sload_outside(self):
        # A constructor that reads storage, compiled by solar from generated text: exit 4, SLOAD named,
        # and nothing built.
        bins = solar_bins("SloadCtor", SLOAD_CTOR)
        if bins is None:
            self.skipTest("needs solar")
        res = life_probe("NoArgCtor", [(1, 1)], runtime=bins["bin-runtime"])
        seen, _ = self.drive(
            "SloadCtor",
            None,
            res,
            self.ctor_ast("SloadCtor", []),
            bins=bins,
            frame=True,
            build=lambda mod, text: (True, ""),
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertIn("SLOAD at", seen["line"])
        self.assertNotIn("built", seen)

    def test_revert_path_outside(self):
        # A constructor whose creation reverts with the run's argument, compiled by solar from
        # generated text: exit 4 before the clone, the probe or any Lean build.
        bins = solar_bins("RevertCtor", REVERT_CTOR)
        if bins is None:
            self.skipTest("needs solar")
        seen, rec = self.drive(
            "RevertCtor",
            "1",
            REAL["ArgCtor"][1],
            self.ctor_ast("RevertCtor", ["uint256"]),
            bins=bins,
            frame=True,
            build=lambda mod, text: (True, ""),
        )
        self.assertEqual((seen["status"], seen["code"]), ("UNSUPPORTED-lifecycle", 4))
        self.assertIn("a revert path", seen["line"])
        self.assertEqual(rec["lifecycle"]["halt"], "revert")
        self.assertNotIn("built", seen)

    def test_wrong_arity_blocked(self):
        with self.assertRaises(generic.Blocked):
            self.drive(
                "ArgCtor",
                "42",
                REAL["ArgCtor"][1],
                self.ctor_ast("ArgCtor", ["uint256", "address"]),
            )


SOLAR_BIN = pathlib.Path(
    os.environ.get("SOLAR", HERE.parent.parent / "target" / "debug" / "solar")
)
# A constructor that reads storage (outside the generic creation frame), as generated text.
SLOAD_CTOR = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SloadCtor {
    uint256 public x;
    uint256 public y;

    constructor() {
        y = x + 1;
    }
}
"""

# A constructor that reverts unless its argument exceeds 5, as generated text.
REVERT_CTOR = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertCtor {
    uint256 public x;

    constructor(uint256 a) {
        require(a > 5);
        x = a;
    }
}
"""


def solar_bins(contract, text=None, src=None):
    """solar's bin and bin-runtime of CONTRACT with the driver's options, from TEXT (written to a
    temporary file) or the file SRC; None without solar."""
    if not SOLAR_BIN.exists():
        return None
    with tempfile.TemporaryDirectory() as t:
        if src is None:
            src = pathlib.Path(t) / f"{contract}.sol"
            src.write_text(text)
        out = subprocess.run(
            [str(SOLAR_BIN), str(src), *generic.OPTIONS, "--emit=bin,bin-runtime"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    return next(
        v
        for k, v in json.loads(out)["contracts"].items()
        if k.rsplit(":", 1)[1] == contract
    )


class SpecConnectAnyCaller(unittest.TestCase):
    """--any-caller reads the symbolic-caller probe lines and refuses what holdsFn_of_tree_from
    does not cover, rather than stating it."""

    def test_parse_and_refusals(self):
        import spec_connect

        ok = spec_connect.parse_sym("sym C.f false some [256, 256] TREE .stop")
        self.assertEqual(ok, ("C.f", False, "[256, 256]", ".stop"))
        self.assertIsNone(spec_connect.any_caller_refusal(ok[1:], ([], [])))
        pay = spec_connect.parse_sym("sym C.g true some [] TREE .stop")
        self.assertIn("payable", spec_connect.any_caller_refusal(pay[1:], ([], [])))
        self.assertIn("mappings", spec_connect.any_caller_refusal(ok[1:], ([3], [])))
        none = spec_connect.parse_sym("sym C.h noerase")
        self.assertIn("eraseFnSym", spec_connect.any_caller_refusal(none[1:], ([], [])))
        self.assertIn("selector", spec_connect.any_caller_refusal(None, ([], [])))
        nw = spec_connect.parse_sym("sym C.k false none TREE .stop")
        self.assertIn("widths", spec_connect.any_caller_refusal(nw[1:], ([], [])))

    def test_from_block_names_no_fixed_caller(self):
        import spec_connect

        for nsi in (0, 1):
            lines = spec_connect.from_block("f", "C.f", 2, nsi, "NS", "M", "B", "[256, 256]", "u")
            text = "\n".join(lines)
            self.assertIn("theorem f_holds_from", text)
            self.assertIn("holdsFn_of_tree_from hdd", text)
            self.assertIn("#print axioms f_holds_from", text)
            self.assertNotIn("0x2000", text)
            self.assertNotIn(" ctxOf ", text)
            self.assertNotIn("sorry", text)


class LifecycleFrame(unittest.TestCase):
    """The kernel stage's inputs: the opcodes outside the generic creation frame, and the immutable
    patches the generated module states."""

    def test_outside(self):
        self.assertEqual(generic.frame_outside(bytes.fromhex("5f5455")), {"SLOAD": 1})
        self.assertEqual(
            generic.frame_outside(bytes.fromhex("7f" + "54" * 32 + "a0")), {"LOG0": 33}
        )
        self.assertEqual(
            generic.frame_outside(bytes.fromhex("34600a57602060005ff35b5f5ffd")), {}
        )
        for ex in ("NoArgCtor", "ArgCtor", "PairCtor", "ImmutableCtor"):
            bins = solar_bins(ex, src=LIFE / f"{ex}.sol")
            if bins is None:
                self.skipTest("needs solar")
            init, plain = bytes.fromhex(bins["bin"]), bytes.fromhex(bins["bin-runtime"])
            self.assertEqual(
                generic.frame_outside(generic.creation_prefix(init, plain)), {}, ex
            )

    def test_creation_halt(self):
        # The four examples return with their arguments, though each carries REVERT on its guard
        # paths; a creation that reaches REVERT or INVALID is found before any build.
        args = {
            "NoArgCtor": [],
            "ArgCtor": [42, 0xBEEF],
            "PairCtor": [42, 0xBEEF],
            "ImmutableCtor": [1000],
        }
        for ex, ws in args.items():
            bins = solar_bins(ex, src=LIFE / f"{ex}.sol")
            if bins is None:
                self.skipTest("needs solar")
            init = bytes.fromhex(bins["bin"])
            self.assertIn(
                0xFD,
                generic.creation_prefix(init, bytes.fromhex(bins["bin-runtime"])),
                ex,
            )
            creation = init + b"".join(w.to_bytes(32, "big") for w in ws)
            self.assertEqual(generic.creation_halt(creation)[0], "return", ex)
            self.assertEqual(
                generic.creation_halt(init + b"\0" * (32 * len(ws) - 1))[0],
                "revert" if ws else "return",
                ex,
            )
        bins = solar_bins("RevertCtor", REVERT_CTOR)
        init = bytes.fromhex(bins["bin"])
        self.assertEqual(
            generic.creation_halt(init + (1).to_bytes(32, "big"))[0], "revert"
        )
        self.assertEqual(
            generic.creation_halt(init + (6).to_bytes(32, "big"))[0], "return"
        )
        self.assertEqual(generic.creation_halt(bytes.fromhex("fe"))[0], "invalid")
        self.assertEqual(generic.creation_halt(bytes.fromhex("6003565b"))[0], "invalid")
        self.assertEqual(generic.creation_halt(bytes.fromhex("5f54"))[0], "outside")

    def test_patches(self):
        plain = bytes.fromhex(LIFE_RUNTIME["ImmutableCtor"])
        filled = bytes.fromhex(REAL["ImmutableCtor"][1]["runtime"])
        self.assertEqual(generic.immutable_patches(plain, plain), [])
        [(off, w)] = generic.immutable_patches(filled, plain)
        self.assertEqual((w, plain[off - 1]), (1000, 0x7F))
        self.assertIsNone(generic.immutable_patches(filled + b"\0", plain))


BASE_DRIVER = "128b674ed"


class NoConstructorRoute(unittest.TestCase):
    """The lifecycle kernel stage leaves the other tiers' generated text byte-identical to the
    driver it is built on (BASE_DRIVER), and their envelope unchanged for a source without
    constructor parameters."""

    def test_identical(self):
        p = subprocess.run(
            ["git", "-C", str(HERE), "show", f"{BASE_DRIVER}:tools/certify/generic.py"],
            capture_output=True,
            text=True,
            check=False,
        )
        if p.returncode != 0:
            self.skipTest(f"no {BASE_DRIVER} in this checkout")
        with tempfile.TemporaryDirectory() as t:
            (pathlib.Path(t) / "generic_base.py").write_text(p.stdout)
            sys.path.insert(0, t)
            try:
                import generic_base as old
            finally:
                sys.path.remove(t)
        fns = [
            {"sig": "get()", "sel": 0x6D4CE63C, "params": []},
            {"sig": "set(uint256)", "sel": 0x60FE47B1, "params": [".uint 256"]},
        ]
        events = [{"sig": "E(uint256)", "topic": 0x1234}]
        for m in (old, generic):
            m.out = [
                m.lowering_lean("n_x", "C", fns, events, "core", "[]"),
                m.lowering_lean("n_x", "C", fns, events, "core", "[(0, 1)]", sym=True),
                m.probe_lean("n_x", "C", fns),
                m.lakefile_lib("n_x", "C"),
            ]
        self.assertEqual(old.out, generic.out)
        if SOLC.exists():
            src = LIFE / "NoArgCtor.sol"
            ast, contracts = generic.solc_ast(str(SOLC), src)
            self.assertEqual(
                old.envelope(src.read_text(), ast, contracts, None, None, False),
                generic.envelope(src.read_text(), ast, contracts, None, None, False),
            )


LEAN_PATH = os.environ.get("LIFECYCLE_LEAN_PATH")


@unittest.skipUnless(
    LEAN_PATH and SOLC.exists() and SOLAR_BIN.exists(),
    "needs LIFECYCLE_LEAN_PATH (a solidity-lean build with Verify/Lifecycle), solc and solar",
)
class LifecycleKernel(unittest.TestCase):
    """The kernel stage on Lean the driver generates from solar's own output: the four examples
    prove Deploys and DeployedAtWith (Verify.Lifecycle.Statement); a wrong argument, a missing slot
    and an unpatched immutable are refuted by the kernel instead. Each build is one Lean process,
    one thread, at most 6000 MB."""

    CASES: typing.ClassVar = {
        "NoArgCtor": ([], [(0, 7)]),
        "ArgCtor": ([42, 0xBEEF], [(0, 42), (1, 0xBEEF)]),
        "PairCtor": ([42, 0xBEEF], [(0, 42), (1, 48921)]),
        "ImmutableCtor": ([1000], [(0, 1001)]),
    }

    def build(self, mod, text):
        with tempfile.TemporaryDirectory() as t:
            f = pathlib.Path(t) / f"{mod}.lean"
            f.write_text(text)
            p = subprocess.run(
                ["lean", "-M", "6000", "-j", "1", str(f)],
                capture_output=True,
                text=True,
                env=dict(os.environ, LEAN_PATH=LEAN_PATH),
                check=False,
            )
        return p.returncode == 0, p.stdout + p.stderr

    def kernel(self, ex, ws, storage, deployed=None, plain=None):
        src = LIFE / f"{ex}.sol"
        ast, contracts = generic.solc_ast(str(SOLC), src)
        _, fns, _ = generic.envelope(
            src.read_text(), ast, contracts, ex, None, False, True
        )
        bins = solar_bins(ex, src=src)
        init, rt = bytes.fromhex(bins["bin"]), bytes.fromhex(bins["bin-runtime"])
        plain = rt if plain is None else plain
        if deployed is None:
            deployed = rt
            if ex == "ImmutableCtor":
                off = rt.index(bytes.fromhex("7f" + "00" * 32)) + 1
                deployed = rt[:off] + (1000).to_bytes(32, "big") + rt[off + 32 :]
        sigs = [{"sig": f["sig"], "sel": f["sel"]} for f in fns]
        outside = [
            f"{op} at {o}"
            for op, o in generic.frame_outside(
                generic.creation_prefix(init, rt)
            ).items()
        ]
        return generic.lifecycle_kernel(
            self.build,
            f"Certified.T.{ex}",
            f"Verify.Lifecycle.Examples.{ex}",
            f"Verify.Lifecycle.Examples.{ex}.importedSourceUnit",
            ex,
            sigs,
            init,
            plain,
            deployed,
            ws,
            storage,
            outside,
        )

    def test_examples_prove(self):
        for ex, (ws, storage) in self.CASES.items():
            q = self.kernel(ex, ws, storage)
            self.assertEqual(
                (q["verdict"], q["ok"]), ("proved", True), (ex, q["detail"])
            )
            self.assertIn("theorem statement", q["lean"])

    def test_wrong_arg_refuted(self):
        q = self.kernel("ArgCtor", [41, 0xBEEF], [(0, 42), (1, 0xBEEF)])
        self.assertEqual(q["verdict"], "refuted", q["detail"])

    def test_missing_slot_refuted(self):
        q = self.kernel("PairCtor", [42, 0xBEEF], [(0, 42)])
        self.assertEqual(q["verdict"], "refuted", q["detail"])

    def test_unpatched_immutable_refuted(self):
        bins = solar_bins("ImmutableCtor", src=LIFE / "ImmutableCtor.sol")
        rt = bytes.fromhex(bins["bin-runtime"])
        q = self.kernel("ImmutableCtor", [1000], [(0, 1001)], deployed=rt)
        self.assertEqual(q["verdict"], "refuted", q["detail"])


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class LifecycleEnvelope(unittest.TestCase):
    """The examples through solc: constructor parameters stay outside the other tiers' envelope,
    and the lifecycle tier reads their types and the immutables."""

    def test_examples(self):
        want = {
            "NoArgCtor": ([], []),
            "ArgCtor": (["uint256", "address"], []),
            "ImmutableCtor": (["uint256"], ["limit"]),
        }
        for ex, (types, imm) in want.items():
            src = LIFE / f"{ex}.sol"
            ast, contracts = generic.solc_ast(str(SOLC), src)
            self.assertEqual(generic.constructor_params(ast, ex), types, ex)
            self.assertEqual(generic.immutables(ast, ex), imm, ex)
            generic.envelope(src.read_text(), ast, contracts, None, None, False, True)
            if types:
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(
                        src.read_text(), ast, contracts, None, None, False, False
                    )
                self.assertIn("--tier lifecycle", str(e.exception.reasons))

    def test_cli(self):
        with tempfile.TemporaryDirectory() as t:
            src = LIFE / "ArgCtor.sol"
            p = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "generic.py"),
                    str(src),
                    "none",
                    "--constructor-args",
                    "1,2",
                    "--solidity-lean",
                    "/nonexistent",
                    "--work",
                    t + "/w",
                    "--record-dir",
                    t + "/r",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(p.returncode, 2)
            self.assertIn("--constructor-args needs --tier lifecycle", p.stderr)
            p = subprocess.run(
                [sys.executable, str(HERE / "generic.py"), "--help"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertIn("lifecycle", p.stdout)
            self.assertIn("UNSUPPORTED-lifecycle", p.stdout)


class FreshCertified(unittest.TestCase):
    """A base that already holds Certified/NAME (a published record of the same source): the run's
    clone drops it and keeps one library stanza; a foreign stanza for NAME is refused."""

    def test_existing_record(self):
        import tempfile
        with tempfile.TemporaryDirectory() as t:
            c = pathlib.Path(t)
            (c / "Certified" / "n_1" / "Proofs").mkdir(parents=True)
            (c / "Certified" / "n_1" / "Final.lean").write_text("stale")
            (c / "lakefile.lean").write_text("x" + generic.lakefile_lib("n_1", "C"))
            self.assertTrue(generic.fresh_certified(c, "n_1", "C"))
            self.assertFalse((c / "Certified" / "n_1").exists())
            self.assertEqual((c / "lakefile.lean").read_text().count("lean_lib «n_1»"), 1)

    def test_new_and_foreign(self):
        import tempfile
        with tempfile.TemporaryDirectory() as t:
            c = pathlib.Path(t)
            (c / "lakefile.lean").write_text("x\n")
            self.assertFalse(generic.fresh_certified(c, "n_1", "C"))
            self.assertEqual((c / "lakefile.lean").read_text(), "x\n" + generic.lakefile_lib("n_1", "C"))
            (c / "lakefile.lean").write_text("lean_lib «n_1» where\n  roots := #[]\n")
            with self.assertRaises(generic.Failed):
                generic.fresh_certified(c, "n_1", "C")


class FreshCertifiedControls(unittest.TestCase):
    """Negative controls of the existing-record repair: a stale generated file is not reused, the
    supplied base is never touched (only its clone), and duplicate or foreign stanzas are handled."""

    def setUp(self):
        import tempfile, subprocess
        self.t = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.t.name)
        self.base = root / "base"
        (self.base / "Certified" / "n_1" / "gen").mkdir(parents=True)
        (self.base / "Certified" / "n_1" / "gen" / "Stage0.lean").write_text("stale export")
        (self.base / "Certified" / "n_1" / "Final.lean").write_text("stale final")
        (self.base / "lakefile.lean").write_text("x" + generic.lakefile_lib("n_1", "C"))
        g = ["git", "-c", "user.name=t", "-c", "user.email=t@l"]
        subprocess.run(g + ["init", "-q", str(self.base)], check=True)
        subprocess.run(g + ["-C", str(self.base), "add", "-A"], check=True)
        subprocess.run(g + ["-C", str(self.base), "commit", "-qm", "base"], check=True)
        self.head = subprocess.run(["git", "-C", str(self.base), "rev-parse", "HEAD"], check=True,
                                   capture_output=True, text=True).stdout
        self.clone = root / "clone"
        subprocess.run(["git", "clone", "-q", "--shared", str(self.base), str(self.clone)], check=True)

    def tearDown(self):
        self.t.cleanup()

    def base_unchanged(self):
        import subprocess
        st = subprocess.run(["git", "-C", str(self.base), "status", "--porcelain"], check=True,
                            capture_output=True, text=True).stdout
        head = subprocess.run(["git", "-C", str(self.base), "rev-parse", "HEAD"], check=True,
                              capture_output=True, text=True).stdout
        self.assertEqual((st, head), ("", self.head))
        self.assertEqual((self.base / "Certified" / "n_1" / "Final.lean").read_text(), "stale final")

    def test_stale_and_duplicate(self):
        self.assertTrue(generic.fresh_certified(self.clone, "n_1", "C"))
        # The stale generated files are gone from the clone, so nothing of the base's record is reused.
        self.assertFalse((self.clone / "Certified" / "n_1").exists())
        self.assertEqual((self.clone / "lakefile.lean").read_text().count("lean_lib «n_1»"), 1)
        self.base_unchanged()

    def test_foreign_stanza(self):
        (self.clone / "lakefile.lean").write_text("lean_lib n_1 where\n  roots := #[]\n")
        with self.assertRaises(generic.Failed):
            generic.fresh_certified(self.clone, "n_1", "C")
        self.base_unchanged()

    def test_other_names_untouched(self):
        (self.clone / "Certified" / "m_2").mkdir()
        (self.clone / "Certified" / "m_2" / "Final.lean").write_text("other")
        generic.fresh_certified(self.clone, "n_1", "C")
        self.assertEqual((self.clone / "Certified" / "m_2" / "Final.lean").read_text(), "other")


if __name__ == "__main__":
    unittest.main(verbosity=2)
