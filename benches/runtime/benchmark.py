#!/usr/bin/env python3
"""Compare solc and Solar codegen on the curated runtime corpus."""

# Adapted from walnuthq/solidity-compiler-benchmarks at
# 01209d2b8ac81645b92e3ef801b5bcdfd61bfd69 under Apache-2.0.

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Dict, Optional, Sequence, Tuple

from cases import (
    DEFAULT_FOURTH,
    DEFAULT_SENDER,
    DEFAULT_SPENDER,
    DEFAULT_THIRD,
    EDGE_BYTES32,
    MAX_UINT128,
    MAX_UINT256,
    MIXED_BYTES32,
    SIGNED_HASH,
    TEST_CASES,
    TestCase,
    ZERO_ADDRESS,
    gas_calls,
    runtime_checks,
)
from common import (
    DEFAULT_PRIVATE_KEY,
    DEFAULT_RPC_URL,
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


def verbose_log(enabled: bool, message: str) -> None:
    if enabled:
        print(message, flush=True)


def find_binary(explicit: Optional[str], candidates: Sequence[str]) -> Optional[Path]:
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


def binary_version(path: Path) -> Tuple[str, str]:
    result = run([str(path), "--version"], timeout=30)
    if result.returncode != 0:
        error = (result.stderr or result.stdout or "version command failed").strip()
        return "unavailable", error[:500]
    text = (result.stdout + "\n" + result.stderr).strip()
    match = re.search(r"(\d+\.\d+\.\d+(?:[-+][^\s]+)?)", text)
    version = match.group(1) if match else text.splitlines()[0] if text else "unknown"
    return version, ""


def parse_version_tuple(version: str) -> Optional[Tuple[int, int, int]]:
    match = re.match(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def version_in_range(version: str, minimum: Optional[str], maximum: Optional[str]) -> bool:
    parsed = parse_version_tuple(version)
    if parsed is None:
        return True
    if minimum and parsed < parse_version_tuple(minimum):
        return False
    if maximum and parsed > parse_version_tuple(maximum):
        return False
    return True


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


@lru_cache(maxsize=None)
def project_standard_json_input(project_file: str, source: str, contract_name: str) -> str:
    path = REPOSITORY_ROOT / project_file
    project = load_project(path)
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


def compile_case(spec: CompilerSpec, test_case: TestCase) -> Dict[str, object]:
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
        input_text = project_standard_json_input(
            test_case.project_file, test_case.source, test_case.contract_name
        )
        timeout = 180
    else:
        input_text = standard_json_input(test_case)
        timeout = 120

    cmd = [str(spec.path)]
    if spec.kind != "solc":
        # Solar gates its experimental code generator behind `-Zcodegen`.
        cmd.append("-Zcodegen")
    cmd.append("--standard-json")
    started = time.monotonic()
    proc = run(
        cmd,
        input_text=input_text,
        timeout=timeout,
        measure_peak_rss=True,
    )
    result["compile_time_seconds"] = time.monotonic() - started
    result["peak_rss_bytes"] = proc.peak_rss_bytes
    result["command"] = display_command(cmd)
    if proc.returncode != 0:
        result["status"] = "failed"
        result["error"] = (proc.stderr or proc.stdout or "compiler failed")[:1000]
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


def parse_standard_json_output(
    stdout: str, test_case: TestCase
) -> Tuple[Optional[str], Optional[str], str]:
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
        name
        for source_contracts in contracts.values()
        for name in source_contracts.keys()
    ]
    return None, None, f"contract {test_case.contract_name} not found; available: {', '.join(available)}"




@lru_cache(maxsize=None)
def compile_runtime_fixture(solc_path: str, contract_name: str) -> Tuple[Optional[str], str]:
    source_name = str(RUNTIME_FIXTURES.relative_to(ROOT))
    payload = {
        "language": "Solidity",
        "sources": {source_name: {"content": RUNTIME_FIXTURES.read_text()}},
        "settings": {
            "optimizer": {"enabled": True, "runs": 200},
            "outputSelection": {"*": {contract_name: ["evm.bytecode.object"]}},
        },
    }
    proc = run([solc_path, "--standard-json"], input_text=json.dumps(payload), timeout=120)
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


def abi_encode_constructor(constructor_args: Sequence[str], constructor_sig: Optional[str]) -> Optional[str]:
    if not constructor_args:
        return ""
    if not constructor_sig:
        return None
    proc = run(["cast", "abi-encode", constructor_sig, *constructor_args], timeout=30)
    if proc.returncode != 0:
        return None
    encoded = proc.stdout.strip()
    return encoded[2:] if encoded.startswith("0x") else encoded


def deploy_contract(
    bytecode: str,
    test_case: TestCase,
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[str], Optional[int], str]:
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
    constructor_sig: Optional[str],
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[str], Optional[int], str]:
    if not bytecode.startswith("0x"):
        bytecode = "0x" + bytecode

    encoded = abi_encode_constructor(constructor_args, constructor_sig)
    if encoded is None:
        return None, None, "constructor args require constructor_sig"
    bytecode += encoded

    proc = run(
        [
            "cast",
            "send",
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
            "--create",
            bytecode,
        ],
        timeout=CAST_DEPLOY_TIMEOUT,
    )
    if proc.returncode != 0:
        return None, None, proc.stderr[:1000]
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        return None, None, f"invalid deploy JSON: {exc}"
    status = parse_receipt_int(data.get("status"))
    gas = data.get("gasUsed")
    deploy_gas = parse_receipt_int(gas)
    if status is not None and status != 1:
        return None, deploy_gas, f"deploy transaction failed (status={status}, gasUsed={deploy_gas})"
    if deploy_gas is None:
        return None, None, "deploy receipt missing gasUsed"
    return data.get("contractAddress"), deploy_gas, ""


def call_contract(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
    private_key: str,
) -> Tuple[Optional[int], str]:
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
) -> Tuple[Optional[str], str]:
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


def rpc_request(method: str, params: Sequence[object], rpc_url: str) -> Tuple[Optional[object], str]:
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


def encode_calldata(signature: str, args: Sequence[str]) -> Tuple[Optional[str], str]:
    proc = run(["cast", "calldata", signature, *args], timeout=30)
    if proc.returncode != 0:
        return None, proc.stderr[:1000]
    return proc.stdout.strip(), ""


def eth_call_raw(
    address: str,
    signature: str,
    args: Sequence[str],
    rpc_url: str,
) -> Tuple[Optional[str], Optional[str], str]:
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


def runtime_ok(label: str, value: object) -> Dict[str, object]:
    return {"label": label, "status": "ok", "value": str(value)}


def runtime_error(label: str, error: str) -> Dict[str, object]:
    return {"label": label, "status": "failed", "error": error}


def checked_value(label: str, actual: object, expected: object) -> Dict[str, object]:
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
) -> Tuple[Optional[int], str]:
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
) -> Tuple[Optional[str], str]:
    value, error = read_contract(address, signature, args, rpc_url)
    if value is None:
        return None, error
    match = re.search(r"0x[0-9a-fA-F]{40}", value)
    return (match.group(0).lower(), "") if match else (None, f"invalid address result: {value}")


def decode_words(data: str) -> List[int]:
    raw = data[2:] if data.startswith("0x") else data
    if len(raw) % 64 != 0:
        raise ValueError(f"ABI result has {len(raw)} hex digits")
    return [int(raw[index:index + 64], 16) for index in range(0, len(raw), 64)]


def run_vesting_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> List[Dict[str, object]]:
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
    _, error = call_contract(address, "release(address)", (token,), rpc_url, private_key)
    if error:
        return [runtime_error("cold-vesting-token-release", error)]

    observations = []
    reads = (
        ("cold-vesting-released-eth", address, "released()(uint256)", (), 1000),
        ("cold-vesting-releasable-eth", address, "releasable()(uint256)", (), 0),
        ("cold-vesting-released-token", address, "released(address)(uint256)", (token,), 2000),
        ("cold-vesting-releasable-token", address, "releasable(address)(uint256)", (token,), 0),
        ("cold-vesting-token-empty", token, "balanceOf(address)(uint256)", (address,), 0),
        ("cold-vesting-owner-token", token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), 2000),
    )
    for label, target, signature, args, expected in reads:
        value, error = read_uint(target, signature, args, rpc_url)
        observations.append(runtime_error(label, error) if value is None else checked_value(label, value, expected))
    balance, error = rpc_request("eth_getBalance", (address, "latest"), rpc_url)
    if balance is None:
        observations.append(runtime_error("cold-vesting-eth-empty", error))
    else:
        observations.append(checked_value("cold-vesting-eth-empty", int(str(balance), 16), 0))
    return observations


