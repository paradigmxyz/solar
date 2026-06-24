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
    args = parser.parse_args()

    source = pathlib.Path(args.source)
    solc_runtime = _compile_solc(args.solc, source, args.contract)
    solar_runtime = _compile_solar(args.solar, source, args.contract)
    _set_code(args.cast, args.rpc_url, SOLC_ADDRESS, solc_runtime)
    _set_code(args.cast, args.rpc_url, SOLAR_ADDRESS, solar_runtime)

    vectors = _read_vectors(sys.stdin)
    failures = []
    for vector in vectors:
        solc_result = _eth_call(args.cast, args.rpc_url, SOLC_ADDRESS, vector["calldata"])
        solar_result = _eth_call(args.cast, args.rpc_url, SOLAR_ADDRESS, vector["calldata"])
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


def _compile_solc(solc: str, source: pathlib.Path, contract: str) -> str:
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
    )
    return _runtime_from_contracts(json.loads(result.stdout)["contracts"], contract)


def _compile_solar(solar: str, source: pathlib.Path, contract: str) -> str:
    result = subprocess.run(
        [solar, "-Zcodegen", "--emit=bin-runtime", "--pretty-json", str(source)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
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


def _set_code(cast: str, rpc_url: str, address: str, runtime: str) -> None:
    subprocess.run(
        [cast, "rpc", "--rpc-url", rpc_url, "anvil_setCode", address, runtime],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def _eth_call(cast: str, rpc_url: str, address: str, calldata: str) -> dict[str, Any]:
    result = subprocess.run(
        [cast, "call", "--rpc-url", rpc_url, address, "--data", calldata],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
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
