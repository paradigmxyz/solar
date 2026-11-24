"""Expand fixed-slot source operations before checking the resolved MIR graph."""

import re
import subprocess
import sys
from copy import deepcopy

from . import mir_cfg, prune
from .common import Unsupported

WORD = {"kind": "word", "bits": 256, "name": "i256"}
SLOT_WORDS = {"mapping_slot": 2, "array_data_slot": 1}


def constant(value):
    return {"kind": "constant", "type": WORD, "value": str(value)}


def opcode(name, arguments, result=None):
    return {
        "operation": {"kind": "opcode", "name": name, "args": arguments},
        "result": result,
        "type": WORD if result is not None else None,
    }


def materialize(body):
    body = deepcopy(body)
    count = 0
    for block in body["blocks"]:
        output = []
        for inst in block["instructions"]:
            operation = inst["operation"]
            size = SLOT_WORDS.get(operation["kind"])
            if size is None:
                output.append(inst)
                continue
            arguments = operation["args"]
            if (
                len(arguments) != size
                or inst["result"] is None
                or inst["type"] != WORD
                or any(
                    arg["type"] != WORD or arg["kind"] == "undefined"
                    for arg in arguments
                )
            ):
                raise Unsupported("fixed-slot lowering requires live i256 words")
            output.extend(
                opcode("mstore", [constant(index * 32), arg])
                for index, arg in enumerate(arguments)
            )
            output.append(
                opcode("keccak256", [constant(0), constant(size * 32)], inst["result"])
            )
            count += 1
        block["instructions"] = output
    return body, count


FIXED_SLOT = re.compile(r"\b(?:mapping_slot|storage_array_data_slot)\b")


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
        if len(FIXED_SLOT.findall(before)) <= len(FIXED_SLOT.findall(after)):
            return None
        return mir_cfg.check(
            paths,
            solar,
            replay,
            version,
            timeout,
            function,
            lowering="lower-mapping-slots",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Storage slot proof: {error}", file=sys.stderr)
        return None
