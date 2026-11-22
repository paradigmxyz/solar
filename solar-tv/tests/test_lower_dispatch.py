"""Check the implicit-to-explicit runtime entry through CLI and pass audits."""

import gzip
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate


class LowerDispatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.folder = Path(temporary.name)
        archive = cls.folder / "tiny.json.gz"
        archive.write_bytes(
            gzip.compress(
                evaluate.stable(
                    {"sources": {"C.sol": {"content": "contract C {}"}}, "settings": {}}
                )
            )
        )
        cls.case = {
            "name": "tiny",
            "source": "C.sol",
            "contract": "C",
            "archive": str(archive),
            "sha256": evaluate.digest(archive.read_bytes()),
        }
        cls.before = cls.folder / "before.mir"
        cls.before.write_text(
            "@module C\nfn @keep() -> i256 {\nbb0:\nv0 = icall @leaf\nret v0\n}\n"
            "fn @leaf() -> i256 {\nbb0:\nret 7\n}\n"
        )
        cls.after = cls.folder / "after.mir"
        cls.after.write_text(
            subprocess.check_output(
                [SOLAR, cls.before, "-j1", "-Zmir-pipeline=lower-dispatch"],
                text=True,
                timeout=30,
            )
        )

    def check_pair(self, before, after, status):
        report = self.folder / "dispatch-report.json"
        run = subprocess.run(
            [
                CLI,
                "mir",
                before,
                after,
                "--prune",
                "--solar",
                SOLAR,
                "--report",
                report,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        self.assertEqual(run.returncode, status, run.stdout + run.stderr)
        return json.loads(report.read_text())

    def test_strict_smoke_checks_every_pass(self):
        manifest = self.folder / "manifest.json"
        manifest.write_text(json.dumps({"version": 1, "cases": [self.case]}))
        output = self.folder / "smoke"
        run = subprocess.run(
            [
                ROOT / "evaluate",
                "--manifest",
                manifest,
                "--solar",
                SOLAR,
                "--all",
                "--require-verified",
                "--timeout",
                "30",
                "--output",
                output,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=120,
        )
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        report = json.loads((output / "results.json").read_text())
        (case,) = report["cases"]
        self.assertTrue(case["output_identical"])
        self.assertEqual(case["captured"], 3)
        self.assertEqual(
            {row["pass"]: row["status"] for row in case["checks"]},
            {
                "cfg-simplify": "verified",
                "lower-dispatch": "verified",
                "lower-evm-shaped": "verified",
            },
        )
        (dispatch,) = (row for row in case["checks"] if row["pass"] == "lower-dispatch")
        self.assertEqual(dispatch["proof_mode"], "empty-runtime-dispatch")
        proof = json.loads((output / dispatch["report"]).read_text())
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertFalse(proof["whole_pass"])

    def test_live_dispatch_audit_keeps_bytecode(self):
        source = evaluate.inputs(self.case)
        baseline, _ = evaluate.compile_input(SOLAR, source, self.folder)
        audit = self.folder / "audit"
        run = subprocess.run(
            [SOLAR, "--standard-json", "-j1", f"-Zsolar-tv={audit}"],
            input=json.dumps(source),
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            capture_output=True,
            text=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        contracts = json.loads(run.stdout)["contracts"]
        self.assertEqual(evaluate.digest(evaluate.stable(contracts)), baseline)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        (dispatch,) = (
            row
            for row in records
            if row["context"].startswith("solar-tv: mir pass `lower-dispatch`")
        )
        self.assertEqual(dispatch["status"], "verified")
        self.assertEqual(dispatch["region"]["proof"], "empty-runtime-dispatch")

    def test_retained_function_and_bad_dispatchers(self):
        record = self.check_pair(self.before, self.after, 0)
        self.assertEqual(record["region"]["retained"], ["keep", "leaf"])
        self.assertEqual(record["region"]["work_steps"], 6)
        text = self.after.read_text()
        broken = self.folder / "broken.mir"
        for old, new, diagnostic in (
            ("revert 0, 0", "stop", "must reach an empty revert"),
            ("revert 0, 0", "revert 0, 32", "must reach an empty revert"),
            ("revert 0, 0", "jump bb1", "must reach an empty revert"),
            ("ret 7", "ret 8", "changed retained functions"),
        ):
            with self.subTest(edit=new):
                self.assertIn(old, text)
                broken.write_text(text.replace(old, new))
                report = self.check_pair(self.before, broken, 2)
                self.assertEqual(report["status"], "unsupported")
                self.assertIn(diagnostic, report["diagnostic"])

    def test_absent_routes_are_checked_in_the_source(self):
        routed = self.folder / "routed.mir"
        routed.write_text(
            self.before.read_text().replace("-> i256", "-> i256 [selector=0x01020304]")
        )
        report = self.check_pair(routed, self.after, 2)
        self.assertEqual(report["status"], "unsupported")
        self.assertIn("source with no runtime routes", report["diagnostic"])
