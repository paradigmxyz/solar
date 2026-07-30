#!/usr/bin/env python3
"""Benchmark hot calls against large contracts from the project corpus."""

from __future__ import annotations

import argparse
import gzip
import json
import os
import posixpath
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence


DEFAULT_SENDER = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
DEFAULT_RPC_URL = "http://127.0.0.1:8545"
GAS_LIMIT = "80000000"

IMPORT_RE = re.compile(
    r"""\bimport\s+(?:(?:[^;]*?)\s+from\s+)?["']([^"']+)["']"""
    r"""(?:\s+as\s+[A-Za-z_$][A-Za-z0-9_$]*)?\s*;"""
)


@dataclass(frozen=True)
class Call:
    label: str
    signature: str
    args: tuple[str, ...] = ()
    repeat: int = 1


@dataclass(frozen=True)
class Check:
    label: str
    signature: str
    args: tuple[str, ...] = ()


@dataclass(frozen=True)
class Case:
    test_id: str
    description: str
    project: str
    source: str
    contract: str
    calls: tuple[Call, ...]
    checks: tuple[Check, ...] = ()
    constructor_signature: str | None = None
    constructor_args: tuple[str, ...] = ()
    setup_code: tuple[tuple[str, str], ...] = ()


GOVERNOR_HASH_SIGNATURE = "hashProposal(address[],uint256[],bytes[],bytes32)"
GOVERNOR_HASH_EMPTY = ("[]", "[]", "[]", "0x" + "00" * 32)

SIGNER = "0x70997970C51812dc3A010C7D01b50e0d17dc79C8"
SIGNED_HASH = "0x7d768af957ef8cbf6219a37e743d5546d911dae3e46449d8a5810522db2ef65e"
SIGNATURE_CHECK = "isValidSignatureNowCalldata(address,bytes32,bytes)"


CASES = (
    Case(
        test_id="openzeppelin-governor",
        description="OpenZeppelin Governor",
        project="testdata/projects/openzeppelin-5.6.1.json.gz",
        source="test/governance/Governor.t.sol",
        contract="GovernorInternalTest",
        calls=(
            Call("hash-proposal-empty-zero", GOVERNOR_HASH_SIGNATURE, GOVERNOR_HASH_EMPTY, 3),
            Call(
                "hash-proposal-empty-nonzero",
                GOVERNOR_HASH_SIGNATURE,
                ("[]", "[]", "[]", "0x" + "42" * 32),
                3,
            ),
            Call("name", "name()", repeat=3),
            Call("version", "version()", repeat=3),
        ),
        checks=(
            Check("name", "name()(string)"),
            Check("version", "version()(string)"),
            Check(
                "hash-proposal-empty",
                GOVERNOR_HASH_SIGNATURE + "(uint256)",
                GOVERNOR_HASH_EMPTY,
            ),
        ),
    ),
    Case(
        test_id="solady-signature-checker",
        description="Solady SignatureCheckerLib",
        project="testdata/projects/solady-0.1.26.json.gz",
        source="test/SignatureCheckerLib.t.sol",
        contract="SignatureCheckerLibTest",
        calls=(
            Call("empty-signature", SIGNATURE_CHECK, (SIGNER, SIGNED_HASH, "0x"), 3),
            Call("empty-helpers", "testEmptyCalldataHelpers()", repeat=3),
            Call(
                "eth-signed-hash-word",
                "testToEthSignedMessageHashDifferential(bytes32)",
                (SIGNED_HASH,),
                3,
            ),
            Call(
                "eth-signed-hash-bytes",
                "testToEthSignedMessageHashDifferential(bytes)",
                ("0x" + "ab" * 96,),
                3,
            ),
        ),
        checks=(
            Check(
                "empty-signature",
                SIGNATURE_CHECK + "(bool)",
                (SIGNER, SIGNED_HASH, "0x"),
            ),
        ),
    ),
    Case(
        test_id="solady-lib-string",
        description="Solady LibString",
        project="testdata/projects/solady-0.1.26.json.gz",
        source="test/LibString.t.sol",
        contract="LibStringTest",
        calls=(
            Call("serial-number", "checkIsSN(string)", ("123456789",), 3),
            Call("not-serial-number", "checkIsSN(string)", ("12ab",), 3),
            Call(
                "return-string",
                "returnString(string)",
                ("the quick brown fox jumps over the lazy dog",),
                3,
            ),
            Call("small-string", "toSmallString(string)", ("short string",), 3),
            Call("replace-medium", "testStringReplaceMedium()", repeat=3),
            Call("replace-long", "testStringReplaceLong()", repeat=3),
        ),
        checks=(
            Check("serial-number", "checkIsSN(string)(bool)", ("123456789",)),
            Check("not-serial-number", "checkIsSN(string)(bool)", ("12ab",)),
            Check(
                "return-string",
                "returnString(string)(string)",
                ("the quick brown fox jumps over the lazy dog",),
            ),
            Check(
                "small-string",
                "toSmallString(string)(bytes32)",
                ("short string",),
            ),
        ),
    ),
)


