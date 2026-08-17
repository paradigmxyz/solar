import importlib.util
import json
import sys
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("benchmark.py")
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("codegen_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class CorpusTests(unittest.TestCase):
    def test_vendored_cases_and_projects_exist(self) -> None:
        self.assertEqual(len(benchmark.TEST_CASES), 16)
        repository_cases = [
            case for case in benchmark.TEST_CASES if case.suite == "repository"
        ]
        self.assertEqual(len(repository_cases), 9)
        self.assertTrue(benchmark.RUNTIME_FIXTURES.is_file())
        expected_source_counts = {
            "uniswap-v2-pair": 10,
            "openzeppelin-erc20-mock": 6,
            "openzeppelin-vesting-wallet": 12,
            "nitro-one-step-proof": 22,
            "aave-l2-encoder": 6,
            "lilweb3-ens": 1,
            "lilweb3-flashloan": 2,
            "lilweb3-fractional": 3,
            "maple-erc20": 2,
        }
        for case in repository_cases:
            self.assertTrue(case.project_path.is_file(), case.test_id)
            payload = json.loads(
                benchmark.project_standard_json_input(
                    case.project_file, case.source, case.contract_name
                )
            )
            self.assertIn(case.source, payload["sources"], case.test_id)
            self.assertEqual(
                len(payload["sources"]), expected_source_counts[case.test_id]
            )
            self.assertTrue(payload["settings"]["viaIR"])

    def test_micro_sources_are_externalized(self) -> None:
        payload = json.loads(benchmark.standard_json_input(benchmark.TEST_CASES[0]))

        self.assertEqual(list(payload["sources"]), ["Factorial.sol"])
        self.assertTrue(
            payload["sources"]["Factorial.sol"]["content"].startswith(
                "\n// SPDX-License-Identifier: MIT"
            )
        )
        self.assertTrue(payload["settings"]["viaIR"])

    def test_filters_incompatible_solc_versions(self) -> None:
        uniswap = next(
            case for case in benchmark.TEST_CASES if case.test_id == "uniswap-v2-pair"
        )

        self.assertTrue(
            benchmark.version_in_range("0.5.16", uniswap.min_solc, uniswap.max_solc)
        )
        self.assertFalse(
            benchmark.version_in_range("0.8.36", uniswap.min_solc, uniswap.max_solc)
        )

    def test_large_cases_resolve_from_pinned_projects(self) -> None:
        large_cases = [case for case in benchmark.TEST_CASES if case.suite == "large"]
        self.assertEqual(len(large_cases), 3)
        expected_source_counts = {
            "openzeppelin-governor": 48,
            "solady-signature-checker": 12,
            "solady-lib-string": 9,
        }
        for case in large_cases:
            project = benchmark.load_project(case.project_path)
            selected = benchmark.project_slice(project, case.source)
            self.assertEqual(len(selected), expected_source_counts[case.test_id])

    def test_project_slice_resolves_relative_and_remapped_imports(self) -> None:
        project = {
            "sources": {
                "src/A.sol": {
                    "content": 'import "./B.sol";\nimport {C} from "@pkg/C.sol";\n'
                },
                "src/B.sol": {"content": 'import "../shared/D.sol";'},
                "vendor/pkg/C.sol": {"content": ""},
                "shared/D.sol": {"content": ""},
                "unused.sol": {"content": ""},
            },
            "settings": {"remappings": ["@pkg/=vendor/pkg/"]},
        }

        self.assertEqual(
            list(benchmark.project_slice(project, "src/A.sol")),
            ["shared/D.sol", "src/A.sol", "src/B.sol", "vendor/pkg/C.sol"],
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


class RpcTransportTests(unittest.TestCase):
    def test_streams_large_params_through_stdin(self) -> None:
        bytecode = "ab" * 100_000
        process = mock.Mock(returncode=0, stdout='"0x1234"\n', stderr="")

        with mock.patch.object(benchmark, "run", return_value=process) as run:
            value, error = benchmark.rpc_request(
                "eth_sendTransaction",
                ({"data": "0x" + bytecode},),
                "http://127.0.0.1:8545",
            )

        self.assertEqual(value, "0x1234")
        self.assertEqual(error, "")
        command = run.call_args.args[0]
        self.assertTrue(all(bytecode not in argument for argument in command))
        self.assertEqual(
            run.call_args.kwargs["input_text"],
            json.dumps([{"data": "0x" + bytecode}]),
        )


if __name__ == "__main__":
    unittest.main()
