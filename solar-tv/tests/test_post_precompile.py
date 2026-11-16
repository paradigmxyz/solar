"""Post-call suffix proofs bind the full common prefix and its live state."""

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR


class PostPrecompileTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.folder = ROOT / "examples/evmir/post-precompile"
        self.paths = [self.folder / f"{side}.evmir" for side in ("before", "after")]

    def cli(self, paths, *flags):
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--solar",
                SOLAR,
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

    def test_real_peephole_and_shared_prefix_certificate(self):
        compiled = subprocess.run(
            [SOLAR, self.paths[0], "-Ogas", "-Zevm-ir-pipeline=peephole"],
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
        self.assertEqual(
            normalize(compiled.stdout), normalize(self.paths[1].read_text())
        )
        for flags in (("--prefix", "bb0"), ("--window", "bb0", "--stack-height", "0")):
            with self.subTest(flags=flags):
                result, record = self.cli(self.paths, *flags)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    record["region"]["proof"], "universal-post-precompile-suffix"
                )
                self.assertIsNone(record["region"]["stack_height"])
                self.assertEqual(record["gas_policy"], "refine")
                self.assertFalse(record["whole_pass"])
                self.assertNotIn("world", record)
                self.assertNotIn("sufficient gas", record["assumptions"])
                before, after = record["import_correspondence"]
                self.assertEqual(
                    before["shared_prefix_sha256"], after["shared_prefix_sha256"]
                )
                self.assertEqual(before["recovery_calls"], 1)
                # Include the precompile, GAS, RETURNDATASIZE and following load.
                self.assertEqual(before["shared_prefix_operations"], 15)
                self.assertEqual(
                    [before["suffix_operations"], after["suffix_operations"]], [4, 3]
                )

    def test_broken_predicate_is_not_a_universal_proof(self):
        result, record = self.cli(
            [self.paths[0], self.folder / "broken.evmir"], "--prefix", "bb0"
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("no reachable counterexample established", result.stderr)

    def test_broken_predicate_is_replayed_and_prefix_oog_is_preserved(self):
        result, record = self.cli(
            [self.paths[0], self.folder / "broken.evmir"],
            "--world",
            self.folder / "world.json",
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )
        low = self.directory / "low-gas.json"
        low.write_text(json.dumps({"version": 1, "gas": 1}))
        result, _ = self.cli(self.paths, "--world", low, "--gas", "exact")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_prefix_changes_and_unbound_call_targets_are_rejected(self):
        changes = [
            (None, "  push 7\n", "  push 8\n"),
            ("both", "  push 1\n  gas", "  push 65536\n  gas"),
            ("both", "  push 7\n", "  codesize\n  pop\n  push 7\n"),
            (None, "  gas\n", "  push 100\n"),
        ]
        for sides, old, new in changes:
            with self.subTest(change=new):
                paths = []
                for side, source in zip(("before", "after"), self.paths, strict=True):
                    path = self.directory / f"{side}.evmir"
                    text = source.read_text()
                    path.write_text(
                        text.replace(old, new, 1)
                        if sides == "both" or side == "after"
                        else text
                    )
                    paths.append(path)
                result, record = self.cli(paths, "--prefix", "bb0")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(record["status"], "unsupported")
                self.assertIn("code-image/call correspondence", result.stderr)

    def test_exact_and_two_resource_gas_are_not_silently_weakened(self):
        for flags, diagnostic in (
            (("--gas", "exact"), "not exact gas"),
            (("--evm-version", "amsterdam"), "two-resource gas"),
        ):
            with self.subTest(flags=flags):
                result, record = self.cli(self.paths, "--prefix", "bb0", *flags)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(diagnostic, record["diagnostic"])

    def test_live_pass_keeps_output(self):
        out = self.directory / "audit"
        command = [SOLAR, self.paths[0], "-Ogas", "-Zevm-ir-pipeline=peephole"]
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
            records[0]["region"]["proof"], "universal-post-precompile-suffix"
        )

    def test_composition_and_target_lemmas_have_only_kernel_axioms(self):
        path = self.directory / "Axioms.lean"
        path.write_text(
            "import SolarTV\n"
            + "\n".join(
                f"#print axioms SolarTV.EVM.Suffix.{name}"
                for name in (
                    "resource",
                    "stopped",
                    "recovery_target",
                    "recovery_active",
                )
            )
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
        lines = result.stdout.splitlines()
        self.assertEqual(len(lines), 4)
        for line in lines:
            if "does not depend on any axioms" in line:
                continue
            axioms = re.search(r"depends on axioms: \[(.*?)\]", line)
            self.assertIsNotNone(axioms, line)
            self.assertLessEqual(
                set(axioms[1].split(", ")),
                {"propext", "Quot.sound", "Classical.choice"},
            )


if __name__ == "__main__":
    unittest.main()
