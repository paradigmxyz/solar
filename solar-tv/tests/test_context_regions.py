"""Universal read-only context, incoming stack demand and independent witnesses."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import correspondence, native
from validator.common import Unsupported


class ContextRegionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name)

    def check(self, before, after, *flags, status=0):
        report = self.folder / "report.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                before,
                after,
                "--solar",
                SOLAR,
                "--timeout",
                "20",
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=120,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_real_layout_with_caller_and_incoming_words(self):
        folder = ROOT / "examples/evmir/caller-tail"
        before, after = [folder / f"{side}.evmir" for side in ("before", "after")]
        _, record = self.check(before, after, "--region", "bb1")
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["region"]["proof"], "universal-stack-region")
        self.assertIsNone(record["region"]["stack_height"])
        self.assertIn(
            "arbitrary 160-bit caller and calldata bytes",
            " ".join(record["assumptions"]),
        )
        self.assertEqual(
            [row["kind"] for row in record["import_correspondence"]],
            ["kernel-checked-cfg-expansion"] * 2,
        )

    def test_real_calldata_cse_prefix(self):
        folder = ROOT / "examples/evmir/calldata-prefix"
        _, record = self.check(
            folder / "before.evmir", folder / "after.evmir", "--prefix", "bb0"
        )
        self.assertEqual(record["region"]["proof"], "universal-stack-prefix")
        self.assertEqual(record["gas_policy"], "refine")
        self.assertIn(
            "arbitrary 160-bit caller and calldata bytes",
            " ".join(record["assumptions"]),
        )

    def test_callvalue_layout_keeps_the_full_value(self):
        example = ROOT / "examples/evmir/caller-tail"
        before, after, broken = [
            self.folder / f"{side}.evmir" for side in ("before", "after", "broken")
        ]
        before.write_text(
            (example / "before.evmir")
            .read_text()
            .replace("  caller\n", "  callvalue\n")
        )
        after.write_text(
            subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zevm-ir-pipeline=block-layout"],
                text=True,
                timeout=30,
            )
        )
        self.assertNotEqual(before.read_text(), after.read_text().split("\n", 1)[1])
        _, record = self.check(before, after, "--region", "bb1", "--stack-height", "2")
        self.assertEqual(record["region"]["proof"], "universal-stack-region")
        self.assertIn("arbitrary 256-bit call value", record["assumptions"])

        broken.write_text(after.read_text().replace("  callvalue\n", "  push 0\n"))
        _, record = self.check(
            before, broken, "--region", "bb1", "--stack-height", "2", status=2
        )
        self.assertEqual(record["status"], "unsupported")

    def test_examples_match_actual_pass_outputs(self):
        for name, pipeline in (
            ("caller-tail", "block-layout"),
            ("calldata-prefix", "block-cse"),
        ):
            with self.subTest(name=name):
                folder = ROOT / "examples/evmir" / name
                result = subprocess.run(
                    [
                        SOLAR,
                        folder / "before.evmir",
                        "-Ogas",
                        f"-Zevm-ir-pipeline={pipeline}",
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                normalize = lambda text: [
                    line.strip()
                    for line in text.splitlines()
                    if line.strip() and not line.startswith(";")
                ]
                self.assertEqual(
                    normalize(result.stdout),
                    normalize((folder / "after.evmir").read_text()),
                )

    def test_context_changes_cannot_verify_from_zero_defaults(self):
        for name, selector in (
            ("caller-tail", ("--region", "bb1")),
            ("calldata-prefix", ("--prefix", "bb0")),
        ):
            with self.subTest(name=name):
                folder = ROOT / "examples/evmir" / name
                _, record = self.check(
                    folder / "before.evmir",
                    folder / "broken.evmir",
                    *selector,
                    status=2,
                )
                self.assertEqual(record["status"], "unsupported")

    def test_broken_context_has_independently_replayed_witnesses(self):
        for name in ("caller-tail", "calldata-prefix"):
            with self.subTest(name=name):
                folder = ROOT / "examples/evmir" / name
                _, record = self.check(
                    folder / "before.evmir",
                    folder / "broken.evmir",
                    "--world",
                    folder / "world.json",
                    status=1,
                )
                self.assertEqual(record["status"], "counterexample")
                self.assertEqual(
                    record["witness"]["observations"],
                    record["witness"]["native_observations"],
                )

    def test_fixed_scalar_inputs_do_not_assume_empty_context(self):
        folder = ROOT / "examples/evmir/calldata-prefix"
        _, record = self.check(
            folder / "before.evmir", folder / "after.evmir", status=2
        )
        self.assertIn("calldataload", record["diagnostic"])

    def test_short_stacks_remain_observable(self):
        before = self.folder / "before.evmir"
        after = self.folder / "after.evmir"
        text = "@module Short\nbb0:\n caller\n push 32\n mstore\n push 64\n mstore\n stop\n"
        before.write_text(text)
        after.write_text(text.replace(" push 64\n mstore\n", ""))
        _, record = self.check(before, after, "--region", "bb0", status=2)
        self.assertEqual(record["status"], "unsupported")

    def test_schema_bounds_do_not_recurse_on_instruction_chains(self):
        catalog = {
            row["name"]: row
            for row in native.catalog(
                str(Path(SOLAR).with_name("solar-tv-replay")), 15
            )["evmir"]
        }
        rows = [
            {"node": {"kind": "operation", "operation": "pop", "next": i + 1}}
            for i in range(2048)
        ]
        rows.append({"node": {"kind": "terminal", "operation": "stop"}})
        self.assertEqual(correspondence.stack_bounds(rows, 0, catalog), (2048, 0))
        rows[-1] = {"node": {"kind": "jump", "next": 0, "growth": 1}}
        with self.assertRaisesRegex(Unsupported, "cyclic stack bounds"):
            correspondence.stack_bounds(rows, 0, catalog)


if __name__ == "__main__":
    unittest.main()
