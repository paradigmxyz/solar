#!/usr/bin/env python3
"""Run encoded ABI vectors against solc and this compiler on anvil.

This is a local fuzzing bridge. It expects an anvil RPC to be running and
compares byte-for-byte `eth_call` output for each JSONL vector produced by
`encode_abi_vectors.py`.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
from typing import Any


SOLC_ADDRESS = "0x1000000000000000000000000000000000000001"
SOLAR_ADDRESS = "0x1000000000000000000000000000000000000002"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default="fuzz/fandango/AbiVectorFixture.sol")
    parser.add_argument("--contract", default="AbiVectorFixture")
    parser.add_argument("--solc", default=shutil.which("solc") or "solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--cast", default=shutil.which("cast") or "cast")
    parser.add_argument("--rpc-url", default="http://127.0.0.1:8545")
    parser.add_argument("--failure-dir", default="fuzz/fandango/out/failures")
    parser.add_argument("--max-vectors", type=int, default=256)
    parser.add_argument("--max-calldata-bytes", type=int, default=4096)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()

    source = pathlib.Path(args.source)
    solc_runtime = _compile_solc(args.solc, source, args.contract, args.timeout)
    solar_runtime = _compile_solar(args.solar, source, args.contract, args.timeout)
    _set_code(args.cast, args.rpc_url, SOLC_ADDRESS, solc_runtime, args.timeout)
    _set_code(args.cast, args.rpc_url, SOLAR_ADDRESS, solar_runtime, args.timeout)

    vectors = _read_vectors(sys.stdin)
    _check_vector_budget(vectors, args.max_vectors, args.max_calldata_bytes)
    failures = []
    for vector in vectors:
        solc_result = _eth_call(
            args.cast, args.rpc_url, SOLC_ADDRESS, vector["calldata"], args.timeout
        )
        solar_result = _eth_call(
            args.cast, args.rpc_url, SOLAR_ADDRESS, vector["calldata"], args.timeout
        )
        if solc_result != solar_result:
            failure = {
                "vector": vector,
                "solc": solc_result,
                "solar": solar_result,
                "source": str(source),
                "contract": args.contract,
            }
            failures.append(failure)
            _write_failure(pathlib.Path(args.failure_dir), failure)

    summary = {"vectors": len(vectors), "failures": len(failures)}
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
    vectors: list[dict[str, Any]], max_vectors: int, max_calldata_bytes: int
) -> None:
    if len(vectors) > max_vectors:
        raise ValueError(f"too many vectors: {len(vectors)} > {max_vectors}")
    for vector in vectors:
        calldata = vector["calldata"]
        if not isinstance(calldata, str) or not calldata.startswith("0x"):
            raise ValueError(f"invalid calldata in vector {vector.get('index')}")
        byte_len = (len(calldata) - 2) // 2
        if byte_len > max_calldata_bytes:
            raise ValueError(
                f"calldata too large in vector {vector.get('index')}: "
                f"{byte_len} > {max_calldata_bytes}"
            )


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


def _set_code(cast: str, rpc_url: str, address: str, runtime: str, timeout: float) -> None:
    subprocess.run(
        [cast, "rpc", "--rpc-url", rpc_url, "anvil_setCode", address, runtime],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )


def _eth_call(
    cast: str, rpc_url: str, address: str, calldata: str, timeout: float
) -> dict[str, Any]:
    try:
        result = subprocess.run(
            [cast, "call", "--rpc-url", rpc_url, address, "--data", calldata],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as err:
        return {
            "status": "timeout",
            "stdout": (err.stdout or "").strip(),
            "stderr": (err.stderr or "").strip(),
        }
    return {
        "status": "ok" if result.returncode == 0 else "error",
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
    }


def _write_failure(directory: pathlib.Path, failure: dict[str, Any]) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    index = failure["vector"].get("index", len(list(directory.glob("*.json"))))
    path = directory / f"failure-{index}.json"
    path.write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
