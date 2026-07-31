#!/usr/bin/env python3
"""Pure helpers for bounded Solc-vs-Solar symbolic differentials."""

from __future__ import annotations

import contextlib
import json
import os
import pathlib
import re
import secrets
import subprocess
import tempfile
import time
from collections.abc import Iterator
from typing import Any

import evm_runtime as evm


RESULT_SCHEMA = "solar:symbolic-differential@v1"
CAMPAIGN_SCHEMA = "solar:symbolic-differential-campaign@v1"
# Keep automatic dynamic-shape expansion small enough for multi-argument cross
# products while covering empty, singleton, and short multi-element encodings.
DEFAULT_SYMBOLIC_DYNAMIC_LENGTHS = (0, 1, 2, 3)
# Match Foundry's default hard bound so this wrapper cannot silently request a
# shape that the pinned symbolic engine must reject.
MAX_SYMBOLIC_DYNAMIC_LENGTH = 256
UNSUPPORTED_RUNTIME_OPCODES = {
    # The symbolic router and independent Anvil replay intentionally use
    # different callers and fresh execution environments. Reject opcodes that
    # could make a compiler-introduced context dependency invisible in one of
    # those fixed contexts.
    0x31: "BALANCE",
    0x32: "ORIGIN",
    0x33: "CALLER",
    0x3A: "GASPRICE",
    0x3B: "EXTCODESIZE",
    0x3C: "EXTCODECOPY",
    0x3F: "EXTCODEHASH",
    0x40: "BLOCKHASH",
    0x41: "COINBASE",
    0x42: "TIMESTAMP",
    0x43: "NUMBER",
    0x44: "PREVRANDAO",
    0x45: "GASLIMIT",
    0x46: "CHAINID",
    0x47: "SELFBALANCE",
    0x48: "BASEFEE",
    0x49: "BLOBHASH",
    0x4A: "BLOBBASEFEE",
    0x58: "PC",
    0x59: "MSIZE",
    0x5A: "GAS",
    # A pure entry point can reach a caller-supplied contract through an
    # interface that claims to be pure. The stateless comparison router cannot
    # soundly model those open-world calls, so the entire runtime is rejected.
    0xF0: "CREATE",
    0xF1: "CALL",
    0xF2: "CALLCODE",
    0xF4: "DELEGATECALL",
    0xF5: "CREATE2",
    0xFA: "STATICCALL",
}


def runtime_source_map_instructions(source_map: str | None) -> int | None:
    """Return the executable instruction count encoded by a source map."""
    if source_map is None or source_map == "":
        return None
    if not isinstance(source_map, str):
        raise ValueError("runtime source map must be text")
    return source_map.count(";") + 1


def runtime_scope_opcodes(
    runtime: str, *, instruction_count: int | None = None
) -> list[dict[str, Any]]:
    """Return fail-closed opcodes in the executable legacy EVM bytecode."""
    if not isinstance(runtime, str):
        raise ValueError("runtime bytecode must be text")
    if instruction_count is not None and (
        not isinstance(instruction_count, int)
        or isinstance(instruction_count, bool)
        or instruction_count <= 0
    ):
        raise ValueError("runtime instruction count must be a positive integer")
    payload = runtime.removeprefix("0x")
    if len(payload) % 2:
        raise ValueError("runtime bytecode must be byte-aligned hex")
    try:
        code = bytes.fromhex(payload)
    except ValueError as err:
        raise ValueError("runtime bytecode must be hex") from err
    if code.startswith(b"\xef"):
        return [{"offset": 0, "opcode": "EF_PREFIXED_NON_LEGACY_RUNTIME"}]
    found = []
    offset = 0
    instructions = 0
    while offset < len(code) and (
        instruction_count is None or instructions < instruction_count
    ):
        opcode = code[offset]
        if opcode in UNSUPPORTED_RUNTIME_OPCODES:
            found.append(
                {
                    "offset": offset,
                    "opcode": UNSUPPORTED_RUNTIME_OPCODES[opcode],
                }
            )
        offset += 1
        if 0x60 <= opcode <= 0x7F:
            offset += opcode - 0x5F
        instructions += 1
    if instruction_count is not None and instructions != instruction_count:
        raise ValueError("runtime source map exceeds deployed bytecode")
    if (
        instruction_count is not None
        and offset < len(code)
        and code[offset] != 0xFE
    ):
        raise ValueError(
            "runtime source map boundary is not followed by an INVALID data "
            "separator"
        )
    return found


