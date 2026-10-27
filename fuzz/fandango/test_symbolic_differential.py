#!/usr/bin/env python3

from __future__ import annotations

import argparse
import io
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import symbolic_differential as symbolic


def _artifact(
    *,
    mutability: str = "pure",
    inputs: list[dict[str, object]] | None = None,
    selector: str = "b3de648b",
    runtime: str = "0x60006000f3",
) -> dict[str, object]:
    inputs = inputs or [{"name": "value", "type": "uint256"}]
    return {
        "abi": [
            {
                "type": "function",
                "name": "f",
                "inputs": inputs,
                "outputs": [{"name": "", "type": "uint256"}],
                "stateMutability": mutability,
            }
        ],
        "method_identifiers": {
            f"f({','.join(symbolic._canonical_type(item) for item in inputs)})": selector
        },
        "runtime": runtime,
    }


def _forge_report(
    status: str,
    *,
    replay: str = "not_required",
    max_dynamic_length: int = 256,
) -> dict[str, object]:
    symbolic_result: dict[str, object] = {
        "status": status,
        "replay": {"status": replay},
        "bounds": {
            "timeout_seconds": 5,
            "max_paths": 32,
            "max_solver_queries": 100,
            "max_calldata_bytes": 512,
            "max_dynamic_length": max_dynamic_length,
            "max_depth": symbolic.SYMBOLIC_MAX_DEPTH,
            "exploration_order": "bfs",
            "storage_layout": "solidity",
            "default_array_lengths": [0, 1],
            "default_bytes_lengths": [0, 1],
            "dynamic_lengths": {},
        },
        "assumptions": [
            {"kind": "bounded_exploration", "description": "bounded"},
            {"kind": "hash_model", "description": "hashes"},
        ],
    }
    outer_status = "Success" if status == "pass" else "Failure"
    if status == "incomplete":
        symbolic_result["incomplete"] = {"reason": "path limit reached"}
    elif status == "fail_counterexample":
        symbolic_result["counterexample"] = {
            "calldata": "0x12345678" + "00" * 32,
        }
        symbolic_result["artifact"] = {
            "path": "cache/symbolic-counterexample.json",
        }
    test_results: dict[str, object] = {
        "checkSymbolicDifferential(uint256)": {
            "status": outer_status,
            "symbolic": symbolic_result,
        }
    }
    return {
        "test/SymbolicDifferential.t.sol:SymbolicDifferentialTest": {
            "test_results": test_results
        }
    }


class FunctionSelectionTests(unittest.TestCase):
    def test_selects_one_matching_pure_function(self) -> None:
        selected = symbolic._select_function(
            _artifact(),
            _artifact(),
            "f(uint256)",
        )

        self.assertEqual(selected["signature"], "f(uint256)")
        self.assertEqual(selected["selector"], "0xb3de648b")

    def test_view_and_stateful_targets_require_explicit_opt_in(self) -> None:
        with self.assertRaisesRegex(ValueError, "allowed: pure"):
            symbolic._select_function(
                _artifact(mutability="view"),
                _artifact(mutability="view"),
                "f(uint256)",
            )

        view = symbolic._select_function(
            _artifact(mutability="view"),
            _artifact(mutability="view"),
            "f(uint256)",
            include_view=True,
        )
        stateful = symbolic._select_function(
            _artifact(mutability="nonpayable"),
            _artifact(mutability="nonpayable"),
            "f(uint256)",
            include_stateful=True,
        )

        self.assertEqual(view["mutability"], "view")
        self.assertEqual(stateful["mutability"], "nonpayable")

        with self.assertRaisesRegex(ValueError, "allowed: nonpayable, pure"):
            symbolic._select_function(
                _artifact(mutability="payable"),
                _artifact(mutability="payable"),
                "f(uint256)",
                include_stateful=True,
            )
        payable = symbolic._select_function(
            _artifact(mutability="payable"),
            _artifact(mutability="payable"),
            "f(uint256)",
            include_payable=True,
        )
        self.assertEqual(payable["mutability"], "payable")

    def test_rejects_selector_disagreement(self) -> None:

        with self.assertRaisesRegex(ValueError, "selector disagreement"):
            symbolic._select_function(
                _artifact(),
                _artifact(selector="aaaaaaaa"),
                "f(uint256)",
            )

    def test_renders_nested_static_tuple_inputs(self) -> None:
        inputs = [
            {
                "name": "value",
                "type": "tuple[2]",
                "components": [
                    {"name": "x", "type": "uint256"},
                    {
                        "name": "inner",
                        "type": "tuple",
                        "components": [{"name": "ok", "type": "bool"}],
                    },
                ],
            }
        ]

        definitions, declarations = symbolic._solidity_parameters(inputs)

        self.assertIn("struct SymbolicInput0", definitions)
        self.assertIn("struct SymbolicInput0_1", definitions)
        self.assertEqual(
            declarations,
            ["SymbolicInput0[2] calldata arg0"],
        )

    def test_validates_per_input_dynamic_lengths(self) -> None:
        inputs = [{"type": "bytes"}, {"type": "uint256[]"}, {"type": "uint256"}]

        self.assertEqual(
            symbolic._normalize_input_lengths([(0, (0, 32)), (1, (1, 4))], inputs),
            {"arg0": (0, 32), "arg1": (1, 4)},
        )
        with self.assertRaisesRegex(ValueError, "not a top-level dynamic input"):
            symbolic._normalize_input_lengths([(2, (0,))], inputs)

    def test_validates_prefix_calldata(self) -> None:
        self.assertEqual(symbolic._prefix_calldata("0xAa00"), "0xaa00")
        for value in ("", "1234", "0x0", "0xzz"):
            with (
                self.subTest(value=value),
                self.assertRaisesRegex(
                    argparse.ArgumentTypeError,
                    "even-length hexadecimal",
                ),
            ):
                symbolic._prefix_calldata(value)


