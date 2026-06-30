#!/usr/bin/env python3
"""Run generated Solidity harnesses against solc and this compiler.

`solidity-runtime-source.fan` emits contracts with a fixed harness:

    setup(uint256)
    run(uint256,uint256,bytes)
    observe(uint256)

This runner compiles each generated source with solc and this compiler, installs
both runtimes into anvil with `anvil_setCode`, then runs deterministic fuzz-like
input sequences and compares raw call results plus transaction side effects.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
from typing import Any

import evm_runtime as evm


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=pathlib.Path)
    parser.add_argument(
        "--replay-failure",
        type=pathlib.Path,
        help="replay a failure JSON emitted by this runner",
    )
    parser.add_argument("--contract", default="FandangoRuntime")
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--cast", default=shutil.which("cast") or "cast")
    parser.add_argument("--rpc-url", default="http://127.0.0.1:8545")
    parser.add_argument("--sender", default=evm.ANVIL_SENDER)
    parser.add_argument("--failure-dir", default="fuzz/fandango/out/runtime-failures")
    parser.add_argument("--max-sources", type=int, default=64)
    parser.add_argument("--cases-per-source", type=int, default=8)
    parser.add_argument("--infra-retries", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    if args.replay_failure is not None:
        return _replay_failure(args, args.replay_failure)
    if args.source_dir is None:
        raise ValueError("--source-dir is required unless --replay-failure is used")

    sources = sorted(args.source_dir.glob("*.sol"))
    if len(sources) > args.max_sources:
        raise ValueError(f"too many sources: {len(sources)} > {args.max_sources}")

    failures = []
    infra_errors = []
    skipped = []
    executed_cases = 0
    written_failures = 0
    for index, source in enumerate(sources):
        if args.verbose:
            print(f"[runtime-source {index + 1}/{len(sources)}] {source}", file=sys.stderr)
        result = _check_source_with_retry(args, source)
        if result is None:
            executed_cases += args.cases_per_source
        elif result["kind"] == "infra":
            infra_errors.append(result)
            _write_failure(pathlib.Path(args.failure_dir), written_failures, result)
            written_failures += 1
        elif result["kind"] == "solc-compile-error":
            skipped.append(result)
            _write_failure(pathlib.Path(args.failure_dir), written_failures, result)
            written_failures += 1
        else:
            failures.append(result)
            _write_failure(pathlib.Path(args.failure_dir), written_failures, result)
            written_failures += 1

    summary = {
        "sources": len(sources),
        "cases": executed_cases,
        "skipped": len(skipped),
        "infra_errors": len(infra_errors),
        "failures": len(failures),
    }
    print(json.dumps(summary, separators=(",", ":")))
    return 1 if failures else 0


def _replay_failure(args: argparse.Namespace, failure_path: pathlib.Path) -> int:
    failure = json.loads(failure_path.read_text())
    with tempfile.TemporaryDirectory(prefix="solar-fandango-runtime-replay-") as tmp:
        source = pathlib.Path(tmp) / "replay.sol"
        source.write_text(failure["source_text"])
        replay_failure = _check_replay(args, source, failure)

    summary = {
        "replayed": str(failure_path),
        "failures": int(replay_failure is not None),
    }
    if replay_failure is not None:
        summary["failure"] = replay_failure
    print(json.dumps(summary, separators=(",", ":"), sort_keys=True))
    return 1 if replay_failure is not None else 0


def _check_source_with_retry(
    args: argparse.Namespace, source: pathlib.Path
) -> dict[str, Any] | None:
    last_error = None
    for _ in range(args.infra_retries + 1):
        try:
            return _check_source(args, source)
        except (evm.InfraError, subprocess.TimeoutExpired) as err:
            last_error = err
    return {
        "kind": "infra",
        "source": str(source),
        "source_text": source.read_text(),
        "error": str(last_error),
    }


def _check_replay(
    args: argparse.Namespace,
    source: pathlib.Path,
    failure: dict[str, Any],
) -> dict[str, Any] | None:
    snapshot = _snapshot(args.rpc_url, args.timeout)
    try:
        solc_runtime = evm.compile_solc(args.solc, source, args.contract, args.timeout)
        solar_runtime = evm.compile_solar(args.solar, source, args.contract, args.timeout)
        evm.set_code(args.rpc_url, evm.SOLC_ADDRESS, solc_runtime, args.timeout)
        evm.set_code(args.rpc_url, evm.SOLAR_ADDRESS, solar_runtime, args.timeout)

        history: list[dict[str, Any]] = []
        for entry in failure.get("history", []):
            for label in ("setup", "run"):
                replay = _compare_tx(
                    args,
                    source,
                    entry["case"],
                    {},
                    f"history.{label}",
                    entry[label],
                    history,
                )
                if replay is not None:
                    return replay
            history.append(entry)

        label = failure["label"]
        if label in ("setup", "run"):
            return _compare_tx(
                args,
                source,
                failure["case_index"],
                failure["case"],
                label,
                failure["calldata"],
                history,
            )
        return _compare_call(
            args,
            source,
            failure["case_index"],
            failure["case"],
            label,
            failure["calldata"],
            history,
        )
    finally:
        _revert(args.rpc_url, snapshot, args.timeout)


def _check_source(args: argparse.Namespace, source: pathlib.Path) -> dict[str, Any] | None:
    snapshot = _snapshot(args.rpc_url, args.timeout)
    try:
        try:
            solc_runtime = evm.compile_solc(args.solc, source, args.contract, args.timeout)
        except subprocess.CalledProcessError as err:
            return {
                "kind": "solc-compile-error",
                "source": str(source),
                "source_text": source.read_text(),
                "error": _process_error(err),
            }

        try:
            solar_runtime = evm.compile_solar(args.solar, source, args.contract, args.timeout)
        except subprocess.CalledProcessError as err:
            return {
                "kind": "solar-compile-error",
                "source": str(source),
                "source_text": source.read_text(),
                "error": _process_error(err),
            }
        except ValueError as err:
            return {
                "kind": "solar-output-error",
                "source": str(source),
                "source_text": source.read_text(),
                "error": str(err),
            }

        evm.set_code(args.rpc_url, evm.SOLC_ADDRESS, solc_runtime, args.timeout)
        evm.set_code(args.rpc_url, evm.SOLAR_ADDRESS, solar_runtime, args.timeout)

        history = []
        for case_index, case in enumerate(_cases(args.cases_per_source)):
            failure = _check_case(args, source, case_index, case, history)
            if failure is not None:
                return failure
        return None
    except RuntimeError as err:
        return {
            "kind": "setup",
            "source": str(source),
            "source_text": source.read_text(),
            "error": str(err),
        }
    finally:
        _revert(args.rpc_url, snapshot, args.timeout)


def _check_case(
    args: argparse.Namespace,
    source: pathlib.Path,
    case_index: int,
    case: dict[str, Any],
    history: list[dict[str, Any]],
) -> dict[str, Any] | None:
    setup_calldata = evm.cast_calldata(args.cast, "setup(uint256)", [case["seed"]])
    run_calldata = evm.cast_calldata(
        args.cast, "run(uint256,uint256,bytes)", [case["a"], case["b"], case["data"]]
    )
    observe_calldata = evm.cast_calldata(args.cast, "observe(uint256)", [case["key"]])

    for label, calldata in (("setup", setup_calldata), ("run", run_calldata)):
        failure = _compare_tx(args, source, case_index, case, label, calldata, history)
        if failure is not None:
            return failure

    failure = _compare_call(args, source, case_index, case, "observe", observe_calldata, history)
    if failure is not None:
        return failure

    history.append({
        "case": case_index,
        "setup": setup_calldata,
        "run": run_calldata,
        "observe": observe_calldata,
    })
    return None


def _compare_tx(
    args: argparse.Namespace,
    source: pathlib.Path,
    case_index: int,
    case: dict[str, Any],
    label: str,
    calldata: str,
    history: list[dict[str, Any]],
) -> dict[str, Any] | None:
    call_envelope = {"from": args.sender, "gas": evm.TX_GAS}
    solc_result: dict[str, Any] = {
        "call": evm.eth_call(args.rpc_url, evm.SOLC_ADDRESS, calldata, args.timeout, call_envelope)
    }
    solar_result: dict[str, Any] = {
        "call": evm.eth_call(args.rpc_url, evm.SOLAR_ADDRESS, calldata, args.timeout, call_envelope)
    }
    solc_result["receipt"] = evm.send_tx(
        args.rpc_url, args.sender, evm.SOLC_ADDRESS, calldata, args.timeout
    )
    solar_result["receipt"] = evm.send_tx(
        args.rpc_url, args.sender, evm.SOLAR_ADDRESS, calldata, args.timeout
    )
    return _failure_if_different(
        source, case_index, case, label, calldata, solc_result, solar_result, history
    )


def _compare_call(
    args: argparse.Namespace,
    source: pathlib.Path,
    case_index: int,
    case: dict[str, Any],
    label: str,
    calldata: str,
    history: list[dict[str, Any]],
) -> dict[str, Any] | None:
    solc_result = {"call": evm.eth_call(args.rpc_url, evm.SOLC_ADDRESS, calldata, args.timeout)}
    solar_result = {"call": evm.eth_call(args.rpc_url, evm.SOLAR_ADDRESS, calldata, args.timeout)}
    return _failure_if_different(
        source, case_index, case, label, calldata, solc_result, solar_result, history
    )


def _failure_if_different(
    source: pathlib.Path,
    case_index: int,
    case: dict[str, Any],
    label: str,
    calldata: str,
    solc_result: dict[str, Any],
    solar_result: dict[str, Any],
    history: list[dict[str, Any]],
) -> dict[str, Any] | None:
    if solc_result == solar_result:
        return None
    return {
        "kind": "runtime",
        "source": str(source),
        "source_text": source.read_text(),
        "case_index": case_index,
        "case": case,
        "label": label,
        "calldata": calldata,
        "solc": solc_result,
        "solar": solar_result,
        "history": list(history),
    }


def _cases(count: int) -> list[dict[str, Any]]:
    edge_cases = [
        _case(0, 0, 0, b""),
        _case(1, 1, 1, b"\x00"),
        _case(7, 7, 8, bytes(range(31))),
        _case(1023, 255, 256, bytes(range(32))),
        _case(U256_MAX, U64_MAX, U64_MAX - 1, bytes(range(33))),
        _case(0x45D9F3B, 0, U64_MAX, bytes([0xFF] * 16)),
        _case(0xDEADBEEF, U64_MAX, 0, bytes([0x80] + [0] * 32)),
        _case(0x123456789ABCDEF, 2**32 - 1, 2**32, bytes([1, 3, 5, 8, 13, 21])),
    ]
    if count <= len(edge_cases):
        return edge_cases[:count]

    cases = list(edge_cases)
    for index in range(count - len(edge_cases)):
        seed = ((index + 1) * 0x45D9F3B) & U256_MAX
        a = (seed ^ (index + 1) * 17) & U64_MAX
        b = ((seed >> 3) + index * 31 + 1) & U64_MAX
        data = bytes(((seed + j * 13) & 0xFF for j in range(index % 34)))
        cases.append(_case(seed, a, b, data))
    return cases


U64_MAX = (1 << 64) - 1
U256_MAX = (1 << 256) - 1


def _case(seed: int, a: int, b: int, data: bytes) -> dict[str, str]:
    return {
        "seed": str(seed & U256_MAX),
        "a": str(a & U256_MAX),
        "b": str(b & U256_MAX),
        "data": "0x" + data.hex(),
        "key": str((a + b) & 7),
    }


def _process_error(err: subprocess.CalledProcessError) -> dict[str, Any]:
    return {
        "returncode": err.returncode,
        "cmd": err.cmd,
        "stdout": err.stdout or "",
        "stderr": err.stderr or "",
    }


def _snapshot(url: str, timeout: float) -> str:
    response = evm.rpc(url, "evm_snapshot", [], timeout)
    if "result" not in response:
        raise RuntimeError(f"evm_snapshot failed: {response.get('error')}")
    return response["result"]


def _revert(url: str, snapshot: str, timeout: float) -> None:
    response = evm.rpc(url, "evm_revert", [snapshot], timeout)
    if response.get("result") is not True:
        raise RuntimeError(f"evm_revert failed: {response.get('error')}")


def _write_failure(directory: pathlib.Path, index: int, failure: dict[str, Any]) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    stem = pathlib.Path(failure["source"]).stem
    path = directory / f"runtime-{index}-{stem}.json"
    path.write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
