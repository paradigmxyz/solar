"""Relate typed pointer arithmetic and its word-carrier lowering."""

import re
import subprocess
import sys
from copy import deepcopy

from . import mir_cfg, prune
from .common import Unsupported

RETURN_MEMORY = re.compile(r", may_return_memory(?=[,\]])|(?<=\[)may_return_memory, ")
WORD = {"kind": "word", "bits": 256, "name": "i256"}


def semantic_header(header):
    return RETURN_MEMORY.sub("", header).replace(" [may_return_memory] {", " {")


def materialize(body):
    body = deepcopy(body)
    converted = 0
    for block in body["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] != "pointer_add":
                continue
            pointer, delta = op["args"]
            ty = pointer["type"]
            if (
                ty.get("kind") != "pointer"
                or ty.get("bits") != 256
                or inst["type"] != ty
                or delta["type"].get("kind") != "word"
                or delta["type"].get("bits") != 256
                or inst["result"] is None
            ):
                raise Unsupported("memory pointer addition needs two word carriers")
            inst["type"] = WORD
            op.update(
                kind="opcode", name="add", args=[{**pointer, "type": WORD}, delta]
            )
            converted += 1
    return body, converted


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
        lowered_addition = before.count(" = ptradd ") > after.count(" = ptradd ")
        added_cast = any(
            after.count(operation) > before.count(operation)
            for operation in (" = ptrtoint ", " = inttoptr ")
        )
        marked_return = (
            "may_return_memory" in after and "may_return_memory" not in before
        )
        if not (lowered_addition or added_cast or marked_return):
            return None
        return mir_cfg.check(
            paths,
            solar,
            replay,
            version,
            timeout,
            function,
            lowering="lower-memory-objects",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Memory object proof: {error}", file=sys.stderr)
        return None
