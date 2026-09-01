import importlib.util
import json
import sys
import tempfile
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
        self.assertEqual(len(benchmark.TEST_CASES), 25)
        repository_cases = [
            case for case in benchmark.TEST_CASES if case.suite == "repository"
        ]
        self.assertEqual(len(repository_cases), 9)
        heavy_cases = [case for case in benchmark.TEST_CASES if case.suite == "heavy"]
        self.assertEqual(len(heavy_cases), 9)
        for case in heavy_cases:
            self.assertTrue(case.whole_project)
            self.assertTrue(case.project_path.is_file())
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
                    case.project_file,
                    case.source,
                    case.contract_name,
                    case.settings_profile,
                )
            )
            self.assertIn(case.source, payload["sources"], case.test_id)
            self.assertEqual(
                len(payload["sources"]), expected_source_counts[case.test_id]
            )
            self.assertTrue(payload["settings"]["viaIR"])
            self.assertEqual(
                payload["settings"]["metadata"],
                {"appendCBOR": False, "bytecodeHash": "none"},
            )

    def test_runtime_projects_are_loaded_by_codspeed(self) -> None:
        criterion_sources = (
            benchmark.REPOSITORY_ROOT / "benches/src/lib.rs"
        ).read_text()
        runtime_projects = {
            case.project_file
            for case in benchmark.TEST_CASES
            if case.project_file is not None
        }
        missing = sorted(
            project_file
            for project_file in runtime_projects
            if f'"../testdata/projects/{project_file}"' not in criterion_sources
        )
        self.assertEqual(missing, [])

    def test_full_project_cases_preserve_archives(self) -> None:
        heavy_cases = [case for case in benchmark.TEST_CASES if case.suite == "heavy"]
        self.assertEqual(len(heavy_cases), 9)
        self.assertTrue(all(case.whole_project for case in heavy_cases))
        case = next(case for case in heavy_cases if case.project == "solady-0.1.26")
        archive = benchmark.load_project(case.project_path)
        payload = json.loads(
            benchmark.full_project_standard_json_input(case.project_file)
        )
        self.assertEqual(payload, archive)
        self.assertEqual(len(payload["sources"]), 208)

    def test_evm_version_override_replaces_project_pin(self) -> None:
        original = benchmark.full_project_standard_json_input("solady-0.1.26.json.gz")
        overridden = benchmark.with_evm_version(original, "amsterdam")

        self.assertEqual(json.loads(original)["settings"]["evmVersion"], "paris")
        self.assertEqual(json.loads(overridden)["settings"]["evmVersion"], "amsterdam")
        self.assertEqual(benchmark.with_evm_version(original, None), original)

    def test_compiler_output_fingerprint_ignores_diagnostic_order(self) -> None:
        first = json.dumps(
            {
                "contracts": {"A.sol": {"A": {}}},
                "errors": [{"message": "a"}, {"message": "b"}],
            }
        )
        second = json.dumps(
            {
                "errors": [{"message": "b"}, {"message": "a"}],
                "contracts": {"A.sol": {"A": {}}},
            }
        )

        self.assertEqual(
            benchmark.compiler_output_fingerprint(first),
            benchmark.compiler_output_fingerprint(second),
        )

    def test_select_tests(self) -> None:
        heavy = [case for case in benchmark.TEST_CASES if case.suite == "heavy"]
        micro = [case for case in benchmark.TEST_CASES if case.suite == "micro"]
        runtime = [case for case in benchmark.TEST_CASES if case.suite != "heavy"]

        self.assertEqual(benchmark.select_tests(("runtime",), "all"), runtime)
        self.assertEqual(benchmark.select_tests(("compile-time",), "all"), heavy)
        self.assertEqual(
            benchmark.select_tests(("runtime", "compile-time"), "all"),
            list(benchmark.TEST_CASES),
        )
        self.assertEqual(benchmark.select_tests(("runtime",), "micro"), micro)

    def test_parse_full_project_output_counts_contract_artifacts(self) -> None:
        case = next(case for case in benchmark.TEST_CASES if case.whole_project)
        stdout = json.dumps(
            {
                "contracts": {
                    "src/A.sol": {
                        "A": {"evm": {"bytecode": {"object": "6000"}}},
                        "Interface": {"evm": {"bytecode": {"object": ""}}},
                    }
                }
            }
        )
        self.assertEqual(
            benchmark.parse_full_project_output(stdout, case),
            (2, 1, ""),
        )

    def test_micro_sources_are_externalized(self) -> None:
        payload = json.loads(benchmark.standard_json_input(benchmark.TEST_CASES[0]))

        self.assertEqual(list(payload["sources"]), ["Factorial.sol"])
        self.assertTrue(
            payload["sources"]["Factorial.sol"]["content"].startswith(
                "\n// SPDX-License-Identifier: MIT"
            )
        )
        self.assertTrue(payload["settings"]["viaIR"])
        self.assertEqual(
            payload["settings"]["metadata"],
            {"appendCBOR": False, "bytecodeHash": "none"},
        )

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


