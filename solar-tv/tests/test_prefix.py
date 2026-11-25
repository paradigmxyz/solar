"""Real block prefixes, universal stacks and conservative contextual boundaries."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR


class PrefixTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.report = self.directory / "report.json"

    def check(self, before, after, *flags):
        result = subprocess.run(
            [
                CLI,
                "evmir",
                before,
                after,
                "--solar",
                SOLAR,
                "--prefix",
                "bb0",
                "--report",
                self.report,
                "--timeout",
                "15",
                *flags,
            ],
            capture_output=True,
            text=True,
            timeout=90,
            check=False,
        )
        return result, json.loads(self.report.read_text())

    def pair(self, before, after, *, height=5):
        paths = []
        for side, body in (("before", before), ("after", after)):
            path = self.directory / f"{side}.evmir"
            path.write_text(f"@module Prefix\nbb0 [stack={height}]:\n{body}\n")
            paths.append(path)
        return paths

    def test_invalid_entry_is_reported_without_a_traceback(self):
        example = ROOT / "examples/evmir/prefix"
        for entry in ["entry", "bb4294967296"]:
            result, record = self.check(
                example / "before.evmir", example / "after.evmir", "--prefix", entry
            )
            self.assertEqual(result.returncode, 2)
            self.assertEqual(record["status"], "unsupported")
            self.assertIn("valid block label", result.stderr)

    def test_real_block_cse_prefix_and_stateful_suffix(self):
        example = ROOT / "examples/evmir/prefix"
        before, after = example / "before.evmir", example / "after.evmir"
        compiled = subprocess.run(
            [SOLAR, before, "-Ogas", "-Zevm-ir-pipeline=block-cse"],
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(compiled.returncode, 0, compiled.stderr)
        # Match the actual pass output, omitting its path banner and formatting.
        canonical = lambda text: [
            line.strip()
            for line in text.splitlines()
            if line.strip() and not line.startswith(";")
        ]
        self.assertEqual(canonical(compiled.stdout), canonical(after.read_text()))
        result, record = self.check(before, after)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "verified\n")
        self.assertEqual(record["gas_policy"], "exact")
        self.assertEqual(record["region"]["proof"], "universal-stack-prefix")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertFalse(record["whole_pass"])
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["import_correspondence"][0]["prefix_operations"], 3)
        self.assertIn(
            "arbitrary live world; no zero-memory or empty-storage assumption",
            record["assumptions"],
        )
        self.assertNotIn("sufficient gas", record["assumptions"])

    def test_broken_prefix_cannot_claim_a_reachable_counterexample(self):
        example = ROOT / "examples/evmir/prefix"
        result, record = self.check(example / "before.evmir", example / "broken.evmir")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(record["status"], "unsupported")
        self.assertEqual(result.stdout, "")
        self.assertIn("no reachable counterexample established", result.stderr)

    def test_cleanup_proves_every_height_with_resource_policy(self):
        paths = self.pair(
            "pop\nswap 1\npop\nswap 1\npop\njump",
            "pop\nswap 2\npop\npop\njump",
        )
        result, record = self.check(*paths)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record["gas_policy"], "refine")
        self.assertIn(
            "sufficient gas; resource refinement and gas-insensitive continuation",
            record["assumptions"],
        )
        result, record = self.check(*paths, "--gas=exact")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("requested gas policy", result.stderr)

    def test_cheaper_prefix_cannot_hide_a_visible_gas_observer(self):
        for suffix in ["gas\npop\nstop", "sstore\nstop"]:
            paths = self.pair(
                "pop\nswap 1\npop\nswap 1\npop\n" + suffix,
                "pop\nswap 2\npop\npop\n" + suffix,
            )
            result, record = self.check(*paths)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(record["status"], "unsupported")
            self.assertIn("gas-sensitive continuation", result.stderr)

    def test_prefix_retains_underflow_and_full_stack_faults(self):
        for before, after in [
            ("push 1\npop\nstop", "dup 1\npop\nstop"),
            ("push 1\npop\nstop", "stop"),
        ]:
            with self.subTest(after=after):
                paths = self.pair(before, after)
                result, record = self.check(*paths)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(record["status"], "unsupported")
                self.assertNotEqual(result.stdout, "verified\n")

    def test_changed_effect_or_terminal_is_not_an_unchanged_suffix(self):
        for after in [
            "push 32\ndup 1\nmstore8\nstop",
            "push 32\ndup 1\nmstore\ninvalid",
        ]:
            paths = self.pair("push 32\npush 32\nmstore\nstop", after)
            result, record = self.check(*paths)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(record["status"], "unsupported")
            self.assertIn("continuation changed", result.stderr)

    def test_code_observers_and_calls_require_correspondence(self):
        for suffix in [
            "pc\npop\nstop",
            "codesize\npop\nstop",
            "push 0\nextcodehash\npop\nstop",
            "staticcall\npop\nstop",
        ]:
            paths = self.pair(
                "push 32\npush 32\n" + suffix, "push 32\ndup 1\n" + suffix, height=8
            )
            result, record = self.check(*paths)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(record["status"], "unsupported")
            self.assertIn("code-image/call correspondence", result.stderr)

    def test_pass_hook_checks_unknown_height_without_changing_code(self):
        # A label copied as a word deliberately defeats exact-height inference.
        # The prefix proof must quantify over stacks, not trust the entry guess.
        source = self.directory / "input.evmir"
        source.write_text("""@module Hook
bb0 [stack=5]:
    push bb1
    swap 1
    jump
bb1:
    push 101
    push 32
    push 32
    log3
    stop
""")
        commands = [SOLAR, source, "-Ogas", "-Zevm-ir-pipeline=block-cse", "--emit=bin"]
        environment = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
        baseline = subprocess.run(
            commands, text=True, capture_output=True, timeout=30, check=False
        )
        audited = subprocess.run(
            [*commands, f"-Zsolar-tv={self.directory / 'audit'}"],
            env=environment,
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
        self.assertTrue(
            any(
                record["status"] == "verified"
                and record.get("region", {}).get("proof") == "universal-stack-prefix"
                for record in records
            ),
            audited.stderr,
        )
