#!/usr/bin/env python3
"""Compare solc and Solar codegen on the curated runtime corpus."""

# Adapted from walnuthq/solidity-compiler-benchmarks at
# 01209d2b8ac81645b92e3ef801b5bcdfd61bfd69 under Apache-2.0.

from __future__ import annotations

import argparse
import copy
import gzip
import hashlib
import json
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from collections.abc import Sequence
from dataclasses import dataclass
from functools import cache, lru_cache
from pathlib import Path

from cases import (
    DEFAULT_FOURTH,
    DEFAULT_SENDER,
    DEFAULT_SPENDER,
    DEFAULT_THIRD,
    MAX_UINT256,
    TEST_CASES,
    ZERO_ADDRESS,
    TestCase,
    gas_calls,
    runtime_checks,
)
from common import (
    DEFAULT_PRIVATE_KEY,
    DEFAULT_RPC_URL,
    PROJECTS_ROOT,
    REPOSITORY_ROOT,
    RUNTIME_CORPUS_ROOT,
    load_project,
    parse_receipt_int,
    project_slice,
    run,
    stop_anvil,
)

ROOT = RUNTIME_CORPUS_ROOT
RESULT_ROOT = REPOSITORY_ROOT / "target/codegen-bench"
CAST_DEPLOY_TIMEOUT = 45
CAST_TX_TIMEOUT = 30
CAST_READ_TIMEOUT = 10
CAST_RPC_TIMEOUT = 10
CAST_GAS_LIMIT = "80000000"
RUNTIME_FIXTURES = ROOT / "fixtures/runtime/RuntimeFixtures.sol"
ARTIFACT_DUMP_KINDS = (
    "mir",
    "evm-ir",
    "evm-ir-runtime",
    "disasm-deploy",
    "disasm-runtime",
)

EVM_OPCODES = {
    0x00: "STOP", 0x01: "ADD", 0x02: "MUL", 0x03: "SUB", 0x04: "DIV", 0x05: "SDIV",
    0x06: "MOD", 0x07: "SMOD", 0x08: "ADDMOD", 0x09: "MULMOD", 0x0A: "EXP", 0x0B: "SIGNEXTEND",
    0x10: "LT", 0x11: "GT", 0x12: "SLT", 0x13: "SGT", 0x14: "EQ", 0x15: "ISZERO",
    0x16: "AND", 0x17: "OR", 0x18: "XOR", 0x19: "NOT", 0x1A: "BYTE", 0x1B: "SHL", 0x1C: "SHR", 0x1D: "SAR",
    0x20: "KECCAK256",
    0x30: "ADDRESS", 0x31: "BALANCE", 0x32: "ORIGIN", 0x33: "CALLER", 0x34: "CALLVALUE", 0x35: "CALLDATALOAD",
    0x36: "CALLDATASIZE", 0x37: "CALLDATACOPY", 0x38: "CODESIZE", 0x39: "CODECOPY", 0x3A: "GASPRICE",
    0x3B: "EXTCODESIZE", 0x3C: "EXTCODECOPY", 0x3D: "RETURNDATASIZE", 0x3E: "RETURNDATACOPY", 0x3F: "EXTCODEHASH",
    0x40: "BLOCKHASH", 0x41: "COINBASE", 0x42: "TIMESTAMP", 0x43: "NUMBER", 0x44: "PREVRANDAO", 0x45: "GASLIMIT",
    0x46: "CHAINID", 0x47: "SELFBALANCE", 0x48: "BASEFEE", 0x49: "BLOBHASH", 0x4A: "BLOBBASEFEE",
    0x50: "POP", 0x51: "MLOAD", 0x52: "MSTORE", 0x53: "MSTORE8", 0x54: "SLOAD", 0x55: "SSTORE",
    0x56: "JUMP", 0x57: "JUMPI", 0x58: "PC", 0x59: "MSIZE", 0x5A: "GAS", 0x5B: "JUMPDEST", 0x5C: "TLOAD", 0x5D: "TSTORE", 0x5E: "MCOPY", 0x5F: "PUSH0",
    0xF0: "CREATE", 0xF1: "CALL", 0xF2: "CALLCODE", 0xF3: "RETURN", 0xF4: "DELEGATECALL", 0xF5: "CREATE2", 0xFA: "STATICCALL", 0xFD: "REVERT", 0xFE: "INVALID", 0xFF: "SELFDESTRUCT",
}


def disassemble_evm(bytecode: bytes) -> str:
    """Format EVM bytecode like Solar's disassembly dump."""
    instructions: list[tuple[int, str, bytes]] = []
    offset = 0
    while offset < len(bytecode):
        opcode = bytecode[offset]
        width = opcode - 0x5F if 0x60 <= opcode <= 0x7F else 0
        data = bytecode[offset + 1 : offset + 1 + width]
        if width:
            name = f"PUSH{width}"
        elif 0x80 <= opcode <= 0x8F:
            name = f"DUP{opcode - 0x7F}"
        elif 0x90 <= opcode <= 0x9F:
            name = f"SWAP{opcode - 0x8F}"
        elif 0xA0 <= opcode <= 0xA4:
            name = f"LOG{opcode - 0xA0}"
        else:
            name = EVM_OPCODES.get(opcode, f"UNKNOWN 0x{opcode:02x}")
        instructions.append((offset, name, data))
        offset += 1 + width

    jumpdests = {offset for offset, name, _ in instructions if name == "JUMPDEST"}
    targets = set()
    for index, (_, name, _) in enumerate(instructions):
        if name in {"JUMP", "JUMPI"} and index:
            _, previous, data = instructions[index - 1]
            if previous.startswith("PUSH") and data:
                target = int.from_bytes(data, "big")
                if target in jumpdests:
                    targets.add(target)
    labels = {offset: index for index, offset in enumerate(sorted(targets))}

    output = []
    for index, (offset, name, data) in enumerate(instructions):
        if name == "JUMPDEST" and offset in labels:
            output.append(f"; bb{labels[offset]}")
        line = name
        if data:
            line += f" 0x{data.hex()}"
        if name.startswith("PUSH") and index + 1 < len(instructions):
            if instructions[index + 1][1] in {"JUMP", "JUMPI"}:
                target = int.from_bytes(data, "big") if data else 0
                line += f" ; bb{labels[target]}" if target in labels else " ; unknown"
        elif name in {"JUMP", "JUMPI"} and (not index or not instructions[index - 1][1].startswith("PUSH")):
            line += " ; unknown"
        output.append(line)
    return "\n".join(output) + "\n"

RESET = "\033[0m"
YELLOW = "\033[33m"
RED = "\033[31m"
USE_COLOR = sys.stdout.isatty()


def _color(text: str, color: str) -> str:
    if not USE_COLOR:
        return text
    return f"{color}{text}{RESET}"


def display_path(path: str | Path) -> str:
    path = Path(path)
    if not path.is_absolute():
        return str(path)
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        pass

    try:
        return str(path.relative_to(ROOT.parent).as_posix()).replace("../", "", 1)
    except ValueError:
        pass

    home = Path.home()
    try:
        return "~/" + str(path.relative_to(home))
    except ValueError:
        pass

    return path.name


def display_command(cmd: Sequence[str | Path]) -> str:
    sanitized = []
    for part in cmd:
        text = str(part)
        candidate = Path(text)
        sanitized.append(display_path(candidate) if candidate.is_absolute() else text)
    return " ".join(sanitized)


def compiler_output_fingerprint(output: str) -> str:
    payload = json.loads(output)
    if errors := payload.get("errors"):
        payload["errors"] = sorted(
            errors,
            key=lambda error: json.dumps(error, sort_keys=True, separators=(",", ":")),
        )
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode()).hexdigest()


def verbose_log(enabled: bool, message: str) -> None:
    if enabled:
        print(message, flush=True)


def find_binary(explicit: str | None, candidates: Sequence[str]) -> Path | None:
    if explicit:
        path = Path(explicit)
        if path.exists():
            return path.resolve()
        found = shutil.which(explicit)
        if found:
            return Path(found).resolve()
        return None

    for candidate in candidates:
        path = Path(candidate)
        if not path.is_absolute():
            path = REPOSITORY_ROOT / candidate
        if path.exists():
            return path.resolve()
        found = shutil.which(candidate)
        if found:
            return Path(found).resolve()
    return None


