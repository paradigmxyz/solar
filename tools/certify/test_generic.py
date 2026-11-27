#!/usr/bin/env python3
"""Negative controls for tools/certify/generic.py that need no Lean: the strict predicate's source
binding and --replay's manifest check. Run: python3 tools/certify/test_generic.py"""

import argparse
import ast as pyast
import contextlib
import hashlib
import io
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import typing
import unittest
import unittest.mock

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
    cr["solar_sha256"] = BIN
    ok, need_c, comps = generic.strict(cr, "certified")
    rec = {
        "name": NAME,
        "contract": CONTRACT,
        "requested_tier": "certified",
        "status": "CERTIFIED",
        "strict": {"tier": "certified", "holds": ok, "components": need_c},
        "tier_reached": generic.reached_tier(comps),
        "published_statement": cr["end_to_end_statement"],
        "solar": solar_rec(),
        **{k: json.loads(json.dumps(cr[k])) for k in generic.CLAIM_KEYS},
    }
    return cr, rec


BIN = "e" * 64


# A lifecycle-tier record's provenance-bound inputs (generic.LIFECYCLE_PROVENANCE).
LIFE_PROV = {
    "bind": {
        "source": "Verify.LoopSource.Equiv.CounterCtx.src 8",
        "source_module": "Verify.LoopSource.Equiv.CounterCtx",
        "ws": [8],
    },
    "args": ["0x8"],
    "creation_sha256": "4" * 64,
}
SOLAR_COMMIT = "5" * 40


def solar_rec():
    """A record's solar entry with build provenance version 2, as build_provenance writes it."""
    return {
        "commit": SOLAR_COMMIT,
        "dirty": False,
        "binary_sha256": BIN,
        "binary_commit": SOLAR_COMMIT,
        "version": "solar 0.3.0",
        "build": {
            "provenance_version": 2,
            "commit": SOLAR_COMMIT,
            "tree": "6" * 40,
            "submodules": [
                "-1835d247de183f23e7b0e2bc4a50732af525b77c testdata/solidity"
            ],
            "cargo_lock_sha256": "7" * 64,
            "rustc_vv": "rustc 1.100.0 (abc 2026-01-01)\nbinary: rustc\ncommit-hash: abc\nhost: x86_64-unknown-linux-gnu",
            "target": "x86_64-unknown-linux-gnu",
            "profile": "debug",
            "features": "mimalloc,tracing",
            "rustflags": {
                "RUSTFLAGS": None,
                "CARGO_ENCODED_RUSTFLAGS": None,
                "CARGO_BUILD_RUSTFLAGS": None,
            },
            "command": generic.SOLAR_BUILD_CMD,
            "target_source": "host (no --target; CARGO_BUILD_TARGET and build.target refused)",
            "cargo_env": {"CARGO_INCREMENTAL": None},
            "cargo_config": [
                {"scope": "repo", "name": "config.toml", "sha256": "3" * 64}
            ],
            "artifact": {"sha256": BIN, "fresh": False},
        },
    }


class SolarBuildConfig(unittest.TestCase):
    """The one supported solar build configuration (build_solar, cargo_env_problem,
    cargo_config_problem, artifact_problem): each way Cargo could build something else is refused
    by name, and only the artifact Cargo reports, unchanged, is run."""

    HOST = "x86_64-unknown-linux-gnu"

    def config(self, root, text, where="home"):
        d = (root / "home") if where == "home" else (root / "repo" / ".cargo")
        d.mkdir(parents=True, exist_ok=True)
        (d / "config.toml").write_text(text)

    def problem(self, root):
        (root / "repo").mkdir(exist_ok=True)
        return generic.cargo_config_problem(root / "repo", root / "home", self.HOST)

    def test_env_refused(self):
        for k, v in [
            ("CARGO_BUILD_TARGET", "aarch64-unknown-linux-gnu"),
            ("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", "-Copt-level=3"),
            ("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER", "cc"),
            ("CARGO_PROFILE_DEV_OPT_LEVEL", "3"),
            ("RUSTC_WRAPPER", "sccache"),
            ("RUSTC_WORKSPACE_WRAPPER", "w"),
            ("RUSTC", "/opt/rustc"),
            ("RUSTFLAGS", "-Cdebuginfo=0"),
            ("CARGO_ENCODED_RUSTFLAGS", "-Cx"),
            ("CARGO_BUILD_RUSTFLAGS", "-Cx"),
            ("CARGO_UNSTABLE_BUILD_STD", "true"),
        ]:
            with self.subTest(k):
                why = generic.cargo_env_problem({"PATH": "/bin", k: v})
                self.assertIsNotNone(why)
                self.assertTrue(why.startswith(f"{k} is set"), why)
        self.assertIsNone(
            generic.cargo_env_problem(
                {
                    "CARGO_BUILD_JOBS": "2",
                    "CARGO_TARGET_DIR": "t",
                    "CARGO_HOME": "h",
                    "CARGO_NET_OFFLINE": "true",
                }
            )
        )

    def test_config_refused(self):
        cases = {
            "build.target": ('[build]\ntarget = "aarch64-unknown-linux-gnu"\n', "home"),
            "build.rustflags": ('[build]\nrustflags = ["-Cx"]\n', "home"),
            "build.rustc-wrapper": ('[build]\nrustc-wrapper = "sccache"\n', "home"),
            "build.target-dir": ('[build]\ntarget-dir = "elsewhere"\n', "repo"),
            "[target.x86_64-unknown-linux-gnu]": (
                '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-Cx"]\n',
                "repo",
            ),
            "[target.cfg(unix)]": (
                "[target.'cfg(unix)']\nrustflags = [\"-Cx\"]\n",
                "repo",
            ),
            "[target.aarch64-unknown-linux-gnu]": (
                '[target.aarch64-unknown-linux-gnu]\nlinker = "x"\n',
                "home",
            ),
            "[profile]": ("[profile.dev]\nopt-level = 3\n", "home"),
            "[env]": ('[env]\nX = "1"\n', "home"),
            "[unstable]": ('[unstable]\nbuild-std = ["std"]\n', "repo"),
        }
        for label, (text, where) in cases.items():
            with self.subTest(label), tempfile.TemporaryDirectory() as t:
                root = pathlib.Path(t)
                self.config(root, text, where)
                why, _ = self.problem(root)
                self.assertIsNotNone(why, label)
                self.assertIn(
                    label.split("(")[0] if label.startswith("[target.cfg") else label,
                    why,
                )

    def test_external_dependency_overrides_refused(self):
        """paths, [patch] and [source] outside the checkout swap crate code without changing
        Cargo.lock: refused by name in a parent's and in CARGO_HOME's config, by build_solar before
        cargo runs (no cargo call, no artifact)."""
        cases = {
            "paths": 'paths = ["/elsewhere/serde"]\n',
            "[patch]": '[patch.crates-io]\nserde = { path = "/elsewhere/serde" }\n',
            "[source]": '[source.crates-io]\nreplace-with = "mine"\n[source.mine]\ndirectory = "/v"\n',
        }
        for label, text in cases.items():
            for where in ("cargo_home", "parent"):
                with (
                    self.subTest(label=label, where=where),
                    tempfile.TemporaryDirectory() as t,
                ):
                    why, calls = self.external(pathlib.Path(t), where, text)
                    self.assertIsNotNone(why)
                    self.assertTrue(why.endswith(f"sets {label}"), why)
                    self.assertIn(f"({where})", why)
                    self.assertFalse([c for c in calls if c[:1] == ["cargo"]], calls)
                    self.assertIsNone(generic.SOLAR_ARTIFACT)

    def external(self, base, where, text):
        """build_solar for a checkout under BASE with TEXT as a parent's or CARGO_HOME's config:
        (the refusal or None, the commands it ran)."""
        root = base / "parent" / "repo"
        root.mkdir(parents=True)
        home = base / "home"
        d = home if where == "cargo_home" else base / "parent" / ".cargo"
        d.mkdir(parents=True)
        (d / "config.toml").write_text(text)
        exe = root / "target/debug/solar"
        exe.parent.mkdir(parents=True)
        exe.write_text("built")
        calls = []
        generic.SOLAR_ARTIFACT = None
        with unittest.mock.patch.object(generic, "ROOT", root):
            try:
                self.build(root, exe, cargo_home=home, calls=calls)
            except generic.Failed as e:
                return str(e), calls
        return None, calls

    def test_external_baseline_builds_and_binds(self):
        """A harmless parent and CARGO_HOME config ([net], [term]) builds and is bound."""
        for where, text in (
            ("cargo_home", "[net]\noffline = true\n"),
            ("parent", "[term]\nquiet = true\n"),
        ):
            with self.subTest(where), tempfile.TemporaryDirectory() as t:
                why, calls = self.external(pathlib.Path(t), where, text)
                self.assertIsNone(why)
                self.assertTrue([c for c in calls if c[:1] == ["cargo"]])
                self.assertIsNotNone(generic.SOLAR_ARTIFACT)
                base = pathlib.Path(t)
                _, bound = generic.cargo_config_problem(
                    base / "parent" / "repo", base / "home", self.HOST
                )
                self.assertEqual([b["scope"] for b in bound], [where])
                generic.SOLAR_ARTIFACT = None

    def test_config_bound(self):
        """The checkout's own config (its [env] and other targets' tables) and a harmless
        CARGO_HOME config are bound by digest, not refused."""
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            self.config(
                root, (generic.ROOT / ".cargo" / "config.toml").read_text(), "repo"
            )
            self.config(root, "[net]\noffline = true\n", "home")
            why, bound = self.problem(root)
            self.assertIsNone(why)
            self.assertEqual([b["scope"] for b in bound], ["repo", "cargo_home"])
            self.assertTrue(
                all(re.fullmatch(r"[0-9a-f]{64}", b["sha256"]) for b in bound)
            )
            self.assertNotIn(t, json.dumps(bound))

    def build(self, root, executable, fresh=False, cargo_home=None, calls=None):
        """build_solar with rustc -vV answered and cargo replaced by one reporting EXECUTABLE as the
        solar artifact; CALLS collects every command it was asked to run."""
        line = json.dumps(
            {
                "reason": "compiler-artifact",
                "target": {"name": "solar", "kind": ["bin"]},
                "executable": str(executable),
                "fresh": fresh,
            }
        )
        calls = [] if calls is None else calls

        def fake(cmd, *a, **k):
            calls.append(list(cmd))
            if cmd[:2] == ["rustc", "-vV"]:
                return 0, f"rustc 1.100.0\nhost: {self.HOST}\n", ""
            return 0, line + "\n", ""

        env = {
            k: v for k, v in os.environ.items() if not generic.cargo_env_problem({k: v})
        }
        env["CARGO_TARGET_DIR"] = str(root / "target")
        env["CARGO_HOME"] = str(cargo_home or root / "nohome")
        with (
            unittest.mock.patch.object(generic, "run", fake),
            unittest.mock.patch.dict(os.environ, env, clear=True),
        ):
            return generic.build_solar()

    def test_stale_host_output_trap(self):
        """An old target/debug/solar while Cargo built elsewhere: refused before anything runs."""
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            (root / "target/debug").mkdir(parents=True)
            (root / "target/debug/solar").write_text("stale")
            elsewhere = root / "target/aarch64-unknown-linux-gnu/debug/solar"
            elsewhere.parent.mkdir(parents=True)
            elsewhere.write_text("fresh")
            with self.assertRaises(generic.Failed) as e:
                self.build(root, elsewhere)
            self.assertIn("not the expected output", str(e.exception))
            self.assertIsNone(generic.SOLAR_ARTIFACT)

    def test_artifact_bound_and_changed(self):
        with tempfile.TemporaryDirectory() as t:
            root = pathlib.Path(t)
            exe = root / "target/debug/solar"
            exe.parent.mkdir(parents=True)
            exe.write_text("built")
            got = self.build(root, exe)
            self.assertEqual(got.resolve(), exe.resolve())
            self.assertEqual(generic.SOLAR_ARTIFACT["sha256"], generic.sha256_file(exe))
            self.assertIsNone(generic.artifact_problem(exe))
            exe.write_text("swapped")
            self.assertIn("changed after the build", generic.artifact_problem(exe))
            self.assertIn(
                "is not the build's artifact", generic.artifact_problem(root / "other")
            )
            generic.SOLAR_ARTIFACT = None


