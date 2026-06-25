#!/usr/bin/env python3
"""Run encoded ABI vectors against solc and this compiler on anvil.

This is a local fuzzing bridge. It expects an anvil RPC to be running and
compares the *raw* EVM behavior of `eth_call` and transactions for vectors
produced by `encode_abi_vectors.py`.

For every vector the runtime is exercised with `eth_call` and the exact
return-data (on success) or revert-data (on failure) bytes are compared
byte-for-byte — the human-readable decode in the JSON-RPC `message` is ignored
on purpose, so the comparison is exactly the ABI/panic payload the contract
produced. Vectors with `"mode":"tx"` are additionally *sent* as transactions to
both runtimes so later vectors can observe stateful behavior; for those the
receipt status and emitted logs (topics + data, contract address excluded) are
compared too.

Both runtimes are installed at distinct addresses with `anvil_setCode`, so no
constructor runs: the fixture must not rely on constructor logic, immutables, or
preset storage. State accumulates across the run; a failure record therefore
includes the ordered transaction history needed to reproduce it.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import time
import urllib.error
import urllib.request
from typing import Any


SOLC_ADDRESS = "0x1000000000000000000000000000000000000001"
SOLAR_ADDRESS = "0x1000000000000000000000000000000000000002"
# Well-known anvil dev account 0; unlocked, so `eth_sendTransaction` needs no
# signature.
ANVIL_SENDER = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
# Explicit gas limit so a reverting transaction is mined with status 0x0 instead
# of being rejected during gas estimation. Safely below anvil's block limit.
TX_GAS = "0x1000000"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default="tests/fuzz/fandango/AbiVectorFixture.sol")
    parser.add_argument("--contract", default="AbiVectorFixture")
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--rpc-url", default="http://127.0.0.1:8545")
    parser.add_argument("--sender", default=ANVIL_SENDER)
    parser.add_argument("--failure-dir", default="tests/fuzz/fandango/out/failures")
    parser.add_argument("--max-vectors", type=int, default=256)
    parser.add_argument("--max-transactions", type=int, default=64)
    parser.add_argument("--max-calldata-bytes", type=int, default=4096)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()

    source = pathlib.Path(args.source)
    solc_runtime = _compile_solc(args.solc, source, args.contract, args.timeout)
    solar_runtime = _compile_solar(args.solar, source, args.contract, args.timeout)
    _set_code(args.rpc_url, SOLC_ADDRESS, solc_runtime, args.timeout)
    _set_code(args.rpc_url, SOLAR_ADDRESS, solar_runtime, args.timeout)

    vectors = _read_vectors(sys.stdin)
    _check_vector_budget(
        vectors, args.max_vectors, args.max_transactions, args.max_calldata_bytes
    )

    failures = []
    transactions = 0
    history: list[dict[str, Any]] = []
    for vector in vectors:
        mode = vector.get("mode", "call")
        calldata = vector["calldata"]

        # Always exercise `eth_call` so the exact return/revert bytes are
        # compared, including for the panic/revert vectors.
        call_envelope = {"from": args.sender, "gas": TX_GAS} if mode == "tx" else None
        solc_result: dict[str, Any] = {
            "call": _eth_call(
                args.rpc_url, SOLC_ADDRESS, calldata, args.timeout, call_envelope
            )
        }
        solar_result: dict[str, Any] = {
            "call": _eth_call(
                args.rpc_url, SOLAR_ADDRESS, calldata, args.timeout, call_envelope
            )
        }

        if mode == "tx":
            transactions += 1
            solc_result["receipt"] = _send_tx(
                args.rpc_url, args.sender, SOLC_ADDRESS, calldata, args.timeout
            )
            solar_result["receipt"] = _send_tx(
                args.rpc_url, args.sender, SOLAR_ADDRESS, calldata, args.timeout
            )
            history.append(
                {
                    "uid": vector.get("uid", vector.get("index")),
                    "label": vector.get("label"),
                    "calldata": calldata,
                }
            )
        elif mode != "call":
            raise ValueError(f"unsupported vector mode `{mode}`")

        if solc_result != solar_result:
            failure = {
                "vector": vector,
                "solc": solc_result,
                "solar": solar_result,
                "history": list(history),
                "source": str(source),
                "contract": args.contract,
            }
            failures.append(failure)
            _write_failure(pathlib.Path(args.failure_dir), failure)

    summary = {
        "vectors": len(vectors),
        "transactions": transactions,
        "calls": len(vectors) - transactions,
        "failures": len(failures),
    }
    print(json.dumps(summary, separators=(",", ":")))
    return 1 if failures else 0


def _read_vectors(stream: Any) -> list[dict[str, Any]]:
    vectors = []
    for line in stream:
        line = line.strip()
        if line:
            vectors.append(json.loads(line))
    return vectors


def _check_vector_budget(
    vectors: list[dict[str, Any]],
    max_vectors: int,
    max_transactions: int,
    max_calldata_bytes: int,
) -> None:
    if len(vectors) > max_vectors:
        raise ValueError(f"too many vectors: {len(vectors)} > {max_vectors}")
    transactions = 0
    for vector in vectors:
        if vector.get("mode", "call") == "tx":
            transactions += 1
        calldata = vector["calldata"]
        if not isinstance(calldata, str) or not calldata.startswith("0x"):
            raise ValueError(f"invalid calldata in vector {vector.get('uid')}")
        byte_len = (len(calldata) - 2) // 2
        if byte_len > max_calldata_bytes:
            raise ValueError(
                f"calldata too large in vector {vector.get('uid')}: "
                f"{byte_len} > {max_calldata_bytes}"
            )
    if transactions > max_transactions:
        raise ValueError(f"too many transactions: {transactions} > {max_transactions}")


def _compile_solc(solc: str, source: pathlib.Path, contract: str, timeout: float) -> str:
    result = subprocess.run(
        [
            solc,
            "--via-ir",
            "--optimize",
            "--metadata-hash",
            "none",
            "--combined-json",
            "bin-runtime",
            str(source),
        ],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    return _runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def _compile_solar(solar: str, source: pathlib.Path, contract: str, timeout: float) -> str:
    result = subprocess.run(
        [solar, "-Zcodegen", "--emit=bin-runtime", "--pretty-json", str(source)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    return _runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def _runtime_from_contracts(contracts: dict[str, Any], contract: str) -> str:
    suffix = f":{contract}"
    for name, artifact in contracts.items():
        if name.endswith(suffix):
            runtime = artifact.get("bin-runtime")
            if not runtime:
                raise ValueError(f"contract {name} has no runtime bytecode")
            return "0x" + runtime.removeprefix("0x")
    raise ValueError(f"contract {contract} not found")


def _rpc(url: str, method: str, params: list[Any], timeout: float) -> dict[str, Any]:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    request = urllib.request.Request(
        url, data=payload.encode(), headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode())
    except (urllib.error.URLError, TimeoutError) as err:
        raise RuntimeError(f"JSON-RPC transport error for {method}: {err}") from err


def _set_code(url: str, address: str, runtime: str, timeout: float) -> None:
    response = _rpc(url, "anvil_setCode", [address, runtime], timeout)
    if "error" in response:
        raise RuntimeError(f"anvil_setCode for {address} failed: {response['error']}")


def _eth_call(
    url: str,
    address: str,
    calldata: str,
    timeout: float,
    envelope: dict[str, str] | None = None,
) -> dict[str, Any]:
    tx = {"to": address, "data": calldata}
    if envelope is not None:
        tx.update(envelope)
    response = _rpc(url, "eth_call", [tx, "latest"], timeout)
    if "result" in response:
        return {"status": "ok", "data": _normalize_hex(response["result"])}
    return {"status": "revert", "data": _revert_data(response.get("error"))}


def _send_tx(
    url: str, sender: str, address: str, calldata: str, timeout: float
) -> dict[str, Any]:
    response = _rpc(
        url,
        "eth_sendTransaction",
        [{"from": sender, "to": address, "data": calldata, "gas": TX_GAS}],
        timeout,
    )
    if "result" not in response:
        # Submission rejected outright (should not happen with an explicit gas
        # limit, but record the reason for differential comparison anyway).
        return {"status": "rejected", "data": _revert_data(response.get("error"))}

    receipt = _wait_for_receipt(url, response["result"], timeout)
    if not receipt:
        return {"status": "no-receipt"}
    return {
        "status": "ok" if receipt.get("status") == "0x1" else "revert",
        "logs": [_normalize_log(log) for log in receipt.get("logs", [])],
        # Storage-trie root: a digest of all persisted storage. Solidity assigns
        # the same slots for the same source, so this catches storage divergence
        # even when no later `eth_call` reads the written slot back.
        "storage": _storage_root(url, address, timeout),
    }


def _storage_root(url: str, address: str, timeout: float) -> str | None:
    response = _rpc(url, "eth_getProof", [address, [], "latest"], timeout)
    if "error" in response:
        raise RuntimeError(f"eth_getProof for {address} failed: {response['error']}")
    proof = response.get("result")
    if isinstance(proof, dict) and isinstance(proof.get("storageHash"), str):
        return proof["storageHash"].lower()
    raise RuntimeError(f"eth_getProof for {address} did not return storageHash")


def _wait_for_receipt(url: str, tx_hash: str, timeout: float) -> dict[str, Any] | None:
    """Polls for a transaction receipt. anvil returns the hash from
    `eth_sendTransaction` before the receipt is queryable, so a single lookup
    races the miner."""
    deadline = time.monotonic() + timeout
    while True:
        receipt = _rpc(url, "eth_getTransactionReceipt", [tx_hash], timeout).get("result")
        if receipt:
            return receipt
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.05)


def _revert_data(error: Any) -> str:
    """Extracts the raw revert-data hex from a JSON-RPC error, ignoring the
    human-readable `message` (which is the node's decode, not contract output)."""
    if not isinstance(error, dict):
        return "0x"
    data = error.get("data")
    if isinstance(data, dict):
        data = data.get("data") or data.get("result")
    return _normalize_hex(data)


def _normalize_log(log: dict[str, Any]) -> dict[str, Any]:
    """Compares logs by topics + data only. The emitting address is excluded
    because solc and solar run at different addresses."""
    return {
        "topics": [_normalize_hex(topic) for topic in log.get("topics", [])],
        "data": _normalize_hex(log.get("data")),
    }


def _normalize_hex(value: Any) -> str:
    if isinstance(value, str) and value.startswith("0x"):
        return value.lower()
    return "0x"


def _write_failure(directory: pathlib.Path, failure: dict[str, Any]) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    vector = failure["vector"]
    seed = _safe_filename_part(vector.get("seed", "unknown"))
    label = _safe_filename_part(vector.get("label", "vector"))
    uid = _safe_filename_part(
        vector.get("uid", vector.get("index", len(list(directory.glob("*.json")))))
    )
    path = directory / f"failure-{seed}-{uid}-{label}.json"
    path.write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n")


def _safe_filename_part(value: Any) -> str:
    text = str(value)
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in text)


if __name__ == "__main__":
    raise SystemExit(main())
