"""Real Solidity pass audits, independent replay and conservative region boundaries."""

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

# Retain scalar arithmetic until EVM peephole, while performing the required
# conversions. This exercises the backend rather than folding the example in MIR.
CONVERSIONS = (
    "lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,"
    "lower-dispatch,lower-structs,lower-frame-slots,lower-memory-objects,"
    "lower-slices,lower-immutables,lower-mcopy,lower-alloc,lower-memory-zero,"
    "lower-integers,lower-shifts,inline-physical,cfg-simplify,lower-evm-shaped"
)


class RegionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.audits = self.directory / "tv"

    def run_command(self, command):
        return subprocess.run(
            command,
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            timeout=180,
            check=False,
        )

    def compile(self, example, flags=(), *, audit=False):
        args = [
            SOLAR,
            ROOT / "examples/solidity" / example,
            "-Ogas",
            "--allow=2264",
            "--evm-version=cancun",
            *flags,
        ]
        if audit:
            args.append(f"-Zsolar-tv={self.audits}")
        result = self.run_command(args)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def records(self):
        return [
            json.loads(path.read_text())
            for path in (self.audits / "audits").glob("*.json")
        ]

    def test_solidity_function_audits_keep_bytecode(self):
        flags = ["--emit=bin,bin-runtime"]
        before = self.compile("functions.sol", flags)
        after = self.compile("functions.sol", flags, audit=True)
        self.assertEqual(json.loads(before.stdout), json.loads(after.stdout))
        records = self.records()
        proved = [record for record in records if record["status"] == "verified"]
        self.assertTrue(
            any(record.get("region", {}).get("name") == "helper" for record in proved)
        )
        self.assertTrue(any(record["status"] == "unsupported" for record in records))
        self.assertTrue(all(record["whole_pass"] is False for record in records))
        self.assertTrue(
            all(record["region"]["kind"] == "function" for record in proved)
        )

    def test_solidity_evm_windows_keep_bytecode(self):
        flags = [
            "--emit=bin,bin-runtime",
            f"-Zmir-pipeline={CONVERSIONS}",
            "-Zevm-ir-pipeline=peephole",
        ]
        before = self.compile("windows.sol", flags)
        after = self.compile("windows.sol", flags, audit=True)
        self.assertEqual(json.loads(before.stdout), json.loads(after.stdout))
        proved = [
            record
            for record in self.records()
            if record["layer"] == "evmir" and record["status"] == "verified"
        ]
        self.assertTrue(proved, after.stderr)
        self.assertTrue(all(record["region"]["stack_height"] > 0 for record in proved))
        self.assertTrue(all(record["evidence"] == "kernel-proof" for record in proved))

    def test_function_witness_replays_full_solidity_module(self):
        compiled = self.compile("functions.sol", ["-Zmir-pipeline=none"])
        text = compiled.stdout[compiled.stdout.index("@module ") :]
        start = text.index("fn @helper(")
        broken = text[:start] + re.sub(r"ret v\d+", "ret arg0", text[start:], count=1)
        before, after = self.directory / "before.mir", self.directory / "after.mir"
        before.write_text(text)
        after.write_text(broken)
        report = self.directory / "witness.json"
        result = self.run_command(
            [CLI, "mir", before, after, "--function", "helper", "--report", report]
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Rust interpreter: matched", result.stdout)
        data = json.loads(report.read_text())
        self.assertEqual(data["region"]["name"], "helper")
        self.assertNotEqual(
            data["inputs"][0]["module_sha256"], data["inputs"][0]["canonical_sha256"]
        )
        self.assertEqual(data["witness"]["lean"], data["witness"]["rust"])

    def test_window_witness_uses_full_solidity_context(self):
        compiled = self.compile(
            "windows.sol",
            [
                "--emit=bin",
                f"-Zmir-pipeline={CONVERSIONS}",
                "-Zevm-ir-pipeline=none",
                "-Zdump=evm-ir-runtime",
            ],
        )
        text = compiled.stdout[compiled.stdout.index("@module ") :]
        blocks = list(re.finditer(r"(?m)^bb(\d+)(?: \[.*\])?:.*$", text))
        for index, block in enumerate(blocks):
            end = blocks[index + 1].start() if index + 1 < len(blocks) else len(text)
            body = text[block.end() : end]
            if "  add\n" in body and "calldataload" not in body:
                label = "bb" + block[1]
                # The selector and both arguments remain live at this branch.
                broken = (
                    text[: block.end()]
                    + body.replace("  add\n", "  sub\n", 1)
                    + text[end:]
                )
                break
        else:
            self.fail("fixture has no scalar arithmetic window")
        before, after = self.directory / "before.evmir", self.directory / "after.evmir"
        before.write_text(text)
        after.write_text(broken)
        result = self.run_command(
            [CLI, "evmir", before, after, "--window", label, "--stack-height", "3"]
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(f"EVM IR window {label}", result.stdout)
        self.assertIn("Rust interpreter: matched", result.stdout)

    def test_shared_terminal_boundary_is_not_a_counterexample(self):
        before = self.compile("windows.sol", ["--emit=bin,bin-runtime"])
        after = self.compile("windows.sol", ["--emit=bin,bin-runtime"], audit=True)
        self.assertEqual(json.loads(before.stdout), json.loads(after.stdout))
        self.assertIn("window boundaries changed", after.stderr)
        self.assertNotIn(": counterexample", after.stderr)

    def test_join_with_multiple_heights_is_unsupported(self):
        source = self.directory / "join.evmir"
        source.write_text("""@module Join
bb0:
  calldatasize
  push bb1
  jumpi
  push 42
  jump bb2
bb1:
  jump bb2
bb2:
  push 1
  pop
  stop
""")
        result = self.run_command(
            [
                SOLAR,
                source,
                "-Ogas",
                "-Zevm-ir-pipeline=peephole",
                f"-Zsolar-tv={self.audits}",
            ]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("incoming EVM stack height is unknown", result.stderr)
        self.assertFalse(
            any(record["status"] == "verified" for record in self.records())
        )

    def test_non_dense_successors_keep_their_identity(self):
        source = """@module Labels
bb10 [stack=1]:
  jump bb20
bb20:
  iszero
  jumpi bb42, bb99
bb42:
  stop
bb99:
  stop
"""
        before, after = self.directory / "before.evmir", self.directory / "after.evmir"
        before.write_text(source)
        after.write_text(source.replace("  iszero\n", ""))
        result = self.run_command(
            [CLI, "evmir", before, after, "--window", "bb20", "--stack-height", "1"]
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("bb42, stack=[]", result.stdout)
        self.assertIn("bb99, stack=[]", result.stdout)
        self.assertIn("Rust interpreter: matched", result.stdout)

    def test_changed_function_interface_is_unsupported(self):
        result = self.compile("functions.sol", ["-Zmir-pipeline=none"])
        text = result.stdout[result.stdout.index("@module ") :]
        before, after = self.directory / "before.mir", self.directory / "after.mir"
        before.write_text(text)
        after.write_text(text.replace("-> i256 [pure]", "-> i256 [view]"))
        result = self.run_command([CLI, "mir", before, after, "--function", "helper"])
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("input interfaces differ", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_large_unrelated_body_does_not_obscure_a_witness(self):
        text = "@module Context\nfn @other(arg0: i256) {\nbb0:\n"
        text += "  sstore 0, arg0\n" * 10000 + "  ret\n}\n"
        text += (
            "fn @small(arg0: i256) -> i256 {\nbb0:\n  v0 = add arg0, 1\n  ret v0\n}\n"
        )
        before, after = self.directory / "before.mir", self.directory / "after.mir"
        before.write_text(text)
        after.write_text(text.replace("ret v0", "ret arg0"))
        self.assertGreater(before.stat().st_size, 128 * 1024)
        result = self.run_command([CLI, "mir", before, after, "--function", "small"])
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Rust interpreter: matched", result.stdout)

    def test_wrong_window_height_is_a_diagnostic(self):
        source = self.directory / "height.evmir"
        source.write_text("@module Height\nbb0 [stack=1]:\n  iszero\n  stop\n")
        result = self.run_command(
            [CLI, "evmir", source, source, "--window", "bb0", "--stack-height", "0"]
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("compiler rejected window projection", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_complete_conditional_block_executes_untaken_stop(self):
        text = (
            "@module Stop\nbb0 [stack=1]:\n  push bb1\n  jumpi\n  stop\nbb1:\n  stop\n"
        )
        before, after = self.directory / "before.evmir", self.directory / "after.evmir"
        before.write_text(text)
        after.write_text(text.replace("  push bb1", "  iszero\n  push bb1"))
        for policy in ([], ["--gas", "refine"]):
            with self.subTest(policy=policy):
                result = self.run_command(
                    [
                        CLI,
                        "evmir",
                        before,
                        after,
                        "--window",
                        "bb0",
                        "--stack-height",
                        "1",
                        *policy,
                    ]
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("Rust interpreter: matched", result.stdout)
                if not policy:
                    self.assertIn("success(empty)", result.stdout)
                    self.assertNotIn("fallthrough", result.stdout)

    def test_cross_block_stack_shuffle_is_not_a_counterexample(self):
        text = "@module Shuffle\nbb0 [stack=2]:\n  jump bb1\nbb1:\n  swap 1\n  jump bb2\nbb2:\n  sub\n  stop\n"
        before, after = self.directory / "before.evmir", self.directory / "after.evmir"
        before.write_text(text)
        after.write_text(
            text.replace("  swap 1\n", "").replace("  sub\n", "  swap 1\n  sub\n")
        )
        result = self.run_command(
            [CLI, "evmir", before, after, "--window", "bb1", "--stack-height", "2"]
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("stack correspondence is unknown", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_zero_successor_is_a_real_label(self):
        text = "@module Root\nbb0 [stack=1]:\n  jump bb10\nbb10:\n  dup 1\n  iszero\n  jumpi bb0, bb42\nbb42:\n  stop\n"
        before, after = self.directory / "before.evmir", self.directory / "after.evmir"
        before.write_text(text)
        after.write_text(text.replace("  iszero\n", ""))
        result = self.run_command(
            [CLI, "evmir", before, after, "--window", "bb10", "--stack-height", "1"]
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("bb0, stack=", result.stdout)
        self.assertNotIn("fallthrough", result.stdout)
        self.assertIn("Rust interpreter: matched", result.stdout)
