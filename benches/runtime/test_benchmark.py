import hashlib
import importlib.util
import io
import json
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).with_name("benchmark.py")
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("codegen_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)

from cases import GasCall


class CorpusTests(unittest.TestCase):
    def test_fair_solady_calls_include_memory_stress_in_comparison(self):
        case = next(
            case for case in benchmark.TEST_CASES if case.test_id == "solady-lib-string"
        )
        calls = benchmark.gas_calls(case, "hot")
        self.assertTrue(case.fair_memory)
        self.assertEqual(
            [call for call in calls if call.comparison_exclusion_reason], []
        )
        signatures = {
            "testBytesToHexStringNoPrefix(bytes)",
            "testBytesToHexString(bytes)",
            "testStringIs7BitASCIIDifferential(bytes)",
        }
        stressed = [call for call in calls if call.signature in signatures]
        self.assertEqual(len(stressed), 22)
        self.assertEqual({call.signature for call in stressed}, signatures)

    def test_fair_solady_patch_preserves_archive_and_assertions(self):
        case = next(
            case for case in benchmark.TEST_CASES if case.test_id == "solady-lib-string"
        )
        archive = benchmark.load_project(case.project_path)
        original = json.dumps(archive, sort_keys=True)
        with mock.patch.object(benchmark, "load_project", return_value=archive):
            patched, _, fingerprint = benchmark.compiler_input(case, "paris")
            repeated, _, repeated_fingerprint = benchmark.compiler_input(case, "paris")
            unpatched, _, unpatched_fingerprint = benchmark.compiler_input(
                replace(case, fair_memory=False), "paris"
            )
        self.assertEqual(json.dumps(archive, sort_keys=True), original)
        self.assertEqual(patched, repeated)
        self.assertEqual(fingerprint, repeated_fingerprint)
        self.assertNotEqual(fingerprint, unpatched_fingerprint)
        before = json.loads(unpatched)
        after = json.loads(patched)
        self.assertEqual(before["settings"], after["settings"])
        self.assertEqual(before["sources"].keys(), after["sources"].keys())
        for name, source in before["sources"].items():
            if name != benchmark.SOLADY_BRUTALIZER_SOURCE:
                self.assertEqual(source, after["sources"][name], name)
        before_helper = before["sources"][benchmark.SOLADY_BRUTALIZER_SOURCE]["content"]
        after_helper = after["sources"][benchmark.SOLADY_BRUTALIZER_SOURCE]["content"]
        old_body = benchmark.brutalizer_function(before_helper)
        new_body = benchmark.brutalizer_function(after_helper)
        self.assertEqual(
            before_helper.replace(old_body, ""), after_helper.replace(new_body, "")
        )
        self.assertIn("gas()", old_body)
        self.assertIn("codesize()", old_body)
        for opcode in ("gas()", "codesize()", "codecopy("):
            self.assertNotIn(opcode, new_body)
        self.assertIn("lt(i, 0x200)", new_body)
        self.assertIn("mload(0x40)", new_body)
        self.assertIn("mstore(add(free, i), xor(pattern, i))", new_body)

    def test_fair_solady_patch_rejects_changed_upstream_helper(self):
        archive = benchmark.load_project(
            benchmark.PROJECTS_ROOT / "solady-0.1.26.json.gz"
        )
        source = archive["sources"][benchmark.SOLADY_BRUTALIZER_SOURCE]
        source["content"] = source["content"].replace(
            "add(caller(), gas())", "gas()", 1
        )
        with self.assertRaisesRegex(
            ValueError, "pinned Solady memory brutalizer changed"
        ):
            benchmark.with_fair_solady_memory(archive)

    def test_fair_solady_patch_is_scoped_to_lib_string_runtime(self):
        patched = [case.test_id for case in benchmark.TEST_CASES if case.fair_memory]
        self.assertEqual(patched, ["solady-lib-string"])
        case = next(
            case
            for case in benchmark.TEST_CASES
            if case.test_id == "solady-signature-checker"
        )
        payload, _, _ = benchmark.compiler_input(case, None)
        original = benchmark.load_project(case.project_path)
        self.assertEqual(
            json.loads(payload)["sources"][benchmark.SOLADY_BRUTALIZER_SOURCE],
            original["sources"][benchmark.SOLADY_BRUTALIZER_SOURCE],
        )

    def test_fair_solady_source_links_include_patch_fixture(self):
        case = next(
            case for case in benchmark.TEST_CASES if case.test_id == "solady-lib-string"
        )
        with mock.patch.object(benchmark, "source_revision", return_value="a" * 40):
            self.assertEqual(
                [link["label"] for link in benchmark.source_links(case)],
                [
                    "testdata/projects/solady-0.1.26.json.gz",
                    "testdata/runtime/FairBrutalizer.sol",
                    "Vectorized/solady",
                ],
            )

    def test_source_links_pin_checkout_and_upstream(self) -> None:
        case = next(
            case
            for case in benchmark.TEST_CASES
            if case.test_id == "forge-std-1.16.1-project"
        )
        with mock.patch.object(benchmark, "source_revision", return_value="a" * 40):
            links = benchmark.source_links(case)
            failed = benchmark.failed_test_result(case, [], "hot", ValueError("test"))
        self.assertEqual(
            links,
            [
                {
                    "label": "testdata/projects/forge-std-1.16.1.json.gz",
                    "url": "https://github.com/paradigmxyz/solar/blob/"
                    + "a" * 40
                    + "/testdata/projects/forge-std-1.16.1.json.gz",
                },
                {
                    "label": "foundry-rs/forge-std",
                    "url": "https://github.com/foundry-rs/forge-std/tree/"
                    "620536fa5277db4e3fd46772d5cbc1ea0696fb43",
                },
            ],
        )
        self.assertEqual(failed["source_links"], links)

    def test_source_metadata_covers_corpus(self) -> None:
        for case in benchmark.TEST_CASES:
            with self.subTest(case=case.test_id):
                if case.source_code is not None:
                    self.assertTrue(case.source_path)
                    self.assertEqual(
                        (benchmark.REPOSITORY_ROOT / case.source_path).read_text(),
                        case.source_code,
                    )
                if case.project_file:
                    origins = case.project.sources
                    self.assertTrue(origins)
                    for source in origins:
                        self.assertRegex(source.commit, r"^[0-9a-f]{40}$")
        wrapper = next(
            case for case in benchmark.TEST_CASES if case.test_id == "solady-encoding"
        )
        with mock.patch.object(benchmark, "source_revision", return_value="a" * 40):
            self.assertEqual(
                [link["label"] for link in benchmark.source_links(wrapper)],
                [
                    "testdata/projects/solady-0.1.26.json.gz",
                    "testdata/runtime/Encoding.sol",
                    "Vectorized/solady",
                ],
            )

    def test_vendored_cases_and_projects_exist(self) -> None:
        self.assertEqual(len(benchmark.TEST_CASES), 38)
        repository_cases = [
            case for case in benchmark.TEST_CASES if case.suite == "repository"
        ]
        self.assertEqual(len(repository_cases), 12)
        heavy_cases = [case for case in benchmark.TEST_CASES if case.suite == "heavy"]
        self.assertEqual(len(heavy_cases), 10)
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
            "solady-encoding": 3,
            "solady-algorithms": 5,
            "tempo-zones-portal": 23,
        }
        for case in repository_cases:
            self.assertTrue(case.project_path.is_file(), case.test_id)
            payload = json.loads(
                benchmark.project_standard_json_input(
                    case.project_file,
                    case.source,
                    case.contract_name,
                    case.settings_profile,
                    case.source_code,
                )
            )
            self.assertIn(case.source, payload["sources"], case.test_id)
            self.assertEqual(
                len(payload["sources"]), expected_source_counts[case.test_id]
            )
            self.assertTrue(payload["settings"]["viaIR"])
            self.assertEqual(
                payload["settings"]["metadata"],
                benchmark.load_project(case.project_path)["settings"]["metadata"]
                if case.test_id == "tempo-zones-portal"
                else {"appendCBOR": False, "bytecodeHash": "none"},
            )

    def test_zones_fixture_rejects_remote_rpc_before_mutation(self) -> None:
        with mock.patch.object(benchmark, "rpc_request") as rpc:
            address, gas, error = benchmark.prepare_zones_portal(
                "0x" + "11" * 20,
                "https://example.com",
                benchmark.DEFAULT_PRIVATE_KEY,
            )
        self.assertIsNone(address)
        self.assertIsNone(gas)
        self.assertEqual(error, "Zones fixtures require a local Anvil RPC")
        rpc.assert_not_called()

    def test_zones_checks_reject_matching_but_incorrect_state(self) -> None:
        case = next(
            case
            for case in benchmark.TEST_CASES
            if case.test_id == "tempo-zones-portal"
        )
        with mock.patch.object(
            benchmark, "read_uint", side_effect=((0, ""), (0, ""), (100_000_000, ""))
        ):
            checks = benchmark.run_cold_path_checks(
                case,
                "0x" + "11" * 20,
                "solc",
                "http://localhost:8545",
                benchmark.DEFAULT_PRIVATE_KEY,
            )
        self.assertEqual([check["status"] for check in checks], ["failed"] * 3)
        self.assertEqual(checks[0]["error"], "expected 2, got 0")

    def test_zones_mock_bytecode_matches_its_source_revision(self) -> None:
        fixtures = json.loads(
            (benchmark.RUNTIME_FIXTURES.parent / "Zones.json").read_text()
        )
        self.assertEqual(
            fixtures["source_sha256"],
            hashlib.sha256(benchmark.RUNTIME_FIXTURES.read_bytes()).hexdigest(),
        )
        self.assertEqual(
            set(fixtures["creation_bytecode"]),
            {"RuntimeZoneToken", "RuntimeZoneRegistry"},
        )
        for bytecode in fixtures["creation_bytecode"].values():
            self.assertTrue(bytes.fromhex(bytecode))

    def test_benchmark_wrapper_preserves_pinned_sources(self) -> None:
        for test_id, count in (("solady-encoding", 153), ("solady-algorithms", 85)):
            with self.subTest(test_id=test_id):
                case = next(
                    case for case in benchmark.TEST_CASES if case.test_id == test_id
                )
                project = benchmark.load_project(case.project_path)
                self.assertNotIn(case.source, project["sources"])
                payload = json.loads(benchmark.compiler_input(case, None)[0])
                self.assertEqual(
                    payload["sources"][case.source]["content"], case.source_code
                )
                self.assertIn("src/utils/LibString.sol", payload["sources"])
                self.assertNotIn("test/LibString.t.sol", payload["sources"])
                for source, contents in payload["sources"].items():
                    if source != case.source:
                        self.assertEqual(contents, project["sources"][source])
                self.assertNotIn(
                    case.source, benchmark.load_project(case.project_path)["sources"]
                )
                self.assertEqual(len(case.gas_calls), count)
                self.assertEqual(len(case.runtime_checks), count)
                labels = [call.label for call in case.gas_calls]
                self.assertEqual(len(set(labels)), count)
                self.assertEqual(labels, [call.label for call in case.runtime_checks])

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
        self.assertEqual(len(heavy_cases), 10)
        self.assertTrue(all(case.whole_project for case in heavy_cases))
        case = next(
            case for case in heavy_cases if case.project.name == "solady-0.1.26"
        )
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

    def test_optimizer_runs_override_replaces_project_setting(self) -> None:
        original = benchmark.full_project_standard_json_input("solady-0.1.26.json.gz")
        overridden = benchmark.with_optimizer_runs(original, 1)

        self.assertEqual(json.loads(original)["settings"]["optimizer"]["runs"], 1000)
        self.assertEqual(
            json.loads(overridden)["settings"]["optimizer"],
            {"enabled": True, "runs": 1},
        )
        self.assertEqual(benchmark.with_optimizer_runs(original, None), original)

    def test_each_compiler_compiles_its_own_input(self) -> None:
        case = benchmark.TEST_CASES[0]
        specs = (
            benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc"),
            benchmark.CompilerSpec(
                "solc-legacy", "solc", Path("solc"), "solc-legacy", via_ir=False
            ),
            benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),
            benchmark.CompilerSpec(
                "solar-size", "solar", Path("solar"), "solar", optimizer_runs=1
            ),
        )
        inputs = {}
        shared = benchmark.compiler_input(case, None, 200)
        self.assertIs(benchmark.spec_compiler_input(shared, specs[0]), shared)

        def compile_case(spec, _case, prepared_input, *_args):
            inputs[spec.compiler_id] = json.loads(prepared_input[0])["settings"]
            return {"status": "failed", "input_fingerprint": prepared_input[2]}

        with mock.patch.object(benchmark, "compile_case", side_effect=compile_case):
            result = benchmark.run_test_case(
                case, specs, False, "smoke", "rpc", "key", optimizer_runs=200
            )

        self.assertEqual(
            {name: (s["viaIR"], s["optimizer"]["runs"]) for name, s in inputs.items()},
            {
                "solc": (True, 200),
                "solc-legacy": (False, 200),
                "solar": (True, 200),
                "solar-size": (True, 1),
            },
        )
        fingerprints = {
            name: data["input_fingerprint"]
            for name, data in result["compilers"].items()
        }
        self.assertEqual(fingerprints["solc"], fingerprints["solar"])
        self.assertEqual(len(set(fingerprints.values())), 3)

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
        self.assertEqual(len(large_cases), 4)
        expected_source_counts = {
            "openzeppelin-governor": 48,
            "openzeppelin-p256": 6,
            "solady-signature-checker": 12,
            "solady-lib-string": 9,
        }
        for case in large_cases:
            payload, _, _ = benchmark.compiler_input(case, None)
            selected = json.loads(payload)["sources"]
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
        for flags, compilers in (
            ([], {"solar"}),
            (["--solc", "solc"], {"solar", "solc"}),
            (["--solx", "solx"], {"solar", "solx"}),
            (["--oksolc", "oksolc"], {"solar", "oksolc"}),
            (["--solc", "solc", "--solx", "solx"], {"solar", "solc", "solx"}),
            (
                ["--solc", "solc", "--solc-legacy"],
                {"solar", "solc", "solc-legacy"},
            ),
            (["--solar-size"], {"solar", "solar-size"}),
        ):
            with self.subTest(flags=flags):
                self.check_unexpected_test_error(flags, compilers)

    def test_saved_reference_results_do_not_discover_or_run_reference_compilers(
        self,
    ) -> None:
        case = benchmark.TEST_CASES[0]
        shared = benchmark.compiler_input(case, None)
        legacy = benchmark.CompilerSpec(
            "solc-legacy", "solc", Path("solc"), "solc-legacy", via_ir=False
        )
        entry = {
            "test_id": case.test_id,
            "suite": case.suite,
            "gas_profile": "smoke",
            "compilers": {"solar": {"status": "ok", "input_fingerprint": shared[2]}},
        }
        reference = {
            **entry,
            "compilers": {
                name: {"status": "ok", "input_fingerprint": fingerprint}
                for name, fingerprint in (
                    ("solc", shared[2]),
                    ("solc-legacy", benchmark.spec_compiler_input(shared, legacy)[2]),
                    ("solx", shared[2]),
                    ("oksolc", shared[2]),
                )
            },
        }
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark, "find_binary", return_value=Path("solar")
            ) as find_binary,
            mock.patch.object(
                benchmark, "binary_version", return_value=("0.2.0", "")
            ) as binary_version,
            mock.patch.object(
                benchmark, "run_test_case", return_value=entry
            ) as run_case,
        ):
            reference_path = Path(directory) / "reference.json"
            reference_path.write_text(json.dumps({"results": [reference]}))
            output = Path(directory) / "results.json"
            return_code = benchmark.main(
                [
                    "--solar",
                    "solar",
                    "--reference-results",
                    str(reference_path),
                    "--tests",
                    case.test_id,
                    "--output",
                    str(output),
                ]
            )
            document = json.loads(output.read_text())
        self.assertEqual(return_code, 0)
        self.assertEqual(
            [call.args[0] for call in find_binary.call_args_list], ["solar"]
        )
        binary_version.assert_called_once_with(Path("solar"))
        self.assertEqual(
            [spec.compiler_id for spec in run_case.call_args.args[1]], ["solar"]
        )
        self.assertEqual(
            set(document["results"][0]["compilers"]),
            {"solar", "solc", "solc-legacy", "solx", "oksolc"},
        )

    def test_failed_case_does_not_rebuild_input_for_saved_references(self) -> None:
        case = benchmark.TEST_CASES[0]
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(benchmark, "find_binary", return_value=Path("solar")),
            mock.patch.object(benchmark, "binary_version", return_value=("0.2.0", "")),
            mock.patch.object(
                benchmark, "run_test_case", side_effect=RuntimeError("unexpected")
            ),
            mock.patch.object(benchmark, "prepare_input", side_effect=KeyError("boom")),
        ):
            reference_path = Path(directory) / "reference.json"
            reference_path.write_text(json.dumps({"results": []}))
            output = Path(directory) / "results.json"
            return_code = benchmark.main(
                [
                    "--solar",
                    "solar",
                    "--reference-results",
                    str(reference_path),
                    "--tests",
                    case.test_id,
                    "--allow-failures",
                    "--output",
                    str(output),
                ]
            )
            document = json.loads(output.read_text())
        self.assertEqual(return_code, 0)
        self.assertIn("unexpected", document["results"][0]["benchmark_error"])

    def test_live_oksolc_with_saved_references(self) -> None:
        case = benchmark.TEST_CASES[0]
        for jobs in (1, 8):
            with (
                self.subTest(jobs=jobs),
                tempfile.TemporaryDirectory() as directory,
                mock.patch.object(
                    benchmark, "find_binary", side_effect=lambda path, _: Path(path)
                ),
                mock.patch.object(
                    benchmark, "binary_version", return_value=("test", "")
                ),
                mock.patch.object(
                    benchmark,
                    "run",
                    return_value=mock.Mock(
                        returncode=1,
                        stdout="",
                        stderr="InternalFailure",
                        peak_rss_bytes=None,
                    ),
                ) as run,
            ):
                root = Path(directory)
                references = root / "reference.json"
                references.write_text(
                    json.dumps(
                        {
                            "results": [
                                {
                                    "test_id": case.test_id,
                                    "suite": case.suite,
                                    "gas_profile": "smoke",
                                    "compilers": {
                                        "oksolc": {
                                            "status": "ok",
                                            "label": "cached",
                                            "input_fingerprint": benchmark.compiler_input(
                                                case, None
                                            )[2],
                                        }
                                    },
                                }
                            ]
                        }
                    )
                )
                output = root / "results.json"
                self.assertEqual(
                    benchmark.main(
                        [
                            "--solar",
                            "solar",
                            "--oksolc",
                            "oksolc",
                            "--oksolc-jobs",
                            str(jobs),
                            "--reference-results",
                            str(references),
                            "--tests",
                            case.test_id,
                            "--allow-failures",
                            "--artifacts",
                            str(root / "artifacts"),
                            "--output",
                            str(output),
                        ]
                    ),
                    0,
                )
                commands = [call.args[0] for call in run.call_args_list]
                expected = [
                    "oksolc",
                    "standard-json",
                    "--no-cache",
                    "--parallel",
                    "--jobs",
                    str(jobs),
                    "-",
                ]
                self.assertEqual(
                    [cmd for cmd in commands if cmd[0] == "oksolc"],
                    [expected, expected],
                )
                compiler = json.loads(output.read_text())["results"][0]["compilers"][
                    "oksolc"
                ]
                self.assertEqual(compiler["status"], "failed")
                self.assertEqual(compiler["error"], "InternalFailure")
                self.assertEqual(compiler["label"], "oksolc test")

    def test_saved_reference_results_preserve_solc_version_filter(self) -> None:
        case = next(
            case for case in benchmark.TEST_CASES if case.test_id == "uniswap-v2-pair"
        )
        for version, flags, expected_calls in (
            ("0.8.37", [], 0),
            ("0.5.16", [], 1),
            ("0.8.37", ["--include-incompatible"], 1),
        ):
            with (
                self.subTest(version=version, flags=flags),
                tempfile.TemporaryDirectory() as directory,
                mock.patch.object(benchmark, "select_tests", return_value=[case]),
                mock.patch.object(benchmark, "find_binary", return_value=Path("solar")),
                mock.patch.object(
                    benchmark, "binary_version", return_value=("0.2.0", "")
                ),
                mock.patch.object(
                    benchmark,
                    "run_test_case",
                    return_value={
                        "test_id": case.test_id,
                        "suite": case.suite,
                        "compilers": {"solar": {"status": "ok"}},
                    },
                ) as run_case,
            ):
                reference_path = Path(directory) / "reference.json"
                reference_path.write_text(
                    json.dumps(
                        {
                            "results": [
                                {
                                    "test_id": "another-case",
                                    "compilers": {"solc": {"label": f"solc {version}"}},
                                }
                            ]
                        }
                    )
                )
                self.assertEqual(
                    benchmark.main(
                        [
                            "--solar",
                            "solar",
                            "--reference-results",
                            str(reference_path),
                            "--output",
                            str(Path(directory) / "results.json"),
                            *flags,
                        ]
                    ),
                    0,
                )
                self.assertEqual(run_case.call_count, expected_calls)

    def test_saved_results_reject_live_reference_options(self) -> None:
        for flag in ("--solc", "--solx"):
            with (
                self.subTest(flag=flag),
                mock.patch("sys.stderr", new_callable=io.StringIO) as stderr,
                mock.patch.object(benchmark, "find_binary") as find_binary,
                self.assertRaises(SystemExit) as error,
            ):
                benchmark.main(
                    [flag, "compiler", "--reference-results", "results.json"]
                )
            self.assertEqual(error.exception.code, 2)
            self.assertTrue(
                stderr.getvalue().endswith(
                    "error: --reference-results cannot be combined with --solc or --solx\n"
                )
            )
            find_binary.assert_not_called()

    def test_variant_flags_reject_invalid_combinations(self) -> None:
        for flags, message in (
            (["--solc-legacy"], "--solc-legacy requires --solc"),
            (
                ["--solar-size", "--optimizer-runs", "1"],
                "--solar-size cannot be combined with --optimizer-runs",
            ),
        ):
            with (
                self.subTest(flags=flags),
                mock.patch("sys.stderr", new_callable=io.StringIO) as stderr,
                self.assertRaises(SystemExit) as error,
            ):
                benchmark.main(flags)
            self.assertEqual(error.exception.code, 2)
            self.assertTrue(stderr.getvalue().endswith(f"error: {message}\n"))

    def test_variants_are_labelled_by_name_and_ordered_after_solar(self) -> None:
        case = benchmark.TEST_CASES[0]
        names = ("solx", "solc-legacy", "solc", "solar-size", "solar")
        entry = {
            "test_id": case.test_id,
            "suite": case.suite,
            "compilers": {name: {"status": "ok"} for name in names},
        }
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark, "find_binary", side_effect=lambda path, _: Path(path)
            ),
            mock.patch.object(benchmark, "binary_version", return_value=("1.0", "")),
            mock.patch.object(
                benchmark, "run_test_case", return_value=entry
            ) as run_case,
        ):
            output = Path(directory) / "results.json"
            benchmark.main(
                [
                    "--solc",
                    "solc",
                    "--solc-legacy",
                    "--solar-size",
                    "--solar",
                    "solar",
                    "--tests",
                    case.test_id,
                    "--output",
                    str(output),
                ]
            )
            document = json.loads(output.read_text())

        labels = {spec.compiler_id: spec.label for spec in run_case.call_args.args[1]}
        self.assertEqual(labels["solc-legacy"], "solc-legacy 1.0")
        self.assertEqual(labels["solar-size"], "solar-size 1.0")
        self.assertEqual(
            list(document["results"][0]["compilers"]),
            ["solar", "solar-size", "solc", "solc-legacy", "solx"],
        )

    def check_unexpected_test_error(self, flags, compilers) -> None:
        test_id = benchmark.TEST_CASES[0].test_id
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark,
                "find_binary",
                side_effect=lambda value, _fallbacks: Path(value) if value else None,
            ) as find_binary,
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
                    *flags,
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

        if compilers == {"solar"}:
            self.assertEqual(
                [call.args[0] for call in find_binary.call_args_list], ["solar"]
            )
        self.assertEqual(return_code, 0)
        self.assertEqual(len(document["results"]), 1)
        failure = document["results"][0]
        self.assertEqual(set(failure["compilers"]), compilers)
        self.assertIn("RuntimeError: unexpected", failure["benchmark_error"])
        self.assertEqual(
            {compiler["status"] for compiler in failure["compilers"].values()},
            {"failed"},
        )


