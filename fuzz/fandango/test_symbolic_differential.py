#!/usr/bin/env python3

from __future__ import annotations

import argparse
import io
import json
import os
import pathlib
import subprocess
import tempfile
import unittest
from typing import Any
from unittest.mock import patch

import symbolic_differential as symbolic

# A replayed counterexample from `forge equivalence --sig 'quote(uint256)' --json`
# on Bank.sol, compiled by Solc and by Solc with `return count == 1 ? 0 : ...`,
# from arbitrary storage.
NOT_EQUIVALENT = json.dumps(
    {
        "status": "not_equivalent",
        "out_of_gas": False,
        "calldata": "0xed1bd76c4000000000000000000000000000000000000000000000000000000000000000",
        "args": [
            "28948022309329048855892746252171976963317496166410141009864396001978282409984 [2.894e76]"
        ],
        "env": {
            "address": "0x7fa9385be102ac3eac297483dd6233d62b3e1496",
            "caller": "0x0000000000000000000000000000000000000000",
            "origin": "0x0000000000000000000000000000000000000000",
            "value": "0x0",
            "timestamp": "0x0",
            "number": "0x0",
            "prevrandao": "0x0",
            "gas_limit": "0x0",
            "basefee": "0x0",
            "chain_id": "0x0",
            "coinbase": "0x0000000000000000000000000000000000000000",
            "gas_price": "0x0",
        },
        "code_a": {
            "success": False,
            "output": "0x4e487b710000000000000000000000000000000000000000000000000000000000000011",
            "logs": [],
        },
        "code_b": {
            "success": True,
            "output": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "logs": [],
        },
        "storage": [],
        "balances": [],
        "accounts": [],
        "stats": {
            "paths": 4,
            "solver_queries": 7,
            "smt_queries": 5,
            "sat_queries": 10,
            "model_queries": 1,
            "sat_cache_hits": 4,
            "model_cache_hits": 0,
            "heuristic_witnesses": 0,
            "solver_time_ms": 72,
            "smt_input_bytes": 8976,
            "smt_max_query_bytes": 1828,
            "smt_build_time_ms": 0,
            "smt_max_query_time_ms": 35,
        },
    }
)
# The same run on `read()` of Vault.sol, compiled by Solc and Solar.
EQUIVALENT = json.dumps(
    {
        "status": "equivalent",
        "stats": {
            "paths": 6,
            "solver_queries": 6,
            "smt_queries": 2,
            "sat_queries": 14,
            "model_queries": 0,
            "sat_cache_hits": 8,
            "model_cache_hits": 0,
            "heuristic_witnesses": 0,
            "solver_time_ms": 31,
            "smt_input_bytes": 3401,
            "smt_max_query_bytes": 1801,
            "smt_build_time_ms": 0,
            "smt_max_query_time_ms": 21,
        },
    }
)
# Forge reports an incomplete check as an error, here on `bal(address)` of Vault.sol.
INCOMPLETE = "Error: equivalence check incomplete (Error): the solver's counterexample 0x5580f72a000000000000000000000000ffffffffffffffffffffffffffffffffffffffff did not replay concretely\n"
# `spawn(uint256)` of Bank.sol, compiled by Solc and Solar, differs only in the
# code of the created contract.
CREATED_CODE = json.dumps(
    {
        "status": "not_equivalent",
        "out_of_gas": False,
        "calldata": "0x7ec0c82d0000000000000000000000000000000000000000000000000000000000000000",
        "args": ["0"],
        "env": {
            "address": "0x7fa9385be102ac3eac297483dd6233d62b3e1496",
            "caller": "0x0000000000000000000000000000000000000000",
            "origin": "0x0000000000000000000000000000000000000000",
            "value": "0x0",
            "timestamp": "0xffffffffffffffff",
            "number": "0xffffffffffffffff",
            "prevrandao": "0x0",
            "gas_limit": "0xffffffffffffffff",
            "basefee": "0xffffffffffffffff",
            "chain_id": "0xffffffffffffffff",
            "coinbase": "0xffffffffffffffffffffffffffffffffffffffff",
            "gas_price": "0xffffffffffffffffffffffffffffffff",
        },
        "code_a": {
            "success": True,
            "output": "0x0000000000000000000000007d8cb8f412b3ee9ac79558791333f41d2b1ccdac",
            "logs": [],
        },
        "code_b": {
            "success": True,
            "output": "0x0000000000000000000000007d8cb8f412b3ee9ac79558791333f41d2b1ccdac",
            "logs": [],
        },
        "storage": [],
        "balances": [],
        "accounts": [
            {
                "address": "0x7d8cb8f412b3ee9ac79558791333f41d2b1ccdac",
                "code_a": {
                    "nonce": 1,
                    "code_hash": "0x7742b692736346184d895cfc886315f59c6ae6bb999f02f19f22c4e2bfa35da6",
                },
                "code_b": {
                    "nonce": 1,
                    "code_hash": "0x4ed5274b1dc9d1cce882051c01fe75887922b7105426b05229ae1b5496f85231",
                },
            }
        ],
        "stats": {
            "paths": 6,
            "solver_queries": 4,
            "smt_queries": 2,
            "sat_queries": 7,
            "model_queries": 1,
            "sat_cache_hits": 4,
            "model_cache_hits": 0,
            "heuristic_witnesses": 0,
            "solver_time_ms": 43,
            "smt_input_bytes": 3181,
            "smt_max_query_bytes": 1801,
            "smt_build_time_ms": 0,
            "smt_max_query_time_ms": 33,
        },
    }
)


