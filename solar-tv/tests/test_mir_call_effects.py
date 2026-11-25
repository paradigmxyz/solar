"""Supplied MIR callees retain allocation requests and caller continuations."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import mir_effects, mir_symbolic, native
from validator.common import Unsupported


def program(body, helpers=""):
    return "@module Calls\nfn @f() -> i256 {\nbb0:\n" + body + "\n}\n" + helpers


class MirCallEffectTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.replay = native.default(SOLAR)
        self.folder = ROOT / "examples/mir/domain-separator"

    def model(self, text):
        return mir_symbolic.translate(text, "f", self.replay, "cancun", 20)

    def pair(self, before, after, *, succeeds=True):
        args = [self.model(source) for source in (before, after)]
        if succeeds:
            self.assertEqual(
                mir_effects.check(*args, 20, self.replay).status, "verified"
            )
        else:
            with self.assertRaisesRegex(Unsupported, "effect proof did not close"):
                mir_effects.check(*args, 20, self.replay)

    def cli(self, after, *flags):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                self.folder / "before.mir",
                after,
                "--solar",
                SOLAR,
                "--function",
                "DOMAIN_SEPARATOR",
                "--timeout",
                "30",
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=100,
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_sccp_pair_and_broken_callee(self):
        emitted = subprocess.run(
            [SOLAR, self.folder / "before.mir", "-Ogas", "-Zmir-pipeline=sccp"],
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )
        self.assertEqual(emitted.returncode, 0, emitted.stderr)
        normalize = lambda text: [
            line.split(";", 1)[0].strip()
            for line in text.splitlines()
            if line.split(";", 1)[0].strip()
        ]
        self.assertEqual(
            normalize(emitted.stdout),
            normalize((self.folder / "after.mir").read_text()),
        )
        result, record = self.cli(self.folder / "after.mir")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["region"]["proof"], "compositional-effect-tree")
        self.assertEqual(record["gas_policy"], "semantic-effects")
        self.assertNotIn("world", record)
        for side in record["import_correspondence"]:
            self.assertEqual(side["kind"], "independent-call-effect-tree")
            self.assertEqual(side["supplied_functions"], 2)
            self.assertEqual(side["allocation_sites"], 1)
        bad, report = self.cli(
            self.folder / "broken.mir", "--world", self.folder / "world.json"
        )
        self.assertEqual(bad.returncode, 1, bad.stderr)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_calls_preserve_arguments_locals_and_shared_effect_state(self):
        helpers = """fn @write(arg0: i256) -> i256 {
bb0:
  v0 = add arg0, 1
  sstore 0, v0
  ret v0
}
fn @twice(arg0: i256) -> i256 {
bb0:
  v0 = icall @write, arg0
  v1 = icall @write, v0
  ret v1
}
"""
        before = program("v0 = icall @twice, 7\nv1 = add v0, 2\nret v1", helpers)
        self.pair(before, program("sstore 0, 8\nsstore 0, 9\nret 11"))
        self.pair(before, program("sstore 0, 9\nsstore 0, 8\nret 11"), succeeds=False)

    def test_callee_frame_endings_do_not_resume_the_caller(self):
        for ending in ("stop", "revert 0, 0", "returndata 0, 0", "invalid"):
            with self.subTest(ending=ending):
                helper = "fn @end() {\nbb0:\nsstore 0, 7\n" + ending + "\n}\n"
                self.pair(
                    program("icall @end\nsstore 0, 9\nret 0", helper),
                    program("sstore 0, 7\n" + ending),
                )

    def test_allocation_size_policy_and_aliases_are_observable(self):
        helper = "fn @reserve() -> ptr<memory> {\nbb0:\nv0 = alloc checked 32 !metadata(preserves_fmp)\nret v0\n}\n"
        body = "v0 = icall @reserve\nv1 = icall @reserve\nmstore v0, 3\nmstore v1, 7\nv2 = mload v0\nret v2"
        before = program(body, helper)
        self.pair(
            before,
            program(body.replace("mstore v1, 7", "mstore v0, 7"), helper),
            succeeds=False,
        )
        for old, new in (
            ("checked 32", "checked 64"),
            ("alloc checked", "alloc"),
            ("alloc checked", "alloc zeroed checked"),
        ):
            with self.subTest(policy=new):
                self.pair(before, before.replace(old, new), succeeds=False)

    def test_unused_allocation_cannot_disappear(self):
        before = program("v0 = alloc checked 32 !metadata(preserves_fmp)\nret 0")
        self.pair(before, program("ret 0"), succeeds=False)

    def test_mask_removal_requires_a_reached_width_bound(self):
        guard = "v1 = gt v0, 255"
        mask = "  v2 = and v0, 255\n"
        before = program(f"""v0 = calldataload 0
{guard}
jumpi v1, bb1, bb2
bb1:
  revert 0, 0
