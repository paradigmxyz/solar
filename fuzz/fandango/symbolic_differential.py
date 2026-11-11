#!/usr/bin/env python3
"""Run a focused Solc-vs-Solar symbolic differential with `forge equivalence`."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import math
import os
import pathlib
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Iterator
from typing import IO, Any

sys.path.insert(
    0, str(pathlib.Path(__file__).resolve().parents[2] / "tools/compiler-diff")
)
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import evm_runtime as evm
from compiler_diff.artifacts import read_attempt
from compiler_diff.compare import canonical_type as _canonical_type

SCHEMA = "solar:solsymdiff@v3"
DEFAULT_DYNAMIC_LENGTHS = (0, 1, 2, 3)
MAX_DYNAMIC_LENGTH = 256
SYMBOLIC_MAX_PATHS = 1024
SYMBOLIC_MAX_SOLVER_QUERIES = 10_000
SYMBOLIC_MAX_DEPTH = 100_000
SYMBOLIC_MAX_CALLDATA_BYTES = 1024
# Covers empty, one-byte, short, selector-only, and one-word calldata.
RAW_CALLDATA_LENGTHS = (0, 1, 3, 4, 36)
OTHER_CALLDATA = "<other calldata>"
CONSTRUCTOR = "<constructor>"
# Extra seconds a check may run past the Forge timeout before it is killed, so
# Forge can report an incomplete result itself.
FORGE_GRACE_SECONDS = 60
# CREATE and CREATE2.
_CREATE_OPCODES = {0xF0, 0xF5}
# Forge's default sender. Its CREATE at nonce 1 is Forge's test contract
# address, where `forge equivalence` runs both runtimes, so the constructors
# run at the address the runtimes later see as ADDRESS.
DEPLOYER = "0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38"
DEPLOYER_NONCE = 1
# CREATE, CALL, CALLCODE, DELEGATECALL, CREATE2, SELFDESTRUCT: ways a
# constructor can change another account, directly or through code it runs in
# its own context.
_OUTSIDE_EFFECT_OPCODES = {0xF0, 0xF1, 0xF2, 0xF4, 0xF5, 0xFF}


class AbiDisagreement(ValueError):
    """The two builds expose different external functions or selectors."""


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="solsymdiff",
        description="Symbolically compare a contract compiled by Solc and Solar.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--source", required=True)
    parser.add_argument("--solc-attempt", type=pathlib.Path)
    parser.add_argument("--solar-attempt", type=pathlib.Path)
    parser.add_argument("--output-root", type=pathlib.Path)
    parser.add_argument("--contract", required=True)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--signature")
    selection.add_argument(
        "--all-functions",
        action="store_true",
        help="compare every function, plus raw calldata of fixed lengths",
    )
    parser.add_argument(
        "--constructor-args",
        type=_hex_data,
        metavar="HEX",
        help=(
            "ABI-encoded constructor arguments (default: the zero value of "
            "every constructor input)"
        ),
    )
    parser.add_argument(
        "--constructor-value",
        type=_wei,
        default=0,
        metavar="WEI",
        help="value sent with the deployment",
    )
    parser.add_argument(
        "--runtime-only",
        action="store_true",
        help="compare the runtimes as compiled, without running the constructor",
    )
    parser.add_argument(
        "--arbitrary-storage",
        action="store_true",
        help="start each call from arbitrary storage instead of empty storage",
    )
    parser.add_argument(
        "--value",
        type=_wei,
        metavar="WEI",
        help="fix the call value instead of leaving it symbolic",
    )
    parser.add_argument(
        "--promise-no-reent",
        action="store_true",
        help=(
            "assume calls into unknown code never call back, so state-changing "
            "calls into it are abstracted instead of stopping the check"
        ),
    )
    parser.add_argument(
        "--ignore-out-of-gas",
        action="store_true",
        help="leave out inputs on which either build runs out of gas",
    )
    parser.add_argument("--project-root", type=pathlib.Path)
    parser.add_argument(
        "--include-path", type=pathlib.Path, action="append", default=[]
    )
    parser.add_argument("--remapping", action="append", default=[])
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--forge", default="forge")
    parser.add_argument("--solver", default="z3")
    parser.add_argument(
        "--timeout",
        type=_positive_seconds,
        default=60.0,
        help="seconds for each compiler run and each equivalence check",
    )
    parser.add_argument("--max-paths", type=_positive_int, default=SYMBOLIC_MAX_PATHS)
    parser.add_argument(
        "--max-solver-queries",
        type=_positive_int,
        default=SYMBOLIC_MAX_SOLVER_QUERIES,
    )
    parser.add_argument(
        "--max-depth",
        type=_positive_int,
        default=SYMBOLIC_MAX_DEPTH,
        help="opcode limit per path",
    )
    parser.add_argument(
        "--max-calldata-bytes",
        type=_positive_int,
        default=SYMBOLIC_MAX_CALLDATA_BYTES,
        help="longest raw calldata of --all-functions",
    )
    parser.add_argument(
        "--dynamic-lengths",
        type=_dynamic_lengths,
        default=DEFAULT_DYNAMIC_LENGTHS,
        help="lengths of dynamic ABI inputs",
    )
    parser.add_argument("--evm-version", default="osaka")
    parser.add_argument(
        "--optimize",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    parser.add_argument("--optimizer-runs", type=_nonnegative_int, default=200)
    parser.add_argument(
        "--via-ir",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    args = parser.parse_args(argv)

    try:
        result = run(args, output_root=args.output_root)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as err:
        result = {
            "schema": SCHEMA,
            "status": "incomplete",
            "reason": str(err),
            "source": str(args.source),
            "contract": args.contract,
            "signature": args.signature,
        }

    print(json.dumps(result, indent=2, sort_keys=True))
    return {
        "bounded_agreement": 0,
        "mismatch": 1,
        "incomplete": 2,
    }[result["status"]]


def run(
    args: argparse.Namespace, output_root: pathlib.Path | None = None
) -> dict[str, Any]:
    source = args.source
    saved_solc = args.solc_attempt
    saved_solar = args.solar_attempt
    if bool(saved_solc) != bool(saved_solar):
        raise ValueError("--solc-attempt and --solar-attempt must be supplied together")
    tool_names = ("forge",) if saved_solc else ("solc", "solar", "forge")
    tools = {name: _resolve_executable(getattr(args, name)) for name in tool_names}
    # Forge maps bare solver names such as `bitwuzla` to their own commands and
    # runs any path as a z3-compatible command.
    tools["solver"] = (
        args.solver
        if not os.path.dirname(args.solver)
        else _resolve_executable(args.solver)
    )
    if saved_solc:
        left_input, left_output = read_attempt(saved_solc)
        right_input, right_output = read_attempt(saved_solar)
        if json.dumps(left_input, sort_keys=True) != json.dumps(
            right_input, sort_keys=True
        ):
            raise ValueError("saved compiler attempts used different inputs")
        settings = left_input.get("settings", {})
        if not settings.get("evmVersion"):
            raise ValueError("saved symbolic inputs require an explicit evmVersion")
        args.evm_version = settings["evmVersion"]
        root_source = str(args.source)
        if root_source not in left_input.get("sources", {}):
            raise ValueError("--source must name a source unit in the saved input")
        serialized_input = json.dumps(
            left_input, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        )
        standard_input: dict[str, Any] = {
            "input": left_input,
            "root_source": root_source,
            "settings": settings,
            "sources": left_input["sources"],
            "sha256": hashlib.sha256(serialized_input.encode()).hexdigest(),
        }
        for output in (left_output, right_output):
            deployed = (
                output.get("contracts", {})
                .get(root_source, {})
                .get(args.contract, {})
                .get("evm", {})
                .get("deployedBytecode", {})
            )
            if not all(
                isinstance(deployed.get(key), dict)
                for key in ("immutableReferences", "linkReferences")
            ):
                raise ValueError(
                    "saved artifacts require immutableReferences and linkReferences outputs"
                )
        solc_artifact = _artifact(left_output, root_source, args.contract, "Solc")
        solar_artifact = _artifact(right_output, root_source, args.contract, "Solar")
    else:
        source = pathlib.Path(args.source).resolve()
        if not source.is_file():
            raise ValueError(f"source file does not exist: {source}")
        standard_input = _standard_input(
            tools["solc"],
            source,
            evm_version=args.evm_version,
            optimize=args.optimize,
            optimizer_runs=args.optimizer_runs,
            via_ir=args.via_ir,
            project_root=args.project_root,
            include_paths=tuple(args.include_path),
            remappings=tuple(args.remapping),
            timeout=args.timeout,
        )
        serialized_input = json.dumps(
            standard_input["input"],
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        )
        solc_artifact = _compile(
            tools["solc"],
            serialized_input,
            standard_input["root_source"],
            args.contract,
            args.timeout,
            "Solc",
        )
        solar_artifact = _compile(
            tools["solar"],
            serialized_input,
            standard_input["root_source"],
            args.contract,
            args.timeout,
            "Solar",
        )
    runtime_only = args.runtime_only or not (
        solc_artifact["creation"] and solar_artifact["creation"]
    )
    for artifact, label in ((solc_artifact, "Solc"), (solar_artifact, "Solar")):
        if runtime_only and artifact["has_immutables"]:
            raise ValueError("contracts with immutables require constructor execution")
        if not runtime_only and artifact["creation_links"]:
            raise ValueError(f"{label} creation code has unresolved libraries")
    if runtime_only and (args.constructor_args is not None or args.constructor_value):
        raise ValueError(
            "--constructor-args and --constructor-value require constructor execution"
        )
    try:
        if not runtime_only and _function_shape(
            solc_artifact["constructor_entry"]
        ) != _function_shape(solar_artifact["constructor_entry"]):
            raise AbiDisagreement("compiler ABI disagreement for the constructor")
        checks, skipped = _plan_checks(solc_artifact, solar_artifact, args)
    except AbiDisagreement as err:
        # Different external interfaces are an observable difference.
        return {
            "schema": SCHEMA,
            "status": "mismatch",
            "differs_in": "abi",
            "reason": str(err),
            "source": str(source),
            "contract": args.contract,
            "signature": args.signature,
        }

    if output_root is None:
        output_root = (
            pathlib.Path(__file__).resolve().parents[2] / "target" / "solsymdiff"
        )
    output_root.mkdir(parents=True, exist_ok=True)
    prefix = args.contract if saved_solc else source.stem
    project = pathlib.Path(tempfile.mkdtemp(prefix=f"{prefix}-", dir=output_root))
    (project / "standard-input.json").write_text(
        serialized_input + "\n",
        encoding="utf-8",
    )

    outcomes = []
    constructor: dict[str, Any] | None = None
    runtimes = (solc_artifact["runtime"], solar_artifact["runtime"])
    if not runtime_only:
        constructor_args = args.constructor_args
        if constructor_args is None:
            constructor_args = (
                "0x" + _zero_arguments(solc_artifact["constructor"]).hex()
            )
        constructor = {"args": constructor_args, "value": str(args.constructor_value)}
        deployed = _deploy(
            _anvil_executable(tools["forge"]),
            solc_artifact["creation"] + constructor_args.removeprefix("0x"),
            solar_artifact["creation"] + constructor_args.removeprefix("0x"),
            value=args.constructor_value,
            evm_version=args.evm_version,
            timeout=args.timeout,
        )
        outcome = _compare_constructors(deployed, constructor)
        outcomes.append({"label": CONSTRUCTOR, **outcome})
        solc_side, solar_side = deployed["solc"], deployed["solar"]
        if solc_side["success"] and solar_side["success"]:
            constructor["address"] = solc_side["address"]
            runtimes = (solc_side["runtime"], solar_side["runtime"])
        else:
            checks = []

    (project / "solc.hex").write_text(runtimes[0].removeprefix("0x") + "\n")
    (project / "solar.hex").write_text(runtimes[1].removeprefix("0x") + "\n")
    (project / "foundry.toml").write_text(_foundry_toml(args))
    selectors = {
        "0x" + selector.removeprefix("0x").lower(): signature
        for signature, selector in solc_artifact["method_identifiers"].items()
    }
    for index, check in enumerate(checks):
        command = _forge_command(tools, check, args)
        outcome = _run_check(command, project, index, args.timeout, selectors)
        outcomes.append({"label": check["label"], **outcome})

    limitations = []
    opcodes = set().union(*(_runtime_opcodes(code) for code in runtimes))
    if opcodes & _CREATE_OPCODES:
        limitations.append(
            "created contracts are compared byte for byte, so a check that "
            "reaches CREATE or CREATE2 is incomplete when the builds create "
            "different code"
        )
    if not runtime_only and any(
        _runtime_opcodes(artifact["creation"]) & _OUTSIDE_EFFECT_OPCODES
        for artifact in (solc_artifact, solar_artifact)
    ):
        limitations.append(
            "the constructor check compares only the deployed contract, so "
            "contracts the constructor creates and accounts it calls or pays "
            "are not compared"
        )
    bounds = {
        "timeout_seconds": args.timeout,
        "max_paths": args.max_paths,
        "max_solver_queries": args.max_solver_queries,
        "max_depth": args.max_depth,
        "max_calldata_bytes": args.max_calldata_bytes,
        "dynamic_lengths": list(args.dynamic_lengths),
        "starting_storage": "arbitrary" if args.arbitrary_storage else "empty",
        "value": "symbolic" if args.value is None else str(args.value),
        "promise_no_reent": args.promise_no_reent,
        "ignore_out_of_gas": args.ignore_out_of_gas,
        "constructor": "runtime_only"
        if runtime_only
        else "default_args"
        if args.constructor_args is None
        else "fixed_args",
    }
    raw = [check for check in checks if "calldata_lengths" in check]
    if raw:
        bounds["raw_calldata_lengths"] = raw[0]["calldata_lengths"]
    result = _summarize(outcomes)
    result.update(
        {
            "schema": SCHEMA,
            "source": str(source),
            "contract": args.contract,
            "signature": args.signature,
            "checked": [outcome["label"] for outcome in outcomes],
            "skipped": skipped,
            "limitations": limitations,
            "settings": standard_input["settings"],
            "standard_input_sha256": standard_input["sha256"],
            "sources": sorted(standard_input["sources"]),
            "bounds": bounds,
            "project": str(project),
        }
    )
    if constructor is not None:
        result["constructor"] = constructor
    if saved_solc and saved_solar:
        result["attempts"] = {
            "solc": str(saved_solc.resolve()),
            "solar": str(saved_solar.resolve()),
        }
    (project / "result.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return result


def _positive_seconds(value: str) -> float:
    seconds = float(value)
    if not math.isfinite(seconds) or seconds <= 0:
        raise argparse.ArgumentTypeError("timeout must be finite and positive")
    return seconds


def _positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("value must be positive")
    return number


def _nonnegative_int(value: str) -> int:
    number = int(value)
    if number < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return number


def _wei(value: str) -> int:
    number = int(value, 0)
    if number < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return number


def _hex_data(value: str) -> str:
    if not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})*", value):
        raise argparse.ArgumentTypeError(
            "data must be 0x-prefixed, even-length hexadecimal"
        )
    return value.lower()


def _dynamic_lengths(value: str) -> tuple[int, ...]:
    parts = value.split(",")
    try:
        lengths = tuple(int(part) for part in parts)
    except ValueError as err:
        raise argparse.ArgumentTypeError(
            "dynamic lengths must be comma-separated integers"
        ) from err
    if (
        not lengths
        or any(
            not part or length < 0 or length > MAX_DYNAMIC_LENGTH
            for part, length in zip(parts, lengths, strict=True)
        )
        or len(set(lengths)) != len(lengths)
    ):
        raise argparse.ArgumentTypeError(
            f"dynamic lengths must be unique integers from 0 through {MAX_DYNAMIC_LENGTH}"
        )
    return lengths


def _resolve_executable(value: str) -> str:
    path = pathlib.Path(value).expanduser()
    if path.parent != pathlib.Path("."):
        if path.is_file():
            return str(path.resolve())
        raise OSError(f"executable was not found: {value}")
    resolved = shutil.which(value)
    if resolved is None:
        raise OSError(f"executable was not found: {value}")
    return str(pathlib.Path(resolved).resolve())


def _anvil_executable(forge: str) -> str:
    """Prefers the anvil installed next to Forge, then the one on PATH."""
    sibling = pathlib.Path(forge).with_name("anvil")
    if sibling.is_file():
        return str(sibling)
    resolved = shutil.which("anvil")
    if resolved is None:
        raise OSError("anvil was not found next to Forge or on PATH")
    return resolved


def _standard_input(
    solc: str,
    source: pathlib.Path,
    *,
    evm_version: str,
    optimize: bool,
    optimizer_runs: int,
    via_ir: bool,
    project_root: pathlib.Path | None,
    include_paths: tuple[pathlib.Path, ...],
    remappings: tuple[str, ...],
    timeout: float,
) -> dict[str, Any]:
    source = source.resolve()
    project_root = (project_root or source.parent).resolve()
    include_paths = tuple(path.resolve() for path in include_paths)
    if not project_root.is_dir():
        raise ValueError(f"project root is not a directory: {project_root}")
    if not source.is_relative_to(project_root):
        raise ValueError(f"source is outside project root: {source}")
    if any(not path.is_dir() for path in include_paths):
        raise ValueError("every include path must be a directory")
    if any(
        not remapping or "=" not in remapping or not all(remapping.split("=", 1))
        for remapping in remappings
    ):
        raise ValueError("remappings must use non-empty prefix=target syntax")

    root_source = source.relative_to(project_root).as_posix()
    discovery_input = {
        "language": "Solidity",
        "sources": {root_source: {"content": source.read_text(encoding="utf-8")}},
        "settings": {
            "evmVersion": evm_version,
            "outputSelection": {"*": {"": ["ast"]}},
            **({"remappings": list(remappings)} if remappings else {}),
        },
    }
    command = [solc, "--base-path", str(project_root)]
    for include_path in include_paths:
        command.extend(["--include-path", str(include_path)])
    command.append("--standard-json")
    with tempfile.TemporaryDirectory(prefix="solar-solsymdiff-imports-") as cwd:
        result = subprocess.run(
            command,
            check=False,
            input=json.dumps(discovery_input),
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
        )
    output = _compiler_json(result, "Solc import discovery")
    discovered = output.get("sources")
    if not isinstance(discovered, dict) or root_source not in discovered:
        raise ValueError("Solc import discovery did not return the root source")
    roots = (project_root, *include_paths)
    sources = {}
    for name in sorted(discovered):
        if name == root_source:
            path = source
        else:
            candidates = tuple((root / name).resolve() for root in roots)
            path = next(
                (candidate for candidate in candidates if candidate.is_file()), None
            )
            if path is None:
                raise ValueError(f"could not snapshot imported source unit: {name}")
        sources[name] = {"content": path.read_text(encoding="utf-8")}

    settings = {
        "optimizer": {"enabled": optimize, "runs": optimizer_runs},
        "viaIR": via_ir,
        "evmVersion": evm_version,
        "metadata": {"bytecodeHash": "none"},
        "outputSelection": {
            "*": {
                "*": [
                    "abi",
                    "evm.bytecode.linkReferences",
                    "evm.bytecode.object",
                    "evm.deployedBytecode.immutableReferences",
                    "evm.deployedBytecode.linkReferences",
                    "evm.deployedBytecode.object",
                    "evm.methodIdentifiers",
                ]
            }
        },
        **({"remappings": list(remappings)} if remappings else {}),
    }
    value = {"language": "Solidity", "sources": sources, "settings": settings}
    serialized = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    return {
        "input": value,
        "root_source": root_source,
        "settings": settings,
        "sources": sources,
        "sha256": hashlib.sha256(serialized.encode()).hexdigest(),
    }


def _compile(
    compiler: str,
    standard_input: str,
    source_name: str,
    contract: str,
    timeout: float,
    label: str,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="solar-solsymdiff-compile-") as cwd:
        result = subprocess.run(
            [compiler, "--standard-json"],
            check=False,
            input=standard_input,
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
        )
    return _artifact(_compiler_json(result, label), source_name, contract, label)


def _artifact(output, source_name, contract, label):
    contracts = output.get("contracts")
    source_contracts = (
        contracts.get(source_name) if isinstance(contracts, dict) else None
    )
    artifact = (
        source_contracts.get(contract) if isinstance(source_contracts, dict) else None
    )
    if not isinstance(artifact, dict):
        raise ValueError(f"{label} did not emit {source_name}:{contract}")

    evm = artifact.get("evm")
    deployed = evm.get("deployedBytecode") if isinstance(evm, dict) else None
    identifiers = evm.get("methodIdentifiers") if isinstance(evm, dict) else None
    abi = artifact.get("abi")
    if (
        not isinstance(deployed, dict)
        or not isinstance(identifiers, dict)
        or not isinstance(abi, list)
    ):
        raise ValueError(f"{label} emitted a malformed contract artifact")
    if deployed.get("linkReferences"):
        raise ValueError("contracts with unresolved libraries are unsupported")

    runtime = deployed.get("object")
    if not isinstance(runtime, str) or not runtime:
        raise ValueError(f"{label} emitted no deployed runtime")
    payload = runtime.removeprefix("0x")
    if len(payload) % 2:
        raise ValueError(f"{label} emitted bytecode with an odd length")
    try:
        bytes.fromhex(payload)
    except ValueError as err:
        raise ValueError(f"{label} emitted non-hex runtime bytecode") from err

    creation = evm.get("bytecode")
    creation_code = creation.get("object") if isinstance(creation, dict) else None
    if creation_code is not None and (
        not isinstance(creation_code, str)
        or not re.fullmatch(r"(?:0x)?(?:[0-9a-fA-F]{2})+", creation_code)
    ):
        raise ValueError(f"{label} emitted invalid creation bytecode")
    constructors = [
        entry
        for entry in abi
        if isinstance(entry, dict) and entry.get("type") == "constructor"
    ]
    constructor = (
        constructors[0]
        if constructors
        else {"inputs": [], "outputs": [], "stateMutability": "nonpayable"}
    )
    constructor = {"outputs": [], **constructor}
    return {
        "abi": abi,
        "method_identifiers": identifiers,
        "runtime": "0x" + payload.lower(),
        "creation": "0x" + creation_code.removeprefix("0x").lower()
        if creation_code
        else None,
        "creation_links": bool(
            isinstance(creation, dict) and creation.get("linkReferences")
        ),
        "has_immutables": bool(deployed.get("immutableReferences")),
        "constructor_entry": constructor,
        "constructor": constructor["inputs"],
    }


def _compiler_json(
    result: subprocess.CompletedProcess[str],
    label: str,
) -> dict[str, Any]:
    try:
        output = json.loads(result.stdout)
    except json.JSONDecodeError as err:
        detail = result.stderr.strip() or str(err)
        raise ValueError(f"{label} did not emit Standard JSON: {detail}") from err
    if not isinstance(output, dict):
        raise ValueError(f"{label} Standard JSON output must be an object")
    errors = output.get("errors", [])
    failures = [
        error.get("formattedMessage") or error.get("message") or str(error)
        for error in errors
        if isinstance(error, dict) and error.get("severity") == "error"
    ]
    if result.returncode != 0 or failures:
        detail = "\n".join(failures) or result.stderr.strip()
        raise ValueError(f"{label} compilation failed: {detail}")
    return output


def _select_function(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    signature: str,
) -> dict[str, Any]:
    solc_entry = _function_entry(solc_artifact["abi"], signature, "Solc")
    try:
        solar_entry = _function_entry(solar_artifact["abi"], signature, "Solar")
    except ValueError as err:
        raise AbiDisagreement(str(err)) from err
    if _function_shape(solc_entry) != _function_shape(solar_entry):
        raise AbiDisagreement(f"compiler ABI disagreement for {signature}")
    solc_selector = _selector(solc_artifact["method_identifiers"], signature, "Solc")
    solar_selector = _selector(
        solar_artifact["method_identifiers"],
        signature,
        "Solar",
    )
    if solc_selector != solar_selector:
        raise AbiDisagreement(
            f"compiler selector disagreement for {signature}: "
            f"Solc={solc_selector}, Solar={solar_selector}"
        )
    if not all(_supported_input(item) for item in solc_entry["inputs"]):
        raise ValueError(f"{signature} has an input Forge cannot generate")
    return {
        "signature": signature,
        "selector": solc_selector,
        "inputs": solc_entry["inputs"],
    }


def _function_entry(
    abi: list[dict[str, Any]],
    signature: str,
    label: str,
) -> dict[str, Any]:
    matches = [
        entry
        for entry in abi
        if isinstance(entry, dict)
        and entry.get("type") == "function"
        and _abi_signature(entry) == signature
    ]
    if len(matches) != 1:
        raise ValueError(f"{label} must contain exactly one {signature} function")
    return matches[0]


def _abi_signature(entry: dict[str, Any]) -> str:
    name = entry.get("name")
    inputs = entry.get("inputs")
    if not isinstance(name, str) or not isinstance(inputs, list):
        raise ValueError("function ABI entry is malformed")
    return f"{name}({','.join(_canonical_type(item) for item in inputs)})"


def _function_shape(entry: dict[str, Any]) -> tuple[Any, ...]:
    inputs = entry.get("inputs")
    outputs = entry.get("outputs")
    if not isinstance(inputs, list) or not isinstance(outputs, list):
        raise ValueError("function ABI entry is malformed")
    return (
        tuple(_canonical_type(item) for item in inputs),
        tuple(_canonical_type(item) for item in outputs),
        entry.get("stateMutability"),
    )


def _selector(identifiers: dict[str, Any], signature: str, label: str) -> str:
    value = identifiers.get(signature)
    if not isinstance(value, str):
        raise ValueError(f"{label} emitted no selector for {signature}")
    payload = value.removeprefix("0x")
    if len(payload) != 8:
        raise ValueError(f"{label} emitted an invalid selector for {signature}")
    try:
        bytes.fromhex(payload)
    except ValueError as err:
        raise ValueError(f"{label} emitted a non-hex selector for {signature}") from err
    return "0x" + payload.lower()


def _supported_input(item: dict[str, Any]) -> bool:
    abi_type = item.get("type")
    if not isinstance(abi_type, str):
        return False
    if abi_type.startswith("tuple"):
        components = item.get("components")
        base_type = "uint256" + abi_type[len("tuple") :]
        return (
            isinstance(components, list)
            and bool(components)
            and _supported_elementary(base_type)
            and all(
                isinstance(component, dict) and _supported_input(component)
                for component in components
            )
        )
    return _supported_elementary(abi_type)


def _supported_elementary(abi_type: str) -> bool:
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
    if abi_type.startswith(("uint", "int")):
        width = abi_type[4:] if abi_type.startswith("uint") else abi_type[3:]
        return not width or (width.isdigit() and int(width) in range(8, 257, 8))
    if abi_type.startswith("bytes"):
        width = abi_type[5:]
        return width.isdigit() and 1 <= int(width) <= 32
    return False


def _function_signatures(abi: list[dict[str, Any]]) -> set[str]:
    return {
        _abi_signature(entry)
        for entry in abi
        if isinstance(entry, dict) and entry.get("type") == "function"
    }


def _abi_head(item: dict[str, Any]) -> tuple[int, bool]:
    """Returns the ABI head size of one parameter and whether it is dynamic."""
    abi_type = item["type"]
    if abi_type.endswith("]"):
        start = abi_type.rfind("[")
        length = abi_type[start + 1 : -1]
        size, dynamic = _abi_head({**item, "type": abi_type[:start]})
        if not length or dynamic:
            return 32, True
        return size * int(length), False
    if abi_type == "tuple":
        sizes = [_abi_head(component) for component in item["components"]]
        if any(dynamic for _, dynamic in sizes):
            return 32, True
        return sum(size for size, _ in sizes), False
    return 32, abi_type in {"bytes", "string"}


def _runtime_opcodes(runtime: str) -> set[int]:
    """Returns the opcodes of runtime or creation code, skipping PUSH data and trailing metadata."""
    code = bytes.fromhex(runtime.removeprefix("0x"))
    if len(code) >= 2:
        metadata = int.from_bytes(code[-2:]) + 2
        if metadata < len(code) and 0xA1 <= code[-metadata] <= 0xA6:
            code = code[:-metadata]
    opcodes = set()
    pc = 0
    while pc < len(code):
        opcode = code[pc]
        opcodes.add(opcode)
        pc += 1 + (opcode - 0x5F if 0x60 <= opcode <= 0x7F else 0)
    return opcodes


def _plan_checks(
    solc_artifact: dict[str, Any],
    solar_artifact: dict[str, Any],
    args: argparse.Namespace,
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Returns the equivalence checks to run and the functions left out.

    Raises `AbiDisagreement` when the two builds expose different functions.
    """
    if not args.all_functions:
        if not args.signature:
            raise ValueError("--signature is required without --all-functions")
        _select_function(solc_artifact, solar_artifact, args.signature)
        return [{"label": args.signature, "sig": args.signature}], []
    solc_signatures = _function_signatures(solc_artifact["abi"])
    solar_signatures = _function_signatures(solar_artifact["abi"])
    if solc_signatures != solar_signatures:
        raise AbiDisagreement(
            "compiler ABI disagreement: only Solc has "
            f"{sorted(solc_signatures - solar_signatures)}, only Solar has "
            f"{sorted(solar_signatures - solc_signatures)}"
        )
    checks = []
    skipped = []
    for signature in sorted(solc_signatures):
        try:
            _select_function(solc_artifact, solar_artifact, signature)
        except AbiDisagreement:
            raise
        except ValueError as err:
            skipped.append({"signature": signature, "reason": str(err)})
            continue
        checks.append({"label": signature, "sig": signature})
    # Forge cannot fix the selector of raw calldata, so one check covers both
    # calldata that matches no selector, which reaches `fallback` and `receive`,
    # and the short and dirty arguments of every function.
    checks.append(
        {
            "label": OTHER_CALLDATA,
            "calldata_lengths": _raw_lengths(
                solc_artifact["abi"], args.max_calldata_bytes
            ),
        }
    )
    return checks, skipped