def run(
    command: Sequence[str],
    *,
    input_text: str | None = None,
    timeout: int = 180,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )


def remappings(settings: dict[str, Any]) -> list[tuple[str, str]]:
    parsed = []
    for remapping in settings.get("remappings", ()):
        prefix, target = remapping.split("=", 1)
        if ":" in prefix:
            _, prefix = prefix.rsplit(":", 1)
        parsed.append((prefix, target))
    parsed.sort(key=lambda item: len(item[0]), reverse=True)
    return parsed


def resolve_import(
    current: str,
    imported: str,
    sources: dict[str, Any],
    mappings: Sequence[tuple[str, str]],
) -> str:
    if imported.startswith("."):
        candidates = [posixpath.join(posixpath.dirname(current), imported)]
    else:
        candidates = [
            target + imported[len(prefix) :]
            for prefix, target in mappings
            if imported.startswith(prefix)
        ]
        candidates.append(imported)

    for candidate in candidates:
        normalized = posixpath.normpath(candidate)
        if normalized in sources:
            return normalized
    raise ValueError(f"cannot resolve import `{imported}` from `{current}`")


def project_slice(project: dict[str, Any], source: str) -> dict[str, Any]:
    sources = project["sources"]
    mappings = remappings(project.get("settings", {}))
    pending = [source]
    selected = set()

    while pending:
        current = pending.pop()
        if current in selected:
            continue
        if current not in sources:
            raise ValueError(f"project does not contain source `{current}`")
        selected.add(current)
        content = sources[current].get("content")
        if not isinstance(content, str):
            raise ValueError(f"project source `{current}` has no inline content")
        pending.extend(
            resolve_import(current, imported, sources, mappings)
            for imported in IMPORT_RE.findall(content)
        )

    return {name: sources[name] for name in sorted(selected)}


def load_project(path: Path) -> dict[str, Any]:
    with gzip.open(path, mode="rt", encoding="utf-8") as file:
        return json.load(file)


def compiler_version(path: Path) -> str:
    proc = run([str(path), "--version"], timeout=30)
    if proc.returncode != 0:
        return "unknown"
    lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    return lines[-1] if lines else "unknown"


