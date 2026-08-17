#!/usr/bin/env python3

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("lsp-bench-pr-comment.py")
SOURCE_URL = "https://github.com/paradigmxyz/solar.git"
FORK_SOURCE_URL = "https://github.com/example/solar.git"
BASELINE_REVISION = "1" * 40
CANDIDATE_REVISION = "2" * 40
BASELINE_EXECUTABLE = "3" * 64
CANDIDATE_EXECUTABLE = "4" * 64
INCONCLUSIVE_REPORT = (
    "# Solar LSP PR benchmark\n\n"
    "**INCONCLUSIVE**\n\n"
    "The benchmark comparison artifact was missing or invalid. "
    "Inspect the workflow run logs and retained artifacts.\n"
)


def percentage_delta(baseline: float | None, candidate: float | None) -> float | None:
    if baseline is None or candidate is None or baseline == 0.0:
        return None
    return (candidate - baseline) / abs(baseline) * 100.0


def summary(revision: str, executable: str, source_url: str = SOURCE_URL) -> dict:
    candidate = executable == CANDIDATE_EXECUTABLE
    return {
        "schema_version": 5,
        "config_schema_version": 1,
        "config_sha256": "a" * 64,
        "servers_lock_sha256": ("e" if candidate else "f") * 64,
        "fixtures_lock_sha256": "b" * 64,
        "profile": "pr",
        "harness_version": "0.2.0",
        "harness_contract_sha256": "c" * 64,
        "rustc_version": "rustc 1.96.0 (example)",
        "repeat_override": None,
        "timeout_ms": 30_000,
        "environment": {
            "os": "linux",
            "architecture": "x86_64",
            "logical_cpus": 8,
            "accounting_backends": ["rusage-direct-child"],
            "memory_accounting_backends": ["rusage-max-rss-direct-child"],
            "network_isolated": False,
        },
        "servers": [
            {
                "id": "solar",
                "args": ["lsp"],
                "transport": {"kind": "stdio"},
                "version_args": ["--version"],
                "locked_version": None,
                "expected_version": None,
                "enabled": True,
                "env": {},
                "initialization_options": None,
                "configuration": None,
                "source": {"url": source_url, "revision": revision},
                "executable_sha256": executable,
                "artifact_expected_sha256": executable,
                "artifact_sha256": executable,
                "required": True,
                "status": "available",
            }
        ],
        "fixtures": [
            {
                "id": "synthetic",
                "source_file_count": 2,
                "source_line_count": 20,
                "source_byte_count": 200,
                "content_sha256": "d" * 64,
                "solc": None,
                "solc_native_sha256": None,
                "solc_soljson_sha256": None,
                "foundry": None,
                "foundry_native_sha256": None,
                "dependencies": {},
            }
        ],
        "workloads": [
            {
                "id": "synthetic-warm-hover",
                "fixture": "synthetic",
                "methods": ["textDocument/hover"],
                "step_count": 3,
                "repetitions": 3,
            }
        ],
        "summaries": [
            {
                "server": "solar",
                "fixture": "synthetic",
                "workload": "synthetic-warm-hover",
                "successful_runs": 3,
                "status_counts": {"pass": 3},
                "status": "pass",
                "metrics": {
                    "textDocument/hover": {
                        "count": 20,
                        "mean": 1.2 if candidate else 1.0,
                        "p50": 1.2 if candidate else 1.0,
                        "p95": 1.4 if candidate else 1.1,
                        "p99": 1.4,
                        "max": 1.4,
                    }
                },
            }
        ],
    }


