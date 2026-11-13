"""Replay original IR through the independent Rust interpreters."""

import json
import os
import shutil
import subprocess
from functools import lru_cache
from pathlib import Path

from .common import Inconclusive, Program, ReplayTimeout, Unsupported


def replay(
    a: Program, b: Program, values: list[int], *, executable: str, timeout: int
) -> dict:
    binary = shutil.which(executable)
    if binary is None:
        raise Unsupported(
            "build the Rust replay helper with `cargo build -p solar-tv-replay`"
        )
    request = {
        "version": 1,
        "kind": a.interface[1]
        if a.world
        else "evmir"
        if a.interface[0] == "evm-window"
        else "mir",
        "before": a.replay_ir or a.canonical_ir,
        "after": b.replay_ir or b.canonical_ir,
        "selector": a.selector,
        "inputs": {
            name: str(value) for name, value in zip(a.inputs, values, strict=True)
        },
        "memory": a.memory,
        "gas": a.gas is not None,
        "evm_version": a.evm_version or "cancun",
    }
    if a.world:
        from .world import replay_input

        request["world"] = replay_input(a.world)
    try:
        result = subprocess.run(
            [binary],
            input=json.dumps(request),
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise ReplayTimeout(
            error.cmd, error.timeout, error.output, error.stderr
        ) from error
    if result.returncode:
        raise Inconclusive("Rust replay failed\n" + result.stderr)
    try:
        data = json.loads(result.stdout)
    except ValueError as error:
        raise Inconclusive("Rust replay returned invalid JSON") from error
    if (
        not isinstance(data, dict)
        or type(data.get("version")) is not int
        or data["version"] != 1
    ):
        raise Inconclusive("unsupported Rust replay protocol")
    if data.get("status") == "unsupported":
        raise Unsupported(
            "Rust replay: " + str(data.get("reason", "unknown unsupported state"))
        )
    observations = data.get("observations")
    if (
        data.get("status") != "ok"
        or not isinstance(observations, list)
        or len(observations) != 2
        or not all(isinstance(value, str) for value in observations)
    ):
        raise Inconclusive("Rust replay returned malformed observations")
    return data


def default(solar: str) -> str:
    configured = os.getenv("SOLAR_TV_REPLAY")
    if configured:
        return configured
    suffix = ".exe" if os.name == "nt" else ""
    return str(Path(solar).parent / f"solar-tv-replay{suffix}")


@lru_cache(maxsize=4)
def catalog(executable: str, timeout: int) -> dict:
    binary = shutil.which(executable)
    if binary is None:
        raise Unsupported(
            "build the Rust replay helper to check schema and target availability"
        )
    result = subprocess.run(
        [binary, "--catalog"],
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(result.stdout)
    except ValueError as error:
        raise Unsupported(
            "invalid operation catalog from Rust replay helper"
        ) from error
    if (
        result.returncode
        or not isinstance(value, dict)
        or value.get("version") != 1
        or not isinstance(value.get("evmir"), list)
    ):
        raise Unsupported("unsupported Rust operation catalog")
    return value
