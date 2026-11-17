"""The certify entry points run from a clean checkout of this commit.

The Python suites import the driver's modules from the working tree, so a module the entry points
need that is only present untracked (or on another branch) can leave them passing while the
driver itself fails at its first step. These tests copy the commit's tracked files alone
(`git archive HEAD`) and run, from that copy, the real paths that reach cutpoints: generic.py's
stage-proof step (`gas_need`, which runs certify.py), asm_ctx.py's main, and the cutpoints calls
negative.sh makes, from the directory negative.sh resolves. They need a solar binary (SOLAR, else
target/debug/solar) and fail without one rather than skip.
"""

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SOLAR = pathlib.Path(os.environ.get("SOLAR", ROOT / "target" / "debug" / "solar"))
COUNTER = "tools/certify/examples/counter.sol"
MILESTONE_LINE = (
    'milestone="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../solar-tv/milestone" && pwd)"'
)


class CleanCheckoutEntrypoints(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not SOLAR.is_file():
            raise AssertionError(f"no solar binary at {SOLAR} (build it or set SOLAR)")
        cls.tmp = pathlib.Path(tempfile.mkdtemp(prefix="certify-entry-"))
        cls.clean = cls.tmp / "clean"
        cls.clean.mkdir()
        archive = subprocess.run(
            ["git", "-C", str(ROOT), "archive", "HEAD"], check=True, capture_output=True
        ).stdout
        subprocess.run(["tar", "-x", "-C", str(cls.clean)], input=archive, check=True)
        cls.certify = cls.clean / "tools" / "certify"
        # The exported stages of a small contract, as generic.py exports them.
        cls.gen = cls.tmp / "gen"
        p = subprocess.run(
            [
                str(SOLAR),
                str(cls.clean / COUNTER),
                "-Ogas",
                "--evm-version",
                "osaka",
                "--emit=bin-runtime",
                f"-Zemit-lean-stages={cls.gen}",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        assert p.returncode == 0, p.stderr

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def run_clean(self, code):
        """Run Python code with only the clean checkout's modules on its path."""
        env = dict(os.environ, PYTHONPATH="", PYTHONSAFEPATH="1")
        return subprocess.run(
            [sys.executable, "-c", code],
            cwd=self.tmp,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_generic_stage_proofs(self):
        # generic.py's own step that runs certify.py, which rejected every run while cutpoints
        # was missing ("certify.py cannot write the stage proofs").
        code = (
            "import pathlib, sys\n"
            f"sys.path.insert(0, {str(self.certify)!r})\n"
            "import generic\n"
            f"assert generic.HERE == pathlib.Path({str(self.certify)!r}), generic.HERE\n"
            f"need = generic.gas_need(pathlib.Path({str(SOLAR)!r}), "
            f"pathlib.Path({str(self.clean / COUNTER)!r}), 'counter_t', 'Counter', "
            f"pathlib.Path({str(self.tmp / 'probe')!r}))\n"
            "print('NEED', need)\n"
        )
        p = self.run_clean(code)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("NEED ", p.stdout)
        self.assertTrue(any((self.tmp / "probe" / "Proofs").rglob("*.lean")))

    def test_asm_ctx_main(self):
        out = self.tmp / "Ctx.lean"
        p = subprocess.run(
            [
                sys.executable,
                str(self.certify / "asm_ctx.py"),
                str(self.gen / "Counter"),
                "--gen-module",
                "X.gen.Counter",
                "--out",
                str(out),
            ],
            capture_output=True,
            text=True,
            cwd=self.tmp,
            check=False,
        )
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("theorem block_0", out.read_text())

    def test_negative_cutpoints(self):
        neg = (self.certify / "negative.sh").read_text()
        self.assertIn(MILESTONE_LINE, neg)
        subprocess.run(["bash", "-n", str(self.certify / "negative.sh")], check=True)
        milestone = (self.certify / ".." / ".." / "solar-tv" / "milestone").resolve()
        self.assertTrue((milestone / "cutpoints.py").is_file(), milestone)
        manifest = json.loads((self.gen / "Counter" / "manifest.json").read_text())
        last = [s for s in manifest["stages"] if s["kind"] != "bytecode"][-1]
        # The calls negative.sh's loop case makes.
        code = (
            "import sys\n"
            f"sys.path.insert(0, {str(milestone)!r})\n"
            "import cutpoints\n"
            f"s = open({str(self.gen / 'Counter' / last['file'])!r}).read()\n"
            "blocks = cutpoints.parse_evm(s)\n"
            "succ, _, _ = cutpoints.evm_cfg(blocks)\n"
            "loop = sorted(cutpoints.on_cycle(succ))\n"
            "print('BLOCKS', len(blocks), 'LOOP', loop, 'LOOPS', bool(cutpoints.loops(blocks)))\n"
        )
        p = self.run_clean(code)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertRegex(p.stdout, r"BLOCKS [1-9]\d* LOOP \[\] LOOPS False")

    def test_cutpoints_api(self):
        # Every name tools/certify reads from cutpoints.
        names = [
            "EXCLUDED_FUNCTIONS",
            "EXCLUDED_SELECTORS",
            "JOINS",
            "MIR_JOINS",
            "SCOPE",
            "SEGMENT",
            "Unsupported",
            "evm_cfg",
            "evm_states",
            "loops",
            "mir_headers",
            "on_cycle",
            "parse_evm",
            "parse_mir",
            "scope_parts",
            "stage_body",
        ]
        code = (
            "import sys\n"
            f"sys.path.insert(0, {str(self.clean / 'solar-tv' / 'milestone')!r})\n"
            "import cutpoints\n"
            f"missing = [n for n in {names!r} if not hasattr(cutpoints, n)]\n"
            "print('MISSING', missing)\n"
        )
        p = self.run_clean(code)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("MISSING []", p.stdout)


if __name__ == "__main__":
    unittest.main()
