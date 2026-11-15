"""Typed CFG imports must agree with the independent SSA interpreter."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, mir, mir_state
from validator.common import Unsupported


class TypedCorrespondenceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)
        self.replay = str(Path(SOLAR).with_name("solar-tv-replay"))

    def pair(self, name, after="after", *, status=0, extra=()):
        folder = ROOT / "examples/mir" / name
        report = self.folder / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                folder / "before.mir",
                folder / f"{after}.mir",
                "--solar",
                SOLAR,
                "--report",
                report,
                *extra,
            ],
            text=True,
            capture_output=True,
            timeout=180,
            check=False,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return json.loads(report.read_text())

    def test_real_phi_cast_and_checked_pairs_are_certified(self):
        for name in ("phi", "switch", "typed-branch", "casts", "checked"):
            with self.subTest(name=name):
                record = self.pair(name)
                certificates = record["import_correspondence"]
                self.assertEqual(len(certificates), 2)
                self.assertTrue(
                    all(
                        evidence["kind"] == "kernel-checked-scalar-interpreter"
                        for evidence in certificates
                    )
                )

    def test_broken_pairs_still_have_independent_counterexamples(self):
        for name in ("phi", "typed-branch", "checked"):
            with self.subTest(name=name):
                record = self.pair(name, "broken", status=1)
                self.assertEqual(record["witness"]["lean"], record["witness"]["rust"])
                self.assertTrue(all(record["import_correspondence"]))

    def test_corrupt_phi_branch_and_check_imports_cannot_verify(self):
        original = mir.translate
        for name, old, new in (
            ("phi", "MIR.phi 1", "MIR.phi 2"),
            (
                "typed-branch",
                "MIR.branch arg0 ",
                "MIR.branch (MIR.eval .xor arg0 1#1) ",
            ),
            (
                "checked",
                "MIR.checked .add false",
                "(fun a b => Outcome.returned (a + b))",
            ),
        ):
            with self.subTest(name=name):

                def corrupt(*args, _old=old, _new=new, **kwargs):
                    program = original(*args, **kwargs)
                    self.assertIn(_old, program.body)
                    program.body = program.body.replace(_old, _new)
                    return program

                with patch.object(mir, "translate", side_effect=corrupt):
                    program = check.load(
                        "mir", ROOT / f"examples/mir/{name}/before.mir", SOLAR, 60
                    )
                with self.assertRaisesRegex(
                    Unsupported, "correspondence certificate failed"
                ):
                    check.check(
                        program,
                        program,
                        60,
                        replay_command="unused",
                        replay_executable=self.replay,
                    )

    def test_typed_input_retagging_is_rejected(self):
        original = mir.translate

        def corrupt(*args, **kwargs):
            program = original(*args, **kwargs)
            program.input_widths = (256, 160)
            return program

        with (
            patch.object(mir, "translate", side_effect=corrupt),
            self.assertRaisesRegex(Unsupported, "typed scalar interface"),
        ):
            check.load("mir", ROOT / "examples/mir/typed-branch/before.mir", SOLAR, 60)

    def test_execution_exhaustion_is_not_a_scalar_outcome(self):
        original = mir_state.translate

        def exhausted(*args, **kwargs):
            source = original(*args, **kwargs)
            self.assertIn("MIR.State.execute 512 ", source.body)
            source.body = source.body.replace(
                "MIR.State.execute 512 ", "MIR.State.execute 0 "
            )
            return source

        with patch.object(mir_state, "translate", side_effect=exhausted):
            program = check.load("mir", ROOT / "examples/mir/phi/before.mir", SOLAR, 60)
        with self.assertRaisesRegex(Unsupported, "correspondence certificate failed"):
            check.check(
                program,
                program,
                60,
                replay_command="unused",
                replay_executable=self.replay,
            )

    def test_direct_state_switch_pair(self):
        fixture = self.folder / "world.json"
        fixture.write_text('{"version":1}')
        self.pair("phi", extra=("--world", fixture, "--function", "pick"))
        record = self.pair(
            "phi", "broken", status=1, extra=("--world", fixture, "--function", "pick")
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )
