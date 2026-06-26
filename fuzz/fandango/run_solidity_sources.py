#!/usr/bin/env python3
"""Compile Fandango-generated Solidity sources with solc and this compiler.

This is a source-shape fuzzing prototype. Unlike `abi-values.fan`, which samples
runtime ABI values for a fixed fixture, `solidity-source.fan` emits complete
Solidity contracts. The runner either reads generated `.sol` files from
`--source-dir` or writes streamed sources to temporary files, then compares
compile success between solc and this compiler. It records any disagreement so
the source can be minimized and promoted into a UI/codegen test.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
from collections.abc import Iterable
from typing import Any


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--failure-dir", default="fuzz/fandango/out/source-failures")
    parser.add_argument(
        "--source-dir",
        type=pathlib.Path,
        help="directory containing generated .sol files to compile",
    )
    parser.add_argument("--max-sources", type=int, default=256)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="print each generated source file as it is compiled",
    )
    args = parser.parse_args()

    sources = _read_sources(args.source_dir, sys.stdin, args.max_sources)
    failure_dir = pathlib.Path(args.failure_dir)
    failures = []
    valid = 0
    invalid = 0

    if args.source_dir is not None:
        for index, (path, source) in enumerate(sources):
            solc_result, solar_result = _check_source(args, index, len(sources), path)
            valid, invalid = _update_counts(valid, invalid, solc_result)
            if solc_result["status"] != solar_result["status"]:
                failure = _failure(index, source, solc_result, solar_result)
                failures.append(failure)
                _write_failure(failure_dir, index, failure)
    else:
        with tempfile.TemporaryDirectory(prefix="solar-fandango-source-") as tmp:
            tmpdir = pathlib.Path(tmp)
            for index, (_, source) in enumerate(sources):
                path = tmpdir / f"source_{index}.sol"
                path.write_text(source)

                solc_result, solar_result = _check_source(args, index, len(sources), path)
                valid, invalid = _update_counts(valid, invalid, solc_result)
                if solc_result["status"] != solar_result["status"]:
                    failure = _failure(index, source, solc_result, solar_result)
                    failures.append(failure)
                    _write_failure(failure_dir, index, failure)

    summary = {
        "sources": len(sources),
        "valid": valid,
        "invalid": invalid,
        "failures": len(failures),
    }
    print(json.dumps(summary, separators=(",", ":")))
    return 1 if failures else 0


def _check_source(
    args: argparse.Namespace, index: int, total: int, path: pathlib.Path
) -> tuple[dict[str, Any], dict[str, Any]]:
    if args.verbose:
        print(f"[source {index + 1}/{total}] {path}", file=sys.stderr)

    solc_result = _compile_solc(args.solc, path, args.timeout)
    if args.verbose:
        print(f"  solc:  {solc_result['status']}", file=sys.stderr)
        print(
            f"  solar: {args.solar} -Zcodegen --emit=bin-runtime {path}",
            file=sys.stderr,
        )

    solar_result = _compile_solar(args.solar, path, args.timeout)
    if args.verbose:
        print(f"  solar: {solar_result['status']}", file=sys.stderr)

    return solc_result, solar_result


def _update_counts(
    valid: int, invalid: int, solc_result: dict[str, Any]
) -> tuple[int, int]:
    valid += int(solc_result["status"] == "ok")
    invalid += int(solc_result["status"] != "ok")
    return valid, invalid


def _failure(
    index: int,
    source: str,
    solc_result: dict[str, Any],
    solar_result: dict[str, Any],
) -> dict[str, Any]:
    return {
        "index": index,
        "source": source,
        "solc": solc_result,
        "solar": solar_result,
    }


def _read_sources(
    source_dir: pathlib.Path | None,
    stream: Any,
    max_sources: int,
) -> list[tuple[pathlib.Path | None, str]]:
    sources = list(_iter_sources(source_dir, stream))
    if len(sources) > max_sources:
        raise ValueError(f"too many sources: {len(sources)} > {max_sources}")
    return sources


def _iter_sources(
    source_dir: pathlib.Path | None,
    stream: Any,
) -> Iterable[tuple[pathlib.Path | None, str]]:
    if source_dir is not None:
        for path in sorted(source_dir.glob("*.sol")):
            yield path, path.read_text()
        return

    for line in stream:
        source = line.strip()
        if source:
            yield None, source


def _compile_solc(solc: str, source: pathlib.Path, timeout: float) -> dict[str, Any]:
    return _run([
        solc,
        "--via-ir",
        "--optimize",
        "--metadata-hash",
        "none",
        "--bin-runtime",
        str(source),
    ], timeout)


def _compile_solar(solar: str, source: pathlib.Path, timeout: float) -> dict[str, Any]:
    return _run([
        solar,
        "-Zcodegen",
        "--emit=bin-runtime",
        str(source),
    ], timeout)


def _run(argv: list[str], timeout: float) -> dict[str, Any]:
    try:
        result = subprocess.run(
            argv,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as err:
        return {
            "status": "timeout",
            "argv": argv,
            "stdout": err.stdout or "",
            "stderr": err.stderr or "",
        }

    return {
        "status": "ok" if result.returncode == 0 else "error",
        "argv": argv,
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def _write_failure(failure_dir: pathlib.Path, index: int, failure: dict[str, Any]) -> None:
    failure_dir.mkdir(parents=True, exist_ok=True)
    (failure_dir / f"source-{index}.json").write_text(
        json.dumps(failure, indent=2, sort_keys=True)
    )
    (failure_dir / f"source-{index}.sol").write_text(failure["source"])


if __name__ == "__main__":
    raise SystemExit(main())