class FailureHandlingTests(unittest.TestCase):
    def test_process_spawn_error_is_a_command_failure(self) -> None:
        with mock.patch(
            "common.subprocess.Popen",
            side_effect=OSError(7, "Argument list too long"),
        ):
            result = benchmark.run(["cast", "send"])

        self.assertEqual(result.returncode, -1)
        self.assertEqual(result.stdout, "")
        self.assertIn("Argument list too long", result.stderr)

    def test_unexpected_test_error_is_written_as_a_failure(self) -> None:
        test_id = benchmark.TEST_CASES[0].test_id
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark,
                "find_binary",
                side_effect=lambda value, _fallbacks: Path(value),
            ),
            mock.patch.object(
                benchmark,
                "binary_version",
                return_value=("0.8.36", ""),
            ),
            mock.patch.object(
                benchmark,
                "run_test_case",
                side_effect=RuntimeError("unexpected"),
            ),
        ):
            output = Path(directory) / "results.json"
            return_code = benchmark.main(
                [
                    "--solc",
                    "solc",
                    "--solar",
                    "solar",
                    "--tests",
                    test_id,
                    "--allow-failures",
                    "--output",
                    str(output),
                ]
            )
            document = json.loads(output.read_text())

        self.assertEqual(return_code, 0)
        self.assertEqual(len(document["results"]), 1)
        failure = document["results"][0]
        self.assertIn("RuntimeError: unexpected", failure["benchmark_error"])
        self.assertEqual(
            {compiler["status"] for compiler in failure["compilers"].values()},
            {"failed"},
        )


