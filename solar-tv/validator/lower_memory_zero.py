"""Expand a typed memory fill through CALLDATASIZE and zero padding."""

import re
import subprocess
import sys
from copy import deepcopy

from . import mir_cfg, prune
from .common import Unsupported
from .lower_arithmetic import WORD, constant, effect

POINTER = {"bits": 256, "kind": "pointer", "location": "memory", "name": "ptr<memory>"}


def materialize(body):
    body = deepcopy(body)
    definitions = {
        inst["result"]: inst
        for block in body["blocks"]
        for inst in block["instructions"]
        if inst["result"] is not None
    }
    next_local = 1 + max(
        (
            inst["result"]
            for block in body["blocks"]
            for inst in (*block["phis"], *block["instructions"])
            if inst["result"] is not None
        ),
        default=0,
    )
    fills = 0
    for block in body["blocks"]:
        lowered = []
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] != "memory_zero":
                lowered.append(inst)
                continue
            if (
                len(op["args"]) != 2
                or inst["type"] is not None
                or inst["result"] is not None
            ):
                raise Unsupported("memory zero has an unexpected arity or result")
            destination, count = op["args"]
            if destination["type"] != POINTER or count["type"] != WORD:
                raise Unsupported("memory zero needs a memory pointer and i256 count")
            if destination["kind"] == "constant":
                address = {**destination, "type": WORD}
            else:
                carrier = (
                    definitions.get(destination.get("index"))
                    if destination["kind"] == "local"
                    else None
                )
                if (
                    carrier is not None
                    and carrier["type"] == POINTER
                    and carrier["operation"]["kind"] == "word"
                    and len(carrier["operation"]["args"]) == 1
                    and carrier["operation"]["args"][0]["type"] == WORD
                ):
                    (address,) = carrier["operation"]["args"]
                else:
                    address = {"kind": "local", "index": next_local, "type": WORD}
                    lowered.append(
                        {
                            "type": WORD,
                            "result": next_local,
                            "operation": {
                                "kind": "low_bits",
                                "bits": 256,
                                "args": [destination],
                            },
                        }
                    )
                    next_local += 1
            if count == constant(32):
                lowered.append(effect("mstore", [address, constant(0)]))
            else:
                size = {"kind": "local", "index": next_local, "type": WORD}
                lowered.extend(
                    (
                        effect("calldatasize", [], WORD, next_local),
                        effect("calldatacopy", [address, size, count]),
                    )
                )
                next_local += 1
            fills += 1
        block["instructions"] = lowered
    return body, fills


SOURCE_OPERATION = re.compile(r"(?m)^memory_zero\s")


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    try:
        if any(
            path.suffix != ".mir" or path.stat().st_size > 1024 * 1024 for path in paths
        ):
            return None
        tables = [prune.declarations(path.read_text())[1] for path in paths]
        if any(function not in table for table in tables):
            return None
        before, after = ("\n".join(table[function]["text"]) for table in tables)
        if len(SOURCE_OPERATION.findall(before)) <= len(
            SOURCE_OPERATION.findall(after)
        ):
            return None
        return mir_cfg.check(
            paths,
            solar,
            replay,
            version,
            timeout,
            function,
            lowering="lower-memory-zero",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Memory zero proof: {error}", file=sys.stderr)
        return None