class RuntimeComparisonTests(unittest.TestCase):
    def test_cold_paths_use_candidate_without_reference_and_preserve_failures(
        self,
    ) -> None:
        spec = benchmark.CompilerSpec("solar", "solar", Path("candidate"), "solar")
        for test_id in (
            "openzeppelin-vesting-wallet",
            "lilweb3-fractional",
            "nitro-one-step-proof",
        ):
            case = next(
                case for case in benchmark.TEST_CASES if case.test_id == test_id
            )
            for reference in (None, Path("reference")):
                for status in ("ok", "failed"):
                    with (
                        self.subTest(
                            test_id=test_id, reference=reference, status=status
                        ),
                        mock.patch.object(
                            benchmark,
                            "compiler_input",
                            return_value=("{}", 120, "input"),
                        ),
                        mock.patch.object(
                            benchmark,
                            "compile_case",
                            return_value={"status": "ok", "bytecode": "00"},
                        ),
                        mock.patch.object(
                            benchmark,
                            "deploy_contract",
                            return_value=("address", 1, ""),
                        ),
                        mock.patch.object(benchmark, "gas_calls", return_value=[]),
                        mock.patch.object(benchmark, "runtime_checks", return_value=[]),
                        mock.patch.object(
                            benchmark,
                            "run_cold_path_checks",
                            return_value=[
                                {"label": "cold", "status": status, "value": "1"}
                            ],
                        ) as cold_checks,
                    ):
                        result = benchmark.run_test_case(
                            case,
                            [
                                benchmark.CompilerSpec(
                                    "solc", "solc", reference, "solc"
                                ),
                                spec,
                            ]
                            if reference
                            else [spec],
                            True,
                            "hot",
                            "rpc",
                            "key",
                        )
                    self.assertEqual(
                        cold_checks.call_args_list,
                        [
                            mock.call(
                                case, "address", reference or spec.path, "rpc", "key"
                            )
                        ]
                        * (2 if reference else 1),
                    )
                    self.assertEqual(
                        result["compilers"]["solar"]["runtime_status"], status
                    )
                    self.assertEqual(
                        result["runtime_status"],
                        "failed"
                        if status == "failed"
                        else "ok"
                        if reference
                        else "skipped",
                    )

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

        merged = benchmark.merge_reference_compiler(entry, references, "solc", "input")
        specs = (
            benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc"),
            benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar"),
        )
        benchmark.compare_runtime_results(entry, specs)

        self.assertTrue(merged)
        self.assertEqual(entry["runtime_status"], "ok")
        self.assertEqual(list(entry["compilers"]), ["solc", "solar"])

        references[("runtime", "test")]["compilers"]["solx"] = references[
            ("runtime", "test")
        ]["compilers"]["solc"]
        self.assertTrue(
            benchmark.merge_reference_compiler(entry, references, "solx", "input")
        )
        benchmark.compare_runtime_results(
            entry,
            (*specs, benchmark.CompilerSpec("solx", "solx", Path("solx"), "solx")),
        )
        self.assertEqual(entry["runtime_status"], "ok")
        self.assertEqual(set(entry["compilers"]), {"solar", "solc", "solx"})

    def test_reuses_compile_failure_without_runtime_observations(self) -> None:
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "gas_profile": "hot",
            "compilers": {
                "solar": {
                    "input_fingerprint": "input",
                    "runtime_results": [{"label": "value", "value": "1"}],
                },
            },
        }
        failure = {
            "status": "failed",
            "error": "unsupported input",
            "input_fingerprint": "input",
        }
        references = {
            ("runtime", "test"): {
                "gas_profile": "hot",
                "compilers": {"solx": failure},
            },
        }
        self.assertTrue(
            benchmark.merge_reference_compiler(entry, references, "solx", "input")
        )
        self.assertEqual(entry["compilers"]["solx"], failure)
        self.assertIsNot(entry["compilers"]["solx"], failure)

        entry["compilers"].pop("solx")
        for fingerprint, profile in (("other", "hot"), ("input", "smoke")):
            with self.subTest(fingerprint=fingerprint, profile=profile):
                failure["input_fingerprint"] = fingerprint
                entry["gas_profile"] = profile
                self.assertFalse(
                    benchmark.merge_reference_compiler(
                        entry, references, "solx", "input"
                    )
                )


