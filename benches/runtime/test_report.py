import copy
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from jsonschema import Draft202012Validator

sys.path.insert(0, str(Path(__file__).parent))
SPEC = importlib.util.spec_from_file_location(
    "benchmark_compare", Path(__file__).with_name("benchmark-compare.py")
)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)

SCHEMA = json.loads(
    (
        Path(__file__).resolve().parents[2]
        / "benches/schema/benchmark-result-v1.schema.json"
    ).read_text()
)


def result(test_id="test", suite="repository", **compiler):
    return {"test_id": test_id, "suite": suite, "compilers": {"solar": compiler}}


class ReportFormattingTests(unittest.TestCase):
    def test_perf_link_uses_default_site(self):
        with patch.dict(
            os.environ,
            {
                "BENCHMARK_BASE_SHA": "0123456789abcdef",
                "BENCHMARK_PR_HEAD_SHA": "fedcba9876543210",
            },
            clear=True,
        ):
            self.assertEqual(
                benchmark.perf_link("Results"),
                "[Results](https://getfoundry.sh/perf/?base=01234567&head=fedcba98#benchmarks)",
            )
            self.assertEqual(
                benchmark.perf_link("factorial", "factorial"),
                "[factorial](https://getfoundry.sh/perf/?base=01234567&head=fedcba98&benchmark=factorial#artifacts)",
            )

    def test_perf_link_targets_artifact(self):
        with patch.dict(
            os.environ,
            {
                "BENCHMARK_BASE_SHA": "0123456789abcdef",
                "BENCHMARK_PR_HEAD_SHA": "fedcba9876543210",
                "BENCHMARK_SITE_URL": "https://example.test/",
            },
        ):
            link = benchmark.perf_link("factorial", "factorial")

        self.assertEqual(
            link,
            "[factorial](https://example.test/?base=01234567&head=fedcba98&benchmark=factorial#artifacts)",
        )

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
        self.assertEqual(
            benchmark.format_report(
                "## Results", True, False, comparison="### Run comparison"
            ),
            "### Run comparison\n\n## Results",
        )

    def test_notices_precede_comparison_and_details(self):
        self.assertEqual(
            benchmark.format_report(
                "## Results", False, True, "main", "### Run comparison"
            ),
            "> [!WARNING]\n"
            "> This branch is behind `main`, so these benchmark results may be incorrect.\n\n"
            "> [!NOTE]\n> Codegen benchmark output is unchanged from `main`.\n\n"
            "<details>\n<summary>Codegen benchmark output</summary>\n\n"
            "### Run comparison\n\n"
            "## Results\n\n</details>\n",
        )

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
        report = benchmark.format_report(
            "## Results", True, True, comparison="### Run comparison"
        )
        self.assertEqual(
            report,
            "> [!WARNING]\n"
            "> This branch is behind `main`, so these benchmark results may be incorrect.\n\n"
            "<details>\n"
            "<summary>Codegen benchmark output</summary>\n\n"
            "### Run comparison\n\n"
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
        micro = result(
            "micro", suite="micro", status="ok", total_gas=10, runtime_size=20
        )
        repository = result("repository", status="ok", total_gas=30, runtime_size=40)
        large = result(
            "large", suite="large", status="ok", total_gas=50, runtime_size=60
        )
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
            "| large | 50 (~0%) | n/a (n/a) | 60B (~0%) | n/a (n/a) |\n",
        )

    def test_codegen_report_uses_base_branch(self):
        micro = result(
            "micro", suite="micro", status="ok", total_gas=10, runtime_size=20
        )
        self.assertEqual(
            benchmark.codegen_report([micro], [micro], "feat/base"),
            "## Codegen benchmark\n"
            "\n"
            "| bench | gas (vs feat/base) | solc | size (vs feat/base) | solc |\n"
            "| ----- | ------------- | ---- | -------------- | ---- |\n"
            "| micro | 10 (~0%) | n/a (n/a) | 20B (~0%) | n/a (n/a) |\n",
        )

    def test_codegen_report_adds_deployment_table(self):
        current = result(status="ok", deploy_gas=110, bytecode_size=210)
        baseline = result(status="ok", deploy_gas=100, bytecode_size=200)
        report = benchmark.codegen_report([current], [baseline])
        self.assertEqual(
            report,
            "## Codegen benchmark\n"
            "\n"
            "### Deployment\n"
            "\n"
            "| bench | gas (vs main) | solc | size (vs main) | solc |\n"
            "| ----- | ------------- | ---- | -------------- | ---- |\n"
            "| test | 110 (❌ +10.00%) | n/a (n/a) | 210B (❌ +5.00%) | n/a (n/a) |\n",
        )

    def test_codegen_report_labels_failed_revision(self):
        def timed_result(test_id, solar_status):
            return {
                "test_id": test_id,
                "suite": "repository",
                "compilers": {
                    "solc": {"status": "ok", "compile_time_seconds": 0.100},
                    "solar": {
                        "status": solar_status,
                        "compile_time_seconds": 0.010,
                        "command": "target/release/solar --standard-json",
                        "label": "solar 0.2.0",
                        "input_fingerprint": "input",
                    },
                },
            }

        current = [
            timed_result("base-failed", "ok"),
            timed_result("branch-failed", "failed"),
            timed_result("both-failed", "failed"),
        ]
        baseline = [
            timed_result("base-failed", "failed"),
            timed_result("branch-failed", "ok"),
            timed_result("both-failed", "failed"),
        ]
        report = benchmark.codegen_report(current, baseline)
        self.assertIn(
            "> [!NOTE]\n"
            "> The compiler failed on these benchmarks; `n/a` marks the failed revision:\n"
            ">\n"
            "> - `repository/base-failed`: `main` = `n/a`, branch = `ok`\n"
            "> - `repository/branch-failed`: `main` = `ok`, branch = `n/a`\n"
            "> - `repository/both-failed`: `main` = `n/a`, branch = `n/a`\n",
            report,
        )
        self.assertIn(
            "| base-failed | 10.0 ms (n/a) | 100.0 ms (✅ +900.00%) |", report
        )
        self.assertIn("| branch-failed | n/a (n/a) | 100.0 ms (n/a) |", report)

        for index in range(2):
            self.assertTrue(
                benchmark.has_codegen_changes([current[index]], [baseline[index]])
            )
        self.assertFalse(benchmark.has_codegen_changes(current[2:], baseline[2:]))

    def test_unexpected_benchmark_failure_is_reported_once(self):
        failure = {
            "test_id": "crashed",
            "suite": "repository",
            "benchmark_error": "unexpected benchmark failure: RuntimeError: broken",
            "compilers": {
                "solc": {"status": "failed"},
                "solar": {"status": "failed"},
            },
        }

        self.assertEqual(
            benchmark.compiler_failures([failure]),
            [
                (
                    "repository/crashed: unexpected benchmark failure: "
                    "RuntimeError: broken"
                )
            ],
        )