def binary_version(path: Path) -> tuple[str, str]:
    result = run([str(path), "--version"], timeout=30)
    if result.returncode != 0:
        error = (result.stderr or result.stdout or "version command failed").strip()
        return "unavailable", error[:500]
    text = (result.stdout + "\n" + result.stderr).strip()
    match = re.search(r"(\d+\.\d+\.\d+(?:[-+][^\s]+)?)", text)
    version = match.group(1) if match else text.splitlines()[0] if text else "unknown"
    return version, ""


def parse_version_tuple(version: str) -> tuple[int, int, int] | None:
    match = re.match(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def version_in_range(version: str, minimum: str | None, maximum: str | None) -> bool:
    parsed = parse_version_tuple(version)
    if parsed is None:
        return True
    if minimum and parsed < parse_version_tuple(minimum):
        return False
    return not (maximum and parsed > parse_version_tuple(maximum))


@dataclass(frozen=True)
class CompilerSpec:
    compiler_id: str
    label: str
    path: Path
    kind: str


def standard_json_input(test_case: TestCase) -> str:
    if test_case.source_code is None:
        raise ValueError(f"project case {test_case.test_id} has no inline source")
    source_name = test_case.source_name or f"{test_case.test_id}.sol"
    payload = {
        "language": "Solidity",
        "sources": {
            source_name: {
                "content": test_case.source_code,
            }
        },
        "settings": {
            "metadata": {"appendCBOR": False, "bytecodeHash": "none"},
            "optimizer": {"enabled": True, "runs": 200},
            "viaIR": True,
            "outputSelection": {
                "*": {
                    "*": [
                        "abi",
                        "evm.bytecode.object",
                        "evm.deployedBytecode.object",
                    ]
                }
            },
        },
    }
    return json.dumps(payload)


@cache
def project_full_standard_json_input(project_file: str) -> str:
    path = PROJECTS_ROOT / project_file
    with gzip.open(path, mode="rt", encoding="utf-8") as file:
        return file.read()


def full_project_standard_json_input(project_file: str) -> str:
    return project_full_standard_json_input(project_file)


def with_evm_version(input_text: str, evm_version: str | None) -> str:
    if evm_version is None:
        return input_text
    payload = json.loads(input_text)
    payload.setdefault("settings", {})["evmVersion"] = evm_version
    return json.dumps(payload)


def compiler_input(
    test_case: TestCase, evm_version: str | None
) -> tuple[str, int, str]:
    if test_case.project_file is not None:
        if test_case.whole_project:
            input_text = project_full_standard_json_input(test_case.project_file)
            timeout = 900
        else:
            input_text = project_standard_json_input(
                test_case.project_file,
                test_case.source,
                test_case.contract_name,
                test_case.settings_profile,
            )
            timeout = 180
    else:
        input_text = standard_json_input(test_case)
        timeout = 120
    input_text = with_evm_version(input_text, evm_version)
    return input_text, timeout, hashlib.sha256(input_text.encode()).hexdigest()


def artifact_compiler_input(input_text: str, test_case: TestCase, kind: str) -> str:
    payload = json.loads(input_text)
    source = test_case.source_name or test_case.source or f"{test_case.test_id}.sol"
    outputs = [
        "abi",
        "evm.bytecode.object",
        "evm.deployedBytecode.object",
    ]
    if kind == "solc":
        outputs.append("irOptimized")
    payload.setdefault("settings", {})["outputSelection"] = {
        source: {test_case.contract_name: outputs}
    }
    return json.dumps(payload, sort_keys=True, separators=(",", ":"))


def split_solar_artifact_output(
    stdout: str, contract_path: str
) -> tuple[dict[str, str], str]:
    marker = "\n{"
    json_start = stdout.rfind(marker)
    if json_start < 0 and stdout.startswith("{"):
        return {}, stdout
    if json_start < 0:
        raise ValueError("Solar artifact output did not contain Standard JSON")

    dump = stdout[:json_start].strip()
    output = stdout[json_start + 1 :]
    escaped = re.escape(contract_path)
    headings = list(
        re.finditer(
            rf"^// === {escaped}(?: \((creation|runtime|deployment)\))? ===$",
            dump,
            re.MULTILINE,
        )
    )
    names = (
        "mir.mir",
        "creation.evmir",
        "runtime.evmir",
        "creation.disasm",
        "runtime.disasm",
    )
    if len(headings) != len(names):
        raise ValueError(
            f"expected {len(names)} Solar artifact sections, found {len(headings)}"
        )
    artifacts = {}
    following = [*headings[1:], None]
    for name, match, next_match in zip(names, headings, following, strict=True):
        end = next_match.start() if next_match else len(dump)
        artifacts[name] = dump[match.end() : end].strip() + "\n"
    return artifacts, output


def selected_contract_output(
    output: dict[str, object], test_case: TestCase
) -> dict[str, object]:
    contracts = output.get("contracts") or {}
    for source_contracts in contracts.values():
        if test_case.contract_name in source_contracts:
            return source_contracts[test_case.contract_name]
    return {}


def write_artifacts(
    root: Path,
    spec: CompilerSpec,
    test_case: TestCase,
    prepared_input: tuple[str, int, str],
) -> str:
    if test_case.whole_project:
        return ""
    input_text, timeout, _ = prepared_input
    input_text = artifact_compiler_input(input_text, test_case, spec.kind)
    output_dir = root / test_case.test_id / spec.compiler_id
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "input.json").write_text(input_text + "\n")

    cmd = [str(spec.path), "--standard-json"]
    source = test_case.source_name or test_case.source or f"{test_case.test_id}.sol"
    contract_path = f"{source}:{test_case.contract_name}"
    if spec.kind == "solar":
        kinds = ",".join(ARTIFACT_DUMP_KINDS)
        cmd.extend(["--color", "never", f"-Zdump={kinds}={contract_path}"])
    proc = run(cmd, input_text=input_text, timeout=timeout)
    if proc.returncode != 0:
        return (proc.stderr or proc.stdout or "artifact compiler failed")[:1000]

    extra = {}
    raw_output = proc.stdout
    if spec.kind == "solar":
        try:
            extra, raw_output = split_solar_artifact_output(proc.stdout, contract_path)
        except ValueError as error:
            return str(error)
    (output_dir / "output.json").write_text(raw_output.rstrip() + "\n")
    for name, contents in extra.items():
        (output_dir / name).write_text(contents)

    try:
        contract = selected_contract_output(json.loads(raw_output), test_case)
    except json.JSONDecodeError as error:
        return f"invalid artifact compiler output: {error}"
    evm = contract.get("evm") or {}
    bytecodes: dict[str, bytes] = {}
    for prefix, key in (("creation", "bytecode"), ("runtime", "deployedBytecode")):
        bytecode = evm.get(key) or {}
        if object_hex := bytecode.get("object"):
            try:
                bytes_ = bytes.fromhex(str(object_hex).removeprefix("0x"))
            except ValueError as error:
                return f"invalid {prefix} bytecode: {error}"
            bytecodes[prefix] = bytes_
            (output_dir / f"{prefix}.hex").write_text(str(object_hex) + "\n")
    if spec.kind == "solc":
        runtime = bytecodes.get("runtime", b"")
        creation = bytecodes.get("creation", b"")
        if creation:
            deployment = creation.removesuffix(runtime) if runtime else creation
            (output_dir / "creation.disasm").write_text(disassemble_evm(deployment))
        if runtime:
            (output_dir / "runtime.disasm").write_text(disassemble_evm(runtime))
    if spec.kind == "solc" and (ir := contract.get("irOptimized")):
        (output_dir / "optimized-ir.yul").write_text(str(ir).rstrip() + "\n")
    return ""


def project_standard_json_input(
    project_file: str, source: str, contract_name: str, settings_profile: str = ""
) -> str:
    path = PROJECTS_ROOT / project_file
    project = load_project(path)
    if settings_profile == "runtime":
        project_settings = project["settings"]
        settings = {
            "metadata": {"appendCBOR": False, "bytecodeHash": "none"},
            "optimizer": {"enabled": True, "runs": 200},
            "remappings": project_settings.get("remappings", []),
            "viaIR": True,
        }
    else:
        settings = dict(project["settings"])
    settings["outputSelection"] = {
        source: {
            contract_name: [
                "abi",
                "evm.bytecode.object",
                "evm.deployedBytecode.object",
            ]
        }
    }
    payload = {
        "language": project["language"],
        "sources": project_slice(project, source),
        "settings": settings,
    }
    return json.dumps(payload)


