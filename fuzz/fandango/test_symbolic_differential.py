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
                                        "deployedBytecode": {"object": "6000"},
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
        self._assert_wrapper_grandchild_is_reaped(timeout=0.5, wrapper_sleep=60)

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
            wrapper = root / "compiler-wrapper"
            wrapper.write_text(
                "#!/usr/bin/env python3\n"
                "import pathlib\n"
                "import subprocess\n"
                "import sys\n"
                "import time\n"
                "grandchild = subprocess.Popen(\n"
                "    [sys.executable, '-c', 'import time; time.sleep(60)'],\n"
                "    stdin=subprocess.DEVNULL,\n"
                "    stdout=subprocess.DEVNULL,\n"
                "    stderr=subprocess.DEVNULL,\n"
                ")\n"
                f"pathlib.Path({str(pid_file)!r}).write_text(str(grandchild.pid))\n"
                "print('wrapper version', flush=True)\n"
                f"time.sleep({wrapper_sleep})\n"
            )
            wrapper.chmod(0o755)
            grandchild_pid = None
            try:
                if wrapper_sleep:
                    with self.assertRaises(subprocess.TimeoutExpired):
                        evm.compiler_version(str(wrapper), timeout)
                else:
                    self.assertEqual(
                        evm.compiler_version(str(wrapper), timeout),
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
                                            "deployedBytecode": {"object": runtime},
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


class StaticFunctionSelectionTests(unittest.TestCase):
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

    def test_rejects_dynamic_inputs_and_outputs(self):
        dynamic_cases = [
            ([{"name": "value", "type": "bytes"}], None),
            ([{"name": "value", "type": "string"}], None),
            ([{"name": "value", "type": "uint256[]"}], None),
            ([{"name": "value", "type": "bytes[2]"}], None),
            (None, [{"name": "value", "type": "uint256[]"}]),
        ]
        for inputs, outputs in dynamic_cases:
            with self.subTest(inputs=inputs, outputs=outputs):
                solc = artifact(inputs=inputs, outputs=outputs)
                solar = copy.deepcopy(solc)
                signature = next(iter(solc["hashes"]))
                with self.assertRaisesRegex(ValueError, "dynamic|static"):
                    symbolic.select_function(solc, solar, signature)

    def test_rejects_tuple_input_even_when_its_components_are_static(self):
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

        with self.assertRaises(ValueError):
            symbolic.select_function(
                solc, solar, "probe((uint256,address))"
            )

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
        cls.imported = (fixture_dir / "imported" / "ImportedReference.sol").resolve()
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
        cls.static_proxy = evm.compile_standard_artifact(
            cls.solc,
            (Path(__file__).parent / "StaticCallProxy.sol").resolve(),
            "FandangoStaticCallProxy",
            30,
            kind="solc",
            evm_version="osaka",
        )

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
        temporary = tempfile.TemporaryDirectory(prefix="solar-symbolic-e2e-")
        self.addCleanup(temporary.cleanup)
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
            max_returndata_bytes=max_returndata_bytes,
            artifact_dir=Path(temporary.name),
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
            returncode = run_foundry_target._run_symbolic(args)
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

    def test_focused_wrapper_resolves_the_default_relative_solar_path(self):
        repository = Path(__file__).resolve().parents[2]
        solar = os.path.relpath(
            Path(run_foundry_target._resolve_executable(self.solar)),
            repository,
        )
        if solar != "target/debug/solar":
            self.skipTest("E2E Solar binary is not the focused command default")
        with tempfile.TemporaryDirectory(
            prefix="solar-symbolic-wrapper-"
        ) as artifacts:
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

    def test_fixed_array_arguments_keep_the_target_abi_layout(self):
        returncode, summary, manifest = self._run(
            self.solar_reference, signature="fixedArray(uint256[2])"
        )

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(manifest["function"]["inputs"], ["uint256[2]"])

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

    def test_byte_identical_runtimes_share_one_execution_address(self):
        returncode, summary, manifest = self._run(
            self.solc_address_context,
            source=self.address_context,
            contract="AddressContextDifferential",
            solc_artifact=self.solc_address_context,
            materialized=self.address_context_input,
        )

        self.assertEqual(returncode, 0)
        self.assertEqual(summary["status"], "no_mismatch_within_bounds")
        self.assertEqual(
            manifest["compilers"]["solc"]["runtime_bytecode_sha256"],
            manifest["compilers"]["solar"]["runtime_bytecode_sha256"],
        )

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

    def test_imported_source_bundle_contains_the_exact_compiler_input(self):
        output = io.StringIO()
        temporary = tempfile.TemporaryDirectory(prefix="solar-symbolic-imports-")
        self.addCleanup(temporary.cleanup)
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
            artifact_dir=Path(temporary.name),
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