def function_inventory(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
) -> dict[str, list[dict[str, Any]]]:
    """Inventory the shared runtime surface without silently intersecting ABIs."""
    solc_functions, solc_errors = _collect_functions(
        solc_artifact.get("abi"), "solc"
    )
    solar_functions, solar_errors = _collect_functions(
        solar_artifact.get("abi"), "Solar"
    )
    solc_hashes, solc_hash_errors = _collect_method_identifiers(
        solc_artifact.get(
            "hashes", solc_artifact.get("method_identifiers", {})
        ),
        "solc",
    )
    solar_hashes, solar_hash_errors = _collect_method_identifiers(
        solar_artifact.get(
            "hashes", solar_artifact.get("method_identifiers", {})
        ),
        "Solar",
    )
    errors = solc_errors + solar_errors + solc_hash_errors + solar_hash_errors
    for compiler, functions, identifiers in (
        ("solc", solc_functions, solc_hashes),
        ("Solar", solar_functions, solar_hashes),
    ):
        errors.extend(
            {
                "signature": signature,
                "compiler": compiler,
                "reason": "method identifier has no matching function ABI entry",
            }
            for signature in sorted(set(identifiers) - set(functions))
        )
    eligible = []
    excluded = []
    for signature in sorted(set(solc_functions) | set(solar_functions)):
        solc_entry = solc_functions.get(signature)
        solar_entry = solar_functions.get(signature)
        if solc_entry is None:
            errors.append(
                {
                    "signature": signature,
                    "compiler": "solc",
                    "reason": "function is missing from the solc ABI",
                }
            )
            continue
        if solar_entry is None:
            errors.append(
                {
                    "signature": signature,
                    "compiler": "Solar",
                    "reason": "function is missing from the Solar ABI",
                }
            )
            continue
        if _function_shape(solc_entry) != _function_shape(solar_entry):
            errors.append(
                {
                    "signature": signature,
                    "compiler": "both",
                    "reason": "compiler ABI shapes disagree",
                }
            )
            continue
        if solc_entry.get("stateMutability") != "pure":
            excluded.append(
                {
                    "signature": signature,
                    "reason": "state mutability is not pure",
                }
            )
            continue
        if not _has_supported_symbolic_inputs(solc_entry):
            excluded.append(
                {
                    "signature": signature,
                    "reason": "inputs use an unsupported ABI shape",
                }
            )
            continue
        solc_selector = _inventory_selector(solc_hashes.get(signature))
        solar_selector = _inventory_selector(solar_hashes.get(signature))
        if solc_selector is None or solar_selector is None:
            missing = []
            if solc_selector is None:
                missing.append("solc")
            if solar_selector is None:
                missing.append("Solar")
            errors.append(
                {
                    "signature": signature,
                    "compiler": " and ".join(missing),
                    "reason": "method identifier is missing or invalid",
                }
            )
            continue
        if solc_selector != solar_selector:
            errors.append(
                {
                    "signature": signature,
                    "compiler": "both",
                    "reason": (
                        "compiler selectors disagree: "
                        f"solc={solc_selector[2:]}, "
                        f"solar={solar_selector[2:]}"
                    ),
                }
            )
            continue
        eligible.append(
            _selected_function(signature, solc_entry, solc_selector)
        )

    return {
        "eligible": eligible,
        "excluded": sorted(excluded, key=lambda item: item["signature"]),
        "errors": sorted(
            errors,
            key=lambda item: (
                item.get("signature") or "",
                item.get("compiler") or "",
                item["reason"],
            ),
        ),
    }


