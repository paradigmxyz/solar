"""Typed source execution over arbitrary caller state, with independent witnesses."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, mir_symbolic
from validator.common import Inconclusive, Unsupported


class SymbolicMirTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.report = self.directory / "report.json"
        self.examples = ROOT / "examples/mir/universal-state"

    def check(self, before, after, *flags):
        result = subprocess.run(
            [
                CLI,
                "mir",
                before,
                after,
                "--solar",
                SOLAR,
                "--timeout",
                "20",
                "--report",
                self.report,
                *flags,
            ],
            text=True,
            capture_output=True,
            timeout=150,
            check=False,
        )
        self.assertTrue(self.report.exists(), result.stderr)
        return result, json.loads(self.report.read_text())

    def test_real_pass_pairs_with_live_world_and_typed_returns(self):
        for name in ("storage", "memory", "multiple", "aggregate", "calls"):
            with self.subTest(name=name):
                before, after = [
                    self.examples / f"{name}-{side}.mir" for side in ("before", "after")
                ]
                compiled = subprocess.run(
                    [SOLAR, before, "-Ogas", "-Zmir-pipeline=egraph"],
                    text=True,
                    capture_output=True,
                    timeout=30,
                    check=False,
                )
                normalize = lambda text: [
                    line.strip()
                    for line in text.splitlines()
                    if line.strip() and not line.startswith(";")
                ]
                self.assertEqual(compiled.returncode, 0, compiled.stderr)
                self.assertEqual(
                    normalize(compiled.stdout), normalize(after.read_text())
                )
                flags = ("--function", "f") if name == "calls" else ()
                result, record = self.check(before, after, *flags)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "verified\n")
                self.assertEqual(record["gas_policy"], "semantic-effects")
                self.assertEqual(record["evidence"], "kernel-proof")
                self.assertFalse(record["whole_pass"])
                self.assertEqual(record["region"]["proof"], "universal-caller-state")
                self.assertTrue(
                    all(
                        evidence["kind"] == "independent-typed-source-interpreter"
                        for evidence in record["import_correspondence"]
                    )
                )
                self.assertIn(
                    "all typed argument leaves and arbitrary initial journal, memory contents, return data and caller environment",
                    record["assumptions"],
                )
                self.assertNotIn("world", record)

    def test_changed_effects_and_return_leaves_have_replayed_witnesses(self):
        for name in ("storage", "memory", "multiple", "aggregate"):
            with self.subTest(name=name):
                result, record = self.check(
                    self.examples / f"{name}-before.mir",
                    self.examples / f"{name}-broken.mir",
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(record["status"], "counterexample")
                self.assertEqual(
                    record["witness"]["observations"],
                    record["witness"]["native_observations"],
                )
                self.assertIn("Rust interpreter: matched", result.stdout)
                self.assertIn("world", record)

    def test_changed_cached_import_cannot_certify_itself(self):
        program = check.load("mir", self.examples / "storage-before.mir", SOLAR, 20)
        program.body = program.body.replace(".binary .add 256", ".binary .sub 256")
        with self.assertRaisesRegex(Unsupported, "independent source export"):
            mir_symbolic.check(
                program,
                program,
                20,
                replay_command="unused",
                replay_executable=str(Path(SOLAR).with_name("solar-tv-replay")),
                verbose=False,
            )

    def test_nonempty_initial_storage_finds_an_omitted_load(self):
        before, after = self.directory / "before.mir", self.directory / "after.mir"
        before.write_text(
            "@module LiveSlot\nfn @f() -> i256 {\nbb0:\n v0 = sload 0\n ret v0\n}\n"
        )
        after.write_text(before.read_text().replace("ret v0", "ret 0"))
        result, record = self.check(before, after)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )
        self.assertNotEqual(
            record["witness"]["observations"][0]["values"],
            record["witness"]["observations"][1]["values"],
        )
        self.assertTrue(
            any(
                account["storage"] for account in record["world"]["fixture"]["accounts"]
            )
        )

    def test_native_disagreement_is_not_hidden_as_unsupported(self):
        program = check.load("mir", self.examples / "storage-before.mir", SOLAR, 20)
        with (
            patch(
                "validator.world_check.check",
                side_effect=Inconclusive("native observations disagree"),
            ),
            self.assertRaisesRegex(Inconclusive, "native observations disagree"),
        ):
            mir_symbolic.counterexample(
                program,
                program,
                20,
                "unused",
                str(Path(SOLAR).with_name("solar-tv-replay")),
                False,
            )

    def test_model_errors_and_unbounded_memory_never_verify(self):
        examples = [
            "fn @f() -> struct0 {\nbb0:\n ret undef struct0\n}",
            "fn @f(arg0: i256) {\nbb0:\n mstore arg0, 1\n ret\n}",
            "fn @f() {\nbb0:\n jump bb0\n}",
            "fn @f() {\nbb0:\n icall @f\n ret\n}",
        ]
        for index, body in enumerate(examples):
            with self.subTest(index=index):
                path = self.directory / "unsupported.mir"
                path.write_text(
                    "@module Unsupported\n@types\nstruct0: {i256, i256}\n" + body + "\n"
                )
                result, record = self.check(path, path)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(record["status"], "unsupported")
                self.assertIsNone(record["evidence"])

    def test_pointer_address_spaces_and_return_interfaces_stay_distinct(self):
        before, after = self.directory / "before.mir", self.directory / "after.mir"
        before.write_text(
            "@module Pointers\nfn @f(arg0: ptr<memory>) -> ptr<memory> {\nbb0:\n ret arg0\n}\n"
        )
        after.write_text(before.read_text().replace("memory", "calldata"))
        result, record = self.check(before, after)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("interfaces or declarations differ", result.stderr)

    def test_live_pass_audit_preserves_output(self):
        before = self.examples / "storage-before.mir"
        args = [SOLAR, before, "-Ogas", "-Zmir-pipeline=egraph"]
        baseline = subprocess.run(
            args, text=True, capture_output=True, timeout=30, check=False
        )
        audited = subprocess.run(
            [*args, f"-Zsolar-tv={self.directory / 'audit'}"],
            env={**os.environ, "SOLAR": SOLAR, "SOLAR_TV": CLI},
            text=True,
            capture_output=True,
            timeout=90,
            check=False,
        )
        self.assertEqual(baseline.returncode, 0, baseline.stderr)
        self.assertEqual(audited.returncode, 0, audited.stderr)
        self.assertEqual(baseline.stdout, audited.stdout)
        records = [
            json.loads(path.read_text())
            for path in (self.directory / "audit/audits").glob("*.json")
        ]
        self.assertTrue(records)
        self.assertTrue(
            all(record["status"] == "verified" for record in records), audited.stderr
        )
