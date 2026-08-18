#!/usr/bin/env python3

from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError

from test_benchmark import CONTEXT, RawArtifact, benchmark, constant_sessions
from test_benchmark import CURRENT_MAIN_SHA, CURRENT_PR_HEAD_SHA


DIRECTORY = Path(__file__).resolve().parent
RAW_SCHEMA = json.loads((DIRECTORY / "raw.schema.json").read_text(encoding="utf-8"))
COMPARISON_SCHEMA = json.loads(
    (DIRECTORY / "comparison.schema.json").read_text(encoding="utf-8")
)


def conclusive_comparison(sessions: list[benchmark.BenchmarkSession]) -> dict:
    return benchmark.add_publication_state(
        benchmark.build_comparison(sessions, CONTEXT),
        CONTEXT,
        CURRENT_MAIN_SHA,
        CURRENT_PR_HEAD_SHA,
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
        comparisons = (
            conclusive_comparison(constant_sessions()),
            benchmark.inconclusive_comparison(CONTEXT, "invalid raw artifact"),
        )

        for comparison in comparisons:
            with self.subTest(overall=comparison["overall"]):
                self.comparison_validator.validate(comparison)

    def test_schemas_reject_unversioned_or_extended_outputs(self) -> None:
        comparison = conclusive_comparison(constant_sessions())
        invalid = copy.deepcopy(comparison)
        invalid["unexpected"] = True

        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid)

    def test_schemas_require_dfm_and_publication_provenance(self) -> None:
        comparison = conclusive_comparison(constant_sessions())
        missing = copy.deepcopy(comparison)
        del missing["merge_candidate_sha"]
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(missing)

        invalid_mode = copy.deepcopy(comparison)
        invalid_mode["comparison_mode"] = "main-head"
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid_mode)

        missing_freshness = copy.deepcopy(comparison)
        del missing_freshness["freshness"]
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(missing_freshness)

        invalid_freshness = copy.deepcopy(comparison)
        invalid_freshness["freshness"] = "stale"
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid_freshness)

        invalid_current_sha = copy.deepcopy(comparison)
        invalid_current_sha["current_main_sha"] = "A" * 40
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid_current_sha)

        with tempfile.TemporaryDirectory() as directory:
            artifact = RawArtifact(Path(directory))
            del artifact.manifest["context"]["main_sha"]
            with self.assertRaises(ValidationError):
                self.raw_validator.validate(artifact.manifest)

            artifact = RawArtifact(Path(directory) / "freshness")
            artifact.manifest["context"]["freshness"] = "current"
            with self.assertRaises(ValidationError):
                self.raw_validator.validate(artifact.manifest)

    def test_comparison_schema_rejects_metrics_below_output_precision(self) -> None:
        comparison = conclusive_comparison(constant_sessions())
        comparison["methods"][0]["base"]["p50_ms"] = 0.00001

        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(comparison)

    def test_comparison_schema_rejects_upstream_diagnostics_selector(self) -> None:
        comparison = conclusive_comparison(constant_sessions())
        comparison["methods"][1]["name"] = "textDocument/diagnostic"

        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(comparison)

    def test_schemas_require_session_and_confidence_contracts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            raw = RawArtifact(Path(directory)).manifest
        missing_session = copy.deepcopy(raw)
        missing_session["passes"][0].pop("session")
        with self.assertRaises(ValidationError):
            self.raw_validator.validate(missing_session)

        duplicate_session = copy.deepcopy(raw)
        duplicate_session["passes"][1] = copy.deepcopy(duplicate_session["passes"][0])
        with self.assertRaises(ValidationError):
            self.raw_validator.validate(duplicate_session)

        mismatched_path = copy.deepcopy(raw)
        mismatched_path["passes"][0]["session"] = 2
        with self.assertRaises(ValidationError):
            self.raw_validator.validate(mismatched_path)

        comparison = conclusive_comparison(constant_sessions())
        comparison["methods"][0].pop("session_count")
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(comparison)

        comparison = conclusive_comparison(constant_sessions())
        comparison["methods"][0]["strata"][0].pop("confidence_interval_95")
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(comparison)

        inconclusive = benchmark.inconclusive_comparison(CONTEXT, "failure")
        inconclusive.pop("threshold_absolute_ms")
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(inconclusive)

    def test_schemas_reject_trailing_control_characters(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            raw = RawArtifact(Path(directory)).manifest
        invalid_raw = copy.deepcopy(raw)
        invalid_raw["context"]["repository"] += "\n"
        with self.assertRaises(ValidationError):
            self.raw_validator.validate(invalid_raw)

        invalid_comparison = conclusive_comparison(constant_sessions())
        invalid_comparison["run_url"] += "\t"
        with self.assertRaises(ValidationError):
            self.comparison_validator.validate(invalid_comparison)


if __name__ == "__main__":
    unittest.main()
