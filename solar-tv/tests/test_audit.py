"""Proof context, independent replay gates and separate audit statuses."""

import contextlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import audit, native
from validator import check as checker
from validator.common import (
    Inconclusive,
    Program,
    ReplayTimeout,
    Unsupported,
    lean_root,
)


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / "result.json"
        self.before = ROOT / "examples/mir/dce/before.mir"
        self.after = ROOT / "examples/mir/dce/after.mir"

    def args(self, after=None):
        return [
            CLI,
            "mir",
            str(self.before),
            str(after or self.after),
            "--solar",
            SOLAR,
            "--report",
            str(self.path),
        ]

    def test_verified_report_binds_context_and_input(self):
        result = subprocess.run(
            self.args(), capture_output=True, text=True, timeout=120, check=False
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "verified\n")
        data = json.loads(self.path.read_text())
        self.assertEqual(data["status"], "verified")
        self.assertEqual(data["evidence"], "kernel-proof")
        self.assertEqual(data["target"], "cancun")
        self.assertEqual(data["gas_policy"], "sufficient")
        self.assertEqual(len(data["semantics"]["sha256"]), 64)
        self.assertEqual(
            data["semantics"]["lean"],
            (lean_root() / "lean-toolchain").read_text().strip(),
        )
        self.assertTrue(
            all(len(source["canonical_sha256"]) == 64 for source in data["inputs"])
        )
        self.assertIn("semantics: solar-tv-v3 sha256=", result.stderr)
        self.assertIn("assumptions:", result.stderr)

    def test_counterexample_has_matching_independent_observations(self):
        result = subprocess.run(
            self.args(ROOT / "examples/mir/dce/broken.mir"),
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        data = json.loads(self.path.read_text())
        self.assertEqual(data["evidence"], "kernel-and-rust-replay")
        self.assertEqual(data["witness"]["lean"], ["1", "0"])
        self.assertEqual(data["witness"]["rust"], ["1", "0"])
        self.assertIn("Rust interpreter: matched", result.stdout)

    def test_model_disagreement_is_not_a_reported_counterexample(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            patch.object(sys, "argv", self.args(ROOT / "examples/mir/dce/broken.mir")),
            patch.object(native, "replay", return_value={"observations": ["9", "9"]}),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            status = checker.main()
        self.assertEqual(status, 2)
        self.assertEqual(stdout.getvalue(), "")
        data = json.loads(self.path.read_text())
        self.assertEqual(data["status"], "inconclusive")
        self.assertIsNone(data["evidence"])
        self.assertIn("Lean and Rust interpreter replay disagree", stderr.getvalue())

    def test_statuses_do_not_conflate_unsupported_timeout_and_error(self):
        for error, expected in [
            (Unsupported("operation outside model"), "unsupported"),
            (Inconclusive("proof search unfinished"), "inconclusive"),
            (subprocess.TimeoutExpired(["lean"], 1), "timeout"),
            (ReplayTimeout(["helper"], 1), "timeout"),
            (OSError("cannot read input"), "error"),
        ]:
            with self.subTest(expected=expected):
                stdout, stderr = io.StringIO(), io.StringIO()
                with (
                    patch.object(sys, "argv", self.args()),
                    patch.object(checker, "load", side_effect=error),
                    contextlib.redirect_stdout(stdout),
                    contextlib.redirect_stderr(stderr),
                ):
                    status = checker.main()
                self.assertEqual(status, 2)
                self.assertEqual(stdout.getvalue(), "")
                self.assertEqual(json.loads(self.path.read_text())["status"], expected)
                if isinstance(error, ReplayTimeout):
                    self.assertIn(
                        "Rust interpreter replay timed out", stderr.getvalue()
                    )

    def test_report_write_failure_cannot_print_verified(self):
        self.path.mkdir()
        result = subprocess.run(
            self.args(), capture_output=True, text=True, timeout=120, check=False
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertIn("cannot write audit report", result.stderr)

    def test_summary_counts_each_status_independently(self):
        folder = Path(self.tmp.name)
        for i, status in enumerate(audit.STATUSES):
            audit.write(
                folder / "audits" / f"{i}.json", {"version": 1, "status": status}
            )
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(audit.summarize([str(folder)]), 0)
        self.assertEqual(
            json.loads(output.getvalue()), dict.fromkeys(audit.STATUSES, 1)
        )

    def test_invalid_summary_is_an_error(self):
        folder = Path(self.tmp.name)
        for status in ("typo", [], None):
            audit.write(
                folder / "audits" / "bad.json", {"version": 1, "status": status}
            )
            with (
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as error,
            ):
                audit.summarize([str(folder)])
            self.assertEqual(error.exception.code, 2)

    def test_native_protocol_does_not_accept_incomplete_observations(self):
        program = Program(("f",), [], "Outcome (BitVec 256)", "")
        for response in (
            {},
            {"version": 1, "status": "ok", "observations": []},
            {"version": 1, "status": "ok", "observations": [0, 1]},
        ):
            result = subprocess.CompletedProcess([], 0, json.dumps(response), "")
            with (
                patch.object(native.shutil, "which", return_value="helper"),
                patch.object(native.subprocess, "run", return_value=result),
                self.assertRaises(Inconclusive),
            ):
                native.replay(program, program, [], executable="helper", timeout=1)

    def test_missing_native_helper_cannot_certify_a_bug(self):
        result = subprocess.run(
            self.args(ROOT / "examples/mir/dce/broken.mir")
            + ["--replay", str(Path(self.tmp.name) / "missing")],
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertEqual(json.loads(self.path.read_text())["status"], "unsupported")

    def test_native_timeout_keeps_its_stage(self):
        program = Program(("f",), [], "Outcome (BitVec 256)", "")
        with (
            patch.object(native.shutil, "which", return_value="helper"),
            patch.object(
                native.subprocess,
                "run",
                side_effect=subprocess.TimeoutExpired(["helper"], 1),
            ),
            self.assertRaises(ReplayTimeout),
        ):
            native.replay(program, program, [], executable="helper", timeout=1)

    def test_semantics_changes_cannot_print_verified(self):
        program = Program(("f",), [], "Outcome (BitVec 256)", "")
        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            patch.object(sys, "argv", self.args()),
            patch.object(checker, "load", return_value=program),
            patch.object(
                checker, "check", return_value=audit.Result("verified", "verified\n")
            ),
            patch.object(
                audit,
                "semantics",
                side_effect=[
                    {"version": "test", "sha256": "before", "lean": "test"},
                    {"version": "test", "sha256": "after", "lean": "test"},
                ],
            ),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            self.assertEqual(checker.main(), 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(json.loads(self.path.read_text())["status"], "inconclusive")
        self.assertIn("semantics changed during this check", stderr.getvalue())
