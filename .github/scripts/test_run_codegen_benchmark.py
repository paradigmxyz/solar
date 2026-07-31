import importlib.util
import json
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("run_codegen_benchmark.py")
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("codegen_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class CorpusTests(unittest.TestCase):
    def test_vendored_cases_and_helpers_exist(self) -> None:
        self.assertEqual(len(benchmark.TEST_CASES), 4)
        self.assertEqual(len(benchmark.REPO_TEST_CASES), 9)
        self.assertTrue(benchmark.RUNTIME_FIXTURES.is_file())
        for case in benchmark.REPO_TEST_CASES:
            self.assertTrue(case.source_path.is_file(), case.test_id)

    def test_micro_sources_are_externalized(self) -> None:
        payload = json.loads(benchmark.standard_json_input(benchmark.TEST_CASES[0]))

        self.assertEqual(list(payload["sources"]), ["factorial.sol"])
        self.assertTrue(
            payload["sources"]["factorial.sol"]["content"].startswith(
                "\n// SPDX-License-Identifier: MIT"
            )
        )
        self.assertTrue(payload["settings"]["viaIR"])

    def test_filters_incompatible_solc_versions(self) -> None:
        uniswap = benchmark.REPO_TEST_CASES[0]

        self.assertTrue(
            benchmark.version_in_range("0.5.16", uniswap.min_solc, uniswap.max_solc)
        )
        self.assertFalse(
            benchmark.version_in_range("0.8.36", uniswap.min_solc, uniswap.max_solc)
        )


class RuntimeComparisonTests(unittest.TestCase):
    def test_reports_cross_compiler_mismatch(self) -> None:
        specs = (
            benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc"),
            benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),
        )
        entry = {
            "compilers": {
                "solc": {
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "1"}
                    ],
                },
                "solar": {
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "2"}
                    ],
                },
            }
        }

        benchmark.compare_runtime_results(entry, specs)

        self.assertEqual(entry["runtime_status"], "mismatch")
        self.assertEqual(
            entry["runtime_mismatches"],
            [{"label": "value", "values": {"solc": "1", "solar": "2"}}],
        )


if __name__ == "__main__":
    unittest.main()
