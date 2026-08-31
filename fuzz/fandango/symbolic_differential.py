#!/usr/bin/env python3
"""Run a focused Solc-vs-Solar symbolic differential."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
from typing import Any

SCHEMA = "solar:solsymdiff@v1"
TEST_NAME = "checkSymbolicDifferential"
DEFAULT_DYNAMIC_LENGTHS = (0, 1, 2, 3)
MAX_DYNAMIC_LENGTH = 256
MAX_RETURNDATA_BYTES = 256
SYMBOLIC_QUERY_TIMEOUT = 30
SYMBOLIC_MAX_SOLVER_QUERIES = 10_000
SYMBOLIC_MAX_CALLDATA_BYTES = 4096
CALL_GAS = 10_000_000


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="solsymdiff",
        description="Symbolically compare one function compiled by Solc and Solar.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--source", type=pathlib.Path, required=True)
    parser.add_argument("--contract", required=True)
    parser.add_argument("--signature", required=True)
    parser.add_argument(
        "--include-view",
        action="store_true",
        help="allow a view target under the clean zero-storage model",
    )
    parser.add_argument(
        "--include-stateful",
        action="store_true",
        help=(
            "allow a nonpayable target under the clean zero-storage, single-call model"
        ),
    )
    parser.add_argument(
        "--prefix-calldata",
        type=_prefix_calldata,
        action="append",
        default=[],
        metavar="HEX",
        help="prepare fixed zero-value target calls before symbolic comparison",
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
    parser.add_argument("--timeout", type=_positive_seconds, default=60.0)
    parser.add_argument(
        "--symbolic-timeout",
        type=_positive_int,
        default=SYMBOLIC_QUERY_TIMEOUT,
    )
    parser.add_argument("--max-paths", type=_positive_int, default=1024)
    parser.add_argument(
        "--max-solver-queries",
        type=_positive_int,
        default=SYMBOLIC_MAX_SOLVER_QUERIES,
    )
    parser.add_argument(
        "--max-calldata-bytes",
        type=_positive_int,
        default=SYMBOLIC_MAX_CALLDATA_BYTES,
    )
    parser.add_argument("--max-depth", type=_positive_int)
    parser.add_argument(
        "--exploration-order",
        choices=("bfs", "dfs"),
        default="bfs",
    )
    parser.add_argument(
        "--dynamic-lengths",
        type=_dynamic_lengths,
        default=DEFAULT_DYNAMIC_LENGTHS,
    )
    parser.add_argument(
        "--input-length",
        type=_input_lengths,
        action="append",
        default=[],
        metavar="INDEX=LENGTHS",
    )
    parser.add_argument(
        "--max-returndata-bytes",
        type=_positive_int,
        default=MAX_RETURNDATA_BYTES,
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
        result = run(args)
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
    source = args.source.resolve()
    if not source.is_file():
        raise ValueError(f"source file does not exist: {source}")
    prefix_calldata = tuple(args.prefix_calldata)

    tools = {
        name: _resolve_executable(getattr(args, name))
        for name in ("solc", "solar", "forge", "solver")
    }
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
    function = _select_function(
        solc_artifact,
        solar_artifact,
        args.signature,
        include_view=args.include_view,
        include_stateful=args.include_stateful,
    )
    if prefix_calldata and function["mutability"] == "pure":
        raise ValueError("prefix calls require a view or nonpayable target")
    input_lengths = _normalize_input_lengths(args.input_length, function["inputs"])
    solc_runtime = solc_artifact["runtime"]
    solar_runtime = solar_artifact["runtime"]
    max_dynamic_length = max(
        MAX_DYNAMIC_LENGTH,
        (len(solc_runtime) - 2) // 2,
        (len(solar_runtime) - 2) // 2,
    )
    bounds = {
        "command_timeout_seconds": args.timeout,
        "solver_timeout_seconds": args.symbolic_timeout,
        "max_paths": args.max_paths,
        "max_solver_queries": args.max_solver_queries,
        "max_calldata_bytes": args.max_calldata_bytes,
        "max_dynamic_length": max_dynamic_length,
        "max_depth": args.max_depth,
        "exploration_order": args.exploration_order,
        "storage_layout": (
            "zero_init" if function["mutability"] != "pure" else "solidity"
        ),
        "call_gas": CALL_GAS,
        "dynamic_lengths": list(args.dynamic_lengths),
        "input_lengths": {
            name: list(lengths) for name, lengths in input_lengths.items()
        },
        "max_returndata_bytes": args.max_returndata_bytes,
    }

    if output_root is None:
        output_root = (
            pathlib.Path(__file__).resolve().parents[2] / "target" / "solsymdiff"
        )
    output_root.mkdir(parents=True, exist_ok=True)
    project = pathlib.Path(tempfile.mkdtemp(prefix=f"{source.stem}-", dir=output_root))
    (project / "standard-input.json").write_text(
        serialized_input + "\n",
        encoding="utf-8",
    )
    _write_project(
        project,
        solc_runtime,
        solar_runtime,
        function,
        args.evm_version,
        dynamic_lengths=args.dynamic_lengths,
        input_lengths=input_lengths,
        prefix_calldata=prefix_calldata,
        exploration_order=args.exploration_order,
        max_dynamic_length=max_dynamic_length,
        max_returndata_bytes=args.max_returndata_bytes,
    )
    report = _run_forge(
        tools["forge"],
        tools["solc"],
        tools["solver"],
        project,
        args,
        max_dynamic_length,
    )
    result = _classify(
        report,
        function["selector"],
        bounds,
    )
    result.update(
        {
            "schema": SCHEMA,
            "source": str(source),
            "contract": args.contract,
            "signature": function["signature"],
            "mutability": function["mutability"],
            "settings": standard_input["settings"],
            "standard_input_sha256": standard_input["sha256"],
            "sources": sorted(standard_input["sources"]),
            "bounds": bounds,
            "project": str(project),
        }
    )
    if prefix_calldata:
        result["prefix"] = {"calldata": list(prefix_calldata), "value": 0}
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


def _prefix_calldata(value: str) -> str:
    if not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})*", value):
        raise argparse.ArgumentTypeError(
            "prefix calldata must be 0x-prefixed, even-length hexadecimal"
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


def _input_lengths(value: str) -> tuple[int, tuple[int, ...]]:
    index, separator, lengths = value.partition("=")
    if not separator or not index.isdigit():
        raise argparse.ArgumentTypeError("input lengths must use INDEX=LENGTHS")
    return int(index), _dynamic_lengths(lengths)


def _normalize_input_lengths(
    overrides: list[tuple[int, tuple[int, ...]]],
    inputs: list[dict[str, Any]],
) -> dict[str, tuple[int, ...]]:
    normalized = {}
    for index, lengths in overrides:
        if index >= len(inputs):
            raise ValueError(f"input length override index {index} is out of range")
        abi_type = inputs[index].get("type")
        if not isinstance(abi_type, str) or not (
            abi_type in {"bytes", "string"} or abi_type.endswith("[]")
        ):
            raise ValueError(f"input {index} is not a top-level dynamic input")
        name = f"arg{index}"
        if name in normalized:
            raise ValueError(f"input length override repeats index {index}")
        normalized[name] = lengths
    return normalized


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
    output = _compiler_json(result, label)

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
    if deployed.get("immutableReferences"):
        raise ValueError("contracts with immutables require constructor execution")
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

    return {
        "abi": abi,
        "method_identifiers": identifiers,
        "runtime": "0x" + payload.lower(),
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
    *,
    include_view: bool = False,
    include_stateful: bool = False,
) -> dict[str, Any]:
    solc_entry = _function_entry(solc_artifact["abi"], signature, "Solc")
    solar_entry = _function_entry(solar_artifact["abi"], signature, "Solar")
    if _function_shape(solc_entry) != _function_shape(solar_entry):
        raise ValueError(f"compiler ABI disagreement for {signature}")
    mutability = solc_entry.get("stateMutability")
    allowed = {"pure"}
    if include_view:
        allowed.add("view")
    if include_stateful:
        allowed.add("nonpayable")
    if mutability not in allowed:
        choices = ", ".join(sorted(allowed))
        raise ValueError(
            f"{signature} has mutability {mutability!r}; allowed: {choices}"
        )
    if not all(_supported_input(item) for item in solc_entry["inputs"]):
        raise ValueError(f"{signature} uses an unsupported symbolic input")

    solc_selector = _selector(solc_artifact["method_identifiers"], signature, "Solc")
    solar_selector = _selector(
        solar_artifact["method_identifiers"],
        signature,
        "Solar",
    )
    if solc_selector != solar_selector:
        raise ValueError(
            f"compiler selector disagreement for {signature}: "
            f"Solc={solc_selector}, Solar={solar_selector}"
        )
    return {
        "signature": signature,
        "selector": solc_selector,
        "inputs": solc_entry["inputs"],
        "mutability": mutability,
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


def _canonical_type(item: dict[str, Any]) -> str:
    abi_type = item.get("type")
    if not isinstance(abi_type, str):
        raise ValueError("ABI value has no type")
    if not abi_type.startswith("tuple"):
        return abi_type
    components = item.get("components")
    if not isinstance(components, list):
        raise ValueError("tuple ABI value has no components")
    suffix = abi_type[len("tuple") :]
    return (
        f"({','.join(_canonical_type(component) for component in components)}){suffix}"
    )


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


def _solidity_parameters(
    inputs: list[dict[str, Any]],
) -> tuple[str, list[str]]:
    definitions = []
    declarations = []
    for index, item in enumerate(inputs):
        type_name = _solidity_type(
            item,
            f"SymbolicInput{index}",
            definitions,
        )
        abi_type = item["type"]
        location = (
            " calldata"
            if abi_type.startswith("tuple")
            or abi_type in {"bytes", "string"}
            or "[" in abi_type
            else ""
        )
        declarations.append(f"{type_name}{location} arg{index}")
    return "\n\n".join(definitions), declarations


def _solidity_type(
    item: dict[str, Any],
    name: str,
    definitions: list[str],
) -> str:
    abi_type = item["type"]
    if not abi_type.startswith("tuple"):
        return abi_type

    fields = []
    for index, component in enumerate(item["components"]):
        field_type = _solidity_type(
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


def _write_project(
    project: pathlib.Path,
    solc_runtime: str,
    solar_runtime: str,
    function: dict[str, Any],
    evm_version: str,
    *,
    max_dynamic_length: int,
    dynamic_lengths: tuple[int, ...] = DEFAULT_DYNAMIC_LENGTHS,
    input_lengths: dict[str, tuple[int, ...]] | None = None,
    prefix_calldata: tuple[str, ...] = (),
    exploration_order: str = "bfs",
    max_returndata_bytes: int = MAX_RETURNDATA_BYTES,
) -> None:
    (project / "src").mkdir()
    (project / "test").mkdir()

    definitions, declarations = _solidity_parameters(function["inputs"])
    arguments = ", ".join(f"arg{index}" for index in range(len(function["inputs"])))
    encode_arguments = f", {arguments}" if arguments else ""
    word_checks = "\n".join(
        (
            f"        if (retA.length > {offset}) "
            f"assert(_word(retA, {offset}) == _word(retB, {offset}));"
        )
        for offset in range(0, max_returndata_bytes, 32)
    )
    prefix_setup = ""
    if prefix_calldata:
        prefix_calls_a = []
        prefix_calls_b = []
        for index, calldata in enumerate(prefix_calldata):
            payload = calldata.removeprefix("0x")
            prefix_calls_a.extend(
                (
                    "        {",
                    "            (bool ok, bytes memory ret) =",
                    f'                solcTarget.call{{gas: CALL_GAS}}(hex"{payload}");',
                    f"            prefixResults[{index}] =",
                    "                keccak256(abi.encode(ok, ret));",
                    "        }",
                )
            )
            prefix_calls_b.extend(
                (
                    "        {",
                    "            (bool ok, bytes memory ret) =",
                    f'                solarTarget.call{{gas: CALL_GAS}}(hex"{payload}");',
                    f"            assert(prefixResults[{index}] ==",
                    "                keccak256(abi.encode(ok, ret)));",
                    "        }",
                )
            )
        prefix_setup = _PREFIX_SETUP_TEMPLATE.format(
            prefix_count=len(prefix_calldata),
            prefix_calls_a="\n".join(prefix_calls_a),
            prefix_calls_b="\n".join(prefix_calls_b),
        )
    if function.get("mutability") == "nonpayable" or prefix_calldata:
        template = _STATEFUL_TEST_TEMPLATE
        suffix_comparison = (
            _VIEW_SUFFIX_TEMPLATE
            if function.get("mutability") == "view"
            else _STATEFUL_SUFFIX_TEMPLATE
        ).format(
            max_returndata=max_returndata_bytes,
            word_checks=word_checks,
        ).rstrip()
    else:
        template = _STATELESS_TEST_TEMPLATE
        suffix_comparison = ""
    test_source = template.format(
        definitions=definitions,
        declarations=", ".join(declarations),
        encode_arguments=encode_arguments,
        selector=function["selector"],
        solc_runtime=solc_runtime.removeprefix("0x"),
        solar_runtime=solar_runtime.removeprefix("0x"),
        call_gas=CALL_GAS,
        max_returndata=max_returndata_bytes,
        prefix_setup=prefix_setup,
        suffix_comparison=suffix_comparison,
        word_checks=word_checks,
    )
    (project / "test" / "SymbolicDifferential.t.sol").write_text(
        test_source,
        encoding="utf-8",
    )
    lengths = ", ".join(str(length) for length in dynamic_lengths)
    named_lengths = ", ".join(
        f"{name} = [{', '.join(str(length) for length in values)}]"
        for name, values in sorted((input_lengths or {}).items())
    )
    (project / "foundry.toml").write_text(
        _FOUNDRY_TOML.format(
            evm_version=evm_version,
            dynamic_lengths=lengths,
            input_lengths=named_lengths,
            exploration_order=exploration_order,
            max_dynamic_length=max_dynamic_length,
            storage_layout=(
                "zero_init"
                if function.get("mutability", "pure") != "pure"
                else "solidity"
            ),
        ),
        encoding="utf-8",
    )


def _run_forge(
    forge: str,
    solc: str,
    solver: str,
    project: pathlib.Path,
    args: argparse.Namespace,
    max_dynamic_length: int,
) -> dict[str, Any]:
    help_output = subprocess.run(
        [forge, "test", "--help"],
        check=False,
        text=True,
        capture_output=True,
        timeout=args.timeout,
    ).stdout
    test_pattern = f"^{TEST_NAME}"
    command = [
        forge,
        "test",
        "--root",
        str(project),
        "--use",
        solc,
        "--evm-version",
        args.evm_version,
        "--symbolic",
        "--match-test",
        test_pattern,
        "--symbolic-solver",
        solver,
        "--symbolic-timeout",
        str(args.symbolic_timeout),
        "--symbolic-max-paths",
        str(args.max_paths),
        "--symbolic-max-solver-queries",
        str(args.max_solver_queries),
        "--symbolic-max-calldata-bytes",
        str(args.max_calldata_bytes),
        "--symbolic-max-dynamic-length",
        str(max_dynamic_length),
        "--json",
    ]
    if "--allow-local-compiler" in help_output:
        command.insert(command.index("--evm-version"), "--allow-local-compiler")
    if args.max_depth is not None:
        command.extend(["--symbolic-max-depth", str(args.max_depth)])
    home = project / ".home"
    home.mkdir()
    env = os.environ.copy()
    svm_home = _host_svm_home(env)
    for name in list(env):
        if name.startswith(("FOUNDRY_", "DAPP_")) or name == "SVM_HOME":
            del env[name]
    env.update(
        {
            "HOME": str(home),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_DATA_HOME": str(home / ".local" / "share"),
        }
    )
    if svm_home.is_dir():
        isolated_svm_home = home / ".local" / "share" / "svm"
        isolated_svm_home.parent.mkdir(parents=True, exist_ok=True)
        isolated_svm_home.symlink_to(svm_home, target_is_directory=True)
    result = subprocess.run(
        command,
        check=False,
        cwd=project,
        env=env,
        text=True,
        capture_output=True,
        timeout=args.timeout,
    )
    if not result.stdout.strip():
        detail = result.stderr.strip() or "no output"
        raise ValueError(f"Forge did not emit JSON: {detail}")
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError as err:
        raise ValueError(f"Forge emitted invalid JSON: {err}") from err
    if not isinstance(report, dict):
        raise ValueError("Forge JSON output must be an object")
    return report


def _host_svm_home(env: dict[str, str]) -> pathlib.Path:
    if path := env.get("SVM_HOME"):
        return pathlib.Path(path)
    if path := env.get("XDG_DATA_HOME"):
        return pathlib.Path(path) / "svm"
    return pathlib.Path(env["HOME"]) / ".local" / "share" / "svm"


def _classify(
    report: dict[str, Any],
    target_selector: str,
    expected_bounds: dict[str, Any] | None = None,
) -> dict[str, Any]:
    matches = []
    for suite in report.values():
        if not isinstance(suite, dict):
            continue
        results = suite.get("test_results")
        if not isinstance(results, dict):
            continue
        for test_name, result in results.items():
            symbolic = result.get("symbolic") if isinstance(result, dict) else None
            if isinstance(symbolic, dict):
                matches.append((test_name, result, symbolic))
    if len(matches) != 1:
        raise ValueError(f"expected one symbolic result, found {len(matches)}")

    test_name, result, symbolic = matches[0]
    if not test_name.startswith(TEST_NAME):
        raise ValueError(f"Forge ran an unexpected test: {test_name}")

    status = symbolic.get("status")
    replay = symbolic.get("replay")
    if status == "pass":
        if result.get("status") != "Success":
            raise ValueError("Forge emitted an inconsistent symbolic pass")
        if expected_bounds is not None:
            _validate_bounded_agreement(symbolic, expected_bounds)
        return {"status": "bounded_agreement"}

    if status == "incomplete":
        incomplete = symbolic.get("incomplete")
        reason = (
            incomplete.get("reason")
            if isinstance(incomplete, dict)
            else "symbolic execution was incomplete"
        )
        return {"status": "incomplete", "reason": reason}

    if status != "fail_counterexample":
        raise ValueError(f"unsupported Forge symbolic status: {status}")
    if not isinstance(replay, dict) or replay.get("status") != "confirmed":
        raise ValueError("Forge did not concretely replay the counterexample")

    counterexample = symbolic.get("counterexample")
    artifact = symbolic.get("artifact")
    if not isinstance(counterexample, dict) or not isinstance(artifact, dict):
        raise ValueError("Forge did not emit a counterexample artifact")
    wrapper_calldata = counterexample.get("calldata")
    artifact_path = artifact.get("path")
    if not isinstance(artifact_path, str) or not artifact_path:
        raise ValueError("Forge emitted an invalid artifact path")
    return {
        "status": "mismatch",
        "counterexample": {
            "calldata": _target_calldata(target_selector, wrapper_calldata),
            "forge_artifact": artifact_path,
        },
    }


def _validate_bounded_agreement(
    symbolic: dict[str, Any],
    expected: dict[str, Any],
) -> None:
    actual = symbolic.get("bounds")
    if not isinstance(actual, dict):
        raise ValueError("Forge did not report its effective symbolic bounds")
    fields = {
        "timeout_seconds": expected["solver_timeout_seconds"],
        "max_paths": expected["max_paths"],
        "max_solver_queries": expected["max_solver_queries"],
        "max_calldata_bytes": expected["max_calldata_bytes"],
        "max_dynamic_length": expected["max_dynamic_length"],
        "exploration_order": expected["exploration_order"],
        "storage_layout": expected["storage_layout"],
        "default_array_lengths": expected["dynamic_lengths"],
        "default_bytes_lengths": expected["dynamic_lengths"],
        "dynamic_lengths": expected["input_lengths"],
    }
    if expected["max_depth"] is not None:
        fields["max_depth"] = expected["max_depth"]
    disagreements = [
        f"{name}={actual.get(name)!r}"
        for name, value in fields.items()
        if actual.get(name) != value
    ]
    assumptions = symbolic.get("assumptions")
    kinds = set()
    if isinstance(assumptions, list):
        kinds = {
            item.get("kind")
            for item in assumptions
            if isinstance(item, dict) and isinstance(item.get("kind"), str)
        }
    if kinds != {"bounded_exploration", "hash_model"}:
        disagreements.append(f"assumptions={sorted(kinds)!r}")
    if disagreements:
        raise ValueError(
            "Forge effective symbolic configuration disagrees with the request: "
            + ", ".join(disagreements)
        )


def _target_calldata(selector: str, wrapper_calldata: Any) -> str:
    if not isinstance(wrapper_calldata, str) or not re.fullmatch(
        r"0x[0-9a-fA-F]{8,}",
        wrapper_calldata,
    ):
        raise ValueError("Forge emitted invalid counterexample calldata")
    if len(wrapper_calldata) % 2:
        raise ValueError("Forge emitted odd-length counterexample calldata")
    return selector + wrapper_calldata[10:]


_FOUNDRY_TOML = """\
[profile.default]
src = "src"
test = "test"
out = "out"
cache_path = "cache"
libs = []
optimizer = true
optimizer_runs = 200
via_ir = true
evm_version = "{evm_version}"
code_size_limit = 1000000