def _raw_lengths(abi: list[dict[str, Any]], limit: int) -> list[int]:
    """Returns raw calldata lengths: short ones, and around each argument head."""
    lengths = set(RAW_CALLDATA_LENGTHS)
    for entry in abi:
        if isinstance(entry, dict) and entry.get("type") == "function":
            head = sum(_abi_head(item)[0] for item in entry["inputs"])
            lengths.update(
                4 + size for size in (head - 1, head, head + 32) if size >= 0
            )
    return sorted(length for length in lengths if length <= limit)


def _zero_arguments(items: list[dict[str, Any]]) -> bytes:
    """ABI-encodes the zero value of each parameter.

    Static values are zero words; arrays, `bytes`, and `string` are empty.
    """
    head_size = sum(_abi_head(item)[0] for item in items)
    heads = []
    tails = []
    for item in items:
        size, dynamic = _abi_head(item)
        if not dynamic:
            heads.append(bytes(size))
            continue
        heads.append((head_size + sum(len(tail) for tail in tails)).to_bytes(32))
        tails.append(_zero_tail(item))
    return b"".join(heads + tails)


def _zero_tail(item: dict[str, Any]) -> bytes:
    """ABI-encodes the zero value of one dynamic parameter, without its offset."""
    abi_type = item["type"]
    if abi_type.endswith("]"):
        start = abi_type.rfind("[")
        length = abi_type[start + 1 : -1]
        if not length:
            return bytes(32)
        element = {**item, "type": abi_type[:start]}
        return _zero_arguments([element] * int(length))
    if abi_type == "tuple":
        return _zero_arguments(item["components"])
    return bytes(32)


