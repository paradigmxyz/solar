"""Per-import certificates must reject corrupted importers, not certify them."""

import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

import test_evm_regions as region_tests
from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, evm_region, mir
from validator.common import Unsupported


class CorrespondenceTests(unittest.TestCase):
    setUp = region_tests.EvmRegionTests.setUp
    command = region_tests.EvmRegionTests.command
    pair = region_tests.EvmRegionTests.pair

    def test_evm_certificate_covers_source_graph(self):
        folder = ROOT / "examples/evmir/cfg-region"
        _, record = self.pair(
            (folder / "before.evmir").read_text(), (folder / "after.evmir").read_text()
        )
        self.assertTrue(
            all(
                e["kind"] == "kernel-checked-cfg-expansion"
                for e in record["import_correspondence"]
            )
        )

    def test_corrupt_evm_tree_cannot_verify(self):
        original = evm_region.translate

        def corrupt(*args, **kwargs):
            program = original(*args, **kwargs)
            program.region_code = program.region_code.replace(
                ".push 17#256", ".push 99#256"
            )
            program.replay_ir = program.replay_ir.replace("push 17", "push 99")
            program.selector = "bb2"
            return program

        path = ROOT / "examples/evmir/cfg-region/before.evmir"
        replay = str(Path(SOLAR).with_name("solar-tv-replay"))
        with patch.object(evm_region, "translate", side_effect=corrupt):
            program = check.load("evmir", path, SOLAR, 30, region="bb0", stack_height=0)
        self.assertEqual(program.selector, "bb0")
        with self.assertRaisesRegex(Unsupported, "correspondence certificate failed"):
            check.check(
                program, program, 30, replay_command="unused", replay_executable=replay
            )

    def test_corrupt_evm_execution_function_cannot_verify(self):
        original = evm_region.translate

        def corrupt(*args, **kwargs):
            program = original(*args, **kwargs)
            program.body = "EVM.Memory.Execution.failed .underflow"
            return program

        path = ROOT / "examples/evmir/cfg-region/before.evmir"
        replay = str(Path(SOLAR).with_name("solar-tv-replay"))
        with patch.object(evm_region, "translate", side_effect=corrupt):
            program = check.load("evmir", path, SOLAR, 30, region="bb0", stack_height=0)
        with self.assertRaisesRegex(Unsupported, "correspondence certificate failed"):
            check.check(
                program, program, 30, replay_command="unused", replay_executable=replay
            )

    def test_corrupt_entry_interfaces_are_rejected(self):
        original_evm, original_mir = evm_region.translate, mir.translate

        def bad_evm(*args, **kwargs):
            program = original_evm(*args, **kwargs)
            program.inputs = program.inputs[:-1]
            return program

        def bad_mir(*args, **kwargs):
            program = original_mir(*args, **kwargs)
            program.interface = ("different_function", *program.interface[1:])
            return program

        with (
            patch.object(evm_region, "translate", side_effect=bad_evm),
            self.assertRaisesRegex(Unsupported, "selected entry interface"),
        ):
            check.load(
                "evmir",
                ROOT / "examples/evmir/cfg-region/before.evmir",
                SOLAR,
                30,
                region="bb0",
                stack_height=0,
            )
        with (
            patch.object(mir, "translate", side_effect=bad_mir),
            self.assertRaisesRegex(Unsupported, "selected function identity"),
        ):
            check.load("mir", ROOT / "examples/mir/dce/before.mir", SOLAR, 30)

    def test_mir_certificate_checks_independent_ssa_interpreter(self):
        folder = ROOT / "examples/mir/dce"
        report = self.folder / "mir.json"
        result = self.command(
            [
                CLI,
                "mir",
                folder / "before.mir",
                folder / "after.mir",
                "--solar",
                SOLAR,
                "--report",
                report,
            ]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        record = json.loads(report.read_text())
        self.assertTrue(
            all(
                e["kind"] == "kernel-checked-scalar-interpreter"
                for e in record["import_correspondence"]
            )
        )

    def test_corrupt_mir_expression_cannot_verify(self):
        replay = str(Path(SOLAR).with_name("solar-tv-replay"))
        program = check.load("mir", ROOT / "examples/mir/dce/before.mir", SOLAR, 30)
        self.assertTrue(program.import_obligation)
        program.body = "Outcome.returned (0#256)"
        with self.assertRaises(Unsupported):
            check.check(
                program, program, 30, replay_command="unused", replay_executable=replay
            )

    def test_real_cyclic_cfg_pair_is_unbounded(self):
        folder = ROOT / "examples/evmir/cyclic-region"
        before = (folder / "before.evmir").read_text()
        generated = self.command(
            [SOLAR, folder / "before.evmir", "-Ogas", "-Zevm-ir-pipeline=cfg-simplify"]
        )
        after = (folder / "after.evmir").read_text()
        self.assertEqual(
            generated.stdout[generated.stdout.index("@module") :],
            after[after.index("@module") :],
        )
        result, record = self.pair(before, after, height=1)
        self.assertIn("unbounded graph correspondence", result.stdout)
        self.assertIn("no fixed unrolling", " ".join(record["assumptions"]))
        self.assertNotIn("bb99", record["region"]["blocks_before"])
        self.pair(before, after.replace("  sub\n", "  add\n"), height=1, status=2)

    def test_loop_cannot_turn_into_success(self):
        self.pair(
            "@module Loop\nbb0:\n  jump bb0\n", "@module Loop\nbb0:\n  stop\n", status=2
        )