class ArtifactTests(unittest.TestCase):
    def test_artifacts_include_complete_source_tree(self) -> None:
        sources = {
            "src/Main.sol": {"content": 'import "../lib/Lib.sol";\ncontract Main {}\n'},
            "lib/Lib.sol": {"content": "library Lib {}\n"},
            "@scope/package/Source": {"content": "// π\r\ncontract Source {}\r\n"},
            "@scope/package/Source.sol": {"content": ""},
            "folder with spaces/你好.sol": {"content": "// UTF-8\n"},
            "remote.sol": {"urls": ["https://example.com/remote.sol"]},
        }
        spec = benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc")
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark, "run", return_value=mock.Mock(returncode=0, stdout="{}")
            ),
        ):
            root = Path(directory)
            case = benchmark.TEST_CASES[0]
            error = benchmark.write_artifacts(
                root, spec, case, (json.dumps({"sources": sources}), 1, "")
            )
            self.assertEqual(error, "")
            output = root / case.test_id / "solc" / "sources"
            self.assertEqual(
                {
                    p.relative_to(output).as_posix(): p.read_bytes().decode("utf-8")
                    for p in output.rglob("*")
                    if p.is_file()
                },
                {
                    name: source["content"]
                    for name, source in sources.items()
                    if "content" in source
                },
            )
            for name in (
                "../escape.sol",
                "/absolute.sol",
                "a/../../escape.sol",
                "a\\b.sol",
                "C:/escape.sol",
                "C:escape.sol",
                "//server/share.sol",
                "a//b.sol",
                "./a.sol",
                "a/./b.sol",
                "nul\0.sol",
                "control\x7f.sol",
                "",
            ):
                with self.subTest(name=name):
                    error = benchmark.write_artifacts(
                        root,
                        spec,
                        case,
                        (json.dumps({"sources": {name: {"content": ""}}}), 1, ""),
                    )
                    self.assertEqual(error, f"invalid source artifact path: {name!r}")

    def test_source_artifacts_reject_symlinks_and_report_collisions(self) -> None:
        spec = benchmark.CompilerSpec("solc", "solc", Path("solc"), "solc")
        case = benchmark.TEST_CASES[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / case.test_id / "solc" / "sources"
            output.mkdir(parents=True)
            outside = root / "outside"
            outside.mkdir()
            (outside / "Keep.sol").write_text("keep")
            (output / "linked").symlink_to(outside, target_is_directory=True)
            error = benchmark.write_artifacts(
                root,
                spec,
                case,
                (
                    json.dumps(
                        {"sources": {"linked/Keep.sol": {"content": "changed"}}}
                    ),
                    1,
                    "",
                ),
            )
            self.assertEqual(
                error, "source artifact path contains a symlink: 'linked/Keep.sol'"
            )
            self.assertEqual((outside / "Keep.sol").read_text(), "keep")
            for sources in (
                {"file": {"content": "keep"}, "file/Child.sol": {"content": "child"}},
                {
                    "directory/Child.sol": {"content": "child"},
                    "directory": {"content": "keep"},
                },
            ):
                error = benchmark.write_artifacts(
                    root, spec, case, (json.dumps({"sources": sources}), 1, "")
                )
                self.assertTrue(error.startswith("cannot write source artifact"), error)

    def test_artifact_input_requests_portable_outputs(self) -> None:
        test_case = benchmark.TEST_CASES[0]
        input_text, _, _ = benchmark.compiler_input(test_case, None)

        solar = json.loads(
            benchmark.artifact_compiler_input(input_text, test_case, "solar")
        )
        solc = json.loads(
            benchmark.artifact_compiler_input(input_text, test_case, "solc")
        )
        solx = json.loads(
            benchmark.artifact_compiler_input(input_text, test_case, "solx")
        )
        solar_outputs = next(iter(solar["settings"]["outputSelection"].values()))[
            test_case.contract_name
        ]
        solc_outputs = next(iter(solc["settings"]["outputSelection"].values()))[
            test_case.contract_name
        ]
        solx_outputs = next(iter(solx["settings"]["outputSelection"].values()))[
            test_case.contract_name
        ]

        self.assertNotIn("evm.deployedBytecode.opcodes", solar_outputs)
        self.assertNotIn("evm.deployedBytecode.opcodes", solc_outputs)
        legacy = json.loads(
            benchmark.artifact_compiler_input(input_text, test_case, "solc-legacy")
        )
        legacy_outputs = next(iter(legacy["settings"]["outputSelection"].values()))[
            test_case.contract_name
        ]
        self.assertEqual(legacy_outputs, solar_outputs)
        self.assertNotIn("irOptimized", solar_outputs)
        self.assertNotIn("ir", solar_outputs)
        self.assertIn("ir", solc_outputs)
        self.assertIn("irOptimized", solc_outputs)
        self.assertEqual(
            solx_outputs,
            solc_outputs
            + [
                "evm.bytecode.llvmIrUnoptimized",
                "evm.bytecode.llvmIr",
                "evm.deployedBytecode.llvmIrUnoptimized",
                "evm.deployedBytecode.llvmIr",
            ],
        )

    def test_solx_llvm_artifacts(self) -> None:
        test_case = benchmark.TEST_CASES[0]
        output = {
            "contracts": {
                "test.sol": {
                    test_case.contract_name: {
                        "evm": {
                            "bytecode": {
                                "llvmIrUnoptimized": "; creation before\n",
                                "llvmIr": "; creation after\n",
                            },
                            "deployedBytecode": {
                                "llvmIrUnoptimized": "; runtime before\n",
                                "llvmIr": "; runtime after\n",
                            },
                        }
                    }
                }
            }
        }
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                benchmark,
                "run",
                return_value=mock.Mock(returncode=0, stdout=json.dumps(output)),
            ),
        ):
            root = Path(directory)
            error = benchmark.write_artifacts(
                root,
                benchmark.CompilerSpec("solx", "solx", Path("solx"), "solx"),
                test_case,
                benchmark.compiler_input(test_case, None),
            )
            self.assertEqual(error, "")
            self.assertEqual(
                {
                    path.name: path.read_text()
                    for path in (root / test_case.test_id / "solx").glob("*.ll")
                },
                {
                    "creation.unoptimized.ll": "; creation before\n",
                    "creation.optimized.ll": "; creation after\n",
                    "runtime.unoptimized.ll": "; runtime before\n",
                    "runtime.optimized.ll": "; runtime after\n",
                },
            )

    def test_disassemble_evm_matches_solar_dump_style(self) -> None:
        self.assertEqual(
            benchmark.disassemble_evm(bytes.fromhex("6003565b00")),
            "PUSH1 0x03 ; bb0\nJUMP\n; bb0\nJUMPDEST\nSTOP\n",
        )

    def test_solar_dump_is_split_from_standard_json(self) -> None:
        contract = "A.sol:A"
        headings = (
            f"// === {contract} ===",
            f"// === {contract} (creation) ===",
            f"// === {contract} (runtime) ===",
            f"// === {contract} (deployment) ===",
            f"// === {contract} (runtime) ===",
        )
        stdout = (
            "\n".join(
                [
                    part
                    for index, heading in enumerate(headings)
                    for part in (heading, str(index))
                ]
            )
            + '\n{"contracts":{}}'
        )

        artifacts, output = benchmark.split_solar_artifact_output(stdout, contract)

        self.assertEqual(
            list(artifacts),
            [
                "mir.mir",
                "creation.evmir",
                "runtime.evmir",
                "creation.disasm",
                "runtime.disasm",
            ],
        )
        self.assertEqual(artifacts["runtime.disasm"], "4\n")
        self.assertEqual(json.loads(output), {"contracts": {}})

    def test_rejects_reference_results_for_different_inputs(self) -> None:
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "compilers": {"solar": {"input_fingerprint": "new"}},
        }
        references = {
            ("runtime", "test"): {"compilers": {"solc": {"input_fingerprint": "old"}}}
        }

        merged = benchmark.merge_reference_compiler(entry, references, "solc", "new")

        self.assertFalse(merged)
        self.assertNotIn("solc", entry["compilers"])

    def test_reference_matches_the_fingerprint_of_its_own_settings(self) -> None:
        references = {
            ("runtime", "test"): {
                "compilers": {
                    "solc-legacy": {"status": "ok", "input_fingerprint": "legacy"}
                }
            }
        }
        for fingerprint, merged in (("legacy", True), ("shared", False), (None, False)):
            with self.subTest(fingerprint=fingerprint):
                entry = {
                    "test_id": "test",
                    "suite": "runtime",
                    "compilers": {"solar": {"input_fingerprint": "shared"}},
                }
                self.assertEqual(
                    benchmark.merge_reference_compiler(
                        entry, references, "solc-legacy", fingerprint
                    ),
                    merged,
                )

    def test_reference_reuse_copies_exclusions_without_changing_raw_gas(self):
        current_call = {
            "label": "stress",
            "call": "stress()",
            "args": [],
            "gas": 90,
            "comparison_exclusion_reason": "code-dependent workload",
        }
        entry = {
            "test_id": "test",
            "suite": "runtime",
            "gas_profile": "hot",
            "compilers": {
                "solar": {"input_fingerprint": "input", "gas_results": [current_call]}
            },
        }
        old_call = {"label": "stress", "call": "stress()", "args": [], "gas": 70}
        reference = {
            "gas_profile": "hot",
            "compilers": {
                "solc": {
                    "input_fingerprint": "input",
                    "gas_results": [old_call],
                    "total_gas": 70,
                }
            },
        }
        self.assertTrue(
            benchmark.merge_reference_compiler(
                entry, {("runtime", "test"): reference}, "solc", "input"
            )
        )
        self.assertEqual(
            entry["compilers"]["solc"]["gas_results"],
            [{**old_call, "comparison_exclusion_reason": "code-dependent workload"}],
        )
        self.assertEqual(entry["compilers"]["solc"]["total_gas"], 70)
        self.assertEqual(
            old_call, {"label": "stress", "call": "stress()", "args": [], "gas": 70}
        )

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

        merged = benchmark.merge_reference_compiler(entry, references, "solc", "input")

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
        specs = (*specs, benchmark.CompilerSpec("solx", "solx", Path("solx"), "solx"))
        entry["compilers"]["solx"] = {"status": "failed"}
        benchmark.compare_runtime_results(entry, specs)
        self.assertEqual(entry["runtime_status"], "mismatch")
        self.assertEqual(
            entry["runtime_mismatches"],
            [{"label": "value", "values": {"solc": "1", "solar": "2", "solx": None}}],
        )
        entry["compilers"]["solar"]["runtime_results"][0]["value"] = "1"
        benchmark.compare_runtime_results(entry, specs)
        self.assertEqual(entry["runtime_status"], "failed")
        self.assertEqual(entry["runtime_mismatches"], [])


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


