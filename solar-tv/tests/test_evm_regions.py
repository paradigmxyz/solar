"""Acyclic multi-block proofs, real CFG edits and original-module replay."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR


class EvmRegionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)

    def command(self, args):
        return subprocess.run(
            args,
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            timeout=180,
            check=False,
        )

    def pair(self, before, after, *, height=0, entry="bb0", status=0, extra=()):
        paths = [self.folder / f"{name}.evmir" for name in ("before", "after")]
        for path, text in zip(paths, (before, after), strict=True):
            path.write_text(text)
        report = self.folder / "result.json"
        result = self.command(
            [
                CLI,
                "evmir",
                *paths,
                "--region",
                entry,
                "--stack-height",
                str(height),
                "--timeout",
                "45",
                "--report",
                report,
                *extra,
            ]
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        record = json.loads(report.read_text())
        self.assertEqual(record["region"]["kind"], "region")
        self.assertFalse(record["whole_pass"])
        if status == 1:
            self.assertEqual(record["witness"]["lean"], record["witness"]["rust"])
            self.assertIn("Rust interpreter: matched", result.stdout)
            self.assertIn("--region", result.stdout)
        return result, record

    def test_real_cfg_and_shared_tail_examples(self):
        for name, pipeline in (
            ("cfg-region", "cfg-simplify"),
            ("tail-region", "tail-merge"),
        ):
            with self.subTest(name=name):
                folder = ROOT / "examples/evmir" / name
                before = (folder / "before.evmir").read_text()
                after = self.command(
                    [
                        SOLAR,
                        folder / "before.evmir",
                        "-Osize",
                        f"-Zevm-ir-pipeline={pipeline}",
                        "-Zvalidate-ir=true",
                    ]
                )
                self.assertEqual(after.returncode, 0, after.stderr)
                expected = (folder / "after.evmir").read_text()
                self.assertEqual(
                    after.stdout[after.stdout.index("@module") :],
                    expected[expected.index("@module") :],
                )
                self.assertNotEqual(before.splitlines()[1:], expected.splitlines()[1:])
                self.pair(before, expected)
                self.pair(before, (folder / "broken.evmir").read_text(), status=1)

    def test_stack_shuffle_can_cross_changed_blocks(self):
        before = """@module Shuffle
bb0 [stack=2]:
  jump bb1
bb1:
  swap 1
  jump bb2
bb2:
  sub
  push 0
  mstore
  push 32
  push 0
  return
"""
        after = before.replace("  swap 1\n", "").replace("  sub\n", "  swap 1\n  sub\n")
        self.pair(before, after, height=2, entry="bb1")
        self.pair(
            before, after.replace("  swap 1\n", ""), height=2, entry="bb1", status=1
        )

    def test_internal_join_heights_need_not_agree(self):
        before = """@module UnequalHeights
bb0:
  calldatasize
  jumpi bb1, bb2
bb1:
  push 7
  push 17
  jump bb3
bb2:
  push 17
  jump bb3
bb3:
  push 0
  mstore
  push 32
  push 0
  return
"""
        self.pair(before, before.replace("  push 7\n", ""))

    def test_physical_conditional_runs_both_continuations(self):
        before = """@module Physical
bb0:
  calldatasize
  push bb1
  jumpi
  push 17
  push 0
  mstore
  push 32
  push 0
  return
bb1:
  push 0
  push 0
  revert