LONG_COMPILE_CUTOFF_SECONDS = 10.0


def compile_case(
    spec: CompilerSpec,
    test_case: TestCase,
    prepared_input: tuple[str, int, str] | None,
    compile_repeats: int = 1,
    repeat_long_compiles: bool = False,
) -> dict[str, object]:
    result = {
        "compiler_id": spec.compiler_id,
        "label": spec.label,
        "status": "pending",
        "bytecode": "",
        "runtime_bytecode": "",
        "bytecode_size": 0,
        "runtime_size": 0,
        "compile_time_seconds": 0.0,
        "peak_rss_bytes": None,
        "error": "",
    }
    if test_case.project_file is not None:
        result.update(source=test_case.source, project=test_case.project)
        if not test_case.project_path.exists():
            result["status"] = "failed"
            result["error"] = f"vendored project not found: {test_case.project_file}"
            return result
    if prepared_input is None:
        raise ValueError("compiler input is unavailable")
    input_text, timeout, input_fingerprint = prepared_input

    result["input_fingerprint"] = input_fingerprint
    cmd = [str(spec.path), "--standard-json"]
    samples = []
    reference_output = None
    output_fingerprint = None
    proc = None
    for _ in range(max(1, compile_repeats)):
        started = time.monotonic()
        proc = run(
            cmd,
            input_text=input_text,
            timeout=timeout,
            measure_peak_rss=True,
        )
        samples.append(time.monotonic() - started)
        if proc.returncode != 0:
            break
        if reference_output is None:
            reference_output = proc.stdout
        elif proc.stdout != reference_output:
            output_fingerprint = output_fingerprint or compiler_output_fingerprint(
                reference_output
            )
            if compiler_output_fingerprint(proc.stdout) != output_fingerprint:
                result["status"] = "failed"
                result["error"] = "compiler output changed across repeated runs"
                return result
        # One sample is representative for long compiles; repeating a
        # minute-scale solc run per repeat would dominate the whole benchmark.
        if not repeat_long_compiles and samples[-1] >= LONG_COMPILE_CUTOFF_SECONDS:
            break
    result["compile_time_seconds"] = statistics.median(samples)
    result["compile_time_samples"] = samples
    result["peak_rss_bytes"] = proc.peak_rss_bytes
    result["command"] = display_command(cmd)
    if proc.returncode != 0:
        result["status"] = "failed"
        result["error"] = (proc.stderr or proc.stdout or "compiler failed")[:1000]
        return result

    result["output_fingerprint"] = output_fingerprint or compiler_output_fingerprint(
        reference_output
    )

    if test_case.whole_project:
        compiled, objects, error = parse_whole_project_output(proc.stdout)
        if error:
            result["status"] = "failed"
            result["error"] = error
            return result
        # Compile-time only: no single contract is selected, so bytecode
        # sizes and every downstream deploy/gas/runtime stage do not apply.
        # `bytecode_objects` records how many entries carried real code so a
        # compiler silently emitting nothing cannot count as a fast compile.
        result["status"] = "ok"
        result["bytecode_size"] = None
        result["runtime_size"] = None
        result["contracts_compiled"] = compiled
        result["bytecode_objects"] = objects
        return result

    bytecode, runtime, error = parse_standard_json_output(proc.stdout, test_case)
    if not bytecode:
        result["status"] = "failed"
        result["error"] = error
        return result

    result["status"] = "ok"
    result["bytecode"] = bytecode
    result["runtime_bytecode"] = runtime or ""
    result["bytecode_size"] = len(bytecode) // 2
    result["runtime_size"] = len(runtime or "") // 2
    return result


def parse_whole_project_output(stdout: str) -> tuple[int, int, str]:
    """Returns contract entries, entries with nonempty bytecode, and an error."""
    try:
        output = json.loads(stdout)
    except json.JSONDecodeError as error:
        return 0, 0, f"invalid compiler output: {error}"
    errors = [
        entry.get("formattedMessage") or entry.get("message") or "error"
        for entry in output.get("errors", [])
        if entry.get("severity") == "error"
    ]
    if errors:
        return 0, 0, "; ".join(errors)[:1000]
    compiled = 0
    objects = 0
    for contracts in output.get("contracts", {}).values():
        for data in contracts.values():
            compiled += 1
            if ((data.get("evm") or {}).get("bytecode") or {}).get("object"):
                objects += 1
    if objects == 0:
        return compiled, 0, "no bytecode objects were emitted"
    return compiled, objects, ""


def parse_full_project_output(stdout: str, test_case: TestCase) -> tuple[int, int, str]:
    del test_case
    return parse_whole_project_output(stdout)


def parse_standard_json_output(
    stdout: str, test_case: TestCase
) -> tuple[str | None, str | None, str]:
    try:
        output = json.loads(stdout)
    except json.JSONDecodeError as exc:
        return None, None, f"invalid JSON output: {exc}"

    errors = output.get("errors") or []
    fatal = [
        err.get("formattedMessage") or err.get("message") or str(err)
        for err in errors
        if err.get("severity") == "error"
    ]
    if fatal:
        return None, None, fatal[0][:1000]

    contracts = output.get("contracts") or {}
    for source_contracts in contracts.values():
        if test_case.contract_name in source_contracts:
            contract = source_contracts[test_case.contract_name]
            evm = contract.get("evm") or {}
            bytecode = ((evm.get("bytecode") or {}).get("object") or "").strip()
            deployed = ((evm.get("deployedBytecode") or {}).get("object") or "").strip()
            if bytecode:
                return bytecode, deployed, ""

    available = [
        name for source_contracts in contracts.values() for name in source_contracts
    ]
    return (
        None,
        None,
        f"contract {test_case.contract_name} not found; available: {', '.join(available)}",
    )


@cache
def compile_runtime_fixture(
    solc_path: str, contract_name: str
) -> tuple[str | None, str]:
    source_name = str(RUNTIME_FIXTURES.relative_to(ROOT))
    payload = {
        "language": "Solidity",
        "sources": {source_name: {"content": RUNTIME_FIXTURES.read_text()}},
        "settings": {
            "optimizer": {"enabled": True, "runs": 200},
            "outputSelection": {"*": {contract_name: ["evm.bytecode.object"]}},
        },
    }
    proc = run(
        [solc_path, "--standard-json"], input_text=json.dumps(payload), timeout=120
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or "fixture compiler failed")[:1000]
    try:
        output = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, f"invalid fixture compiler JSON: {exc}"
    errors = [
        error.get("formattedMessage") or error.get("message") or str(error)
        for error in output.get("errors") or []
        if error.get("severity") == "error"
    ]
    if errors:
        return None, errors[0][:1000]
    bytecode = (
        output.get("contracts", {})
        .get(source_name, {})
        .get(contract_name, {})
        .get("evm", {})
        .get("bytecode", {})
        .get("object", "")
    )
    if not bytecode:
        return None, f"fixture contract {contract_name} was not produced"
    return str(bytecode), ""


def check_tool(name: str) -> bool:
    return shutil.which(name) is not None


