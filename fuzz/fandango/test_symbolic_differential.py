#!/usr/bin/env python3

from __future__ import annotations

import argparse
import contextlib
import io
import json
import os
import pathlib
import tempfile
import unittest
from typing import Any
from unittest.mock import patch

import symbolic_differential as symbolic


def _artifact(
    *,
    mutability: str = "pure",
    inputs: list[dict[str, Any]] | None = None,
    selector: str = "b3de648b",
    runtime: str = "0x60006000f3",
    creation: str | None = "0x6001600055",
    constructor: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
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
        "creation": creation,
        "creation_links": False,
        "has_immutables": False,
        "constructor_entry": {
            "inputs": constructor or [],
            "outputs": [],
            "stateMutability": "nonpayable",
        },
        "constructor": constructor or [],
        "payable_fallback": False,
    }


def _plan_args(**overrides: object) -> argparse.Namespace:
    values: dict[str, object] = {
        "signature": "f(uint256)",
        "include_view": True,
        "include_stateful": True,
        "include_payable": True,
        "symbolic_caller": False,
        "all_functions": False,
        "depth": 1,
        "max_checks": 256,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


def _checks(
    *,
    inputs: list[dict[str, Any]] | None = None,
    mutability: str = "pure",
    symbolic_caller: bool = False,
    all_functions: bool = False,
    prefix: tuple[str, ...] = (),
) -> list[dict[str, Any]]:
    artifact = _artifact(mutability=mutability, inputs=inputs)
    signature = next(iter(artifact["method_identifiers"]))
    checks, _ = symbolic._plan_checks(
        artifact,
        artifact,
        _plan_args(
            signature=signature,
            symbolic_caller=symbolic_caller,
            all_functions=all_functions,
        ),
        prefix,
    )
    return checks


def _symbolic_step(inputs: list[dict[str, Any]], mutability: str) -> dict[str, Any]:
    return {
        "kind": "symbolic",
        "function": {
            "signature": "g(bytes)",
            "selector": "0xb3de648b",
            "inputs": inputs,
            "mutability": mutability,
        },
        "caller": False,
    }


def _generate(
    checks: list[dict[str, Any]],
    *,
    runtime_only: bool = False,
    constructor: list[dict[str, Any]] | None = None,
    constructor_args: str | None = None,
    allow_precompile_addresses: bool = False,
) -> tuple[str, str]:
    solc = _artifact(constructor=constructor)
    solar = _artifact(
        runtime="0x60016000f3", creation="0x6002600055", constructor=constructor
    )
    with tempfile.TemporaryDirectory() as tmp:
        project = pathlib.Path(tmp)
        symbolic._write_project(
            project,
            solc,
            solar,
            checks,
            "osaka",
            max_dynamic_length=256,
            runtime_only=runtime_only,
            constructor_args=constructor_args,
            allow_precompile_addresses=allow_precompile_addresses,
        )
        return (
            (project / "test" / "SymbolicDifferential.t.sol").read_text(),
            (project / "foundry.toml").read_text(),
        )


def _forge_report(
    status: str,
    *,
    replay: str = "not_required",
    max_dynamic_length: int = 256,
    reason: str = "returndata",
    calldata: str = "0x12345678" + "00" * 96,
) -> dict[str, Any]:
    symbolic_result: dict[str, object] = {
        "status": status,
        "replay": {"status": replay},
        "bounds": {
            "timeout_seconds": 5,
            "max_paths": 32,
            "max_solver_queries": 100,
            "max_calldata_bytes": 512,
            "max_dynamic_length": max_dynamic_length,
            "exploration_order": "bfs",
            "storage_layout": "zero_init",
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
        symbolic_result["counterexample"] = {"calldata": calldata}
        symbolic_result["artifact"] = {
            "path": "cache/symbolic-counterexample.json",
        }
    test_result: dict[str, object] = {
        "status": outer_status,
        "symbolic": symbolic_result,
    }
    if status == "fail_counterexample":
        test_result["reason"] = reason
    test_results: dict[str, object] = {
        "checkSymbolicDifferential0(uint256,uint256,address)": test_result
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
        checks = _checks()
        self.assertEqual(
            symbolic._classify(_forge_report("pass"), checks),
            {"status": "bounded_agreement"},
        )
        self.assertEqual(
            symbolic._classify(_forge_report("incomplete"), checks),
            {
                "status": "incomplete",
                "reason": "path limit reached",
            },
        )

    def test_mismatch_names_what_differs_and_requires_replay(self) -> None:
        checks = _checks()
        result = symbolic._classify(
            _forge_report("fail_counterexample", replay="confirmed", reason="storage"),
            checks,
        )

        self.assertEqual(result["status"], "mismatch")
        self.assertEqual(result["differs_in"], "storage")
        self.assertEqual(result["counterexample"]["calldata"], "0xb3de648b" + "00" * 32)
        self.assertEqual(result["counterexample"]["value"], "0")

        result = symbolic._classify(_forge_report("fail_counterexample"), checks)
        self.assertEqual(
            result,
            {
                "status": "incomplete",
                "reason": "Forge did not concretely replay the counterexample",
            },
        )

    def test_logs_difference_with_inner_frames_is_incomplete(self) -> None:
        report = _forge_report("fail_counterexample", replay="confirmed", reason="logs")
        (check,) = _checks()
        check["log_limit"] = symbolic._inner_frame_log_limit("0xa0f1")

        self.assertEqual(
            symbolic._classify(report, [check]),
            {
                "status": "incomplete",
                "reason": "logs of reverted inner frames are not told apart from committed logs",
            },
        )
        self.assertIsNone(symbolic._inner_frame_log_limit("0xa0", "0xf1"))
        self.assertEqual(symbolic._classify(report, _checks())["status"], "mismatch")

    def test_unsupported_failure_is_incomplete(self) -> None:
        result = symbolic._classify(
            _forge_report(
                "fail_counterexample",
                replay="confirmed",
                reason="unsupported: SELFDESTRUCT within the comparison transaction",
            ),
            _checks(),
        )

        self.assertEqual(
            result,
            {
                "status": "incomplete",
                "reason": "SELFDESTRUCT within the comparison transaction",
            },
        )

    def test_failure_outside_the_comparison_is_incomplete(self) -> None:
        for reason in (
            "FOUNDRY::ASSUME",
            "panic: arithmetic underflow or overflow (0x11)",
        ):
            with self.subTest(reason=reason):
                result = symbolic._classify(
                    _forge_report(
                        "fail_counterexample", replay="confirmed", reason=reason
                    ),
                    _checks(),
                )

                self.assertEqual(result["status"], "incomplete")
                self.assertIn(reason, result["reason"])

    def test_one_unclassifiable_check_keeps_the_others(self) -> None:
        checks = _checks(all_functions=True)
        report = _forge_report("fail_counterexample", replay="confirmed")
        results = next(iter(report.values()))["test_results"]
        # A counterexample that did not replay must not hide a confirmed one.
        unconfirmed = _forge_report("fail_counterexample")
        results["checkSymbolicDifferential1(bytes,uint256,address)"] = next(
            iter(next(iter(unconfirmed.values()))["test_results"].values())
        )
        del results[next(iter(results))]
        results["checkSymbolicDifferential0(uint256,uint256,address)"] = next(
            iter(
                next(
                    iter(
                        _forge_report(
                            "fail_counterexample", replay="confirmed"
                        ).values()
                    )
                )["test_results"].values()
            )
        )

        result = symbolic._classify(report, checks)

        self.assertEqual(result["status"], "mismatch")
        self.assertEqual(
            [(item["label"], item["status"]) for item in result["checks"]],
            [
                ("f(uint256)", "mismatch"),
                ("f(uint256) <arbitrary arguments>", "incomplete"),
                ("<other calldata>", "incomplete"),
            ],
        )
        self.assertEqual(
            result["checks"][2]["reason"], "Forge did not report this check"
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
        (check,) = _checks(inputs=inputs, mutability="payable", symbolic_caller=True)

        decoded = symbolic._decode_counterexample(
            "0x12345678" + harness_args.hex(), check, None
        )

        self.assertEqual(decoded["calldata"], "0xb3de648b" + target_args.hex())
        self.assertEqual(
            decoded["caller"], "0x00000000000000000000000000000000000000ca"
        )
        self.assertEqual(decoded["value"], "5")
        self.assertEqual(
            decoded["observer"], "0x00000000000000000000000000000000000000fe"
        )

    def test_counterexample_splits_constructor_and_call_sequence(self) -> None:
        inputs = [{"name": "data", "type": "bytes"}]
        check = {
            "name": "checkSymbolicDifferential0",
            "label": "g(bytes) -> g(bytes) <arbitrary arguments>",
            "steps": [
                {"kind": "fixed", "calldata": "0xaabbccdd"},
                _symbolic_step(inputs, "nonpayable"),
                {
                    "kind": "raw_args",
                    "caller": False,
                    "function": _symbolic_step(inputs, "nonpayable")["function"],
                },
            ],
            "input_lengths": {},
        }
        constructor = [
            {"name": "owner", "type": "address"},
            {"name": "x", "type": "bytes"},
        ]

        def tail(payload: bytes) -> bytes:
            return len(payload).to_bytes(32) + payload.ljust(32, b"\0")

        # c0, c1, c_value, s1_a0, s1_value, s2_raw, s2_value, observer.
        heads = [
            bytes(12) + bytes.fromhex("00000000000000000000000000000000000000aa"),
            (256).to_bytes(32),
            (3).to_bytes(32),
            (320).to_bytes(32),
            (4).to_bytes(32),
            (384).to_bytes(32),
            (5).to_bytes(32),
            bytes(12) + bytes.fromhex("00000000000000000000000000000000000000fe"),
        ]
        wrapper = (
            b"".join(heads) + tail(b"\x01") + tail(b"\x02\x03") + tail(b"\x04\x05")
        )

        decoded = symbolic._decode_counterexample(
            "0x12345678" + wrapper.hex(), check, constructor
        )

        self.assertEqual(
            decoded["constructor_args"],
            "0x" + (heads[0] + (64).to_bytes(32) + tail(b"\x01")).hex(),
        )
        self.assertEqual(decoded["constructor_value"], "3")
        self.assertEqual(
            [(call["calldata"], call.get("value")) for call in decoded["calls"]],
            [
                ("0xaabbccdd", None),
                ("0xb3de648b" + ((32).to_bytes(32) + tail(b"\x02\x03")).hex(), "4"),
                ("0xb3de648b0405", "5"),
            ],
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
            "storage_layout": "zero_init",
            "dynamic_lengths": [0, 1],
        }
        report = _forge_report("pass")
        checks = _checks()

        self.assertEqual(
            symbolic._classify(report, checks, expected),
            {"status": "bounded_agreement"},
        )
        contract_report = next(iter(report.values()))
        symbolic_result = next(iter(contract_report["test_results"].values()))
        symbolic_result["symbolic"]["bounds"]["max_paths"] = 31
        result = symbolic._classify(report, checks, expected)
        self.assertEqual(result["status"], "incomplete")
        self.assertIn("max_paths=31", result["reason"])


class CheckPlanningTests(unittest.TestCase):
    def test_all_functions_adds_arbitrary_and_unmatched_calldata(self) -> None:
        checks = _checks(all_functions=True)

        self.assertEqual(
            [check["label"] for check in checks],
            ["f(uint256)", "f(uint256) <arbitrary arguments>", "<other calldata>"],
        )
        raw_args = checks[1]["steps"][0]
        self.assertEqual(raw_args["kind"], "raw_args")
        self.assertEqual(raw_args["lengths"], (0, 31, 32, 64))
        self.assertEqual(checks[1]["input_lengths"], {"s0_raw": (0, 31, 32, 64)})
        raw = checks[2]["steps"][0]
        self.assertEqual(raw["kind"], "raw")
        self.assertEqual(raw["known_selectors"], ["0xb3de648b"])

    def test_abi_disagreements_fail_closed(self) -> None:
        solar = _artifact(selector="aaaaaaaa")
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "selector disagreement"):
            symbolic._plan_checks(
                _artifact(), solar, _plan_args(all_functions=True), ()
            )

        solar = _artifact()
        solar["abi"] = []
        with self.assertRaisesRegex(symbolic.AbiDisagreement, "only Solc has"):
            symbolic._plan_checks(
                _artifact(), solar, _plan_args(all_functions=True), ()
            )

    def test_depth_prefixes_state_changing_calls(self) -> None:
        artifact = _artifact(mutability="nonpayable")
        artifact["abi"].append(
            {
                "type": "function",
                "name": "g",
                "inputs": [],
                "outputs": [],
                "stateMutability": "view",
            }
        )
        artifact["method_identifiers"]["g()"] = "e2179b8e"

        checks, skipped = symbolic._plan_checks(
            artifact, artifact, _plan_args(all_functions=True, depth=2), ()
        )

        self.assertEqual(skipped, [])
        labels = [check["label"] for check in checks]
        self.assertEqual(len(labels), 10)
        self.assertEqual(
            labels[:5],
            [
                "f(uint256)",
                "g()",
                "f(uint256) <arbitrary arguments>",
                "g() <arbitrary arguments>",
                "<other calldata>",
            ],
        )
        self.assertTrue(all(label.startswith("f(uint256) -> ") for label in labels[5:]))
        with self.assertRaisesRegex(ValueError, "10 call sequences exceed"):
            symbolic._plan_checks(
                artifact,
                artifact,
                _plan_args(all_functions=True, depth=2, max_checks=9),
                (),
            )

    def test_all_functions_skips_unsupported_inputs(self) -> None:
        artifact = _artifact(inputs=[{"name": "f", "type": "function"}])

        checks, skipped = symbolic._plan_checks(
            artifact, artifact, _plan_args(all_functions=True), ()
        )

        self.assertEqual([check["label"] for check in checks], ["<other calldata>"])
        self.assertEqual(skipped[0]["signature"], "f(function)")

    def test_signature_with_depth_reports_skipped_functions(self) -> None:
        artifact = _artifact()
        artifact["abi"].append(
            {
                "type": "function",
                "name": "g",
                "inputs": [{"name": "f", "type": "function"}],
                "outputs": [],
                "stateMutability": "nonpayable",
            }
        )
        artifact["method_identifiers"]["g(function)"] = "c3de648b"

        _, skipped = symbolic._plan_checks(artifact, artifact, _plan_args(depth=2), ())

        self.assertEqual([item["signature"] for item in skipped], ["g(function)"])

    def test_selector_disagreement_on_an_unsupported_function_is_an_abi_mismatch(
        self,
    ) -> None:
        inputs = [{"name": "f", "type": "function"}]

        with self.assertRaises(symbolic.AbiDisagreement):
            symbolic._plan_checks(
                _artifact(inputs=inputs),
                _artifact(inputs=inputs, selector="aaaaaaaa"),
                _plan_args(all_functions=True),
                (),
            )

    def test_runtime_opcodes_skip_push_data_and_metadata(self) -> None:
        # PUSH1 0x5d, TSTORE, then CBOR metadata whose bytes include 0xf5.
        metadata = bytes.fromhex("a164736f6c6343f5f5f5") + (10).to_bytes(2)
        runtime = "0x605d5d" + metadata.hex()

        self.assertEqual(symbolic._runtime_opcodes(runtime), {0x60, 0x5D})


class ProjectGenerationTests(unittest.TestCase):
    def test_pure_harness_runs_both_runtimes_at_one_address(self) -> None:
        test_source, config = _generate(_checks(), runtime_only=True)

        self.assertIn(
            "function checkSymbolicDifferential0(uint256 s0_a0, uint256 s0_value, "
            "address observer) public",
            test_source,
        )
        self.assertIn(
            "calls[0] = Call(abi.encodeWithSelector(bytes4(0xb3de648b), s0_a0), "
            "address(this), false, s0_value);",
            test_source,
        )
        self.assertNotIn("staticcall", test_source)
        self.assertIn("60006000f3", test_source)
        self.assertIn("60016000f3", test_source)
        self.assertIn("bytes memory initA = _runtimeInit(SOLC_RUNTIME);", test_source)
        self.assertIn("_compare(initA, initB, 0, start, calls, watched);", test_source)
        self.assertNotIn("function setUp", test_source)
        self.assertNotIn("{gas:", test_source)
        self.assertNotIn("MAX_DATA_BYTES", test_source)
        run_a = test_source.index(
            "Outcome memory a = _run(initA, initValue, start, calls, watched);"
        )
        revert = test_source.index("assert(vm.revertToStateAndDelete(snapshot));")
        run_b = test_source.index(
            "Outcome memory b = _run(initB, initValue, start, calls, watched);"
        )
        self.assertLess(test_source.index("vm.snapshotState()"), run_a)
        self.assertLess(run_a, revert)
        self.assertLess(revert, run_b)
        self.assertIn('evm_version = "osaka"', config)
        self.assertIn("max_dynamic_length = 256", config)
        self.assertIn("symbolic_call_targets = true", config)
        self.assertNotIn("dynamic_lengths = {", config)

    def test_harness_names_every_comparison(self) -> None:
        test_source, config = _generate(_checks(mutability="nonpayable"))

        for what in (
            "constructor success",
            "constructor returndata",
            "call success",
            "returndata",
            "logs",
            "storage",
            "balance",
            "nonce",
            "code presence",
            "created accounts",
        ):
            self.assertIn(f'"{what}"', test_source)
        self.assertIn(
            '"unsupported: SELFDESTRUCT within the comparison transaction"', test_source
        )
        self.assertIn("vm.assertTrue(equal, what);", test_source)
        self.assertNotIn("assert(a.", test_source)
        self.assertIn("vm.computeCreateAddress(target, k)", test_source)
        self.assertIn(
            "returndatacopy(add(initRet, 0x20), 0, returndatasize())", test_source
        )
        self.assertIn('storage_layout = "zero_init"', config)

    def test_constructor_arguments_and_value_are_symbolic_or_fixed(self) -> None:
        constructor = [{"name": "owner", "type": "address"}]
        test_source, _ = _generate(_checks(), constructor=constructor)

        self.assertIn(
            "function checkSymbolicDifferential0(address c0, uint256 c_value, "
            "uint256 s0_a0, uint256 s0_value, address observer) public",
            test_source,
        )
        self.assertIn(
            'bytes private constant SOLC_INIT = hex"6001600055";', test_source
        )
        self.assertIn(
            "bytes memory initA = abi.encodePacked(SOLC_INIT, abi.encode(c0));",
            test_source,
        )
        self.assertIn(
            "_compare(initA, initB, c_value, start, calls, watched);", test_source
        )
        self.assertIn(
            "target := create(initValue, add(init, 0x20), mload(init))", test_source
        )

        test_source, _ = _generate(
            _checks(), constructor=constructor, constructor_args="0x" + "00" * 32
        )

        self.assertIn(
            "function checkSymbolicDifferential0(uint256 c_value, uint256 s0_a0, "
            "uint256 s0_value, address observer) public",
            test_source,
        )
        self.assertIn(
            f'bytes memory initA = abi.encodePacked(SOLC_INIT, hex"{"00" * 32}");',
            test_source,
        )

    def test_payable_harness_sends_symbolic_value_from_symbolic_caller(self) -> None:
        test_source, _ = _generate(_checks(mutability="payable", symbolic_caller=True))

        self.assertIn(
            "calls[0] = Call(abi.encodeWithSelector(bytes4(0xb3de648b), s0_a0), "
            "s0_caller, true, s0_value);",
            test_source,
        )
        self.assertIn(
            "if (call.value != 0) vm.deal(call.sender, call.value);", test_source
        )
        self.assertIn("if (call.prank) vm.prank(call.sender);", test_source)
        self.assertIn("target.call{value: call.value}(call.data);", test_source)
        self.assertIn("watched[1] = s0_caller;", test_source)
        self.assertIn("watched[2] = observer;", test_source)

    def test_raw_calldata_checks_use_inline_lengths(self) -> None:
        test_source, _ = _generate(_checks(all_functions=True))

        self.assertIn(
            "    /// forge-config: default.symbolic.dynamic_lengths = "
            "{ s0_raw = [0, 31, 32, 64] }\n"
            "    function checkSymbolicDifferential1(uint256 c_value, bytes calldata s0_raw, "
            "uint256 s0_value, address observer) public",
            test_source,
        )
        self.assertIn(
            "calls[0] = Call(abi.encodePacked(bytes4(0xb3de648b), s0_raw), "
            "address(this), false, s0_value);",
            test_source,
        )
        self.assertIn(
            "    /// forge-config: default.symbolic.dynamic_lengths = "
            "{ s0_raw = [0, 3, 4, 36] }\n"
            "    function checkSymbolicDifferential2(",
            test_source,
        )
        self.assertIn("vm.assume(selector != bytes4(0xb3de648b));", test_source)

    def test_address_inputs_exclude_precompiles_unless_allowed(self) -> None:
        checks = _checks(
            inputs=[{"name": "to", "type": "address"}], symbolic_caller=True
        )
        test_source, _ = _generate(checks)

        for name in ("s0_a0", "s0_caller", "observer"):
            self.assertIn(
                f"vm.assume(uint160({name}) == 0 || uint160({name}) > 0x100);",
                test_source,
            )

        test_source, _ = _generate(checks, allow_precompile_addresses=True)
        self.assertNotIn("> 0x100", test_source)

    def test_starting_storage_is_symbolic_and_validly_encoded(self) -> None:
        layout = {
            "storage": [
                {"slot": "0", "offset": 0, "type": "t_bool"},
                {"slot": "0", "offset": 1, "type": "t_enum(E)"},
                {"slot": "1", "offset": 0, "type": "t_mapping"},
            ],
            "types": {
                "t_bool": {
                    "encoding": "inplace",
                    "label": "bool",
                    "numberOfBytes": "1",
                },
                "t_enum(E)": {
                    "encoding": "inplace",
                    "label": "enum C.E",
                    "numberOfBytes": "1",
                },
                "t_mapping": {
                    "encoding": "mapping",
                    "label": "mapping",
                    "numberOfBytes": "32",
                },
            },
        }
        (check,) = _checks(mutability="nonpayable")
        check["starting_storage"] = symbolic._storage_constraints(layout, {"C.E": 3})
        test_source, _ = _generate([check])

        self.assertIn(
            "function checkSymbolicDifferential0(uint256 c_value, uint256 st0, "
            "uint256 s0_a0, uint256 s0_value, address observer) public",
            test_source,
        )
        self.assertIn("vm.assume(st0 & 0x" + "f" * 60 + "0000 == 0);", test_source)
        self.assertIn("vm.assume((st0 >> 0) & 0xff <= 1);", test_source)
        self.assertIn("vm.assume((st0 >> 8) & 0xff < 3);", test_source)
        self.assertIn(
            "start[0] = Slot(bytes32(uint256(0)), bytes32(st0));", test_source
        )
        self.assertIn(
            "_compareFromStartingStorage(initA, initB, c_value, start, calls, watched);",
            test_source,
        )
        self.assertIn("vm.store(target, start[i].slot, start[i].word);", test_source)
        self.assertNotIn("setArbitraryStorage", test_source)

        with self.assertRaisesRegex(ValueError, "unknown member count"):
            symbolic._storage_constraints(layout)

    def test_counterexample_reports_starting_storage(self) -> None:
        (check,) = _checks(mutability="nonpayable")
        check["starting_storage"] = [(3, (1 << 256) - 1, [], [])]
        # c_value, st3, s0_a0, s0_value, observer.
        wrapper = (0).to_bytes(32) + (7).to_bytes(32) + bytes(96)

        decoded = symbolic._decode_counterexample(
            "0x12345678" + wrapper.hex(), check, []
        )

        self.assertEqual(
            decoded["starting_storage"], {"3": "0x" + (7).to_bytes(32).hex()}
        )

    def test_storage_constraints_follow_the_layout(self) -> None:
        full = (1 << 256) - 1
        layout = {
            "storage": [
                {"slot": "0", "offset": 0, "type": "t_bool"},
                {"slot": "0", "offset": 1, "type": "t_address"},
                {"slot": "1", "offset": 0, "type": "t_struct(S)"},
                {"slot": "3", "offset": 0, "type": "t_array(t_uint64)3_storage"},
                {"slot": "4", "offset": 0, "type": "t_mapping"},
                {"slot": "5", "offset": 0, "type": "t_string_storage"},
                {"slot": "6", "offset": 0, "type": "t_flag"},
                {"slot": "7", "offset": 0, "type": "t_function"},
            ],
            "types": {
                "t_bool": {
                    "encoding": "inplace",
                    "label": "bool",
                    "numberOfBytes": "1",
                },
                "t_address": {
                    "encoding": "inplace",
                    "label": "address",
                    "numberOfBytes": "20",
                },
                "t_uint64": {
                    "encoding": "inplace",
                    "label": "uint64",
                    "numberOfBytes": "8",
                },
                "t_uint256": {
                    "encoding": "inplace",
                    "label": "uint256",
                    "numberOfBytes": "32",
                },
                "t_struct(S)": {
                    "encoding": "inplace",
                    "label": "struct C.S",
                    "numberOfBytes": "64",
                    "members": [
                        {"slot": "0", "offset": 0, "type": "t_uint256"},
                        {"slot": "1", "offset": 0, "type": "t_bool"},
                    ],
                },
                "t_array(t_uint64)3_storage": {
                    "base": "t_uint64",
                    "encoding": "inplace",
                    "label": "uint64[3]",
                    "numberOfBytes": "32",
                },
                "t_mapping": {
                    "encoding": "mapping",
                    "label": "mapping",
                    "numberOfBytes": "32",
                },
                "t_string_storage": {
                    "encoding": "bytes",
                    "label": "string",
                    "numberOfBytes": "32",
                },
                # A user-defined `bool` and an internal function pointer hide
                # their encodings behind their labels.
                "t_flag": {
                    "encoding": "inplace",
                    "label": "Flag",
                    "numberOfBytes": "1",
                },
                "t_function": {
                    "encoding": "inplace",
                    "label": "function (uint256) returns (uint256)",
                    "numberOfBytes": "8",
                },
            },
        }

        self.assertEqual(
            symbolic._storage_constraints(layout),
            [
                (0, (1 << 168) - 1, [0], []),
                (1, full, [], []),
                (2, 0xFF, [0], []),
                (3, (1 << 192) - 1, [], []),
            ],
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
        test_source, _ = _generate(_checks(inputs=inputs))

        self.assertIn("struct checkSymbolicDifferential0_s0_a0 {", test_source)
        self.assertIn("struct checkSymbolicDifferential0_s0_a0_1 {", test_source)
        self.assertIn(
            "function checkSymbolicDifferential0(uint256 c_value, "
            "checkSymbolicDifferential0_s0_a0[2] calldata s0_a0, uint256 s0_value, "
            "address observer) public",
            test_source,
        )

    def test_prefix_calls_run_before_symbolic_calls(self) -> None:
        checks = _checks(mutability="view", prefix=("0x12345678", "0x"))
        test_source, _ = _generate(checks)

        self.assertIn(
            'calls[0] = Call(hex"12345678", address(this), false, 0);', test_source
        )
        self.assertIn('calls[1] = Call(hex"", address(this), false, 0);', test_source)
        self.assertIn(
            "calls[2] = Call(abi.encodeWithSelector(bytes4(0xb3de648b), s2_a0), "
            "address(this), false, s2_value);",
            test_source,
        )
        self.assertIn("Call[] memory calls = new Call[](3);", test_source)
        self.assertIn('_same(a.oks[i] == b.oks[i], "call success");', test_source)


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
            self._write_executable(root / "z3", "#!/bin/sh\nexit 0\n")
            self.enterContext(
                patch.dict(
                    os.environ, {"PATH": f"{root}{os.pathsep}{os.environ['PATH']}"}
                )
            )
            args = argparse.Namespace(
                source=source,
                contract="Probe",
                signature="f(uint256)",
                solc=str(solc),
                solar=str(solar),
                forge=str(forge),
                solver="z3",
                timeout=10.0,
                max_paths=32,
                max_solver_queries=100,
                max_calldata_bytes=512,
                max_depth=None,
                symbolic_timeout=5,
                exploration_order="bfs",
                dynamic_lengths=(0, 1),
                input_length=[],
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
            self.assertEqual((root / "solver").read_text(), "z3")
            args.solver = "./z3"
            with contextlib.chdir(root):
                symbolic.run(args, root / "path-out")
            self.assertEqual(
                (root / "solver").read_text(), str((root / "z3").resolve())
            )
            args.solver = "z3"
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
                                "z3",
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
            self.assertEqual(logging_result["status"], "bounded_agreement")
            self.assertEqual(
                logging_result["limitations"],
                [
                    "logs of reverted inner frames are not told apart from committed logs"
                ],
            )
            fixed_logs = {"isolated_transactions": False, "reverted_logs_dropped": True}
            with patch.object(symbolic, "_forge_semantics", return_value=fixed_logs):
                dropped_result = symbolic.run(args, root / "dropped-logs-out")
            self.assertEqual(dropped_result["limitations"], [])
            deployed["object"] = "60" * 400
            # The constructor's TSTORE would stay visible to the first call.
            output["contracts"]["Probe.sol"]["Probe"]["evm"]["bytecode"] = {
                "object": "5d"
            }
            for directory in (args.solc_attempt, args.solar_attempt):
                (directory / "stdout.txt").write_text(json.dumps(output))
            transient_result = symbolic.run(args, root / "transient-out")
            self.assertEqual(transient_result["status"], "incomplete")
            self.assertIn("transient storage", transient_result["reason"])
            # Forge with call isolation runs each call as its own transaction.
            fixed = {"isolated_transactions": True, "reverted_logs_dropped": True}
            with patch.object(symbolic, "_forge_semantics", return_value=fixed):
                isolated_result = symbolic.run(args, root / "isolated-out")
            self.assertEqual(isolated_result["status"], "bounded_agreement")
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
            "import json, pathlib, sys\n"
            "if '--help' in sys.argv:\n"
            "    print('--allow-local-compiler')\n"
            "    raise SystemExit\n"
            "solver = sys.argv.index('--symbolic-solver')\n"
            "pathlib.Path(__file__).with_name('solver').write_text(sys.argv[solver + 1])\n"
            "index = sys.argv.index('--symbolic-max-dynamic-length')\n"
            f"assert sys.argv[index + 1] == {str(max_dynamic_length)!r}\n"
            f"assert 'max_dynamic_length = {max_dynamic_length}' in "
            "pathlib.Path('foundry.toml').read_text()\n"
            f"report = json.loads({json.dumps(_forge_report('pass', max_dynamic_length=max_dynamic_length))!r})\n"
            "if '--symbolic-max-depth' in sys.argv:\n"
            "    depth = int(sys.argv[sys.argv.index('--symbolic-max-depth') + 1])\n"
            "    for contract in report.values():\n"
            "        for test in contract['test_results'].values():\n"
            "            test['symbolic']['bounds']['max_depth'] = depth\n"
            "print(json.dumps(report))\n"
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
