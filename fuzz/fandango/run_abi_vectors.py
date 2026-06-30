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
import sys
from typing import Any

import evm_runtime as evm


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default="fuzz/fandango/AbiVectorFixture.sol")
    parser.add_argument("--contract", default="AbiVectorFixture")
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--rpc-url", default="http://127.0.0.1:8545")
    parser.add_argument("--sender", default=evm.ANVIL_SENDER)
    parser.add_argument("--failure-dir", default="fuzz/fandango/out/failures")
    parser.add_argument("--max-vectors", type=int, default=256)
    parser.add_argument("--max-transactions", type=int, default=64)
    parser.add_argument("--max-calldata-bytes", type=int, default=4096)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()

    source = pathlib.Path(args.source)
    solc_runtime = evm.compile_solc(args.solc, source, args.contract, args.timeout)
    solar_runtime = evm.compile_solar(args.solar, source, args.contract, args.timeout)
    evm.set_code(args.rpc_url, evm.SOLC_ADDRESS, solc_runtime, args.timeout)
    evm.set_code(args.rpc_url, evm.SOLAR_ADDRESS, solar_runtime, args.timeout)

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
        call_envelope = {"from": args.sender, "gas": evm.TX_GAS} if mode == "tx" else None
        solc_result: dict[str, Any] = {
            "call": evm.eth_call(
                args.rpc_url, evm.SOLC_ADDRESS, calldata, args.timeout, call_envelope
            )
        }
        solar_result: dict[str, Any] = {
            "call": evm.eth_call(
                args.rpc_url, evm.SOLAR_ADDRESS, calldata, args.timeout, call_envelope
            )
        }

        if mode == "tx":
            transactions += 1
            solc_result["receipt"] = evm.send_tx(
                args.rpc_url, args.sender, evm.SOLC_ADDRESS, calldata, args.timeout
            )
            solar_result["receipt"] = evm.send_tx(
                args.rpc_url, args.sender, evm.SOLAR_ADDRESS, calldata, args.timeout
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
