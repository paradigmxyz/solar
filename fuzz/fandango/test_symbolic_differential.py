import argparse
import copy
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import MagicMock, Mock, patch


sys.path.insert(0, str(Path(__file__).parent))
import symbolic_differential as symbolic
import evm_runtime as evm
import run_foundry_target


MANIFEST_KEYS = {
    "schema",
    "schema_version",
    "created_at",
    "status",
    "reason",
    "source",
    "standard_input",
    "contract",
    "function",
    "settings",
    "compilers",
    "bounds",
    "tools",
    "solver",
    "forge",
    "replay",
    "artifacts",
    "artifact_dir",
}

CAMPAIGN_MANIFEST_KEYS = {
    "schema",
    "schema_version",
    "created_at",
    "status",
    "reason",
    "source",
    "standard_input",
    "contract",
    "settings",
    "compilers",
    "tools",
    "solver",
    "bounds",
    "inventory",
    "functions",
    "not_run",
    "findings",
    "counts",
    "all_eligible_completed",
    "campaign_complete",
    "artifacts",
    "artifact_dir",
}


def artifact(
    *,
    inputs: list[dict[str, object]] | None = None,
    outputs: list[dict[str, object]] | None = None,
    mutability: str = "pure",
    selector: str = "a1b2c3d4",
) -> dict[str, object]:
    inputs = (
        inputs
        if inputs is not None
        else [
            {"name": "amount", "type": "uint256"},
            {"name": "recipient", "type": "address"},
        ]
    )
    signature = f"probe({','.join(str(argument['type']) for argument in inputs)})"
    return {
        "abi": [
            {
                "type": "function",
                "name": "probe",
                "stateMutability": mutability,
                "inputs": inputs,
                "outputs": outputs
                if outputs is not None
                else [{"name": "result", "type": "bytes32"}],
            }
        ],
        "bin-runtime": "6000",
        "hashes": {signature: selector},
    }


def forge_payload(symbolic_result: dict[str, object]) -> dict[str, object]:
    symbolic_result = {
        "bounds": {},
        "solver": {},
        "assumptions": [],
        **symbolic_result,
    }
    return {
        "test/SymbolicDifferential.t.sol:SymbolicDifferentialTest": {
            "duration": {"secs": 0, "nanos": 1},
            "test_results": {
                "check_diff_probe(uint256,address)": {
                    "status": (
                        "Success"
                        if symbolic_result["status"] == "pass"
                        else "Failure"
                    ),
                    "counterexample_artifacts": [],
                    "symbolic": symbolic_result,
                }
            },
        }
    }


class FocusedCommandTests(unittest.TestCase):
    def test_focused_command_requires_an_explicit_contract(self):
        with (
            patch.object(
                sys,
                "argv",
                ["solsymdiff", "--source", "Target.sol"],
            ),
            patch.object(sys, "stderr", io.StringIO()),
            self.assertRaises(SystemExit) as raised,
        ):
            run_foundry_target.symbolic_main()

        self.assertEqual(raised.exception.code, 2)

    def test_legacy_symbolic_entrypoint_keeps_its_contract_default(self):
        with (
            patch.object(
                sys,
                "argv",
                [
                    "run_foundry_target.py",
                    "--symbolic",
                    "--source",
                    "Target.sol",
                ],
            ),
            patch.object(
                run_foundry_target,
                "_run_symbolic_or_incomplete",
                return_value=0,
            ) as run,
        ):
            self.assertEqual(run_foundry_target.main(), 0)

        self.assertEqual(run.call_args.args[0].contract, "FandangoRuntime")

    def test_dynamic_length_override_is_parsed_once_for_the_campaign(self):
        with (
            patch.object(
                sys,
                "argv",
                [
                    "solsymdiff",
                    "--source",
                    "Target.sol",
                    "--contract",
                    "Target",
                    "--symbolic-dynamic-lengths",
                    "0,2,4",
                ],
            ),
            patch.object(
                run_foundry_target,
                "_run_symbolic_or_incomplete",
                return_value=0,
            ) as run,
        ):
            self.assertEqual(run_foundry_target.symbolic_main(), 0)

        self.assertEqual(
            run.call_args.args[0].symbolic_dynamic_lengths,
            (0, 2, 4),
        )

    def test_dynamic_length_override_rejects_invalid_or_duplicate_values(self):
        for value in ("", "-1", "257", "1,1", "one"):
            with (
                self.subTest(value=value),
                self.assertRaises(argparse.ArgumentTypeError),
            ):
                run_foundry_target._parse_symbolic_dynamic_lengths(value)