class GasKindsTests(unittest.TestCase):
    def trace(self, **values):
        trace = {
            "kinds": {"push": 30, "stack": 20, "storage": 22100},
            "exec": 22150,
            "refund": 0,
            "firstGas": 78796,
            "gasLimit": 100000,
            **values,
        }
        trace.setdefault("rootSpent", trace["exec"])
        return trace

    def trace_gas_kinds(self, receipt_gas, trace=None, error=""):
        with mock.patch.object(
            benchmark, "rpc_request_from_file", return_value=(trace, error)
        ) as request:
            return benchmark.trace_gas_kinds(
                "0xtx", receipt_gas, "http://127.0.0.1:8545"
            ), request

    def test_traces_the_call_and_splits_execution_gas(self) -> None:
        (kinds, error), request = self.trace_gas_kinds(43354, self.trace())
        self.assertEqual(error, "")
        self.assertEqual(
            kinds,
            {
                "execution_gas": 22150,
                "gas_kinds": {"push": 30, "stack": 20, "storage": 22100},
                "intrinsic_gas": 21204,
                "gas_refund": 0,
                "calldata_floor": False,
            },
        )
        method, (tx_hash, options), rpc_url, _ = request.call_args.args
        self.assertEqual(
            (method, tx_hash, rpc_url),
            ("debug_traceTransaction", "0xtx", "http://127.0.0.1:8545"),
        )
        self.assertEqual(options["tracer"], benchmark.gas_kinds_tracer())

    def test_applies_capped_refund_and_detects_calldata_floor(self) -> None:
        trace = self.trace(refund=1_000_000)
        (kinds, _), _ = self.trace_gas_kinds(43354 - 8670, trace)
        assert kinds is not None
        self.assertEqual((kinds["gas_refund"], kinds["calldata_floor"]), (8670, False))
        floor = self.trace(kinds={"push": 100}, exec=100)
        (kinds, _), _ = self.trace_gas_kinds(21510, floor)
        assert kinds is not None
        self.assertTrue(kinds["calldata_floor"])
        self.assertEqual(kinds["execution_gas"], 100)

    def test_rejects_traces_that_do_not_explain_receipt_gas(self) -> None:
        for receipt_gas, trace, error, message in (
            (43354, self.trace(exec=22151), "", "sum to 22150, not 22151"),
            (43000, self.trace(), "", "does not explain receipt gas 43000"),
            (50000, self.trace(), "", "does not explain receipt gas 50000"),
            (43354, self.trace(rootSpent=22000), "", "root frame spent 22000"),
            (43354, self.trace(firstGas=-1), "", "malformed gas trace"),
            (43354, {"kinds": {}}, "", "malformed gas trace"),
            (43354, None, "rpc down", "rpc down"),
        ):
            with self.subTest(message=message):
                (kinds, error_message), _ = self.trace_gas_kinds(
                    receipt_gas, trace, error
                )
                self.assertIsNone(kinds)
                self.assertIn(message, error_message)
        self.assertEqual(
            benchmark.trace_gas_kinds(None, 1, "rpc"),
            (None, "transaction receipt missing transactionHash"),
        )

    def run_case(self, gas_kinds, trace_result):
        case = next(case for case in benchmark.TEST_CASES if case.test_id == "counter")
        spec = benchmark.CompilerSpec("solar", "solar", Path("solar"), "solar")
        calls = [GasCall("set", "set(uint256)", ("1",))]
        with (
            mock.patch.object(
                benchmark, "compiler_input", return_value=("{}", 120, "input")
            ),
            mock.patch.object(
                benchmark,
                "compile_case",
                return_value={"status": "ok", "bytecode": "00"},
            ),
            mock.patch.object(
                benchmark, "deploy_contract", return_value=("address", 1, "")
            ),
            mock.patch.object(benchmark, "gas_calls", return_value=calls),
            mock.patch.object(benchmark, "runtime_checks", return_value=[]),
            mock.patch.object(benchmark, "send_call", return_value=(43354, "0xtx", "")),
            mock.patch.object(
                benchmark, "trace_gas_kinds", return_value=trace_result
            ) as trace,
        ):
            result = benchmark.run_test_case(
                case, [spec], True, "hot", "rpc", "key", gas_kinds=gas_kinds
            )
        return result["compilers"]["solar"], trace

    def test_gas_results_record_execution_gas_and_kinds(self) -> None:
        kinds = {"execution_gas": 20, "gas_kinds": {"push": 20}, "calldata_floor": True}
        data, trace = self.run_case(True, (kinds, ""))
        trace.assert_called_once_with("0xtx", 43354, "rpc")
        self.assertEqual(data["gas_kinds_status"], "ok")
        self.assertEqual(
            data["gas_results"],
            [
                {
                    "label": "set",
                    "call": "set(uint256)",
                    "args": ["1"],
                    "gas": 43354,
                    "comparison_exclusion_reason": None,
                    **kinds,
                }
            ],
        )

    def test_trace_failure_keeps_receipt_gas_and_fails_the_trace_status(self) -> None:
        data, _ = self.run_case(True, (None, "no trace"))
        self.assertEqual((data["gas_status"], data["total_gas"]), ("ok", 43354))
        self.assertEqual(data["gas_kinds_status"], "failed")
        self.assertEqual(data["gas_results"][0]["gas_kinds_error"], "no trace")
        self.assertNotIn("execution_gas", data["gas_results"][0])

    def test_untraced_runs_record_receipt_gas_only(self) -> None:
        data, trace = self.run_case(False, (None, ""))
        trace.assert_not_called()
        self.assertNotIn("gas_kinds_status", data)
        self.assertEqual(
            set(data["gas_results"][0]),
            {"label", "call", "args", "gas", "comparison_exclusion_reason"},
        )

    def test_send_call_returns_receipt_gas_and_hash(self) -> None:
        process = mock.Mock(
            returncode=0,
            stdout=json.dumps(
                {"status": "0x1", "gasUsed": "0x5208", "transactionHash": "0xab"}
            ),
            stderr="",
        )
        with mock.patch.object(benchmark, "run", return_value=process):
            self.assertEqual(
                benchmark.send_call("0x1", "f()", (), "rpc", "key"), (21000, "0xab", "")
            )
            self.assertEqual(
                benchmark.call_contract("0x1", "f()", (), "rpc", "key"), (21000, "")
            )

    def test_gas_kinds_requires_gas(self) -> None:
        with (
            mock.patch("sys.stderr", new_callable=io.StringIO) as stderr,
            self.assertRaises(SystemExit) as error,
        ):
            benchmark.main(["--gas-kinds"])
        self.assertEqual(error.exception.code, 2)
        self.assertTrue(
            stderr.getvalue().endswith("error: --gas-kinds requires --gas\n")
        )

    @unittest.skipUnless(
        shutil.which("anvil") and shutil.which("cast"),
        "anvil and cast are not installed",
    )
    def test_tracer_explains_receipt_gas_of_nested_frames(self) -> None:
        def push(value: int, size: int = 1) -> str:
            return f"{0x5F + size:02x}{value:0{2 * size}x}"

        def deploy(runtime: str) -> str:
            size = len(runtime) // 2
            # CODECOPY the runtime behind this 12-byte prefix, then RETURN it.
            return f"{push(size)}{push(12)}{push(0)}39{push(size)}{push(0)}f3{runtime}"

        # CALLVALUE PUSH1 jump JUMPI then a JUMPDEST PUSH0 PUSH0 REVERT, as constructor and runtime.
        child_initcode = "34600e57600a60125f39600a5ff35b5f5ffd" + "3460065700005b5f5ffd"
        # Reverts after growing memory: the REVERT step arrives through `fault`.
        reverter = push(50000, 2) + push(0) + "fd"
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            rpc_url = f"http://127.0.0.1:{probe.getsockname()[1]}"
        anvil = benchmark.start_anvil(rpc_url)
        self.addCleanup(benchmark.stop_anvil, anvil)
        reverter_address, _, error = benchmark.deploy_creation_code(
            deploy(reverter), (), None, rpc_url, benchmark.DEFAULT_PRIVATE_KEY
        )
        self.assertEqual(error, "")
        assert reverter_address is not None
        runtime = "".join(
            [
                # CALL the reverter, then the identity precompile after a frame with steps.
                push(0) * 5 + push(int(reverter_address, 16), 20) + "5af150",
                push(0) * 2 + push(0x40) + push(0) + push(4) + "5afa50",
                # CREATE a contract with a guard in its constructor and a 10-byte runtime that
                # has one (2000 gas to deposit), then CALL it.
                push(int(child_initcode, 16), 28)
                + push(0)
                + "52"
                + push(28)
                + push(4)
                + push(0)
                + "f0"
                + push(0) * 5
                + "855af150"
                + "50"
                + "00",
            ]
        )
        address, _, error = benchmark.deploy_creation_code(
            deploy(runtime), (), None, rpc_url, benchmark.DEFAULT_PRIVATE_KEY
        )
        self.assertEqual(error, "")
        assert address is not None
        gas, tx_hash, error = benchmark.send_call(
            address, "f()", (), rpc_url, benchmark.DEFAULT_PRIVATE_KEY
        )
        self.assertEqual(error, "")
        assert gas is not None
        kinds, error = benchmark.trace_gas_kinds(tx_hash, gas, rpc_url)
        self.assertEqual(error, "")
        assert kinds is not None
        self.assertFalse(kinds["calldata_floor"])
        self.assertEqual(kinds["execution_gas"] + kinds["intrinsic_gas"], gas)
        self.assertEqual(sum(kinds["gas_kinds"].values()), kinds["execution_gas"])
        self.assertGreater(kinds["gas_kinds"]["precompile"], 0)
        self.assertTrue(9000 < kinds["gas_kinds"]["mem_expansion"] < 10000)
        self.assertGreater(kinds["gas_kinds"]["call"], 32000 + 2000)
        # The constructor's guard must not hide the runtime's.
        self.assertGreaterEqual(kinds["gas_kinds"].get("guard", 0), 13)

    @unittest.skipUnless(shutil.which("node"), "node is not installed")
    def test_tracer_is_a_javascript_object_literal(self) -> None:
        script = (
            "const t = ("
            + benchmark.gas_kinds_tracer()
            + "); for (const f of ['step', 'enter', 'exit', 'fault', 'result']) if (typeof t[f] !== 'function') throw new Error(f)"
        )
        subprocess.run(["node", "-e", script], check=True)