def select_function(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    signature: str | None,
) -> dict[str, Any]:
    """Validate compiler agreement and select one supported pure function."""
    solc_abi = solc_artifact.get("abi")
    solar_abi = solar_artifact.get("abi")
    _validate_abi(solc_abi, "solc")
    _validate_abi(solar_abi, "Solar")
    if signature is not None:
        solc_entries = {
            _abi_signature(entry): entry
            for entry in solc_abi
            if entry.get("type") == "function"
        }
        solar_entries = {
            _abi_signature(entry): entry
            for entry in solar_abi
            if entry.get("type") == "function"
        }
        if signature not in solc_entries or signature not in solar_entries:
            raise ValueError(f"function `{signature}` was not found in both compiler ABIs")
        solc_entry = solc_entries[signature]
        solar_entry = solar_entries[signature]
        if _function_shape(solc_entry) != _function_shape(solar_entry):
            raise ValueError(f"compiler ABI disagreement for `{signature}`")
        if (
            solc_entry.get("stateMutability") != "pure"
            or solar_entry.get("stateMutability") != "pure"
        ):
            raise ValueError(f"function `{signature}` must be pure")
        if not _has_supported_symbolic_inputs(
            solc_entry
        ) or not _has_supported_symbolic_inputs(solar_entry):
            raise ValueError(
                f"function `{signature}` uses an unsupported symbolic ABI input"
            )

    solc_hashes = solc_artifact.get("hashes", solc_artifact.get("method_identifiers", {}))
    solar_hashes = solar_artifact.get(
        "hashes", solar_artifact.get("method_identifiers", {})
    )
    _validate_method_identifiers(solc_hashes, "solc")
    _validate_method_identifiers(solar_hashes, "Solar")
    selected = select_symbolic_pure_function(
        solc_abi,
        solar_abi,
        solc_hashes,
        signature,
    )
    selector = selected["selector"][2:]
    solar_selector = solar_hashes.get(selected["signature"], "").removeprefix("0x").lower()
    if selector != solar_selector:
        raise ValueError(
            f"compiler selector disagreement for `{selected['signature']}`: "
            f"solc={selector}, solar={solar_selector or 'missing'}"
        )
    solar_entries = {
        _abi_signature(entry): entry
        for entry in solar_abi
        if entry.get("type") == "function"
    }
    solc_entry = next(
        entry
        for entry in solc_abi
        if entry.get("type") == "function"
        and _abi_signature(entry) == selected["signature"]
    )
    solar_entry = solar_entries[selected["signature"]]
    if _function_shape(solc_entry) != _function_shape(solar_entry):
        raise ValueError(f"compiler ABI disagreement for `{selected['signature']}`")
    selected["abi"] = solc_entry
    return selected


def select_symbolic_pure_function(
    solc_abi: list[dict[str, Any]],
    solar_abi: list[dict[str, Any]],
    hashes: dict[str, str],
    signature: str | None,
) -> dict[str, Any]:
    """Select one supported pure function shared by both compiler ABIs."""
    _validate_abi(solc_abi, "solc")
    _validate_abi(solar_abi, "Solar")
    _validate_method_identifiers(hashes, "solc")
    solc_functions = {
        _abi_signature(entry): entry
        for entry in solc_abi
        if entry.get("type") == "function"
    }
    solar_functions = {
        _abi_signature(entry): entry
        for entry in solar_abi
        if entry.get("type") == "function"
    }
    candidates = []
    for candidate, solc_entry in solc_functions.items():
        solar_entry = solar_functions.get(candidate)
        if (
            solar_entry is not None
            and _has_supported_symbolic_inputs(solc_entry)
            and _has_supported_symbolic_inputs(solar_entry)
            and _function_shape(solc_entry) == _function_shape(solar_entry)
            and candidate in hashes
        ):
            candidates.append(candidate)

    if signature is None:
        if len(candidates) != 1:
            available = ", ".join(sorted(candidates)) or "none"
            raise ValueError(
                "select a pure function with supported symbolic inputs using "
                "--signature; "
                f"eligible functions: {available}"
            )
        signature = candidates[0]
    if signature not in solc_functions or signature not in solar_functions:
        raise ValueError(f"function `{signature}` is not present in both compiler ABIs")
    if signature not in candidates:
        raise ValueError(
            f"function `{signature}` is not a shared pure function with "
            "supported symbolic ABI inputs"
        )

    entry = solc_functions[signature]
    selector = hashes[signature].removeprefix("0x").lower()
    if len(selector) != 8:
        raise ValueError(f"invalid selector `{selector}` for `{signature}`")
    int(selector, 16)
    return {
        "name": entry["name"],
        "signature": signature,
        "selector": "0x" + selector,
        "inputs": [_canonical_type(item) for item in entry.get("inputs", [])],
        "outputs": [_canonical_type(item) for item in entry.get("outputs", [])],
        "test": f"checkDiff_{selector}",
    }