class DeadlineTests(unittest.TestCase):
    def test_remaining_decreases_and_expiration_is_explicit(self):
        with patch.object(
            evm.time, "monotonic", side_effect=[10.0, 10.5, 11.1]
        ):
            deadline = evm.Deadline(1.0)
            self.assertEqual(deadline.remaining("first operation"), 0.5)
            with self.assertRaisesRegex(TimeoutError, "second operation"):
                deadline.remaining("second operation")

    def test_expired_deadline_does_not_spawn_forge(self):
        args = argparse.Namespace(
            forge="must-not-run",
            evm_version="osaka",
            symbolic_solver="z3",
            symbolic_timeout=5,
            symbolic_max_paths=16,
            symbolic_max_depth=None,
            timeout=1.0,
            _solc_executable="/unused/solc",
        )
        with patch.object(evm.time, "monotonic", side_effect=[10.0, 11.1]):
            deadline = evm.Deadline(1.0)
            with patch.object(evm.subprocess, "Popen") as popen:
                result = run_foundry_target._forge_symbolic(
                    args, Path("/unused"), "checkDiff_deadbeef(uint256)", deadline
                )

        popen.assert_not_called()
        self.assertEqual(result["status"], "timeout")
        self.assertIn("total wall timeout", result["reason"])

    def test_tool_timeout_kills_the_process_group(self):
        process = Mock()
        process.pid = 1234
        process.returncode = -9
        process.communicate.side_effect = [
            subprocess.TimeoutExpired(["forge"], 1),
            ("partial stdout", "partial stderr"),
        ]
        with (
            patch.object(evm.subprocess, "Popen", return_value=process),
            patch.object(evm, "kill_process_tree") as kill_tree,
            self.assertRaises(subprocess.TimeoutExpired),
        ):
            evm.run_process_group(["tool"], 1)

        kill_tree.assert_called_once_with(process)

    def test_tool_interrupt_kills_and_reaps_the_process_group(self):
        process = Mock()
        process.communicate.side_effect = [KeyboardInterrupt, ("", "")]
        with (
            patch.object(evm.subprocess, "Popen", return_value=process),
            patch.object(evm, "kill_process_tree") as kill_tree,
            self.assertRaises(KeyboardInterrupt),
        ):
            evm.run_process_group(["tool"], 1)

        kill_tree.assert_called_once_with(process)
        self.assertEqual(process.communicate.call_count, 2)

    def test_tool_input_and_check_behavior_match_subprocess_run(self):
        result = evm.run_process_group(
            [
                sys.executable,
                "-c",
                "import sys; sys.stdout.write(sys.stdin.read().upper())",
            ],
            5,
            input="standard json",
            check=True,
        )
        self.assertEqual(result.stdout, "STANDARD JSON")

        with self.assertRaises(subprocess.CalledProcessError) as raised:
            evm.run_process_group(
                [sys.executable, "-c", "raise SystemExit(7)"],
                5,
                check=True,
            )
        self.assertEqual(raised.exception.returncode, 7)

    def test_compilation_and_version_share_the_same_deadline(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "Root.sol"
            source.write_text("contract Root {}")
            standard_input = evm._single_source_standard_input(source, "osaka")
            compile_result = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout=json.dumps(
                    {
                        "contracts": {
                            "Root.sol": {
                                "Root": {
                                    "abi": [],
                                    "evm": {
                                        "deployedBytecode": {
                                            "object": "6000",
                                            "immutableReferences": {},
                                            "linkReferences": {},
                                        },
                                        "methodIdentifiers": {},
                                    },
                                }
                            }
                        }
                    }
                ),
                stderr="",
            )
            version_result = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout="solc version",
                stderr="",
            )
            with (
                patch.object(
                    evm.time,
                    "monotonic",
                    side_effect=[10.0, 10.2, 10.8],
                ),
                patch.object(
                    evm,
                    "run_process_group",
                    side_effect=[compile_result, version_result],
                ) as run,
            ):
                deadline = evm.Deadline(1.0)
                evm.compile_standard_artifact(
                    "solc",
                    source,
                    "Root",
                    1.0,
                    kind="solc",
                    evm_version="osaka",
                    standard_input=standard_input,
                    deadline=deadline,
                )

        self.assertAlmostEqual(run.call_args_list[0].kwargs["timeout"], 0.8)
        self.assertAlmostEqual(run.call_args_list[1].kwargs["timeout"], 0.2)

    @unittest.skipUnless(os.name == "posix", "POSIX process-group regression")
    def test_sigterm_cleans_a_real_tool_grandchild_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            pid_file = Path(temporary) / "pids"
            child_code = (
                "import os,pathlib,subprocess,sys,time;"
                "grand=subprocess.Popen([sys.executable,'-c','import time;"
                "time.sleep(60)']);"
                f"pathlib.Path({str(pid_file)!r}).write_text("
                "f'{os.getpid()} {grand.pid}');"
                "time.sleep(60)"
            )
            module_dir = str(Path(run_foundry_target.__file__).parent)
            helper_code = (
                "import sys;"
                f"sys.path.insert(0,{module_dir!r});"
                "import evm_runtime;"
                "evm_runtime.run_process_group("
                f"[sys.executable,'-c',{child_code!r}],60)"
            )
            helper = subprocess.Popen([sys.executable, "-c", helper_code])
            pids: list[int] = []
            try:
                deadline = time.monotonic() + 5
                while not pid_file.is_file() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(pid_file.is_file())
                pids = [int(value) for value in pid_file.read_text().split()]

                helper.terminate()
                helper.wait(timeout=5)
                for pid in pids:
                    self.assertTrue(self._wait_for_process_exit(pid))
            finally:
                if helper.poll() is None:
                    helper.kill()
                    helper.wait()
                for pid in pids:
                    try:
                        os.kill(pid, 9)
                    except ProcessLookupError:
                        pass

    @unittest.skipUnless(os.name == "posix", "POSIX process-group regression")
    def test_compiler_timeout_reaps_a_real_grandchild(self):
        self._assert_wrapper_grandchild_is_reaped(timeout=1, wrapper_sleep=60)

    @unittest.skipUnless(os.name == "posix", "POSIX process-group regression")
    def test_successful_tool_reaps_a_real_grandchild(self):
        self._assert_wrapper_grandchild_is_reaped(timeout=5, wrapper_sleep=0)

    @unittest.skipUnless(os.name == "posix", "POSIX process-group regression")
    def test_graceful_tree_cleanup_kills_a_descendant_that_ignores_sigterm(self):
        with tempfile.TemporaryDirectory() as temporary:
            child_pid_file = Path(temporary) / "child-pid"
            child_code = (
                "import os,pathlib,signal,time;"
                "signal.signal(signal.SIGTERM,signal.SIG_IGN);"
                f"pathlib.Path({str(child_pid_file)!r}).write_text(str(os.getpid()));"
                "time.sleep(60)"
            )
            leader_code = (
                "import subprocess,sys,time;"
                f"subprocess.Popen([sys.executable,'-c',{child_code!r}]);"
                "time.sleep(60)"
            )
            leader = subprocess.Popen(
                [sys.executable, "-c", leader_code],
                start_new_session=True,
            )
            child_pid = None
            try:
                deadline = time.monotonic() + 5
                while (
                    not child_pid_file.is_file()
                    and time.monotonic() < deadline
                ):
                    time.sleep(0.05)
                self.assertTrue(child_pid_file.is_file())
                child_pid = int(child_pid_file.read_text())

                evm.terminate_process_tree(leader, grace_seconds=0.2)

                self.assertIsNotNone(leader.poll())
                self.assertTrue(self._wait_for_process_exit(child_pid))
            finally:
                if leader.poll() is None:
                    evm.kill_process_tree(leader)
                    leader.wait()
                if child_pid is not None:
                    try:
                        os.kill(child_pid, 9)
                    except ProcessLookupError:
                        pass

    @staticmethod
    def _wait_for_process_exit(pid: int) -> bool:
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return True
            time.sleep(0.05)
        return False

    def _assert_wrapper_grandchild_is_reaped(
        self, *, timeout: float, wrapper_sleep: int
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "grandchild-pid"
            wrapper_code = (
                "import pathlib,subprocess,sys,time;"
                "grandchild=subprocess.Popen("
                "[sys.executable,'-c','import time; time.sleep(60)'],"
                "stdin=subprocess.DEVNULL,"
                "stdout=subprocess.DEVNULL,"
                "stderr=subprocess.DEVNULL);"
                f"pathlib.Path({str(pid_file)!r}).write_text(str(grandchild.pid));"
                "print('wrapper version',flush=True);"
                f"time.sleep({wrapper_sleep})"
            )
            command = [sys.executable, "-c", wrapper_code]
            grandchild_pid = None
            try:
                if wrapper_sleep:
                    with self.assertRaises(subprocess.TimeoutExpired):
                        evm.run_process_group(command, timeout)
                else:
                    self.assertEqual(
                        evm.run_process_group(command, timeout).stdout.strip(),
                        "wrapper version",
                    )
                self.assertTrue(pid_file.is_file())
                grandchild_pid = int(pid_file.read_text())
                self.assertTrue(self._wait_for_process_exit(grandchild_pid))
            finally:
                if grandchild_pid is not None:
                    try:
                        os.kill(grandchild_pid, 9)
                    except ProcessLookupError:
                        pass


class RpcOutcomeTests(unittest.TestCase):
    def test_eth_call_accepts_only_recognized_evm_execution_errors(self):
        recognized = [
            (
                {
                    "error": {
                        "code": 3,
                        "message": "execution reverted",
                        "data": "0x1234",
                    }
                },
                "0x1234",
            ),
            (
                {
                    "error": {
                        "code": -32003,
                        "message": "EVM error: out of gas",
                    }
                },
                "0x",
            ),
            (
                {
                    "error": {
                        "code": -32003,
                        "message": "EVM error InvalidJump",
                    }
                },
                "0x",
            ),
            (
                {
                    "error": {
                        "code": -32003,
                        "message": "EVM error StackUnderflow",
                    }
                },
                "0x",
            ),
        ]
        for response, expected in recognized:
            with (
                self.subTest(response=response),
                patch.object(evm, "rpc", return_value=response),
            ):
                self.assertEqual(
                    evm.eth_call("http://unused", evm.SOLC_ADDRESS, "0x", 1),
                    {"status": "revert", "data": expected},
                )

    def test_eth_call_rejects_rpc_and_malformed_result_failures(self):
        responses = [
            {"error": {"code": -32603, "message": "Internal error"}},
            {"error": None},
            {},
            {"result": "not-hex"},
            {"result": "0x0"},
            {
                "result": "0x",
                "error": {"code": -32603, "message": "Internal error"},
            },
        ]
        for response in responses:
            with (
                self.subTest(response=response),
                patch.object(evm, "rpc", return_value=response),
                self.assertRaises(evm.InfraError),
            ):
                evm.eth_call("http://unused", evm.SOLC_ADDRESS, "0x", 1)

    def test_set_code_requires_an_explicit_json_rpc_result(self):
        with patch.object(evm, "rpc", return_value={"result": None}):
            evm.set_code("http://unused", evm.SOLC_ADDRESS, "0x6000", 1)
        for response in (
            {},
            {"error": {"code": -32603, "message": "Internal error"}},
            {"result": None, "error": {"code": -32603}},
        ):
            with (
                self.subTest(response=response),
                patch.object(evm, "rpc", return_value=response),
                self.assertRaises(evm.InfraError),
            ):
                evm.set_code("http://unused", evm.SOLC_ADDRESS, "0x6000", 1)


class StandardInputMaterializationTests(unittest.TestCase):
    def test_materializes_the_discovered_import_closure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "Root.sol"
            helper = Path(temporary) / "Helper.sol"
            root.write_text('import "./Helper.sol"; contract Root {}')
            helper.write_text("library Helper {}")
            discovery = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout=json.dumps(
                    {"sources": {"Root.sol": {"id": 0}, "Helper.sol": {"id": 1}}}
                ),
                stderr="",
            )
            with patch.object(evm, "run_process_group", return_value=discovery):
                materialized = evm.materialize_standard_input(
                    "solc", root, 10, "osaka"
                )

        standard_input = json.loads(materialized["json"])
        self.assertEqual(
            set(standard_input["sources"]), {"Root.sol", "Helper.sol"}
        )
        self.assertEqual(
            standard_input["sources"]["Helper.sol"]["content"], "library Helper {}"
        )
        self.assertEqual(
            materialized["sha256"],
            hashlib.sha256(materialized["json"].encode("utf-8")).hexdigest(),
        )
        selection = standard_input["settings"]["outputSelection"]["*"]["*"]
        self.assertIn("evm.deployedBytecode.immutableReferences", selection)
        self.assertIn("evm.deployedBytecode.linkReferences", selection)
        self.assertEqual(
            standard_input["settings"]["outputSelection"]["*"][""],
            ["ast"],
        )

    def test_finds_inline_assembly_in_the_exact_solc_source_asts(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "Root.sol"
            source.write_text("contract Root {}")
            standard_input = evm._single_source_standard_input(
                source, "osaka"
            )
        output = {
            "sources": {
                "Root.sol": {
                    "ast": {
                        "nodeType": "SourceUnit",
                        "nodes": [
                            {
                                "nodeType": "InlineAssembly",
                                "src": "42:18:0",
                            }
                        ],
                    }
                }
            }
        }

        self.assertEqual(
            evm._solc_inline_assembly_sites(output, standard_input),
            [{"source": "Root.sol", "src": "42:18:0"}],
        )
        self.assertIsNone(
            evm._solc_inline_assembly_sites({"sources": {}}, standard_input)
        )

    def test_rejects_an_import_that_cannot_be_snapshotted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "Root.sol"
            root.write_text('import "./Missing.sol"; contract Root {}')
            discovery = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout=json.dumps(
                    {"sources": {"Root.sol": {"id": 0}, "Missing.sol": {"id": 1}}}
                ),
                stderr="",
            )
            with (
                patch.object(evm, "run_process_group", return_value=discovery),
                self.assertRaisesRegex(ValueError, "could not snapshot"),
            ):
                evm.materialize_standard_input("solc", root, 10, "osaka")

    def test_rejects_malformed_compiler_json_structures(self):
        malformed = [
            [],
            {"errors": {}},
            {"errors": [None]},
        ]
        for output in malformed:
            result = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout=json.dumps(output),
                stderr="",
            )
            with self.subTest(output=output), self.assertRaises(RuntimeError):
                evm._compiler_output(result, "compiler")

    def test_rejects_malformed_runtime_bytecode_at_the_compiler_boundary(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "Root.sol"
            source.write_text("contract Root {}")
            standard_input = evm._single_source_standard_input(source, "osaka")
            for runtime, message in (
                ("0x", "has no runtime bytecode"),
                ("0", "runtime bytecode is not byte-aligned"),
                ("zz", "runtime bytecode is not hex"),
                ("60  00", "runtime bytecode is not hex"),
            ):
                result = subprocess.CompletedProcess(
                    args=[],
                    returncode=0,
                    stdout=json.dumps(
                        {
                            "contracts": {
                                "Root.sol": {
                                    "Root": {
                                        "abi": [],
                                        "evm": {
                                            "deployedBytecode": {
                                                "object": runtime,
                                                "immutableReferences": {},
                                                "linkReferences": {},
                                            },
                                            "methodIdentifiers": {},
                                        },
                                    }
                                }
                            }
                        }
                    ),
                    stderr="",
                )
                with (
                    self.subTest(runtime=runtime),
                    patch.object(evm, "run_process_group", return_value=result),
                    self.assertRaisesRegex(ValueError, message),
                ):
                    evm.compile_standard_artifact(
                        "solc",
                        source,
                        "Root",
                        1.0,
                        kind="solc",
                        evm_version="osaka",
                        standard_input=standard_input,
                    )

    def test_rejects_runtime_templates_that_require_deployment(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "Root.sol"
            source.write_text("contract Root {}")
            standard_input = evm._single_source_standard_input(source, "osaka")
            cases = [
                (
                    {
                        "object": "6000",
                        "immutableReferences": {
                            "configured": [{"start": 0, "length": 32}]
                        },
                        "linkReferences": {},
                    },
                    "immutable references",
                ),
                (
                    {
                        "object": "6000",
                        "immutableReferences": {
                            "library_deploy_address": [
                                {"start": 0, "length": 20}
                            ]
                        },
                        "linkReferences": {},
                    },
                    "immutable references",
                ),
                (
                    {
                        "object": "6000",
                        "immutableReferences": {},
                        "linkReferences": {
                            "Library.sol": {
                                "Library": [{"start": 0, "length": 20}]
                            }
                        },
                    },
                    "unresolved library links",
                ),
                (
                    {
                        "object": "6000",
                        "immutableReferences": [],
                        "linkReferences": {},
                    },
                    "malformed solc immutable references",
                ),
                (
                    {
                        "object": "6000",
                        "immutableReferences": {},
                        "linkReferences": [],
                    },
                    "malformed solc unresolved library links",
                ),
                (
                    {"object": "6000", "linkReferences": {}},
                    "missing solc immutable references",
                ),
                (
                    {"object": "6000", "immutableReferences": {}},
                    "missing solc unresolved library links",
                ),
            ]
            for deployed, message in cases:
                result = subprocess.CompletedProcess(
                    args=[],
                    returncode=0,
                    stdout=json.dumps(
                        {
                            "contracts": {
                                "Root.sol": {
                                    "Root": {
                                        "abi": [],
                                        "evm": {
                                            "deployedBytecode": deployed,
                                            "methodIdentifiers": {},
                                        },
                                    }
                                }
                            }
                        }
                    ),
                    stderr="",
                )
                with (
                    self.subTest(message=message),
                    patch.object(evm, "run_process_group", return_value=result),
                    self.assertRaisesRegex(ValueError, message),
                ):
                    evm.compile_standard_artifact(
                        "solc",
                        source,
                        "Root",
                        1.0,
                        kind="solc",
                        evm_version="osaka",
                        standard_input=standard_input,
                    )


class SymbolicFunctionSelectionTests(unittest.TestCase):
    def test_selects_matching_pure_function_and_normalizes_selector(self):
        solc = artifact(selector="A1B2C3D4")
        solar = copy.deepcopy(solc)

        selected = symbolic.select_function(
            solc, solar, "probe(uint256,address)"
        )

        self.assertEqual(selected["signature"], "probe(uint256,address)")
        self.assertEqual(selected["selector"], "0xa1b2c3d4")
        self.assertEqual(
            [argument["type"] for argument in selected["abi"]["inputs"]],
            ["uint256", "address"],
        )

    def test_accepts_fixed_array_of_static_values(self):
        inputs = [
            {"name": "values", "type": "uint256[2]"},
            {"name": "tags", "type": "bytes32[2][3]"},
        ]
        solc = artifact(inputs=inputs)
        solc["hashes"] = {
            "probe(uint256[2],bytes32[2][3])": "01020304"
        }
        solar = copy.deepcopy(solc)

        selected = symbolic.select_function(
            solc, solar, "probe(uint256[2],bytes32[2][3])"
        )

        self.assertEqual(selected["selector"], "0x01020304")
        self.assertEqual(selected["abi"]["inputs"], inputs)

    def test_accepts_dynamic_outputs(self):
        outputs = [
            {"name": "data", "type": "bytes"},
            {"name": "text", "type": "string"},
            {"name": "values", "type": "uint256[]"},
            {
                "name": "pairs",
                "type": "tuple[]",
                "components": [
                    {"name": "amount", "type": "uint256"},
                    {"name": "recipient", "type": "address"},
                ],
            },
        ]
        solc = artifact(outputs=outputs)
        solar = copy.deepcopy(solc)

        selected = symbolic.select_function(
            solc, solar, "probe(uint256,address)"
        )

        self.assertEqual(
            selected["outputs"],
            ["bytes", "string", "uint256[]", "(uint256,address)[]"],
        )

    def test_accepts_dynamic_inputs_supported_by_foundry(self):
        dynamic_cases = [
            [{"name": "value", "type": "bytes"}],
            [{"name": "value", "type": "string"}],
            [{"name": "value", "type": "uint256[]"}],
            [{"name": "value", "type": "bytes[2]"}],
            [{"name": "value", "type": "bytes[][2]"}],
        ]
        for inputs in dynamic_cases:
            with self.subTest(inputs=inputs):
                solc = artifact(inputs=inputs)
                solar = copy.deepcopy(solc)
                signature = next(iter(solc["hashes"]))
                selected = symbolic.select_function(solc, solar, signature)
                self.assertEqual(selected["inputs"], [inputs[0]["type"]])
                self.assertEqual(
                    symbolic.solidity_parameter_declarations(
                        selected["inputs"]
                    ),
                    [f"{inputs[0]['type']} calldata arg0"],
                )

    def test_accepts_tuple_input_and_preserves_its_canonical_shape(self):
        inputs = [
            {
                "name": "value",
                "type": "tuple",
                "components": [
                    {"name": "amount", "type": "uint256"},
                    {"name": "recipient", "type": "address"},
                ],
            }
        ]
        solc = artifact(inputs=inputs)
        solc["hashes"] = {"probe((uint256,address))": "01020304"}
        solar = copy.deepcopy(solc)

        selected = symbolic.select_function(
            solc, solar, "probe((uint256,address))"
        )

        self.assertEqual(selected["inputs"], ["(uint256,address)"])

    def test_rejects_an_unsupported_type_nested_inside_a_tuple(self):
        inputs = [
            {
                "name": "value",
                "type": "tuple",
                "components": [
                    {"name": "callback", "type": "function"},
                ],
            }
        ]
        solc = artifact(inputs=inputs)
        solc["hashes"] = {"probe((function))": "01020304"}
        solar = copy.deepcopy(solc)

        with self.assertRaisesRegex(ValueError, "unsupported"):
            symbolic.select_function(solc, solar, "probe((function))")

    def test_rejects_non_pure_function(self):
        solc = artifact(mutability="view")
        solar = copy.deepcopy(solc)

        with self.assertRaisesRegex(ValueError, "pure"):
            symbolic.select_function(solc, solar, "probe(uint256,address)")

    def test_rejects_compiler_abi_or_selector_disagreement(self):
        solc = artifact()
        solar = copy.deepcopy(solc)
        solar["abi"][0]["outputs"] = [{"name": "result", "type": "uint256"}]
        with self.assertRaisesRegex(ValueError, "ABI|abi"):
            symbolic.select_function(solc, solar, "probe(uint256,address)")

        solar = copy.deepcopy(solc)
        solar["hashes"]["probe(uint256,address)"] = "ffffffff"
        with self.assertRaisesRegex(ValueError, "selector"):
            symbolic.select_function(solc, solar, "probe(uint256,address)")

    def test_rejects_non_string_method_identifier(self):
        solc = artifact()
        signature = next(iter(solc["hashes"]))
        solc["hashes"][signature] = 7
        solar = copy.deepcopy(solc)

        with self.assertRaisesRegex(ValueError, "strings"):
            symbolic.select_function(solc, solar, signature)

    def test_rejects_missing_signature(self):
        solc = artifact()
        solar = copy.deepcopy(solc)

        with self.assertRaisesRegex(ValueError, "not found|missing"):
            symbolic.select_function(solc, solar, "missing(uint256)")

    def test_focused_selection_rejects_duplicate_abi_signatures(self):
        solc = artifact()
        solc["abi"].append(copy.deepcopy(solc["abi"][0]))
        solar = copy.deepcopy(solc)

        with self.assertRaisesRegex(ValueError, "duplicate"):
            symbolic.select_function(
                solc, solar, "probe(uint256,address)"
            )


class FunctionInventoryTests(unittest.TestCase):
    @staticmethod
    def _function(
        name: str,
        *,
        mutability: str = "pure",
        input_type: str = "uint256",
        output_type: str = "uint256",
    ) -> dict[str, object]:
        return {
            "type": "function",
            "name": name,
            "stateMutability": mutability,
            "inputs": [{"name": "value", "type": input_type}],
            "outputs": [{"name": "result", "type": output_type}],
        }

    def test_inventory_is_deterministic_and_lists_unsupported_functions(self):
        functions = [
            self._function("zeta"),
            self._function("dynamic", input_type="bytes"),
            self._function("dynamicOutput", output_type="bytes"),
            self._function("unsupported", input_type="function"),
            self._function("observed", mutability="view"),
            self._function("alpha"),
        ]
        solc = {
            "abi": functions,
            "hashes": {
                "zeta(uint256)": "00000004",
                "dynamic(bytes)": "00000002",
                "dynamicOutput(uint256)": "00000005",
                "unsupported(function)": "00000006",
                "observed(uint256)": "00000003",
                "alpha(uint256)": "00000001",
            },
        }
        solar = copy.deepcopy(solc)

        inventory = symbolic.function_inventory(solc, solar)

        self.assertEqual(
            [item["signature"] for item in inventory["eligible"]],
            [
                "alpha(uint256)",
                "dynamic(bytes)",
                "dynamicOutput(uint256)",
                "zeta(uint256)",
            ],
        )
        self.assertEqual(
            [item["signature"] for item in inventory["excluded"]],
            ["observed(uint256)", "unsupported(function)"],
        )
        self.assertEqual(inventory["errors"], [])

    def test_inventory_reports_union_disagreements_without_hiding_valid_siblings(self):
        shared = self._function("shared")
        missing = self._function("missing")
        shaped = self._function("shaped")
        selector = self._function("selector")
        solc = {
            "abi": [shared, missing, shaped, selector],
            "hashes": {
                "shared(uint256)": "00000001",
                "missing(uint256)": "00000002",
                "shaped(uint256)": "00000003",
                "selector(uint256)": "00000004",
            },
        }
        solar = copy.deepcopy(solc)
        solar["abi"] = [
            copy.deepcopy(shared),
            self._function("shaped", output_type="bytes32"),
            copy.deepcopy(selector),
        ]
        solar["hashes"].pop("missing(uint256)")
        solar["hashes"]["selector(uint256)"] = "ffffffff"

        inventory = symbolic.function_inventory(solc, solar)

        self.assertEqual(
            [item["signature"] for item in inventory["eligible"]],
            ["shared(uint256)"],
        )
        self.assertEqual(
            [item["signature"] for item in inventory["errors"]],
            [
                "missing(uint256)",
                "selector(uint256)",
                "shaped(uint256)",
            ],
        )

    def test_malformed_entries_and_identifiers_do_not_suppress_valid_siblings(self):
        solc = {
            "abi": [
                self._function("shared"),
                {
                    "type": "function",
                    "name": "broken",
                    "stateMutability": "pure",
                    "inputs": "not-an-array",
                    "outputs": [],
                },
            ],
            "hashes": {
                "shared(uint256)": "00000001",
                "broken(uint256)": 7,
            },
        }
        solar = {
            "abi": [self._function("shared")],
            "hashes": {"shared(uint256)": "00000001"},
        }

        inventory = symbolic.function_inventory(solc, solar)

        self.assertEqual(
            [item["signature"] for item in inventory["eligible"]],
            ["shared(uint256)"],
        )
        self.assertEqual(len(inventory["errors"]), 2)
        self.assertTrue(
            any("inputs" in item["reason"] for item in inventory["errors"])
        )
        self.assertTrue(
            any("strings" in item["reason"] for item in inventory["errors"])
        )


class RuntimeScopeTests(unittest.TestCase):
    def test_detects_unsupported_opcodes_but_not_push_data(self):
        self.assertEqual(
            symbolic.runtime_scope_opcodes("0x60fa00"),
            [],
        )
        self.assertEqual(
            symbolic.runtime_scope_opcodes("0x6000fa"),
            [{"offset": 2, "opcode": "STATICCALL"}],
        )
        self.assertEqual(
            symbolic.runtime_scope_opcodes("0x383958595a"),
            [
                {"offset": 0, "opcode": "CODESIZE"},
                {"offset": 1, "opcode": "CODECOPY"},
                {"offset": 2, "opcode": "PC"},
                {"offset": 3, "opcode": "MSIZE"},
                {"offset": 4, "opcode": "GAS"},
            ],
        )
        self.assertEqual(
            symbolic.runtime_scope_opcodes("0x323342"),
            [
                {"offset": 0, "opcode": "ORIGIN"},
                {"offset": 1, "opcode": "CALLER"},
                {"offset": 2, "opcode": "TIMESTAMP"},
            ],
        )

    def test_untrusted_metadata_cannot_hide_a_message_call(self):
        # A compiler-under-test may emit malformed metadata, so a length-like
        # trailer is never trusted to remove bytes from the fail-closed scan.
        self.assertEqual(
            symbolic.runtime_scope_opcodes("0x00a1fa0002"),
            [{"offset": 2, "opcode": "STATICCALL"}],
        )

    def test_rejects_reserved_ef_prefixed_runtime_formats(self):
        for runtime in ("0xef00", "0xef0100" + "11" * 20):
            with self.subTest(runtime=runtime):
                self.assertEqual(
                    symbolic.runtime_scope_opcodes(runtime),
                    [
                        {
                            "offset": 0,
                            "opcode": "EF_PREFIXED_NON_LEGACY_RUNTIME",
                        }
                    ],
                )

    def test_rejects_malformed_runtime_bytecode(self):
        for runtime in ("0x0", "0xzz"):
            with self.subTest(runtime=runtime), self.assertRaises(ValueError):
                symbolic.runtime_scope_opcodes(runtime)


class SymbolicTargetGenerationTests(unittest.TestCase):
    def test_runtime_switching_happens_only_during_concrete_setup(self):
        function = {
            "signature": "probe(uint256)",
            "selector": "0x01020304",
            "inputs": ["uint256"],
            "outputs": ["uint256"],
            "test": "checkDiff_01020304",
        }
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            run_foundry_target.write_foundry_target.write_symbolic_target(
                project,
                "0x6000",
                "0x6001",
                function,
                64,
                "osaka",
            )
            source = (
                project / "test" / "SymbolicDifferential.t.sol"
            ).read_text()
            foundry_config = (project / "foundry.toml").read_text()

        property_body = source.split("function checkDiff_01020304", 1)[1]
        router_body = source.split("contract RuntimeRouter", 1)[1].split(
            "contract SymbolicDifferentialTest", 1
        )[0]
        self.assertNotIn("vm.etch", property_body)
        self.assertNotIn("function ", router_body)
        self.assertIn("type(RuntimeRouter).runtimeCode", source)
        self.assertIn(
            "_routedStaticCall(SOLC_IMPLEMENTATION, callData)",
            source,
        )
        self.assertIn(
            "_routedStaticCall(SOLAR_IMPLEMENTATION, callData)",
            source,
        )
        self.assertIn(
            "default_array_lengths = [0, 1, 2, 3]",
            foundry_config,
        )
        self.assertIn(
            "default_bytes_lengths = [0, 1, 2, 3]",
            foundry_config,
        )
        self.assertLess(
            property_body.index("_warmRouter();"),
            property_body.index(
                "_routedStaticCall(SOLC_IMPLEMENTATION, callData)"
            ),
        )
        self.assertIn("delegatecall", source)

    def test_nested_tuple_inputs_generate_canonical_struct_parameters(self):
        tuple_input = {
            "name": "value",
            "type": "tuple[]",
            "components": [
                {"name": "owner", "type": "address"},
                {
                    "name": "inner",
                    "type": "tuple",
                    "components": [
                        {"name": "amount", "type": "uint256"},
                        {"name": "tag", "type": "bytes"},
                    ],
                },
            ],
        }
        function = {
            "signature": "probe((address,(uint256,bytes))[])",
            "selector": "0x01020304",
            "inputs": ["(address,(uint256,bytes))[]"],
            "outputs": ["uint256"],
            "test": "checkDiff_01020304",
            "abi": {"inputs": [tuple_input]},
        }
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            run_foundry_target.write_foundry_target.write_symbolic_target(
                project,
                "0x6000",
                "0x6001",
                function,
                64,
                "osaka",
            )
            source = (
                project / "test" / "SymbolicDifferential.t.sol"
            ).read_text()

        self.assertIn("struct SymbolicInput0_1", source)
        self.assertIn("uint256 field0;", source)
        self.assertIn("bytes field1;", source)
        self.assertIn("SymbolicInput0_1 field1;", source)
        self.assertIn(
            "function checkDiff_01020304("
            "SymbolicInput0[] calldata arg0)",
            source,
        )


class TargetCalldataTests(unittest.TestCase):
    def test_replaces_only_the_wrapper_selector(self):
        wrapper_calldata = (
            "0xdeadbeef"
            + "00" * 31
            + "2a"
            + "00" * 12
            + "1234567890abcdef1234567890abcdef12345678"
        )

        target = symbolic.target_calldata("0xa1b2c3d4", wrapper_calldata)

        self.assertEqual(target, "0xa1b2c3d4" + wrapper_calldata[10:])

    def test_supports_a_zero_argument_wrapper(self):
        self.assertEqual(
            symbolic.target_calldata("0xa1b2c3d4", "0xdeadbeef"),
            "0xa1b2c3d4",
        )

    def test_rejects_malformed_selector_or_wrapper_calldata(self):
        cases = [
            ("0x010203", "0xdeadbeef"),
            ("0x0102030405", "0xdeadbeef"),
            ("0xzzzzzzzz", "0xdeadbeef"),
            ("0x01020304", "0x"),
            ("0x01020304", "0xdeadbee"),
            ("0x01020304", "0xdeadbeef0"),
            ("0x01020304", "deadbeef"),
        ]
        for selector, calldata in cases:
            with self.subTest(selector=selector, calldata=calldata):
                with self.assertRaises(ValueError):
                    symbolic.target_calldata(selector, calldata)


class ForgeJsonClassificationTests(unittest.TestCase):
    def test_classifies_pass_without_claiming_unbounded_equivalence(self):
        classified = symbolic.classify_forge_json(
            forge_payload(
                {
                    "status": "pass",
                    "incomplete": None,
                    "counterexample": None,
                    "replay": {
                        "required": False,
                        "status": "not_required",
                        "reason": None,
                    },
                }
            )
        )

        self.assertEqual(classified["status"], "no_mismatch_within_bounds")
        self.assertEqual(
            classified["test"], "check_diff_probe(uint256,address)"
        )

    def test_classifies_only_replay_confirmed_counterexample_as_mismatch(self):
        counterexample = {
            "calldata": "0xdeadbeef" + "00" * 64,
            "args": "[0, 0x0000000000000000000000000000000000000000]",
            "raw_args": "[0, 0]",
        }
        classified = symbolic.classify_forge_json(
            forge_payload(
                {
                    "status": "fail_counterexample",
                    "incomplete": None,
                    "counterexample": counterexample,
                    "replay": {
                        "required": True,
                        "status": "confirmed",
                        "reason": None,
                    },
                    "artifact": {
                        "schema": "foundry:symbolic.counterexample@v1",
                        "path": "/tmp/counterexample.json",
                    },
                }
            )
        )

        self.assertEqual(classified["status"], "replay_confirmed_mismatch")
        self.assertEqual(classified["counterexample"], counterexample)
        self.assertEqual(
            classified["artifact"]["path"], "/tmp/counterexample.json"
        )

    def test_preserves_incomplete_reason(self):
        incomplete = {
            "kind": "timeout",
            "reason": "symbolic execution exceeded its path budget",
        }
        classified = symbolic.classify_forge_json(
            forge_payload(
                {
                    "status": "incomplete",
                    "incomplete": incomplete,
                    "counterexample": None,
                    "replay": {
                        "required": False,
                        "status": "not_required",
                        "reason": None,
                    },
                }
            )
        )

        self.assertEqual(classified["status"], "incomplete")
        self.assertEqual(classified["incomplete"], incomplete)

    def test_rejects_malformed_or_non_symbolic_forge_results(self):
        inconsistent_pass = forge_payload(
            {
                "status": "pass",
                "incomplete": None,
                "counterexample": None,
                "replay": {"required": False, "status": "not_required"},
            }
        )
        next(iter(inconsistent_pass.values()))["test_results"][
            "check_diff_probe(uint256,address)"
        ]["status"] = "Failure"
        malformed_payloads = [
            {},
            {"suite": {"test_results": {"test()": {"status": "Success"}}}},
            {"suite": {"test_results": []}},
            forge_payload(
                {
                    "status": "pass",
                    "counterexample": None,
                    "replay": {"status": "not_required"},
                    "bounds": None,
                }
            ),
            forge_payload(
                {
                    "status": "pass",
                    "counterexample": None,
                    "replay": {"status": "not_required"},
                    "assumptions": "unexpected",
                }
            ),
            inconsistent_pass,
            forge_payload(
                {
                    "status": "fail_counterexample",
                    "counterexample": {"calldata": "0xdeadbeef"},
                    "replay": {
                        "required": True,
                        "status": "mismatch",
                        "reason": "concrete replay did not reproduce",
                    },
                }
            ),
            forge_payload(
                {
                    "status": "fail_counterexample",
                    "counterexample": {"calldata": "0xnot-hex"},
                    "replay": {
                        "required": True,
                        "status": "confirmed",
                        "reason": None,
                    },
                    "artifact": {
                        "schema": "foundry:symbolic.counterexample@v1",
                        "path": "/tmp/counterexample.json",
                    },
                }
            ),
        ]
        for payload in malformed_payloads:
            with self.subTest(payload=payload):
                with self.assertRaises(ValueError):
                    symbolic.classify_forge_json(payload)


class ConcreteOutcomeConfirmationTests(unittest.TestCase):
    def test_equal_success_or_revert_outcomes_do_not_confirm_mismatch(self):
        outcomes = [
            {"status": "ok", "data": "0x" + "00" * 32},
            {"status": "revert", "data": "0x08c379a0"},
        ]
        for outcome in outcomes:
            with self.subTest(outcome=outcome):
                self.assertFalse(
                    symbolic.confirm_outcomes(outcome, copy.deepcopy(outcome))
                )

    def test_status_or_exact_bytes_difference_confirms_mismatch(self):
        cases = [
            (
                {"status": "ok", "data": "0x"},
                {"status": "revert", "data": "0x"},
            ),
            (
                {"status": "ok", "data": "0x" + "00" * 32},
                {"status": "ok", "data": "0x" + "00" * 31 + "01"},
            ),
            (
                {"status": "revert", "data": "0x08c379a0"},
                {"status": "revert", "data": "0x4e487b71"},
            ),
        ]
        for solc_outcome, solar_outcome in cases:
            with self.subTest(solc=solc_outcome, solar=solar_outcome):
                self.assertTrue(
                    symbolic.confirm_outcomes(solc_outcome, solar_outcome)
                )

    def test_rejects_malformed_concrete_outcome(self):
        malformed = [
            {},
            {"status": "ok"},
            {"status": "unknown", "data": "0x"},
            {"status": "ok", "data": "not-hex"},
        ]
        valid = {"status": "ok", "data": "0x"}
        for outcome in malformed:
            with self.subTest(outcome=outcome):
                with self.assertRaises(ValueError):
                    symbolic.confirm_outcomes(outcome, valid)

    def test_independent_replay_swaps_runtimes_at_one_address(self):
        manager = MagicMock()
        manager.__enter__.return_value = {
            "rpc_url": "http://anvil",
            "chain_id": 123,
            "command": ["anvil", "--chain-id", "123"],
        }
        proxy = {
            "runtime": "0x6000",
            "standard_input": {"json": "{}"},
            "standard_input_sha256": "00" * 32,
            "version": "solc",
            "command": ["solc"],
            "settings": {},
        }
        with (
            patch.object(
                evm, "compile_standard_artifact", return_value=proxy
            ),
            patch.object(symbolic, "_anvil", return_value=manager),
            patch.object(
                evm,
                "rpc",
                return_value={"result": {"number": "0x0", "timestamp": "0x1"}},
            ),
            patch.object(evm, "set_code") as set_code,
            patch.object(
                evm,
                "eth_call",
                side_effect=[
                    {"status": "ok", "data": "0x01"},
                    {"status": "ok", "data": "0x02"},
                ],
            ) as eth_call,
        ):
            replay = symbolic.run_direct_replay(
                "solc",
                "anvil",
                "osaka",
                "0x6001",
                "0x6002",
                "0xdeadbeef",
                30,
            )

        self.assertEqual(
            [call.args[1] for call in set_code.call_args_list],
            [evm.STATIC_PROXY_ADDRESS, evm.SOLC_ADDRESS, evm.SOLC_ADDRESS],
        )
        self.assertEqual(
            [call.args[2] for call in set_code.call_args_list],
            ["0x6000", "0x6001", "0x6002"],
        )
        self.assertEqual(eth_call.call_args_list[0].args[2][:42], "0x" + evm.SOLC_ADDRESS[2:])
        self.assertEqual(eth_call.call_args_list[0].args[2], eth_call.call_args_list[1].args[2])
        self.assertEqual(replay["implementation_address"], evm.SOLC_ADDRESS)
        self.assertEqual(replay["anvil"]["chain_id"], 123)


class DurableArtifactValidationTests(unittest.TestCase):
    def test_requires_matching_confirmed_single_call(self):
        calldata = "0xdeadbeef" + "00" * 32
        artifact = {
            "schema": "foundry:symbolic.counterexample@v1",
            "replay": {"status": "confirmed"},
            "test": {"test": "check_diff_probe(uint256)"},
            "calls": [{"calldata": calldata}],
        }

        self.assertTrue(
            symbolic.counterexample_artifact_matches(
                artifact, "check_diff_probe(uint256)", calldata
            )
        )
        for mutation in [
            ("replay", {"status": "mismatch"}),
            ("replay", None),
            ("test", {"test": "other(uint256)"}),
            ("calls", [{"calldata": "0xdeadbeef"}]),
            ("calls", [None]),
            ("calls", []),
        ]:
            with self.subTest(mutation=mutation):
                changed = copy.deepcopy(artifact)
                changed[mutation[0]] = mutation[1]
                self.assertFalse(
                    symbolic.counterexample_artifact_matches(
                        changed, "check_diff_probe(uint256)", calldata
                    )
                )

    def test_malformed_durable_replay_report_does_not_reproduce(self):
        reports = [
            {"suite": {"test_results": []}},
            {
                "suite": {
                    "test_results": {
                        "check_diff_probe(uint256)": {
                            "status": "Failure",
                            "kind": {"Unit": {}},
                            "counterexample": None,
                        }
                    }
                }
            },
        ]
        for report in reports:
            with self.subTest(report=report):
                self.assertFalse(
                    symbolic.unit_replay_reproduced(
                        report, "check_diff_probe(uint256)", "0xdeadbeef"
                    )
                )


class ManifestPersistenceTests(unittest.TestCase):
    def _args(self, source: Path, artifact_dir: Path) -> argparse.Namespace:
        return argparse.Namespace(
            source=source,
            contract="Root",
            signature="probe(uint256)",
            solc="solc",
            solar="solar",
            forge="forge",
            anvil="anvil",
            timeout=60.0,
            evm_version="osaka",
            symbolic_solver="z3",
            symbolic_timeout=5,
            symbolic_max_paths=16,
            symbolic_max_depth=None,
            max_returndata_bytes=256,
            artifact_dir=artifact_dir,
            verbose=False,
        )

    def test_setup_error_uses_stable_v1_shape_and_snapshotted_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "Root.sol"
            source.write_text("contract Root {}", encoding="utf-8")
            standard_input = evm._single_source_standard_input(source, "osaka")
            args = self._args(source, root / "artifacts")
            args._standard_input = standard_input
            source.unlink()
            output = io.StringIO()

            with redirect_stdout(output):
                returncode = run_foundry_target._symbolic_setup_incomplete(
                    args, ValueError("invalid signature")
                )

            summary = json.loads(output.getvalue())
            bundle = Path(summary["artifact_dir"])
            manifest = json.loads((bundle / "manifest.json").read_text())
            saved_source = (bundle / "source" / "Root.sol").read_text()

        self.assertEqual(returncode, 2)
        self.assertEqual(set(manifest), MANIFEST_KEYS)
        self.assertIsInstance(manifest["source"], dict)
        self.assertEqual(manifest["status"], "incomplete")
        self.assertEqual(saved_source, "contract Root {}")

    def test_campaign_setup_error_keeps_the_campaign_schema(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "Root.sol"
            source.write_text("contract Root {}", encoding="utf-8")
            args = self._args(source, root / "artifacts")
            args.signature = None
            output = io.StringIO()

            with redirect_stdout(output):
                returncode = run_foundry_target._symbolic_setup_incomplete(
                    args, OSError("missing compiler")
                )

            summary = json.loads(output.getvalue())
            manifest = json.loads(
                (
                    Path(summary["artifact_dir"]) / "manifest.json"
                ).read_text()
            )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["schema"], symbolic.CAMPAIGN_SCHEMA)
        self.assertEqual(manifest["schema"], symbolic.CAMPAIGN_SCHEMA)
        self.assertEqual(set(manifest), CAMPAIGN_MANIFEST_KEYS)
        self.assertEqual(manifest["counts"]["selection_errors"], 1)
        self.assertFalse(manifest["campaign_complete"])

    def test_provisional_bundle_never_claims_a_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "Root.sol"
            source.write_text("contract Root {}", encoding="utf-8")
            standard_input = evm._single_source_standard_input(source, "osaka")
            source.write_text("contract Changed {}", encoding="utf-8")
            project = root / "project"
            project.mkdir()
            (project / "foundry.toml").write_text("[profile.default]\n")
            manifest = {
                "status": "replay_confirmed_mismatch",
                "reason": None,
                "function": {"selector": "0xdeadbeef"},
                "forge": {"artifact": None},
                "artifacts": {
                    "project": {"path": "project", "sha256": None},
                    "source": None,
                    "standard_input": None,
                    "foundry_counterexample": None,
                    "static_call_proxy_standard_input": None,
                    "static_call_proxy_runtime": None,
                },
            }

            bundle = run_foundry_target._persist_bundle(
                root / "artifacts",
                source,
                standard_input,
                project,
                manifest,
                {"report": None, "stdout": "", "stderr": ""},
                None,
                None,
            )
            provisional = json.loads((bundle / "manifest.json").read_text())
            saved_source = (bundle / "source" / "Root.sol").read_text()

        self.assertEqual(provisional["status"], "incomplete")
        self.assertIn("pending", provisional["reason"])
        self.assertEqual(saved_source, "contract Root {}")
        self.assertIsNotNone(manifest["artifacts"]["project"]["sha256"])

    def test_forge_environment_removes_ambient_configuration(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary) / "home"
            with patch.dict(
                os.environ,
                {
                    "FOUNDRY_EVM_VERSION": "homestead",
                    "FOUNDRY_PROFILE": "hostile",
                    "DAPP_TEST_DEPTH": "1",
                    "SVM_HOME": "/hostile/svm",
                    "HOME": "/hostile/home",
                    "PATH": os.environ["PATH"],
                },
            ):
                env = run_foundry_target._forge_environment(home)

        self.assertNotIn("FOUNDRY_EVM_VERSION", env)
        self.assertNotIn("FOUNDRY_PROFILE", env)
        self.assertNotIn("DAPP_TEST_DEPTH", env)
        self.assertNotIn("SVM_HOME", env)
        self.assertEqual(env["HOME"], str(home))
        self.assertEqual(env["XDG_CONFIG_HOME"], str(home / ".config"))
        self.assertIn("PATH", env)

        with patch.dict(os.environ, {"ANVIL_IP_ADDR": "0.0.0.0"}):
            anvil_env = symbolic._anvil_environment()
        self.assertNotIn("ANVIL_IP_ADDR", anvil_env)

    def test_symbolic_lane_rejects_platforms_without_process_tree_isolation(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self._args(
                Path(temporary) / "Root.sol", Path(temporary) / "artifacts"
            )
            with (
                patch.object(run_foundry_target.os, "name", "nt"),
                self.assertRaisesRegex(ValueError, "Linux or macOS"),
            ):
                run_foundry_target._run_symbolic(args)


class CampaignAggregationTests(unittest.TestCase):
    @staticmethod
    def _function(name: str, selector: str) -> dict[str, object]:
        return {
            "signature": f"{name}(uint256)",
            "selector": selector,
            "inputs": ["uint256"],
            "outputs": ["uint256"],
        }

    @staticmethod
    def _manifest(records: list[dict[str, object]]) -> dict[str, object]:
        return {
            "status": "incomplete",
            "reason": "campaign is still running",
            "bounds": {},
            "functions": records,
            "not_run": [],
            "findings": [],
            "counts": {},
            "all_eligible_completed": False,
            "campaign_complete": False,
        }

    @staticmethod
    def _record(
        function: dict[str, object], status: str
    ) -> dict[str, object]:
        return {
            "signature": function["signature"],
            "selector": function["selector"],
            "status": status,
            "reason": None,
            "artifact_dir": f"functions/{function['selector']}",
            "manifest": f"functions/{function['selector']}/manifest.json",
            "manifest_sha256": "00" * 32,
        }

    def test_confirmed_mismatch_dominates_an_incomplete_child(self):
        first = self._function("first", "0x00000001")
        second = self._function("second", "0x00000002")
        inventory = {
            "eligible": [first, second],
            "excluded": [],
            "errors": [],
        }
        manifest = self._manifest(
            [
                self._record(first, "replay_confirmed_mismatch"),
                self._record(second, "incomplete"),
            ]
        )
        deadline = Mock()
        deadline.elapsed.return_value = 1.0

        run_foundry_target._refresh_campaign_manifest(
            manifest,
            inventory,
            deadline,
            final=True,
            deadline_reason=None,
        )

        self.assertEqual(manifest["status"], "replay_confirmed_mismatch")
        self.assertEqual(manifest["counts"]["mismatches"], 1)
        self.assertEqual(manifest["counts"]["incomplete"], 1)
        self.assertFalse(manifest["campaign_complete"])

    def test_deadline_leaves_deterministic_not_run_inventory(self):
        first = self._function("first", "0x00000001")
        second = self._function("second", "0x00000002")
        inventory = {
            "eligible": [first, second],
            "excluded": [],
            "errors": [],
        }
        manifest = self._manifest(
            [self._record(first, "no_mismatch_within_bounds")]
        )
        deadline = Mock()
        deadline.elapsed.return_value = 10.0

        run_foundry_target._refresh_campaign_manifest(
            manifest,
            inventory,
            deadline,
            final=True,
            deadline_reason="campaign deadline expired",
        )

        self.assertEqual(manifest["status"], "incomplete")
        self.assertEqual(
            [item["signature"] for item in manifest["not_run"]],
            ["second(uint256)"],
        )
        self.assertEqual(
            manifest["not_run"][0]["reason"], "campaign deadline expired"
        )

    def test_in_progress_function_is_attempted_but_not_completed(self):
        first = self._function("first", "0x00000001")
        second = self._function("second", "0x00000002")
        inventory = {
            "eligible": [first, second],
            "excluded": [],
            "errors": [],
        }
        manifest = self._manifest(
            [
                self._record(first, "no_mismatch_within_bounds"),
                self._record(second, "in_progress"),
            ]
        )
        deadline = Mock()
        deadline.elapsed.return_value = 2.0

        run_foundry_target._refresh_campaign_manifest(
            manifest,
            inventory,
            deadline,
            final=True,
            deadline_reason=None,
        )

        self.assertEqual(manifest["status"], "incomplete")
        self.assertEqual(manifest["counts"]["attempted"], 2)
        self.assertEqual(manifest["counts"]["completed"], 1)
        self.assertEqual(manifest["counts"]["in_progress"], 1)
        self.assertEqual(manifest["not_run"], [])
        self.assertFalse(manifest["all_eligible_completed"])
        self.assertFalse(manifest["campaign_complete"])

    def test_final_deadline_prevents_a_clean_campaign_pass(self):
        function = self._function("probe", "0x00000001")
        inventory = {
            "eligible": [function],
            "excluded": [],
            "errors": [],
        }
        manifest = self._manifest(
            [self._record(function, "no_mismatch_within_bounds")]
        )
        deadline = Mock()
        deadline.elapsed.return_value = 10.1

        run_foundry_target._refresh_campaign_manifest(
            manifest,
            inventory,
            deadline,
            final=True,
            deadline_reason="campaign deadline expired",
        )

        self.assertEqual(manifest["status"], "incomplete")
        self.assertTrue(manifest["all_eligible_completed"])
        self.assertFalse(manifest["campaign_complete"])
        self.assertEqual(manifest["reason"], "campaign deadline expired")

    def test_final_deadline_does_not_suppress_a_confirmed_mismatch(self):
        function = self._function("probe", "0x00000001")
        inventory = {
            "eligible": [function],
            "excluded": [],
            "errors": [],
        }
        manifest = self._manifest(
            [self._record(function, "replay_confirmed_mismatch")]
        )
        deadline = Mock()
        deadline.elapsed.return_value = 10.1

        run_foundry_target._refresh_campaign_manifest(
            manifest,
            inventory,
            deadline,
            final=True,
            deadline_reason="campaign deadline expired",
        )

        self.assertEqual(manifest["status"], "replay_confirmed_mismatch")
        self.assertTrue(manifest["all_eligible_completed"])
        self.assertFalse(manifest["campaign_complete"])

    def test_campaign_allocations_carry_unused_wall_time_forward(self):
        functions = [
            self._function("first", "0x00000001"),
            self._function("second", "0x00000002"),
            self._function("third", "0x00000003"),
        ]
        inventory = {
            "eligible": functions,
            "excluded": [],
            "errors": [],
        }

        class ScriptedDeadline:
            def __init__(self):
                self.remaining_wall = iter([90.0, 60.0, 45.0])

            def remaining(self, operation):
                if operation.startswith("symbolic campaign function"):
                    return next(self.remaining_wall)
                return 1.0

            @staticmethod
            def elapsed():
                return 1.0

        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "campaign"
            functions_root = bundle / "functions"
            functions_root.mkdir(parents=True)
            manifest = self._manifest([])
            manifest["schema"] = symbolic.CAMPAIGN_SCHEMA
            manifest["counts"] = {}
            manifest["bounds"] = {}
            allocations = []

            def run_child(
                _args,
                child_root,
                _source,
                _standard_input,
                _solc_artifact,
                _solar_artifact,
                _function,
                deadline,
                *,
                emit_summary,
                bundle_name,
            ):
                self.assertFalse(emit_summary)
                allocations.append(deadline.total_seconds)
                child_bundle = child_root / bundle_name
                child_bundle.mkdir()
                (child_bundle / "manifest.json").write_text(
                    json.dumps(
                        {
                            "status": "no_mismatch_within_bounds",
                            "reason": None,
                            "bounds": {"elapsed_wall_seconds": 0.1},
                        }
                    )
                )
                return 0

            args = argparse.Namespace(
                timeout=90.0,
                contract="Contract",
                verbose=False,
            )
            output = io.StringIO()
            with (
                patch.object(
                    run_foundry_target,
                    "_create_campaign_bundle",
                    return_value=(bundle, manifest),
                ),
                patch.object(
                    run_foundry_target,
                    "_run_symbolic_function",
                    side_effect=run_child,
                ),
                redirect_stdout(output),
            ):
                returncode = run_foundry_target._run_symbolic_campaign(
                    args,
                    Path(temporary),
                    Path(temporary) / "Contract.sol",
                    {},
                    {},
                    {},
                    ScriptedDeadline(),
                    inventory,
                )

        self.assertEqual(returncode, 0)
        self.assertEqual(allocations, [30.0, 30.0, 45.0])

    def test_second_function_bounds_use_its_own_elapsed_time(self):
        campaign_deadline = Mock()
        campaign_deadline.elapsed.return_value = 47.0
        function_deadline = Mock()
        function_deadline.elapsed.return_value = 0.25
        args = argparse.Namespace(
            timeout=90.0,
            symbolic_timeout=1.0,
            symbolic_max_paths=32,
            symbolic_max_depth=16,
            max_returndata_bytes=4096,
            _deadline=campaign_deadline,
            _function_timeout=30.0,
            _campaign_timeout=90.0,
        )

        bounds = run_foundry_target._bounds_manifest(
            args, None, function_deadline
        )

        self.assertEqual(bounds["total_wall_timeout_seconds"], 30.0)
        self.assertEqual(bounds["campaign_total_wall_timeout_seconds"], 90.0)
        self.assertEqual(bounds["elapsed_wall_seconds"], 0.25)


@unittest.skipUnless(
    os.environ.get("FANDANGO_SYMBOLIC_E2E") == "1",
    "set FANDANGO_SYMBOLIC_E2E=1 to run compiler/Forge/Anvil integration tests",
)
class SymbolicDifferentialIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.solc = os.environ.get("FANDANGO_SOLC", "solc")
        cls.solar = os.environ.get("FANDANGO_SOLAR", "target/debug/solar")
        cls.forge = os.environ.get("FANDANGO_FORGE", "forge")
        cls.anvil = os.environ.get("FANDANGO_ANVIL", "anvil")
        cls.z3 = os.environ.get("FANDANGO_Z3", "z3")
        help_result = subprocess.run(
            [cls.forge, "test", "--help"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if "--symbolic" not in help_result.stdout:
            raise unittest.SkipTest("installed Forge does not support --symbolic")

        fixture_dir = Path(__file__).parent / "symbolic-fixtures"
        cls.reference = (fixture_dir / "ControlledReference.sol").resolve()
        cls.mutant = (fixture_dir / "ControlledMutant.sol").resolve()
        cls.address_context = (
            fixture_dir / "AddressContextReference.sol"
        ).resolve()
        cls.calldata_context = (
            fixture_dir / "CalldataContextReference.sol"
        ).resolve()
        cls.calldata_mutant = (
            fixture_dir / "CalldataContextMutant.sol"
        ).resolve()
        cls.imported = (fixture_dir / "imported" / "ImportedReference.sol").resolve()
        cls.dynamic_input_reference = (
            fixture_dir / "DynamicInputReference.sol"
        ).resolve()
        cls.dynamic_input_mutant = (
            fixture_dir / "DynamicInputMutant.sol"
        ).resolve()
        cls.tuple_input_reference = (
            fixture_dir / "TupleInputReference.sol"
        ).resolve()
        cls.tuple_input_mutant = (
            fixture_dir / "TupleInputMutant.sol"
        ).resolve()
        cls.reference_input = evm.materialize_standard_input(
            cls.solc,
            cls.reference,
            30,
            "osaka",
        )
        cls.solc_reference = evm.compile_standard_artifact(
            cls.solc,
            cls.reference,
            "ControlledDifferential",
            30,
            kind="solc",
            evm_version="osaka",
            standard_input=cls.reference_input,
        )
        cls.solar_reference = evm.compile_standard_artifact(
            cls.solar,
            cls.reference,
            "ControlledDifferential",
            30,
            kind="solar",
            evm_version="osaka",
            standard_input=cls.reference_input,
        )
        mutant_input = evm.materialize_standard_input(
            cls.solc,
            cls.mutant,
            30,
            "osaka",
        )
        cls.solar_mutant = evm.compile_standard_artifact(
            cls.solar,
            cls.mutant,
            "ControlledDifferential",
            30,
            kind="solar",
            evm_version="osaka",
            standard_input=mutant_input,
        )
        cls.dynamic_input_reference_input = evm.materialize_standard_input(
            cls.solc,
            cls.dynamic_input_reference,
            30,
            "osaka",
        )
        cls.solc_dynamic_input_reference = evm.compile_standard_artifact(
            cls.solc,
            cls.dynamic_input_reference,
            "DynamicInputDifferential",
            30,
            kind="solc",
            evm_version="osaka",
            standard_input=cls.dynamic_input_reference_input,
        )
        dynamic_input_mutant = evm.materialize_standard_input(
            cls.solc,
            cls.dynamic_input_mutant,
            30,
            "osaka",
        )
        cls.solar_dynamic_input_mutant = evm.compile_standard_artifact(
            cls.solar,
            cls.dynamic_input_mutant,
            "DynamicInputDifferential",
            30,
            kind="solar",
            evm_version="osaka",
            standard_input=dynamic_input_mutant,
        )
        cls.tuple_input_reference_input = evm.materialize_standard_input(
            cls.solc,
            cls.tuple_input_reference,
            30,
            "osaka",
        )
        cls.solc_tuple_input_reference = evm.compile_standard_artifact(
            cls.solc,
            cls.tuple_input_reference,
            "TupleInputDifferential",
            30,
            kind="solc",
            evm_version="osaka",
            standard_input=cls.tuple_input_reference_input,
        )
        tuple_input_mutant = evm.materialize_standard_input(
            cls.solc,
            cls.tuple_input_mutant,
            30,
            "osaka",
        )
        cls.solar_tuple_input_mutant = evm.compile_standard_artifact(
            cls.solar,
            cls.tuple_input_mutant,
            "TupleInputDifferential",
            30,
            kind="solar",
            evm_version="osaka",
            standard_input=tuple_input_mutant,
        )
        cls.address_context_input = evm.materialize_standard_input(
            cls.solc,
            cls.address_context,
            30,
            "osaka",
        )
        cls.solc_address_context = evm.compile_standard_artifact(
            cls.solc,
            cls.address_context,
            "AddressContextDifferential",
            30,
            kind="solc",
            evm_version="osaka",
            standard_input=cls.address_context_input,
        )
        cls.calldata_context_input = evm.materialize_standard_input(
            cls.solc,
            cls.calldata_context,
            30,
            "osaka",
        )
        cls.solc_calldata_context = evm.compile_standard_artifact(
            cls.solc,
            cls.calldata_context,
            "CalldataContextDifferential",
            30,
            kind="solc",
            evm_version="osaka",
            standard_input=cls.calldata_context_input,
        )
        calldata_mutant_input = evm.materialize_standard_input(
            cls.solc,
            cls.calldata_mutant,
            30,
            "osaka",
        )
        cls.solar_calldata_mutant = evm.compile_standard_artifact(
            cls.solar,
            cls.calldata_mutant,
            "CalldataContextDifferential",
            30,
            kind="solar",
            evm_version="osaka",
            standard_input=calldata_mutant_input,
        )
        cls.static_proxy = evm.compile_standard_artifact(
            cls.solc,
            (Path(__file__).parent / "StaticCallProxy.sol").resolve(),
            "FandangoStaticCallProxy",
            30,
            kind="solc",
            evm_version="osaka",
        )

    def _artifact_root(self, label: str) -> Path:
        configured = os.environ.get("FANDANGO_SYMBOLIC_ARTIFACT_DIR")
        if configured:
            root = Path(configured).resolve() / self._testMethodName / label
            root.mkdir(parents=True, exist_ok=True)
            return root
        temporary = tempfile.TemporaryDirectory(prefix=f"solar-symbolic-{label}-")
        self.addCleanup(temporary.cleanup)
        return Path(temporary.name)

    def _run(
        self,
        solar_artifact,
        max_paths=512,
        max_returndata_bytes=256,
        signature="probe(uint256)",
        *,
        source=None,
        contract="ControlledDifferential",
        solc_artifact=None,
        materialized=None,
        dynamic_lengths=symbolic.DEFAULT_SYMBOLIC_DYNAMIC_LENGTHS,
        catch_setup_errors=False,
    ):
        output = io.StringIO()
        source = source or self.reference
        solc_artifact = solc_artifact or self.solc_reference
        materialized = copy.deepcopy(materialized or self.reference_input)
        artifacts = [
            copy.deepcopy(solc_artifact),
            copy.deepcopy(solar_artifact),
            copy.deepcopy(self.static_proxy),
        ]
        # The controlled mutant is injected only as a test oracle. The product
        # path always gives both compilers the same materialized input.
        for artifact in artifacts[:2]:
            artifact["standard_input"] = materialized
            artifact["standard_input_sha256"] = materialized["sha256"]
            artifact["settings"] = materialized["settings"]
        args = argparse.Namespace(
            source=source,
            contract=contract,
            solc=self.solc,
            solar=self.solar,
            forge=self.forge,
            anvil=self.anvil,
            fuzz_runs=64,
            timeout=60.0,
            symbolic=True,
            signature=signature,
            evm_version="osaka",
            symbolic_solver=self.z3,
            symbolic_timeout=5,
            symbolic_max_paths=max_paths,
            symbolic_max_depth=None,
            symbolic_dynamic_lengths=dynamic_lengths,
            max_returndata_bytes=max_returndata_bytes,
            artifact_dir=self._artifact_root("runs"),
            verbose=False,
        )
        with (
            patch.object(
                run_foundry_target.evm,
                "materialize_standard_input",
                return_value=materialized,
            ),
            patch.object(
                run_foundry_target.evm,
                "compile_standard_artifact",
                side_effect=artifacts,
            ),
            redirect_stdout(output),
        ):
            runner = (
                run_foundry_target._run_symbolic_or_incomplete
                if catch_setup_errors
                else run_foundry_target._run_symbolic
            )
            returncode = runner(args)
        summary = json.loads(output.getvalue())
        manifest = json.loads(
            (Path(summary["artifact_dir"]) / "manifest.json").read_text()
        )
        return returncode, summary, manifest

    def test_equivalent_fixture_has_bounded_no_mismatch_status(self):
        returncode, summary, manifest = self._run(self.solar_reference)

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(manifest["forge"]["symbolic_status"], "pass")
        self.assertEqual(set(manifest), MANIFEST_KEYS)
        self.assertTrue(manifest["tools"]["forge"])
        self.assertEqual(manifest["forge"]["command"][3], "project")

    def test_campaign_scans_every_eligible_function(self):
        returncode, summary, manifest = self._run(
            self.solar_reference, signature=None
        )
        bundle = Path(summary["artifact_dir"])

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["schema"], symbolic.CAMPAIGN_SCHEMA)
        self.assertEqual(set(manifest), CAMPAIGN_MANIFEST_KEYS)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertTrue(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["eligible"], 2)
        self.assertEqual(manifest["counts"]["no_mismatch"], 2)
        self.assertEqual(
            [item["signature"] for item in manifest["functions"]],
            ["fixedArray(uint256[2])", "probe(uint256)"],
        )
        self.assertEqual(manifest["not_run"], [])
        self.assertTrue((bundle / "solc-runtime.hex").is_file())
        self.assertTrue((bundle / "solar-runtime.hex").is_file())
        for function in manifest["functions"]:
            child = bundle / function["artifact_dir"]
            self.assertEqual(
                run_foundry_target._file_sha256(child / "manifest.json"),
                function["manifest_sha256"],
            )

    def test_campaign_aggregates_a_durable_finding_and_a_pass(self):
        returncode, summary, manifest = self._run(
            self.solar_mutant, signature=None
        )
        bundle = Path(summary["artifact_dir"])

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertTrue(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["mismatches"], 1)
        self.assertEqual(manifest["counts"]["no_mismatch"], 1)
        self.assertEqual(
            manifest["findings"][0]["signature"], "probe(uint256)"
        )
        finding = bundle / manifest["findings"][0]["artifact_dir"]
        child_manifest = json.loads(
            (finding / "manifest.json").read_text(encoding="utf-8")
        )
        self.assertTrue(
            child_manifest["replay"]["durable_foundry_artifact"]["reproduced"]
        )

    def test_campaign_checkpoints_an_in_progress_child_before_execution(self):
        original = run_foundry_target._run_symbolic_function
        observed = []

        def observe_parent(*call_args, **call_kwargs):
            functions_root = call_args[1]
            function = call_args[6]
            parent_manifest = json.loads(
                (functions_root.parent / "manifest.json").read_text(
                    encoding="utf-8"
                )
            )
            record = parent_manifest["functions"][-1]
            observed.append((record["signature"], record["status"]))
            self.assertEqual(record["signature"], function["signature"])
            self.assertEqual(record["status"], "in_progress")
            self.assertEqual(parent_manifest["counts"]["in_progress"], 1)
            return original(*call_args, **call_kwargs)

        with patch.object(
            run_foundry_target,
            "_run_symbolic_function",
            side_effect=observe_parent,
        ):
            returncode, _, manifest = self._run(
                self.solar_reference, signature=None
            )

        self.assertEqual(returncode, 0)
        self.assertEqual(
            observed,
            [
                ("fixedArray(uint256[2])", "in_progress"),
                ("probe(uint256)", "in_progress"),
            ],
        )
        self.assertEqual(manifest["counts"]["attempted"], 2)
        self.assertEqual(manifest["counts"]["completed"], 2)
        self.assertEqual(manifest["counts"]["in_progress"], 0)

    def test_transient_parent_write_failure_cannot_hide_a_durable_finding(self):
        original = run_foundry_target._write_json_atomic
        campaign_writes = 0

        def fail_after_finding(path, value):
            nonlocal campaign_writes
            if value.get("schema") == symbolic.CAMPAIGN_SCHEMA:
                campaign_writes += 1
                if campaign_writes == 5:
                    raise OSError("injected parent persistence failure")
            return original(path, value)

        with patch.object(
            run_foundry_target,
            "_write_json_atomic",
            side_effect=fail_after_finding,
        ):
            returncode, summary, manifest = self._run(
                self.solar_mutant, signature=None
            )

        bundle = Path(summary["artifact_dir"])
        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertEqual(manifest["status"], "replay_confirmed_mismatch")
        self.assertEqual(manifest["counts"]["mismatches"], 1)
        self.assertEqual(
            manifest["findings"][0]["signature"], "probe(uint256)"
        )
        self.assertIn("-all-", bundle.name)
        self.assertNotIn("-all-incomplete-", bundle.name)
        self.assertEqual(
            [entry for entry in bundle.parent.iterdir() if entry.is_dir()],
            [bundle],
        )
        self.assertGreaterEqual(campaign_writes, 6)

    def test_child_manifest_hash_failure_preserves_the_original_finding_bundle(self):
        original = run_foundry_target._file_sha256

        def fail_child_manifest(path):
            if path.name == "manifest.json" and "functions" in path.parts:
                raise OSError("injected child manifest hash failure")
            return original(path)

        with patch.object(
            run_foundry_target,
            "_file_sha256",
            side_effect=fail_child_manifest,
        ):
            returncode, summary, manifest = self._run(
                self.solar_mutant,
                signature=None,
                catch_setup_errors=True,
            )

        bundle = Path(summary["artifact_dir"])
        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertFalse(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["mismatches"], 1)
        self.assertIsNone(manifest["findings"][0]["manifest_sha256"])
        self.assertEqual(
            [entry for entry in bundle.parent.iterdir() if entry.is_dir()],
            [bundle],
        )
        self.assertNotIn("-all-incomplete-", bundle.name)

    def test_deadline_expiring_after_the_last_child_prevents_a_clean_pass(self):
        original = run_foundry_target.evm.Deadline.remaining

        def expire_at_finalization(deadline, operation):
            if operation == "campaign finalization":
                raise TimeoutError("injected campaign deadline expiry")
            return original(deadline, operation)

        with patch.object(
            run_foundry_target.evm.Deadline,
            "remaining",
            autospec=True,
            side_effect=expire_at_finalization,
        ):
            returncode, summary, manifest = self._run(
                self.solar_reference, signature=None
            )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertTrue(summary["all_eligible_completed"])
        self.assertFalse(summary["campaign_complete"])
        self.assertIn("deadline expiry", manifest["reason"])

    def test_campaign_runs_valid_siblings_but_inventory_errors_prevent_a_pass(self):
        solar = copy.deepcopy(self.solar_reference)
        solar["abi"] = [
            entry
            for entry in solar["abi"]
            if entry.get("name") != "fixedArray"
        ]
        solar["method_identifiers"].pop("fixedArray(uint256[2])")

        returncode, summary, manifest = self._run(solar, signature=None)

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertTrue(summary["all_eligible_completed"])
        self.assertFalse(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["completed"], 1)
        self.assertEqual(manifest["counts"]["selection_errors"], 1)
        self.assertEqual(
            manifest["functions"][0]["signature"], "probe(uint256)"
        )

    def test_campaign_mismatch_dominates_an_inventory_error(self):
        solar = copy.deepcopy(self.solar_mutant)
        solar["abi"] = [
            entry
            for entry in solar["abi"]
            if entry.get("name") != "fixedArray"
        ]
        solar["method_identifiers"].pop("fixedArray(uint256[2])")

        returncode, summary, manifest = self._run(solar, signature=None)

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertFalse(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["mismatches"], 1)
        self.assertEqual(manifest["counts"]["selection_errors"], 1)

    def test_campaign_with_no_eligible_function_is_incomplete_without_forge(self):
        solc = copy.deepcopy(self.solc_reference)
        solar = copy.deepcopy(self.solar_reference)
        for compiler in (solc, solar):
            for entry in compiler["abi"]:
                if entry.get("type") == "function":
                    entry["stateMutability"] = "view"

        with patch.object(run_foundry_target, "_forge_symbolic") as forge:
            returncode, summary, manifest = self._run(
                solar,
                signature=None,
                solc_artifact=solc,
            )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertIn("no eligible", summary["reason"])
        self.assertEqual(manifest["counts"]["excluded"], 2)
        forge.assert_not_called()

    def test_focused_wrapper_resolves_the_default_relative_solar_path(self):
        repository = Path(__file__).resolve().parents[2]
        solar = os.path.relpath(
            Path(run_foundry_target._resolve_executable(self.solar)),
            repository,
        )
        if solar != "target/debug/solar":
            self.skipTest("E2E Solar binary is not the focused command default")
        artifacts = self._artifact_root("focused-wrapper")
        result = subprocess.run(
            [
                str(repository / "fuzz" / "bin" / "solsymdiff"),
                "--source",
                str(self.reference),
                "--contract",
                "ControlledDifferential",
                "--signature",
                "probe(uint256)",
                "--solc",
                self.solc,
                "--forge",
                self.forge,
                "--anvil",
                self.anvil,
                "--symbolic-solver",
                self.z3,
                "--artifact-dir",
                artifacts,
                "--timeout",
                "60",
            ],
            cwd=repository,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout)["status"],
            "no_mismatch_within_bounds",
        )

    def test_public_command_scans_the_real_abi_vector_contract(self):
        repository = Path(__file__).resolve().parents[2]
        artifacts = self._artifact_root("abi-vector-campaign")
        result = subprocess.run(
            [
                str(repository / "fuzz" / "bin" / "solsymdiff"),
                "--source",
                str(repository / "fuzz" / "fandango" / "AbiVectorFixture.sol"),
                "--contract",
                "AbiVectorFixture",
                "--solc",
                self.solc,
                "--solar",
                self.solar,
                "--forge",
                self.forge,
                "--anvil",
                self.anvil,
                "--symbolic-solver",
                self.z3,
                "--artifact-dir",
                artifacts,
                "--timeout",
                "60",
                "--symbolic-timeout",
                "5",
            ],
            cwd=repository,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout)
        manifest = json.loads(
            (
                Path(summary["artifact_dir"]) / "manifest.json"
            ).read_text()
        )
        self.assertEqual(summary["schema"], symbolic.CAMPAIGN_SCHEMA)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(manifest["counts"]["eligible"], 6)
        self.assertEqual(manifest["counts"]["excluded"], 5)
        self.assertEqual(manifest["counts"]["no_mismatch"], 6)

    def test_public_command_compares_dynamic_return_data(self):
        repository = Path(__file__).resolve().parents[2]
        artifacts = self._artifact_root("abi-dynamic-return-campaign")
        result = subprocess.run(
            [
                str(repository / "fuzz" / "bin" / "solsymdiff"),
                "--source",
                str(
                    repository
                    / "tests"
                    / "ui"
                    / "codegen"
                    / "lowering"
                    / "abi_encode_bytes.sol"
                ),
                "--contract",
                "AbiEncodeBytes",
                "--solc",
                self.solc,
                "--solar",
                self.solar,
                "--forge",
                self.forge,
                "--anvil",
                self.anvil,
                "--symbolic-solver",
                self.z3,
                "--artifact-dir",
                artifacts,
                "--timeout",
                "60",
                "--symbolic-timeout",
                "5",
            ],
            cwd=repository,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout)
        manifest = json.loads(
            (
                Path(summary["artifact_dir"]) / "manifest.json"
            ).read_text()
        )
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertTrue(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["eligible"], 5)
        self.assertEqual(manifest["counts"]["excluded"], 0)
        self.assertEqual(manifest["counts"]["no_mismatch"], 5)
        encode3 = next(
            item
            for item in manifest["inventory"]["eligible"]
            if item["signature"] == "encode3(uint256,uint256,uint256)"
        )
        self.assertEqual(encode3["outputs"], ["bytes"])

    def test_public_command_compares_a_real_nested_struct_input(self):
        repository = Path(__file__).resolve().parents[2]
        artifacts = self._artifact_root("abi-nested-struct-campaign")
        result = subprocess.run(
            [
                str(repository / "fuzz" / "bin" / "solsymdiff"),
                "--source",
                str(
                    repository
                    / "tests"
                    / "ui"
                    / "codegen"
                    / "lowering"
                    / "nested_static_struct_param.sol"
                ),
                "--contract",
                "NestedStaticStructParam",
                "--solc",
                self.solc,
                "--solar",
                self.solar,
                "--forge",
                self.forge,
                "--anvil",
                self.anvil,
                "--symbolic-solver",
                self.z3,
                "--artifact-dir",
                artifacts,
                "--timeout",
                "60",
                "--symbolic-timeout",
                "5",
            ],
            cwd=repository,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout)
        manifest = json.loads(
            (
                Path(summary["artifact_dir"]) / "manifest.json"
            ).read_text()
        )
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertTrue(summary["campaign_complete"])
        self.assertEqual(manifest["counts"]["eligible"], 1)
        self.assertEqual(manifest["counts"]["no_mismatch"], 1)
        self.assertEqual(
            manifest["inventory"]["eligible"][0]["inputs"],
            ["(uint256,(uint256,uint256),uint256)"],
        )

    def test_unsupported_contract_scopes_are_incomplete_before_forge(self):
        cases = [
            (
                "InlineImmutable",
                (
                    "contract InlineImmutable {"
                    "uint256 public immutable configured = 1;"
                    "function probe(uint256 x) external pure returns (uint256) {"
                    "return x + 1;"
                    "}"
                    "}"
                ),
                "immutable references",
            ),
            (
                "LinkedLibrary",
                (
                    "library ExternalLibrary {"
                    "function add(uint256 x) external pure returns (uint256) {"
                    "return x + 1;"
                    "}"
                    "}"
                    "contract LinkedLibrary {"
                    "function probe(uint256 x) external pure returns (uint256) {"
                    "return ExternalLibrary.add(x);"
                    "}"
                    "}"
                ),
                "unresolved library links",
            ),
            (
                "ExternalPureCall",
                (
                    "interface ClaimedPure {"
                    "function read() external pure returns (uint256);"
                    "}"
                    "contract ExternalPureCall {"
                    "uint256 private value;"
                    "function read() external view returns (uint256) {"
                    "return value;"
                    "}"
                    "function probe(uint256 raw) external pure returns (uint256) {"
                    "return ClaimedPure(address(uint160(raw))).read();"
                    "}"
                    "}"
                ),
                "external-control-flow",
            ),
            (
                "CodeIntrospection",
                (
                    "contract CodeIntrospection {"
                    "function probe(uint256) "
                    "external pure returns (uint256 size) {"
                    "assembly { size := codesize() }"
                    "}"
                    "}"
                ),
                "CODESIZE",
            ),
            (
                "MemoryLayoutIntrospection",
                (
                    "contract MemoryLayoutIntrospection {"
                    "function probe(uint256) "
                    "external pure returns (uint256 pointer) {"
                    "assembly { pointer := mload(0x40) }"
                    "}"
                    "}"
                ),
                "user inline assembly",
            ),
        ]
        for contract, source_text, reason in cases:
            with self.subTest(contract=contract):
                returncode, summary, forge = self._run_rejected_source(
                    contract, source_text
                )
                self.assertEqual(returncode, 2)
                self.assertEqual(summary["status"], "incomplete")
                self.assertIn(reason, summary["reason"])
                forge.assert_not_called()

    def test_fixed_array_arguments_keep_the_target_abi_layout(self):
        returncode, summary, manifest = self._run(
            self.solar_reference, signature="fixedArray(uint256[2])"
        )

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(manifest["function"]["inputs"], ["uint256[2]"])

    def test_dynamic_bytes_shape_and_contents_find_a_durable_mismatch(self):
        returncode, summary, manifest = self._run(
            self.solar_dynamic_input_mutant,
            signature="probeBytes(bytes)",
            source=self.dynamic_input_reference,
            contract="DynamicInputDifferential",
            solc_artifact=self.solc_dynamic_input_reference,
            materialized=self.dynamic_input_reference_input,
        )

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        calldata = bytes.fromhex(
            manifest["replay"]["target_calldata"].removeprefix("0x")
        )
        self.assertEqual(int.from_bytes(calldata[4:36]), 32)
        self.assertEqual(int.from_bytes(calldata[36:68]), 3)
        self.assertEqual(calldata[68:71], b"abc")
        self.assertEqual(
            manifest["bounds"]["dynamic_input_lengths"],
            [0, 1, 2, 3],
        )
        self.assertEqual(
            manifest["bounds"]["forge_effective"]["default_bytes_lengths"],
            [0, 1, 2, 3],
        )

    def test_dynamic_array_shape_and_elements_find_a_durable_mismatch(self):
        returncode, summary, manifest = self._run(
            self.solar_dynamic_input_mutant,
            signature="probeArray(uint256[])",
            source=self.dynamic_input_reference,
            contract="DynamicInputDifferential",
            solc_artifact=self.solc_dynamic_input_reference,
            materialized=self.dynamic_input_reference_input,
        )

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        calldata = bytes.fromhex(
            manifest["replay"]["target_calldata"].removeprefix("0x")
        )
        self.assertEqual(int.from_bytes(calldata[4:36]), 32)
        self.assertEqual(int.from_bytes(calldata[36:68]), 2)
        self.assertEqual(int.from_bytes(calldata[68:100]), 42)
        self.assertEqual(int.from_bytes(calldata[100:132]), 99)
        self.assertEqual(
            manifest["bounds"]["forge_effective"]["default_array_lengths"],
            [0, 1, 2, 3],
        )

    def test_dynamic_length_override_changes_the_explored_shapes(self):
        returncode, summary, manifest = self._run(
            self.solar_dynamic_input_mutant,
            signature="probeBytes(bytes)",
            source=self.dynamic_input_reference,
            contract="DynamicInputDifferential",
            solc_artifact=self.solc_dynamic_input_reference,
            materialized=self.dynamic_input_reference_input,
            dynamic_lengths=(0, 2),
        )

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(
            manifest["bounds"]["forge_effective"]["default_bytes_lengths"],
            [0, 2],
        )

    def test_nested_tuple_values_find_a_durable_mismatch(self):
        returncode, summary, manifest = self._run(
            self.solar_tuple_input_mutant,
            signature="probe((address,(uint256,uint256)))",
            source=self.tuple_input_reference,
            contract="TupleInputDifferential",
            solc_artifact=self.solc_tuple_input_reference,
            materialized=self.tuple_input_reference_input,
        )

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        calldata = bytes.fromhex(
            manifest["replay"]["target_calldata"].removeprefix("0x")
        )
        self.assertEqual(len(calldata), 100)
        self.assertEqual(int.from_bytes(calldata[4:36]), 0x1234)
        self.assertEqual(int.from_bytes(calldata[36:68]), 42)
        self.assertEqual(int.from_bytes(calldata[68:100]), 99)
        self.assertEqual(
            manifest["function"]["inputs"],
            ["(address,(uint256,uint256))"],
        )

    def test_controlled_cold_branch_mismatch_is_independently_replayed(self):
        returncode, summary, manifest = self._run(self.solar_mutant)
        bundle = Path(summary["artifact_dir"])

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertTrue(
            manifest["replay"]["durable_foundry_artifact"]["reproduced"]
        )
        self.assertTrue(manifest["replay"]["target_calldata"].endswith("2a"))
        self.assertNotEqual(manifest["replay"]["solc"], manifest["replay"]["solar"])
        self.assertEqual(
            manifest["replay"]["implementation_address"], evm.SOLC_ADDRESS
        )
        self.assertEqual(
            manifest["replay"]["proxy"]["standard_input_path"],
            "static-call-proxy-standard-input.json",
        )
        self.assertEqual(
            manifest["replay"]["durable_foundry_artifact"]["command"][3],
            "project",
        )
        self.assertEqual(
            manifest["artifacts"]["project"]["sha256"],
            run_foundry_target._tree_sha256(bundle / "project"),
        )
        self.assertIsInstance(manifest["replay"]["anvil"]["block"], dict)

    def test_router_preserves_target_msg_data_and_selector(self):
        returncode, summary, manifest = self._run(
            self.solar_calldata_mutant,
            source=self.calldata_context,
            contract="CalldataContextDifferential",
            solc_artifact=self.solc_calldata_context,
            materialized=self.calldata_context_input,
        )

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertTrue(
            manifest["replay"]["durable_foundry_artifact"]["reproduced"]
        )
        self.assertEqual(
            len(bytes.fromhex(manifest["replay"]["target_calldata"][2:])),
            36,
        )

    def test_external_call_runtime_is_incomplete_before_forge(self):
        with patch.object(run_foundry_target, "_forge_symbolic") as forge:
            returncode, summary, manifest = self._run(
                self.solc_address_context,
                source=self.address_context,
                contract="AddressContextDifferential",
                solc_artifact=self.solc_address_context,
                materialized=self.address_context_input,
                catch_setup_errors=True,
            )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertIn("external-control-flow", summary["reason"])
        self.assertEqual(
            manifest["compilers"]["solc"]["runtime_bytecode_sha256"],
            manifest["compilers"]["solar"]["runtime_bytecode_sha256"],
        )
        forge.assert_not_called()

    def test_campaign_excludes_external_call_runtimes_before_forge(self):
        with patch.object(run_foundry_target, "_forge_symbolic") as forge:
            returncode, summary, manifest = self._run(
                self.solc_address_context,
                signature=None,
                source=self.address_context,
                contract="AddressContextDifferential",
                solc_artifact=self.solc_address_context,
                materialized=self.address_context_input,
            )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["schema"], symbolic.CAMPAIGN_SCHEMA)
        self.assertIn("external-control-flow", summary["reason"])
        self.assertEqual(manifest["counts"]["eligible"], 0)
        self.assertEqual(manifest["counts"]["selection_errors"], 1)
        self.assertEqual(manifest["counts"]["excluded"], 1)
        forge.assert_not_called()

    def test_nonlegacy_and_context_dependent_runtimes_stop_before_forge(self):
        cases = [
            ("0xef00", "EF_PREFIXED_NON_LEGACY_RUNTIME"),
            ("0x3300", "CALLER"),
        ]
        for runtime, reason in cases:
            solar = copy.deepcopy(self.solar_reference)
            solar["runtime"] = runtime
            with (
                self.subTest(runtime=runtime),
                patch.object(run_foundry_target, "_forge_symbolic") as forge,
            ):
                returncode, summary, _ = self._run(
                    solar, catch_setup_errors=True
                )

            self.assertEqual(returncode, 2)
            self.assertEqual(summary["status"], "incomplete")
            self.assertIn(reason, summary["reason"])
            forge.assert_not_called()

    def test_ambient_foundry_configuration_cannot_change_the_oracle(self):
        with tempfile.TemporaryDirectory() as hostile:
            Path(hostile, ".env").write_text(
                "FOUNDRY_EVM_VERSION=homestead\n"
                "ANVIL_IP_ADDR=0.0.0.0\n"
            )
            Path(hostile, ".foundry").mkdir()
            Path(hostile, ".foundry", "foundry.toml").write_text(
                "[profile.default]\n"
                'solc = "/definitely/missing-solc"\n'
                'evm_version = "homestead"\n'
            )
            original_cwd = os.getcwd()
            try:
                os.chdir(hostile)
                with patch.dict(
                    os.environ,
                    {
                        "FOUNDRY_EVM_VERSION": "homestead",
                        "FOUNDRY_PROFILE": "hostile",
                        "FOUNDRY_CODE_SIZE_LIMIT": "1",
                        "ANVIL_IP_ADDR": "0.0.0.0",
                        "HOME": hostile,
                        "XDG_CONFIG_HOME": hostile,
                        "SVM_HOME": str(Path(hostile, "svm")),
                    },
                ):
                    returncode, summary, manifest = self._run(self.solar_mutant)
            finally:
                os.chdir(original_cwd)

        self.assertEqual(returncode, 1)
        self.assertEqual(summary["status"], "replay_confirmed_mismatch")
        self.assertEqual(manifest["settings"]["evmVersion"], "osaka")
        self.assertEqual(
            manifest["forge"]["environment"]["home"],
            "isolated temporary directory",
        )
        self.assertEqual(
            manifest["forge"]["environment"]["solc_from_cli"],
            run_foundry_target._resolve_executable(self.solc),
        )

    def test_direct_replay_timeout_preserves_the_forge_witness(self):
        with patch.object(
            symbolic,
            "run_direct_replay",
            side_effect=subprocess.TimeoutExpired(["solc"], 1),
        ):
            returncode, summary, manifest = self._run(self.solar_mutant)

        bundle = Path(summary["artifact_dir"])
        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertIn("replay failed", summary["reason"])
        self.assertIsNotNone(manifest["forge"]["counterexample"])
        self.assertTrue((bundle / "foundry-counterexample.json").is_file())

    def test_path_exhaustion_is_incomplete(self):
        returncode, summary, manifest = self._run(
            self.solar_reference, max_paths=1
        )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertIn("path limit", manifest["reason"])

    def test_returndata_bound_is_an_incomplete_sentinel(self):
        returncode, summary, manifest = self._run(
            self.solar_reference, max_returndata_bytes=1
        )

        self.assertEqual(returncode, 2)
        self.assertEqual(summary["status"], "incomplete")
        self.assertIn("--max-returndata-bytes", manifest["reason"])
        self.assertEqual(manifest["replay"]["solc"], manifest["replay"]["solar"])
        self.assertFalse(
            manifest["replay"]["durable_foundry_artifact"]["required"]
        )

    def _run_rejected_source(self, contract: str, source_text: str):
        root = self._artifact_root(f"rejected-{contract}")
        source = root / f"{contract}.sol"
        source.write_text(source_text)
        args = argparse.Namespace(
            source=source,
            contract=contract,
            solc=self.solc,
            solar=self.solar,
            forge=self.forge,
            anvil=self.anvil,
            timeout=60.0,
            signature="probe(uint256)",
            evm_version="osaka",
            symbolic_solver=self.z3,
            symbolic_timeout=5,
            symbolic_max_paths=64,
            symbolic_max_depth=None,
            max_returndata_bytes=256,
            artifact_dir=root / "artifacts",
            verbose=False,
        )
        output = io.StringIO()
        with (
            patch.object(run_foundry_target, "_forge_symbolic") as forge,
            redirect_stdout(output),
        ):
            returncode = run_foundry_target._run_symbolic_or_incomplete(args)
        return returncode, json.loads(output.getvalue()), forge

    def test_imported_source_bundle_contains_the_exact_compiler_input(self):
        output = io.StringIO()
        args = argparse.Namespace(
            source=self.imported,
            contract="ImportedDifferential",
            solc=self.solc,
            solar=self.solar,
            forge=self.forge,
            anvil=self.anvil,
            fuzz_runs=64,
            timeout=60.0,
            symbolic=True,
            signature="probe(uint256)",
            evm_version="osaka",
            symbolic_solver=self.z3,
            symbolic_timeout=5,
            symbolic_max_paths=512,
            symbolic_max_depth=None,
            max_returndata_bytes=256,
            artifact_dir=self._artifact_root("imports"),
            verbose=False,
        )
        with redirect_stdout(output):
            returncode = run_foundry_target._run_symbolic(args)

        summary = json.loads(output.getvalue())
        bundle = Path(summary["artifact_dir"])
        manifest = json.loads((bundle / "manifest.json").read_text())
        standard_input_bytes = (bundle / "standard-input.json").read_bytes()
        standard_input = json.loads(standard_input_bytes)
        expected_hash = hashlib.sha256(standard_input_bytes).hexdigest()

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(manifest["standard_input"]["sha256"], expected_hash)
        self.assertEqual(
            set(standard_input["sources"]),
            {"ImportedReference.sol", "ImportedHelper.sol"},
        )
        self.assertIn(
            "library ImportedHelper",
            standard_input["sources"]["ImportedHelper.sol"]["content"],
        )
        for compiler in manifest["compilers"].values():
            self.assertEqual(compiler["standard_input_sha256"], expected_hash)
            self.assertNotIn("--base-path", compiler["command"])

    def test_final_compilers_cannot_load_an_omitted_import_from_cwd(self):
        standard_input = copy.deepcopy(self.reference_input)
        value = json.loads(standard_input["json"])
        root_name = standard_input["root_source"]
        root_content = """\
pragma solidity ^0.8.0;
import {AmbientHelper} from "./Ambient.sol";

contract ControlledDifferential {
    function probe(uint256 input) external pure returns (uint256) {
        return AmbientHelper.identity(input);
    }
}
"""
        value["sources"] = {root_name: {"content": root_content}}
        serialized = json.dumps(
            value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        )
        standard_input["json"] = serialized
        standard_input["sha256"] = hashlib.sha256(serialized.encode()).hexdigest()
        standard_input["sources"] = []

        with tempfile.TemporaryDirectory(prefix="solar-symbolic-hostile-cwd-") as hostile:
            Path(hostile, "Ambient.sol").write_text(
                """\
pragma solidity ^0.8.0;
library AmbientHelper {
    function identity(uint256 input) internal pure returns (uint256) {
        return input;
    }
}
"""
            )
            original_cwd = os.getcwd()
            try:
                os.chdir(hostile)
                for compiler, kind in (
                    (self.solc, "solc"),
                    (self.solar, "solar"),
                ):
                    with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                        evm.compile_standard_artifact(
                            compiler,
                            self.reference,
                            "ControlledDifferential",
                            30,
                            kind=kind,
                            evm_version="osaka",
                            standard_input=standard_input,
                        )
            finally:
                os.chdir(original_cwd)

    def test_independent_replay_preserves_staticcall_context(self):
        # PUSH1 1; PUSH1 0; SSTORE; STOP succeeds in a top-level eth_call but
        # must fail when the proxy enters it through STATICCALL.
        runtime = "0x600160005500"
        replay = symbolic.run_direct_replay(
            self.solc,
            self.anvil,
            "osaka",
            runtime,
            runtime,
            "0x",
            30,
        )

        self.assertEqual(replay["call_kind"], "staticcall")
        self.assertEqual(replay["solc"]["status"], "revert")
        self.assertEqual(replay["solar"]["status"], "revert")


if __name__ == "__main__":
    unittest.main()