def _function(
    name: str = "f",
    inputs: list[dict[str, Any]] | None = None,
    mutability: str = "pure",
) -> dict[str, Any]:
    return {
        "type": "function",
        "name": name,
        "inputs": [{"name": "value", "type": "uint256"}] if inputs is None else inputs,
        "outputs": [{"name": "", "type": "uint256"}],
        "stateMutability": mutability,
    }


def _artifact(
    *functions: dict[str, Any],
    selectors: dict[str, str] | None = None,
    runtime: str = "0x60006000f3",
    creation: str | None = "0x6001600055",
    constructor: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    functions = functions or (_function(),)
    if selectors is None:
        selectors = {
            symbolic._abi_signature(entry): f"{index:08x}"
            for index, entry in enumerate(functions)
        }
    return {
        "abi": list(functions),
        "method_identifiers": selectors,
        "runtime": runtime,
        "creation": creation,
        "creation_links": False,
        "has_immutables": False,
        "constructor_entry": {
            "inputs": constructor or [],
            "outputs": [],
            "stateMutability": "nonpayable",
        },
        "constructor": constructor or [],
    }


def _args(**overrides: object) -> argparse.Namespace:
    values: dict[str, object] = {
        "signature": None,
        "all_functions": False,
        "constructor_args": None,
        "constructor_value": 0,
        "runtime_only": False,
        "arbitrary_storage": False,
        "value": None,
        "promise_no_reent": False,
        "ignore_out_of_gas": False,
        "timeout": 30.5,
        "max_paths": 64,
        "max_solver_queries": 100,
        "max_depth": 1000,
        "max_calldata_bytes": 1024,
        "dynamic_lengths": (0, 1),
        "evm_version": "osaka",
    }
    values.update(overrides)
    return argparse.Namespace(**values)


class PlanTests(unittest.TestCase):
    def test_signature_selects_one_function_of_any_mutability(self) -> None:
        artifact = _artifact(_function(mutability="payable"))

        checks, skipped = symbolic._plan_checks(
            artifact, artifact, _args(signature="f(uint256)")
        )

        self.assertEqual(checks, [{"label": "f(uint256)", "sig": "f(uint256)"}])
        self.assertEqual(skipped, [])

    def test_signature_is_required_without_all_functions(self) -> None:
        artifact = _artifact()
        with self.assertRaisesRegex(ValueError, "--signature is required"):
            symbolic._plan_checks(artifact, artifact, _args())

    def test_rejects_selector_and_shape_disagreements(self) -> None:
        solc = _artifact()
        solar = _artifact(selectors={"f(uint256)": "ffffffff"})
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "selector"):
            symbolic._plan_checks(solc, solar, _args(signature="f(uint256)"))
        solar = _artifact(_function(mutability="view"))
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "ABI disagreement"):
            symbolic._plan_checks(solc, solar, _args(signature="f(uint256)"))

    def test_all_functions_adds_raw_calldata(self) -> None:
        artifact = _artifact(
            _function("b", [{"name": "x", "type": "uint256"}]),
            _function(
                "a", [{"name": "x", "type": "bytes"}, {"name": "y", "type": "uint8[2]"}]
            ),
            _function("c", []),
        )

        checks, skipped = symbolic._plan_checks(
            artifact, artifact, _args(all_functions=True)
        )

        self.assertEqual(skipped, [])
        self.assertEqual(
            checks,
            [
                {"label": "a(bytes,uint8[2])", "sig": "a(bytes,uint8[2])"},
                {"label": "b(uint256)", "sig": "b(uint256)"},
                {"label": "c()", "sig": "c()"},
                {
                    "label": "<other calldata>",
                    # Short calldata, and around the heads of 0, 32, and 96 bytes.
                    "calldata_lengths": [0, 1, 3, 4, 35, 36, 68, 99, 100, 132],
                },
            ],
        )

    def test_raw_lengths_respect_the_calldata_limit(self) -> None:
        abi = [_function(inputs=[{"name": "x", "type": "uint256[40]"}])]

        self.assertEqual(symbolic._raw_lengths(abi, 1300), [0, 1, 3, 4, 36, 1283, 1284])
        self.assertEqual(symbolic._raw_lengths(abi, 36), [0, 1, 3, 4, 36])

    def test_all_functions_skips_inputs_forge_cannot_generate(self) -> None:
        artifact = _artifact(
            _function("f"),
            _function("g", [{"name": "x", "type": "function"}]),
            _function("h", [{"name": "x", "type": "fixed128x18"}]),
        )

        checks, skipped = symbolic._plan_checks(
            artifact, artifact, _args(all_functions=True)
        )

        self.assertEqual(
            [check["label"] for check in checks], ["f(uint256)", "<other calldata>"]
        )
        self.assertEqual(
            [item["signature"] for item in skipped],
            ["g(function)", "h(fixed128x18)"],
        )

    def test_abi_disagreements_fail_closed(self) -> None:
        solc = _artifact(_function("f"), _function("g", []))
        solar = _artifact(_function("f"))
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "only Solc has"):
            symbolic._plan_checks(solc, solar, _args(all_functions=True))
        # A selector disagreement on a function Forge cannot generate still counts.
        solc = _artifact(_function("g", [{"name": "x", "type": "function"}]))
        solar = _artifact(
            _function("g", [{"name": "x", "type": "function"}]),
            selectors={"g(function)": "ffffffff"},
        )
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "selector"):
            symbolic._plan_checks(solc, solar, _args(all_functions=True))

    def test_zero_arguments_encode_empty_dynamic_values(self) -> None:
        def words(*values: int) -> str:
            return "".join(f"{value:064x}" for value in values)

        self.assertEqual(symbolic._zero_arguments([]), b"")
        self.assertEqual(
            symbolic._zero_arguments(
                [{"type": "uint256"}, {"type": "address[2]"}]
            ).hex(),
            words(0, 0, 0),
        )
        encoded = symbolic._zero_arguments(
            [
                {"type": "bytes"},
                {"type": "uint8"},
                {"type": "string[]"},
                {
                    "type": "tuple",
                    "components": [{"type": "uint256"}, {"type": "bytes"}],
                },
                {"type": "bytes[2]"},
            ]
        )
        self.assertEqual(
            encoded.hex(),
            words(
                # Heads: offsets of the dynamic values, then a zero uint8.
                0xA0,
                0,
                0xC0,
                0xE0,
                0x140,
                # bytes, string[], (uint256,bytes), and bytes[2].
                0,
                0,
                0,
                0x40,
                0,
                0x40,
                0x60,
                0,
                0,
            ),
        )