def compile_case(
    compiler_id: str,
    compiler: Path,
    case: Case,
    root: Path,
) -> dict[str, Any]:
    project_path = root / case.project
    project = load_project(project_path)
    settings = dict(project.get("settings", {}))
    settings.pop("compilationTarget", None)
    settings["outputSelection"] = {
        case.source: {
            case.contract: [
                "abi",
                "evm.bytecode.object",
                "evm.deployedBytecode.object",
            ]
        }
    }
    payload = {
        "language": "Solidity",
        "sources": project_slice(project, case.source),
        "settings": settings,
    }
    command = [str(compiler)]
    if compiler_id == "solar":
        command.append("-Zcodegen")
    command.append("--standard-json")

    started = time.monotonic()
    proc = run(command, input_text=json.dumps(payload))
    elapsed = time.monotonic() - started
    result = {
        "compiler_id": compiler_id,
        "label": f"{compiler_id} {compiler_version(compiler)}",
        "status": "failed",
        "bytecode_size": 0,
        "runtime_size": 0,
        "compile_time_seconds": elapsed,
        "command": " ".join(command),
    }
    if proc.returncode != 0:
        result["error"] = (proc.stderr or proc.stdout or "compiler failed")[:2000]
        return result

    try:
        output = json.loads(proc.stdout)
    except json.JSONDecodeError as error:
        result["error"] = f"invalid compiler JSON: {error}"
        return result

    errors = [
        diagnostic.get("formattedMessage")
        or diagnostic.get("message")
        or str(diagnostic)
        for diagnostic in output.get("errors") or ()
        if diagnostic.get("severity") == "error"
    ]
    if errors:
        result["error"] = errors[0][:2000]
        return result

    contract = (
        output.get("contracts", {})
        .get(case.source, {})
        .get(case.contract, {})
    )
    evm = contract.get("evm", {})
    bytecode = evm.get("bytecode", {}).get("object", "")
    runtime = evm.get("deployedBytecode", {}).get("object", "")
    if not bytecode or not runtime:
        result["error"] = f"compiler did not emit `{case.source}:{case.contract}`"
        return result
    if (
        re.fullmatch(r"[0-9a-fA-F]+", bytecode) is None
        or re.fullmatch(r"[0-9a-fA-F]+", runtime) is None
    ):
        result["error"] = "bytecode has unresolved link references"
        return result

    result.update(
        {
            "status": "ok",
            "bytecode": bytecode,
            "runtime_bytecode": runtime,
            "bytecode_size": len(bytecode) // 2,
            "runtime_size": len(runtime) // 2,
        }
    )
    return result


def receipt_int(value: Any) -> int | None:
    if value is None:
        return None
    if isinstance(value, str):
        return int(value, 16) if value.startswith("0x") else int(value)
    return int(value)


def rpc(
    method: str,
    params: Sequence[Any],
    rpc_url: str,
    *,
    timeout: int = 30,
) -> tuple[Any | None, str]:
    proc = run(
        [
            "cast",
            "rpc",
            "--rpc-url",
            rpc_url,
            "--raw",
            method,
        ],
        input_text=json.dumps(list(params)),
        timeout=timeout,
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or f"{method} failed")[:2000]
    try:
        return json.loads(proc.stdout), ""
    except json.JSONDecodeError as error:
        return None, f"invalid `{method}` response: {error}"


def send(
    address: str,
    call: Call,
    rpc_url: str,
    sender: str,
) -> tuple[int | None, str]:
    proc = run(
        [
            "cast",
            "send",
            address,
            call.signature,
            *call.args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            "30",
            "--timeout",
            "30",
            "--gas-limit",
            GAS_LIMIT,
            "--unlocked",
            "--from",
            sender,
            "--json",
        ],
        timeout=45,
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or "transaction failed")[:2000]
    try:
        receipt = json.loads(proc.stdout)
    except json.JSONDecodeError as error:
        return None, f"invalid transaction receipt: {error}"
    status = receipt_int(receipt.get("status"))
    gas = receipt_int(receipt.get("gasUsed"))
    if status != 1:
        return None, f"transaction failed with status {status} and gas {gas}"
    if gas is None:
        return None, "transaction receipt has no gas usage"
    return gas, ""


def read(address: str, check: Check, rpc_url: str) -> tuple[str | None, str]:
    proc = run(
        [
            "cast",
            "call",
            address,
            check.signature,
            *check.args,
            "--rpc-url",
            rpc_url,
            "--rpc-timeout",
            "30",
            "--gas-limit",
            GAS_LIMIT,
        ],
        timeout=30,
    )
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or "call failed")[:2000]
    value = " ".join(proc.stdout.split())
    return value.lower() if value.startswith("0x") else value, ""