class LifecycleRecordReplay(unittest.TestCase):
    """--replay of a record the lifecycle driver itself writes: lifecycle_run, with only solar's
    compile, the front end, the probe and lake replaced, against a real git base, sealed by
    seal_record (main's record writer). Every all-tier replay check applies to it unchanged."""

    EXAMPLE = "NoArgCtor"
    NAME = "noargctor_0000abcd"
    ARGS = None
    TYPES: typing.ClassVar[list] = []
    BIND: typing.ClassVar[dict] = {
        "unit": "Spec.unit",
        "unit_module": "Spec.Unit",
        "fuel": "Spec.fuel",
        "init": "Spec.init",
        "runtime": "Spec.runtime",
        "modules": ["Spec.Bytes"],
        "ws": [],
        "source": "Spec.src",
        "source_module": "Spec.Src",
        "expected": {
            "module": "Spec.Expected",
            "binders": "",
            "prop": "SolidCore.Verify.Generic.Expected Spec.src Spec.spec Spec.runtime",
            "term": "Spec.expected_created",
        },
    }

    def run_driver(self, root):
        """A real lifecycle record directory, written by lifecycle_run and seal_record."""
        self.repo = root / "sl"
        manifest = json.dumps({"version": "1.1.0", "packages": []})
        base = git_init(
            self.repo,
            {
                "lakefile.lean": "-- lakefile\n",
                "lean-toolchain": "leanprover/lean4:v4.35.0\n",
                "lake-manifest.json": manifest,
                "Verify/Lifecycle/CreationFrame.lean": "-- frame\n",
            },
        )
        src = LIFE / f"{self.EXAMPLE}.sol"
        src_sha = hashlib.sha256(src.read_text().encode()).hexdigest()
        bins = {
            "bin": "6080" + LIFE_RUNTIME[self.EXAMPLE],
            "bin-runtime": LIFE_RUNTIME[self.EXAMPLE],
        }
        (root / "bind.json").write_text(json.dumps(self.BIND))
        rec_dir = root / "rec"
        rec_dir.mkdir()
        record = {
            "schema": "solar-certify-generic/1",
            "name": self.NAME,
            "contract": self.EXAMPLE,
            "requested_tier": "lifecycle",
            "source": {"file": str(src), "sha256": src_sha},
            "spec": {"kind": "none", "sha256": None},
            "options": generic.OPTIONS,
            "solar": solar_rec(),
            "solc": {"version": generic.SOLC_VERSION, "sha256": "1" * 64},
            "lean_toolchain": {"recorded_digest": "2" * 64},
        }
        saved = (
            generic.run,
            generic.frontend,
            generic.run_lifecycle_probe,
            generic.log,
            generic.lifecycle_build,
        )
        real_run = generic.run

        def fake_run(cmd, *a, **kw):
            if cmd[:1] == ["solar"]:
                return 0, json.dumps({"contracts": {f"{src}:{self.EXAMPLE}": bins}}), ""
            return real_run(cmd, *a, **kw)

        def fake_build(lean_dir, name, source_text, mod, text, cache):
            d = lean_dir / "Certified" / name
            d.mkdir(parents=True, exist_ok=True)
            (d / "Source.lean").write_text(source_text)
            (d / f"{mod}.lean").write_text(text)
            return mod != "LifecycleRefute", ""

        def finish(status, code, line, **extra):
            generic.seal_record(rec_dir, record, status, **extra)
            raise SystemExit(code)

        generic.run, generic.lifecycle_build = fake_run, fake_build
        generic.frontend = lambda *a, **kw: (0, "-- body", "")
        generic.run_lifecycle_probe = lambda *a, **kw: REAL[self.EXAMPLE][1]
        generic.log = lambda *a, **kw: None
        try:
            ns = argparse.Namespace(
                constructor_args=self.ARGS,
                solidity_lean=str(self.repo),
                base=base,
                solc="solc",
                probe_cache=None,
                lifecycle_bind=str(root / "bind.json"),
            )
            with self.assertRaises(SystemExit) as e:
                generic.lifecycle_run(
                    ns,
                    record,
                    "solar",
                    src,
                    src_sha,
                    self.NAME,
                    self.EXAMPLE,
                    LifecycleRun.ctor_ast(None, self.EXAMPLE, self.TYPES),
                    root / "w",
                    finish,
                    [],
                    rec_dir,
                )
            self.assertEqual(
                e.exception.code, 0, record.get("lifecycle_manifest_error")
            )
        finally:
            (
                generic.run,
                generic.frontend,
                generic.run_lifecycle_probe,
                generic.log,
                generic.lifecycle_build,
            ) = saved
        return rec_dir

    def replay(self, d, fresh_edit=None):
        orig = json.loads((d / "record.json").read_text())

        def fake_run(cmd, *args, **kw):
            if "--record-dir" not in cmd:
                return saved(cmd, *args, **kw)
            self.fresh_cmd = list(cmd)
            fresh = json.loads(json.dumps(orig))
            if fresh_edit:
                fresh_edit(fresh)
            out = pathlib.Path(cmd[cmd.index("--record-dir") + 1])
            out.mkdir(parents=True)
            (out / "record.json").write_text(json.dumps(fresh))
            return subprocess.CompletedProcess(cmd, 0)

        a = argparse.Namespace(
            replay=str(d),
            work=str(d.parent / "rw"),
            solidity_lean=str(self.repo),
            packages_from=None,
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

    def edit_record(self, d, f):
        rec = json.loads((d / "record.json").read_text())
        f(rec)
        (d / "record.json").write_text(json.dumps(rec))

    def test_the_driver_writes_a_manifest(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.run_driver(pathlib.Path(t))
            rec = json.loads((d / "record.json").read_text())
        self.assertEqual(rec["status"], "CERTIFIED")
        files = rec["run_files"]
        for f in (
            f"{self.EXAMPLE}.sol",
            *generic.LIFECYCLE_FILES.values(),
            f"inputs/Certified/{self.NAME}/Source.lean",
            f"inputs/Certified/{self.NAME}/Lifecycle.lean",
            f"inputs/Certified/{self.NAME}/SourceSame.lean",
            f"inputs/Certified/{self.NAME}/Composed.lean",
            "inputs/lakefile.lean",
        ):
            self.assertIn(f, files)
        sl = rec["solidity_lean"]
        self.assertEqual(sorted(sl["inputs"]), sorted(sl["inputs_sha256"]))
        self.assertRegex(sl["executed_tree"], r"^[0-9a-f]{40}$")
        self.assertIsNone(rec["packages"]["cache_sha256"])
        self.assertNotIn("certify-record.json", files)

    def test_complete_record_replays(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.run_driver(pathlib.Path(t))
            code, line = self.replay(d)
        self.assertEqual(code, 0, line)
        self.assertTrue(line.startswith("CERTIFIED: replay of "), line)
        self.assertIn("--tier", self.fresh_cmd)
        self.assertEqual(
            self.fresh_cmd[self.fresh_cmd.index("--tier") + 1], "lifecycle"
        )
        # NoArgCtor has no constructor words: the record's args [] are passed as the empty list.
        self.assertEqual(
            self.fresh_cmd[self.fresh_cmd.index("--constructor-args") + 1], ""
        )
        self.assertIn("--lifecycle-bind", self.fresh_cmd)

    def test_each_compared_key_refused(self):
        """Each difference the fresh run could show, refused by the key that actually refuses it."""
        cases = {
            "lifecycle.args": lambda r: r["lifecycle"].update(args=["0x5"]),
            "lifecycle.bind": lambda r: r["lifecycle"]["bind"].update(
                source_module="Spec.Other"
            ),
            "lifecycle.bind (source)": lambda r: r["lifecycle"]["bind"].update(
                source="Spec.other"
            ),
            "lifecycle.creation_sha256": lambda r: r["lifecycle"].update(
                creation_sha256="5" * 64
            ),
            "lifecycle.kernel_ok": lambda r: r["lifecycle"].update(stages=[]),
            "lifecycle.composed.theorem": lambda r: r["lifecycle"]["composed"].update(
                theorem="X.y"
            ),
            "lifecycle.composed.built": lambda r: r["lifecycle"]["composed"].update(
                built=False
            ),
            "status": lambda r: r.update(status="UNSUPPORTED-lifecycle"),
            "tier_reached": lambda r: r.update(tier_reached="none"),
            "solidity_lean.inputs_sha256": lambda r: r["solidity_lean"][
                "inputs_sha256"
            ].update(x="0"),
            "solidity_lean.executed_tree": lambda r: r["solidity_lean"].update(
                executed_tree="0" * 40
            ),
        }
        for key, f in cases.items():
            with self.subTest(key), tempfile.TemporaryDirectory() as t:
                code, line = self.replay(self.run_driver(pathlib.Path(t)), f)
                self.assertEqual(code, 1, line)
                self.assertIn(
                    f"REJECTED: replay: {key.split(' ')[0]} differs between the record and the fresh run",
                    line,
                )

    def test_tampered_record_refused_by_its_own_files(self):
        """A record whose args, bind or creation hash is not its own listed files' is refused by name
        before the fresh run."""
        cases = {
            "args are not the words lifecycle/creation.hex appends": lambda r: r[
                "lifecycle"
            ].update(args=["0x5"]),
            "lifecycle.bind is not lifecycle/bind.json": lambda r: r["lifecycle"][
                "bind"
            ].update(source_module="Spec.Other"),
            "creation_sha256 is not lifecycle/creation.hex's": lambda r: r[
                "lifecycle"
            ].update(creation_sha256="5" * 64),
            "the record lists no lifecycle/creation.hex": lambda r: r["run_files"].pop(
                "lifecycle/creation.hex"
            ),
        }
        for needle, f in cases.items():
            with self.subTest(needle), tempfile.TemporaryDirectory() as t:
                d = self.run_driver(pathlib.Path(t))
                self.edit_record(d, f)
                if "lists no" in needle:
                    (d / "lifecycle" / "creation.hex").unlink()
                self.fresh_cmd = None
                code, line = self.replay(d)
                self.assertEqual(code, 1, line)
                self.assertIn(needle, line)
                self.assertIsNone(self.fresh_cmd)

    def test_missing_provenance_fails_closed(self):
        for k in ("bind", "args", "creation_sha256"):
            with self.subTest(k), tempfile.TemporaryDirectory() as t:
                d = self.run_driver(pathlib.Path(t))
                self.edit_record(d, lambda r, k=k: r["lifecycle"].pop(k))
                code, line = self.replay(d)
                self.assertEqual(code, 1, line)
                self.assertIn(
                    f"versioned-unsupported: the recorded run has no lifecycle provenance ({k})",
                    line,
                )

    def test_claims_hold_together(self):
        """A lifecycle record's verdict must be its stages': each inconsistency is refused by name."""
        cases = {
            "no proved kernel stage": lambda r: r["lifecycle"].update(stages=[]),
            "has no built composed theorem": lambda r: r["lifecycle"][
                "composed"
            ].update(built=False),
            "has tier_reached 'functional'": lambda r: r.update(
                tier_reached="functional"
            ),
            "claims tier_reached 'lifecycle'": lambda r: r.update(
                status="UNSUPPORTED-lifecycle"
            ),
            "publishes no final": lambda r: r.update(final={"theorem": "x"}),
        }
        for needle, f in cases.items():
            with self.subTest(needle), tempfile.TemporaryDirectory() as t:
                d = self.run_driver(pathlib.Path(t))
                self.edit_record(d, f)
                code, line = self.replay(d)
                self.assertEqual(code, 1, line)
                self.assertIn(needle, line)

    def test_stray_certify_record_is_refused_not_read(self):
        """An unlisted certify-record.json is refused as an unlisted file before anything reads it;
        a listed one is refused because the lifecycle tier writes none."""
        with tempfile.TemporaryDirectory() as t:
            d = self.run_driver(pathlib.Path(t))
            (d / "certify-record.json").write_text(
                '{"solar_sha256": "not json evidence"'
            )
            code, line = self.replay(d)
        self.assertEqual(code, 1, line)
        self.assertIn(
            "files the record's manifest does not list: ['certify-record.json']", line
        )
        with tempfile.TemporaryDirectory() as t:
            d = self.run_driver(pathlib.Path(t))
            (d / "certify-record.json").write_text("{}")
            self.edit_record(
                d,
                lambda r: r["run_files"].update(
                    {"certify-record.json": sha(d / "certify-record.json")}
                ),
            )
            code, line = self.replay(d)
        self.assertEqual(code, 1, line)
        self.assertIn(
            "the lifecycle tier runs no scripts/certify, but the record lists a certify-record.json",
            line,
        )


class LifecycleRecordReplayArgs(LifecycleRecordReplay):
    """The same, for a constructor with words (ArgCtor 42, 0xbeef): the fresh run gets the record's
    words in decimal and its binding."""

    EXAMPLE = "ArgCtor"
    NAME = "argctor_0000abcd"
    ARGS = "42,0xbeef"
    TYPES: typing.ClassVar[list] = ["uint256", "address"]
    BIND: typing.ClassVar[dict] = dict(
        LifecycleRecordReplay.BIND,
        ws=[42, 48879],
        source="Spec.src 42 48879",
        expected=dict(
            LifecycleRecordReplay.BIND["expected"],
            prop="SolidCore.Verify.Generic.Expected (Spec.src 42 48879) Spec.spec Spec.runtime",
        ),
    )

    def test_complete_record_replays(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.run_driver(pathlib.Path(t))
            code, line = self.replay(d)
            cmd = self.fresh_cmd
            bind = json.loads(
                pathlib.Path(cmd[cmd.index("--lifecycle-bind") + 1]).read_text()
            )
        self.assertEqual(code, 0, line)
        self.assertTrue(line.startswith("CERTIFIED: replay of "), line)
        self.assertEqual(cmd[cmd.index("--constructor-args") + 1], "42,48879")
        self.assertEqual(bind, self.BIND)


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
        self.cr_sha = files["certify-record.json"]
        if edit:
            edit(rec)
        if "certify-record.json" not in rec.get("run_files", {}):
            # A record that does not list it (the lifecycle tier) has no certify-record.json.
            (d / "certify-record.json").unlink()
        (d / "record.json").write_text(json.dumps(rec))
        return d

    def replay(self, d, fresh_edit=None, packages_from=True, fresh_cr_edit=None):
        """replay_main in this process, its fresh run replaced by one that writes the record's own
        claims back (edited by fresh_edit) and its certify-record.json (edited by fresh_cr_edit):
        the checks run, no Lean does."""
        orig = json.loads((d / "record.json").read_text())
        orig_cr = (
            json.loads((d / "certify-record.json").read_text())
            if (d / "certify-record.json").exists()
            else {}
        )

        def fake_run(cmd, *args, **kw):
            if "--record-dir" not in cmd:
                # git and the checks' own commands run.
                return saved(cmd, *args, **kw)
            self.fresh_cmd = list(cmd)
            if "--lifecycle-bind" in cmd:
                self.fresh_bind = json.loads(
                    pathlib.Path(cmd[cmd.index("--lifecycle-bind") + 1]).read_text()
                )
            fresh = json.loads(json.dumps(orig))
            if fresh_edit:
                fresh_edit(fresh)
            out = pathlib.Path(cmd[cmd.index("--record-dir") + 1])
            out.mkdir(parents=True)
            (out / "record.json").write_text(json.dumps(fresh))
            fcr = json.loads(json.dumps(orig_cr))
            if fresh_cr_edit:
                fresh_cr_edit(fcr)
            (out / "certify-record.json").write_text(json.dumps(fcr))
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
            self.assertIn(
                ": a fresh cold run from the same pinned source/build inputs gave identical checked outputs (",
                line,
            )
            self.assertNotIn("every pin", line)

    def test_build_provenance_binds_the_build_output(self):
        """build_provenance refuses an executable that is not this run's build artifact, and reads
        the rest by running git, rustc and Cargo."""
        old = generic.SOLAR_ARTIFACT
        try:
            generic.SOLAR_ARTIFACT = None
            with self.assertRaises(generic.Failed) as e:
                generic.build_provenance(
                    generic.ROOT,
                    "/usr/bin/true",
                    "Build Profile: debug\nBuild Features: x",
                )
            self.assertIn("not one this run's build produced", str(e.exception))
        finally:
            generic.SOLAR_ARTIFACT = old

    def test_non_lifecycle_fresh_command_has_no_lifecycle_inputs(self):
        """A certified-tier record's fresh command carries neither --constructor-args nor
        --lifecycle-bind."""
        with tempfile.TemporaryDirectory() as t:
            self.fresh_cmd = None
            code, line = self.replay(self.make(pathlib.Path(t)))
            self.assertEqual(code, 0, line)
            self.assertIsNotNone(self.fresh_cmd)
            self.assertNotIn("--constructor-args", self.fresh_cmd)
            self.assertNotIn("--lifecycle-bind", self.fresh_cmd)

    def test_non_lifecycle_record_without_lifecycle_keys(self):
        """A record of another tier without any lifecycle key replays as before."""
        with tempfile.TemporaryDirectory() as t:
            d = self.make(pathlib.Path(t))
            self.assertNotIn("lifecycle", json.loads((d / "record.json").read_text()))
            code, line = self.replay(d)
        self.assertEqual(code, 0, line)
        self.assertNotIn("lifecycle provenance", line)

    # Replay pins (build provenance instead of cross-build binary identity). Each line is snapshot
    # in testdata/replay_pin.snapshot, which REPLAY_PIN_SNAPSHOT=1 rewrites.
    def pin_case(self, edit=None, fresh_edit=None, fresh_cr_edit=None):
        with tempfile.TemporaryDirectory() as t:
            code, line = self.replay(
                self.make(pathlib.Path(t), edit),
                fresh_edit,
                fresh_cr_edit=fresh_cr_edit,
            )
        return code, re.sub(r"(?<=[ (])/[^ ()]*", "<path>", line)

    def test_replay_pins(self):
        def fresh_bin(r):
            r["solar"]["binary_sha256"] = "f" * 64
            r["solar"]["build"]["artifact"]["sha256"] = "f" * 64

        def build(k, v):
            def e(r):
                r["solar"]["build"][k] = v

            return e

        def both(*es):
            def e(r):
                for x in es:
                    x(r)

            return e

        def unversioned(r):
            del r["solar"]["build"]

        cases = {
            "only the binary digest differs": (
                0,
                None,
                fresh_bin,
                lambda c: c.update(solar_sha256="f" * 64),
            ),
            "bytecode differs": (
                1,
                None,
                both(fresh_bin, lambda r: r.update(bytecode_sha256="9" * 64)),
                lambda c: c.update(solar_sha256="f" * 64),
            ),
            "Cargo.lock differs": (1, None, build("cargo_lock_sha256", "8" * 64), None),
            "toolchain differs": (
                1,
                None,
                build("rustc_vv", "rustc 1.101.0\nhost: x86_64-unknown-linux-gnu"),
                None,
            ),
            "flags differ": (
                1,
                None,
                build(
                    "rustflags",
                    {
                        "RUSTFLAGS": "-Copt-level=3",
                        "CARGO_ENCODED_RUSTFLAGS": None,
                        "CARGO_BUILD_RUSTFLAGS": None,
                    },
                ),
                None,
            ),
            "source tree differs": (1, None, build("tree", "0" * 40), None),
            "stale producer": (
                1,
                lambda r: r["solar"].update(binary_commit="4" * 40),
                None,
                None,
            ),
            "substituted executable (record)": (
                1,
                lambda r: r["solar"].update(binary_sha256="d" * 64),
                None,
                None,
            ),
            "substituted executable (fresh)": (1, None, fresh_bin, None),
            "old record without provenance": (1, unversioned, None, None),
            "fresh record without provenance": (1, None, unversioned, None),
            "dirty checkout": (1, lambda r: r["solar"].update(dirty=True), None, None),
            "artifact is not the recorded binary": (
                1,
                lambda r: r["solar"]["build"]["artifact"].update(sha256="c" * 64),
                None,
                None,
            ),
            "Cargo config differs": (
                1,
                None,
                build(
                    "cargo_config",
                    [
                        {
                            "scope": "cargo_home",
                            "name": "config.toml",
                            "sha256": "2" * 64,
                        }
                    ],
                ),
                None,
            ),
            "Cargo environment differs": (
                1,
                None,
                build("cargo_env", {"CARGO_INCREMENTAL": "1"}),
                None,
            ),
            "lifecycle binding differs": (
                1,
                None,
                lambda r: r.setdefault("lifecycle", {}).update(
                    bind={"source_module": "Verify.LoopSource.Equiv.CounterCreated"}
                ),
                None,
            ),
            **{
                f"lifecycle record without {k}": (
                    1,
                    lambda r, k=k: r.update(
                        requested_tier="lifecycle",
                        lifecycle={x: v for x, v in LIFE_PROV.items() if x != k},
                    ),
                    None,
                    None,
                )
                for k in ("bind", "args", "creation_sha256")
            },
            "old provenance version 1": (
                1,
                lambda r: r["solar"]["build"].update(provenance_version=1),
                None,
                None,
            ),
        }
        got = {}
        for label, (want, edit, fe, fce) in cases.items():
            with self.subTest(label):
                code, line = self.pin_case(edit, fe, fce)
                got[label] = f"{code} {line}"
                self.assertEqual(code, want, line)
        snap = HERE / "testdata" / "replay_pin.snapshot"
        text = "".join(f"{k}: {v}\n" for k, v in got.items())
        if os.environ.get("REPLAY_PIN_SNAPSHOT") == "1":
            snap.write_text(text)
        self.assertEqual(text, snap.read_text())

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
            # Another compiler: the fresh run's solar built from another commit. (Its binary digest
            # alone is not compared across runs: test_replay_pins.)
            "stale producer": lambda r: r.setdefault("solar", {}).update(
                binary_commit="0" * 40
            ),
            "solar.commit": lambda r: (
                r["solar"].update(commit="0" * 40, binary_commit="0" * 40),
                r["solar"]["build"].update(commit="0" * 40),
            ),
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
        # Without succeeds-if conditions a spec'd function is a candidate succeeds witness, chosen
        # after the probe (test_succeeds_witness.py); with no spec'd function at all the run is
        # refused before any build, as before.
        f = {"name": "f", "mutates": True, "specified": False, "succeeds": False}
        with self.assertRaises(generic.Failed) as e:
            generic.spec_witness([f])
        self.assertIn("spec: no function has @custom:succeeds-if", str(e.exception))
        self.assertEqual(
            generic.spec_witness([dict(f, specified=True)]), generic.SYNTH_WITNESS
        )

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
            "BLOCKED: recursion (1x, first line 5: unsupported: decrement can call itself "
            "(no @custom:decreases measure",
            line,
        )

    def test_self_recursion_with_measure_admitted(self):
        # One function calling itself with its measure written: the backend stages cut it at its
        # call frames (rec_cuts.py), so the envelope lets it through.
        _code, line = self.blocked("""    /// @custom:decreases n
    function sum(uint256 n) internal pure returns (uint256) { return n == 0 ? 0 : sum(n - 1) + n; }
    function total(uint256 n) external pure returns (uint256) { require(n <= 64); return sum(n); }""")
        self.assertNotIn("recursion", line)

    def test_mutual_recursion_with_measures_refused(self):
        code, line = (
            self.blocked("""    /// @custom:decreases n
    function even(uint256 n) internal pure returns (bool) { return n == 0 ? true : odd(n - 1); }
    /// @custom:decreases n
    function odd(uint256 n) internal pure returns (bool) { return n == 0 ? false : even(n - 1); }
    function f(uint256 n) external pure returns (bool) { return even(n); }""")
        )
        self.assertEqual(code, 3, line)
        self.assertIn(
            "unsupported: even and odd can call each other (only one function calling itself, "
            "with @custom:decreases, has cut points)",
            line,
        )

    def test_overloaded_self_recursion_refused(self):
        # The measure must belong to the one function of that name.
        code, line = self.blocked("""    /// @custom:decreases n
    function g(uint256 n) internal pure returns (uint256) { return n; }
    function g(uint256 n, uint256 m) internal pure returns (uint256) { return n == 0 ? m : g(n - 1, m); }
    function f(uint256 n) external pure returns (uint256) { return g(n, 0) + g(n); }""")
        self.assertEqual(code, 3, line)
        self.assertIn("unsupported: g can call itself (no @custom:decreases measure", line)

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

    def test_recursion_through_a_free_function(self):
        with tempfile.TemporaryDirectory() as t:
            code, line = run_driver(
                HEAD
                + "function e(uint256 n) pure returns (uint256) { return n == 0 ? 1 : e(n - 1); }\n"
                + "contract C {\n    function f(uint256 n) external pure returns (uint256) { return e(n); }\n}\n",
                "none",
                pathlib.Path(t),
            )
        self.assertEqual(code, 3, line)
        self.assertIn("unsupported: e can call itself", line)

    def test_unreached_free_function_recursion_passes(self):
        with tempfile.TemporaryDirectory() as t:
            _code, line = run_driver(
                HEAD
                + "function e(uint256 n) pure returns (uint256) { return n == 0 ? 1 : e(n - 1); }\n"
                + "contract C {\n    function f(uint256 n) external pure returns (uint256) { return n + 1; }\n}\n",
                "none",
                pathlib.Path(t),
            )
        self.assertNotIn("recursion", line)


class BytecodeScan(unittest.TestCase):
    def test_unlinked_library_placeholder(self):
        # PUSH20 <placeholder> DELEGATECALL: the placeholder is no code, the scan goes on after it.
        hexcode = "73" + "__$" + "ab" * 17 + "$__" + "f4"
        self.assertEqual(
            generic.scan_bytecode(hexcode), {"unlinked library": 1, "DELEGATECALL": 21}
        )

    def test_plain_code(self):
        self.assertEqual(generic.scan_bytecode("6001600201f1"), {"CALL": 5})


class ImportClosure(unittest.TestCase):
    def test_relative_and_base_imports(self):
        with tempfile.TemporaryDirectory() as t:
            t = pathlib.Path(t)
            (t / "aux" / "deep").mkdir(parents=True)
            (t / "main.sol").write_text(
                '// import "./ignored.sol";\nimport {A as B} from "./aux/a.sol";\nimport "base.sol";\n'
                'import "./missing.sol";\ncontract C {}\n'
            )
            (t / "aux" / "a.sol").write_text('import * as D from "./deep/d.sol";\n')
            (t / "aux" / "deep" / "d.sol").write_text('import "../a.sol";\n')
            (t / "base.sol").write_text("")
            (t / "ignored.sol").write_text("")
            got = [
                p.relative_to(t.resolve()).as_posix()
                for p in generic.import_closure(t / "main.sol")
            ]
        self.assertEqual(got, ["main.sol", "aux/a.sol", "base.sol", "aux/deep/d.sol"])


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
    nothing to rewrite; the emitted proof must allow that: tree_succeeds_try, not tree_succeeds."""

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
        self.assertIn(
            "  tree_succeeds_try [set_tree, set_spec, M.spec, B, M.ctx]\n", text
        )


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

    def drive(
        self,
        example,
        args,
        res,
        ast=None,
        bins=None,
        frame=False,
        build=None,
        record=None,
    ):
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
            # The clone's checkout holds the base's lakefile.
            if args[:1] == ("checkout",):
                pathlib.Path(cwd).mkdir(parents=True, exist_ok=True)
                (pathlib.Path(cwd) / "lakefile.lean").write_text("-- lakefile\n")
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
        record = {} if record is None else record
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
                    "x_0000abcd",
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
        self.assertIn(
            "Certified.x_0000abcd.Lifecycle.statement", rec["lifecycle"]["statement"]
        )

    def test_kernel_proves_with_runtime_loops_open(self):
        """A source with a runtime loop: the lifecycle tier's CERTIFIED names the loops OPEN and
        that their fixed-fuel resource obligation is not discharged."""
        opened = [{"feature": "loop", "where": "LoopRuntime.sol:13", "detail": "d"}]
        seen, _rec = self.drive(
            "NoArgCtor",
            None,
            REAL["NoArgCtor"][1],
            self.ctor_ast("NoArgCtor", []),
            frame=True,
            build=lambda mod, text: (mod == "Lifecycle", ""),
            record={"lifecycle_open": opened},
        )
        self.assertEqual((seen["status"], seen["code"]), ("CERTIFIED", 0))
        self.assertIn(
            "; runtime loops OPEN, outside this tier (LoopRuntime.sol:13): their fixed-fuel "
            "resource obligation is not discharged",
            seen["line"],
        )

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


class SpecConnectLoops(unittest.TestCase):
    """--loops reads the loop erasure's probe lines, refuses what the loop bridge does not prove,
    and states each loop function's premises of the loop route from a kernel check."""

    def args(self, **kw):
        return argparse.Namespace(
            **{"any_caller": False, "tree_only": False, "ghost_fixed": False, **kw}
        )

    def test_parse_and_refusals(self):
        import spec_connect

        ok = spec_connect.parse_loop("loop C.f 0 2987308228 [256, 160] TREE .stop")
        self.assertEqual(ok, (0, 2987308228, "[256, 160]", ".stop"))
        self.assertIsNone(spec_connect.loop_refusal(ok, 2, ([], []), self.args()))
        for why, word in (
            ("noloop", "eraseLoopStmt"),
            ("nowidths", "widthsOf"),
            ("returns", "wordBinding"),
        ):
            got = spec_connect.parse_loop(f"loop C.g {why}")
            self.assertIn(
                word, spec_connect.loop_refusal(got, 1, ([], []), self.args())
            )
        self.assertIn(
            "probe", spec_connect.loop_refusal(None, 1, ([], []), self.args())
        )
        self.assertIn("eight", spec_connect.loop_refusal(ok, 9, ([], []), self.args()))
        self.assertIn(
            "selCall",
            spec_connect.loop_refusal(ok, 2, ([], []), self.args(any_caller=True)),
        )
        succ = spec_connect.parse_loop("loop C.f 1 7 [] TREE .stop")
        self.assertIn(
            "succeeds-if", spec_connect.loop_refusal(succ, 0, ([], []), self.args())
        )
        self.assertIsNone(
            spec_connect.loop_refusal(succ, 0, ([], []), self.args(tree_only=True))
        )
        self.assertIn(
            "mappings", spec_connect.loop_refusal(ok, 2, ([3], []), self.args())
        )

    def test_loop_block(self):
        import spec_connect

        for fixed in (False, True):
            lines = spec_connect.loop_block(
                "f", "C.f", 1, 1, 7, "[256]", ".stop", "NS", "M", "B", "u", fixed, False
            )
            text = "\n".join(lines)
            self.assertIn(
                "TreeLoop.contractLoopCheck NS.contract? NS.fuel 7 1 [256] f_tree spec 1",
                text,
            )
            self.assertIn("kernel_rfl f_loop_eq f_loop? true using u", text)
            self.assertIn("TreeLoop.LoopSpec c NS.fuel 7 [256] f_tree spec :=", text)
            self.assertIn(
                "loopSpec_of_check_fixed" if fixed else "loopSpec_of_check hc", text
            )
            self.assertIn("#print axioms f_loop_spec", text)
            self.assertNotIn("sorry", text)
        tree = "\n".join(
            spec_connect.loop_block(
                "f", "C.f", 1, 1, 7, "[256]", ".stop", "NS", "M", "B", "u", False, True
            )
        )
        self.assertIn("f_loop_eq", tree)
        self.assertNotIn("f_meets", tree)

    def test_provenance(self):
        """--loops states the spec of the bundle it names only when that bundle is the core
        module's contract's export: named alike, of the file the contract was imported from, with
        that file's SHA-256."""
        import spec_connect

        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            (root / "C.sol").write_text("contract C {}\n")
            sha = hashlib.sha256(b"contract C {}\n").hexdigest()
            (root / "Core").mkdir()
            (root / "Core" / "Src.lean").write_text("import Core.Ast\n")
            (root / "Core" / "Ast.lean").write_text(
                'namespace X\nnamespace Y\ndef importedSourceName : String := "C.sol"\nend Y\nend X\n'
            )
            (root / "Lib" / "gen").mkdir(parents=True)
            spec = (
                'def manifest : Manifest :=\n  { source := "C.sol",\n'
                f'    sourceSha256 := "{sha}",\n    compiler := "" }}\n'
            )
            (root / "Lib" / "gen" / "Spec.lean").write_text(spec)
            self.assertEqual(
                spec_connect.module_path(root, "gen.Spec"), root / "Lib/gen/Spec.lean"
            )
            sources = spec_connect.core_source(root, "Core.Src")
            self.assertEqual(sources, [("X.Y.importedSourceName", "C.sol")])
            manifest = spec_connect.manifest_of(spec)
            ok = spec_connect.provenance_refusal(("C", "C"), manifest, sources, root)
            self.assertIsNone(ok)
            for names, man, srcs, word in (
                (("D", "C"), manifest, sources, "not of the checked contract"),
                (None, manifest, sources, "names"),
                (("C", "C"), None, sources, "manifest"),
                (("C", "C"), manifest, [], "no single importedSourceName"),
                (("C", "C"), manifest, sources * 2, "no single importedSourceName"),
                (
                    ("C", "C"),
                    manifest,
                    [("A.importedSourceName", "D.sol")],
                    "imported from D.sol",
                ),
                (("C", "C"), ("C.sol", "0" * 64), sources, "SHA-256"),
                (
                    ("C", "C"),
                    ("E.sol", sha),
                    [("A.importedSourceName", "E.sol")],
                    "not in the checkout",
                ),
            ):
                self.assertIn(
                    word, spec_connect.provenance_refusal(names, man, srcs, root)
                )

    BUNDLE = (
        "def C : Bundle where\n"
        "  fuel := 0\n"
        "  spec :=\n"
        '  { name := "C",\n'
        "    invariants := [\n"
        "    (.bin .le (.lit 0) (.lit 1))],\n"
        "    specs := [\n"
        '    { name := "C.f",\n'
        "      ensures := [\n"
        "        (.bin .eq (.arg 0) (.arg 0))],\n"
        "      succeedsIf := [] }] }\n"
    )

    def fresh_root(self, d, sha_ok=True, exports=True):
        """A checkout with C.sol, a front end that prints an AST body (and refuses unless it runs as
        a copy, with its caches off and Python isolated), and a solar-verify that exports BUNDLE with
        the source's SHA-256 (or a wrong one). Returns the root, solar-verify's path and its
        SHA-256."""
        root = pathlib.Path(d)
        (root / "C.sol").write_text("contract C {}\n")
        (root / "scripts").mkdir()
        (root / "scripts" / "harness_cache.py").write_text("# cache\n")
        (root / "scripts" / "solc_ast_to_lean_source.py").write_text(
            "import os, pathlib, sys\n"
            f"if pathlib.Path(__file__).resolve().parent == pathlib.Path({str(root.resolve() / 'scripts')!r}):\n"
            "    sys.exit('run from the checkout, not a copy')\n"
            "if os.environ.get('CONTEST_DISABLE_CACHE') != '1' or 'CONTEST_CACHE_DIR' in os.environ:\n"
            "    sys.exit('caches on')\n"
            "if not sys.flags.isolated:\n"
            "    sys.exit('not isolated')\n"
            "if not (pathlib.Path(__file__).parent / 'harness_cache.py').is_file():\n"
            "    sys.exit('no harness_cache.py beside the copy')\n"
            "solc = sys.argv[sys.argv.index('--solc') + 1]\n"
            "if open(solc, 'rb').read() != b'solc':\n"
            "    sys.exit('not the pinned solc')\n"
            "ns = sys.argv[sys.argv.index('--namespace') + 1]\n"
            "print('namespace ' + ns)\n"
            "print('def importedSourceName : String := \"/abs/C.sol\"')\n"
            "print('def importedSourceUnit : SourceUnit := { items := [] }')\n"
            "print('end ' + ns)\n"
        )
        verify = root / "fake-solar-verify"
        verify.write_text(
            "#!/usr/bin/env python3\n"
            "import hashlib, sys\n"
            f"if not {exports!r}:\n"
            "    sys.exit('error: cannot export to Lean: no')\n"
            "out, src = sys.argv[2], sys.argv[-1]\n"
            "sha = hashlib.sha256(open(src, 'rb').read()).hexdigest()\n"
            f"sha = sha if {sha_ok!r} else '0' * 64\n"
            "text = ('def manifest : Manifest :=\\n  { source := \"' + src + '\",\\n'\n"
            "        '    sourceSha256 := \"' + sha + '\",\\n    compiler := \"\" }\\n\\n')\n"
            f"open(out, 'w').write(text + {self.BUNDLE!r} + '\\n/-- wf -/\\n')\n"
        )
        verify.chmod(0o755)
        (root / "solc").write_bytes(b"solc")
        return root, str(verify), hashlib.sha256(verify.read_bytes()).hexdigest()

    def export(self, root, verify, vsha, spec, cname="C", data=None, solc=None):
        """fresh_export of C.sol's bytes (or `data`) with the pinned tools of `root`."""
        import spec_connect

        exporter = spec_connect.pinned_exporter(verify, vsha)
        self.assertNotIsInstance(exporter, str, exporter)
        front = spec_connect.front_end(root)
        self.assertNotIsInstance(front, str, front)
        if solc is None:
            solc = (str(root / "solc"), "0.8.35", hashlib.sha256(b"solc").hexdigest())
        if data is None:
            data = (root / "C.sol").read_bytes()
        return spec_connect.fresh_export(
            "C.sol", data, cname, "M", spec, exporter, solc, front, "Out.FreshSource"
        )

    def test_bundle_block(self):
        import spec_connect

        text = (
            "x\n\n/-- The specs. -/\n"
            + self.BUNDLE
            + "\ntheorem C_wf : C.WF = true := by decide\n"
        )
        self.assertEqual(spec_connect.bundle_block(text, "C"), self.BUNDLE)
        self.assertIsNone(spec_connect.bundle_block(text, "D"))

    def test_fresh_export(self):
        """--loops binds the bundle's contents to the source bytes: the spec module's bundle must be
        solar-verify's export of those bytes in this run, character for character; removing the
        ensures or the invariant is refused. The tools run as copies of the bytes hashed."""
        spec = "/-- s -/\n" + self.BUNDLE + "\n"
        with tempfile.TemporaryDirectory() as d:
            root, verify, vsha = self.fresh_root(d)
            got = self.export(root, verify, vsha, spec)
            self.assertNotIsInstance(got, str, got)
            block, bsha, sha, ast = got
            self.assertEqual(block, self.BUNDLE)
            self.assertEqual(bsha, hashlib.sha256(self.BUNDLE.encode()).hexdigest())
            self.assertEqual(sha, hashlib.sha256(b"contract C {}\n").hexdigest())
            self.assertIn("namespace Out.FreshSource", ast)
            self.assertIn("importedSourceUnit", ast)
            self.assertNotIn("importedSourceName", ast)
            # The snapshot's bytes are exported, not the file's now.
            other = self.export(root, verify, vsha, spec, data=b"contract C { }\n")
            self.assertEqual(other[2], hashlib.sha256(b"contract C { }\n").hexdigest())
            for old, new in (
                (
                    "      ensures := [\n        (.bin .eq (.arg 0) (.arg 0))],\n",
                    "      ensures := [],\n",
                ),
                (
                    "    invariants := [\n    (.bin .le (.lit 0) (.lit 1))],\n",
                    "    invariants := [],\n",
                ),
                ("(.arg 0) (.arg 0)", "(.arg 0) (.arg 1)"),
            ):
                self.assertIn(old, spec)
                why = self.export(root, verify, vsha, spec.replace(old, new))
                self.assertIsInstance(why, str)
                self.assertIn("not solar-verify's export of C.sol in this run", why)
            self.assertIn("defines no bundle", self.export(root, verify, vsha, ""))
            self.assertIn(
                "cannot be regenerated",
                self.export(root, verify, vsha, spec, solc="no solc"),
            )
            self.assertIn(
                "changed after",
                self.export(
                    root,
                    verify,
                    vsha,
                    spec,
                    solc=(str(root / "solc"), "0.8.35", "0" * 64),
                ),
            )
            self.assertIn("no bundle `D`", self.export(root, verify, vsha, spec, "D"))
        with tempfile.TemporaryDirectory() as d:
            root, verify, vsha = self.fresh_root(d, sha_ok=False)
            self.assertIn(
                "does not record its SHA-256", self.export(root, verify, vsha, spec)
            )
        with tempfile.TemporaryDirectory() as d:
            root, verify, vsha = self.fresh_root(d, exports=False)
            self.assertIn("does not export", self.export(root, verify, vsha, spec))

    def test_pinned_exporter(self):
        """solar-verify runs only when its bytes have the pinned SHA-256."""
        import spec_connect

        with tempfile.TemporaryDirectory() as d:
            root, verify, vsha = self.fresh_root(d)
            data, got = spec_connect.pinned_exporter(verify, vsha)
            self.assertEqual((data, got), (pathlib.Path(verify).read_bytes(), vsha))
            self.assertIn(
                "not the pinned solar-verify",
                spec_connect.pinned_exporter(verify, "0" * 64),
            )
            self.assertIn(
                "no pinned SHA-256", spec_connect.pinned_exporter(verify, None)
            )
            self.assertIn(
                "no pinned SHA-256", spec_connect.pinned_exporter(verify, "ab")
            )
            self.assertIn("no solar-verify", spec_connect.pinned_exporter(None, vsha))
            self.assertIn(
                "not a file", spec_connect.pinned_exporter(str(root / "nope"), vsha)
            )
            (root / "scripts" / "harness_cache.py").unlink()
            self.assertIn("missing", spec_connect.front_end(root))

    def test_snapshot_source(self):
        """The source the manifest names is a relative path inside the checkout, read once; the
        provenance gate checks the snapshot's bytes when given."""
        import spec_connect

        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as out:
            root = pathlib.Path(d)
            (root / "C.sol").write_bytes(b"contract C {}\n")
            (pathlib.Path(out) / "D.sol").write_bytes(b"contract D {}\n")
            (root / "L.sol").symlink_to(pathlib.Path(out) / "D.sol")
            self.assertEqual(
                spec_connect.snapshot_source(root, "C.sol"), b"contract C {}\n"
            )
            for src, word in (
                (str(root / "C.sol"), "not a relative path"),
                ("../C.sol", "not a relative path"),
                ("a/../../C.sol", "not a relative path"),
                ("", "not a relative path"),
                ("L.sol", "out of the checkout"),
                ("E.sol", "not in the checkout"),
            ):
                self.assertIn(word, spec_connect.snapshot_source(root, src), src)
            sha = hashlib.sha256(b"contract C {}\n").hexdigest()
            srcs = [("A.importedSourceName", "C.sol")]
            self.assertIsNone(
                spec_connect.provenance_refusal(
                    ("C", "C"), ("C.sol", sha), srcs, root, b"contract C {}\n"
                )
            )
            self.assertIn(
                "not the export's",
                spec_connect.provenance_refusal(
                    ("C", "C"), ("C.sol", sha), srcs, root, b"contract C { }\n"
                ),
            )
            # The gate after Lean: a file changed since the snapshot is not the receipt's bytes.
            (root / "C.sol").write_bytes(b"contract C { uint x; }\n")
            self.assertIn(
                "not the export's",
                spec_connect.provenance_refusal(("C", "C"), ("C.sol", sha), srcs, root),
            )
            absolute = [("A.importedSourceName", str(root / "C.sol"))]
            self.assertIn(
                "not a relative path",
                spec_connect.provenance_refusal(
                    ("C", "C"), (str(root / "C.sol"), sha), absolute, root
                ),
            )

    def test_pinned_solc(self):
        import spec_connect

        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            self.assertIn("missing", spec_connect.pinned_solc(root))
            (root / "scripts" / "refinement").mkdir(parents=True)
            solc = root / "solc"
            solc.write_bytes(b"solc")
            sha = hashlib.sha256(b"solc").hexdigest()
            (root / "scripts/refinement/gen.sh").write_text(
                f"solc_version=0.8.35\nsolc_sha256={sha}\n"
            )
            self.assertEqual(
                spec_connect.pinned_solc(root, solc), (str(solc), "0.8.35", sha)
            )
            solc.write_bytes(b"other")
            self.assertIn("not the pinned solc", spec_connect.pinned_solc(root, solc))

    def test_fresh_block(self):
        """The module names this run's export as its spec and has the kernel check the spec
        module's bundle and the checked contract against it."""
        import spec_connect

        text = "\n".join(
            spec_connect.fresh_block(
                "Out",
                "NS",
                "Lib.M.C",
                "C",
                "C.sol",
                "ab" * 32,
                self.BUNDLE,
                "cd" * 32,
                "0.8.35",
                {
                    "exporterSha256": "e" * 64,
                    "astToolSha256": "f" * 64,
                    "astToolCacheSha256": "1" * 64,
                    "solcSha256": "2" * 64,
                },
                "NS.selector_eq",
            )
        )
        self.assertIn("def exported : Bundle where\n  fuel := 0", text)
        self.assertIn("theorem spec_is_fresh : Lib.M.C = exported := rfl", text)
        # The canonical type checker, its instance given: no instance found at elaboration.
        explicit = (
            "@SolidCore.Solidity.TypeCheck.CheckedInput.contract SolidCore.Solidity.TypeCheck.SourceUnitAst\n"
            "      SolidCore.Solidity.TypeCheck.checkedInputSourceUnit"
        )
        self.assertIn(
            "theorem source_is_fresh : NS.contract? =\n    "
            + explicit
            + ' Out.FreshSource.importedSourceUnit "C" :=\n  rfl',
            text,
        )
        self.assertNotIn(
            " SolidCore.Solidity.TypeCheck.CheckedInput.contract Out.FreshSource", text
        )
        # The changed-AST negative: the kernel checks that the canonical checker on a source unit
        # without the contract does not give the core module's contract.
        self.assertIn(
            "theorem source_is_fresh_discriminates : NS.contract? ≠\n    "
            + explicit
            + ' ({ items := [] } : SolidCore.Solidity.SourceUnit) "C" := by',
            text,
        )
        self.assertIn("  rw [h, hr] at hl\n  exact Bool.false_ne_true hl", text)
        self.assertIn(
            "kernel_rfl contractOk_eq contractOk? true using NS.selector_eq", text
        )
        self.assertIn("kernel_rfl emptyOk_eq emptyOk? false using NS.selector_eq", text)
        for k, v in (
            ("sourceSha256", "ab" * 32),
            ("exporterSha256", "e" * 64),
            ("astToolSha256", "f" * 64),
            ("astToolCacheSha256", "1" * 64),
            ("solcSha256", "2" * 64),
        ):
            self.assertIn(f'def {k} : String := "{v}"', text)
        self.assertIn('def exportedSha256 : String := "' + "cd" * 32 + '"', text)
        self.assertIn("#print axioms source_is_fresh", text)
        self.assertNotIn("sorry", text)
        lines = spec_connect.loop_block(
            "f",
            "C.f",
            1,
            1,
            7,
            "[256]",
            ".stop",
            "NS",
            "Out",
            "Out.exported",
            "u",
            False,
            False,
        )
        self.assertIn(
            "tree_meets [f_tree, f_spec, Out.spec, Out.exported, Out.ctx",
            "\n".join(lines),
        )

    def test_overloads_refused(self):
        import spec_connect

        lines = ["fn C.f 1 noerase", "fn C.g 0 noerase", "fn C.f 2 noerase"]
        self.assertEqual(spec_connect.duplicate_names(lines), ["C.f"])
        self.assertEqual(spec_connect.lean_string('"C.f\\"x"'), 'C.f"x')


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
            lines = spec_connect.from_block(
                "f", "C.f", 2, nsi, "NS", "M", "B", "[256, 256]", "u"
            )
            text = "\n".join(lines)
            self.assertIn("theorem f_holds_from", text)
            self.assertIn("holdsFn_of_tree_from hdd", text)
            self.assertIn("#print axioms f_holds_from", text)
            self.assertNotIn("0x2000", text)
            self.assertNotIn(" ctxOf ", text)
            self.assertNotIn("sorry", text)

    def test_tree_only_any_caller(self):
        import spec_connect

        mapped = spec_connect.parse_sym("sym C.f false some [160, 256] TREE .stop")
        self.assertIsNone(
            spec_connect.any_caller_refusal(mapped[1:], ([1], []), tree_only=True)
        )
        none = spec_connect.parse_sym("sym C.h noerase")
        self.assertIn(
            "eraseFnSym",
            spec_connect.any_caller_refusal(none[1:], ([1], []), tree_only=True),
        )
        lines = spec_connect.from_block(
            "f", "C.f", 2, 1, "NS", "M", "B", "[160, 256]", "u", False, True
        )
        text = "\n".join(lines)
        self.assertIn("def f_tree_from", text)
        self.assertIn("kernel_rfl f_erase_from_eq", text)
        self.assertIn(
            "some 2, some f_tree_from, false, some [160, 256]".replace("some 2", "(2"),
            text,
        )
        self.assertNotIn("theorem", text)


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
    """The lifecycle kernel stage leaves the probe and the lakefile stanza byte-identical to the
    driver it is built on (BASE_DRIVER), and the envelope unchanged for a source without
    constructor parameters. The generated source check changed on purpose since (its tables and
    proofs by library lemmas, LoweringTemplate), so it is checked there instead."""

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
        for m in (old, generic):
            m.out = [
                m.probe_lean("x_0000abcd", "C", fns),
                m.lakefile_lib("x_0000abcd", "C"),
            ]
        # The one change on purpose since: the probe compares the two trees as the stage does
        # (Generic.Tree.agree, Stage.checkFn), not by equal canonical forms alone.
        agree = "if Generic.Tree.agree a b then"
        self.assertEqual(old.out[0].count("if a.canon.beq b.canon then"), 2 * len(fns))
        old.out[0] = old.out[0].replace("if a.canon.beq b.canon then", agree)
        self.assertEqual(generic.out[0].count(agree), 2 * len(fns))
        # And the run's stages are exported under the run's own namespace (export_ns.py).
        old.out[0] = old.out[0].replace(
            "SolarTV.Export.C.", "SolarTV.Export.x_0000abcd.C."
        )
        # And the probe says why the erasure is undefined when a parameter or a result is not one
        # of its words: the static ABI envelope (static_words) admits those, and the erasure decides.
        why = (
            '"source tree undefined (erasure)" ++ (match Generic.widthsOf fn.params fn.paramAbiCleanups with\n'
            '        | none => ": a parameter is not a word"\n'
            '        | some _ => if fn.returns.all Generic.wordBinding then "" else ": a result is not a word")\n'
        )
        self.assertEqual(old.out[0].count('"source tree undefined (erasure)"\n'), 2 * len(fns))
        old.out[0] = old.out[0].replace('"source tree undefined (erasure)"\n', why)
        # The other: the succeeds-witness search's runs of every function on the zero call
        # (synth_witness), printed after the checks and nothing else changed.
        new = generic.out[0]
        self.assertEqual(new.count('PROBE run fn '), len(fns))
        i, j = new.index("\n  let st0 :"), new.index("\n\nend Certified.x_0000abcd.Probe")
        generic.out[0] = new[:i] + new[j:]
        self.assertEqual(old.out, generic.out)
        if SOLC.exists():
            src = LIFE / "NoArgCtor.sol"
            ast, contracts = generic.solc_ast(str(SOLC), src)
            self.assertEqual(
                old.envelope(src.read_text(), ast, contracts, None, None, False),
                generic.envelope(src.read_text(), ast, contracts, None, None, False),
            )


class LoweringTemplate(unittest.TestCase):
    """The generated modules state each Keccak table as data (KernelTable.tableOr) and prove it
    in one line from one equation per entry, whatever the number of entries; the source check's
    deployed_of and the lifecycle's statement are one library lemma each (Generic.shapeOf,
    Verify.Lifecycle.lifeOf), with no tactic script."""

    FNS: typing.ClassVar = [
        {"sig": "get()", "sel": 0x6D4CE63C, "params": []},
        {"sig": "set(uint256)", "sel": 0x60FE47B1, "params": [".uint 256"]},
    ]

    def test_lowering(self):
        events = [{"sig": "E(uint256)", "topic": 0x1234}]
        t = generic.lowering_lean("x_0000abcd", "C", self.FNS, events, "core", "[]")
        for want in (
            "import SolidCore.Refinement.KernelTable\n",
            "import SolidCore.Refinement.Generic.Shape\n",
            (
                "def selectorTable : String → SolidCore.Solidity.Shared.Word :=\n"
                "  KernelTable.tableOr SolidCore.Solidity.Source.ABI.selectorFromSignature\n"
                '    [("get()", sel0),\n     ("set(uint256)", sel1)]\n'
            ),
            (
                "theorem selector_eq : SolidCore.Solidity.Source.ABI.selectorFromSignature = "
                "selectorTable :=\n  KernelTable.eq_tableOr ⟨selSig0_eq, selSig1_eq, trivial⟩\n"
            ),
            '    [("E(uint256)", 0x1234)]\n',
            "  KernelTable.eq_tableOr ⟨topicSig0_eq, trivial⟩\n",
            "def shape? : Option (Contract × Bool) := Generic.shapeOf fuel contract? storageLit\n",
            "kernel_rfl shape_eq shape? (some (coreLit, true)) using selector_eq digest_eq\n",
            "  Generic.deployed_of_shapeOf shape_eq hc\n",
        ):
            self.assertIn(want, t)
        for gone in ("split", "unfold", "by\n  have"):
            self.assertNotIn(gone, t)
        sym = generic.lowering_lean(
            "x_0000abcd", "C", self.FNS, events, "core", "[(0, 1)]", sym=True
        )
        self.assertIn("theorem check_sym_stage0", sym)
        self.assertNotIn("check_sym_stage0", t)

    def test_no_entries(self):
        t = generic.lowering_lean("x_0000abcd", "C", [], [], "core", "[]")
        self.assertIn("    []\n", t)
        self.assertIn("  KernelTable.eq_tableOr trivial\n", t)
        self.assertNotIn("digestTable", t)
        self.assertIn("using selector_eq\n", t)

    def test_lifecycle(self):
        sigs = [{"sig": f["sig"], "sel": f["sel"]} for f in self.FNS]
        code = bytes.fromhex("6001600055")
        args = (
            "Certified.T.X",
            "M",
            "M.importedSourceUnit",
            "X",
            sigs,
            code,
            code,
            code,
        )
        t = generic.lifecycle_lean(*args, [7], [(0, 7)])
        for want in (
            "import Verify.Lifecycle.Life\n",
            '    [("get()", 0x6d4ce63c),\n     ("set(uint256)", 0x60fe47b1)]\n',
            "  KernelTable.eq_tableOr ⟨selSig0_eq, selSig1_eq, trivial⟩\n",
            "def life? : Bool := lifeOf fuel contract? ws storageLit\n",
            "kernel_rfl life_eq life? true using selector_eq mkIdx_eq\n",
            "  statement_of_lifeOf deploys life_eq\n",
        ):
            self.assertIn(want, t)
        self.assertNotIn("split", t)
        r = generic.lifecycle_lean(*args, [7], [(0, 7)], refute=True)
        self.assertIn("theorem refuted", r)
        self.assertNotIn("selectorTable", r)


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
    """The examples through solc: static word constructor parameters enter the other tiers'
    envelope (every argument list, Verify/Generic/CtorWords.lean) without a specification and
    without immutables, and the lifecycle tier reads their types and the immutables."""

    def test_examples(self):
        want = {
            "NoArgCtor": ([], [], None),
            "ArgCtor": (["uint256", "address"], [], [256, 160]),
            "PairCtor": (["uint256", "uint256"], [], [256, 256]),
            "ImmutableCtor": (["uint256"], ["limit"], "immutable limit"),
        }
        for ex, (types, imm, ctor) in want.items():
            src = LIFE / f"{ex}.sol"
            ast, contracts = generic.solc_ast(str(SOLC), src)
            self.assertEqual(generic.constructor_params(ast, ex), types, ex)
            self.assertEqual(generic.immutables(ast, ex), imm, ex)
            generic.envelope(src.read_text(), ast, contracts, None, None, False, True)
            if isinstance(ctor, str):
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(
                        src.read_text(), ast, contracts, None, None, False, False
                    )
                self.assertIn(ctor, str(e.exception.reasons))
                continue
            _, fns, _ = generic.envelope(
                src.read_text(), ast, contracts, None, None, False, False
            )
            got = fns.ctor and [q["width"] for q in fns.ctor["params"]]
            self.assertEqual(got, ctor, ex)
            if ctor:
                # With a specification, constructor arguments stay refused.
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(
                        src.read_text(), ast, contracts, None, None, True, False
                    )
                self.assertIn("with a specification", str(e.exception.reasons))

    def test_ctor_param_refusals(self):
        """Constructor parameters outside the static words are refused, each with its type."""
        cases = {
            "string memory s": "a dynamic constructor argument",
            "bytes memory b": "a dynamic constructor argument",
            "uint256[] memory xs": "a dynamic constructor argument",
            "bool f": "only uintN and address words",
            "int256 i": "only uintN and address words",
            "bytes32 h": "only uintN and address words",
            "uint256[2] memory p": "a static array, not one static word",
        }
        with tempfile.TemporaryDirectory() as d:
            for decl, why in cases.items():
                src = pathlib.Path(d) / "C.sol"
                src.write_text(
                    "pragma solidity ^0.8.0;\ncontract C {\n    uint256 public x;\n"
                    f"    constructor({decl}) {{ x = 1; }}\n"
                    "    function get() external view returns (uint256) { return x; }\n}\n"
                )
                ast, contracts = generic.solc_ast(str(SOLC), src)
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(src.read_text(), ast, contracts, None, None, False, False)
                self.assertIn(why, str(e.exception.reasons), decl)

    def test_runtime_loop_is_open_not_rejected(self):
        """A runtime loop (LoopRuntime: Counter's shape) does not stop the lifecycle tier: it is
        recorded OPEN, and no fixed-fuel resource obligation is computed or passed. Every other tier
        still blocks it, and a loop in the constructor still blocks the lifecycle tier."""
        src = LIFE / "LoopRuntime.sol"
        ast, contracts = generic.solc_ast(str(SOLC), src)
        opened = []
        contract, _fns, _ = generic.envelope(
            src.read_text(), ast, contracts, None, None, False, True, opened
        )
        self.assertEqual(contract, "LoopRuntime")
        self.assertEqual([o["feature"] for o in opened], ["loop"])
        self.assertIn("OPEN, not required here", opened[0]["detail"])
        for lifecycle, out in ((False, []), (True, None)):
            with self.assertRaises(generic.Blocked) as e:
                generic.envelope(
                    src.read_text(), ast, contracts, None, None, False, lifecycle, out
                )
            self.assertIn("loop", [r["feature"] for r in e.exception.reasons])
        ctor = LIFE / "LoopCtor.sol"
        ast2, contracts2 = generic.solc_ast(str(SOLC), ctor)
        opened = []
        with self.assertRaises(generic.Blocked) as e:
            generic.envelope(
                ctor.read_text(), ast2, contracts2, None, None, False, True, opened
            )
        self.assertEqual([r["feature"] for r in e.exception.reasons], ["loop"])
        self.assertEqual(opened, [])

    def test_creation_reachable_loops_block(self):
        """A loop creation can run is never opened as a runtime loop: in an internal helper the
        constructor calls, in a modifier on the constructor, in a base constructor, and in a
        function a state variable's initializer calls. Each is BLOCKED at the lifecycle tier."""
        for ex, contract in (
            ("LoopCtorHelper", "LoopCtorHelper"),
            ("LoopCtorModifier", "LoopCtorModifier"),
            ("LoopBaseCtor", "LoopBaseCtor"),
            ("LoopInitializer", "LoopInitializer"),
        ):
            with self.subTest(ex):
                src = LIFE / f"{ex}.sol"
                ast, contracts = generic.solc_ast(str(SOLC), src)
                opened = []
                with self.assertRaises(generic.Blocked) as e:
                    generic.envelope(
                        src.read_text(),
                        ast,
                        contracts,
                        contract,
                        None,
                        False,
                        True,
                        opened,
                    )
                self.assertEqual(opened, [], ex)
                feats = [r["feature"] for r in e.exception.reasons]
                # The base constructor is a creation root, so its loop blocks too.
                self.assertIn("loop", feats)
                if ex == "LoopBaseCtor":
                    self.assertIn("inheritance: base constructor arguments", feats)

    def test_creation_reach_unresolved_is_conservative(self):
        """A base-constructor invocation (or any call creation_reach cannot resolve) marks the
        closure unresolved, so no loop is treated as runtime-only."""
        src = LIFE / "LoopBaseCtor.sol"
        ast, _ = generic.solc_ast(str(SOLC), src)
        cdef = next(n for n in ast["nodes"] if n.get("name") == "LoopBaseCtor")
        _, unresolved = generic.creation_reach(cdef)
        self.assertTrue(unresolved)
        # With the linearization, the base constructor is a root and its loop is reached.
        lin, _ = generic.linearization(ast, cdef)
        reach, unresolved = generic.creation_reach(cdef, lin)
        self.assertFalse(unresolved)
        base = next(n for n in ast["nodes"] if n.get("name") == "LoopBase")
        loop = next(
            n for n in generic.walk(base) if n.get("nodeType") == "ForStatement"
        )
        self.assertIn(id(loop), reach)
        src = LIFE / "LoopRuntime.sol"
        ast, _ = generic.solc_ast(str(SOLC), src)
        cdef = next(n for n in ast["nodes"] if n.get("name") == "LoopRuntime")
        reach, unresolved = generic.creation_reach(cdef)
        self.assertFalse(unresolved)
        loop = next(
            n for n in generic.walk(cdef) if n.get("nodeType") == "ForStatement"
        )
        self.assertNotIn(id(loop), reach)

    def test_runtime_loop_cli_reaches_lifecycle(self):
        """The driver on LoopRuntime with --tier lifecycle passes the envelope with the loop OPEN and
        never computes the fixed-fuel obligation (it stops later, at the absent solidity-lean)."""
        with tempfile.TemporaryDirectory() as t:
            p = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "generic.py"),
                    str(LIFE / "LoopRuntime.sol"),
                    "none",
                    "--tier",
                    "lifecycle",
                    "--constructor-args",
                    "8",
                    "--solidity-lean",
                    "/nonexistent",
                    "--work",
                    t + "/w",
                    "--record-dir",
                    t + "/r",
                    "--solc",
                    str(SOLC),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            out = p.stdout + p.stderr
            self.assertNotIn("resource obligation (fixed fuel)", out)
            self.assertNotIn("out of gas", out)
            self.assertIn("lifecycle: runtime loops OPEN", out)
            rec = json.loads((pathlib.Path(t) / "r" / "record.json").read_text())
            self.assertEqual([o["feature"] for o in rec["lifecycle_open"]], ["loop"])
            self.assertTrue(rec["resources"].startswith("OPEN: "))
            self.assertNotEqual(rec["status"], "CERTIFIED")

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
            (c / "Certified" / "n_0000abcd" / "Proofs").mkdir(parents=True)
            (c / "Certified" / "n_0000abcd" / "Final.lean").write_text("stale")
            (c / "lakefile.lean").write_text(
                "x" + generic.lakefile_lib("n_0000abcd", "C")
            )
            self.assertTrue(generic.fresh_certified(c, "n_0000abcd", "C"))
            self.assertFalse((c / "Certified" / "n_0000abcd").exists())
            self.assertEqual(
                (c / "lakefile.lean").read_text().count("lean_lib «n_0000abcd»"), 1
            )

    def test_new_and_foreign(self):
        import tempfile

        with tempfile.TemporaryDirectory() as t:
            c = pathlib.Path(t)
            (c / "lakefile.lean").write_text("x\n")
            self.assertFalse(generic.fresh_certified(c, "n_0000abcd", "C"))
            self.assertEqual(
                (c / "lakefile.lean").read_text(),
                "x\n" + generic.lakefile_lib("n_0000abcd", "C"),
            )
            (c / "lakefile.lean").write_text(
                "lean_lib «n_0000abcd» where\n  roots := #[]\n"
            )
            with self.assertRaises(generic.Failed):
                generic.fresh_certified(c, "n_0000abcd", "C")


class FreshCertifiedControls(unittest.TestCase):
    """Negative controls of the existing-record repair: a stale generated file is not reused, the
    supplied base is never touched (only its clone), and duplicate or foreign stanzas are handled."""

    def setUp(self):
        import subprocess
        import tempfile

        self.t = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.t.name)
        self.base = root / "base"
        (self.base / "Certified" / "n_0000abcd" / "gen").mkdir(parents=True)
        (self.base / "Certified" / "n_0000abcd" / "gen" / "Stage0.lean").write_text(
            "stale export"
        )
        (self.base / "Certified" / "n_0000abcd" / "Final.lean").write_text(
            "stale final"
        )
        (self.base / "lakefile.lean").write_text(
            "x" + generic.lakefile_lib("n_0000abcd", "C")
        )
        g = ["git", "-c", "user.name=t", "-c", "user.email=t@l"]
        subprocess.run(g + ["init", "-q", str(self.base)], check=True)
        subprocess.run(g + ["-C", str(self.base), "add", "-A"], check=True)
        subprocess.run(g + ["-C", str(self.base), "commit", "-qm", "base"], check=True)
        self.head = subprocess.run(
            ["git", "-C", str(self.base), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        self.clone = root / "clone"
        subprocess.run(
            ["git", "clone", "-q", "--shared", str(self.base), str(self.clone)],
            check=True,
        )

    def tearDown(self):
        self.t.cleanup()

    def base_unchanged(self):
        import subprocess

        st = subprocess.run(
            ["git", "-C", str(self.base), "status", "--porcelain"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        head = subprocess.run(
            ["git", "-C", str(self.base), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        self.assertEqual((st, head), ("", self.head))
        self.assertEqual(
            (self.base / "Certified" / "n_0000abcd" / "Final.lean").read_text(),
            "stale final",
        )

    def test_stale_and_duplicate(self):
        self.assertTrue(generic.fresh_certified(self.clone, "n_0000abcd", "C"))
        # The stale generated files are gone from the clone, so nothing of the base's record is reused.
        self.assertFalse((self.clone / "Certified" / "n_0000abcd").exists())
        self.assertEqual(
            (self.clone / "lakefile.lean").read_text().count("lean_lib «n_0000abcd»"), 1
        )
        self.base_unchanged()

    def test_foreign_stanza(self):
        (self.clone / "lakefile.lean").write_text(
            "lean_lib n_0000abcd where\n  roots := #[]\n"
        )
        with self.assertRaises(generic.Failed):
            generic.fresh_certified(self.clone, "n_0000abcd", "C")
        self.base_unchanged()

    def test_other_names_untouched(self):
        (self.clone / "Certified" / "m_2").mkdir()
        (self.clone / "Certified" / "m_2" / "Final.lean").write_text("other")
        generic.fresh_certified(self.clone, "n_0000abcd", "C")
        self.assertEqual(
            (self.clone / "Certified" / "m_2" / "Final.lean").read_text(), "other"
        )


class LeanLibDeclaredOnce(unittest.TestCase):
    """No path that appends a lean_lib stanza declares NAME a second time: a base whose lakefile
    already declares it, in either spelling, keeps one declaration (the driver's own stanza is
    reused, any other is refused) in the probe clones, the run's clone and the lifecycle build."""

    FOREIGN = "x\nlean_lib n_0000abcd where\n  roots := #[]\n"

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.t.name)
        self.saved = generic.run

    def tearDown(self):
        generic.run = self.saved
        self.t.cleanup()

    def count(self, lf):
        return len(
            re.findall(
                r"^lean_lib (n_0000abcd|«n_0000abcd»)( |$)",
                lf.read_text(),
                re.MULTILINE,
            )
        )

    def base(self, lakefile):
        b = self.root / "base"
        b.mkdir()
        (b / "lakefile.lean").write_text(lakefile)
        g = ["git", "-c", "user.name=t", "-c", "user.email=t@l"]
        subprocess.run(g + ["init", "-q", str(b)], check=True)
        subprocess.run(g + ["-C", str(b), "add", "-A"], check=True)
        subprocess.run(g + ["-C", str(b), "commit", "-qm", "base"], check=True)
        (self.root / "w").mkdir()
        return b

    def lake_unreached(self):
        def fake_run(cmd, *a, **kw):
            # Git still runs; lake would read the lakefile under test.
            if cmd[0] != "lake":
                return self.saved(cmd, *a, **kw)
            raise AssertionError(
                f"lake runs on a lakefile that declares n_0000abcd twice: {cmd}"
            )

        generic.run = fake_run

    def test_spellings(self):
        for text in (
            "lean_lib n_0000abcd where\n",
            "lean_lib n_0000abcd\n",
            "lean_lib «n_0000abcd» where\n",
        ):
            self.assertTrue(generic.lean_lib_declared(text, "n_0000abcd"), text)
        for text in ("lean_lib n_0000abcd0 where\n", "-- lean_lib n_0000abcd where\n"):
            self.assertFalse(generic.lean_lib_declared(text, "n_0000abcd"), text)

    def test_run_clone(self):
        lf = self.root / "lakefile.lean"
        lf.write_text(self.FOREIGN)
        with self.assertRaises(generic.Failed):
            generic.fresh_certified(self.root, "n_0000abcd", "C")
        self.assertEqual(self.count(lf), 1)

    def test_probe_clone(self):
        b = self.base(self.FOREIGN)
        self.lake_unreached()
        with self.assertRaises(generic.Failed):
            generic.run_probe(b, self.root / "w", "n_0000abcd", "C", [], "", None)
        self.assertEqual(
            self.count(self.root / "w" / "probe-lean" / "lakefile.lean"), 1
        )

    def test_lifecycle_probe_clone(self):
        b = self.base(self.FOREIGN)
        self.lake_unreached()
        with self.assertRaises(generic.Failed):
            generic.run_lifecycle_probe(
                b, self.root / "w", "n_0000abcd", "C", "", b"", [], None
            )
        self.assertEqual(
            self.count(self.root / "w" / "life-probe" / "lakefile.lean"), 1
        )

    def test_lifecycle_build(self):
        lf = self.root / "lakefile.lean"
        for foreign in (self.FOREIGN, "lean_lib «n_0000abcd» where\n  roots := #[]\n"):
            lf.write_text(foreign)
            self.lake_unreached()
            with self.assertRaises(generic.Failed):
                generic.lifecycle_build(
                    self.root, "n_0000abcd", "", "Lifecycle", "", None
                )
            self.assertEqual(lf.read_text(), foreign)

    def test_lifecycle_build_reuses(self):
        lf = self.root / "lakefile.lean"
        generic.run = lambda cmd, *a, **kw: (0, "", "")
        # The run's or the probe's own stanza, already in the base, is kept as it is.
        lf.write_text("x" + generic.lakefile_lib("n_0000abcd", "C"))
        self.assertTrue(
            generic.lifecycle_build(self.root, "n_0000abcd", "", "L", "", None)[0]
        )
        self.assertEqual(lf.read_text(), "x" + generic.lakefile_lib("n_0000abcd", "C"))
        # Its own stanza is appended once, however many modules it builds.
        lf.write_text("x\n")
        for mod in ("Lifecycle", "LifecycleAll"):
            generic.lifecycle_build(self.root, "n_0000abcd", "", mod, "", None)
        self.assertEqual(self.count(lf), 1)


class ChangedPaths(unittest.TestCase):
    """The executed-tree check reads git status whole: a tracked file edited inside the run's own
    directory is never misread as one outside it, and a file outside it is still reported."""

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.t.name)
        g = ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-C", str(self.d)]
        self.g = g
        subprocess.run(["git", "init", "-q", str(self.d)], check=True)
        (self.d / "Certified" / NAME).mkdir(parents=True)
        (self.d / "Certified" / NAME / "Shape.lean").write_text("old shape")
        (self.d / "lakefile.lean").write_text("lakefile")
        (self.d / "Base.lean").write_text("base")
        subprocess.run(g + ["add", "-A"], check=True)
        subprocess.run(g + ["commit", "-qm", "base"], check=True)

    def tearDown(self):
        self.t.cleanup()

    def outside(self):
        return generic.changed_outside(self.d, f"Certified/{NAME}/")

    def test_tracked_edit_inside_sorts_first(self):
        """A modified tracked file in Certified/<name>/ is the first status line (' M ...')."""
        (self.d / "Certified" / NAME / "Shape.lean").write_text("new shape")
        (self.d / "Certified" / NAME / "Statement.lean").write_text("new")
        (self.d / "Certified" / NAME / "gen").mkdir()
        (self.d / "Certified" / NAME / "gen" / "Stage0.lean").write_text("new")
        first = subprocess.run(
            [
                "git",
                "-C",
                str(self.d),
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.splitlines()[0]
        self.assertEqual(first, f" M Certified/{NAME}/Shape.lean")
        self.assertEqual(
            generic.changed_paths(self.d),
            [
                f"Certified/{NAME}/Shape.lean",
                f"Certified/{NAME}/Statement.lean",
                f"Certified/{NAME}/gen/Stage0.lean",
            ],
        )
        self.assertEqual(self.outside(), [])

    def test_only_edit_inside(self):
        (self.d / "Certified" / NAME / "Shape.lean").write_text("new shape")
        self.assertEqual(self.outside(), [])

    def test_tracked_edit_outside_refused(self):
        """An edited base file outside Certified/<name>/ is reported, even after an inside edit."""
        (self.d / "Certified" / NAME / "Shape.lean").write_text("new shape")
        (self.d / "lakefile.lean").write_text("edited")
        self.assertEqual(self.outside(), ["lakefile.lean"])

    def test_untracked_and_deleted_outside_refused(self):
        (self.d / "Certified" / "other").mkdir()
        (self.d / "Certified" / "other" / "X.lean").write_text("x")
        (self.d / f"Certified/{NAME}x.lean").write_text("sibling prefix")
        (self.d / "Base.lean").unlink()
        self.assertEqual(
            sorted(self.outside()),
            ["Base.lean", f"Certified/{NAME}x.lean", "Certified/other/X.lean"],
        )

    def test_rename_into_the_directory_reports_its_source(self):
        """A staged rename from outside into Certified/<name>/ still names the base file it moved."""
        subprocess.run(
            self.g + ["mv", "Base.lean", f"Certified/{NAME}/Base.lean"], check=True
        )
        self.assertEqual(self.outside(), ["Base.lean"])

    def test_quoted_path_kept_whole(self):
        """A path git would quote in plain porcelain output (a space, a non-ASCII byte) is read
        verbatim."""
        (self.d / "Certified" / NAME / "a b.lean").write_text("x")
        (self.d / "x\u00e9y.lean").write_text("x")
        self.assertEqual(self.outside(), ["x\u00e9y.lean"])
        self.assertIn(f"Certified/{NAME}/a b.lean", generic.changed_paths(self.d))


class SolarBinary(unittest.TestCase):
    """The driver's compiler check: a binary whose digest is not the expected one, or whose
    embedded commit is not one with this checkout's compiler sources, is refused. The binaries are
    shell scripts that print a version."""

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.t.name) / "solar"
        (self.root / "crates").mkdir(parents=True)
        (self.root / "crates/lib.rs").write_text("// v1\n")
        (self.root / "Cargo.toml").write_text("[workspace]\n")
        (self.root / "Cargo.lock").write_text("# lock\n")
        self.first = self.commit("compiler")
        (self.root / "README.md").write_text("docs\n")
        self.head = self.commit("docs")

    def tearDown(self):
        self.t.cleanup()

    def commit(self, msg):
        g = ["git", "-C", str(self.root), "-c", "user.name=t", "-c", "user.email=t@t"]
        if not (self.root / ".git").exists():
            subprocess.run([*g, "init", "-q"], check=True)
        subprocess.run([*g, "add", "-A"], check=True)
        subprocess.run(
            [*g, "-c", "commit.gpgsign=false", "commit", "-qm", msg], check=True
        )
        return generic.git(self.root, "rev-parse", "HEAD")

    def binary(self, version):
        b = pathlib.Path(self.t.name) / "bin" / "solar"
        b.parent.mkdir(exist_ok=True)
        b.write_text(f"#!/bin/sh\necho '{version}'\n")
        b.chmod(0o755)
        return b

    def problem(self, b, want_sha=None, allow_foreign=False):
        return generic.solar_problem(b, self.root, want_sha, allow_foreign)

    def test_own_commit_passes(self):
        """The embedded commit may be an earlier one with the same compiler sources."""
        for c in (self.head[:8], self.first[:8]):
            with self.subTest(c):
                b = self.binary(f"solar 0.3.0-dev+commit.{c}")
                self.assertIsNone(self.problem(b, sha(b)))
        b = self.binary(f"solar 0.3.0-dev\nCommit SHA: {self.head}")
        self.assertIsNone(self.problem(b))

    def test_digest_differs(self):
        b = self.binary(f"solar 0.3.0-dev+commit.{self.head[:8]}")
        why = self.problem(b, "0" * 64)
        self.assertIn(f"has digest {sha(b)}, not the expected {'0' * 64}", why)
        self.assertIn("not the expected", self.problem(b, "0" * 64, True))

    def test_commit_differs(self):
        unknown = self.binary("solar 0.3.0-dev+commit.deadbeef")
        (self.root / "crates/lib.rs").write_text("// v2\n")
        self.commit("compiler v2")
        older = self.binary(f"solar 0.3.0-dev+commit.{self.first[:8]}")
        for label, b in (("unknown commit", unknown), ("other sources", older)):
            with self.subTest(label):
                why = self.problem(b, sha(b))
                self.assertIn("whose compiler sources are not this checkout's", why)
                self.assertIsNone(self.problem(b, sha(b), allow_foreign=True))

    def test_checkout_edited(self):
        b = self.binary(f"solar 0.3.0-dev+commit.{self.head[:8]}")
        (self.root / "crates/lib.rs").write_text("// edited\n")
        self.assertIn(
            "whose compiler sources are not this checkout's", self.problem(b, sha(b))
        )

    def test_no_commit(self):
        for version in ("solar 0.3.0-dev", "solar 0.3.0-dev+commit.xyz"):
            with self.subTest(version):
                b = self.binary(version)
                self.assertIn("cannot read the compiler commit", self.problem(b))

    def test_main_checks_the_expected_digest(self):
        """main refuses the binary it built before recording it, against the runner's
        CERTIFY_SOLAR_SHA256 rather than its own digest."""
        src = pathlib.Path(generic.__file__).read_text()
        main = src[src.index("\ndef main(") :]
        call = main.index("solar_problem(")
        self.assertLess(call, main.index('record["solar"]["binary_sha256"] ='))
        self.assertIn('os.environ.get("CERTIFY_SOLAR_SHA256")', main[call : call + 200])


class AfterRunPackages(unittest.TestCase):
    """The post-run check gets the pinned package set, never the pinned commit string: main's
    after-run call is the path the r5e run crashed on (`pin` rebound to the rev)."""

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.t.name)
        self.repo, self.base, cache = lean_fixture(root)
        (self.repo / ".lake/packages").mkdir(parents=True)
        shutil.move(cache / ".lake/packages/Pkg", self.repo / ".lake/packages/Pkg")
        (self.repo / ".gitignore").write_text(".lake\n")
        (self.repo / "Certified" / NAME).mkdir(parents=True)
        (self.repo / "Certified" / NAME / "S.lean").write_text("-- s\n")
        g = ["git", "-C", str(self.repo), "-c", "user.name=t", "-c", "user.email=t@t"]
        subprocess.run([*g, "add", "-A"], check=True)
        subprocess.run(
            [*g, "-c", "commit.gpgsign=false", "commit", "-qm", "inputs"], check=True
        )
        self.inputs = generic.git(self.repo, "rev-parse", "HEAD")
        self.pin, _ = generic.pinned_packages(self.repo, self.base)

    def tearDown(self):
        self.t.cleanup()

    def problem(self, pin=None):
        return generic.after_run_problem(
            self.repo,
            NAME,
            self.inputs,
            {"solidity_lean": self.inputs},
            self.pin if pin is None else pin,
        )

    def test_pinned_set_passes(self):
        (self.repo / "Certified" / NAME / "Final.lean").write_text("-- run output\n")
        self.assertIsNone(self.problem())

    def test_edited_package_refused(self):
        (self.repo / ".lake/packages/Pkg/Pkg/Foo.lean").write_text("def foo := 9\n")
        why = self.problem()
        self.assertTrue(why.startswith("packages: after the run: "), why)
        self.assertIn("edited or untracked", why)

    def test_executed_tree_refused_first(self):
        (self.repo / "SolidCore/A.lean").write_text("-- edited base\n")
        why = self.problem()
        self.assertTrue(why.startswith("executed tree: "), why)
        self.assertIn("SolidCore/A.lean", why)

    def test_rev_string_is_not_a_pin(self):
        """What main passed before the fix: the rev string crashes packages_problem."""
        with self.assertRaises(TypeError):
            self.problem(pin=self.base)

    def test_main_passes_the_pinned_set(self):
        """In main, every name given as the pin to packages_problem or after_run_problem is bound
        only by pinned_packages, so no later rebinding (such as the pinned rev) can reach it."""
        src = pathlib.Path(generic.__file__).read_text()
        main = next(
            n
            for n in pyast.walk(pyast.parse(src))
            if isinstance(n, pyast.FunctionDef) and n.name == "main"
        )
        pos = {"packages_problem": 1, "after_run_problem": 4}
        used = {
            c.args[pos[c.func.id]].id
            for c in pyast.walk(main)
            if isinstance(c, pyast.Call)
            and isinstance(c.func, pyast.Name)
            and c.func.id in pos
            and len(c.args) > pos[c.func.id]
            and isinstance(c.args[pos[c.func.id]], pyast.Name)
        }
        self.assertIn("pin", used)
        bad = []
        for n in pyast.walk(main):
            if not isinstance(
                n, (pyast.Assign, pyast.AnnAssign, pyast.AugAssign, pyast.NamedExpr)
            ):
                continue
            targets = n.targets if isinstance(n, pyast.Assign) else [n.target]
            names = {
                t.id
                for tg in targets
                for t in pyast.walk(tg)
                if isinstance(t, pyast.Name) and t.id in used
            }
            from_pin = (
                isinstance(n.value, pyast.Call)
                and isinstance(n.value.func, pyast.Name)
                and n.value.func.id == "pinned_packages"
            )
            if names and not from_pin:
                bad.append((sorted(names), n.lineno))
        self.assertEqual(
            bad, [], "the pin passed to the package checks is rebound in main"
        )


class LakeJobsAdmission(unittest.TestCase):
    """CERTIFY_LAKE_JOBS Lean processes of LEAN_MEMORY MB start only when they fit the memory.max of
    the driver's cgroup, read under a fixture CERTIFY_CGROUP_ROOT."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name)
        lines = pathlib.Path("/proc/self/cgroup").read_text().splitlines()
        path = next(line[3:] for line in lines if line.startswith("0::"))
        self.cg = self.root / path.strip("/")
        self.cg.mkdir(parents=True, exist_ok=True)

    def admit(self, jobs, limit):
        if limit is not None:
            (self.cg / "memory.max").write_text(f"{limit}\n")
        env = dict(
            os.environ, CERTIFY_LAKE_JOBS=str(jobs), CERTIFY_CGROUP_ROOT=str(self.root)
        )
        with unittest.mock.patch.dict(os.environ, env, clear=True):
            try:
                generic.admit_lake_jobs()
            except SystemExit as e:
                return str(e)
        return None

    def driver(self, jobs, limit):
        """scripts/certify itself, which must refuse before it reads its arguments."""
        (self.cg / "memory.max").write_text(f"{limit}\n")
        env = dict(
            os.environ,
            CERTIFY_LAKE_JOBS=str(jobs),
            CERTIFY_CGROUP_ROOT=str(self.root),
            LEAN_MEMORY="3000",
        )
        return subprocess.run(
            ["bash", str(HERE.parent.parent / "scripts" / "certify")],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_one_job_needs_no_limit(self):
        self.assertIsNone(self.admit(1, None))
        self.assertIsNone(self.admit(1, "max"))

    def test_fits(self):
        self.assertIsNone(self.admit(2, 2 * 6000 * 1024 * 1024))

    def test_too_big(self):
        err = self.admit(3, 2 * 6000 * 1024 * 1024)
        self.assertIn("need 18000 MB, more than the 12000 MB", err)
        r = self.driver(4, 8000 * 1024 * 1024)
        self.assertEqual(r.returncode, 1)
        self.assertIn("need 12000 MB, more than the 8000 MB", r.stderr)

    def test_unlimited(self):
        self.assertIn("is max (no limit)", self.admit(2, "max"))
        r = self.driver(2, "max")
        self.assertEqual(r.returncode, 1)
        self.assertIn("is max (no limit)", r.stderr)

    def test_unreadable(self):
        self.assertIn("cannot read", self.admit(2, None))


class ProbeExportPaths(unittest.TestCase):
    """The probe's stage export (gas_need) records the arguments solar ran with and the source's
    path (Manifest.arguments, manifest.json): two runs in different work directories, of copies of
    one source in different directories, write byte-identical manifests."""

    FAKE = """
import json, pathlib, sys
args = sys.argv[1:]
gen = pathlib.Path(next(a for a in args if a.startswith("-Zemit-lean-stages=")).split("=", 1)[1])
(gen / "C").mkdir(parents=True)
(gen / "C" / "Manifest.lean").write_text(repr(args) + chr(10))
(gen / "C" / "manifest.json").write_text(json.dumps({"arguments": args, "sources": [args[0]]}))
sys.exit(1)
"""

    def export(self, root, src_dir):
        solar = root / "solar"
        solar.write_text(f"#!{sys.executable}" + self.FAKE)
        solar.chmod(0o755)
        src = root / src_dir / "c.sol"
        src.parent.mkdir(parents=True)
        src.write_text("contract C {}\n")
        with self.assertRaises(generic.Blocked):
            generic.gas_need(solar, src, "c_0123abcd", "C", root / "work" / "probe")
        gen = root / "work" / "probe" / "gen" / "C"
        return {f: (gen / f).read_bytes() for f in ("Manifest.lean", "manifest.json")}

    def test_manifests_do_not_record_the_work_directory(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            one = self.export(pathlib.Path(a), "x")
            two = self.export(pathlib.Path(b), "y/z")
            for f in one:
                self.assertNotIn(a.encode(), one[f], f)
            self.assertEqual(one, two)


RETURNS = HERE / "testdata" / "asm_cert_returns"


def returns_program():
    """testdata/asm_cert_returns: a final EVM IR whose stack heights have no bound (test_asm_cert)."""
    m = json.loads((RETURNS / "manifest.json").read_text())
    return (RETURNS / "Evm5.lean").read_text(), m["bytecode"]


class StackBoundRefusal(unittest.TestCase):
    """asm_cert.infer refuses a stack with no bound by SystemExit (it is also a command). In the
    driver the refusal is a typed BLOCKED result (loop_checker_gas), and a run that meets it, on
    the checked-in returns fixture, still seals its record: never an exit past the handlers."""

    def test_the_bound_is_blocked_by_name(self):
        text, code = returns_program()
        with self.assertRaises(generic.Blocked) as e:
            generic.loop_checker_gas(text, bytes.fromhex(code), 1)
        (r,) = e.exception.reasons
        self.assertEqual(r["feature"], "stack bound")
        self.assertIn("the cycle bb3 -> bb4 -> bb3 gains stack words", r["detail"])
        self.assertFalse(r["detail"].startswith("error: "))

    def test_a_layout_mismatch_is_rejected(self):
        text, _ = returns_program()
        with self.assertRaises(generic.Failed) as e:
            generic.loop_checker_gas(text, bytes.fromhex("00"), 1)
        self.assertIn("asm_cert.py: error:", str(e.exception))

    def run_main(self, root, gas_need):
        """generic.main on a loop-free source with the build replaced and gas_need GAS_NEED: the
        exit code and the sealed record (None when there is none)."""
        _, code = returns_program()
        solar = root / "solar"
        solar.write_text(f"#!{sys.executable}\nprint('solar 0.0.0')\n")
        solar.chmod(0o755)
        src = root / "C.sol"
        src.write_text(
            HEAD + "contract C { uint256 x; function f() external { x = 1; } }\n"
        )
        argv = [
            "generic.py",
            str(src),
            "none",
            "--solidity-lean",
            "/nonexistent",
            "--work",
            str(root / "w"),
            "--record-dir",
            str(root / "r"),
            "--solc",
            str(SOLC),
        ]
        patches = [
            unittest.mock.patch.object(sys, "argv", argv),
            unittest.mock.patch.object(generic, "build_solar", lambda: solar),
            unittest.mock.patch.object(generic, "solar_problem", lambda *a: None),
            unittest.mock.patch.object(generic, "artifact_problem", lambda *a: None),
            unittest.mock.patch.object(generic, "build_provenance", lambda *a: {}),
            unittest.mock.patch.object(
                generic, "solar_runtime", lambda *a, **k: (code, "")
            ),
            unittest.mock.patch.object(generic, "gas_need", gas_need),
        ]
        with contextlib.ExitStack() as stack:
            for q in patches:
                stack.enter_context(q)
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            stack.enter_context(contextlib.redirect_stderr(io.StringIO()))
            with self.assertRaises(SystemExit) as e:
                generic.main()
        rec = root / "r" / "record.json"
        return e.exception.code, json.loads(rec.read_text()) if rec.exists() else None

    def check_blocked(self, gas_need):
        with tempfile.TemporaryDirectory() as t:
            code, rec = self.run_main(pathlib.Path(t), gas_need)
        self.assertEqual(code, 3)
        self.assertIsNotNone(rec)
        self.assertEqual(rec["status"], "BLOCKED")
        (r,) = rec["blocked"]
        self.assertEqual(r["feature"], "stack bound")
        self.assertIn("bb3 -> bb4 -> bb3", r["detail"])

    @unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
    def test_the_run_seals_a_blocked_record(self):
        text, code = returns_program()
        self.check_blocked(
            lambda *a: generic.loop_checker_gas(text, bytes.fromhex(code), 1)
        )

    @unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
    def test_an_unconverted_refusal_still_seals_one(self):
        # A call site that leaves asm_cert.infer's SystemExit alone: main's handler still records it.
        text, code = returns_program()

        def bare(*a):
            generic.asm_cert.infer(text, bytes.fromhex(code))

        self.check_blocked(bare)


class RegeneratedThreads(unittest.TestCase):
    """regenerated.sh compares a regenerated manifest with the committed one: the options'
    thread count, the host's, never makes two exports of one contract differ, whether the
    committed copy still records one and whatever the hosts' counts were."""

    LINE = '  "options": "CompileOpts { input: [], language: Solidity,%s evm_version: Osaka }",\n'

    def norm(self, text):
        script = (HERE / "regenerated.sh").read_text()
        start = script.index("norm() {")
        body = script[start : script.index("; }", start) + 3]
        r = subprocess.run(
            ["bash", "-c", body + "\nnorm"],
            input=text,
            capture_output=True,
            text=True,
            check=True,
        )
        return r.stdout

    def test_thread_count_is_not_compared(self):
        new = self.norm(self.LINE % "")
        for committed in (" threads: 24,", " threads: 32,", ""):
            self.assertEqual(self.norm(self.LINE % committed), new, committed)


class InputsFilterAgrees(unittest.TestCase):
    """generic.py's input commit and checkout.sh's CERTIFY_PIN_BASE check, fed the same fixture
    tree, accept the same files: either both accept and the commit holds exactly the files the
    checkout has under Certified/NAME/, or both refuse."""

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.t.name)
        (self.root / "solar" / "tools" / "certify").mkdir(parents=True)
        for f in ("checkout.sh", "lakefile_inputs.py"):
            shutil.copy(HERE / f, self.root / "solar" / "tools" / "certify" / f)
        self.repo = self.root / "sl"
        (self.repo / "Certified" / "m_2").mkdir(parents=True)
        (self.repo / "lakefile.lean").write_text("-- lakefile\n")
        (self.repo / "Certified" / "m_2" / "A.lean").write_text("other\n")
        (self.repo / ".gitignore").write_text(".lake/\n*.olean\n")
        self.g("init", "-q")
        self.g("add", "-A")
        self.g("commit", "-qm", "pin")
        self.pin = self.g("rev-parse", "HEAD")
        (self.root / "solar" / "tools" / "certify" / "solidity-lean.rev").write_text(
            self.pin + "\n"
        )
        (self.repo / "Certified" / "n_0000abcd").mkdir()
        (self.repo / "Certified" / "n_0000abcd" / "n_0000abcd.sol").write_text(
            "contract C {}\n"
        )
        (self.repo / "Certified" / "n_0000abcd" / "Source.lean").write_text(
            "-- source\n"
        )
        (self.repo / ".lake").mkdir()
        (self.repo / ".lake" / "x").write_text("build\n")

    def tearDown(self):
        self.t.cleanup()

    def g(self, *a):
        return subprocess.run(
            [
                "git",
                "-C",
                str(self.repo),
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@l",
                *a,
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def both(self):
        """(generic's inputs or None, checkout.sh's accepted files or None), None for a refusal."""
        try:
            _, ins = generic.commit_inputs(self.repo, self.pin, "n_0000abcd", "C")
            ins = sorted(ins)
        except generic.Failed:
            ins = None
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("CI", "GITHUB_ACTIONS", "CERTIFY_ALLOW_UNPINNED")
        }
        env.update(SOLIDITY_LEAN_DIR=str(self.repo), CERTIFY_PIN_BASE=self.pin)
        r = subprocess.run(
            [
                "bash",
                str(self.root / "solar" / "tools" / "certify" / "checkout.sh"),
                "Certified/n_0000abcd",
            ],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        d = self.repo / "Certified" / "n_0000abcd"
        seen = (
            sorted(
                str(q.relative_to(self.repo)) for q in d.rglob("*") if not q.is_dir()
            )
            if r.returncode == 0
            else None
        )
        # The one path outside Certified/NAME/ both may accept: lakefile_inputs.py's stanza.
        if seen is not None and "lakefile.lean" in (ins or []):
            seen = sorted([*seen, "lakefile.lean"])
        return ins, seen

    def test_plain(self):
        ins, seen = self.both()
        self.assertEqual(
            ins,
            ["Certified/n_0000abcd/Source.lean", "Certified/n_0000abcd/n_0000abcd.sol"],
        )
        self.assertEqual(ins, seen)

    def test_lakefile_stanza(self):
        """The canonical stanza appended for NAME: both accept it, and only it."""
        with open(self.repo / "lakefile.lean", "a") as f:
            f.write(generic.lakefile_lib("n_0000abcd", "C"))
        ins, seen = self.both()
        self.assertIn("lakefile.lean", ins)
        self.assertEqual(ins, seen)

    def test_lakefile_other_edit(self):
        with open(self.repo / "lakefile.lean", "a") as f:
            f.write(generic.lakefile_lib("n_0000abcd", "C") + "-- more\n")
        self.assertEqual(self.both(), (None, None))

    def test_ignored_input(self):
        (self.repo / "Certified" / "n_0000abcd" / "Gen.olean").write_text(
            "read by the run\n"
        )
        self.assertEqual(self.both(), (None, None))

    def test_moved_from_another_contract(self):
        self.g("mv", "Certified/m_2/A.lean", "Certified/n_0000abcd/Source2.lean")
        self.assertEqual(self.both(), (None, None))



class LoopVerdict(unittest.TestCase):
    """generic.loop_verdict: CERTIFIED at the bounded loop tier only from a checked final that passes
    every gate, with the unbounded tier reported beside it; anything else keeps the branch's verdict."""

    PLAIN = "6080"

    def setUp(self):
        import argparse
        import hashlib

        fn = lambda name, sel, doc=None: {
            "nodeType": "FunctionDefinition", "name": name, "kind": "function", "visibility": "public",
            "functionSelector": sel, "body": {"nodeType": "WhileStatement"},
            **({"documentation": {"text": doc}} if doc else {})}
        self.ast = {"nodes": [{"nodeType": "ContractDefinition", "id": 1, "name": "T", "linearizedBaseContracts": [1],
                               "nodes": [fn("f", "fd0011a5", " @custom:ensures result == n")]}]}
        self.a = argparse.Namespace(spec="natspec", contract="T")
        self.part = {"signature": "f(uint256)", "selector": "0xfd0011a5", "spec": "proved", "label": "x",
                     "nonvacuous": True, "nonvac_witness": {"well_typed": True, "output_words": [1]},
                     "valid": "V.valid K", "iteration_bound": {"var": "K", "max": 7}, "gas": "g"}
        self.record = {
            "name": "t_1", "requested_tier": "certified", "contract": "T",
            "loop_final": {
                "functions": ["f"], "final": {"theorem": "Certified.t_1.Final.final", "strongest": False},
                "axioms": ["propext", "Classical.choice", "Quot.sound"],
                "fresh_byte": {"check_final": "FRESH x", "runtime_sha256": hashlib.sha256(bytes.fromhex(self.PLAIN)).hexdigest()},
                "replay": {"closure": 10, "replayed_count": 10, "trusted_count": 0},
                "parts": {"f": self.part},
                "inventory": {"theorem": "Certified.t_1.LoopSpecInventory.inventory",
                              "specs": [{"name": "T.f", "selector": "0xfd0011a5"}]},
            },
        }
        self.reasons = [{"feature": "loop", "where": "f", "detail": "loop final checked (Certified.t_1.Final.final, record loop_final): runtime only"}]
        self.saved = generic.solar_runtime, generic.build_solar
        generic.solar_runtime = lambda *x, **k: (self.PLAIN, "")
        generic.build_solar = lambda: "solar"

    def tearDown(self):
        generic.solar_runtime, generic.build_solar = self.saved

    def verdict(self, record=None, reasons=None, oog=None, ast=None):
        return generic.loop_verdict(record or self.record, reasons or self.reasons, "s.sol", ast or self.ast, self.a, oog)

    def test_certified_bounded_with_unbounded_reported(self):
        with self.assertRaises(generic.LoopCertified) as e:
            self.verdict(oog=generic.Failed("resource: out of gas: the fixed bound is at least 2^64"))
        c = e.exception
        self.assertEqual(c.status, "CERTIFIED")
        self.assertEqual(c.tier["tier"], "bounded")
        self.assertEqual(c.tier["bounds"], {"f": {"K_max": 7, "valid": "V.valid K", "gas": "g"}})
        self.assertEqual(c.tier["unbounded"]["status"], "not certified")
        self.assertIn("out of gas", c.tier["unbounded"]["why"])
        self.assertIn("unbounded n: not certified", c.line)
        self.assertIs(self.record["loop_tier"], c.tier)

    def test_without_out_of_gas_the_unbounded_tier_is_still_not_certified(self):
        with self.assertRaises(generic.LoopCertified) as e:
            self.verdict()
        self.assertEqual(e.exception.tier["unbounded"]["status"], "not certified")
        self.assertIn("K ≤ 7", e.exception.tier["unbounded"]["why"])

    def test_refusals_keep_the_verdict(self):
        import copy

        def m(f):
            r = copy.deepcopy(self.record)
            f(r["loop_final"])
            return r

        cases = {
            "refused final": m(lambda lf: lf.update(refused={"step": "spec", "why": "x"})),
            "extra axiom": m(lambda lf: lf.update(axioms=["propext", "sorryAx"])),
            "no axioms": m(lambda lf: lf.update(axioms=[])),
            "no fresh line": m(lambda lf: lf["fresh_byte"].update(check_final=None)),
            "other bytes": m(lambda lf: lf["fresh_byte"].update(runtime_sha256="0" * 64)),
            "trusted replay": m(lambda lf: lf["replay"].update(trusted_count=3)),
            "no inventory": m(lambda lf: lf.pop("inventory")),
            "unstated export spec": m(lambda lf: lf["inventory"]["specs"].append({"name": "T.g", "selector": "0x01"})),
            "vacuous spec": m(lambda lf: lf["parts"]["f"].update(nonvacuous="no witness")),
            "ABI-only witness": m(lambda lf: lf["parts"]["f"].update(nonvac_witness={"well_typed": False})),
            "no witness record": m(lambda lf: lf["parts"]["f"].pop("nonvac_witness")),
            "refused part": m(lambda lf: lf["parts"]["f"].update(spec="refused")),
            "unchecked True": m(lambda lf: lf["parts"]["f"].update(spec="none", no_spec=None)),
            "translation only at certified": m(lambda lf: lf["parts"]["f"].update(
                spec="none", no_spec={"theorem": "Certified.t_1.LoopSpecInventory.f_no_spec"})),
        }
        for k, rec in cases.items():
            with self.subTest(k):
                out = self.verdict(record=rec)
                self.assertIsInstance(out, list)
        # Another reason left beside the final's (an uncovered loop's OPEN reason, an envelope reason).
        more = self.reasons + [{"feature": "loop", "where": "line 9", "detail": "g: a while loop ... (OPEN)"}]
        out = self.verdict(reasons=more)
        self.assertIn("not certified: other reasons remain", out[0]["detail"])
        self.assertEqual(self.record["loop_tier"]["status"], "not certified")

    def test_coverage_of_the_source(self):
        import copy

        fn = copy.deepcopy(self.ast["nodes"][0]["nodes"][0])
        for name, node in {
            "a second loop function": dict(fn, name="g", functionSelector="01020304"),
            "an uncovered entry point": dict(fn, name="g", body={"nodeType": "Block"}),
            "a public getter": {"nodeType": "VariableDeclaration", "name": "x", "stateVariable": True, "visibility": "public"},
            "a fallback": {"nodeType": "FunctionDefinition", "kind": "fallback", "body": {"nodeType": "Block"}},
            "a constructor spec": {"nodeType": "FunctionDefinition", "kind": "constructor", "name": "",
                                   "documentation": {"text": " @custom:ensures x == 0"}, "body": {"nodeType": "Block"}},
        }.items():
            with self.subTest(name):
                ast = copy.deepcopy(self.ast)
                ast["nodes"][0]["nodes"].append(node)
                self.assertIsInstance(self.verdict(ast=ast), list)
        ast = copy.deepcopy(self.ast)
        ast["nodes"][0]["documentation"] = {"text": " @custom:invariant x <= 1"}
        self.assertIsInstance(self.verdict(ast=ast), list)

    def test_compiled_tier_takes_translation_only(self):
        import copy

        rec = copy.deepcopy(self.record)
        rec["requested_tier"] = "compiled"
        rec["loop_final"]["parts"]["f"].update(spec="none", no_spec={"theorem": "t"})
        rec["loop_final"]["inventory"]["specs"] = []
        self.a.spec = "none"
        ast = copy.deepcopy(self.ast)
        ast["nodes"][0]["nodes"][0].pop("documentation")
        with self.assertRaises(generic.LoopCertified) as e:
            self.verdict(record=rec, ast=ast)
        self.assertEqual(e.exception.status, "COMPILED")


if __name__ == "__main__":
    unittest.main(verbosity=2)