[symbolic]
exploration_order = "{exploration_order}"
storage_layout = "{storage_layout}"
max_dynamic_length = {max_dynamic_length}
dynamic_lengths = {{ {input_lengths} }}
default_array_lengths = [{dynamic_lengths}]
default_bytes_lengths = [{dynamic_lengths}]
"""


_PREFIX_SETUP_TEMPLATE = """\
        bytes32[] memory prefixResults = new bytes32[]({prefix_count});
{prefix_calls_a}
{prefix_calls_b}
"""


_STATELESS_TEST_TEMPLATE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    function assume(bool condition) external pure;
}}

contract SymbolicDifferentialTest {{
{definitions}
    Vm private constant vm =
        Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    address private solcTarget;
    address private solarTarget;
    // Prevent sequential calls from observing different gas stipends.
    uint256 private constant CALL_GAS = {call_gas};
    bytes4 private constant TARGET_SELECTOR = bytes4({selector});
    bytes private constant SOLC_CODE = hex"{solc_runtime}";
    bytes private constant SOLAR_CODE = hex"{solar_runtime}";

    function setUp() public {{
        solcTarget = _deploy(SOLC_CODE);
        solarTarget = _deploy(SOLAR_CODE);
    }}

    function checkSymbolicDifferential({declarations}) public {{
        bytes memory callData =
            abi.encodeWithSelector(TARGET_SELECTOR{encode_arguments});
        (bool okA, bytes memory retA) =
            solcTarget.staticcall{{gas: CALL_GAS}}(callData);
        (bool okB, bytes memory retB) =
            solarTarget.staticcall{{gas: CALL_GAS}}(callData);

        assert(okA == okB);
        assert(retA.length == retB.length);
        vm.assume(retA.length <= {max_returndata});
{word_checks}
    }}

    function _word(
        bytes memory value,
        uint256 offset
    ) private pure returns (uint256 result) {{
        assembly ("memory-safe") {{
            result := mload(add(add(value, 0x20), offset))
        }}
        uint256 remaining = value.length - offset;
        if (remaining < 32) result >>= (32 - remaining) * 8;
    }}

    function _deploy(bytes memory runtime) private returns (address target) {{
        bytes memory init = abi.encodePacked(
            hex"63", uint32(runtime.length), hex"601260003963",
            uint32(runtime.length), hex"6000f3", runtime
        );
        assembly ("memory-safe") {{
            target := create(0, add(init, 0x20), mload(init))
        }}
        assert(target != address(0));
    }}
}}
"""