def metric_row() -> dict:
    baseline_mean = 1.0
    candidate_mean = 1.2
    baseline_p50 = 1.0
    candidate_p50 = 1.2
    baseline_p95 = 1.1
    candidate_p95 = 1.4
    return {
        "server": "solar",
        "fixture": "synthetic",
        "workload": "synthetic-warm-hover",
        "metric": "textDocument/hover",
        "baseline_status": "pass",
        "candidate_status": "pass",
        "expected_runs": 3,
        "baseline_successful_runs": 3,
        "candidate_successful_runs": 3,
        "baseline_count": 20,
        "candidate_count": 20,
        "baseline_mean": baseline_mean,
        "candidate_mean": candidate_mean,
        "mean_delta_pct": percentage_delta(baseline_mean, candidate_mean),
        "baseline_p50": baseline_p50,
        "candidate_p50": candidate_p50,
        "p50_delta_pct": percentage_delta(baseline_p50, candidate_p50),
        "baseline_p95": baseline_p95,
        "candidate_p95": candidate_p95,
        "p95_delta_pct": percentage_delta(baseline_p95, candidate_p95),
        "verdict": "regression",
        "reason": None,
    }


def comparison(baseline_digest: str, candidate_digest: str) -> dict:
    return {
        "schema_version": 2,
        "baseline": {
            "path": "target/lsp-bench/pr/baseline/summary.json",
            "summary_sha256": baseline_digest,
            "source_url": SOURCE_URL,
            "revision": BASELINE_REVISION,
            "executable_sha256": BASELINE_EXECUTABLE,
        },
        "candidate": {
            "path": "target/lsp-bench/pr/current/summary.json",
            "summary_sha256": candidate_digest,
            "source_url": SOURCE_URL,
            "revision": CANDIDATE_REVISION,
            "executable_sha256": CANDIDATE_EXECUTABLE,
        },
        "threshold_pct": 10.0,
        "min_samples": 2,
        "compatible": True,
        "blockers": [],
        "comparable_metrics": 1,
        "regressions": 1,
        "improvements": 0,
        "stable": 0,
        "inconclusive": 0,
        "rows": [metric_row()],
    }


def refresh_deltas(row: dict) -> None:
    for name in ("mean", "p50", "p95"):
        row[f"{name}_delta_pct"] = percentage_delta(
            row[f"baseline_{name}"], row[f"candidate_{name}"]
        )


def make_inconclusive(value: dict, reason: str) -> None:
    value["comparable_metrics"] = 0
    value["regressions"] = 0
    value["improvements"] = 0
    value["stable"] = 0
    value["inconclusive"] = 1
    value["rows"][0]["verdict"] = "inconclusive"
    value["rows"][0]["reason"] = reason


class LspBenchPrCommentTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.input = self.directory / "comparison.json"
        self.baseline_summary = self.directory / "baseline-summary.json"
        self.candidate_summary = self.directory / "candidate-summary.json"
        self.output = self.directory / "report.md"
        self.write_summary_files()
        self.value = comparison(
            self.file_sha256(self.baseline_summary), self.file_sha256(self.candidate_summary)
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def file_sha256(path: Path) -> str:
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def write_summary_files(
        self,
        *,
        baseline_revision: str = BASELINE_REVISION,
        candidate_revision: str = CANDIDATE_REVISION,
        baseline_url: str = SOURCE_URL,
        candidate_url: str = SOURCE_URL,
    ) -> None:
        self.baseline_summary.write_text(
            json.dumps(summary(baseline_revision, BASELINE_EXECUTABLE, baseline_url)),
            encoding="utf-8",
        )
        self.candidate_summary.write_text(
            json.dumps(summary(candidate_revision, CANDIDATE_EXECUTABLE, candidate_url)),
            encoding="utf-8",
        )

    def run_script(
        self,
        *,
        with_input: bool = True,
        expected_threshold: str = "10",
        expected_min_samples: str = "2",
        expected_baseline_revision: str = BASELINE_REVISION,
        expected_candidate_revision: str = CANDIDATE_REVISION,
        expected_baseline_source_url: str = SOURCE_URL,
        expected_candidate_source_url: str = SOURCE_URL,
    ) -> subprocess.CompletedProcess[str]:
        command = [
            sys.executable,
            str(SCRIPT),
            "--expected-baseline-revision",
            expected_baseline_revision,
            "--expected-candidate-revision",
            expected_candidate_revision,
            "--expected-baseline-source-url",
            expected_baseline_source_url,
            "--expected-candidate-source-url",
            expected_candidate_source_url,
            "--expected-threshold-pct",
            expected_threshold,
            "--expected-min-samples",
            expected_min_samples,
            "--output",
            str(self.output),
        ]
        if with_input:
            command.extend(
                (
                    "--input",
                    str(self.input),
                    "--baseline-summary",
                    str(self.baseline_summary),
                    "--candidate-summary",
                    str(self.candidate_summary),
                )
            )
        return subprocess.run(command, capture_output=True, text=True)

    def write_comparison(self, value: dict | None = None) -> None:
        self.input.write_text(json.dumps(self.value if value is None else value), encoding="utf-8")

    def assert_invalid(self, value: dict | None = None, **run_options: str) -> None:
        if value is not None:
            self.write_comparison(value)
        result = self.run_script(**run_options)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(encoding="utf-8"), INCONCLUSIVE_REPORT)

    def test_renders_valid_v2_comparison_in_fixed_format(self) -> None:
        self.write_comparison()

        result = self.run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        report = self.output.read_text(encoding="utf-8")
        self.assertIn("**REGRESSION**", report)
        self.assertIn("solar&#47;synthetic&#47;synthetic-warm-hover", report)
        self.assertIn(
            "| textDocument&#47;hover | 20 | 1.00 | 1.20 | +20.00% |", report
        )
        self.assertIn(f"| Baseline revision | {BASELINE_REVISION} |", report)
        self.assertIn(
            f"| Candidate summary | {self.value['candidate']['summary_sha256']} |", report
        )
        self.assertIn(f"| Candidate executable | {CANDIDATE_EXECUTABLE} |", report)
        self.assertNotIn("target/lsp-bench/pr/current/summary.json", report)

    def test_missing_artifact_renders_fixed_inconclusive_report(self) -> None:
        result = self.run_script(with_input=False)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(encoding="utf-8"), INCONCLUSIVE_REPORT)

    def test_partial_artifact_paths_render_fixed_inconclusive_report(self) -> None:
        command = [
            sys.executable,
            str(SCRIPT),
            "--input",
            str(self.input),
            "--expected-baseline-revision",
            BASELINE_REVISION,
            "--expected-candidate-revision",
            CANDIDATE_REVISION,
            "--expected-baseline-source-url",
            SOURCE_URL,
            "--expected-candidate-source-url",
            SOURCE_URL,
            "--expected-threshold-pct",
            "10",
            "--expected-min-samples",
            "2",
            "--output",
            str(self.output),
        ]

        result = subprocess.run(command, capture_output=True, text=True)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(encoding="utf-8"), INCONCLUSIVE_REPORT)

    def test_rejects_empty_rows_for_valid_incompatible_summaries(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate["config_sha256"] = "e" * 64
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        value["compatible"] = False
        value["blockers"] = ["candidate summary or comparison command was unavailable"]
        value["comparable_metrics"] = 0
        value["regressions"] = 0
        value["rows"] = []
        self.write_comparison(value)

        result = self.run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(encoding="utf-8"), INCONCLUSIVE_REPORT)

    def test_renders_valid_fork_candidate(self) -> None:
        self.write_summary_files(candidate_url=FORK_SOURCE_URL)
        self.value["candidate"]["source_url"] = FORK_SOURCE_URL
        self.value["candidate"]["summary_sha256"] = self.file_sha256(
            self.candidate_summary
        )
        self.write_comparison()

        result = self.run_script(expected_candidate_source_url=FORK_SOURCE_URL)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("**REGRESSION**", self.output.read_text(encoding="utf-8"))

    def test_rejects_tampered_deltas(self) -> None:
        for field in ("mean_delta_pct", "p50_delta_pct", "p95_delta_pct"):
            with self.subTest(field=field):
                value = copy.deepcopy(self.value)
                value["rows"][0][field] += 1.0
                self.assert_invalid(value)

    def test_rejects_self_consistent_statistics_not_present_in_the_summary(self) -> None:
        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        row["candidate_mean"] = 2.0
        row["candidate_p50"] = 2.0
        row["candidate_p95"] = 2.2
        refresh_deltas(row)

        self.assert_invalid(value)

    def test_rejects_statistics_nudged_across_the_verdict_threshold(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate_stats = candidate["summaries"][0]["metrics"]["textDocument/hover"]
        candidate_stats.update(
            {
                "mean": 1.1 - 5e-11,
                "p50": 1.1 - 5e-11,
                "p95": 1.21 - 5e-11,
            }
        )
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")

        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        row = value["rows"][0]
        row["candidate_mean"] = 1.1 + 5e-11
        row["candidate_p50"] = 1.1 + 5e-11
        row["candidate_p95"] = 1.21 + 5e-11
        refresh_deltas(row)

        self.assert_invalid(value)

    def test_rejects_forged_compatibility_for_different_summary_contracts(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate["harness_contract_sha256"] = "e" * 64
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)

        self.assert_invalid(value)

    def test_rejects_inconsistent_summary_status_counts(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate["summaries"][0]["status_counts"] = {"crash": 3}
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(
            self.candidate_summary
        )

        self.assert_invalid(value)

    def test_rejects_zero_count_summary_metrics(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate["summaries"][0]["metrics"]["textDocument/hover"]["count"] = 0
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(
            self.candidate_summary
        )

        self.assert_invalid(value)

    def test_rejects_a_declared_workload_missing_from_both_summaries(self) -> None:
        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        baseline["summaries"] = []
        candidate["summaries"] = []
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)
        value["candidate"]["summary_sha256"] = self.file_sha256(
            self.candidate_summary
        )
        value["rows"] = []
        value["comparable_metrics"] = 0
        value["regressions"] = 0

        self.assert_invalid(value)

    def test_rejects_unbound_solar_executable_provenance(self) -> None:
        for field, value in (
            ("artifact_sha256", "9" * 64),
            ("artifact_expected_sha256", "9" * 64),
            ("status", "unavailable"),
        ):
            with self.subTest(field=field):
                candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
                candidate["servers"][0][field] = value
                self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
                report = copy.deepcopy(self.value)
                report["candidate"]["summary_sha256"] = self.file_sha256(
                    self.candidate_summary
                )
                self.assert_invalid(report)

    def test_rejects_json_type_collisions_in_server_contracts(self) -> None:
        cases = (
            ("initialization_options", True, 1),
            ("configuration", 1, 1.0),
        )
        for field, baseline_value, candidate_value in cases:
            with self.subTest(field=field):
                baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
                candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
                baseline["servers"][0][field] = {"value": baseline_value}
                candidate["servers"][0][field] = {"value": candidate_value}
                self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
                self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")

                value = copy.deepcopy(self.value)
                value["baseline"]["summary_sha256"] = self.file_sha256(
                    self.baseline_summary
                )
                value["candidate"]["summary_sha256"] = self.file_sha256(
                    self.candidate_summary
                )
                self.assert_invalid(value)

        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        baseline["servers"][0]["configuration"] = {"value": 0}
        candidate["servers"][0]["configuration"] = {"value": 0}
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        candidate_text = json.dumps(candidate).replace('"value": 0', '"value": -0', 1)
        self.candidate_summary.write_text(candidate_text, encoding="utf-8")

        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)
        value["candidate"]["summary_sha256"] = self.file_sha256(
            self.candidate_summary
        )
        self.assert_invalid(value)

    def test_rejects_missing_required_server_json_contracts(self) -> None:
        for field in ("initialization_options", "configuration"):
            with self.subTest(field=field):
                candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
                candidate["servers"][0].pop(field)
                self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
                value = copy.deepcopy(self.value)
                value["candidate"]["summary_sha256"] = self.file_sha256(
                    self.candidate_summary
                )
                self.assert_invalid(value)

    def test_rejects_tampered_verdict_even_when_totals_match(self) -> None:
        value = copy.deepcopy(self.value)
        value["rows"][0]["verdict"] = "stable"
        value["regressions"] = 0
        value["stable"] = 1

        self.assert_invalid(value)

    def test_rejects_tampered_row_and_top_level_counts(self) -> None:
        row_count = copy.deepcopy(self.value)
        row_count["rows"][0]["candidate_count"] = 19
        self.assert_invalid(row_count)

        total_count = copy.deepcopy(self.value)
        total_count["regressions"] = 0
        self.assert_invalid(total_count)

    def test_rejects_duplicate_metric_rows_even_when_totals_match(self) -> None:
        value = copy.deepcopy(self.value)
        value["rows"].append(copy.deepcopy(value["rows"][0]))
        value["comparable_metrics"] = 2
        value["regressions"] = 2

        self.assert_invalid(value)

    def test_rejects_rows_outside_the_summary_metric_contract(self) -> None:
        for field, replacement in (
            ("server", "other"),
            ("fixture", "other-fixture"),
            ("workload", "other-workload"),
            ("metric", "textDocument/definition"),
        ):
            with self.subTest(field=field):
                value = copy.deepcopy(self.value)
                value["rows"][0][field] = replacement
                self.assert_invalid(value)

    def test_rejects_row_repetition_counts_not_declared_by_candidate(self) -> None:
        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        row["expected_runs"] = 4
        row["baseline_successful_runs"] = 4
        row["candidate_successful_runs"] = 4

        self.assert_invalid(value)

    def test_rejects_summary_digest_revision_and_source_mismatches(self) -> None:
        with self.subTest(kind="digest"):
            value = copy.deepcopy(self.value)
            value["candidate"]["summary_sha256"] = "f" * 64
            self.assert_invalid(value)

        with self.subTest(kind="comparison revision"):
            value = copy.deepcopy(self.value)
            value["candidate"]["revision"] = "5" * 40
            self.assert_invalid(value)

        with self.subTest(kind="comparison source"):
            value = copy.deepcopy(self.value)
            value["candidate"]["source_url"] = "https://example.invalid/solar.git"
            self.assert_invalid(value)

        with self.subTest(kind="summary revision"):
            self.write_summary_files(candidate_revision="5" * 40)
            self.assert_invalid(self.value)
            self.write_summary_files()

        with self.subTest(kind="summary source"):
            self.write_summary_files(candidate_url="https://example.invalid/solar.git")
            self.assert_invalid(self.value)
            self.write_summary_files()

        with self.subTest(kind="comparison executable"):
            value = copy.deepcopy(self.value)
            value["candidate"]["executable_sha256"] = "6" * 64
            self.assert_invalid(value)

    def test_rejects_report_parameters_that_differ_from_trusted_values(self) -> None:
        value = copy.deepcopy(self.value)
        value["threshold_pct"] = 100.0
        self.assert_invalid(value)

        value = copy.deepcopy(self.value)
        value["min_samples"] = 1
        self.assert_invalid(value)

        self.write_comparison()
        self.assert_invalid(expected_threshold="0")
        self.assert_invalid(expected_threshold="11")
        self.assert_invalid(expected_min_samples="3")
        self.assert_invalid(expected_candidate_revision="5" * 40)
        self.assert_invalid(
            expected_baseline_source_url="https://example.invalid/solar.git"
        )
        self.assert_invalid(
            expected_candidate_source_url="https://example.invalid/solar.git"
        )

    def test_rejects_row_state_not_backed_by_the_summaries(self) -> None:
        cases = []

        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        row["baseline_status"] = None
        row["baseline_successful_runs"] = None
        for field in ("count", "mean", "p50", "p95"):
            row[f"baseline_{field}"] = None
        refresh_deltas(row)
        cases.append((value, "metric group is missing from the baseline"))

        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        row["candidate_status"] = None
        row["candidate_successful_runs"] = None
        for field in ("count", "mean", "p50", "p95"):
            row[f"candidate_{field}"] = None
        refresh_deltas(row)
        cases.append((value, "metric group is missing from the candidate"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["baseline_status"] = "failed"
        cases.append((value, "baseline group did not pass"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["candidate_status"] = "failed"
        cases.append((value, "candidate group did not pass"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["expected_runs"] = None
        cases.append((value, "workload repetition contract is missing"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["baseline_successful_runs"] = 2
        cases.append((value, "baseline group did not complete every configured repetition"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["candidate_successful_runs"] = 2
        cases.append((value, "candidate group did not complete every configured repetition"))

        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        for field in ("count", "mean", "p50", "p95"):
            row[f"baseline_{field}"] = None
        refresh_deltas(row)
        cases.append((value, "metric is missing from the baseline"))

        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        for field in ("count", "mean", "p50", "p95"):
            row[f"candidate_{field}"] = None
        refresh_deltas(row)
        cases.append((value, "metric is missing from the candidate"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["baseline_count"] = 1
        value["rows"][0]["candidate_count"] = 1
        cases.append((value, "metric has fewer than 2 samples"))

        value = copy.deepcopy(self.value)
        value["rows"][0]["candidate_count"] = 19
        cases.append((value, "baseline and candidate sample counts differ"))

        value = copy.deepcopy(self.value)
        row = value["rows"][0]
        row["baseline_mean"] = 0.0
        row["baseline_p50"] = 0.0
        row["baseline_p95"] = 0.0
        refresh_deltas(row)
        cases.append((value, "baseline metric contains a zero or non-finite value"))

        for value, reason in cases:
            with self.subTest(reason=reason):
                make_inconclusive(value, reason)
                self.assert_invalid(value)

    def test_rejects_a_forged_inconclusive_reason_that_hides_a_regression(self) -> None:
        value = copy.deepcopy(self.value)
        make_inconclusive(value, "candidate group did not complete every configured repetition")

        self.assert_invalid(value)

    def test_accepts_metric_present_on_only_one_side(self) -> None:
        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        baseline["summaries"][0]["metrics"] = {}
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")

        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)
        row = value["rows"][0]
        for field in ("count", "mean", "p50", "p95"):
            row[f"baseline_{field}"] = None
        refresh_deltas(row)
        make_inconclusive(value, "metric is missing from the baseline")
        self.write_comparison(value)

        result = self.run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        report = self.output.read_text(encoding="utf-8")
        self.assertIn("**INCONCLUSIVE**", report)
        self.assertNotIn("artifact was missing or invalid", report)

    def test_rejects_missing_workload_repetition_contract(self) -> None:
        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        baseline["workloads"] = []
        candidate["workloads"] = []
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")

        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        value["rows"][0]["expected_runs"] = None
        make_inconclusive(value, "workload repetition contract is missing")
        self.assert_invalid(value)

    def test_classifies_improvement_and_mixed_quantiles_from_recomputed_deltas(self) -> None:
        improvement = copy.deepcopy(self.value)
        row = improvement["rows"][0]
        row["candidate_mean"] = 0.8
        row["candidate_p50"] = 0.8
        row["candidate_p95"] = 0.9
        refresh_deltas(row)
        row["verdict"] = "improvement"
        improvement["regressions"] = 0
        improvement["improvements"] = 1
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate_stats = candidate["summaries"][0]["metrics"]["textDocument/hover"]
        candidate_stats.update(
            {"mean": row["candidate_mean"], "p50": row["candidate_p50"], "p95": row["candidate_p95"]}
        )
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        improvement["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        self.write_comparison(improvement)
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("NO REGRESSION", self.output.read_text(encoding="utf-8"))

        stable = copy.deepcopy(self.value)
        row = stable["rows"][0]
        row["candidate_p50"] = 1.05
        row["candidate_p95"] = 1.3
        refresh_deltas(row)
        row["verdict"] = "stable"
        stable["regressions"] = 0
        stable["stable"] = 1
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate_stats = candidate["summaries"][0]["metrics"]["textDocument/hover"]
        candidate_stats.update(
            {"mean": row["candidate_mean"], "p50": row["candidate_p50"], "p95": row["candidate_p95"]}
        )
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        stable["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        self.write_comparison(stable)
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("**STABLE**", self.output.read_text(encoding="utf-8"))

    def test_untrusted_blocker_text_is_not_rendered(self) -> None:
        candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
        candidate["config_sha256"] = "e" * 64
        self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        value["compatible"] = False
        value["blockers"] = [
            "@maintainers [click](https://example.invalid) <details> | unsafe & text"
        ]
        make_inconclusive(value, "run metadata is incompatible")
        self.write_comparison(value)

        result = self.run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        report = self.output.read_text(encoding="utf-8")
        self.assertIn("benchmark config differs between baseline and candidate", report)
        self.assertNotIn("@maintainers", report)
        self.assertNotIn("[click](https://example.invalid)", report)
        self.assertNotIn("https://example.invalid", report)

    def test_untrusted_row_labels_are_markdown_escaped(self) -> None:
        evil = "https://evil.example"
        for path in (self.baseline_summary, self.candidate_summary):
            value = json.loads(path.read_text(encoding="utf-8"))
            value["workloads"][0]["id"] = evil
            value["summaries"][0]["workload"] = evil
            metrics = value["summaries"][0].pop("metrics")
            value["summaries"][0]["metrics"] = {evil: next(iter(metrics.values()))}
            path.write_text(json.dumps(value), encoding="utf-8")

        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)
        value["candidate"]["summary_sha256"] = self.file_sha256(self.candidate_summary)
        value["rows"][0]["workload"] = evil
        value["rows"][0]["metric"] = evil
        self.write_comparison(value)

        result = self.run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        report = self.output.read_text(encoding="utf-8")
        self.assertIn("https&#58;&#47;&#47;evil&#46;example", report)
        self.assertNotIn("https://evil.example", report)

    def test_rejects_non_pr_or_multi_server_summaries(self) -> None:
        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        baseline["profile"] = "default"
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        self.assert_invalid(self.value)

        baseline["profile"] = "pr"
        baseline["servers"].append(
            {
                "id": "other",
                "source": {"url": SOURCE_URL, "revision": BASELINE_REVISION},
                "executable_sha256": "7" * 64,
            }
        )
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        self.assert_invalid(self.value)

    def test_rejects_unknown_accounting_backends(self) -> None:
        fields = ("accounting_backends", "memory_accounting_backends")
        for field in fields:
            with self.subTest(field=field):
                candidate = summary(CANDIDATE_REVISION, CANDIDATE_EXECUTABLE)
                candidate["environment"][field] = ["forged-backend"]
                self.candidate_summary.write_text(json.dumps(candidate), encoding="utf-8")
                value = copy.deepcopy(self.value)
                value["candidate"]["summary_sha256"] = self.file_sha256(
                    self.candidate_summary
                )
                self.assert_invalid(value)

    def test_malformed_fixture_metadata_renders_fixed_inconclusive_report(self) -> None:
        baseline = summary(BASELINE_REVISION, BASELINE_EXECUTABLE)
        baseline["fixtures"] = [None]
        self.baseline_summary.write_text(json.dumps(baseline), encoding="utf-8")
        value = copy.deepcopy(self.value)
        value["baseline"]["summary_sha256"] = self.file_sha256(self.baseline_summary)

        self.assert_invalid(value)

    def test_rejects_untrusted_source_path_and_duplicate_json_keys(self) -> None:
        value = copy.deepcopy(self.value)
        value["candidate"]["path"] = "../../outside/summary.json"
        self.assert_invalid(value)

        self.input.write_text(
            json.dumps(self.value).replace(
                '"schema_version": 2', '"schema_version": 2, "schema_version": 2', 1
            ),
            encoding="utf-8",
        )
        self.assert_invalid()


if __name__ == "__main__":
    unittest.main()
