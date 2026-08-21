import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from jsonschema import Draft202012Validator

sys.path.insert(0, str(Path(__file__).parent))
import report as benchmark


SCHEMA = json.loads(
    (Path(__file__).resolve().parents[2] / "benches/schema/benchmark-result-v1.schema.json").read_text()
)


def result(test_id="test", suite="repository", **compiler):
    return {"test_id": test_id, "suite": suite, "compilers": {"solar": compiler}}


class ReportFormattingTests(unittest.TestCase):
    def test_unchanged_report_has_note(self):
        report = benchmark.format_report("## Results", False, False)
        self.assertEqual(
            report,
            "> [!NOTE]\n"
            "> Codegen benchmark output is unchanged from `main`.\n\n"
            "<details>\n"
            "<summary>Codegen benchmark output</summary>\n\n"
            "## Results\n\n"
            "</details>\n",
        )

    def test_changed_report_has_no_details(self):
        self.assertEqual(benchmark.format_report("## Results", True, False), "## Results")

    def test_unchanged_report_uses_base_branch(self):
        report = benchmark.format_report("## Results", False, False, "feat/base")
        self.assertEqual(
            report,
            "> [!NOTE]\n"
            "> Codegen benchmark output is unchanged from `feat/base`.\n\n"
            "<details>\n"
            "<summary>Codegen benchmark output</summary>\n\n"
            "## Results\n\n"
            "</details>\n",
        )

    def test_behind_main_report_has_warning(self):
        report = benchmark.format_report("## Results", True, True)
        self.assertEqual(
            report,
            "> [!WARNING]\n"
            "> This branch is behind `main`, so these benchmark results may be incorrect.\n\n"
            "<details>\n"
            "<summary>Codegen benchmark output</summary>\n\n"
            "## Results\n\n"
            "</details>\n",
        )

    def test_unchanged_behind_main_report_has_note_and_warning(self):
        report = benchmark.format_report("## Results", False, True)
        self.assertEqual(
            report,
            "> [!WARNING]\n"
            "> This branch is behind `main`, so these benchmark results may be incorrect.\n\n"
            "> [!NOTE]\n"
            "> Codegen benchmark output is unchanged from `main`.\n\n"
            "<details>\n"
            "<summary>Codegen benchmark output</summary>\n\n"
            "## Results\n\n"
            "</details>\n",
        )

    def test_lower_is_better_delta_uses_conventional_sign(self):
        self.assertEqual(
            benchmark.fmt_value_with_lower_is_better_delta(95, 95, 100),
            "95 (✅ -5.00%)",
        )
        self.assertEqual(
            benchmark.fmt_value_with_lower_is_better_delta(105, 105, 100, "B"),
            "105B (❌ +5.00%)",
        )

    def test_peak_rss_report_is_collapsed(self):
        report = benchmark.memory_report(
            [
                {
                    "test_id": "test",
                    "compilers": {
                        "solar": {"status": "ok", "peak_rss_bytes": 1024 * 1024},
                        "solc": {"status": "ok", "peak_rss_bytes": 2 * 1024 * 1024},
                    },
                }
            ]
        )
        self.assertEqual(
            report,
            [
                "<details>",
                "<summary>Peak RSS</summary>",
                "",
                "| compiler | benches | average peak RSS | maximum peak RSS | maximum bench |",
                "| -------- | ------- | ---------------- | ---------------- | ------------- |",
                "| solar | 1 | 1.0 MiB | 1.0 MiB | test |",
                "| solc | 1 | 2.0 MiB | 2.0 MiB | test |",
                "",
                "#### Per-benchmark peak RSS",
                "",
                "| bench | solar peak | solc peak | Solar vs solc |",
                "| --- | --- | --- | --- |",
                "| test | 1.0 MiB | 2.0 MiB | ✅ -50.00% |",
                "",
                "</details>",
                "",
            ],
        )

    def test_codegen_report_combines_all_benches(self):
        micro = result("micro", suite="micro", status="ok", total_gas=10, runtime_size=20)
        repository = result("repository", status="ok", total_gas=30, runtime_size=40)
        large = result("large", suite="large", status="ok", total_gas=50, runtime_size=60)
        report = benchmark.codegen_report(
            [micro, repository, large], [micro, repository, large]
        )
        self.assertEqual(
            report,
            "## Codegen benchmark\n"
            "\n"
            "| bench | gas (vs main) | solc | size (vs main) | solc |\n"
            "| ----- | ------------- | ---- | -------------- | ---- |\n"
            "| micro | 10 (~0%) | n/a (n/a) | 20B (~0%) | n/a (n/a) |\n"
            "| repository | 30 (~0%) | n/a (n/a) | 40B (~0%) | n/a (n/a) |\n"
            "| large | 50 (~0%) | n/a (n/a) | 60B (~0%) | n/a (n/a) |\n"
        )

    def test_codegen_report_uses_base_branch(self):
        micro = result("micro", suite="micro", status="ok", total_gas=10, runtime_size=20)
        self.assertEqual(
            benchmark.codegen_report([micro], [micro], "feat/base"),
            "## Codegen benchmark\n"
            "\n"
            "| bench | gas (vs feat/base) | solc | size (vs feat/base) | solc |\n"
            "| ----- | ------------- | ---- | -------------- | ---- |\n"
            "| micro | 10 (~0%) | n/a (n/a) | 20B (~0%) | n/a (n/a) |\n",
        )