def deploy(
    bytecode: str,
    case: Case,
    rpc_url: str,
    sender: str,
) -> tuple[str | None, int | None, str]:
    encoded = ""
    if case.constructor_args:
        if case.constructor_signature is None:
            return None, None, "constructor arguments have no signature"
        proc = run(
            [
                "cast",
                "abi-encode",
                case.constructor_signature,
                *case.constructor_args,
            ],
            timeout=30,
        )
        if proc.returncode != 0:
            return None, None, (proc.stderr or proc.stdout)[:2000]
        encoded = proc.stdout.strip().removeprefix("0x")

    if re.fullmatch(r"0x[0-9a-fA-F]{40}", sender) is None:
        return None, None, "invalid deployer address"

    transaction_hash, error = rpc(
        "eth_sendTransaction",
        (
            {
                "from": sender,
                "gas": hex(int(GAS_LIMIT)),
                "data": "0x" + bytecode + encoded,
            },
        ),
        rpc_url,
        timeout=90,
    )
    if error:
        return None, None, error
    if not isinstance(transaction_hash, str):
        return None, None, "deployment returned no transaction hash"

    receipt = None
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        receipt, error = rpc("eth_getTransactionReceipt", (transaction_hash,), rpc_url)
        if error:
            return None, None, error
        if receipt is not None:
            break
        time.sleep(0.1)
    if not isinstance(receipt, dict):
        return None, None, "timed out waiting for deployment receipt"

    status = receipt_int(receipt.get("status"))
    gas = receipt_int(receipt.get("gasUsed"))
    if status != 1:
        return None, gas, f"deployment failed with status {status} and gas {gas}"
    address = receipt.get("contractAddress")
    if not isinstance(address, str):
        return None, gas, "deployment receipt has no contract address"
    return address, gas, ""


def run_case(
    case: Case,
    compilers: Sequence[tuple[str, Path]],
    root: Path,
    rpc_url: str,
    sender: str,
) -> dict[str, Any]:
    entry: dict[str, Any] = {
        "test_id": case.test_id,
        "description": case.description,
        "contract_name": case.contract,
        "project": case.project,
        "source": case.source,
        "suite": "large",
        "gas_profile": "hot",
        "compilers": {},
    }

    for compiler_id, compiler in compilers:
        compiled = compile_case(compiler_id, compiler, case, root)
        entry["compilers"][compiler_id] = compiled
        if compiled["status"] != "ok":
            continue

        error = ""
        for address, code in case.setup_code:
            _, error = rpc("anvil_setCode", (address, code), rpc_url)
            if error:
                break
        if not error:
            address, deploy_gas, error = deploy(
                compiled["bytecode"],
                case,
                rpc_url,
                sender,
            )
        else:
            address = None
            deploy_gas = None
        if error:
            compiled["deploy_status"] = "failed"
            compiled["deploy_error"] = error
            continue
        assert address is not None
        compiled["deploy_status"] = "ok"
        compiled["deploy_gas"] = deploy_gas
        compiled["address"] = address

        gas_results = []
        total_gas = 0
        gas_failed = False
        for call in case.calls:
            for index in range(call.repeat):
                label = call.label if call.repeat == 1 else f"{call.label}#{index + 1}"
                gas, error = send(address, call, rpc_url, sender)
                row = {
                    "label": label,
                    "call": call.signature,
                    "args": list(call.args),
                    "gas": gas,
                }
                if error:
                    row["error"] = error
                    gas_failed = True
                else:
                    total_gas += gas or 0
                gas_results.append(row)
        compiled["gas_results"] = gas_results
        compiled["gas_status"] = "failed" if gas_failed else "ok"
        compiled["total_gas"] = None if gas_failed else total_gas

        runtime_results = []
        runtime_failed = False
        for check in case.checks:
            value, error = read(address, check, rpc_url)
            row = {"label": check.label, "status": "ok", "value": value}
            if error:
                row.update({"status": "failed", "error": error})
                runtime_failed = True
            runtime_results.append(row)
        compiled["runtime_results"] = runtime_results
        compiled["runtime_status"] = "failed" if runtime_failed else "ok"
        compiled.pop("bytecode", None)
        compiled.pop("runtime_bytecode", None)

    compare_runtime(entry, tuple(compiler_id for compiler_id, _ in compilers))
    return entry