def target_calldata(selector: str, wrapper_calldata: str) -> str:
    """Replace a generated check function selector with the target selector."""
    selector = _normalized_selector(selector)
    if not isinstance(wrapper_calldata, str) or not wrapper_calldata.startswith("0x"):
        raise ValueError("wrapper calldata must be 0x-prefixed hex")
    payload = wrapper_calldata[2:]
    if len(payload) < 8 or len(payload) % 2:
        raise ValueError("wrapper calldata must contain a four-byte selector")
    int(payload, 16)
    return selector + payload[8:]


def classify_forge_json(report: dict[str, Any]) -> dict[str, Any]:
    """Extract exactly one symbolic result from Forge's stable JSON output."""
    matches = []
    for suite_name, suite in report.items():
        if not isinstance(suite, dict):
            continue
        test_results = suite.get("test_results", {})
        if not isinstance(test_results, dict):
            raise ValueError("Forge test_results must be an object")
        for test_name, result in test_results.items():
            if isinstance(result, dict) and isinstance(result.get("symbolic"), dict):
                matches.append((suite_name, test_name, result, result["symbolic"]))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one symbolic test result, found {len(matches)}")

    suite_name, test_name, result, symbolic = matches[0]
    forge_status = symbolic.get("status")
    outer_status = result.get("status")
    replay = symbolic.get("replay")
    bounds = symbolic.get("bounds")
    solver = symbolic.get("solver")
    assumptions = symbolic.get("assumptions", [])
    if not isinstance(bounds, dict) or not isinstance(solver, dict):
        raise ValueError("Forge symbolic bounds and solver metadata must be objects")
    if not isinstance(assumptions, list) or not all(
        isinstance(assumption, dict) for assumption in assumptions
    ):
        raise ValueError("Forge symbolic assumptions must be an array of objects")
    if forge_status == "pass":
        if (
            outer_status != "Success"
            or symbolic.get("counterexample") is not None
            or not isinstance(replay, dict)
            or replay.get("status") != "not_required"
        ):
            raise ValueError("inconsistent Forge pass result")
        status = "no_mismatch_within_bounds"
    elif forge_status == "incomplete":
        if (
            outer_status != "Failure"
            or not isinstance(symbolic.get("incomplete"), dict)
            or not isinstance(replay, dict)
            or replay.get("status") != "not_required"
        ):
            raise ValueError("inconsistent Forge incomplete result")
        status = "incomplete"
    elif forge_status == "fail_counterexample":
        artifact = symbolic.get("artifact")
        counterexample = symbolic.get("counterexample")
        if (
            outer_status != "Failure"
            or not isinstance(replay, dict)
            or replay.get("status") != "confirmed"
            or not isinstance(counterexample, dict)
            or not isinstance(artifact, dict)
            or artifact.get("schema") != "foundry:symbolic.counterexample@v1"
            or not isinstance(artifact.get("path"), str)
            or not artifact["path"]
        ):
            raise ValueError(
                "Forge counterexample was not concretely replay-confirmed with "
                "a durable artifact"
            )
        target_calldata("0x00000000", counterexample.get("calldata"))
        status = "replay_confirmed_mismatch"
    else:
        raise ValueError(f"unsupported Forge symbolic status `{forge_status}`")
    return {
        "status": status,
        "forge_status": forge_status,
        "suite": suite_name,
        "test": test_name,
        "replay": symbolic.get("replay"),
        "counterexample": symbolic.get("counterexample"),
        "artifact": symbolic.get("artifact"),
        "incomplete": symbolic.get("incomplete"),
        "bounds": bounds,
        "solver": solver,
        "assumptions": assumptions,
        "result": result,
    }


def confirm_outcomes(
    solc_outcome: dict[str, Any], solar_outcome: dict[str, Any]
) -> bool:
    """Return true only when independent concrete EVM outcomes differ exactly."""
    _validate_outcome(solc_outcome)
    _validate_outcome(solar_outcome)
    return solc_outcome != solar_outcome