class CommonBenchmarkResultTests(unittest.TestCase):
    def write_result(self, results, timings=None):
        if timings is None:
            timings = {"micro": 1.25}
        with tempfile.TemporaryDirectory() as directory, patch.dict(
            os.environ,
            {
                "GITHUB_REPOSITORY": "paradigmxyz/solar",
                "GITHUB_SHA": "0123456789abcdef0123456789abcdef01234567",
                "BENCHMARK_PR_NUMBER": "123",
            },
        ), patch.object(
            benchmark,
            "runner_metadata",
            return_value={"os": "linux", "arch": "x86_64", "logical_cpus": 4},
        ):
            output = Path(directory) / "common.json"
            benchmark.write_common_result(output, results, timings)
            document = json.loads(output.read_text())
        Draft202012Validator(SCHEMA).validate(document)
        return document

    def test_writes_complete_schema_valid_result(self):
        micro = [
            result(
                status="ok",
                total_gas=10,
                deploy_gas=20,
                bytecode_size=30,
                runtime_size=40,
                peak_rss_bytes=100,
            ),
            result(
                status="ok",
                total_gas=1,
                deploy_gas=2,
                bytecode_size=3,
                runtime_size=4,
                peak_rss_bytes=200,
            ),
        ]
        document = self.write_result(
            [{**entry, "suite": "micro"} for entry in micro]
        )
        self.assertEqual(
            document,
            {
                "schema_version": 1,
                "repo": "paradigmxyz/solar",
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "pr": 123,
                "runner": {"os": "linux", "arch": "x86_64", "logical_cpus": 4},
                "benchmarks": [
                    {
                        "name": "codegen_runtime_suite/micro",
                        "wall_time": {
                            "value": 1.25,
                            "unit": "second",
                            "statistic": "total",
                        },
                        "counters": {
                            "tests": {"value": 2, "unit": "count", "statistic": "total"},
                            "successful_compilations": {
                                "value": 2,
                                "unit": "count",
                                "statistic": "total",
                            },
                            "failed_compilations": {
                                "value": 0,
                                "unit": "count",
                                "statistic": "total",
                            },
                        },
                        "gas": {
                            "runtime": {"value": 11, "unit": "gas", "statistic": "total"},
                            "deployment": {
                                "value": 22,
                                "unit": "gas",
                                "statistic": "total",
                            },
                        },
                        "compiler": {
                            "creation_bytecode_size": {
                                "value": 33,
                                "unit": "byte",
                                "statistic": "total",
                            },
                            "runtime_bytecode_size": {
                                "value": 44,
                                "unit": "byte",
                                "statistic": "total",
                            },
                        },
                        "memory": {
                            "value": 200,
                            "unit": "byte",
                            "statistic": "max",
                        },
                    }
                ],
            },
        )

    def test_omits_aggregates_after_compilation_failure(self):
        compilation_failure = [
            result(
                status="ok",
                total_gas=10,
                deploy_gas=20,
                bytecode_size=30,
                runtime_size=40,
            ),
            result(status="failed"),
        ]
        document = self.write_result(
            [{**entry, "suite": "micro"} for entry in compilation_failure]
        )
        benchmark_result = document["benchmarks"][0]
        self.assertNotIn("gas", benchmark_result)
        self.assertNotIn("compiler", benchmark_result)
        self.assertEqual(benchmark_result["counters"]["failed_compilations"]["value"], 1)

    def test_omits_each_incomplete_metric(self):
        complete = {
            "status": "ok",
            "total_gas": 10,
            "deploy_gas": 20,
            "bytecode_size": 30,
            "runtime_size": 40,
        }
        cases = [
            ("total_gas", "gas", "runtime"),
            ("deploy_gas", "gas", "deployment"),
            ("bytecode_size", "compiler", "creation_bytecode_size"),
            ("runtime_size", "compiler", "runtime_bytecode_size"),
        ]
        for missing, group, metric_name in cases:
            with self.subTest(missing=missing):
                incomplete = complete | {missing: None}
                document = self.write_result(
                    [
                        {**result(**complete), "suite": "micro"},
                        {**result(**incomplete), "suite": "micro"},
                    ]
                )
                self.assertNotIn(metric_name, document["benchmarks"][0][group])

    def test_omits_suite_without_timing(self):
        results = [result(status="ok", bytecode_size=1, runtime_size=1, suite="repo")]
        document = self.write_result(
            results,
            {"repo": {"wall_time_seconds": 2.0}},
        )
        self.assertEqual(
            [entry["name"] for entry in document["benchmarks"]],
            ["codegen_runtime_suite/repository"],
        )

    def test_writes_large_contract_suite(self):
        results = [
            result(
                "large",
                suite="large",
                status="ok",
                total_gas=10,
                deploy_gas=20,
                bytecode_size=30,
                runtime_size=40,
            )
        ]
        document = self.write_result(
            results,
            {"large": {"wall_time_seconds": 3.5}},
        )
        self.assertEqual(
            [entry["name"] for entry in document["benchmarks"]],
            ["codegen_runtime_suite/large"],
        )


