"""Unchecked capture, output identity and reproducible non-cherry-picked samples."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate


class EvaluationTests(unittest.TestCase):
    def test_sampling_is_stable_and_includes_both_layers(self):
        rows = [
            {"layer": layer, "pass": pass_name, "id": f"{layer}-{pass_name}-{i}"}
            for layer in ("mir", "evmir")
            for pass_name in ("a", "b", "c")
            for i in range(3)
        ]
        chosen = evaluate.select(rows, 6, "0")
        self.assertEqual(chosen, evaluate.select(list(reversed(rows)), 6, "0"))
        self.assertEqual({row["layer"] for row in chosen[:2]}, {"mir", "evmir"})
        self.assertEqual(len({(r["layer"], r["pass"]) for r in chosen}), 6)

    def test_capture_is_unchecked_and_bytecode_neutral(self):
        source = {
            "language": "Solidity",
            "sources": {
                "C.sol": {
                    "content": "contract C { function f(uint256 x) external pure returns (uint256) { return x + 0; } }"
                }
            },
            "settings": {
                "optimizer": {"enabled": True},
                "outputSelection": {"*": {"*": ["evm.bytecode.object"]}},
            },
        }
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            baseline, _ = evaluate.compile_input(SOLAR, source, path)
            captured, _ = evaluate.compile_input(SOLAR, source, path, capture=True)
            self.assertEqual(baseline, captured)
            rows = evaluate.captures(path)
            self.assertTrue(rows)
            self.assertTrue(all(r["status"] == "captured" for r in rows))
            self.assertFalse((path / "audits").exists())
            # Many function regions share a single immutable module pair.
            self.assertLess(len(list((path / "captures").iterdir())), len(rows))

    def test_pinned_archives_resolve_real_import_closures(self):
        manifest = json.loads((ROOT / "evaluation.json").read_text())
        for case in manifest["cases"]:
            with self.subTest(case=case["name"]):
                value = evaluate.inputs(case)
                self.assertIn(case["source"], value["sources"])
                self.assertEqual(value["settings"]["evmVersion"], "cancun")
                with self.assertRaisesRegex(ValueError, "fingerprint changed"):
                    evaluate.inputs({**case, "sha256": "0" * 64})
