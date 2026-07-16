import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from jsonschema import Draft202012Validator

sys.path.insert(0, str(Path(__file__).parent))
import check_codegen_benchmark as benchmark


SCHEMA = json.loads(
    (Path(__file__).resolve().parents[2] / "benches/schema/benchmark-result-v1.schema.json").read_text()
)
DEFAULT_TIMING = object()


def result(**compiler):
    return {"test_id": "test", "compilers": {"solar": compiler}}


class ReportFormattingTests(unittest.TestCase):
    def test_unchanged_report_has_note(self):
        report = benchmark.format_report("## Results", False)
        self.assertEqual(
            report,
            "> [!NOTE]\n"
            "> Codegen benchmark output is unchanged from `main`.\n\n"
            "## Results",
        )

    def test_changed_report_has_no_note(self):
        self.assertEqual(benchmark.format_report("## Results", True), "## Results")

    def test_size_delta_uses_conventional_sign(self):
        self.assertEqual(
            benchmark.fmt_value_with_size_delta(95, 95, 100, "B"),
            "95B (✅ -5.00%)",
        )
        self.assertEqual(
            benchmark.fmt_value_with_size_delta(105, 105, 100, "B"),
            "105B (❌ +5.00%)",
        )


class CommonBenchmarkResultTests(unittest.TestCase):
    def write_result(self, micro, repo=None, micro_timing=DEFAULT_TIMING, repo_timing=None):
        if repo is None:
            repo = []
        if micro_timing is DEFAULT_TIMING:
            micro_timing = {"wall_time_seconds": 1.25}
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
            benchmark.write_common_result(
                output,
                micro,
                repo,
                micro_timing,
                repo_timing,
            )
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
            ),
            result(
                status="ok",
                total_gas=1,
                deploy_gas=2,
                bytecode_size=3,
                runtime_size=4,
            ),
        ]
        document = self.write_result(micro)
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
        document = self.write_result(compilation_failure)
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
                document = self.write_result([result(**complete), result(**incomplete)])
                self.assertNotIn(metric_name, document["benchmarks"][0][group])

    def test_omits_suite_without_timing(self):
        results = [result(status="ok", bytecode_size=1, runtime_size=1)]
        document = self.write_result(
            results,
            results,
            micro_timing=None,
            repo_timing={"wall_time_seconds": 2.0},
        )
        self.assertEqual(
            [entry["name"] for entry in document["benchmarks"]],
            ["codegen_runtime_suite/repo"],
        )


if __name__ == "__main__":
    unittest.main()
