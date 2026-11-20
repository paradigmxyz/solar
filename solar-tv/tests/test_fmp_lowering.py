"""Real constructor FMP lowering without guessed heap or constructor locations."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import mir_effects, mir_symbolic, native, prune
from validator.common import Unsupported


class FmpLoweringTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tmp = Path(temporary.name)
        self.folder = ROOT / "examples/mir/fmp-lowering"
        self.before = self.folder / "before.mir"
        self.after = self.folder / "after.mir"

    def pair(self, after=None, *, world=False):
        report = self.tmp / "audit.json"
        command = [
            CLI,
            "mir",
            self.before,
            after or self.after,
            "--solar",
            SOLAR,
            "--function=constructor",
            "--evm-version=cancun",
            "--timeout=30",
            "--report",
            report,
        ]
        if world:
            command += [
                "--world",
                self.folder / "world.json" if world is True else world,
            ]
        result = subprocess.run(
            command, text=True, capture_output=True, check=False, timeout=120
        )
        return result, json.loads(report.read_text())

    def test_real_lower_alloc_pair_keeps_unknown_context(self):
        emitted = subprocess.check_output(
            [
                SOLAR,
                self.before,
                "-j1",
                "-Ogas",
                "-Zmir-pipeline=lower-alloc",
                "--evm-version=cancun",
            ],
            text=True,
        )
        self.assertEqual(
            prune.declarations(emitted), prune.declarations(self.after.read_text())
        )
        result, record = self.pair()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["region"]["proof"], "compositional-effect-tree")
        self.assertEqual(record["gas_policy"], "semantic-effects")
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        self.assertEqual(
            [side["normalized_fmp_reads"] for side in record["import_correspondence"]],
            [1, 0],
        )
        self.assertEqual(
            [
                side["constructor_range_queries"]
                for side in record["import_correspondence"]
            ],
            [2, 2],
        )

    def test_wrong_fmp_slot_has_independent_counterexample(self):
        result, record = self.pair(self.folder / "broken.mir", world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_small_constructor_does_not_need_a_fabricated_context(self):
        self.before = self.tmp / "small-before.mir"
        text = """@module SmallConstructor
fn @constructor() -> i256 [constructor] {
bb0:
  v0 = constructor_args_base
  v1 = constructor_args_end
  v2 = sub v1, v0
  v3 = fmp
  v4 = ptrtoint ptr<memory> v3 to i256
  v5 = add v4, v2
  ret v5
}
"""
        self.before.write_text(text)
        after = self.tmp / "small-after.mir"
        after.write_text(
            text.replace(
                "v3 = fmp", "v6 = mload 64\n  v3 = inttoptr i256 v6 to ptr<memory>"
            )
        )
        result, record = self.pair(after)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["proof"], "compositional-effect-tree")
        self.assertNotIn("world", record)

    def test_constructor_base_and_end_remain_distinct(self):
        changed = self.tmp / "range.mir"
        changed.write_text(
            self.after.read_text().replace(
                "constructor_args_end", "constructor_args_base"
            )
        )
        result, record = self.pair(changed)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        result, record = self.pair(changed, world=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_heap_floor_is_not_substituted_for_the_live_fmp(self):
        changed = self.tmp / "heap.mir"
        changed.write_text(self.after.read_text().replace("mload 64", "heap_floor", 1))
        result, record = self.pair(changed)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_explicit_world_still_requires_its_constructor_binding(self):
        fixture = json.loads((self.folder / "world.json").read_text())
        del fixture["mir_layout"]
        world = self.tmp / "missing.json"
        world.write_text(json.dumps(fixture))
        result, record = self.pair(world=world)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("missing constructor range", result.stderr)

    def test_malformed_or_other_layout_queries_are_not_erased(self):
        replay = native.default(SOLAR)
        program = mir_symbolic.translate(
            self.before.read_text(), "constructor", replay, "cancun", 30
        )
        definitions = {row["name"]: row for row in native.catalog(replay, 30)["evmir"]}
        mutations = [
            ("layout", lambda i: i["operation"].update(end=1)),
            ("layout", lambda i: i["operation"].update(query="heap_floor")),
            ("layout", lambda i: i.update(type=None)),
            ("free_pointer", lambda i: i.update(type=None)),
            ("free_pointer", lambda i: i.update(result=None)),
        ]
        for kind, change in mutations:
            with self.subTest(kind=kind, change=change):
                model = copy.deepcopy(program.symbolic_mir["model"])
                inst = next(
                    i
                    for f in model["functions"]
                    for b in f["blocks"]
                    for i in b["instructions"]
                    if i["operation"]["kind"] == kind
                )
                change(inst)
                with self.assertRaises(Unsupported):
                    mir_effects.render(model, definitions)

    def test_live_pass_uses_certificate_without_changing_output(self):
        audits = self.tmp / "pairs"
        command = [
            SOLAR,
            self.before,
            "-j1",
            "-Ogas",
            "-Zmir-pipeline=lower-alloc",
            "--evm-version=cancun",
        ]
        outputs = []
        for flags in ([], [f"-Zsolar-tv={audits}"]):
            result = subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                text=True,
                capture_output=True,
                check=False,
                timeout=120,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            outputs.append(result.stdout)
        self.assertEqual(*outputs)
        records = [
            json.loads(p.read_text()) for p in (audits / "audits").glob("*.json")
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["status"], "verified")
        self.assertEqual(records[0]["region"]["proof"], "compositional-effect-tree")