class ResultClassificationTests(unittest.TestCase):
    def test_classifies_bounded_agreement_and_incomplete(self) -> None:
        self.assertEqual(
            symbolic._classify(_forge_report("pass"), "0xb3de648b"),
            {"status": "bounded_agreement"},
        )
        self.assertEqual(
            symbolic._classify(_forge_report("incomplete"), "0xb3de648b"),
            {
                "status": "incomplete",
                "reason": "path limit reached",
            },
        )

    def test_requires_concrete_replay_for_mismatches(self) -> None:
        result = symbolic._classify(
            _forge_report("fail_counterexample", replay="confirmed"),
            "0xb3de648b",
        )

        self.assertEqual(result["status"], "mismatch")
        self.assertEqual(
            result["counterexample"]["calldata"],
            "0xb3de648b" + "00" * 32,
        )

        with self.assertRaisesRegex(ValueError, "concretely replay"):
            symbolic._classify(
                _forge_report("fail_counterexample"),
                "0xb3de648b",
            )

    def test_counterexample_separates_harness_inputs(self) -> None:
        inputs = [
            {"name": "a", "type": "uint256"},
            {"name": "b", "type": "bytes"},
            {"name": "c", "type": "uint8[2]"},
        ]
        target_args = (
            (7).to_bytes(32)
            + (128).to_bytes(32)
            + (1).to_bytes(32)
            + (2).to_bytes(32)
            + (2).to_bytes(32)
            + bytes.fromhex("abcd").ljust(32, b"\0")
        )
        harness_args = (
            (7).to_bytes(32)
            + (128 + 96).to_bytes(32)
            + (1).to_bytes(32)
            + (2).to_bytes(32)
            + bytes(12)
            + bytes.fromhex("00000000000000000000000000000000000000ca")
            + (5).to_bytes(32)
            + bytes(12)
            + bytes.fromhex("00000000000000000000000000000000000000fe")
            + (2).to_bytes(32)
            + bytes.fromhex("abcd").ljust(32, b"\0")
        )

        calldata, values = symbolic._target_calldata(
            "0xb3de648b",
            "0x12345678" + harness_args.hex(),
            inputs,
            ("caller", "value", "observer"),
        )

        self.assertEqual(calldata, "0xb3de648b" + target_args.hex())
        self.assertEqual(
            values,
            {
                "caller": "0x00000000000000000000000000000000000000ca",
                "value": "5",
                "observer": "0x00000000000000000000000000000000000000fe",
            },
        )

    def test_bounded_agreement_requires_the_requested_forge_bounds(self) -> None:
        expected = {
            "solver_timeout_seconds": 5,
            "max_paths": 32,
            "max_solver_queries": 100,
            "max_calldata_bytes": 512,
            "max_dynamic_length": 256,
            "max_depth": None,
            "exploration_order": "bfs",
            "storage_layout": "solidity",
            "dynamic_lengths": [0, 1],
            "input_lengths": {},
        }
        report = _forge_report("pass")

        self.assertEqual(
            symbolic._classify(report, "0xb3de648b", expected),
            {"status": "bounded_agreement"},
        )
        contract_report = next(iter(report.values()))
        assert isinstance(contract_report, dict)
        symbolic_result = contract_report["test_results"]
        symbolic_result = next(iter(symbolic_result.values()))["symbolic"]
        symbolic_result["bounds"]["max_paths"] = 31
        with self.assertRaisesRegex(ValueError, "max_paths=31"):
            symbolic._classify(report, "0xb3de648b", expected)