class RuntimeComparisonTests(unittest.TestCase):
    def test_single_compiler_is_not_a_semantic_oracle(self) -> None:
        specs = (benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),)
        entry = {
            "compilers": {
                "solar": {
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "1"}
                    ],
                }
            }
        }

        benchmark.compare_runtime_results(entry, specs)

        self.assertEqual(entry["runtime_status"], "skipped")
        self.assertEqual(entry["runtime_mismatches"], [])

    def test_single_compiler_runtime_failure_still_fails(self) -> None:
        specs = (benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),)
        entry = {
            "compilers": {
                "solar": {
                    "runtime_status": "failed",
                    "runtime_results": [
                        {"label": "value", "status": "failed", "error": "reverted"}
                    ],
                }
            }
        }

        benchmark.compare_runtime_results(entry, specs)

        self.assertEqual(entry["runtime_status"], "failed")
        self.assertEqual(entry["runtime_mismatches"], [])

    def test_merges_matching_reference_compiler_results(self) -> None:
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "compilers": {
                "solar": {
                    "input_fingerprint": "input",
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "1"}
                    ],
                }
            },
        }
        references = {
            ("runtime", "test"): {
                "compilers": {
                    "solc": {
                        "input_fingerprint": "input",
                        "runtime_status": "ok",
                        "runtime_results": [
                            {"label": "value", "status": "ok", "value": "1"}
                        ],
                    }
                }
            }
        }

        merged = benchmark.merge_reference_compiler(entry, references, "solc")
        specs = (
            benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc"),
            benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),
        )
        benchmark.compare_runtime_results(entry, specs)

        self.assertTrue(merged)
        self.assertEqual(entry["runtime_status"], "ok")
        self.assertEqual(list(entry["compilers"]), ["solc", "solar"])

    def test_rejects_reference_results_for_different_inputs(self) -> None:
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "compilers": {"solar": {"input_fingerprint": "new"}},
        }
        references = {
            ("runtime", "test"): {"compilers": {"solc": {"input_fingerprint": "old"}}}
        }

        merged = benchmark.merge_reference_compiler(entry, references, "solc")

        self.assertFalse(merged)
        self.assertNotIn("solc", entry["compilers"])

    def test_rejects_reference_results_for_different_workloads(self) -> None:
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "gas_profile": "hot",
            "compilers": {
                "solar": {
                    "input_fingerprint": "input",
                    "gas_results": [{"label": "new", "call": "new()"}],
                }
            },
        }
        references = {
            ("runtime", "test"): {
                "gas_profile": "hot",
                "compilers": {
                    "solc": {
                        "input_fingerprint": "input",
                        "gas_results": [{"label": "old", "call": "old()"}],
                    }
                },
            }
        }

        merged = benchmark.merge_reference_compiler(entry, references, "solc")

        self.assertFalse(merged)
        self.assertNotIn("solc", entry["compilers"])

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
    def test_streams_large_params_through_file(self) -> None:
        bytecode = "ab" * 100_000
        process = mock.Mock(returncode=0, stdout='"0x1234"\n', stderr="")
        observed = {}

        def run_with_file(command, **kwargs):
            observed["path"] = kwargs["input_path"]
            observed["params"] = kwargs["input_path"].read_text()
            return process

        with mock.patch.object(benchmark, "run", side_effect=run_with_file) as run:
            value, error = benchmark.rpc_request_from_file(
                "eth_sendTransaction",
                ({"data": "0x" + bytecode},),
                "http://127.0.0.1:8545",
            )

        self.assertEqual(value, "0x1234")
        self.assertEqual(error, "")
        command = run.call_args.args[0]
        self.assertTrue(all(bytecode not in argument for argument in command))
        self.assertEqual(
            observed["params"],
            json.dumps([{"data": "0x" + bytecode}]),
        )
        self.assertFalse(observed["path"].exists())

    def test_deploys_creation_code_from_file(self) -> None:
        bytecode = "ab"
        receipt = {
            "status": "0x1",
            "gasUsed": "0x5208",
            "contractAddress": "0x1234",
        }

        with (
            mock.patch.object(
                benchmark,
                "rpc_request_from_file",
                return_value=("0xtx", ""),
            ) as send,
            mock.patch.object(
                benchmark,
                "rpc_request",
                return_value=(receipt, ""),
            ) as request,
        ):
            address, gas, error = benchmark.deploy_creation_code(
                bytecode,
                (),
                None,
                "http://127.0.0.1:8545",
                benchmark.DEFAULT_PRIVATE_KEY,
            )

        self.assertEqual((address, gas, error), ("0x1234", 21000, ""))
        transaction = send.call_args.args[1][0]
        self.assertEqual(transaction["from"], benchmark.DEFAULT_SENDER)
        self.assertEqual(transaction["data"], "0x" + bytecode)
        self.assertEqual(transaction["gas"], hex(int(benchmark.CAST_GAS_LIMIT)))
        request.assert_called_once_with(
            "eth_getTransactionReceipt",
            ("0xtx",),
            "http://127.0.0.1:8545",
        )


if __name__ == "__main__":
    unittest.main()
