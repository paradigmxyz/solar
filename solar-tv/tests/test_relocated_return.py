"""Check a real ERC20Mock layout change and a misdirected mint return."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate


class RelocatedReturnTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.folder = Path(temporary.name)
        cases = json.loads((ROOT / "evaluation.json").read_text())["cases"]
        case = next(row for row in cases if row["name"] == "openzeppelin-erc20-mock")
        inputs = evaluate.inputs(case)
        cls.baseline, _ = evaluate.compile_input(SOLAR, inputs, cls.folder)
        cls.captured, _ = evaluate.compile_input(
            SOLAR, inputs, cls.folder, capture=True
        )
        pairs = [
            row["paths"]
            for row in evaluate.captures(cls.folder)
            if row["module"] == "ERC20Mock_runtime"
            and row["pass"] == "block-layout"
            and (row["region"] or {}).get("name") == "bb67"
        ]
        (cls.paths,) = pairs
        cls.world = cls.folder / "world.json"
        cls.world.write_text(
            json.dumps(
                {
                    "version": 1,
                    "calldata": "0x40c10f19" + f"{0x2222:064x}{7:064x}",
                    "accounts": [
                        {"address": 65536, "storage": {"2": "9", "4": "99"}},
                        {"address": 99, "balance": 123, "storage": {"1": "9"}},
                    ],
                }
            )
        )

    def check_pair(self, stem, paths, status):
        report = self.folder / f"{stem}.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--solar",
                SOLAR,
                "--return-path=0x40c10f19",
                "--world",
                self.world,
                "--report",
                report,
                "--timeout=90",
                "--verbose",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=300,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_mint_returns_to_the_relocated_continuation(self):
        self.assertEqual(self.baseline, self.captured)
        result, record = self.check_pair("verified", self.paths, 0)
        self.assertIn("continuation PC", result.stdout)
        self.assertEqual(record["region"]["proof"], "kernel-checked-bytecode-return")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertFalse(record["whole_pass"])
        old, new = record["import_correspondence"]
        self.assertNotEqual(old["continuation_pc"], new["continuation_pc"])
        for side in (old, new):
            self.assertNotEqual(side["caller_push_pc"], side["indirect_jump_pc"])
        self.assertEqual(len(set(record["world"]["backend_images"])), 2)

    def test_mint_returning_to_the_bool_stub_is_a_counterexample(self):
        rewrite = self.paths[1].read_text()
        changed, count = re.subn(
            r"(?m)^(  push )bb18(?=\n  jump bb63$)", r"\g<1>bb55", rewrite
        )
        self.assertEqual(count, 1)
        broken = self.folder / "wrong-target.evmir"
        broken.write_text(changed)
        result, record = self.check_pair("counterexample", [self.paths[0], broken], 1)
        self.assertIn("--return-path 0x40c10f19", result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        old, new = record["witness"]["observations"]
        self.assertEqual(old[:2], ["success", []])
        self.assertEqual(new[:2], ["success", [0] * 31 + [1]])
        self.assertEqual(old[4:], new[4:])
        (_, _, _, _, slots) = old[4][1]
        self.assertEqual(dict(slots)["2"], "16")
        self.assertEqual(old[5][0][2], [0] * 31 + [7])
        before, after = record["witness"]["cuts"]
        self.assertEqual(before["stack"], [str(0x40C10F19)])
        self.assertEqual(before["stack"], after["stack"])
        self.assertEqual(before["memory"], after["memory"])
        self.assertNotEqual(before["pc"], after["pc"])