def run_fractional_cold_paths(
    address: str,
    solc_path: Path,
    rpc_url: str,
    private_key: str,
) -> List[Dict[str, object]]:
    nft_bytecode, error = compile_runtime_fixture(str(solc_path), "RuntimeNFT")
    if nft_bytecode is None:
        return [runtime_error("cold-fractional-nft-compile", error)]
    nft, _, error = deploy_creation_code(nft_bytecode, (), None, rpc_url, private_key)
    if nft is None:
        return [runtime_error("cold-fractional-nft-deploy", error)]
    _, error = call_contract(nft, "setApprovalForAll(address,bool)", (address, "true"), rpc_url, private_key)
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
        return [runtime_error("cold-fractional-vault", f"expected 4 words, got {len(vault)}")]
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
        else checked_value("cold-fractional-nft-custody", nft_owner == address.lower(), True)
    )
    share_balance, error = read_uint(token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url)
    observations.append(
        runtime_error("cold-fractional-share-minted", error)
        if share_balance is None
        else checked_value("cold-fractional-share-minted", share_balance, 1000)
    )

    _, error = call_contract(token, "approve(address,uint256)", (address, MAX_UINT256), rpc_url, private_key)
    if error:
        observations.append(runtime_error("cold-fractional-approve-share", error))
        return observations
    _, error = call_contract(address, "join(uint256)", ("1",), rpc_url, private_key)
    if error:
        observations.append(runtime_error("cold-fractional-join", error))
        return observations

    empty_vault, _, error = eth_call_raw(address, "getVault(uint256)", ("1",), rpc_url)
    if empty_vault is None:
        observations.append(runtime_error("cold-fractional-vault-cleared", error or "getVault reverted"))
    else:
        observations.append(checked_value("cold-fractional-vault-cleared", all(word == 0 for word in decode_words(empty_vault)), True))
    nft_owner, error = read_address(nft, "ownerOf(uint256)(address)", ("1",), rpc_url)
    observations.append(
        runtime_error("cold-fractional-nft-returned", error)
        if nft_owner is None
        else checked_value("cold-fractional-nft-returned", nft_owner, DEFAULT_SENDER.lower())
    )
    share_balance, error = read_uint(token, "balanceOf(address)(uint256)", (DEFAULT_SENDER,), rpc_url)
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


@lru_cache(maxsize=None)
def nitro_dispatch_vector(opcode: int) -> Tuple[str, str]:
    zero = bytes(32)
    u32_zero = bytes(4)
    u64_zero = bytes(8)
    u256_zero = bytes(32)
    instruction = opcode.to_bytes(2, "big") + u256_zero
    instructions_hash = keccak256(b"Instructions:" + b"\x01" + instruction)
    functions_root = keccak256(b"Function:" + instructions_hash)
    memory_hash = keccak256(b"Memory:" + u64_zero + u64_zero + zero)
    module = zero + u64_zero + u64_zero + zero + zero + functions_root + zero + u32_zero
    module_hash = keccak256(b"Module:" + zero + memory_hash + zero + functions_root + zero + u32_zero)

    inactive_multi = zero + zero
    multistack_hash = keccak256(b"multistack:" + zero + zero + zero)
    recovery_pc = bytes([0xff]) * 32
    machine = (
        b"\x00"
        + zero + u256_zero
        + inactive_multi
        + zero + u256_zero
        + zero + b"\x00"
        + inactive_multi
        + zero
        + u32_zero + u32_zero + u32_zero
        + recovery_pc
        + module_hash
    )
    before_hash = keccak256(
        b"Machine running:"
        + multistack_hash
        + zero
        + multistack_hash
        + zero
        + u32_zero + u32_zero + u32_zero
        + recovery_pc
        + module_hash
    )
    proof = machine + module + b"\x00" + b"\x01" + instruction + b"\x00\x00"
    return "0x" + before_hash.hex(), "0x" + proof.hex()


def run_nitro_cold_paths(address: str, rpc_url: str) -> List[Dict[str, object]]:
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
                observations.append(runtime_error(f"cold-nitro-{label}", "expected mock-prover revert"))
            else:
                observations.append(checked_value(f"cold-nitro-{label}", revert_data, expected))
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
) -> List[Dict[str, object]]:
    if test_case.test_id == "openzeppelin-vesting-wallet":
        return run_vesting_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "lilweb3-fractional":
        return run_fractional_cold_paths(address, solc_path, rpc_url, private_key)
    if test_case.test_id == "nitro-one-step-proof":
        return run_nitro_cold_paths(address, rpc_url)
    return []


def compare_runtime_results(entry: Dict[str, object], specs: Sequence[CompilerSpec]) -> None:
    labels = []
    values_by_compiler: Dict[str, Dict[str, str]] = {}
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