def run_direct_replay(
    solc: str,
    anvil: str,
    evm_version: str,
    solc_runtime: str,
    solar_runtime: str,
    calldata: str,
    timeout: float,
    *,
    deadline: evm.Deadline | None = None,
) -> dict[str, Any]:
    """Replay one call against both runtimes on a fresh ephemeral Anvil."""
    proxy_source = pathlib.Path(__file__).with_name("StaticCallProxy.sol")
    proxy = evm.compile_standard_artifact(
        solc,
        proxy_source,
        "FandangoStaticCallProxy",
        timeout,
        kind="solc",
        evm_version=evm_version,
        deadline=deadline,
    )
    with _anvil(anvil, evm_version, timeout, deadline=deadline) as instance:
        rpc_url = instance["rpc_url"]
        target = evm.SOLC_ADDRESS
        proxy_calldata = _proxy_calldata(target, calldata)
        block_response = evm.rpc(
            rpc_url,
            "eth_getBlockByNumber",
            ["latest", False],
            timeout,
            deadline=deadline,
        )
        block = block_response.get("result")
        if not isinstance(block, dict):
            raise evm.InfraError(
                f"Anvil latest block was unavailable: {block_response!r}"
            )
        evm.set_code(
            rpc_url,
            evm.STATIC_PROXY_ADDRESS,
            proxy["runtime"],
            timeout,
            deadline=deadline,
        )
        evm.set_code(
            rpc_url,
            target,
            solc_runtime,
            timeout,
            deadline=deadline,
        )
        solc_outcome = evm.eth_call(
            rpc_url,
            evm.STATIC_PROXY_ADDRESS,
            proxy_calldata,
            timeout,
            deadline=deadline,
        )
        evm.set_code(
            rpc_url,
            target,
            solar_runtime,
            timeout,
            deadline=deadline,
        )
        solar_outcome = evm.eth_call(
            rpc_url,
            evm.STATIC_PROXY_ADDRESS,
            proxy_calldata,
            timeout,
            deadline=deadline,
        )
        return {
            "call_kind": "staticcall",
            "calldata": calldata,
            "implementation_address": target,
            "proxy_address": evm.STATIC_PROXY_ADDRESS,
            "rpc_block": "latest",
            "rpc_transaction": {
                "to": evm.STATIC_PROXY_ADDRESS,
                "data": proxy_calldata,
            },
            "anvil": {
                "command": instance["command"],
                "chain_id": instance["chain_id"],
                "block": block,
            },
            "solc": solc_outcome,
            "solar": solar_outcome,
            "proxy": proxy,
        }


def parse_json_output(stdout: str, stderr: str, label: str) -> dict[str, Any]:
    if not stdout.strip():
        detail = stderr.strip() or "no output"
        raise ValueError(f"{label} did not emit JSON: {detail}")
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as err:
        raise ValueError(f"{label} emitted invalid JSON: {err}") from err
    if not isinstance(value, dict):
        raise ValueError(f"{label} JSON must be an object")
    return value


def unit_replay_reproduced(
    report: dict[str, Any], expected_test: str, expected_calldata: str
) -> bool:
    """Check that durable artifact replay ran the intended test and failed."""
    matches = []
    for suite in report.values():
        if not isinstance(suite, dict):
            continue
        test_results = suite.get("test_results", {})
        if not isinstance(test_results, dict):
            return False
        for test_name, result in test_results.items():
            if test_name == expected_test and isinstance(result, dict):
                matches.append(result)
    if len(matches) != 1:
        return False
    result = matches[0]
    counterexamples = result.get("counterexample")
    if not isinstance(counterexamples, dict):
        return False
    counterexample = counterexamples.get("Single")
    return (
        result.get("status") == "Failure"
        and isinstance(result.get("kind"), dict)
        and isinstance(result["kind"].get("Unit"), dict)
        and isinstance(counterexample, dict)
        and counterexample.get("calldata") == expected_calldata
    )


def counterexample_artifact_matches(
    artifact: dict[str, Any], expected_test: str, expected_calldata: str
) -> bool:
    if not isinstance(artifact, dict):
        return False
    replay = artifact.get("replay")
    test = artifact.get("test")
    calls = artifact.get("calls")
    return (
        artifact.get("schema") == "foundry:symbolic.counterexample@v1"
        and isinstance(replay, dict)
        and replay.get("status") == "confirmed"
        and isinstance(test, dict)
        and test.get("test") == expected_test
        and isinstance(calls, list)
        and len(calls) == 1
        and isinstance(calls[0], dict)
        and calls[0].get("calldata") == expected_calldata
    )


def solidity_parameter_declarations(types: list[str]) -> list[str]:
    declarations = []
    for index, abi_type in enumerate(types):
        location = (
            " calldata"
            if abi_type in {"bytes", "string"} or "[" in abi_type
            else ""
        )
        declarations.append(f"{abi_type}{location} arg{index}")
    return declarations


