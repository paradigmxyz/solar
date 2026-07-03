#!/usr/bin/env python3
"""Run a generated Foundry differential fuzz target."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import tempfile
from typing import Any

import evm_runtime as evm
import write_foundry_target


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=pathlib.Path, required=True)
    parser.add_argument("--contract", default="FandangoRuntime")
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--fuzz-runs", type=int, default=64)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    solc_runtime = evm.compile_solc(args.solc, args.source, args.contract, args.timeout)
    solar_runtime = evm.compile_solar(args.solar, args.source, args.contract, args.timeout)

    with tempfile.TemporaryDirectory(prefix="solar-foundry-fuzz-") as tmp:
        project = pathlib.Path(tmp)
        write_foundry_target.write_target(args.source, project, solc_runtime, solar_runtime)
        foundry = _forge_test(project, args.fuzz_runs, args.timeout)

    summary = {
        "source": str(args.source),
        "foundry": foundry,
        "match": foundry["status"] == "ok",
    }
    print(json.dumps(summary, indent=2 if args.verbose else None, sort_keys=True))
    return 0 if summary["match"] else 1


def _forge_test(project: pathlib.Path, fuzz_runs: int, timeout: float) -> dict[str, Any]:
    env = os.environ.copy()
    try:
        result = subprocess.run(
            ["forge", "test", "--fuzz-runs", str(fuzz_runs)],
            cwd=project,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as err:
        return {
            "status": "timeout",
            "stdout": err.stdout or "",
            "stderr": err.stderr or "",
        }
    return {
        "status": "ok" if result.returncode == 0 else "error",
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


if __name__ == "__main__":
    raise SystemExit(main())
