#!/usr/bin/env python3

from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError

from test_benchmark import CONTEXT, RawArtifact, benchmark


DIRECTORY = Path(__file__).resolve().parent
RAW_SCHEMA = json.loads((DIRECTORY / "raw.schema.json").read_text(encoding="utf-8"))
COMPARISON_SCHEMA = json.loads(
    (DIRECTORY / "comparison.schema.json").read_text(encoding="utf-8")
)


class SchemaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        Draft202012Validator.check_schema(RAW_SCHEMA)
        Draft202012Validator.check_schema(COMPARISON_SCHEMA)
        cls.raw_validator = Draft202012Validator(RAW_SCHEMA)
        cls.comparison_validator = Draft202012Validator(COMPARISON_SCHEMA)

    def test_raw_manifest_matches_the_versioned_schema(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))

            self.raw_validator.validate(artifact.manifest)

    def test_conclusive_and_inconclusive_comparisons_match_the_schema(self) -> None:
        samples = {
            role: {method: [10.0] * 20 for method in benchmark.METHODS}
            for role in ("base", "head")
        }
        comparisons = (
            benchmark.build_comparison(samples, CONTEXT),
            benchmark.inconclusive_comparison(CONTEXT, "invalid raw artifact"),
        )

        for comparison in comparisons:
            with self.subTest(overall=comparison["overall"]):
                self.comparison_validator.validate(comparison)

    def test_schemas_reject_unversioned_or_extended_outputs(self) -> None:
        samples = {
            role: {method: [10.0] * 20 for method in benchmark.METHODS}
            for role in ("base", "head")
        }
        comparison = benchmark.build_comparison(samples, CONTEXT)
        invalid = copy.deepcopy(comparison)
        invalid["unexpected"] = True

        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid)

    def test_comparison_schema_rejects_metrics_below_output_precision(self) -> None:
        samples = {
            role: {method: [10.0] * 20 for method in benchmark.METHODS}
            for role in ("base", "head")
        }
        comparison = benchmark.build_comparison(samples, CONTEXT)
        comparison["methods"][0]["base"]["p50_ms"] = 0.00001

        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(comparison)

    def test_schemas_reject_trailing_control_characters(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            raw = RawArtifact(Path(directory)).manifest
        invalid_raw = copy.deepcopy(raw)
        invalid_raw["context"]["repository"] += "\n"
        with self.assertRaises(ValidationError):
            self.raw_validator.validate(invalid_raw)

        samples = {
            role: {method: [10.0] * 20 for method in benchmark.METHODS}
            for role in ("base", "head")
        }
        invalid_comparison = benchmark.build_comparison(samples, CONTEXT)
        invalid_comparison["run_url"] += "\t"
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid_comparison)


if __name__ == "__main__":
    unittest.main()
