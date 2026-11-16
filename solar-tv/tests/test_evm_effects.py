"""Effectful stack rewrites retain memory/log traffic, faults and gas charges."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evm_effects, native
from validator.common import Unsupported


class EffectPrefixTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.example = ROOT / "examples/evmir/log-stack"

    def cli(self, paths, *flags):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--solar",
                SOLAR,
                "--report",
                report,
                "--timeout",
                "30",
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=100,
        )
        self.assertTrue(report.exists(), result.stderr)
        return result, json.loads(report.read_text())

    def test_real_peephole_and_universal_resource_proof(self):
        before, after = [self.example / f"{name}.evmir" for name in ("before", "after")]
        compiled = subprocess.run(
            [SOLAR, before, "-Ogas", "-Zevm-ir-pipeline=peephole"],
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )
        self.assertEqual(compiled.returncode, 0, compiled.stderr)
        normalize = lambda text: [
            line.strip()
            for line in text.splitlines()
            if line.strip() and not line.lstrip().startswith(";")
        ]
        self.assertEqual(normalize(compiled.stdout), normalize(after.read_text()))
        result, record = self.cli([before, after], "--prefix", "bb0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["gas_policy"], "refine")
        self.assertEqual(record["region"]["proof"], "universal-effectful-stack-prefix")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertFalse(record["whole_pass"])
        self.assertNotIn("world", record)
        self.assertEqual(
            record["import_correspondence"][0]["kind"],
            "independent-effectful-block-prefix",
        )
        exact, _ = self.cli([before, after], "--prefix", "bb0", "--gas", "exact")
        self.assertEqual(exact.returncode, 2)
        future, report = self.cli(
            [before, after], "--prefix", "bb0", "--evm-version", "amsterdam"
        )
        self.assertEqual(future.returncode, 2)
        self.assertIn("two-resource gas", report["diagnostic"])

    def test_changed_log_cannot_claim_a_universal_proof(self):
        before = self.example / "before.evmir"
        broken = self.directory / "broken.evmir"
        broken.write_text(
            (self.example / "after.evmir").read_text().replace("fd42a43", "fd42a44")
        )
        result, record = self.cli([before, broken], "--prefix", "bb0")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("no reachable counterexample established", result.stderr)

    def entry(self, path, *, broken=False):
        text = path.read_text().replace("[stack=6]", "[stack=0]")
        # Concrete safe memory pointers propose a witness, never a proof precondition.
        setup = "\n".join(
            [
                "push 256",
                "push 64",
                "mstore",
                *[f"push {value}" for value in (768, 832, 896, 960, 1024, 1088)],
            ]
        )
        text = text.replace("bb0 [stack=0]:", "bb0 [stack=0]:\n" + setup)
        if broken:
            text = text.replace("fd42a43", "fd42a44")
        out = self.directory / ("broken.evmir" if broken else path.name)
        out.write_text(text)
        return out

    def test_broken_log_is_replayed_independently(self):
        before = self.entry(self.example / "before.evmir")
        after = self.entry(self.example / "after.evmir")
        world = self.directory / "world.json"
        world.write_text(json.dumps({"version": 1, "gas": 500000}))
        valid, _ = self.cli([before, after], "--world", world, "--gas", "refine")
        self.assertEqual(valid.returncode, 0, valid.stderr)
        broken = self.entry(self.example / "after.evmir", broken=True)
        result, record = self.cli([before, broken], "--world", world)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_more_expensive_candidate_does_not_refine(self):
        body = "push 64\nmload\ndup 1\nmstore\n"
        paths = []
        for side, code in (("before", body), ("after", body + "push 1\npop\n")):
            path = self.directory / f"{side}.evmir"
            path.write_text("@module Cost\nbb0:\n" + code + "stop\n")
            paths.append(path)
        result, record = self.cli(paths, "--prefix", "bb0")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("effectful stack proof did not close", result.stderr)

    def test_live_pass_keeps_output(self):
        out = self.directory / "audit"
        command = [
            SOLAR,
            self.example / "before.evmir",
            "-Ogas",
            "-Zevm-ir-pipeline=peephole",
        ]
        results = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                capture_output=True,
                text=True,
                check=False,
                timeout=100,
            )
            for flags in ([], [f"-Zsolar-tv={out}"])
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (out / "audits").glob("*.json")
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["status"], "verified")
        self.assertEqual(
            records[0]["region"]["proof"], "universal-effectful-stack-prefix"
        )

    def test_control_transfers_and_gas_observers_are_not_effects(self):
        catalog = {
            row["name"]: row
            for row in native.catalog(native.default(SOLAR), 20)["evmir"]
        }
        for instruction in ("jump", "jumpi", "gas", "sstore", "staticcall", "codesize"):
            with self.subTest(instruction=instruction), self.assertRaises(Unsupported):
                evm_effects.instructions([instruction], catalog, "cancun")


if __name__ == "__main__":
    unittest.main()