def solidity_symbolic_parameters(
    inputs: list[dict[str, Any]],
) -> tuple[str, list[str]]:
    """Render ABI inputs as Solidity parameters, synthesizing tuple structs."""
    if not isinstance(inputs, list) or not all(
        isinstance(item, dict) for item in inputs
    ):
        raise ValueError("symbolic ABI inputs must be an array")
    definitions = []
    declarations = []
    for index, item in enumerate(inputs):
        _validate_abi_value(item, "symbolic")
        if not _is_supported_symbolic_input(item):
            raise ValueError("symbolic ABI input uses an unsupported shape")
        type_name = _solidity_symbolic_input_type(
            item,
            f"SymbolicInput{index}",
            definitions,
        )
        location = (
            " calldata"
            if item["type"].startswith("tuple")
            or item["type"] in {"bytes", "string"}
            or "[" in item["type"]
            else ""
        )
        declarations.append(f"{type_name}{location} arg{index}")
    return "\n\n".join(definitions), declarations


def _solidity_symbolic_input_type(
    item: dict[str, Any],
    name: str,
    definitions: list[str],
) -> str:
    abi_type = item["type"]
    if not abi_type.startswith("tuple"):
        return abi_type
    components = item["components"]
    fields = []
    for index, component in enumerate(components):
        field_type = _solidity_symbolic_input_type(
            component,
            f"{name}_{index}",
            definitions,
        )
        fields.append(f"        {field_type} field{index};")
    definitions.append(
        "\n".join(
            [
                f"    struct {name} {{",
                *fields,
                "    }",
            ]
        )
    )
    return name + abi_type[len("tuple") :]


def _collect_functions(
    abi: Any, label: str
) -> tuple[dict[str, dict[str, Any]], list[dict[str, Any]]]:
    if not isinstance(abi, list):
        return {}, [
            {
                "signature": None,
                "compiler": label,
                "reason": "ABI must be an array",
            }
        ]
    functions = {}
    errors = []
    duplicate_signatures = set()
    for index, entry in enumerate(abi):
        if not isinstance(entry, dict):
            errors.append(
                {
                    "signature": None,
                    "compiler": label,
                    "reason": f"ABI entry {index} must be an object",
                }
            )
            continue
        if entry.get("type") != "function":
            continue
        try:
            _validate_function_entry(entry, label)
            signature = _abi_signature(entry)
        except ValueError as err:
            errors.append(
                {
                    "signature": _best_effort_signature(entry),
                    "compiler": label,
                    "reason": f"ABI entry {index}: {err}",
                }
            )
            continue
        if signature in functions or signature in duplicate_signatures:
            duplicate_signatures.add(signature)
            functions.pop(signature, None)
            errors.append(
                {
                    "signature": signature,
                    "compiler": label,
                    "reason": "ABI contains duplicate function signatures",
                }
            )
            continue
        functions[signature] = entry
    return functions, errors