def run_test_case(
    test_case: TestCase,
    specs: Sequence[CompilerSpec],
    include_gas: bool,
    gas_profile: str,
    rpc_url: str,
    private_key: str,
    verbose: bool = False,
) -> Dict[str, object]:
    entry: Dict[str, object] = {
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
    reference_solc = next((spec for spec in specs if spec.kind == "solc"), None)

    for spec in specs:
        verbose_log(verbose, f"[{test_case.test_id}] compiling with {spec.compiler_id}")
        compiled = compile_case(spec, test_case)
        compiler_entry = dict(compiled)
        compiler_entry.pop("bytecode", None)
        compiler_entry.pop("runtime_bytecode", None)
        entry["compilers"][spec.compiler_id] = compiler_entry

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
                verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} tx {label}: {call.signature}")
                gas, error = call_contract(address, call.signature, call.args, rpc_url, private_key)
                if gas is None:
                    gas_failed = True
                    gas_results.append({
                        "label": label,
                        "call": call.signature,
                        "args": list(call.args),
                        "gas": None,
                        "error": error,
                    })
                    continue
                gas_results.append({
                    "label": label,
                    "call": call.signature,
                    "args": list(call.args),
                    "gas": gas,
                })
                total_gas += gas
        compiler_entry["gas_results"] = gas_results
        compiler_entry["gas_status"] = "failed" if gas_failed else "ok"
        compiler_entry["total_gas"] = None if gas_failed else total_gas

        runtime_results = []
        runtime_failed = False
        for check in checks:
            verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} read {check.label}: {check.signature}")
            value, error = read_contract(address, check.signature, check.args, rpc_url)
            if value is None:
                runtime_failed = True
                runtime_results.append({
                    "label": check.label,
                    "call": check.signature,
                    "args": list(check.args),
                    "status": "failed",
                    "error": error,
                })
                continue
            runtime_results.append({
                "label": check.label,
                "call": check.signature,
                "args": list(check.args),
                "status": "ok",
                "value": value,
            })
        if has_cold_paths:
            if reference_solc is None:
                cold_results = [runtime_error("cold-path-setup", "reference solc is required")]
            else:
                verbose_log(verbose, f"[{test_case.test_id}] {spec.compiler_id} cold-path differential")
                cold_results = run_cold_path_checks(
                    test_case,
                    address,
                    reference_solc.path,
                    rpc_url,
                    private_key,
                )
            runtime_results.extend(cold_results)
            runtime_failed |= any(result.get("status") != "ok" for result in cold_results)
        if checks or has_cold_paths:
            compiler_entry["runtime_results"] = runtime_results
            compiler_entry["runtime_status"] = "failed" if runtime_failed else "ok"

    if include_gas:
        compare_runtime_results(entry, specs)

    return entry


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="Benchmark solc vs Solar codegen on inline and repository contracts"
    )
    parser.add_argument("--solc", default="solc", help="Path to solc binary (default: solc)")
    parser.add_argument(
        "--solar",
        help="Path to solar binary (default: solar or target/{release,debug}/solar)",
    )
    parser.add_argument(
        "--suite",
        choices=("micro", "repository", "large", "all"),
        default="micro",
        help="Benchmark suite to run",
    )
    parser.add_argument("--tests", nargs="*", help="Subset of test IDs to run")
    parser.add_argument("--projects", nargs="*", help="Subset of repository project names to run")
    parser.add_argument("--list-tests", action="store_true", help="List available tests and exit")
    parser.add_argument(
        "--include-incompatible",
        action="store_true",
        help="Run repository contracts even when their pragma is incompatible with the selected solc",
    )
    parser.add_argument("--gas", action="store_true", help="Deploy, execute gas calls, and compare runtime results with cast/anvil")
    parser.add_argument(
        "--gas-profile",
        choices=("smoke", "hot"),
        default="smoke",
        help="Gas workload profile: smoke preserves the existing calls, hot runs a broader optimizer workload",
    )
    parser.add_argument("--start-anvil", action="store_true", help="Start anvil automatically for --gas")
    parser.add_argument("--rpc-url", default=DEFAULT_RPC_URL, help=f"RPC URL (default: {DEFAULT_RPC_URL})")
    parser.add_argument("--private-key", default=DEFAULT_PRIVATE_KEY, help="Private key for transactions")
    parser.add_argument("--output", help="Output JSON path")
    parser.add_argument("--verbose", action="store_true", help="Print compiler errors for failed rows")
    parser.add_argument(
        "--allow-failures",
        action="store_true",
        help="Exit successfully even if a compiler fails for one or more tests",
    )
    args = parser.parse_args(argv)

    all_tests = [
        test
        for test in TEST_CASES
        if args.suite == "all" or test.suite == args.suite
    ]

    if args.projects:
        project_set = set(args.projects)
        all_tests = [test for test in all_tests if test.project in project_set]

    test_map = {test.test_id: test for test in all_tests}
    if args.list_tests:
        for test in all_tests:
            if test.project_file is not None:
                print(f"{test.test_id}	{test.project}	{test.source}	{test.contract_name}")
            else:
                print(f"{test.test_id}	{test.suite}	inline	{test.contract_name}")
        return 0

    solc = find_binary(args.solc, ["solc"])
    if not solc:
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
        print(_color("solar binary not found; build Solar or pass --solar /path/to/solar", RED), file=sys.stderr)
        return 1

    if args.gas and not check_tool("cast"):
        print(_color("cast not found; install Foundry or omit --gas", RED), file=sys.stderr)
        return 1

    solc_version, solc_version_error = binary_version(solc)
    solar_version, solar_version_error = binary_version(solar)

    specs = [
        CompilerSpec("solc", f"solc {solc_version}", solc, "solc"),
        CompilerSpec("solar", f"solar {solar_version}", solar, "solar"),
    ]

    if args.tests:
        missing = [test_id for test_id in args.tests if test_id not in test_map]
        if missing:
            print(_color(f"unknown test IDs: {', '.join(missing)}", RED), file=sys.stderr)
            return 1
        tests = [test_map[test_id] for test_id in args.tests]
    else:
        tests = list(all_tests)

    skipped = []
    if not args.include_incompatible:
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
        and not gas_calls(test, args.gas_profile)
        and not runtime_checks(test)
        for test in tests
    ):
        print(_color("repository contracts without gas calls or runtime checks will show N/A in the gas table", YELLOW), file=sys.stderr)

    if args.gas and args.start_anvil and not check_tool("anvil"):
        print(_color("anvil not found; install Foundry or omit --start-anvil", RED), file=sys.stderr)
        return 1

    print(f"Using {specs[0].label}")
    if solc_version_error:
        print(
            _color(
                "Warning: `solc --version` failed. If this is solc-select, run "
                "`solc-select install 0.8.30 && solc-select use 0.8.30` or pass --solc /path/to/solc.",
                YELLOW,
            ),
            file=sys.stderr,
        )
    print(f"Using {specs[1].label}")
    if solar_version_error:
        print(_color(f"Warning: `solar --version` failed: {solar_version_error}", YELLOW), file=sys.stderr)
    if skipped:
        skipped_ids = ", ".join(test.test_id for test in skipped)
        print(_color(f"Skipping {len(skipped)} incompatible tests for solc {solc_version}: {skipped_ids}", YELLOW))
    print(f"Running {len(tests)} tests")

    results = []
    timings = {}
    anvil_proc = None
    try:
        if args.gas and args.start_anvil:
            print("Starting anvil...")
            anvil_proc = start_anvil(args.rpc_url)
        current_suite = None
        suite_started = None
        for test in tests:
            suite = test.suite
            if suite != current_suite:
                if current_suite is not None and suite_started is not None:
                    timings[current_suite] = timings.get(current_suite, 0.0) + time.monotonic() - suite_started
                current_suite = suite
                suite_started = time.monotonic()
            results.append(
                run_test_case(
                    test,
                    specs,
                    args.gas,
                    args.gas_profile,
                    args.rpc_url,
                    args.private_key,
                    args.verbose,
                )
            )
        if current_suite is not None and suite_started is not None:
            timings[current_suite] = timings.get(current_suite, 0.0) + time.monotonic() - suite_started
    finally:
        if anvil_proc:
            print("Stopping anvil...")
            stop_anvil(anvil_proc)

    if args.verbose:
        for result in results:
            for compiler_id, data in result["compilers"].items():
                if data.get("status") != "ok":
                    print(f"\n{result['test_id']} {compiler_id} error:\n{data.get('error', '')}")
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
                print(f"\n{result['test_id']} runtime mismatch {mismatch.get('label')}: {values}")

    RESULT_ROOT.mkdir(parents=True, exist_ok=True)
    output = Path(args.output) if args.output else RESULT_ROOT / "solar_latest.json"
    document = {
        "format_version": 1,
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
                _color(f"{len(failed)} compiler runs failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        if runtime_failed:
            print(
                _color(f"{len(runtime_failed)} runtime checks failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        if gas_failed:
            print(
                _color(f"{len(gas_failed)} gas transaction runs failed; use --allow-failures to keep exit code 0", RED),
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