class ForgeTests(unittest.TestCase):
    def test_command_compares_solc_and_solar_runtimes(self) -> None:
        tools = {"forge": "/bin/forge", "solver": "bitwuzla"}

        command = symbolic._forge_command(
            tools, {"label": "f(uint256)", "sig": "f(uint256)"}, _args()
        )

        self.assertEqual(
            command,
            [
                "/bin/forge",
                "equivalence",
                "--code-a-file",
                "solc.hex",
                "--code-b-file",
                "solar.hex",
                "--sig",
                "f(uint256)",
                "--max-calldata",
                "1024",
                "--solver",
                "bitwuzla",
                "--timeout",
                "31",
                "--empty-storage",
                "--json",
            ],
        )

    def test_command_passes_raw_lengths_and_options(self) -> None:
        tools = {"forge": "forge", "solver": "z3"}
        check = {"label": "<other calldata>", "calldata_lengths": [0, 4, 36]}

        command = symbolic._forge_command(
            tools,
            check,
            _args(
                arbitrary_storage=True,
                value=7,
                promise_no_reent=True,
                ignore_out_of_gas=True,
            ),
        )

        self.assertIn("--calldata-lengths", command)
        self.assertEqual(command[command.index("--calldata-lengths") + 1], "0,4,36")
        self.assertNotIn("--sig", command)
        self.assertNotIn("--empty-storage", command)
        self.assertEqual(command[command.index("--value") + 1], "7")
        self.assertIn("--promise-no-reent", command)
        self.assertIn("--ignore-out-of-gas", command)

    def test_foundry_toml_holds_the_symbolic_bounds(self) -> None:
        self.assertEqual(
            symbolic._foundry_toml(_args(evm_version="cancun")),
            "[profile.default]\n"
            'evm_version = "cancun"\n'
            "\n"
            "[profile.default.symbolic]\n"
            "max_paths = 64\n"
            "max_depth = 1000\n"
            "max_solver_queries = 100\n"
            "max_dynamic_length = 256\n"
            "default_array_lengths = [0, 1]\n"
            "default_bytes_lengths = [0, 1]\n",
        )

    def test_classifies_equivalence(self) -> None:
        outcome = symbolic._classify(0, EQUIVALENT + "\n", "", {})

        self.assertEqual(outcome["status"], "bounded_agreement")
        self.assertEqual(outcome["stats"], json.loads(EQUIVALENT)["stats"])

    def test_classifies_replayed_counterexample(self) -> None:
        report = json.loads(NOT_EQUIVALENT)

        outcome = symbolic._classify(
            1,
            NOT_EQUIVALENT + "\n",
            "Error: contracts are not equivalent\n",
            {"0xed1bd76c": "quote(uint256)"},
        )

        self.assertEqual(outcome["status"], "mismatch")
        # Solc's build overflows where the mutated one returns 0.
        self.assertEqual(outcome["differs_in"], "call success")
        counterexample = outcome["counterexample"]
        self.assertEqual(counterexample["calldata"], report["calldata"])
        self.assertEqual(counterexample["function"], "quote(uint256)")
        self.assertEqual(counterexample["args"], report["args"])
        self.assertEqual(counterexample["caller"], report["env"]["caller"])
        self.assertEqual(counterexample["value"], str(int(report["env"]["value"], 16)))
        self.assertEqual(counterexample["solc"], report["code_a"])
        self.assertEqual(counterexample["solar"], report["code_b"])

    def test_names_what_differs(self) -> None:
        base = json.loads(NOT_EQUIVALENT)
        same = {"success": True, "output": "0x", "logs": []}
        cases = [
            ({"out_of_gas": True}, "out of gas"),
            ({"code_b": {**same, "success": False}}, "call success"),
            ({"code_b": {**same, "output": "0x01"}}, "returndata"),
            ({"code_b": {**same, "logs": [{"topics": []}]}}, "logs"),
            (
                {
                    "storage": [
                        {
                            "address": "0x1",
                            "slot": "0x0",
                            "code_a": "0x1",
                            "code_b": "0x2",
                        }
                    ]
                },
                "storage",
            ),
            (
                {"balances": [{"address": "0x1", "code_a": "0x1", "code_b": "0x2"}]},
                "balance",
            ),
            (
                {
                    "accounts": [
                        {
                            "address": "0x1",
                            "code_a": {"nonce": 1, "code_hash": "0xa"},
                            "code_b": {"nonce": 2, "code_hash": "0xa"},
                        }
                    ]
                },
                "nonce",
            ),
        ]
        for change, differs_in in cases:
            with self.subTest(differs_in=differs_in):
                report = {
                    **base,
                    "out_of_gas": False,
                    "code_a": same,
                    "code_b": same,
                    "storage": [],
                    "balances": [],
                    "accounts": [],
                    **change,
                }
                outcome = symbolic._classify(1, json.dumps(report), "", {})
                self.assertEqual(outcome["status"], "mismatch")
                self.assertEqual(outcome["differs_in"], differs_in)
        storage = symbolic._classify(1, json.dumps({**base, **cases[4][0]}), "", {})
        self.assertEqual(
            storage["counterexample"]["storage"],
            [{"address": "0x1", "slot": "0x0", "solc": "0x1", "solar": "0x2"}],
        )

    def test_created_code_difference_is_incomplete(self) -> None:
        outcome = symbolic._classify(1, CREATED_CODE, "", {})

        self.assertEqual(outcome["status"], "incomplete")
        self.assertIn("different code", outcome["reason"])
        self.assertIn("accounts", outcome["counterexample"])

    def test_forge_errors_are_incomplete(self) -> None:
        outcome = symbolic._classify(1, "", INCOMPLETE, {})
        self.assertEqual(outcome["status"], "incomplete")
        self.assertTrue(
            outcome["reason"].startswith("equivalence check incomplete"),
            outcome["reason"],
        )
        crash = symbolic._classify(-11, "", "", {})
        self.assertEqual(
            crash,
            {
                "status": "incomplete",
                "reason": "forge equivalence exited with status -11 without a result",
            },
        )
        # A result must match the exit status.
        self.assertEqual(
            symbolic._classify(1, EQUIVALENT, "", {})["status"], "incomplete"
        )
        self.assertEqual(
            symbolic._classify(0, NOT_EQUIVALENT, "", {})["status"], "incomplete"
        )

    def test_timeout_is_incomplete_and_logged(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            with patch.object(
                symbolic.subprocess,
                "run",
                side_effect=subprocess.TimeoutExpired(["forge"], 90),
            ):
                outcome = symbolic._run_check(["forge"], project, 3, 30.0, {})
            log = json.loads((project / "check-3.json").read_text())

        self.assertEqual(outcome["status"], "incomplete")
        self.assertIn("did not finish within 90 seconds", outcome["reason"])
        self.assertEqual(log, {"command": ["forge"]})

    def test_runs_forge_with_only_the_project_configuration(self) -> None:
        seen = {}

        def run(command, **kwargs):
            seen.update(kwargs)
            return subprocess.CompletedProcess(command, 0, EQUIVALENT, "")

        with tempfile.TemporaryDirectory() as tmp:
            project = pathlib.Path(tmp)
            with (
                patch.dict(os.environ, {"FOUNDRY_PROFILE": "ci"}),
                patch.object(symbolic.subprocess, "run", side_effect=run),
            ):
                outcome = symbolic._run_check(["forge"], project, 0, 30.0, {})

        self.assertEqual(outcome["status"], "bounded_agreement")
        self.assertEqual(seen["cwd"], project)
        self.assertEqual(seen["timeout"], 30.0 + symbolic.FORGE_GRACE_SECONDS)
        self.assertNotIn("FOUNDRY_PROFILE", seen["env"])
        self.assertEqual(seen["env"]["HOME"], str(project / ".home"))

    def test_one_incomplete_check_hides_no_mismatch(self) -> None:
        mismatch = {
            "label": "g()",
            "status": "mismatch",
            "differs_in": "storage",
            "counterexample": {"calldata": "0xe2179b8e"},
        }
        summary = symbolic._summarize(
            [
                {"label": "f()", "status": "incomplete", "reason": "path limit"},
                mismatch,
                {"label": "h()", "status": "bounded_agreement"},
            ]
        )

        self.assertEqual(summary["status"], "mismatch")
        self.assertEqual(summary["differs_in"], "storage")
        self.assertEqual(summary["counterexample"], {"calldata": "0xe2179b8e"})
        self.assertEqual(len(summary["checks"]), 3)
        incomplete = symbolic._summarize(
            [
                {"label": "f()", "status": "bounded_agreement"},
                {"label": "g()", "status": "incomplete", "reason": "path limit"},
            ]
        )
        self.assertEqual(incomplete["status"], "incomplete")
        self.assertEqual(incomplete["reason"], "g(): path limit")
        self.assertEqual(symbolic._summarize([])["status"], "incomplete")


class ConstructorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.constructor = {"args": "0x", "value": "0"}

    def _side(self, **overrides: object) -> dict[str, Any]:
        side: dict[str, Any] = {
            "success": True,
            "address": "0x5fbdb2315678afecb367f032d93f642f64180aa3",
            "runtime": "0x00",
            "logs": [],
            "storage_root": "0xaa",
            "balance": "0",
            "nonce": 1,
        }
        side.update(overrides)
        return side

    def test_agreeing_deployments(self) -> None:
        outcome = symbolic._compare_constructors(
            {"solc": self._side(), "solar": self._side(runtime="0x01")},
            self.constructor,
        )
        self.assertEqual(outcome, {"status": "bounded_agreement"})

    def test_differences_are_mismatches(self) -> None:
        failed = {"success": False, "data": "0x", "error": "execution reverted"}
        cases = [
            (failed, "success"),
            (self._side(logs=[{"topics": [], "data": "0x"}]), "logs"),
            (self._side(storage_root="0xbb"), "storage"),
            (self._side(balance="1"), "balance"),
            (self._side(nonce=2), "nonce"),
        ]
        for solar, comparison in cases:
            with self.subTest(comparison=comparison):
                outcome = symbolic._compare_constructors(
                    {"solc": self._side(), "solar": solar}, self.constructor
                )
                self.assertEqual(outcome["status"], "mismatch")
                self.assertEqual(outcome["differs_in"], "constructor")
                self.assertEqual(outcome["comparison"], comparison)
                self.assertNotIn("runtime", outcome["counterexample"]["solc"])
        outcome = symbolic._compare_constructors(
            {"solc": {**failed, "data": "0x01"}, "solar": failed}, self.constructor
        )
        self.assertEqual(outcome["comparison"], "revert data")

    def test_constructor_logs_keep_their_emitter(self) -> None:
        raw = {"address": "0xAB", "topics": ["0x01"], "data": "0x02"}
        log = symbolic._constructor_log(raw)
        self.assertEqual(log, {"address": "0xab", "topics": ["0x01"], "data": "0x02"})
        outcome = symbolic._compare_constructors(
            {
                "solc": self._side(logs=[log]),
                "solar": self._side(
                    logs=[symbolic._constructor_log({**raw, "address": "0xcd"})]
                ),
            },
            self.constructor,
        )
        self.assertEqual(outcome["status"], "mismatch")
        self.assertEqual(outcome["comparison"], "logs")

    def test_identical_reverts_compare_nothing(self) -> None:
        failed = {"success": False, "data": "0x01", "error": "execution reverted"}
        outcome = symbolic._compare_constructors(
            {"solc": failed, "solar": dict(failed)}, self.constructor
        )
        self.assertEqual(outcome["status"], "incomplete")
        self.assertIn("--constructor-args", outcome["reason"])


class ValidationTests(unittest.TestCase):
    def test_validates_hex_wei_and_lengths(self) -> None:
        self.assertEqual(symbolic._hex_data("0xAB"), "0xab")
        for value in ("ab", "0xa", "0xzz"):
            with self.assertRaises(argparse.ArgumentTypeError):
                symbolic._hex_data(value)
        self.assertEqual(symbolic._wei("0x10"), 16)
        self.assertEqual(symbolic._wei("10"), 10)
        with self.assertRaises(argparse.ArgumentTypeError):
            symbolic._wei("-1")
        self.assertEqual(symbolic._dynamic_lengths("0,2"), (0, 2))
        for value in ("", "1,1", "257", "a"):
            with self.assertRaises(argparse.ArgumentTypeError):
                symbolic._dynamic_lengths(value)

    def test_runtime_opcodes_skip_push_data_and_metadata(self) -> None:
        self.assertEqual(symbolic._runtime_opcodes("0x60f0f5"), {0x60, 0xF5})
        # Trailing CBOR metadata of two bytes, announced by its length.
        self.assertEqual(symbolic._runtime_opcodes("0x00a1f00002"), {0x00})


class RunTests(unittest.TestCase):
    def test_compares_identical_compiler_inputs_with_forge(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source = root / "Probe.sol"
            source.write_text(
                "contract Probe { function f(uint256 x) "
                "external pure returns (uint256) { return x; } }\n"
            )
            solc_input = root / "solc-input.json"
            solar_input = root / "solar-input.json"
            solc = self._write_compiler(root / "solc", solc_input, "6001")
            solar = self._write_compiler(root / "solar", solar_input, "6002")
            forge = self._write_forge(root / "forge")
            args = _args(
                source=source,
                contract="Probe",
                signature="f(uint256)",
                solc=str(solc),
                solar=str(solar),
                forge=str(forge),
                solver="z3",
                solc_attempt=None,
                solar_attempt=None,
                optimize=True,
                optimizer_runs=200,
                via_ir=True,
                project_root=None,
                include_path=[],
                remapping=[],
            )

            result = symbolic.run(args, root / "out")

            self.assertEqual(result["status"], "bounded_agreement", result)
            self.assertEqual(result["checked"], ["f(uint256)"])
            self.assertEqual(result["bounds"]["constructor"], "runtime_only")
            self.assertEqual(result["schema"], "solar:solsymdiff@v3")
            project = pathlib.Path(result["project"])
            self.assertEqual((project / "solc.hex").read_text(), "6001\n")
            self.assertEqual((project / "solar.hex").read_text(), "6002\n")
            argv = json.loads((root / "argv").read_text())
            self.assertEqual(argv[argv.index("--sig") + 1], "f(uint256)")
            self.assertIn("max_paths = 64", (project / "foundry.toml").read_text())
            self.assertEqual(solc_input.read_bytes(), solar_input.read_bytes())
            standard_input = json.loads(solc_input.read_text())
            self.assertEqual(
                standard_input["settings"]["optimizer"],
                {"enabled": True, "runs": 200},
            )

            for name, runtime in (("solc", "6001"), ("solar", "6002")):
                directory = root / (name + "-attempt")
                directory.mkdir()
                (directory / "input.json").write_text(json.dumps(standard_input))
                (directory / "result.json").write_text(
                    json.dumps({"returncode": 0, "error": None})
                )
                (directory / "stdout.txt").write_text(json.dumps(self._output(runtime)))
            with patch("sys.stdout", new_callable=io.StringIO) as stdout:
                code = symbolic.main(
                    [
                        "--source",
                        "Probe.sol",
                        "--contract",
                        "Probe",
                        "--all-functions",
                        "--solc-attempt",
                        str(root / "solc-attempt"),
                        "--solar-attempt",
                        str(root / "solar-attempt"),
                        "--solc",
                        "missing-compiler-must-not-be-resolved",
                        "--forge",
                        str(forge),
                        "--output-root",
                        str(root / "saved-out"),
                    ]
                )
            saved = json.loads(stdout.getvalue())
            self.assertEqual(code, 0, saved)
            self.assertEqual(saved["checked"], ["f(uint256)", "<other calldata>"])
            self.assertEqual(saved["sources"], ["Probe.sol"])
            argv = json.loads((root / "argv").read_text())
            self.assertEqual(
                argv[argv.index("--calldata-lengths") + 1], "0,1,3,4,35,36,68"
            )

            (root / "solar-attempt" / "input.json").write_text("{}")
            args.solc_attempt = root / "solc-attempt"
            args.solar_attempt = root / "solar-attempt"
            args.source = "Probe.sol"
            with self.assertRaisesRegex(ValueError, "different inputs"):
                symbolic.run(args, root / "saved-out")

    def test_runs_the_constructor_before_comparing_runtimes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            forge = self._write_forge(root / "forge")
            constructor = [{"name": "f", "type": "uint256"}]
            solc = _artifact(creation="0x60aa", constructor=constructor)
            solar = _artifact(creation="0x60bb", constructor=constructor)
            deployed = {
                label: {
                    "success": True,
                    "address": "0x5fbdb2315678afecb367f032d93f642f64180aa3",
                    "runtime": runtime,
                    "logs": [],
                    "storage_root": "0xaa",
                    "balance": "0",
                    "nonce": 1,
                }
                for label, runtime in (("solc", "0x6011"), ("solar", "0x6022"))
            }
            args = _args(
                source=root / "Probe.sol",
                contract="Probe",
                signature="f(uint256)",
                solc="solc",
                solar="solar",
                forge=str(forge),
                solver="z3",
                solc_attempt=None,
                solar_attempt=None,
                constructor_value=3,
                optimize=True,
                optimizer_runs=200,
                via_ir=True,
                project_root=None,
                include_path=[],
                remapping=[],
            )
            (root / "Probe.sol").write_text("")
            with (
                patch.object(symbolic, "_resolve_executable", side_effect=str),
                patch.object(
                    symbolic,
                    "_standard_input",
                    return_value={
                        "input": {},
                        "root_source": "Probe.sol",
                        "settings": {},
                        "sources": {"Probe.sol": {}},
                        "sha256": "0",
                    },
                ),
                patch.object(symbolic, "_compile", side_effect=[solc, solar]),
                patch.object(symbolic, "_anvil_executable", return_value="anvil"),
                patch.object(symbolic, "_deploy", return_value=deployed) as deploy,
            ):
                result = symbolic.run(args, root / "out")

            zero = "00" * 32
            deploy.assert_called_once_with(
                "anvil",
                "0x60aa" + zero,
                "0x60bb" + zero,
                value=3,
                evm_version="osaka",
                timeout=30.5,
            )
            self.assertEqual(result["status"], "bounded_agreement", result)
            self.assertEqual(result["checked"], ["<constructor>", "f(uint256)"])
            self.assertEqual(result["bounds"]["constructor"], "default_args")
            self.assertEqual(
                result["constructor"],
                {
                    "args": "0x" + zero,
                    "value": "3",
                    "address": "0x5fbdb2315678afecb367f032d93f642f64180aa3",
                },
            )
            project = pathlib.Path(result["project"])
            self.assertEqual((project / "solc.hex").read_text(), "6011\n")
            self.assertEqual((project / "solar.hex").read_text(), "6022\n")

            # A constructor that reverts in one build leaves no runtime to compare.
            deployed["solar"] = {"success": False, "data": "0x", "error": "reverted"}
            with (
                patch.object(symbolic, "_resolve_executable", side_effect=str),
                patch.object(
                    symbolic,
                    "_standard_input",
                    return_value={
                        "input": {},
                        "root_source": "Probe.sol",
                        "settings": {},
                        "sources": {"Probe.sol": {}},
                        "sha256": "0",
                    },
                ),
                patch.object(symbolic, "_compile", side_effect=[solc, solar]),
                patch.object(symbolic, "_anvil_executable", return_value="anvil"),
                patch.object(symbolic, "_deploy", return_value=deployed),
            ):
                result = symbolic.run(args, root / "out")
            self.assertEqual(result["status"], "mismatch")
            self.assertEqual(result["differs_in"], "constructor")
            self.assertEqual(result["checked"], ["<constructor>"])

    def test_import_discovery_uses_requested_evm_version(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source_dir = root / "src"
            source_dir.mkdir()
            source = source_dir / "Probe.sol"
            imported = source_dir / "Lib.sol"
            source_content = 'import "./Lib.sol"; contract Probe {}\n'
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

    def _output(self, runtime: str) -> dict[str, Any]:
        return {
            "sources": {"Probe.sol": {"id": 0}},
            "contracts": {
                "Probe.sol": {
                    "Probe": {
                        "abi": [_function()],
                        "evm": {
                            "deployedBytecode": {
                                "object": runtime,
                                "immutableReferences": {},
                                "linkReferences": {},
                            },
                            "methodIdentifiers": {"f(uint256)": "b3de648b"},
                        },
                    }
                }
            },
        }

    def _write_compiler(
        self,
        path: pathlib.Path,
        captured_input: pathlib.Path,
        runtime: str,
    ) -> pathlib.Path:
        script = (
            "#!/usr/bin/env python3\n"
            "import pathlib, sys\n"
            f"pathlib.Path({str(captured_input)!r}).write_text(sys.stdin.read())\n"
            f"print({json.dumps(self._output(runtime))!r})\n"
        )
        return self._write_executable(path, script)

    def _write_forge(self, path: pathlib.Path) -> pathlib.Path:
        script = (
            "#!/usr/bin/env python3\n"
            "import json, pathlib, sys\n"
            "assert pathlib.Path('foundry.toml').is_file()\n"
            "pathlib.Path(__file__).with_name('argv').write_text(json.dumps(sys.argv))\n"
            f"print({EQUIVALENT!r})\n"
        )
        return self._write_executable(path, script)

    def _write_executable(self, path: pathlib.Path, content: str) -> pathlib.Path:
        path.write_text(content)
        path.chmod(0o755)
        return path


if __name__ == "__main__":
    unittest.main()
