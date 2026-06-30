#!/usr/bin/env python3
"""Shared EVM/anvil helpers for Fandango differential runners."""

from __future__ import annotations

import json
import pathlib
import subprocess
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


class InfraError(RuntimeError):
    """Transient local infrastructure failure, not a compiler finding."""


def compile_solc(solc: str, source: pathlib.Path, contract: str, timeout: float) -> str:
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
    return runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def compile_solar(solar: str, source: pathlib.Path, contract: str, timeout: float) -> str:
    result = subprocess.run(
        [solar, "-Zcodegen", "--emit=bin-runtime", "--pretty-json", str(source)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    return runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def cast_calldata(cast: str, signature: str, args: list[str]) -> str:
    result = subprocess.run(
        [cast, "calldata", signature, *args],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


def runtime_from_contracts(contracts: dict[str, Any], contract: str) -> str:
    suffix = f":{contract}"
    for name, artifact in contracts.items():
        if name.endswith(suffix):
            runtime = artifact.get("bin-runtime")
            if not runtime:
                raise ValueError(f"contract {name} has no runtime bytecode")
            return "0x" + runtime.removeprefix("0x")
    raise ValueError(f"contract {contract} not found")


def rpc(
    url: str,
    method: str,
    params: list[Any],
    timeout: float,
    retries: int = 2,
) -> dict[str, Any]:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    request = urllib.request.Request(
        url, data=payload.encode(), headers={"Content-Type": "application/json"}
    )
    for attempt in range(retries + 1):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.loads(response.read().decode())
        except (urllib.error.URLError, TimeoutError) as err:
            if attempt >= retries:
                raise InfraError(f"JSON-RPC transport error for {method}: {err}") from err
            time.sleep(0.1 * (attempt + 1))

    raise InfraError(f"JSON-RPC transport error for {method}")


def set_code(url: str, address: str, runtime: str, timeout: float) -> None:
    response = rpc(url, "anvil_setCode", [address, runtime], timeout)
    if "error" in response:
        raise RuntimeError(f"anvil_setCode for {address} failed: {response['error']}")


def eth_call(
    url: str,
    address: str,
    calldata: str,
    timeout: float,
    envelope: dict[str, str] | None = None,
) -> dict[str, Any]:
    tx = {"to": address, "data": calldata}
    if envelope is not None:
        tx.update(envelope)
    response = rpc(url, "eth_call", [tx, "latest"], timeout)
    if "result" in response:
        return {"status": "ok", "data": normalize_hex(response["result"])}
    return {"status": "revert", "data": revert_data(response.get("error"))}


def send_tx(
    url: str, sender: str, address: str, calldata: str, timeout: float
) -> dict[str, Any]:
    response = rpc(
        url,
        "eth_sendTransaction",
        [{"from": sender, "to": address, "data": calldata, "gas": TX_GAS}],
        timeout,
    )
    if "result" not in response:
        # Submission rejected outright (should not happen with an explicit gas
        # limit, but record the reason for differential comparison anyway).
        return {"status": "rejected", "data": revert_data(response.get("error"))}

    receipt = wait_for_receipt(url, response["result"], timeout)
    if not receipt:
        return {"status": "no-receipt"}
    return {
        "status": "ok" if receipt.get("status") == "0x1" else "revert",
        "logs": [normalize_log(log) for log in receipt.get("logs", [])],
        # Storage-trie root: a digest of all persisted storage. Solidity assigns
        # the same slots for the same source, so this catches storage divergence
        # even when no later `eth_call` reads the written slot back.
        "storage": storage_root(url, address, timeout),
    }


def storage_root(url: str, address: str, timeout: float) -> str:
    response = rpc(url, "eth_getProof", [address, [], "latest"], timeout)
    if "error" in response:
        raise RuntimeError(f"eth_getProof for {address} failed: {response['error']}")
    proof = response.get("result")
    if isinstance(proof, dict) and isinstance(proof.get("storageHash"), str):
        return proof["storageHash"].lower()
    raise RuntimeError(f"eth_getProof for {address} did not return storageHash")


def wait_for_receipt(url: str, tx_hash: str, timeout: float) -> dict[str, Any] | None:
    """Polls for a transaction receipt.

    anvil returns the hash from `eth_sendTransaction` before the receipt is
    queryable, so a single lookup races the miner.
    """
    deadline = time.monotonic() + timeout
    while True:
        receipt = rpc(url, "eth_getTransactionReceipt", [tx_hash], timeout).get("result")
        if receipt:
            return receipt
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.05)


def revert_data(error: Any) -> str:
    """Extracts raw revert-data hex from a JSON-RPC error.

    The human-readable `message` is the node's decode, not contract output.
    """
    if not isinstance(error, dict):
        return "0x"
    data = error.get("data")
    if isinstance(data, dict):
        data = data.get("data") or data.get("result")
    return normalize_hex(data)


def normalize_log(log: dict[str, Any]) -> dict[str, Any]:
    """Compares logs by topics + data only.

    The emitting address is excluded because the runtimes run at different
    addresses.
    """
    return {
        "topics": [normalize_hex(topic) for topic in log.get("topics", [])],
        "data": normalize_hex(log.get("data")),
    }


def normalize_hex(value: Any) -> str:
    if isinstance(value, str) and value.startswith("0x"):
        return value.lower()
    return "0x"