@contextlib.contextmanager
def _anvil(anvil: str, evm_version: str, timeout: float) -> Iterator[str]:
    """Runs a private anvil on a free local port and yields its URL.

    Anvil picks the port itself and prints it, so the URL is always the
    child's own node.
    """
    process = subprocess.Popen(
        [
            anvil,
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--hardfork",
            evm_version.lower(),
            "--disable-code-size-limit",
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    stdout = process.stdout
    assert stdout is not None
    try:
        url = _anvil_url(process, stdout, timeout)
        # Keep reading anvil's output so a full pipe never blocks it.
        threading.Thread(target=lambda: [None for _ in stdout], daemon=True).start()
        yield url
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def _anvil_url(process: subprocess.Popen[str], stdout: IO[str], timeout: float) -> str:
    """Reads the address anvil prints once it listens."""
    lines: queue.Queue[str | None] = queue.Queue()

    def read() -> None:
        for line in stdout:
            lines.put(line)
            if "Listening on" in line:
                return
        lines.put(None)

    threading.Thread(target=read, daemon=True).start()
    output = []
    deadline = time.monotonic() + timeout
    while True:
        try:
            line = lines.get(timeout=max(deadline - time.monotonic(), 0))
        except queue.Empty:
            raise RuntimeError("anvil did not start") from None
        if line is None:
            detail = "".join(output[-20:]).strip()
            raise RuntimeError(f"anvil exited before it started: {detail}")
        output.append(line)
        match = re.search(r"Listening on (\S+:\d+)", line)
        if match:
            return f"http://{match.group(1)}"


def _deploy(
    anvil: str,
    solc_initcode: str,
    solar_initcode: str,
    *,
    value: int,
    evm_version: str,
    timeout: float,
) -> dict[str, dict[str, Any]]:
    """Deploys both builds from the same account and nonce, so at one address.

    That address is the one `forge equivalence` runs the runtimes at.
    """
    with _anvil(anvil, evm_version, timeout) as url:
        _rpc(url, "anvil_impersonateAccount", [DEPLOYER], timeout)
        _rpc(url, "anvil_setNonce", [DEPLOYER, hex(DEPLOYER_NONCE)], timeout)
        # Covers any constructor value.
        _rpc(url, "anvil_setBalance", [DEPLOYER, hex(value + 10**24)], timeout)
        # One explicit gas price for the transactions of both builds, so
        # GASPRICE reads the same everywhere.
        gas_price = int(_rpc(url, "eth_gasPrice", [], timeout), 16)
        return {
            "solc": _create(url, solc_initcode, value, gas_price, timeout),
            "solar": _create(url, solar_initcode, value, gas_price, timeout),
        }


def _create(
    url: str, initcode: str, value: int, gas_price: int, timeout: float
) -> dict[str, Any]:
    """Runs one deployment and reports its outcome, then undoes it."""
    transaction = {
        "from": DEPLOYER,
        "data": initcode,
        "value": hex(value),
        "gas": evm.TX_GAS,
        "gasPrice": hex(gas_price),
    }
    snapshot = _rpc(url, "evm_snapshot", [], timeout)
    try:
        # Both builds deploy in a block mined on the same parent. Pin its
        # timestamp, which anvil otherwise takes from the wall clock.
        latest = _rpc(url, "eth_getBlockByNumber", ["latest", False], timeout)
        _rpc(
            url,
            "evm_setNextBlockTimestamp",
            [int(latest["timestamp"], 16) + 1],
            timeout,
        )
        # The mined transaction decides the outcome, so no simulation can see
        # a different block or transaction environment.
        # Pin the nonce and never resend: a retry after a lost response would
        # deploy at the next nonce, so at another address.
        transaction["nonce"] = _rpc(
            url, "eth_getTransactionCount", [DEPLOYER, "latest"], timeout
        )
        sent = evm.rpc(url, "eth_sendTransaction", [transaction], timeout, retries=0)
        if "result" not in sent:
            raise RuntimeError(f"anvil eth_sendTransaction failed: {sent.get('error')}")
        tx_hash = sent["result"]
        receipt = evm.wait_for_receipt(url, tx_hash, timeout)
        if not receipt:
            raise RuntimeError("the deployment transaction was not mined")
        if receipt.get("status") != "0x1":
            # Only the revert data is the contract's; the error is anvil's.
            trace = _rpc(
                url,
                "debug_traceTransaction",
                [tx_hash, {"tracer": "callTracer"}],
                timeout,
            )
            return {
                "success": False,
                "data": evm.normalize_hex(trace.get("output")),
                "error": str(trace.get("error", "")),
            }
        address = receipt["contractAddress"]
        balance = _rpc(url, "eth_getBalance", [address, "latest"], timeout)
        nonce = _rpc(url, "eth_getTransactionCount", [address, "latest"], timeout)
        return {
            "success": True,
            "address": address,
            "runtime": evm.normalize_hex(
                _rpc(url, "eth_getCode", [address, "latest"], timeout)
            ),
            "logs": [_constructor_log(log) for log in receipt.get("logs", [])],
            "storage_root": evm.storage_root(url, address, timeout),
            "balance": str(int(balance, 16)),
            "nonce": int(nonce, 16),
        }
    finally:
        _rpc(url, "evm_revert", [snapshot], timeout)


def _constructor_log(log: dict[str, Any]) -> dict[str, Any]:
    """Normalizes a constructor log, keeping its emitter.

    Both builds deploy at the same address, so the emitting contract is
    comparable and is part of the log.
    """
    return {"address": evm.normalize_hex(log.get("address")), **evm.normalize_log(log)}


def _rpc(url: str, method: str, params: list[Any], timeout: float) -> Any:
    response = evm.rpc(url, method, params, timeout)
    if "result" not in response:
        raise RuntimeError(f"anvil {method} failed: {response.get('error')}")
    return response["result"]


def _compare_constructors(
    deployed: dict[str, dict[str, Any]], constructor: dict[str, Any]
) -> dict[str, Any]:
    """Compares the two deployments' success, revert data, logs, storage, balance, and nonce."""
    solc, solar = deployed["solc"], deployed["solar"]
    if solc["success"] != solar["success"]:
        comparison = "success"
    elif not solc["success"]:
        if solc["data"] == solar["data"]:
            return {
                "status": "incomplete",
                "reason": (
                    "both constructors revert with the same data, so no runtime "
                    "was compared; pass --constructor-args that deploy"
                ),
            }
        comparison = "revert data"
    else:
        comparison = next(
            (
                name
                for key, name in (
                    ("logs", "logs"),
                    ("storage_root", "storage"),
                    ("balance", "balance"),
                    ("nonce", "nonce"),
                )
                if solc[key] != solar[key]
            ),
            None,
        )
        if comparison is None:
            return {"status": "bounded_agreement"}
    sides = {
        label: {key: value for key, value in side.items() if key != "runtime"}
        for label, side in deployed.items()
    }
    return {
        "status": "mismatch",
        "differs_in": "constructor",
        "comparison": comparison,
        "counterexample": {
            "constructor_args": constructor["args"],
            "constructor_value": constructor["value"],
            **sides,
        },
    }


def _foundry_toml(args: argparse.Namespace) -> str:
    """Returns the Forge configuration that holds the symbolic bounds."""
    lengths = ", ".join(str(length) for length in args.dynamic_lengths)
    return (
        "[profile.default]\n"
        f'evm_version = "{args.evm_version}"\n'
        "\n"
        "[profile.default.symbolic]\n"
        f"max_paths = {args.max_paths}\n"
        f"max_depth = {args.max_depth}\n"
        f"max_solver_queries = {args.max_solver_queries}\n"
        f"max_dynamic_length = {MAX_DYNAMIC_LENGTH}\n"
        f"default_array_lengths = [{lengths}]\n"
        f"default_bytes_lengths = [{lengths}]\n"
    )


def _forge_command(
    tools: dict[str, str], check: dict[str, Any], args: argparse.Namespace
) -> list[str]:
    command = [
        tools["forge"],
        "equivalence",
        "--code-a-file",
        "solc.hex",
        "--code-b-file",
        "solar.hex",
    ]
    if "sig" in check:
        command.extend(["--sig", check["sig"]])
    else:
        lengths = ",".join(str(length) for length in check["calldata_lengths"])
        command.extend(["--calldata-lengths", lengths])
    command.extend(
        [
            "--max-calldata",
            str(args.max_calldata_bytes),
            "--solver",
            tools["solver"],
            "--timeout",
            str(math.ceil(args.timeout)),
        ]
    )
    if not args.arbitrary_storage:
        command.append("--empty-storage")
    if args.value is not None:
        command.extend(["--value", str(args.value)])
    if args.promise_no_reent:
        command.append("--promise-no-reent")
    if args.ignore_out_of_gas:
        command.append("--ignore-out-of-gas")
    command.append("--json")
    return command


def _run_check(
    command: list[str],
    project: pathlib.Path,
    index: int,
    timeout: float,
    selectors: dict[str, str],
) -> dict[str, Any]:
    """Runs one equivalence check in the project and classifies its result."""
    home = project / ".home"
    home.mkdir(exist_ok=True)
    # Only the project's foundry.toml configures Forge.
    env = {
        name: value
        for name, value in os.environ.items()
        if not name.startswith(("FOUNDRY_", "DAPP_"))
    }
    env.update({"HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config")})
    limit = timeout + FORGE_GRACE_SECONDS
    log: dict[str, Any] = {"command": command}
    try:
        process = subprocess.run(
            command,
            check=False,
            cwd=project,
            env=env,
            text=True,
            capture_output=True,
            timeout=limit,
        )
    except subprocess.TimeoutExpired:
        outcome = {
            "status": "incomplete",
            "reason": f"forge equivalence did not finish within {limit:g} seconds",
        }
    else:
        log.update(
            returncode=process.returncode,
            stdout=process.stdout,
            stderr=process.stderr,
        )
        outcome = _classify(
            process.returncode, process.stdout, process.stderr, selectors
        )
    (project / f"check-{index}.json").write_text(
        json.dumps(log, indent=2) + "\n", encoding="utf-8"
    )
    return outcome


def _classify(
    returncode: int, stdout: str, stderr: str, selectors: dict[str, str]
) -> dict[str, Any]:
    """Classifies one `forge equivalence --json` run.

    Forge prints a JSON result and exits with 0 when no explored input tells the
    codes apart, and prints a JSON counterexample and exits with 1 only after
    replaying it concretely. Every other run, such as an incomplete check that
    Forge reports as an error, is incomplete.
    """
    report = _last_json(stdout)
    status = report.get("status") if report else None
    if report and status == "equivalent" and returncode == 0:
        return {"status": "bounded_agreement", "stats": report.get("stats")}
    if report and status == "not_equivalent" and returncode != 0:
        return _counterexample(report, selectors)
    reason = _forge_error(stderr) or (
        f"forge equivalence exited with status {returncode} without a result"
    )
    return {"status": "incomplete", "reason": reason}


def _last_json(stdout: str) -> dict[str, Any] | None:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    return None


def _forge_error(stderr: str) -> str:
    """Returns Forge's error message, such as why a check is incomplete."""
    lines = re.sub(r"\x1b\[[0-9;]*m", "", stderr).strip().splitlines()
    for index, line in enumerate(lines):
        if line.startswith("Error:"):
            parts = [line.removeprefix("Error:"), *lines[index + 1 :]]
            return " ".join(part.strip() for part in parts if part.strip())[:2000]
    return "\n".join(lines)[-2000:]


def _counterexample(
    report: dict[str, Any], selectors: dict[str, str]
) -> dict[str, Any]:
    """Turns a replayed Forge counterexample into a mismatch; Solc's build is code A."""
    solc = report.get("code_a") or {}
    solar = report.get("code_b") or {}
    storage = [_sides(item) for item in report.get("storage") or []]
    balances = [_sides(item) for item in report.get("balances") or []]
    accounts = [_sides(item) for item in report.get("accounts") or []]
    if report.get("out_of_gas"):
        differs_in = "out of gas"
    elif solc.get("success") != solar.get("success"):
        differs_in = "call success"
    elif solc.get("output") != solar.get("output"):
        differs_in = "returndata"
    elif solc.get("logs") != solar.get("logs"):
        differs_in = "logs"
    elif storage:
        differs_in = "storage"
    elif balances:
        differs_in = "balance"
    elif any(
        (item["solc"] or {}).get("nonce") != (item["solar"] or {}).get("nonce")
        for item in accounts
    ):
        differs_in = "nonce"
    else:
        differs_in = "code"
    env = report.get("env") or {}
    calldata = report.get("calldata") or "0x"
    counterexample: dict[str, Any] = {
        "calldata": calldata,
        "caller": env.get("caller"),
        "value": _decimal(env.get("value")),
        "env": env,
        "solc": solc,
        "solar": solar,
    }
    if function := selectors.get(calldata[:10].lower()):
        counterexample["function"] = function
    if report.get("args"):
        counterexample["args"] = report["args"]
    for key, items in (
        ("storage", storage),
        ("balances", balances),
        ("accounts", accounts),
    ):
        if items:
            counterexample[key] = items
    if differs_in == "code":
        # Created code legitimately differs between compilers.
        return {
            "status": "incomplete",
            "reason": (
                "the builds create contracts with different code, which Forge "
                "counts as a difference; compare the created contract's source "
                "on its own"
            ),
            "counterexample": counterexample,
            "stats": report.get("stats"),
        }
    return {
        "status": "mismatch",
        "differs_in": differs_in,
        "counterexample": counterexample,
        "stats": report.get("stats"),
    }


def _sides(item: dict[str, Any]) -> dict[str, Any]:
    """Renames Forge's code A and code B to the builds they hold."""
    renamed = {
        key: value for key, value in item.items() if key not in {"code_a", "code_b"}
    }
    renamed.update(solc=item.get("code_a"), solar=item.get("code_b"))
    return renamed


def _decimal(value: Any) -> Any:
    if isinstance(value, str) and value.startswith("0x"):
        return str(int(value, 16))
    if isinstance(value, int):
        return str(value)
    return value


def _summarize(outcomes: list[dict[str, Any]]) -> dict[str, Any]:
    """Combines the checks; one check that is incomplete hides no other."""
    statuses = {outcome["status"] for outcome in outcomes}
    if "mismatch" in statuses:
        status = "mismatch"
    elif "incomplete" in statuses or not outcomes:
        status = "incomplete"
    else:
        status = "bounded_agreement"
    summary: dict[str, Any] = {"status": status, "checks": outcomes}
    if status == "mismatch":
        first = next(item for item in outcomes if item["status"] == "mismatch")
        summary["differs_in"] = first["differs_in"]
        summary["counterexample"] = first["counterexample"]
    elif status == "incomplete":
        first = next(
            (item for item in outcomes if item["status"] == "incomplete"), None
        )
        summary["reason"] = (
            f"{first['label']}: {first['reason']}" if first else "nothing was compared"
        )
    return summary


if __name__ == "__main__":
    raise SystemExit(main())