bb2:
{mask}  mstore 0, v2
  returndata 0, 32""")
        after = before.replace(mask, "").replace("mstore 0, v2", "mstore 0, v0")
        self.pair(before, after)
        self.pair(before.replace("and v0, 255", "and 255, v0"), after)

        widened = "v1 = gt v0, 256"
        invalid = [text.replace(guard, widened) for text in (before, after)]
        self.pair(*invalid, succeeds=False)

        paths = [self.directory / f"{side}.mir" for side in ("before", "after")]
        for path, text in zip(paths, invalid, strict=True):
            path.write_text(text)
        world = self.directory / "world.json"
        world.write_text(json.dumps({"version": 1, "calldata": f"0x{256:064x}"}))
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *paths,
                "--solar",
                SOLAR,
                "--function",
                "f",
                "--world",
                world,
                "--timeout",
                "20",
                "--report",
                report,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        record = json.loads(report.read_text())
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        witness = record["witness"]
        self.assertEqual(witness["observations"], witness["native_observations"])

    def test_mir_interaction_trace_preserves_the_call_gas_operand(self):
        before = self.directory / "before.mir"
        after = self.directory / "after.mir"
        report = self.directory / "report.json"
        before.write_text(
            program(
                "v0 = gas\nv1 = add v0, 0\nv2 = staticcall v1, 4, 0, 0, 0, 0\nret 0"
            )
        )
        equivalent = program("v0 = gas\nv2 = staticcall v0, 4, 0, 0, 0, 0\nret 0")
        changed = before.read_text().replace("add v0, 0", "add v0, 1")
        command = [
            CLI,
            "mir",
            before,
            after,
            "--solar",
            SOLAR,
            "--function",
            "f",
            "--interaction-trace",
            "--timeout",
            "20",
            "--report",
            report,
        ]
        for text, status in ((equivalent, 0), (changed, 2)):
            after.write_text(text)
            result = subprocess.run(
                command, capture_output=True, text=True, check=False, timeout=90
            )
            self.assertEqual(result.returncode, status, result.stdout + result.stderr)
            if status == 0:
                record = json.loads(report.read_text())
                self.assertEqual(record["gas_policy"], "unpriced-shared-interactions")
                self.assertEqual(
                    record["region"],
                    {
                        "kind": "function",
                        "name": "f",
                        "proof": "kernel-checked-mir-interaction-effects",
                        "stack_height": None,
                    },
                )

    def test_unmodeled_callee_instructions_and_excess_expansion_are_rejected(self):
        definitions = {
            row["name"]: row for row in native.catalog(self.replay, 20)["evmir"]
        }
        helper = "fn @end() {\nbb0:\nv0 = heap_floor\nret\n}\n"
        model = self.model(program("icall @end\nret 0", helper))
        with self.assertRaisesRegex(Unsupported, "explicit constructor-range query"):
            mir_effects.render(model.symbolic_mir["model"], definitions)
        helper = "fn @empty() {\nbb0:\nret\n}\n"
        model = self.model(program("icall @empty\n" * 100 + "ret 0", helper))
        with self.assertRaisesRegex(Unsupported, "expansion exceeds nesting 128"):
            mir_effects.render(model.symbolic_mir["model"], definitions)

    def test_interaction_trace_checks_retained_callee_bodies(self):
        before, after, report = (
            self.directory / name for name in ("before.mir", "after.mir", "check.json")
        )
        helper = """fn @write(arg0: i256) {
  bb0:
    jump bb1
  bb1:
    v0 = phi [bb0: arg0], [bb2: v1]
    v2 = ne v0, 0
    jumpi v2, bb2, bb3
  bb2:
    sstore v0, 1
    v1 = sub v0, 1
    jump bb1
  bb3:
    ret
}
"""
        caller = "icall @write, 2\nv0 = gas\n"
        before.write_text(program(caller + "v1 = add v0, 0\nret v1", helper))
        equivalent = program(caller + "ret v0", helper)
        command = [
            CLI,
            "mir",
            before,
            after,
            "--solar",
            SOLAR,
            "--function",
            "f",
            "--interaction-trace",
            "--timeout",
            "20",
            "--report",
            report,
        ]
        for source, status in (
            (equivalent, 0),
            (equivalent.replace("sstore v0, 1", "sstore v0, 2"), 2),
        ):
            after.write_text(source)
            result = subprocess.run(
                command, capture_output=True, text=True, check=False, timeout=90
            )
            self.assertEqual(result.returncode, status, result.stdout + result.stderr)
            record = json.loads(report.read_text())
            if status == 0:
                self.assertEqual(record["gas_policy"], "unpriced-shared-interactions")
                self.assertEqual(
                    [
                        side["retained_closed_callees"]
                        for side in record["import_correspondence"]
                    ],
                    [[1], [1]],
                )
            else:
                self.assertIn(
                    "effect-tree cycles require an invariant", record["diagnostic"]
                )

    def test_live_pass_keeps_output(self):
        audit = self.directory / "audit"
        command = [SOLAR, self.folder / "before.mir", "-Ogas", "-Zmir-pipeline=sccp"]
        runs = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR": SOLAR, "SOLAR_TV": CLI},
                capture_output=True,
                text=True,
                check=False,
                timeout=100,
            )
            for flags in ([], [f"-Zsolar-tv={audit}"])
        ]
        for result in runs:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(runs[0].stdout, runs[1].stdout)
        reports = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertTrue(reports)
        self.assertTrue(all(report["status"] == "verified" for report in reports))
        self.assertIn(
            "compositional-effect-tree",
            [report["region"]["proof"] for report in reports],
        )

    def test_continuation_theorem_has_no_axioms(self):
        path = self.directory / "Axioms.lean"
        path.write_text(
            "import SolarTV\n"
            "/-- info: 'SolarTV.MIR.Effects.interpret_resume' does not depend on any axioms -/\n"
            "#guard_msgs in\n#print axioms SolarTV.MIR.Effects.interpret_resume\n"
        )
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