"""
        self.pair(before, before)
        fallthrough = (
            before.replace("push bb1", "push bb2")
            .replace("  jumpi\n", "  jumpi\n  jump bb1\nbb1:\n")
            .replace("bb1:\n  push 0\n", "bb2:\n  push 0\n")
        )
        self.pair(before, fallthrough)
        self.pair(before, before.replace("  revert\n", "  return\n"), status=1)

    def test_unrelated_world_code_is_not_imported(self):
        text = "@module Unrelated\nbb0:\n  jump bb1\nbb1:\n  stop\nbb99:\n  push 0\n  push 0\n  sstore\n  stop\n"
        self.pair(text, text.replace("bb1:\n  stop", "bb1:\n  push 1\n  pop\n  stop"))

    def test_cycles_dynamic_targets_and_gas_are_explicitly_unsupported(self):
        for body in (
            "bb0:\n  jump bb0\n",
            "bb0 [stack=1]:\n  jump\n",
            "bb0:\n  push bb1\n  push 0\n  mstore\n  stop\nbb1:\n  stop\n",
        ):
            with self.subTest(body=body):
                self.pair(
                    "@module Unsupported\n" + body,
                    "@module Unsupported\n" + body,
                    height=1 if "[stack=1]" in body else 0,
                    status=2,
                )
        text = "@module Gas\nbb0:\n  stop\n"
        result, _ = self.pair(text, text, status=2, extra=("--gas", "exact"))
        self.assertIn("physical transfer pricing", result.stderr)

    def test_pass_hook_uses_regions_for_structural_edits(self):
        for name, pipeline in (
            ("cfg-region", "cfg-simplify"),
            ("tail-region", "tail-merge"),
        ):
            with self.subTest(name=name):
                path = ROOT / "examples/evmir" / name / "before.evmir"
                output = self.folder / name
                flags = ["-Osize", f"-Zevm-ir-pipeline={pipeline}"]
                plain = self.command([SOLAR, path, *flags])
                audited = self.command([SOLAR, path, *flags, f"-Zsolar-tv={output}"])
                self.assertEqual(audited.returncode, 0, audited.stderr)
                self.assertEqual(plain.stdout, audited.stdout)
                records = [
                    json.loads(p.read_text())
                    for p in (output / "audits").glob("*.json")
                ]
                self.assertTrue(
                    any(
                        r["status"] == "verified" and r["region"]["kind"] == "region"
                        for r in records
                    ),
                    audited.stderr,
                )
                self.assertTrue(all(not r["whole_pass"] for r in records))
                for record in records:
                    if (
                        record["status"] == "verified"
                        and record["region"]["kind"] == "region"
                    ):
                        self.assertTrue(record["region"]["blocks_before"])
                        self.assertTrue(record["region"]["blocks_after"])

    def test_shared_path_expansion_is_bounded(self):
        body = "@module Expansion\n"
        for index in range(9):
            root = 3 * index
            body += (
                f"bb{root}:\n  calldatasize\n  jumpi bb{root + 1}, bb{root + 2}\n"
                f"bb{root + 1}:\n  jump bb{root + 3}\n"
                f"bb{root + 2}:\n  jump bb{root + 3}\n"
            )
        body += "bb27:\n  stop\n"
        result, _ = self.pair(body, body, status=2)
        self.assertIn("expansion budget exceeded", result.stderr)

    @unittest.skipIf(os.name == "nt", "executable checker wrapper uses a shebang")
    def test_region_counterexample_stops_the_pass_hook(self):
        wrapper = self.folder / "broken-checker"
        wrapper.write_text(
            f"#!{sys.executable}\n"
            "import os, sys\nfrom pathlib import Path\n"
            "path = Path(sys.argv[3])\n"
            "path.write_text(path.read_text().replace('push 11', 'push 12', 1))\n"
            f"os.execv({CLI!r}, [{CLI!r}, *sys.argv[1:]])\n"
        )
        wrapper.chmod(0o755)
        output = self.folder / "rejected"
        result = subprocess.run(
            [
                SOLAR,
                ROOT / "examples/evmir/tail-region/before.evmir",
                "-Osize",
                "-Zevm-ir-pipeline=tail-merge,cfg-simplify",
                f"-Zsolar-tv={output}",
            ],
            env={**os.environ, "SOLAR_TV": str(wrapper), "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn(": counterexample", result.stderr)
        records = [
            json.loads(p.read_text()) for p in (output / "audits").glob("*.json")
        ]
        self.assertEqual(len(records), 1)
        record = records[0]
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(record["region"]["kind"], "region")
        self.assertEqual(record["witness"]["lean"], record["witness"]["rust"])
        self.assertTrue((Path(record["pair"]) / "before.evmir").is_file())
        self.assertTrue((Path(record["pair"]) / "after.evmir").is_file())
