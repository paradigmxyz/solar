#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import pathlib
import tempfile
import unittest

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
) -> dict[str, object]:
    symbolic_result: dict[str, object] = {
        "status": status,
        "replay": {"status": replay},
        "bounds": {
            "timeout_seconds": 5,
            "max_paths": 32,
            "max_solver_queries": 100,
            "max_calldata_bytes": 512,
            "max_dynamic_length": 256,
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
    return {
        "test/SymbolicDifferential.t.sol:SymbolicDifferentialTest": {
            "test_results": {
                "checkSymbolicDifferential(uint256)": {
                    "status": outer_status,
                    "symbolic": symbolic_result,
                }
            }
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

    def test_bounded_agreement_requires_the_requested_forge_bounds(self) -> None:
        expected = {
            "solver_timeout_seconds": 5,
            "max_paths": 32,
            "max_solver_queries": 100,
            "max_calldata_bytes": 512,
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
        symbolic_result = next(iter(report.values()))["test_results"]
        symbolic_result = next(iter(symbolic_result.values()))["symbolic"]
        symbolic_result["bounds"]["max_paths"] = 31
        with self.assertRaisesRegex(ValueError, "max_paths=31"):
            symbolic._classify(report, "0xb3de648b", expected)


class ProjectGenerationTests(unittest.TestCase):
    def test_writes_only_the_focused_symbolic_harness(self) -> None:
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
            )

            test_source = (
                project / "test" / "SymbolicDifferential.t.sol"
            ).read_text()
            config = (project / "foundry.toml").read_text()

        self.assertIn("checkSymbolicDifferential(uint256 arg0)", test_source)
        self.assertIn("0xb3de648b", test_source)
        self.assertIn("60006000f3", test_source)
        self.assertIn("60016000f3", test_source)
        self.assertNotIn("recordLogs", test_source)
        self.assertNotIn("vm.store", test_source)
        self.assertIn('evm_version = "osaka"', config)

    def test_stateful_harness_compares_logs_and_written_storage(self) -> None:
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
            )

            test_source = (
                project / "test" / "SymbolicDifferential.t.sol"
            ).read_text()
            config = (project / "foundry.toml").read_text()

        self.assertIn("vm.recordLogs()", test_source)
        self.assertIn("vm.accesses(ROUTER)", test_source)
        self.assertIn("vm.revertToState(snapshot)", test_source)
        self.assertIn("vm.load(STATE_MIRROR", test_source)
        self.assertIn('storage_layout = "zero_init"', config)


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
            solc = self._write_compiler(root / "solc", solc_input)
            solar = self._write_compiler(root / "solar", solar_input)
            forge = self._write_forge(root / "forge")
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
                project_root=None,
                include_path=[],
                remapping=[],
            )

            result = symbolic.run(args, root / "out")

            self.assertEqual(result["status"], "bounded_agreement")
            self.assertEqual(solc_input.read_bytes(), solar_input.read_bytes())
            standard_input = json.loads(solc_input.read_text())
            self.assertEqual(
                standard_input["settings"]["optimizer"],
                {"enabled": True, "runs": 200},
            )

    def test_project_imports_are_embedded_in_the_shared_input(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source_dir = root / "src"
            source_dir.mkdir()
            source = source_dir / "Probe.sol"
            imported = source_dir / "Lib.sol"
            source.write_text('import "./Lib.sol"; contract Probe {}\n')
            imported.write_text("library Lib {}\n")
            compiler = self._write_executable(
                root / "solc",
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "json.load(sys.stdin)\n"
                "print(json.dumps({'sources': {"
                "'src/Probe.sol': {'id': 0}, 'src/Lib.sol': {'id': 1}}}))\n",
            )

            materialized = symbolic._standard_input(
                str(compiler),
                source,
                evm_version="osaka",
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
                "src/Probe.sol": {
                    "content": 'import "./Lib.sol"; contract Probe {}\n'
                },
            },
        )

    def _write_compiler(
        self,
        path: pathlib.Path,
        captured_input: pathlib.Path,
    ) -> pathlib.Path:
        artifact = {
            "sources": {"Probe.sol": {"id": 0}},
            "contracts": {
                "Probe.sol": {
                    "Probe": {
                        "abi": _artifact()["abi"],
                        "evm": {
                            "deployedBytecode": {
                                "object": "60006000f3",
                                "immutableReferences": {},
                                "linkReferences": {},
                            },
                            "methodIdentifiers": {
                                "f(uint256)": "b3de648b",
                            },
                        },
                    }
                }
            }
        }
        script = (
            "#!/usr/bin/env python3\n"
            "import pathlib, sys\n"
            f"pathlib.Path({str(captured_input)!r}).write_text(sys.stdin.read())\n"
            f"print({json.dumps(artifact)!r})\n"
        )
        return self._write_executable(path, script)

    def _write_forge(self, path: pathlib.Path) -> pathlib.Path:
        script = (
            "#!/usr/bin/env python3\n"
            f"print({json.dumps(_forge_report('pass'))!r})\n"
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