class CommonBenchmarkResultTests(unittest.TestCase):
    def write_result(self, results, timings=None):
        if timings is None:
            timings = {"micro": 1.25}
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.dict(
                os.environ,
                {
                    "GITHUB_REPOSITORY": "paradigmxyz/solar",
                    "GITHUB_SHA": "0123456789abcdef0123456789abcdef01234567",
                    "BENCHMARK_PR_NUMBER": "123",
                },
            ),
            patch.object(
                benchmark,
                "runner_metadata",
                return_value={"os": "linux", "arch": "x86_64", "logical_cpus": 4},
            ),
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
        document = self.write_result([{**entry, "suite": "micro"} for entry in micro])
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
                            "tests": {
                                "value": 2,
                                "unit": "count",
                                "statistic": "total",
                            },
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
                            "runtime": {
                                "value": 11,
                                "unit": "gas",
                                "statistic": "total",
                            },
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
        self.assertEqual(
            benchmark_result["counters"]["failed_compilations"]["value"], 1
        )

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
        self.assertIn("<summary>Compilation time</summary>", text)
        self.assertIn("| fast | 5.0 ms (n/a) | 100.0 ms (✅ +1900.00%) |", text)
        self.assertIn("| slow | 55.0 ms (n/a) | 1.500 s (✅ +2627.27%) |", text)
        self.assertIn(
            "| **sum of medians** | **60.0 ms** | **1.600 s (✅ +2566.67%)** |", text
        )
        self.assertTrue(text.endswith("\n</details>\n"))

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
        base = [
            {
                "test_id": "bench",
                "suite": "repository",
                "compilers": {"solar": {"status": "ok"}},
            }
        ]
        self.assertTrue(benchmark.has_baseline_changes(current, base))

    def test_codegen_changes_trigger_comments(self):
        current = [result(status="ok", total_gas=2)]
        base = [result(status="ok", total_gas=1)]
        self.assertTrue(benchmark.has_codegen_changes(current, base))

    def test_deployment_changes_trigger_comments(self):
        for metric in ("deploy_gas", "bytecode_size"):
            with self.subTest(metric=metric):
                current = [result(status="ok", **{metric: 2})]
                base = [result(status="ok", **{metric: 1})]
                self.assertTrue(benchmark.has_codegen_changes(current, base))

    def test_compile_time_report_empty_without_pairs(self):
        results = [self.timed_result("failed", 0.400, 0.010, solar_status="failed")]
        self.assertEqual(benchmark.compile_time_report(results, {}, "`main`"), [])


