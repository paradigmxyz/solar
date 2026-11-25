"""Whole-module type erasure preserves resolved source data and every body."""

import copy
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
from validator import evaluate, native, prune, type_prune
from validator.common import Unsupported


class TypePruningTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.folder = ROOT / "examples/mir/type-pruning"
        self.paths = [self.folder / f"{side}.mir" for side in ("before", "after")]
        self.replay = native.default(SOLAR)

    def cli(self, paths=None, flags=("--prune",)):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *(paths or self.paths),
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=120,
        )
        self.assertTrue(report.exists(), result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_pass_resolves_every_function_and_keeps_module_scope(self):
        result = subprocess.run(
            [SOLAR, self.paths[0], "-j1", "-Ogas", "-Zmir-pipeline=lower-structs"],
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            prune.declarations(result.stdout),
            prune.declarations(self.paths[1].read_text()),
        )
        result, record = self.cli()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["region"]["kind"], "module")
        self.assertEqual(record["region"]["proof"], "resolved-type-table-erasure")
        self.assertEqual(record["region"]["removed_types"], ["struct0", "struct1"])
        self.assertEqual(record["region"]["functions"], ["entry", "f"])
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        evidence = record["import_correspondence"]
        self.assertEqual(evidence[0]["resolved_sha256"], evidence[1]["resolved_sha256"])
        self.assertEqual(evidence[0]["kind"], "kernel-checked-resolved-module-identity")
        self.assertTrue(any("every function" in a for a in record["assumptions"]))
        self.assertFalse(any(a == "sufficient gas" for a in record["assumptions"]))

    def test_changed_body_has_an_independent_runtime_counterexample(self):
        broken = self.folder / "broken.mir"
        result, record = self.cli([self.paths[0], broken])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("changed executable", record["diagnostic"])
        # The witness compares the same alias-free declaration context.
        result, record = self.cli([self.paths[1], broken], ("--function", "f"))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(record["witness"]["lean"], record["witness"]["rust"])

    def test_corrupt_resolved_data_cannot_hide_behind_equal_source_text(self):
        original = type_prune.export
        calls = 0

        def corrupt(*args):
            nonlocal calls
            model = copy.deepcopy(original(*args))
            calls += 1
            if calls == 2:
                inst = model["functions"][1]["blocks"][0]["instructions"][0]
                inst["operation"]["args"][1]["value"] = "8"
            return model

        with (
            patch.object(type_prune, "export", side_effect=corrupt),
            self.assertRaisesRegex(
                Unsupported, "resolved module identity proof did not close"
            ),
        ):
            type_prune.check(
                [p.read_text() for p in self.paths], "cancun", 30, self.replay
            )

    def test_used_types_and_partial_table_erasure_are_rejected(self):
        used = self.directory / "used.mir"
        used.write_text(self.paths[1].read_text().replace("-> i256", "-> struct0"))
        result, record = self.cli([self.paths[0], used])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
        partial = self.directory / "partial.mir"
        partial.write_text(
            self.paths[1]
            .read_text()
            .replace(
                "@module TypePruning", "@module TypePruning\n@types\n struct0: {i256}"
            )
        )
        result, record = self.cli([self.paths[0], partial])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("complete type table", record["diagnostic"])

    def test_globals_phase_order_and_unreachable_bodies_are_preserved(self):
        for old, new in (
            ("@module TypePruning", "@module Different"),
            ("@module TypePruning", "@module TypePruning\n@phase lowered"),
            ("ret v0", "ret 9"),
        ):
            with self.subTest(change=new):
                after = self.directory / "changed.mir"
                after.write_text(self.paths[1].read_text().replace(old, new))
                result, record = self.cli([self.paths[0], after])
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(record["status"], "unsupported")
        before = self.directory / "with-dead.mir"
        before.write_text(self.paths[0].read_text() + "fn @dead() {\nbb0:\nret\n}\n")
        result, record = self.cli([before, self.paths[1]])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("changed executable", record["diagnostic"])

    def test_structural_export_retains_gas_and_cycles_without_executing_them(self):
        text = "@module Gas\nfn @entry() [entry] {\nbb0:\nstop\n}\nfn @observe() -> i256 {\nbb0:\nv0 = gas\nv1 = add v0, 0\nret v1\n}\nfn @loop() {\nbb0:\njump bb1\nbb1:\njump bb1\n}\n"
        model = type_prune.export(text, self.replay, 30)
        self.assertEqual(len(model["functions"]), 3)
        self.assertEqual(
            model["functions"][1]["blocks"][0]["instructions"][0]["operation"]["name"],
            "gas",
        )
        old = subprocess.run(
            [self.replay, "--mir-model"],
            input=json.dumps({"version": 1, "text": text, "function": "observe"}),
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(json.loads(old.stdout)["status"], "unsupported")
        self.assertIn("GAS requires an isolated", json.loads(old.stdout)["reason"])
        selected = subprocess.run(
            [self.replay, "--mir-module-model"],
            input=json.dumps({"version": 1, "text": text, "function": "loop"}),
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(json.loads(selected.stdout)["status"], "unsupported")

    def test_live_audit_keeps_bytecode_and_capture_identity(self):
        command = [
            SOLAR,
            self.paths[0],
            "-j1",
            "-Ogas",
            "-Zmir-pipeline=lower-structs,lower-evm-shaped",
            "--emit=bin",
        ]
        audit, capture = [self.directory / n for n in ("audit", "capture")]
        outputs = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=120,
            )
            for flags in (
                [],
                [f"-Zsolar-tv={audit}"],
                [f"-Zsolar-tv={capture}", "-Zsolar-tv-capture"],
            )
        ]
        for result in outputs:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(outputs[0].stdout, outputs[1].stdout)
        self.assertEqual(outputs[0].stdout, outputs[2].stdout)
        records = [json.loads(p.read_text()) for p in (audit / "audits").glob("*.json")]
        self.assertTrue(
            any(
                row["status"] == "verified"
                and row.get("region", {}).get("proof") == "resolved-type-table-erasure"
                for row in records
            )
        )
        rows = evaluate.captures(capture)
        self.assertTrue(
            any(
                row["pass"] == "lower-structs"
                and row["region"] is None
                and row["rejection"]
                == "shared MIR declarations or phase changed; no function-local claim"
                for row in rows
            )
        )

    def test_kernel_data_identity_rejects_changed_resolved_types(self):
        path = self.directory / "Types.lean"
        path.write_text("""import SolarTV
open SolarTV
def word : MIR.State.Function := ⟨[],[.word 256],[],none⟩
def pointer : MIR.State.Function := ⟨[],[.pointer 0],[],none⟩
def a : MIR.TypeDCE.Program := ⟨[],[(0,word)]⟩
def b : MIR.TypeDCE.Program := ⟨[],[(0,pointer)]⟩
-- Equal widths do not erase resolved type identity.
example : a ≠ b := by
 intro same
 have h := congrArg (fun p : MIR.TypeDCE.Program => match p.functions with
   | (_,f) :: _ => match f.returns with | [.word _] => true | _ => false
   | [] => false) same
 change true = false at h
 cases h
#print axioms MIR.TypeDCE.certificate
""")
        result = subprocess.run(
            ["lake", "env", "lean", path],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("sorryAx", result.stdout)