if __name__ == "__main__":
    unittest.main()


class CompileTimeReportTests(unittest.TestCase):
    @staticmethod
    def timed_result(test_id, solc_seconds, solar_seconds, solar_status="ok"):
        return {
            "test_id": test_id,
            "suite": "repository",
            "compilers": {
                "solc": {"status": "ok", "compile_time_seconds": solc_seconds},
                "solar": {
                    "status": solar_status,
                    "compile_time_seconds": solar_seconds,
                    "command": "target/release/solar --standard-json",
                    "label": "solar 0.2.0",
                    "input_fingerprint": "input",
                },
            },
        }

    def test_compile_time_report_rows_and_sum(self):
        results = [
            self.timed_result("fast", 0.100, 0.005),
            self.timed_result("slow", 1.500, 0.055),
        ]
        lines = benchmark.compile_time_report(results, {}, "`main`")
        text = "\n".join(lines)
        self.assertIn("### Compilation time", text)
        self.assertIn("| fast | 5.0 ms (n/a) | 100.0 ms (✅ +1900.00%) |", text)
        self.assertIn("| slow | 55.0 ms (n/a) | 1.500 s (✅ +2627.27%) |", text)
        self.assertIn(
            "| **sum of medians** | **60.0 ms** | **1.600 s (✅ +2566.67%)** |", text
        )

    def test_compile_time_sum_skips_unpaired_results(self):
        results = [
            self.timed_result("ok", 0.200, 0.010),
            self.timed_result("failed", 0.400, 0.010, solar_status="failed"),
        ]
        text = "\n".join(benchmark.compile_time_report(results, {}, "`main`"))
        self.assertIn("| failed | n/a (n/a) | 400.0 ms (n/a) |", text)
        self.assertIn(
            "| **sum of medians** | **10.0 ms** | **200.0 ms (✅ +1900.00%)** |", text
        )

    def test_compile_time_report_uses_solar_baseline_delta(self):
        results = [self.timed_result("bench", 0.100, 0.011)]
        baseline = {
            ("repository", "bench"): self.timed_result("bench", 0.100, 0.010),
        }
        text = "\n".join(benchmark.compile_time_report(results, baseline, "`main`"))
        self.assertIn("(❌ +10.00%)", text)

    def test_compile_time_baseline_ignores_different_build_profile(self):
        current = self.timed_result("bench", 0.100, 0.011)
        baseline = self.timed_result("bench", 0.100, 0.010)
        baseline["compilers"]["solar"]["command"] = "target/debug/solar --standard-json"
        text = "\n".join(
            benchmark.compile_time_report(
                [current], {("repository", "bench"): baseline}, "`main`"
            )
        )
        self.assertIn("| bench | 11.0 ms (n/a) | 100.0 ms (✅ +809.09%) |", text)
        self.assertTrue(benchmark.has_baseline_changes([current], [baseline]))

    def test_compile_time_baseline_ignores_different_input(self):
        current = self.timed_result("bench", 0.100, 0.011)
        baseline = self.timed_result("bench", 0.100, 0.010)
        baseline["compilers"]["solar"]["input_fingerprint"] = "old-input"
        text = "\n".join(
            benchmark.compile_time_report(
                [current], {("repository", "bench"): baseline}, "`main`"
            )
        )
        self.assertIn("| bench | 11.0 ms (n/a) | 100.0 ms (✅ +809.09%) |", text)
        self.assertTrue(benchmark.has_baseline_changes([current], [baseline]))

    def test_whole_project_rows_skip_missing_codegen(self):
        result = {
            "test_id": "heavy-project",
            "suite": "heavy",
            "compilers": {
                "solc": {
                    "status": "ok",
                    "compile_time_seconds": 60.0,
                    "bytecode_size": None,
                    "runtime_size": None,
                },
                "solar": {
                    "status": "ok",
                    "compile_time_seconds": 5.0,
                    "bytecode_size": None,
                    "runtime_size": None,
                },
            },
        }
        rows = benchmark.benchmark_rows([result], {})
        self.assertEqual(rows, [])
        text = "\n".join(benchmark.compile_time_report([result], {}, "`main`"))
        self.assertIn(
            "| heavy-project | 5.000 s (n/a) | 60.000 s (✅ +1100.00%) |", text
        )

    @staticmethod
    def paired(current_seconds, baseline_seconds):
        current = CompileTimeReportTests.timed_result("bench", 1.0, current_seconds)
        base = CompileTimeReportTests.timed_result("bench", 1.0, baseline_seconds)
        return [current], [base]

    def test_compile_time_change_requires_relative_and_absolute_thresholds(self):
        results, baseline = self.paired(0.125, 0.100)
        self.assertTrue(benchmark.has_baseline_changes(results, baseline))
        self.assertFalse(benchmark.has_codegen_changes(results, baseline))
        results, baseline = self.paired(0.006, 0.003)
        self.assertFalse(benchmark.has_baseline_changes(results, baseline))

    def test_compile_time_total_change_requires_relative_and_absolute_thresholds(self):
        current = [
            self.timed_result("a", 1.0, 0.112),
            self.timed_result("b", 1.0, 0.112),
        ]
        base = [
            self.timed_result("a", 1.0, 0.100),
            self.timed_result("b", 1.0, 0.100),
        ]
        self.assertFalse(benchmark.has_baseline_changes(current, base))
        current = [self.timed_result(str(i), 1.0, 0.056) for i in range(200)]
        base = [self.timed_result(str(i), 1.0, 0.050) for i in range(200)]
        self.assertTrue(benchmark.has_baseline_changes(current, base))

    def test_compile_time_bootstrap_triggers_comments(self):
        current = [self.timed_result("bench", 1.0, 0.1)]
        base = [{"test_id": "bench", "suite": "repository", "compilers": {"solar": {"status": "ok"}}}]
        self.assertTrue(benchmark.has_baseline_changes(current, base))

    def test_codegen_changes_trigger_comments(self):
        current = [result(status="ok", total_gas=2)]
        base = [result(status="ok", total_gas=1)]
        self.assertTrue(benchmark.has_codegen_changes(current, base))

    def test_compile_time_report_empty_without_pairs(self):
        results = [self.timed_result("failed", 0.400, 0.010, solar_status="failed")]
        self.assertEqual(benchmark.compile_time_report(results, {}, "`main`"), [])