class ParallelRunTests(unittest.TestCase):
    def test_jobs_argument_errors(self) -> None:
        for flags, message in (
            (["--jobs", "0"], "--jobs must be positive"),
            (["--jobs", "2", "--gas"], "--jobs cannot be combined with --gas"),
            (
                ["--jobs", "2", "--mode", "compile-time"],
                "--jobs skews compile-time mode; pass --ignore-compile-time",
            ),
        ):
            with (
                self.subTest(flags=flags),
                mock.patch("sys.stderr", new_callable=io.StringIO) as stderr,
                self.assertRaises(SystemExit) as error,
            ):
                benchmark.main(flags)
            self.assertEqual(error.exception.code, 2)
            self.assertTrue(stderr.getvalue().endswith(f"error: {message}\n"))

    def test_jobs_run_cases_concurrently_in_input_order(self) -> None:
        test_ids = ["counter", "factorial", "seaport-1.6-project"]
        barrier = threading.Barrier(len(test_ids), timeout=10)

        def run_case(test, *_args):
            barrier.wait()
            return {
                "test_id": test.test_id,
                "suite": test.suite,
                "compilers": {
                    "solar": {
                        "status": "ok",
                        "compile_time_seconds": 1.0,
                        "compile_time_samples": [1.0],
                        "output_fingerprint": test.test_id,
                    }
                },
            }

        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(benchmark, "find_binary", return_value=Path("solar")),
            mock.patch.object(benchmark, "binary_version", return_value=("test", "")),
            mock.patch.object(benchmark, "run_test_case", side_effect=run_case),
        ):
            output = Path(directory) / "results.json"
            return_code = benchmark.main(
                [
                    "--mode",
                    "runtime",
                    "compile-time",
                    "--jobs",
                    str(len(test_ids)),
                    "--ignore-compile-time",
                    "--tests",
                    *test_ids,
                    "--output",
                    str(output),
                ]
            )
            document = json.loads(output.read_text())

        self.assertEqual(return_code, 0)
        self.assertEqual(
            [result["test_id"] for result in document["results"]], test_ids
        )
        self.assertEqual(document["timings"], {})
        self.assertEqual(
            [result["compilers"]["solar"] for result in document["results"]],
            [
                {
                    "status": "ok",
                    "compile_time_seconds": None,
                    "output_fingerprint": test_id,
                }
                for test_id in test_ids
            ],
        )


if __name__ == "__main__":
    unittest.main()