def compare_runtime(entry: dict[str, Any], compiler_ids: Sequence[str]) -> None:
    values: dict[str, dict[str, str]] = {}
    failed = False
    for compiler_id in compiler_ids:
        compiler = entry["compilers"].get(compiler_id, {})
        if compiler.get("runtime_status") != "ok":
            failed = True
        values[compiler_id] = {
            row["label"]: row["value"]
            for row in compiler.get("runtime_results", ())
            if row.get("status") == "ok"
        }

    labels = sorted({label for compiler in values.values() for label in compiler})
    mismatches = []
    for label in labels:
        observed = {
            compiler_id: values[compiler_id].get(label)
            for compiler_id in compiler_ids
        }
        if None in observed.values():
            failed = True
        elif len(set(observed.values())) != 1:
            mismatches.append({"label": label, "values": observed})

    entry["runtime_mismatches"] = mismatches
    entry["runtime_status"] = "mismatch" if mismatches else "failed" if failed else "ok"


def start_anvil(rpc_url: str) -> subprocess.Popen[bytes]:
    match = re.fullmatch(r"http://(?:127\.0\.0\.1|localhost):(\d+)", rpc_url)
    if match is None:
        raise ValueError("`--start-anvil` requires a localhost HTTP RPC URL")
    process = subprocess.Popen(
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
        if process.poll() is not None:
            raise RuntimeError("anvil exited before accepting RPC requests")
        _, error = rpc("eth_chainId", (), rpc_url)
        if not error:
            return process
        time.sleep(0.1)
    process.terminate()
    raise RuntimeError("anvil did not accept RPC requests")


def stop_anvil(process: subprocess.Popen[bytes]) -> None:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()


def failed(results: Sequence[dict[str, Any]]) -> bool:
    for result in results:
        if result.get("runtime_status") != "ok":
            return True
        for compiler in result["compilers"].values():
            if (
                compiler.get("status") != "ok"
                or compiler.get("deploy_status") != "ok"
                or compiler.get("gas_status") != "ok"
                or compiler.get("runtime_status") != "ok"
            ):
                return True
    return False


def print_summary(results: Sequence[dict[str, Any]]) -> None:
    print("Large-contract codegen benchmark")
    print("contract                     solc size   solar size   calls   solc gas   solar gas")
    for result in results:
        solc = result["compilers"].get("solc", {})
        solar = result["compilers"].get("solar", {})
        calls = len(solar.get("gas_results", ()))
        print(
            f"{result['test_id']:<28}"
            f"{solc.get('runtime_size', 0):>10,}"
            f"{solar.get('runtime_size', 0):>13,}"
            f"{calls:>8}"
            f"{(solc.get('total_gas') or 0):>11,}"
            f"{(solar.get('total_gas') or 0):>12,}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Benchmark hot calls against large real contracts",
    )
    parser.add_argument("--solc", required=True, type=Path)
    parser.add_argument("--solar", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--project-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    parser.add_argument("--rpc-url", default=DEFAULT_RPC_URL)
    parser.add_argument(
        "--sender",
        default=DEFAULT_SENDER,
        help="unlocked account used for deployments and state-changing calls",
    )
    parser.add_argument("--start-anvil", action="store_true")
    parser.add_argument("--allow-failures", action="store_true")
    args = parser.parse_args(argv)

    root = args.project_root.resolve()
    compilers = (
        ("solc", args.solc.resolve()),
        ("solar", args.solar.resolve()),
    )
    process = start_anvil(args.rpc_url) if args.start_anvil else None
    try:
        results = [
            run_case(
                case,
                compilers,
                root,
                args.rpc_url,
                args.sender,
            )
            for case in CASES
        ]
    finally:
        if process is not None:
            stop_anvil(process)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2) + "\n")
    print_summary(results)
    return 0 if args.allow_failures or not failed(results) else 1


if __name__ == "__main__":
    sys.exit(main())