_STATEFUL_TEST_TEMPLATE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    function assume(bool condition) external pure;
}}

contract SymbolicDifferentialTest {{
{definitions}
    Vm private constant vm =
        Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    address private solcTarget;
    address private solarTarget;
    // Prevent sequential calls from observing different gas stipends.
    uint256 private constant CALL_GAS = {call_gas};
    bytes4 private constant TARGET_SELECTOR = bytes4({selector});
    bytes private constant SOLC_CODE = hex"{solc_runtime}";
    bytes private constant SOLAR_CODE = hex"{solar_runtime}";

    function setUp() public {{
        solcTarget = _deploy(SOLC_CODE);
        solarTarget = _deploy(SOLAR_CODE);
    }}

    function checkSymbolicDifferential({declarations}) public {{
        bytes memory callData =
            abi.encodeWithSelector(TARGET_SELECTOR{encode_arguments});
{prefix_setup}
{suffix_comparison}
    }}

    function _word(
        bytes memory value,
        uint256 offset
    ) private pure returns (uint256 result) {{
        assembly ("memory-safe") {{
            result := mload(add(add(value, 0x20), offset))
        }}
        uint256 remaining = value.length - offset;
        if (remaining < 32) result >>= (32 - remaining) * 8;
    }}

    function _deploy(bytes memory runtime) private returns (address target) {{
        bytes memory init = abi.encodePacked(
            hex"63", uint32(runtime.length), hex"601260003963",
            uint32(runtime.length), hex"6000f3", runtime
        );
        assembly ("memory-safe") {{
            target := create(0, add(init, 0x20), mload(init))
        }}
        assert(target != address(0));
    }}
}}
"""


_VIEW_SUFFIX_TEMPLATE = """\
        (bool okA, bytes memory retA) =
            solcTarget.staticcall{{gas: CALL_GAS}}(callData);
        (bool okB, bytes memory retB) =
            solarTarget.staticcall{{gas: CALL_GAS}}(callData);

        assert(okA == okB);
        assert(retA.length == retB.length);
        vm.assume(retA.length <= {max_returndata});
{word_checks}
"""


_STATEFUL_SUFFIX_TEMPLATE = """\
        (bool okA, bytes memory retA) =
            solcTarget.call{{gas: CALL_GAS}}(callData);
        (bool okB, bytes memory retB) =
            solarTarget.call{{gas: CALL_GAS}}(callData);

        assert(okA == okB);
        assert(retA.length == retB.length);
        vm.assume(retA.length <= {max_returndata});
{word_checks}
"""


if __name__ == "__main__":
    raise SystemExit(main())
