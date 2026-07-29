import importlib.util
import json
import sys
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("run_large_codegen_benchmark.py")
SPEC = importlib.util.spec_from_file_location("large_codegen_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class ProjectSliceTests(unittest.TestCase):
    def test_resolves_relative_and_remapped_imports(self) -> None:
        project = {
            "sources": {
                "src/A.sol": {
                    "content": (
                        'import "./B.sol";\n'
                        'import {C} from "@pkg/C.sol";\n'
                    )
                },
                "src/B.sol": {"content": 'import "../shared/D.sol";'},
                "vendor/pkg/C.sol": {"content": ""},
                "shared/D.sol": {"content": ""},
                "unused.sol": {"content": ""},
            },
            "settings": {"remappings": ["@pkg/=vendor/pkg/"]},
        }

        selected = benchmark.project_slice(project, "src/A.sol")

        self.assertEqual(
            list(selected),
            ["shared/D.sol", "src/A.sol", "src/B.sol", "vendor/pkg/C.sol"],
        )

    def test_large_cases_resolve_from_pinned_projects(self) -> None:
        root = SCRIPT.parents[2]
        counts = {}
        for case in benchmark.CASES:
            project = benchmark.load_project(root / case.project)
            counts[case.test_id] = len(
                benchmark.project_slice(project, case.source)
            )

        self.assertEqual(
            counts,
            {
                "openzeppelin-governor": 48,
                "solady-signature-checker": 12,
                "solady-lib-string": 9,
            },
        )


class RuntimeComparisonTests(unittest.TestCase):
    def test_reports_cross_compiler_mismatch(self) -> None:
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

        benchmark.compare_runtime(entry, ("solc", "solar"))

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
            value, error = benchmark.rpc(
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


class DeploymentTests(unittest.TestCase):
    def test_deploys_creation_bytecode_over_rpc(self) -> None:
        bytecode = "ab" * 100_000
        sender = "0x" + "11" * 20
        transaction_hash = "0x" + "22" * 32
        contract_address = "0x" + "33" * 20
        process = mock.Mock(returncode=0, stdout=sender + "\n", stderr="")
        receipt = {
            "status": "0x1",
            "gasUsed": "0x5208",
            "contractAddress": contract_address,
        }
        case = benchmark.Case(
            test_id="large",
            description="large",
            project="project.json",
            source="Large.sol",
            contract="Large",
            calls=(),
        )

        with (
            mock.patch.object(benchmark, "run", return_value=process) as run,
            mock.patch.object(
                benchmark,
                "rpc",
                side_effect=((transaction_hash, ""), (receipt, "")),
            ) as rpc,
        ):
            address, gas, error = benchmark.deploy(
                bytecode,
                case,
                "http://127.0.0.1:8545",
                "0x" + "44" * 32,
            )

        self.assertEqual((address, gas, error), (contract_address, 0x5208, ""))
        self.assertEqual(run.call_args.args[0][:3], ["cast", "wallet", "address"])
        method, params, _ = rpc.call_args_list[0].args
        self.assertEqual(method, "eth_sendTransaction")
        self.assertEqual(params[0]["from"], sender)
        self.assertEqual(params[0]["data"], "0x" + bytecode)
        self.assertEqual(rpc.call_args_list[0].kwargs, {"timeout": 90})
        self.assertEqual(
            rpc.call_args_list[1].args,
            ("eth_getTransactionReceipt", (transaction_hash,), "http://127.0.0.1:8545"),
        )


if __name__ == "__main__":
    unittest.main()
