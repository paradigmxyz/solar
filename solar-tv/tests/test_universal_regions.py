"""Complete terminal regions without guessed incoming stack heights."""

import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, correspondence
from validator.common import Unsupported


class UniversalRegionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)
        self.report = self.folder / "report.json"

    def pair(self, before, after, *, status=0, flags=()):
        paths = [self.folder / f"{name}.evmir" for name in ("before", "after")]
        for path, text in zip(paths, (before, after), strict=True):
            path.write_text(text)
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--region",
                "bb0",
                "--timeout",
                "15",
                "--report",
                self.report,
                "--solar",
                SOLAR,
                *flags,
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result, json.loads(self.report.read_text())

    def test_real_tail_merge_for_all_stacks_and_memory(self):
        folder = ROOT / "examples/evmir/tail-region"
        before, after = [
            (folder / f"{side}.evmir").read_text() for side in ("before", "after")
        ]
        result, record = self.pair(before, after)
        self.assertEqual(result.stdout, "verified\n")
        self.assertEqual(record["region"]["proof"], "universal-stack-region")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertFalse(record["whole_pass"])
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(
            [entry["kind"] for entry in record["import_correspondence"]],
            ["kernel-checked-cfg-expansion"] * 2,
        )
        self.assertFalse(
            any("entry stack height" in item for item in record["assumptions"])
        )
        self.pair(before, (folder / "broken.evmir").read_text(), status=2)

    def test_pass_hook_proves_an_unknown_height_terminal_region(self):
        before = (ROOT / "examples/evmir/tail-region/before.evmir").read_text()
        before = re.sub(
            r"\bbb([012])\b", lambda match: f"bb{int(match[1]) + 1}", before
        )
        # Storing the entry label makes its incoming height unknown to the
        # verifier. The extracted terminal tails must use a universal proof.
        before = before.replace(
            "bb1:\n", "bb0:\n  push bb1\n  push 0\n  mstore\n  stop\nbb1:\n", 1
        )
        path = self.folder / "escaped.evmir"
        path.write_text(before)
        audit = self.folder / "audits"
        command = [SOLAR, path, "-Osize", "-Zevm-ir-pipeline=tail-merge"]
        results = [
            subprocess.run(
                [*command, *flags],
                env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                text=True,
                capture_output=True,
                check=False,
                timeout=180,
            )
            for flags in ([], [f"-Zsolar-tv={audit}"])
        ]
        for result in results:
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(results[0].stdout, results[1].stdout)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        proved = [
            record
            for record in records
            if record["status"] == "verified"
            and record.get("region", {}).get("proof") == "universal-stack-region"
        ]
        self.assertTrue(proved, results[1].stderr)
        self.assertTrue(
            all(record["region"]["stack_height"] is None for record in proved)
        )

    def test_concrete_transfer_growth_is_not_erased(self):
        before = "@module Boundary\nbb0:\n push 1\n pop\n stop\n"
        # At height zero these are equivalent. At height 1023 the added push
        # overflows, so the universal theorem must refuse the pair.
        after = before.replace(" pop\n", " push 2\n pop\n pop\n")
        _, record = self.pair(before, after, status=2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("proof did not close", record["diagnostic"])
        self.assertFalse(record["witness"])
        self.pair(before, after, flags=("--stack-height", "0"))

    def test_layout_transfer_can_introduce_stack_overflow(self):
        before = "@module Boundary\nbb0:\n jump bb1\nbb1:\n stop\nbb2:\n stop\n"
        after = before.replace("bb1:\n stop\nbb2:\n stop", "bb2:\n stop\nbb1:\n stop")
        # The old edge is a fallthrough; the new one needs a label push. A
        # 1024-word incoming stack cannot take the new edge.
        self.pair(before, after, status=2)

    def test_incoming_words_and_unmodeled_context_are_not_assumed(self):
        for body, diagnostic in (
            ("mload\n pop\n stop", "entry stack"),
            ("callvalue\n pop\n stop", "unmodeled EVM"),
            ("jump bb0", "cyclic"),
        ):
            with self.subTest(body=body):
                text = "@module Unsupported\nbb0 [stack=1]:\n " + body + "\n"
                _, record = self.pair(text, text, status=2)
                self.assertIn(diagnostic, record["diagnostic"])
        text = "@module Unpriced\nbb0:\n stop\n"
        _, record = self.pair(text, text, status=2, flags=("--gas", "exact"))
        self.assertIn("physical transfer pricing", record["diagnostic"])

    def test_cached_tree_must_match_independent_source_graph(self):
        path = self.folder / "source.evmir"
        path.write_text("@module Source\nbb0:\n invalid\n")
        replay = str(Path(SOLAR).with_name("solar-tv-replay"))
        program = check.load("evmir", path, SOLAR, 15, region="bb0")
        program.region_code = ".terminal (.control .stop)"
        correspondence.certify(
            program, replay, 15, entry_label="bb0", stack_height=None
        )
        with self.assertRaisesRegex(Unsupported, "proof did not close"):
            check.check(
                program, program, 15, replay_command="unused", replay_executable=replay
            )


if __name__ == "__main__":
    unittest.main()