def _collect_method_identifiers(
    identifiers: Any, label: str
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    if not isinstance(identifiers, dict):
        return {}, [
            {
                "signature": None,
                "compiler": label,
                "reason": "method identifiers must be an object",
            }
        ]
    valid = {}
    errors = []
    for signature, selector in identifiers.items():
        if not isinstance(signature, str) or not isinstance(selector, str):
            errors.append(
                {
                    "signature": signature if isinstance(signature, str) else None,
                    "compiler": label,
                    "reason": "method identifier must map strings to strings",
                }
            )
            continue
        valid[signature] = selector
    return valid, errors


def _validate_function_entry(entry: dict[str, Any], label: str) -> None:
    if not isinstance(entry.get("name"), str) or not entry["name"]:
        raise ValueError(f"{label} ABI function name must be non-empty text")
    if not isinstance(entry.get("stateMutability"), str):
        raise ValueError(f"{label} ABI function stateMutability must be text")
    for field in ("inputs", "outputs"):
        values = entry.get(field)
        if not isinstance(values, list) or not all(
            isinstance(value, dict) for value in values
        ):
            raise ValueError(f"{label} ABI function {field} must be an array")
        for value in values:
            _validate_abi_value(value, label)


def _best_effort_signature(entry: dict[str, Any]) -> str | None:
    name = entry.get("name")
    inputs = entry.get("inputs")
    if (
        not isinstance(name, str)
        or not name
        or not isinstance(inputs, list)
        or not all(
            isinstance(value, dict) and isinstance(value.get("type"), str)
            for value in inputs
        )
    ):
        return None
    try:
        return _abi_signature(entry)
    except (KeyError, TypeError, ValueError):
        return None


def _inventory_selector(selector: str | None) -> str | None:
    if not isinstance(selector, str):
        return None
    payload = selector.removeprefix("0x").lower()
    if len(payload) != 8:
        return None
    try:
        int(payload, 16)
    except ValueError:
        return None
    return "0x" + payload


def _selected_function(
    signature: str, entry: dict[str, Any], selector: str
) -> dict[str, Any]:
    return {
        "name": entry["name"],
        "signature": signature,
        "selector": selector,
        "inputs": [_canonical_type(item) for item in entry.get("inputs", [])],
        "outputs": [_canonical_type(item) for item in entry.get("outputs", [])],
        "test": f"checkDiff_{selector[2:]}",
        "abi": entry,
    }


def _abi_signature(entry: dict[str, Any]) -> str:
    inputs = ",".join(_canonical_type(item) for item in entry.get("inputs", []))
    return f"{entry.get('name', '')}({inputs})"


def _validate_abi(abi: Any, label: str) -> None:
    if not isinstance(abi, list) or not all(isinstance(entry, dict) for entry in abi):
        raise ValueError(f"{label} ABI must be an array of objects")
    signatures = set()
    for entry in abi:
        if entry.get("type") != "function":
            continue
        _validate_function_entry(entry, label)
        signature = _abi_signature(entry)
        if signature in signatures:
            raise ValueError(
                f"{label} ABI contains duplicate function `{signature}`"
            )
        signatures.add(signature)


def _validate_method_identifiers(identifiers: Any, label: str) -> None:
    if not isinstance(identifiers, dict) or not all(
        isinstance(signature, str) and isinstance(selector, str)
        for signature, selector in identifiers.items()
    ):
        raise ValueError(f"{label} method identifiers must map strings to strings")


def _validate_abi_value(value: dict[str, Any], label: str) -> None:
    abi_type = value.get("type")
    if not isinstance(abi_type, str):
        raise ValueError(f"{label} ABI value type must be text")
    if not abi_type.startswith("tuple"):
        return
    components = value.get("components")
    if not isinstance(components, list) or not all(
        isinstance(component, dict) for component in components
    ):
        raise ValueError(f"{label} ABI tuple components must be an array")
    for component in components:
        _validate_abi_value(component, label)


def _canonical_type(item: dict[str, Any]) -> str:
    abi_type = item.get("type", "")
    if not abi_type.startswith("tuple"):
        return abi_type
    suffix = abi_type[len("tuple") :]
    components = ",".join(_canonical_type(component) for component in item.get("components", []))
    return f"({components}){suffix}"


def _function_shape(entry: dict[str, Any]) -> tuple[Any, ...]:
    return (
        entry.get("stateMutability"),
        tuple(_canonical_type(item) for item in entry.get("inputs", [])),
        tuple(_canonical_type(item) for item in entry.get("outputs", [])),
    )


def _has_supported_symbolic_inputs(entry: dict[str, Any]) -> bool:
    return (
        entry.get("stateMutability") == "pure"
        and all(
            _is_supported_symbolic_input(item)
            for item in entry.get("inputs", [])
        )
    )


def _is_supported_symbolic_input(item: dict[str, Any]) -> bool:
    abi_type = item.get("type", "")
    if not abi_type.startswith("tuple"):
        return _is_supported_symbolic_input_type(abi_type)
    components = item.get("components")
    return (
        _is_supported_symbolic_input_type(
            "uint256" + abi_type[len("tuple") :]
        )
        and isinstance(components, list)
        and bool(components)
        and all(
            isinstance(component, dict)
            and _is_supported_symbolic_input(component)
            for component in components
        )
    )


def _is_supported_symbolic_input_type(abi_type: str) -> bool:
    while abi_type.endswith("]"):
        start = abi_type.rfind("[")
        if start < 0:
            return False
        length = abi_type[start + 1 : -1]
        if length and (not length.isdigit() or int(length) <= 0):
            return False
        abi_type = abi_type[:start]
    if abi_type in {"address", "bool", "bytes", "string"}:
        return True
    if abi_type.startswith("uint") or abi_type.startswith("int"):
        width = abi_type[4:] if abi_type.startswith("uint") else abi_type[3:]
        return not width or (width.isdigit() and int(width) in range(8, 257, 8))
    if abi_type.startswith("bytes"):
        width = abi_type[5:]
        return width.isdigit() and 1 <= int(width) <= 32
    return False


def _normalized_selector(selector: str) -> str:
    if not isinstance(selector, str) or not selector.startswith("0x"):
        raise ValueError("selector must be 0x-prefixed hex")
    payload = selector[2:]
    if len(payload) != 8:
        raise ValueError("selector must be exactly four bytes")
    int(payload, 16)
    return "0x" + payload.lower()


def _proxy_calldata(target: str, calldata: str) -> str:
    target_payload = target.removeprefix("0x")
    if len(target_payload) != 40:
        raise ValueError("proxy target must be a 20-byte address")
    return "0x" + target_payload + calldata.removeprefix("0x")


def _validate_outcome(outcome: dict[str, Any]) -> None:
    if not isinstance(outcome, dict) or outcome.get("status") not in {"ok", "revert"}:
        raise ValueError("concrete outcome must have status `ok` or `revert`")
    data = outcome.get("data")
    if not isinstance(data, str) or not data.startswith("0x") or len(data) % 2:
        raise ValueError("concrete outcome data must be 0x-prefixed byte-aligned hex")
    try:
        int(data[2:] or "0", 16)
    except ValueError as err:
        raise ValueError("concrete outcome data is not hex") from err


@contextlib.contextmanager
def _anvil(
    anvil: str,
    evm_version: str,
    timeout: float,
    *,
    deadline: evm.Deadline | None = None,
) -> Iterator[dict[str, Any]]:
    deadline = deadline or evm.Deadline(timeout)
    chain_id = secrets.randbits(31) or 1
    with tempfile.TemporaryDirectory(prefix="solar-symbolic-anvil-") as temporary:
        log_path = pathlib.Path(temporary) / "anvil.log"
        with log_path.open("w", encoding="utf-8") as log:
            command = [
                anvil,
                "--host",
                "127.0.0.1",
                "--port",
                "0",
                "--chain-id",
                str(chain_id),
                "--hardfork",
                evm_version,
            ]
            process = subprocess.Popen(
                command,
                stdout=log,
                stderr=subprocess.STDOUT,
                text=True,
                env=_anvil_environment(),
                cwd=temporary,
                **evm.process_group_options(),
            )
            try:
                with evm.cleanup_process_on_signals(process):
                    rpc_url = _anvil_rpc_url(
                        process, log_path, chain_id, timeout, deadline
                    )
                    yield {
                        "rpc_url": rpc_url,
                        "chain_id": chain_id,
                        "command": command,
                    }
            finally:
                evm.terminate_process_tree(process)


def _anvil_environment() -> dict[str, str]:
    env = os.environ.copy()
    for name in list(env):
        if name.startswith(("ANVIL_", "FOUNDRY_")):
            del env[name]
    return env


def _anvil_rpc_url(
    process: subprocess.Popen[Any],
    log_path: pathlib.Path,
    chain_id: int,
    timeout: float,
    deadline: evm.Deadline,
) -> str:
    expected_chain_id = hex(chain_id)
    rpc_url = None
    while rpc_url is None:
        if process.poll() is not None:
            detail = log_path.read_text(encoding="utf-8", errors="replace").strip()
            raise evm.InfraError(
                f"anvil exited with status {process.returncode}: {detail}"
            )
        output = log_path.read_text(encoding="utf-8", errors="replace")
        match = re.search(r"Listening on 127\.0\.0\.1:(\d+)", output)
        if match:
            rpc_url = f"http://127.0.0.1:{match.group(1)}"
            break
        time.sleep(min(0.05, deadline.remaining("Anvil startup")))

    while True:
        if process.poll() is not None:
            raise evm.InfraError(f"anvil exited with status {process.returncode}")
        try:
            response = evm.rpc(
                rpc_url,
                "eth_chainId",
                [],
                min(timeout, 1),
                retries=0,
                deadline=deadline,
            )
        except evm.InfraError:
            pass
        else:
            if response.get("result") == expected_chain_id:
                return rpc_url
            if "result" in response:
                raise evm.InfraError(
                    "Anvil chain identity did not match the spawned process"
                )
        time.sleep(min(0.05, deadline.remaining("Anvil startup")))