def start_anvil(rpc_url: str) -> subprocess.Popen[bytes]:
    match = re.fullmatch(r"http://(?:127\.0\.0\.1|localhost):(\d+)", rpc_url)
    if match is None:
        raise ValueError("`--start-anvil` requires a localhost HTTP RPC URL")
    proc = subprocess.Popen(
        [
            "anvil",
            "--port",
            match.group(1),
            "--steps-tracing",
            "--disable-code-size-limit",
            "--gas-limit",
            "100000000",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
        if proc.poll() is not None:
            raise RuntimeError("anvil exited before accepting RPC requests")
        _, error = rpc_request("eth_chainId", (), rpc_url)
        if not error:
            return proc
        time.sleep(0.1)
    stop_anvil(proc)
    raise RuntimeError("anvil did not accept RPC requests")


def abi_encode_constructor(
    constructor_args: Sequence[str], constructor_sig: str | None
) -> str | None:
    if not constructor_args:
        return ""
    if not constructor_sig:
        return None
    proc = run(["cast", "abi-encode", constructor_sig, *constructor_args], timeout=30)
    if proc.returncode != 0:
        return None
    encoded = proc.stdout.strip()
    return encoded.removeprefix("0x")


def deploy_contract(
    bytecode: str,
    test_case: TestCase,
    rpc_url: str,
    private_key: str,
) -> tuple[str | None, int | None, str]:
    return deploy_creation_code(
        bytecode,
        test_case.constructor_args,
        getattr(test_case, "constructor_sig", None),
        rpc_url,
        private_key,
    )


def deploy_creation_code(
    bytecode: str,
    constructor_args: Sequence[str],
    constructor_sig: str | None,
    rpc_url: str,
    private_key: str,
) -> tuple[str | None, int | None, str]:
    if not bytecode.startswith("0x"):
        bytecode = "0x" + bytecode

    encoded = abi_encode_constructor(constructor_args, constructor_sig)
    if encoded is None:
        return None, None, "constructor args require constructor_sig"
    bytecode += encoded
    return deploy_creation_code_from_file(bytecode, rpc_url, private_key)


def deploy_creation_code_from_file(
    bytecode: str,
    rpc_url: str,
    private_key: str,
) -> tuple[str | None, int | None, str]:
    sender, error = private_key_address(private_key)
    if sender is None:
        return None, None, error

    transaction = {
        "from": sender,
        "data": bytecode,
        "gas": hex(int(CAST_GAS_LIMIT)),
    }
    tx_hash, error = rpc_request_from_file(
        "eth_sendTransaction",
        (transaction,),
        rpc_url,
        CAST_DEPLOY_TIMEOUT,
    )
    if not isinstance(tx_hash, str):
        return None, None, error or "deploy transaction missing hash"

    deadline = time.monotonic() + CAST_DEPLOY_TIMEOUT
    while True:
        receipt, error = rpc_request("eth_getTransactionReceipt", (tx_hash,), rpc_url)
        if error:
            return None, None, error
        if isinstance(receipt, dict):
            return parse_deploy_receipt(receipt)
        if time.monotonic() >= deadline:
            return None, None, "timed out waiting for deploy receipt"
        time.sleep(0.1)


@lru_cache
def private_key_address(private_key: str) -> tuple[str | None, str]:
    if private_key == DEFAULT_PRIVATE_KEY:
        return DEFAULT_SENDER, ""
    proc = run(["cast", "wallet", "address", "--private-key", private_key], timeout=30)
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    return proc.stdout.strip(), ""


def parse_deploy_receipt(
    data: dict[str, object],
) -> tuple[str | None, int | None, str]:
    status = parse_receipt_int(data.get("status"))
    gas = data.get("gasUsed")
    deploy_gas = parse_receipt_int(gas)
    if status is not None and status != 1:
        return (
            None,
            deploy_gas,
            f"deploy transaction failed (status={status}, gasUsed={deploy_gas})",
        )
    if deploy_gas is None:
        return None, None, "deploy receipt missing gasUsed"
    contract_address = data.get("contractAddress")
    return (
        contract_address if isinstance(contract_address, str) else None,
        deploy_gas,
        "",
    )


def rpc_request_from_file(
    method: str,
    params: Sequence[object],
    rpc_url: str,
    timeout: int = CAST_READ_TIMEOUT,
) -> tuple[object | None, str]:
    with tempfile.NamedTemporaryFile(
        mode="w",
        encoding="utf-8",
        prefix="solar-bench-rpc-",
        suffix=".json",
        delete=False,
    ) as params_file:
        json.dump(list(params), params_file)
        params_path = Path(params_file.name)
    try:
        proc = run(
            ["cast", "rpc", "--rpc-url", rpc_url, "--raw", method],
            input_path=params_path,
            timeout=timeout,
        )
    finally:
        params_path.unlink(missing_ok=True)
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or f"{method} failed")[:1000]
    try:
        return json.loads(proc.stdout), ""
    except json.JSONDecodeError as exc:
        return None, f"invalid {method} JSON: {exc}"


def call_contract(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
    private_key: str,
) -> tuple[int | None, str]:
    proc = run(
        [
            "cast",
            "send",
            address,
            signature,
            *args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
            "--private-key",
            private_key,
            "--json",
        ],
        timeout=CAST_TX_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, f"invalid call JSON: {exc}"
    status = parse_receipt_int(data.get("status"))
    gas = parse_receipt_int(data.get("gasUsed"))
    if status is not None and status != 1:
        return None, f"transaction failed (status={status}, gasUsed={gas})"
    if gas is None:
        return None, "transaction receipt missing gasUsed"
    return gas, ""


def read_contract(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> tuple[str | None, str]:
    proc = run(
        [
            "cast",
            "call",
            address,
            signature,
            *args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
        ],
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    value = " ".join(proc.stdout.split())
    if value.startswith("0x"):
        value = value.lower()
    return value, ""


def rpc_request(
    method: str, params: Sequence[object], rpc_url: str
) -> tuple[object | None, str]:
    proc = run(
        ["cast", "rpc", "--rpc-url", rpc_url, "--raw", method],
        input_text=json.dumps(list(params)),
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or f"{method} failed")[:1000]
    try:
        return json.loads(proc.stdout), ""
    except json.JSONDecodeError as exc:
        return None, f"invalid {method} JSON: {exc}"


def send_value(address: str, amount: str, rpc_url: str, private_key: str) -> str:
    proc = run(
        [
            "cast",
            "send",
            address,
            "--value",
            amount,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            str(CAST_RPC_TIMEOUT),
            "--timeout",
            str(CAST_RPC_TIMEOUT),
            "--gas-limit",
            CAST_GAS_LIMIT,
            "--private-key",
            private_key,
            "--json",
        ],
        timeout=CAST_TX_TIMEOUT,
    )
    if proc.returncode != 0:
        return proc.stderr[:1000]
    try:
        receipt = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return f"invalid value-transfer JSON: {exc}"
    status = parse_receipt_int(receipt.get("status"))
    return "" if status in (None, 1) else f"value transfer failed (status={status})"


def encode_calldata(signature: str, args: Sequence[str]) -> tuple[str | None, str]:
    proc = run(["cast", "calldata", signature, *args], timeout=30)
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    return proc.stdout.strip(), ""


def eth_call_raw(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> tuple[str | None, str | None, str]:
    calldata, error = encode_calldata(signature, args)
    if calldata is None:
        return None, None, error
    proc = run(
        [
            "cast",
            "rpc",
            "--rpc-url",
            rpc_url,
            "--raw",
            "eth_call",
            json.dumps([{"to": address, "data": calldata}, "latest"]),
        ],
        timeout=CAST_READ_TIMEOUT,
    )
    if proc.returncode == 0:
        try:
            value = json.loads(proc.stdout)
        except json.JSONDecodeError as exc:
            return None, None, f"invalid eth_call JSON: {exc}"
        return str(value).lower(), None, ""

    message = proc.stderr or proc.stdout or "eth_call failed"
    data_matches = re.findall(r"0x[0-9a-fA-F]+", message)
    if data_matches:
        return None, data_matches[-1].lower(), ""
    return None, None, message[:1000]


def runtime_ok(label: str, value: object) -> dict[str, object]:
    return {"label": label, "status": "ok", "value": str(value)}


def runtime_error(label: str, error: str) -> dict[str, object]:
    return {"label": label, "status": "failed", "error": error}


def checked_value(label: str, actual: object, expected: object) -> dict[str, object]:
    actual_text = str(actual)
    expected_text = str(expected)
    if actual_text != expected_text:
        return runtime_error(label, f"expected {expected_text}, got {actual_text}")
    return runtime_ok(label, actual_text)


def read_uint(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> tuple[int | None, str]:
    value, error = read_contract(address, signature, args, rpc_url)
    if value is None:
        return None, error
    try:
        return int(value.split()[0], 0), ""
    except (ValueError, IndexError):
        return None, f"invalid uint result: {value}"


def read_address(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> tuple[str | None, str]:
    value, error = read_contract(address, signature, args, rpc_url)
    if value is None:
        return None, error
    match = re.search(r"0x[0-9a-fA-F]{40}", value)
    return (
        (match.group(0).lower(), "")
        if match
        else (None, f"invalid address result: {value}")
    )


def decode_words(data: str) -> list[int]:
    raw = data.removeprefix("0x")
    if len(raw) % 64 != 0:
        raise ValueError(f"ABI result has {len(raw)} hex digits")
    return [int(raw[index : index + 64], 16) for index in range(0, len(raw), 64)]


def run_vesting_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> list[dict[str, object]]:
    error = send_value(address, "1000", rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-eth-setup", error)]
    _, error = call_contract(address, "release()", (), rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-eth-release", error)]

    token_bytecode, error = compile_runtime_fixture(str(solc_path), "RuntimeERC20")
    if token_bytecode is None:
        return [runtime_error("cold-vesting-token-compile", error)]
    token, _, error = deploy_creation_code(
        token_bytecode,
        (address, "2000"),
        "constructor(address,uint256)",
        rpc_url,
        private_key,
    )
    if token is None:
        return [runtime_error("cold-vesting-token-deploy", error)]
    _, error = call_contract(
        address, "release(address)", (token,), rpc_url, private_key
    )
    if error:
        return [runtime_error("cold-vesting-token-release", error)]

    observations = []
    reads = (
        ("cold-vesting-released-eth", address, "released()(uint256)", (), 1000),
        ("cold-vesting-releasable-eth", address, "releasable()(uint256)", (), 0),
        (
            "cold-vesting-released-token",
            address,
            "released(address)(uint256)",
            (token,),
            2000,
        ),
        (
            "cold-vesting-releasable-token",
            address,
            "releasable(address)(uint256)",
            (token,),
            0,
        ),
        (
            "cold-vesting-token-empty",
            token,
            "balanceOf(address)(uint256)",
            (address,),
            0,
        ),
        (
            "cold-vesting-owner-token",
            token,
            "balanceOf(address)(uint256)",
            (DEFAULT_SENDER,),
            2000,
        ),
    )
    for label, target, signature, args, expected in reads:
        value, error = read_uint(target, signature, args, rpc_url)
        observations.append(
            runtime_error(label, error)
            if value is None
            else checked_value(label, value, expected)
        )
    balance, error = rpc_request("eth_getBalance", (address, "latest"), rpc_url)
    if balance is None:
        observations.append(runtime_error("cold-vesting-eth-empty", error))
    else:
        observations.append(
            checked_value("cold-vesting-eth-empty", int(str(balance), 16), 0)
        )
    return observations


def run_fractional_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> list[dict[str, object]]:
    nft_bytecode, error = compile_runtime_fixture(str(solc_path), "RuntimeNFT")
    if nft_bytecode is None:
        return [runtime_error("cold-fractional-nft-compile", error)]
    nft, _, error = deploy_creation_code(nft_bytecode, (), None, rpc_url, private_key)
    if nft is None:
        return [runtime_error("cold-fractional-nft-deploy", error)]
    _, error = call_contract(
        nft, "setApprovalForAll(address,bool)", (address, "true"), rpc_url, private_key
    )
    if error:
        return [runtime_error("cold-fractional-approve-nft", error)]
    _, error = call_contract(
        address,
        "split(address,uint256,uint256,string,string)",
        (nft, "1", "1000", "Fractionalized NFT", "FRAC"),
        rpc_url,
        private_key,
    )
    if error:
        return [runtime_error("cold-fractional-split", error)]

    vault_data, _, error = eth_call_raw(address, "getVault(uint256)", ("1",), rpc_url)
    if vault_data is None:
        return [runtime_error("cold-fractional-vault", error or "getVault reverted")]
    try:
        vault = decode_words(vault_data)
    except ValueError as exc:
        return [runtime_error("cold-fractional-vault", str(exc))]
    if len(vault) != 4:
        return [
            runtime_error(
                "cold-fractional-vault", f"expected 4 words, got {len(vault)}"
            )
        ]
    token = "0x" + f"{vault[3]:040x}"

    observations = [
        checked_value("cold-fractional-vault-nft", vault[0] == int(nft, 16), True),
        checked_value("cold-fractional-vault-token-id", vault[1], 1),
        checked_value("cold-fractional-vault-supply", vault[2], 1000),
        checked_value("cold-fractional-share-created", vault[3] != 0, True),
    ]
    nft_owner, error = read_address(nft, "ownerOf(uint256)(address)", ("1",), rpc_url)
    observations.append(
        runtime_error("cold-fractional-nft-custody", error)
        if nft_owner is None
        else checked_value(
            "cold-fractional-nft-custody", nft_owner == address.lower(), True
        )
    )
    share_balance, error = read_uint(
        token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url
    )
    observations.append(
        runtime_error("cold-fractional-share-minted", error)
        if share_balance is None
        else checked_value("cold-fractional-share-minted", share_balance, 1000)
    )

    _, error = call_contract(
        token, "approve(address,uint256)", (address, MAX_UINT256), rpc_url, private_key
    )
    if error:
        observations.append(runtime_error("cold-fractional-approve-share", error))
        return observations
    _, error = call_contract(address, "join(uint256)", ("1",), rpc_url, private_key)
    if error:
        observations.append(runtime_error("cold-fractional-join", error))
        return observations

    empty_vault, _, error = eth_call_raw(address, "getVault(uint256)", ("1",), rpc_url)
    if empty_vault is None:
        observations.append(
            runtime_error("cold-fractional-vault-cleared", error or "getVault reverted")
        )
    else:
        observations.append(
            checked_value(
                "cold-fractional-vault-cleared",
                all(word == 0 for word in decode_words(empty_vault)),
                True,
            )
        )
    nft_owner, error = read_address(nft, "ownerOf(uint256)(address)", ("1",), rpc_url)
    observations.append(
        runtime_error("cold-fractional-nft-returned", error)
        if nft_owner is None
        else checked_value(
            "cold-fractional-nft-returned", nft_owner, DEFAULT_SENDER.lower()
        )
    )
    share_balance, error = read_uint(
        token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url
    )
    observations.append(
        runtime_error("cold-fractional-share-burned", error)
        if share_balance is None
        else checked_value("cold-fractional-share-burned", share_balance, 0)
    )
    return observations


def keccak256(data: bytes) -> bytes:
    proc = run(["cast", "keccak", "0x" + data.hex()], timeout=30)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr[:1000])
    return bytes.fromhex(proc.stdout.strip().removeprefix("0x"))


@cache
def nitro_dispatch_vector(opcode: int) -> tuple[str, str]:
    zero = bytes(32)
    u32_zero = bytes(4)
    u64_zero = bytes(8)
    u256_zero = bytes(32)
    instruction = opcode.to_bytes(2, "big") + u256_zero
    instructions_hash = keccak256(b"Instructions:" + b"\x01" + instruction)
    functions_root = keccak256(b"Function:" + instructions_hash)
    memory_hash = keccak256(b"Memory:" + u64_zero + u64_zero + zero)
    module = zero + u64_zero + u64_zero + zero + zero + functions_root + zero + u32_zero
    module_hash = keccak256(
        b"Module:" + zero + memory_hash + zero + functions_root + zero + u32_zero
    )

    inactive_multi = zero + zero
    multistack_hash = keccak256(b"multistack:" + zero + zero + zero)
    recovery_pc = bytes([0xFF]) * 32
    machine = (
        b"\x00"
        + zero
        + u256_zero
        + inactive_multi
        + zero
        + u256_zero
        + zero
        + b"\x00"
        + inactive_multi
        + zero
        + u32_zero
        + u32_zero
        + u32_zero
        + recovery_pc
        + module_hash
    )
    before_hash = keccak256(
        b"Machine running:"
        + multistack_hash
        + zero
        + multistack_hash
        + zero
        + u32_zero
        + u32_zero
        + u32_zero
        + recovery_pc
        + module_hash
    )
    proof = machine + module + b"\x00" + b"\x01" + instruction + b"\x00\x00"
    return "0x" + before_hash.hex(), "0x" + proof.hex()


def run_nitro_cold_paths(address: str, rpc_url: str) -> list[dict[str, object]]:
    dispatches = (
        ("prover0", 0x01, DEFAULT_SENDER, 1),
        ("prover-mem", 0x28, DEFAULT_SPENDER, 2),
        ("prover-math", 0x6A, DEFAULT_THIRD, 3),
        ("prover-host-io", 0x8010, DEFAULT_FOURTH, 4),
    )
    observations = []
    try:
        for _, _, prover, marker in dispatches:
            code = f"0x60{marker:02x}60005260206000fd"
            _, error = rpc_request("anvil_setCode", (prover, code), rpc_url)
            if error:
                return [runtime_error("cold-nitro-install-provers", error)]

        for label, opcode, _, marker in dispatches:
            try:
                before_hash, proof = nitro_dispatch_vector(opcode)
            except RuntimeError as exc:
                observations.append(runtime_error(f"cold-nitro-{label}", str(exc)))
                continue
            _, revert_data, error = eth_call_raw(
                address,
                "proveOneStep((uint256,address,bytes32),uint256,bytes32,bytes)",
                (f"(1000000,{ZERO_ADDRESS},0x{'00' * 32})", "0", before_hash, proof),
                rpc_url,
            )
            expected = f"0x{marker:064x}"
            if error:
                observations.append(runtime_error(f"cold-nitro-{label}", error))
            elif revert_data is None:
                observations.append(
                    runtime_error(f"cold-nitro-{label}", "expected mock-prover revert")
                )
            else:
                observations.append(
                    checked_value(f"cold-nitro-{label}", revert_data, expected)
                )
    finally:
        for _, _, prover, _ in dispatches:
            rpc_request("anvil_setCode", (prover, "0x"), rpc_url)
    return observations


def run_cold_path_checks(
    test_case: TestCase,
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> list[dict[str, object]]:
    if test_case.test_id == "openzeppelin-vesting-wallet":
        return run_vesting_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "lilweb3-fractional":
        return run_fractional_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "nitro-one-step-proof":
        return run_nitro_cold_paths(address, rpc_url)
    return []


def compare_runtime_results(
    entry: dict[str, object], specs: Sequence[CompilerSpec]
) -> None:
    labels = []
    values_by_compiler: dict[str, dict[str, str]] = {}
    failed = False

    for spec in specs:
        data = entry["compilers"].get(spec.compiler_id, {})
        check_results = data.get("runtime_results") or []
        if data.get("runtime_status") == "failed":
            failed = True
        values_by_compiler[spec.compiler_id] = {
            str(result.get("label")): str(result.get("value"))
            for result in check_results
            if result.get("status") == "ok"
        }
        for result in check_results:
            label = str(result.get("label"))
            if label not in labels:
                labels.append(label)

    if not labels:
        entry["runtime_status"] = "skipped"
        return
    if len(specs) < 2:
        entry["runtime_status"] = "failed" if failed else "skipped"
        entry["runtime_mismatches"] = []
        return

    mismatches = []
    for label in labels:
        values = {
            spec.compiler_id: values_by_compiler.get(spec.compiler_id, {}).get(label)
            for spec in specs
        }
        if any(value is None for value in values.values()):
            failed = True
            continue
        unique_values = set(values.values())
        if len(unique_values) > 1:
            mismatches.append({"label": label, "values": values})

    entry["runtime_mismatches"] = mismatches
    if mismatches:
        entry["runtime_status"] = "mismatch"
    elif failed:
        entry["runtime_status"] = "failed"
    else:
        entry["runtime_status"] = "ok"


def result_key(result: dict[str, object]) -> tuple[str, str]:
    return str(result.get("suite", "repository")), str(result.get("test_id", ""))


def load_reference_results(path: Path) -> dict[tuple[str, str], dict[str, object]]:
    document = json.loads(path.read_text())
    results = document.get("results") if isinstance(document, dict) else None
    if not isinstance(results, list):
        raise ValueError("expected a benchmark result document")
    return {
        result_key(result): result for result in results if isinstance(result, dict)
    }


def workload_signature(data: dict[str, object]) -> tuple[object, ...]:
    signature = []
    for field in ("gas_results", "runtime_results"):
        observations = data.get(field)
        if not isinstance(observations, list):
            continue
        signature.append(
            (
                field,
                tuple(
                    (
                        observation.get("label"),
                        observation.get("call"),
                        tuple(observation.get("args") or ()),
                    )
                    for observation in observations
                    if isinstance(observation, dict)
                ),
            )
        )
    return tuple(signature)


def merge_reference_compiler(
    entry: dict[str, object],
    references: dict[tuple[str, str], dict[str, object]],
    compiler_id: str,
) -> bool:
    reference = references.get(result_key(entry))
    if reference is None:
        return False
    reference_compilers = reference.get("compilers")
    if not isinstance(reference_compilers, dict):
        return False
    reference_data = reference_compilers.get(compiler_id)
    compilers = entry.get("compilers") or {}
    if not isinstance(reference_data, dict) or not isinstance(compilers, dict):
        return False

    reference_fingerprint = reference_data.get("input_fingerprint")
    current_fingerprints = {
        data.get("input_fingerprint")
        for data in compilers.values()
        if isinstance(data, dict) and data.get("input_fingerprint")
    }
    if not reference_fingerprint or reference_fingerprint not in current_fingerprints:
        return False
    if entry.get("gas_profile") != reference.get("gas_profile"):
        return False
    if not any(
        workload_signature(data) == workload_signature(reference_data)
        for data in compilers.values()
        if isinstance(data, dict)
    ):
        return False

    entry["compilers"] = {
        compiler_id: copy.deepcopy(reference_data),
        **compilers,
    }
    return True


def failed_test_result(
    test_case: TestCase,
    specs: Sequence[CompilerSpec],
    gas_profile: str,
    error: Exception,
) -> dict[str, object]:
    message = f"unexpected benchmark failure: {type(error).__name__}: {error}"[:1000]
    entry: dict[str, object] = {
        "test_id": test_case.test_id,
        "description": test_case.description,
        "contract_name": test_case.contract_name,
        "suite": test_case.suite,
        "gas_profile": gas_profile,
        "benchmark_error": message,
        "compilers": {
            spec.compiler_id: {"status": "failed", "error": message} for spec in specs
        },
    }
    if test_case.project_file is not None:
        entry["project"] = test_case.project
        entry["source"] = test_case.source
    return entry


def run_test_case(
    test_case: TestCase,
    specs: Sequence[CompilerSpec],
    include_gas: bool,
    gas_profile: str,
    rpc_url: str,
    private_key: str,
    verbose: bool = False,
    compile_repeats: int = 1,
    evm_version: str | None = None,
    reference_solc_path: Path | None = None,
    repeat_long_compiles: bool = False,
    artifact_root: Path | None = None,
) -> dict[str, object]:
    entry: dict[str, object] = {
        "test_id": test_case.test_id,
        "description": test_case.description,
        "contract_name": test_case.contract_name,
        "suite": test_case.suite,
        "gas_profile": gas_profile,
        "compilers": {},
    }
    if test_case.project_file is not None:
        entry["project"] = test_case.project
        entry["source"] = test_case.source
    reference_solc = next(
        (spec.path for spec in specs if spec.kind == "solc"), reference_solc_path
    )
    prepared_input = (
        None
        if test_case.project_file is not None and not test_case.project_path.exists()
        else compiler_input(test_case, evm_version)
    )
    for spec in specs:
        verbose_log(verbose, f"[{test_case.test_id}] compiling with {spec.compiler_id}")
        compiled = compile_case(
            spec,
            test_case,
            prepared_input,
            compile_repeats,
            repeat_long_compiles,
        )
        compiler_entry = dict(compiled)
        compiler_entry.pop("bytecode", None)
        compiler_entry.pop("runtime_bytecode", None)
        entry["compilers"][spec.compiler_id] = compiler_entry

        if artifact_root is not None and prepared_input is not None:
            artifact_error = write_artifacts(
                artifact_root, spec, test_case, prepared_input
            )
            if artifact_error:
                compiler_entry["artifact_error"] = artifact_error
                verbose_log(
                    verbose,
                    f"[{test_case.test_id}] {spec.compiler_id} artifact capture failed: {artifact_error}",
                )

        if test_case.whole_project:
            continue

        checks = runtime_checks(test_case)
        calls = gas_calls(test_case, gas_profile)
        has_cold_paths = test_case.test_id in {
            "openzeppelin-vesting-wallet",
            "nitro-one-step-proof",
            "lilweb3-fractional",
        }
        if compiled["status"] != "ok" or not include_gas:
            continue
        if not calls and not checks and not has_cold_paths:
            compiler_entry["deploy_status"] = "skipped"
            compiler_entry["runtime_status"] = "skipped"
            continue

        verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} deploy")
        address, deploy_gas, deploy_error = deploy_contract(
            str(compiled["bytecode"]),
            test_case,
            rpc_url,
            private_key,
        )
        if not address:
            compiler_entry["deploy_status"] = "failed"
            compiler_entry["runtime_status"] = "failed" if checks else "skipped"
            compiler_entry["deploy_error"] = deploy_error
            continue

        compiler_entry["deploy_status"] = "ok"
        compiler_entry["deploy_gas"] = deploy_gas
        compiler_entry["address"] = address
        gas_results = []
        total_gas = 0
        gas_failed = False
        for call in calls:
            for index in range(call.repeat):
                label = call.label if call.repeat == 1 else f"{call.label}#{index + 1}"
                verbose_log(
                    verbose,
                    f"[{test_case.test_id}] {spec.compiler_id} tx {label}: {call.signature}",
                )
                gas, error = call_contract(
                    address, call.signature, call.args, rpc_url, private_key
                )
                if gas is None:
                    gas_failed = True
                    gas_results.append(
                        {
                            "label": label,
                            "call": call.signature,
                            "args": list(call.args),
                            "gas": None,
                            "error": error,
                        }
                    )
                    continue
                gas_results.append(
                    {
                        "label": label,
                        "call": call.signature,
                        "args": list(call.args),
                        "gas": gas,
                    }
                )
                total_gas += gas
        compiler_entry["gas_results"] = gas_results
        compiler_entry["gas_status"] = "failed" if gas_failed else "ok"
        compiler_entry["total_gas"] = None if gas_failed else total_gas

        runtime_results = []
        runtime_failed = False
        for check in checks:
            verbose_log(
                verbose,
                f"[{test_case.test_id}] {spec.compiler_id} read {check.label}: {check.signature}",
            )
            value, error = read_contract(address, check.signature, check.args, rpc_url)
            if value is None:
                runtime_failed = True
                runtime_results.append(
                    {
                        "label": check.label,
                        "call": check.signature,
                        "args": list(check.args),
                        "status": "failed",
                        "error": error,
                    }
                )
                continue
            runtime_results.append(
                {
                    "label": check.label,
                    "call": check.signature,
                    "args": list(check.args),
                    "status": "ok",
                    "value": value,
                }
            )
        if has_cold_paths:
            if reference_solc is None:
                cold_results = [
                    runtime_error("cold-path-setup", "reference solc is required")
                ]
            else:
                verbose_log(
                    verbose,
                    f"[{test_case.test_id}] {spec.compiler_id} cold-path differential",
                )
                cold_results = run_cold_path_checks(
                    test_case,
                    address,
                    reference_solc,
                    rpc_url,
                    private_key,
                )
            runtime_results.extend(cold_results)
            runtime_failed |= any(
                result.get("status") != "ok" for result in cold_results
            )
        if checks or has_cold_paths:
            compiler_entry["runtime_results"] = runtime_results
            compiler_entry["runtime_status"] = "failed" if runtime_failed else "ok"

    if include_gas:
        compare_runtime_results(entry, specs)

    return entry


def select_tests(modes: Sequence[str], suite: str) -> Sequence[TestCase]:
    runtime = "runtime" in modes
    compile_time = "compile-time" in modes
    return [
        test
        for test in TEST_CASES
        if (suite == "all" or test.suite == suite)
        and (
            (test.whole_project and compile_time)
            or (not test.whole_project and runtime)
        )
    ]


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Benchmark solc vs Solar codegen on inline and repository contracts"
    )
    parser.add_argument(
        "--solc", default="solc", help="Path to solc binary (default: solc)"
    )
    parser.add_argument(
        "--solar",
        help="Path to solar binary (default: solar or target/{release,debug}/solar)",
    )
    parser.add_argument(
        "--mode",
        choices=("runtime", "compile-time"),
        nargs="+",
        default=("runtime",),
        help="Benchmark modes to run (default: runtime)",
    )
    parser.add_argument(
        "--suite",
        choices=("micro", "repository", "large", "heavy", "all"),
        default="all",
        help="Subset of the selected modes to run (default: all)",
    )
    parser.add_argument(
        "--compile-repeats",
        type=int,
        default=1,
        help="Compile each test this many times and record the median time (default: 1)",
    )
    parser.add_argument(
        "--repeat-long-compiles",
        action="store_true",
        help="Do not stop repeats after a compile takes at least 10 seconds",
    )
    parser.add_argument(
        "--evm-version",
        help="Override Standard JSON `evmVersion` for every benchmark case",
    )
    parser.add_argument(
        "--solar-only",
        action="store_true",
        help="Benchmark Solar without compiling each case with solc",
    )
    parser.add_argument(
        "--reference-results",
        type=Path,
        help="Reuse matching solc results from another benchmark result document",
    )
    parser.add_argument("--tests", nargs="*", help="Subset of test IDs to run")
    parser.add_argument(
        "--projects", nargs="*", help="Subset of repository project names to run"
    )
    parser.add_argument(
        "--list-tests", action="store_true", help="List available tests and exit"
    )
    parser.add_argument(
        "--include-incompatible",
        action="store_true",
        help="Run repository contracts even when their pragma is incompatible with the selected solc",
    )
    parser.add_argument(
        "--gas",
        action="store_true",
        help="Deploy, execute gas calls, and compare runtime results with cast/anvil",
    )
    parser.add_argument(
        "--gas-profile",
        choices=("smoke", "hot"),
        default="smoke",
        help="Gas workload profile: smoke preserves the existing calls, hot runs a broader optimizer workload",
    )
    parser.add_argument(
        "--start-anvil", action="store_true", help="Start anvil automatically for --gas"
    )
    parser.add_argument(
        "--rpc-url",
        default=DEFAULT_RPC_URL,
        help=f"RPC URL (default: {DEFAULT_RPC_URL})",
    )
    parser.add_argument(
        "--private-key",
        default=DEFAULT_PRIVATE_KEY,
        help="Private key for transactions",
    )
    parser.add_argument("--output", help="Output JSON path")
    parser.add_argument(
        "--artifacts",
        type=Path,
        help="Write per-benchmark compiler inputs, outputs, IR, disassembly, and bytecode",
    )
    parser.add_argument(
        "--verbose", action="store_true", help="Print compiler errors for failed rows"
    )
    parser.add_argument(
        "--allow-failures",
        action="store_true",
        help="Exit successfully even if a compiler fails for one or more tests",
    )
    args = parser.parse_args(argv)

    if args.reference_results and not args.solar_only:
        parser.error("--reference-results requires --solar-only")
    try:
        reference_results = (
            load_reference_results(args.reference_results)
            if args.reference_results
            else {}
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        parser.error(f"failed to load reference results: {exc}")

    suite_tests = select_tests(args.mode, args.suite)

    if args.projects:
        project_set = set(args.projects)
        suite_tests = [test for test in suite_tests if test.project in project_set]

    test_map = {test.test_id: test for test in suite_tests}
    if args.list_tests:
        for test in suite_tests:
            if test.project_file is not None:
                print(
                    f"{test.test_id}	{test.project}	{test.source}	{test.contract_name}"
                )
            else:
                print(
                    f"{test.test_id}	{test.suite}	inline	{test.contract_name}"
                )
        return 0

    solc = find_binary(args.solc, ["solc"])
    if not solc and (not args.solar_only or args.reference_results):
        print(_color(f"solc not found: {args.solc}", RED), file=sys.stderr)
        return 1

    solar = find_binary(
        args.solar,
        [
            "solar",
            "target/release/solar",
            "target/debug/solar",
        ],
    )
    if not solar:
        print(
            _color(
                "solar binary not found; build Solar or pass --solar /path/to/solar",
                RED,
            ),
            file=sys.stderr,
        )
        return 1

    if args.gas and not check_tool("cast"):
        print(
            _color("cast not found; install Foundry or omit --gas", RED),
            file=sys.stderr,
        )
        return 1

    use_reference_solc = bool(args.reference_results)
    solc_version, solc_version_error = (
        binary_version(solc)
        if solc and (not args.solar_only or use_reference_solc)
        else ("unavailable", "")
    )
    solar_version, solar_version_error = binary_version(solar)

    specs = []
    if not args.solar_only:
        assert solc is not None
        specs.append(CompilerSpec("solc", f"solc {solc_version}", solc, "solc"))
    specs.append(CompilerSpec("solar", f"solar {solar_version}", solar, "solar"))
    reference_solc_spec = (
        CompilerSpec("solc", f"solc {solc_version}", solc, "solc")
        if use_reference_solc and solc is not None
        else None
    )

    if args.tests:
        missing = [test_id for test_id in args.tests if test_id not in test_map]
        if missing:
            print(
                _color(f"unknown test IDs: {', '.join(missing)}", RED), file=sys.stderr
            )
            return 1
        tests = [test_map[test_id] for test_id in args.tests]
    else:
        tests = list(suite_tests)

    skipped = []
    if (not args.solar_only or use_reference_solc) and not args.include_incompatible:
        compatible_tests = []
        for test in tests:
            if test.project_file is not None and not version_in_range(
                solc_version, test.min_solc, test.max_solc
            ):
                skipped.append(test)
            else:
                compatible_tests.append(test)
        tests = compatible_tests

    if args.gas and any(
        test.project_file is not None
        and not test.whole_project
        and not gas_calls(test, args.gas_profile)
        and not runtime_checks(test)
        for test in tests
    ):
        print(
            _color(
                "repository contracts without gas calls or runtime checks will show N/A in the gas table",
                YELLOW,
            ),
            file=sys.stderr,
        )

    if args.gas and args.start_anvil and not check_tool("anvil"):
        print(
            _color("anvil not found; install Foundry or omit --start-anvil", RED),
            file=sys.stderr,
        )
        return 1

    for spec in specs:
        print(f"Using {spec.label}")
    if args.reference_results:
        print(f"Reusing solc results from {display_path(args.reference_results)}")
    if (not args.solar_only or use_reference_solc) and solc_version_error:
        print(
            _color(
                "Warning: `solc --version` failed. If this is solc-select, run "
                "`solc-select install 0.8.30 && solc-select use 0.8.30` or pass --solc /path/to/solc.",
                YELLOW,
            ),
            file=sys.stderr,
        )
    if solar_version_error:
        print(
            _color(f"Warning: `solar --version` failed: {solar_version_error}", YELLOW),
            file=sys.stderr,
        )
    if skipped:
        skipped_ids = ", ".join(test.test_id for test in skipped)
        print(
            _color(
                f"Skipping {len(skipped)} incompatible tests for solc {solc_version}: {skipped_ids}",
                YELLOW,
            )
        )
    if args.evm_version:
        print(f"Forcing EVM version {args.evm_version}")
    print(f"Running {len(tests)} tests")

    results = []
    timings = {}
    anvil_proc = None
    try:
        if args.gas and args.start_anvil:
            print("Starting anvil...")
            try:
                anvil_proc = start_anvil(args.rpc_url)
            except Exception as exc:
                print(_color(f"Benchmark setup failed: {exc}", RED), file=sys.stderr)
                results.extend(
                    failed_test_result(test, specs, args.gas_profile, exc)
                    for test in tests
                )
                tests = []
        current_suite = None
        suite_started = None
        for test in tests:
            suite = test.suite
            if suite != current_suite:
                if current_suite is not None and suite_started is not None:
                    timings[current_suite] = (
                        timings.get(current_suite, 0.0)
                        + time.monotonic()
                        - suite_started
                    )
                current_suite = suite
                suite_started = time.monotonic()
            try:
                result = run_test_case(
                    test,
                    specs,
                    args.gas,
                    args.gas_profile,
                    args.rpc_url,
                    args.private_key,
                    args.verbose,
                    args.compile_repeats,
                    args.evm_version,
                    solc,
                    args.repeat_long_compiles,
                    args.artifacts,
                )
            except Exception as exc:
                print(
                    _color(
                        f"[{test.test_id}] Unexpected benchmark failure: {exc}", RED
                    ),
                    file=sys.stderr,
                )
                result = failed_test_result(test, specs, args.gas_profile, exc)
            if reference_solc_spec:
                if merge_reference_compiler(result, reference_results, "solc"):
                    if (
                        args.artifacts is not None
                        and args.reference_results is not None
                    ):
                        source = (
                            args.reference_results.parent
                            / "artifacts"
                            / test.test_id
                            / "solc"
                        )
                        if source.is_dir():
                            shutil.copytree(
                                source,
                                args.artifacts / test.test_id / "solc",
                                dirs_exist_ok=True,
                            )
                    if args.gas:
                        compare_runtime_results(result, (reference_solc_spec, *specs))
                else:
                    print(
                        _color(
                            f"[{test.test_id}] matching solc reference result not found",
                            YELLOW,
                        ),
                        file=sys.stderr,
                    )
            results.append(result)
        if current_suite is not None and suite_started is not None:
            timings[current_suite] = (
                timings.get(current_suite, 0.0) + time.monotonic() - suite_started
            )
    finally:
        if anvil_proc:
            print("Stopping anvil...")
            try:
                stop_anvil(anvil_proc)
            except Exception as exc:
                print(_color(f"Failed to stop anvil: {exc}", RED), file=sys.stderr)

    if args.verbose:
        for result in results:
            for compiler_id, data in result["compilers"].items():
                if data.get("status") != "ok":
                    print(
                        f"\n{result['test_id']} {compiler_id} error:\n{data.get('error', '')}"
                    )
                for check in data.get("runtime_results") or []:
                    if check.get("status") != "ok":
                        print(
                            f"\n{result['test_id']} {compiler_id} runtime {check.get('label')} error:\n"
                            f"{check.get('error', '')}"
                        )
                for gas_result in data.get("gas_results") or []:
                    if gas_result.get("gas") is None:
                        print(
                            f"\n{result['test_id']} {compiler_id} tx {gas_result.get('label')} error:\n"
                            f"{gas_result.get('error', '')}"
                        )
            for mismatch in result.get("runtime_mismatches") or []:
                values = ", ".join(
                    f"{compiler_id}={value}"
                    for compiler_id, value in mismatch.get("values", {}).items()
                )
                print(
                    f"\n{result['test_id']} runtime mismatch {mismatch.get('label')}: {values}"
                )

    RESULT_ROOT.mkdir(parents=True, exist_ok=True)
    output = Path(args.output) if args.output else RESULT_ROOT / "solar_latest.json"
    document = {
        "format_version": 1,
        "evm_version_override": args.evm_version,
        "timings": timings,
        "results": results,
    }
    output.write_text(json.dumps(document, indent=2) + "\n")
    print(f"\nResults saved to {display_path(output)}")

    failed = [
        (result["test_id"], compiler_id)
        for result in results
        for compiler_id, data in result["compilers"].items()
        if data.get("status") != "ok"
    ]
    runtime_failed = [
        result["test_id"]
        for result in results
        if result.get("runtime_status") in ("failed", "mismatch")
    ]
    gas_failed = [
        (result["test_id"], compiler_id)
        for result in results
        for compiler_id, data in result["compilers"].items()
        if data.get("gas_status") == "failed"
    ]
    if (failed or runtime_failed or gas_failed) and not args.allow_failures:
        if failed:
            print(
                _color(
                    f"{len(failed)} compiler runs failed; use --allow-failures to keep exit code 0",
                    RED,
                ),
                file=sys.stderr,
            )
        if runtime_failed:
            print(
                _color(
                    f"{len(runtime_failed)} runtime checks failed; use --allow-failures to keep exit code 0",
                    RED,
                ),
                file=sys.stderr,
            )
        if gas_failed:
            print(
                _color(
                    f"{len(gas_failed)} gas transaction runs failed; use --allow-failures to keep exit code 0",
                    RED,
                ),
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