class ProjectGenerationTests(unittest.TestCase):
    def test_pure_harness_runs_both_runtimes_at_one_address(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            symbolic._write_project(
                project,
                "0x60006000f3",
                "0x60016000f3",
                {
                    "selector": "0xb3de648b",
                    "inputs": [{"name": "value", "type": "uint256"}],
                    "mutability": "pure",
                },
                "osaka",
                max_dynamic_length=256,
            )

            test_source = (project / "test" / "SymbolicDifferential.t.sol").read_text()
            config = (project / "foundry.toml").read_text()

        self.assertIn("checkSymbolicDifferential(uint256 arg0)", test_source)
        self.assertIn("0xb3de648b", test_source)
        self.assertIn("60006000f3", test_source)
        self.assertIn("60016000f3", test_source)
        self.assertNotIn("function setUp", test_source)
        self.assertNotIn("{gas:", test_source)
        self.assertNotIn("prefixResults", test_source)
        self.assertNotIn("vm.prank", test_source)
        self.assertNotIn("vm.deal", test_source)
        self.assertEqual(test_source.count("target.staticcall(callData)"), 2)
        solc = test_source.index("address target = _deploy(SOLC_CODE)")
        revert = test_source.index("assert(vm.revertToStateAndDelete(snapshot))")
        solar = test_source.index("assert(_deploy(SOLAR_CODE) == target)")
        self.assertLess(test_source.index("vm.snapshotState()"), solc)
        self.assertLess(solc, revert)
        self.assertLess(revert, solar)
        self.assertIn("target := create", test_source)
        self.assertIn('evm_version = "osaka"', config)
        self.assertIn("max_dynamic_length = 256", config)
        self.assertIn("symbolic_call_targets = true", config)

    def test_stateful_harness_compares_final_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            symbolic._write_project(
                project,
                "0x60006000f3",
                "0x60016000f3",
                {
                    "selector": "0xb3de648b",
                    "inputs": [{"name": "value", "type": "uint256"}],
                    "mutability": "nonpayable",
                },
                "osaka",
                max_dynamic_length=256,
            )

            test_source = (project / "test" / "SymbolicDifferential.t.sol").read_text()
            config = (project / "foundry.toml").read_text()

        self.assertEqual(test_source.count("target.call(callData)"), 2)
        self.assertNotIn("vm.etch", test_source)
        self.assertEqual(test_source.count("vm.accesses(target)"), 2)
        self.assertEqual(test_source.count("vm.getRecordedLogs()"), 1)
        self.assertIn("logsA[0] = _committedLogs(okA);", test_source)
        self.assertIn("logsB[0] = _committedLogs(okB);", test_source)
        self.assertIn("vm.store(STATE_MIRROR, writesA[i], valuesA[i])", test_source)
        self.assertIn(
            "assert(vm.load(STATE_MIRROR, writesB[i]) == vm.load(target, writesB[i]))",
            test_source,
        )
        self.assertIn("_assertSameBytes(a[i].data, b[i].data)", test_source)
        self.assertIn("assert(target.balance == balanceA)", test_source)
        self.assertIn("assert(vm.getNonce(target) == nonceA)", test_source)
        self.assertNotIn("observer", test_source)
        self.assertIn('storage_layout = "zero_init"', config)

    def test_payable_harness_sends_symbolic_value_from_symbolic_caller(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            symbolic._write_project(
                project,
                "0x60006000f3",
                "0x60016000f3",
                {
                    "selector": "0xb3de648b",
                    "inputs": [{"name": "value", "type": "uint256"}],
                    "mutability": "payable",
                },
                "osaka",
                max_dynamic_length=256,
                extras=("caller", "value", "observer"),
            )

            test_source = (project / "test" / "SymbolicDifferential.t.sol").read_text()

        self.assertIn(
            "checkSymbolicDifferential(uint256 arg0, address caller, "
            "uint256 value, address observer)",
            test_source,
        )
        self.assertIn("vm.deal(caller, value);", test_source)
        self.assertLess(
            test_source.index("vm.deal(caller, value);"),
            test_source.index("vm.snapshotState()"),
        )
        self.assertEqual(
            test_source.count("vm.prank(caller);\n        (bool ok"),
            2,
        )
        self.assertEqual(test_source.count("target.call{value: value}(callData)"), 2)
        self.assertIn("uint256 observedA = observer.balance;", test_source)
        self.assertIn("assert(observer.balance == observedA);", test_source)

    def test_prefix_state_is_prepared_before_symbolic_comparison(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            symbolic._write_project(
                project,
                "0x60006000f3",
                "0x60016000f3",
                {
                    "selector": "0xb3de648b",
                    "inputs": [{"name": "value", "type": "uint256"}],
                    "mutability": "view",
                },
                "osaka",
                max_dynamic_length=256,
                prefix_calldata=("0x12345678", "0x"),
            )

            test_source = (project / "test" / "SymbolicDifferential.t.sol").read_text()

        symbolic_start = test_source.index("function checkSymbolicDifferential")
        self.assertIn("logsA[1] = _committedLogs(ok);", test_source)
        self.assertIn("logsB[1] = _committedLogs(ok);", test_source)
        self.assertIn("logsA[2] = _committedLogs(okA);", test_source)
        self.assertEqual(test_source.count('hex"12345678"'), 2)
        self.assertEqual(test_source.count('hex""'), 2)
        first_prefix_a = test_source.index('hex"12345678"')
        second_prefix_a = test_source.index('hex""')
        first_prefix_b = test_source.index('hex"12345678"', first_prefix_a + 1)
        second_prefix_b = test_source.index('hex""', second_prefix_a + 1)
        self.assertTrue(
            first_prefix_a < second_prefix_a < first_prefix_b < second_prefix_b
        )
        self.assertGreater(first_prefix_a, symbolic_start)
        self.assertGreater(first_prefix_a, test_source.index("vm.record();"))
        self.assertIn("assert(prefixResults[0]", test_source)
        suffix_a = test_source.index("target.staticcall(callData)")
        suffix_b = test_source.index("target.staticcall(callData)", suffix_a + 1)
        self.assertTrue(
            second_prefix_a
            < suffix_a
            < test_source.index("assert(vm.revertToStateAndDelete(snapshot))")
            < second_prefix_b
            < suffix_b
        )


class CommandTests(unittest.TestCase):
    def test_identical_compiler_input_reaches_the_focused_runner(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source = root / "Probe.sol"
            source.write_text(
                "contract Probe { function f(uint256 x) "
                "external pure returns (uint256) { return x; } }\n"
            )
            solc_input = root / "solc-input.json"
            solar_input = root / "solar-input.json"
            solc = self._write_compiler(root / "solc", solc_input, "60" * 300)
            solar = self._write_compiler(root / "solar", solar_input, "60" * 400)
            forge = self._write_forge(root / "forge", max_dynamic_length=400)
            solver = self._write_executable(root / "z3", "#!/bin/sh\nexit 0\n")
            args = argparse.Namespace(
                source=source,
                contract="Probe",
                signature="f(uint256)",
                solc=str(solc),
                solar=str(solar),
                forge=str(forge),
                solver=str(solver),
                timeout=10.0,
                max_paths=32,
                max_solver_queries=100,
                max_calldata_bytes=512,
                max_depth=None,
                symbolic_timeout=5,
                exploration_order="bfs",
                dynamic_lengths=(0, 1),
                input_length=[],
                max_returndata_bytes=256,
                evm_version="osaka",
                optimize=True,
                optimizer_runs=200,
                via_ir=True,
                include_view=False,
                include_stateful=False,
                include_payable=False,
                symbolic_caller=False,
                prefix_calldata=[],
                project_root=None,
                include_path=[],
                remapping=[],
            )

            result = symbolic.run(args, root / "out")

            self.assertEqual(result["status"], "bounded_agreement")
            self.assertEqual(result["bounds"]["max_dynamic_length"], 400)
            self.assertEqual(solc_input.read_bytes(), solar_input.read_bytes())
            standard_input = json.loads(solc_input.read_text())
            self.assertEqual(
                standard_input["settings"]["optimizer"],
                {"enabled": True, "runs": 200},
            )

            for name, runtime in (("solc", "60" * 300), ("solar", "60" * 400)):
                directory = root / (name + "-attempt")
                directory.mkdir()
                (directory / "input.json").write_text(json.dumps(standard_input))
                (directory / "result.json").write_text(
                    json.dumps({"returncode": 0, "error": None})
                )
                output = {
                    "contracts": {
                        "Probe.sol": {
                            "Probe": {
                                "abi": _artifact()["abi"],
                                "evm": {
                                    "methodIdentifiers": {"f(uint256)": "b3de648b"},
                                    "deployedBytecode": {
                                        "object": runtime,
                                        "immutableReferences": {},
                                        "linkReferences": {},
                                    },
                                },
                            }
                        }
                    }
                }
                (directory / "stdout.txt").write_text(json.dumps(output))
            args.solc_attempt = root / "solc-attempt"
            args.solar_attempt = root / "solar-attempt"
            args.source = pathlib.Path("Probe.sol")
            args.solar = "missing-compiler-must-not-be-resolved"
            with patch.object(
                symbolic,
                "_compile",
                side_effect=AssertionError("must reuse saved artifacts"),
            ):
                result = symbolic.run(args, root / "saved-out")
            self.assertEqual(result["status"], "bounded_agreement")
            self.assertEqual(
                result["standard_input_sha256"],
                symbolic.hashlib.sha256(
                    json.dumps(
                        standard_input,
                        ensure_ascii=False,
                        separators=(",", ":"),
                        sort_keys=True,
                    ).encode()
                ).hexdigest(),
            )
            for source_unit in ("./C.sol", "https://example.com/C.sol"):
                with self.subTest(source_unit=source_unit):
                    request = {
                        **standard_input,
                        "sources": {
                            source_unit: next(iter(standard_input["sources"].values()))
                        },
                    }
                    for directory in (args.solc_attempt, args.solar_attempt):
                        saved_output = json.loads(
                            (directory / "stdout.txt").read_text()
                        )
                        saved_output["contracts"] = {
                            source_unit: next(iter(saved_output["contracts"].values()))
                        }
                        (directory / "input.json").write_text(json.dumps(request))
                        (directory / "stdout.txt").write_text(json.dumps(saved_output))
                    with patch("sys.stdout", new_callable=io.StringIO) as stdout:
                        code = symbolic.main(
                            [
                                "--source",
                                source_unit,
                                "--contract",
                                "Probe",
                                "--signature",
                                "f(uint256)",
                                "--solc-attempt",
                                str(args.solc_attempt),
                                "--solar-attempt",
                                str(args.solar_attempt),
                                "--solc",
                                str(solc),
                                "--forge",
                                str(forge),
                                "--solver",
                                str(solver),
                                "--output-root",
                                str(root / "literal-source-out"),
                                "--max-paths",
                                "32",
                                "--max-solver-queries",
                                "100",
                                "--max-calldata-bytes",
                                "512",
                                "--symbolic-timeout",
                                "5",
                                "--dynamic-lengths",
                                "0,1",
                                "--max-returndata-bytes",
                                "256",
                            ]
                        )
                    self.assertEqual(code, 0, stdout.getvalue())
                    saved_result = json.loads(stdout.getvalue())
                    self.assertEqual(saved_result["source"], source_unit)
                    self.assertEqual(saved_result["sources"], [source_unit])
            for directory in (args.solc_attempt, args.solar_attempt):
                (directory / "input.json").write_text(json.dumps(standard_input))
                (directory / "stdout.txt").write_text(json.dumps(output))
            deployed = output["contracts"]["Probe.sol"]["Probe"]["evm"][
                "deployedBytecode"
            ]
            # LOG0 and CALL: a log may come from a frame that later reverts.
            deployed["object"] = "a0f1"
            (args.solar_attempt / "stdout.txt").write_text(json.dumps(output))
            logging_result = symbolic.run(args, root / "logging-out")
            self.assertEqual(logging_result["status"], "incomplete")
            self.assertIn("reverted inner frames", logging_result["reason"])
            deployed.pop("immutableReferences")
            (args.solar_attempt / "stdout.txt").write_text(json.dumps(output))
            with self.assertRaisesRegex(ValueError, "immutableReferences"):
                symbolic.run(args, root / "saved-out")
            (args.solar_attempt / "input.json").write_text("{}")
            with self.assertRaisesRegex(ValueError, "different inputs"):
                symbolic.run(args, root / "saved-out")

    def test_import_discovery_uses_requested_evm_version(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source_dir = root / "src"
            source_dir.mkdir()
            source = source_dir / "Probe.sol"
            imported = source_dir / "Lib.sol"
            source_content = (
                'import "./Lib.sol"; contract Probe { function f() external '
                "pure returns (uint result) { assembly { let mcopy := 1 "
                "result := mcopy } } }\n"
            )
            source.write_text(source_content)
            imported.write_text("library Lib {}\n")
            compiler = self._write_executable(
                root / "solc",
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "value = json.load(sys.stdin)\n"
                "assert value['settings']['evmVersion'] == 'paris'\n"
                "print(json.dumps({'sources': {"
                "'src/Probe.sol': {'id': 0}, 'src/Lib.sol': {'id': 1}}}))\n",
            )

            materialized = symbolic._standard_input(
                str(compiler),
                source,
                evm_version="paris",
                optimize=True,
                optimizer_runs=200,
                via_ir=True,
                project_root=root,
                include_paths=(),
                remappings=(),
                timeout=10.0,
            )

        self.assertEqual(
            materialized["input"]["sources"],
            {
                "src/Lib.sol": {"content": "library Lib {}\n"},
                "src/Probe.sol": {"content": source_content},
            },
        )
        self.assertEqual(materialized["settings"]["evmVersion"], "paris")

    def _write_compiler(
        self,
        path: pathlib.Path,
        captured_input: pathlib.Path,
        runtime: str,
    ) -> pathlib.Path:
        artifact = {
            "sources": {"Probe.sol": {"id": 0}},
            "contracts": {
                "Probe.sol": {
                    "Probe": {
                        "abi": _artifact()["abi"],
                        "evm": {
                            "deployedBytecode": {
                                "object": runtime,
                                "immutableReferences": {},
                                "linkReferences": {},
                            },
                            "methodIdentifiers": {
                                "f(uint256)": "b3de648b",
                            },
                        },
                    }
                }
            },
        }
        script = (
            "#!/usr/bin/env python3\n"
            "import pathlib, sys\n"
            f"pathlib.Path({str(captured_input)!r}).write_text(sys.stdin.read())\n"
            f"print({json.dumps(artifact)!r})\n"
        )
        return self._write_executable(path, script)

    def _write_forge(
        self,
        path: pathlib.Path,
        *,
        max_dynamic_length: int,
    ) -> pathlib.Path:
        script = (
            "#!/usr/bin/env python3\n"
            "import pathlib, sys\n"
            "if '--help' in sys.argv:\n"
            "    print('--allow-local-compiler')\n"
            "    raise SystemExit\n"
            "index = sys.argv.index('--symbolic-max-dynamic-length')\n"
            f"assert sys.argv[index + 1] == {str(max_dynamic_length)!r}\n"
            f"assert 'max_dynamic_length = {max_dynamic_length}' in "
            "pathlib.Path('foundry.toml').read_text()\n"
            f"print({json.dumps(_forge_report('pass', max_dynamic_length=max_dynamic_length))!r})\n"
        )
        return self._write_executable(path, script)

    def _write_executable(
        self,
        path: pathlib.Path,
        content: str,
    ) -> pathlib.Path:
        path.write_text(content)
        path.chmod(0o755)
        return path


if __name__ == "__main__":
    unittest.main()