class RunComparisonTests(unittest.TestCase):
    def test_main_report_tables_with_baseline_rss(self):
        before = self.fixture()
        before["compilers"]["solc"] = copy.deepcopy(before["compilers"]["solar"])
        after = copy.deepcopy(before)
        after["compilers"]["solar"]["runtime_size"] = 90
        compared = benchmark.compare_runs([after], [before])
        rows = {(row["suite"], row["test_id"]): row for row in compared["rows"]}
        # Snapshot from main's report.py at d14d1ebfd.
        expected = """## Codegen benchmark

| bench | gas (vs baseline) | solc | size (vs baseline) | solc |
| ----- | ------------- | ---- | -------------- | ---- |
| test | 30 (~0%) | 30 (~0%) | 90B (✅ -10.00%) | 100B (✅ +11.11%) |

### Deployment

| bench | gas (vs baseline) | solc | size (vs baseline) | solc |
| ----- | ------------- | ---- | -------------- | ---- |
| test | 50 (~0%) | 50 (~0%) | 120B (~0%) | 120B (~0%) |

<details>
<summary>Compilation time</summary>

| bench | time (vs baseline) | solc |
| ----- | --------------------- | ---- |
| test | 100.0 ms (~0%) | 100.0 ms (~0%) |
| **sum of medians** | **100.0 ms** | **100.0 ms (~0%)** |

</details>

<details>
<summary>Peak RSS</summary>

| compiler | benches | average peak RSS | maximum peak RSS | maximum bench |
| -------- | ------- | ---------------- | ---------------- | ------------- |
| solar | 1 | 0.0 MiB | 0.0 MiB | test |
| solc | 1 | 0.0 MiB | 0.0 MiB | test |

#### Per-benchmark peak RSS

| bench | solar peak | solc peak | Solar vs solc | Solar vs baseline |
| --- | --- | --- | --- | --- |
| test | 0.0 MiB | 0.0 MiB | ~0% | ~0% |

</details>
"""
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                benchmark.codegen_report([after], [before], "baseline", rows), expected
            )

    def test_incomplete_capture_does_not_trigger_codegen_comment(self):
        comparison = benchmark.compare_runs([self.fixture()], [self.fixture()])
        row = comparison["rows"][0]
        row["artifacts"] = [{"status": "added"}]
        row["artifacts_available"] = {"before": False, "after": True}
        self.assertFalse(benchmark.comparison_has_changes(comparison, True))
        row["artifacts_available"]["before"] = True
        self.assertTrue(benchmark.comparison_has_changes(comparison, True))

    def test_common_output_rejects_filtered_measurements(self):
        for flags in (["--tests", "test"], ["--compiler", "solc"]):
            with (
                self.subTest(flags=flags),
                patch("sys.stderr", new_callable=io.StringIO),
                self.assertRaises(SystemExit) as error,
            ):
                benchmark.main(
                    [
                        "before.json",
                        "after.json",
                        "--common-output",
                        "unused.json",
                        *flags,
                    ]
                )
            self.assertEqual(error.exception.code, 2)

    def test_missing_input_still_writes_diagnostic_report(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch("sys.stdout", new_callable=io.StringIO),
            patch("sys.stderr", new_callable=io.StringIO),
        ):
            root = Path(directory)
            output = root / "report.md"
            self.assertEqual(
                benchmark.main(
                    [
                        "--results",
                        str(root / "missing.json"),
                        "--report-output",
                        str(output),
                    ]
                ),
                1,
            )
            self.assertEqual(
                output.read_text(),
                "## Codegen benchmark\n\nNo benchmark results were produced.\n",
            )

    def test_solc_report_uses_selected_compiler(self):
        before = self.fixture()
        before["compilers"]["solc"] = copy.deepcopy(before["compilers"]["solar"])
        after = copy.deepcopy(before)
        after["compilers"]["solc"]["runtime_size"] = 90
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.dict(os.environ, {}, clear=True),
            patch("sys.stdout", new_callable=io.StringIO),
            patch("sys.stderr", new_callable=io.StringIO),
        ):
            root = Path(directory)
            paths = [root / "before.json", root / "after.json"]
            for path, row in zip(paths, [before, after], strict=True):
                path.write_text(json.dumps({"results": [row]}))
            output = root / "report.md"
            self.assertEqual(
                benchmark.main(
                    [
                        *(str(path) for path in paths),
                        "--compiler",
                        "solc",
                        "--report-output",
                        str(output),
                    ]
                ),
                0,
            )
            self.assertEqual(
                output.read_text().splitlines()[:3],
                [
                    "### Run comparison",
                    "",
                    "Compiler: `solc`. Deltas are candidate minus baseline; lower is better.",
                ],
            )

    def fixture(self, test_id="test", **values):
        row = result(
            test_id,
            status="ok",
            input_fingerprint="input",
            output_fingerprint="output",
            label="solar test",
            command="target/debug/solar --standard-json",
            runtime_size=100,
            bytecode_size=120,
            total_gas=30,
            deploy_gas=50,
            compile_time_seconds=0.1,
            compile_time_samples=[0.09, 0.1, 0.11],
            peak_rss_bytes=1000,
            gas_status="ok",
            runtime_status="ok",
            gas_results=[
                {"label": "first", "call": "f()", "args": [], "gas": 10},
                {"label": "second", "call": "f()", "args": [], "gas": 20},
            ],
            runtime_results=[
                {
                    "label": "value",
                    "call": "value()",
                    "args": [],
                    "value": "1",
                    "status": "ok",
                }
            ],
        )
        row["gas_profile"] = "hot"
        row["compilers"]["solar"].update(values)
        return row

    def test_totals_exclude_failed_missing_and_changed_inputs(self):
        before = [
            self.fixture(name) for name in ("paired", "failed", "removed", "input")
        ]
        after = [
            self.fixture("paired", runtime_size=90),
            self.fixture("failed", status="failed"),
            self.fixture("input", input_fingerprint="other"),
        ]
        comparison = benchmark.compare_runs(after, before)
        self.assertEqual(
            comparison["totals"]["runtime_size"],
            {
                "tests": ["repository/paired"],
                "before": 100,
                "after": 90,
                "delta": -10,
                "percent": -10.0,
                "reason": None,
            },
        )
        self.assertEqual(
            [row["test_id"] for row in comparison["rows"]],
            ["failed", "input", "paired", "removed"],
        )

    def test_summary_weights_contract_ratios_equally(self):
        before = [
            self.fixture("small", runtime_size=100),
            self.fixture("large", runtime_size=10000),
        ]
        after = [
            self.fixture("small", runtime_size=101),
            self.fixture("large", runtime_size=10001),
        ]
        summary = benchmark.compare_runs(after, before)["summary"]["runtime_size"]
        self.assertAlmostEqual(summary["percent"], ((1.01 * 1.0001) ** 0.5 - 1) * 100)
        self.assertEqual(
            (
                summary["ratio_pairs"],
                summary["improved"],
                summary["regressed"],
                summary["unchanged"],
            ),
            (2, 0, 2, 0),
        )
        reverse = benchmark.compare_runs(before, after)["summary"]["runtime_size"]
        self.assertAlmostEqual(
            (1 + summary["percent"] / 100) * (1 + reverse["percent"] / 100), 1
        )

    def test_summary_excludes_zero_and_incompatible_ratios(self):
        before = [
            self.fixture("zero", runtime_size=0),
            self.fixture("failed"),
            self.fixture("ok"),
        ]
        after = [
            self.fixture("zero", runtime_size=1),
            self.fixture("failed", status="failed"),
            self.fixture("ok"),
        ]
        summary = benchmark.compare_runs(after, before)["summary"]["runtime_size"]
        self.assertEqual(
            summary,
            {
                "paired": 2,
                "ratio_pairs": 1,
                "percent": 0.0,
                "improved": 0,
                "regressed": 1,
                "unchanged": 1,
            },
        )

    def test_summary_omits_empty_and_duplicate_details(self):
        comparison = benchmark.compare_runs([self.fixture()], [self.fixture()])
        markdown = benchmark.comparison_report(comparison)
        self.assertNotIn("Per-call gas changes", markdown)
        self.assertNotIn("Per-case metric changes", markdown)
        self.assertNotIn("inspect sample", markdown)
        self.assertEqual(markdown.count("| runtime bytes |"), 1)

    def test_solar_only_report_keeps_compile_times(self):
        before = self.fixture()
        after = self.fixture(compile_time_seconds=2)
        comparison = benchmark.compare_runs([after], [before])
        rows = {(row["suite"], row["test_id"]): row for row in comparison["rows"]}
        markdown = benchmark.codegen_report([after], [before], "baseline", rows)
        self.assertIn("<summary>Compilation time</summary>", markdown)
        self.assertIn("2.000 s", markdown)
        self.assertNotIn("sum of medians", markdown)

    def test_compile_only_does_not_report_missing_artifacts(self):
        case = self.fixture()
        case["contract_name"] = "*"
        comparison = benchmark.compare_runs([case], [case])
        markdown = benchmark.comparison_report(comparison)
        self.assertIn("1 compilation-only benchmarks", markdown)
        self.assertNotIn("artifacts unavailable", markdown)

    def test_call_deltas_survive_equal_total_gas(self):
        before = self.fixture()
        after = copy.deepcopy(before)
        after["compilers"]["solar"]["gas_results"][0]["gas"] = 15
        after["compilers"]["solar"]["gas_results"][1]["gas"] = 15
        row = benchmark.compare_runs([after], [before])["rows"][0]
        self.assertEqual(row["metrics"]["total_gas"]["delta"], 0)
        self.assertEqual([call["delta"] for call in row["gas_calls"]], [5, -5])
        self.assertTrue(
            benchmark.comparison_has_changes(
                benchmark.compare_runs([after], [before]), True
            )
        )

    def test_report_tables_use_comparison_eligibility(self):
        before = self.fixture()
        after = self.fixture(input_fingerprint="changed", runtime_size=90)
        comparison = benchmark.compare_runs([after], [before])
        rows = {("repository", "test"): comparison["rows"][0]}
        self.assertEqual(
            benchmark.benchmark_rows([after], benchmark.by_test_id([before]), rows),
            ["| test | 30 (n/a) | n/a (n/a) | 90B (n/a) | n/a (n/a) |"],
        )
        self.assertEqual(before["compilers"]["solar"]["runtime_size"], 100)

    def test_compile_noise_does_not_hide_codegen_changes(self):
        before = self.fixture()
        comparison = benchmark.compare_runs(
            [self.fixture(compile_time_seconds=0.11)], [before]
        )
        self.assertFalse(benchmark.comparison_has_changes(comparison, False))
        comparison = benchmark.compare_runs(
            [self.fixture(compile_time_seconds=0.2)], [before]
        )
        self.assertTrue(benchmark.comparison_has_changes(comparison, False))
        self.assertFalse(benchmark.comparison_has_changes(comparison, True))
        comparison = benchmark.compare_runs(
            [self.fixture(output_fingerprint="different")], [before]
        )
        self.assertTrue(benchmark.comparison_has_changes(comparison, True))

    def test_workload_and_profile_changes_disable_gas_deltas(self):
        before = self.fixture()
        for mutation in ("order", "profile", "value"):
            after = copy.deepcopy(before)
            if mutation == "order":
                after["compilers"]["solar"]["gas_results"].reverse()
            elif mutation == "profile":
                after["gas_profile"] = "smoke"
            else:
                after["compilers"]["solar"]["runtime_results"][0]["value"] = "2"
            with self.subTest(mutation=mutation):
                row = benchmark.compare_runs([after], [before])["rows"][0]
                self.assertIsNone(row["metrics"]["total_gas"]["delta"])
                self.assertEqual(row["gas_calls"], [])
                self.assertEqual(row["metrics"]["runtime_size"]["delta"], 0)

    def test_build_profile_changes_disable_time_and_memory_deltas(self):
        row = benchmark.compare_runs(
            [self.fixture(command="target/release/solar --standard-json")],
            [self.fixture()],
        )["rows"][0]
        self.assertIsNone(row["metrics"]["compile_time_seconds"]["delta"])
        self.assertIsNone(row["metrics"]["peak_rss_bytes"]["delta"])
        self.assertEqual(row["compile_samples_before"], [0.09, 0.1, 0.11])

    def test_duplicate_cases_are_rejected(self):
        with self.assertRaisesRegex(
            ValueError, "duplicate benchmark result: repository/test"
        ):
            benchmark.compare_runs([self.fixture(), self.fixture()], [])

    def test_artifact_diff_detects_same_size_bytecode_and_missing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = root / "before" / "test" / "solar"
            after = root / "after" / "test" / "solar"
            before.mkdir(parents=True)
            after.mkdir(parents=True)
            (before / "runtime.hex").write_text("6001\n")
            (after / "runtime.hex").write_text("6002\n")
            (before / "mir.mir").write_text("old mir\n")
            comparison = benchmark.compare_runs([self.fixture()], [self.fixture()])
            patch_path = root / "changes.patch"
            benchmark.compare_artifacts(
                comparison,
                root / "before",
                root / "after",
                ["bytecode", "mir"],
                patch_path,
            )
            self.assertEqual(
                [
                    (item["name"], item["status"])
                    for item in comparison["rows"][0]["artifacts"]
                ],
                [("mir.mir", "removed"), ("runtime.hex", "changed")],
            )
            self.assertEqual(
                patch_path.read_text(),
                "--- before/test/solar/mir.mir\n+++ /dev/null\n@@ -1 +0,0 @@\n-old mir\n--- before/test/solar/runtime.hex\n+++ after/test/solar/runtime.hex\n@@ -1 +1 @@\n-6001\n+6002\n",
            )

    def test_artifact_paths_cannot_escape_root(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            self.assertRaisesRegex(ValueError, "artifact path escapes its root"),
        ):
            benchmark.artifact_files(Path(directory), "../outside", "solar")

    def test_cli_writes_shared_ci_and_agent_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("before", "after"):
                (root / name).mkdir()
                (root / name / "results.json").write_text(
                    json.dumps(
                        {"results": [self.fixture()], "timings": {"repository": 1}}
                    )
                )
            outputs = {
                name: root / name
                for name in (
                    "report.md",
                    "comparison.json",
                    "common.json",
                    "comment",
                    "summary",
                    "github-output",
                )
            }
            with (
                patch.dict(
                    os.environ,
                    {
                        "GITHUB_SHA": "0" * 40,
                        "GITHUB_OUTPUT": str(outputs["github-output"]),
                        "GITHUB_STEP_SUMMARY": str(outputs["summary"]),
                    },
                    clear=True,
                ),
                patch("sys.stdout", new_callable=io.StringIO),
            ):
                code = benchmark.main(
                    [
                        str(root / "before"),
                        str(root / "after"),
                        "--report-output",
                        str(outputs["report.md"]),
                        "--json-output",
                        str(outputs["comparison.json"]),
                        "--common-output",
                        str(outputs["common.json"]),
                        "--comment-output",
                        str(outputs["comment"]),
                    ]
                )
            self.assertEqual(code, 0)
            self.assertEqual(
                outputs["summary"].read_text(), outputs["report.md"].read_text() + "\n"
            )
            self.assertEqual(outputs["comment"].read_text(), "false\n")
            self.assertEqual(
                json.loads(outputs["comparison.json"].read_text())["totals"][
                    "runtime_size"
                ]["delta"],
                0,
            )
            Draft202012Validator(SCHEMA).validate(
                json.loads(outputs["common.json"].read_text())
            )
