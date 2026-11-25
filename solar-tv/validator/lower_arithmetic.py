"""Expand unsigned checked operations with their published panic blocks."""

import re
import subprocess
import sys
from copy import deepcopy

from . import mir_cfg, prune
from .common import Unsupported

WORD = {"kind": "word", "bits": 256, "name": "i256"}
BIT = {"kind": "word", "bits": 1, "name": "i1"}


def constant(value):
    return {"kind": "constant", "type": WORD, "value": str(value)}


def effect(name, args, ty=None, register=None):
    return {
        "type": ty,
        "result": register,
        "operation": {"kind": "opcode", "name": name, "args": args},
    }


def materialize(original):
    body = deepcopy(original)
    blocks = body["blocks"]
    count = len(blocks)
    next_register = 1 + max(
        (
            node["result"]
            for block in blocks
            for node in [*block["phis"], *block["instructions"]]
            if node["result"] is not None
        ),
        default=0,
    )
    panics = {}
    lowered = 0

    def push(block):
        number = len(blocks)
        blocks.append(block)
        return number

    def panic(code):
        if code not in panics:
            panics[code] = push(
                {
                    "phis": [],
                    "instructions": [
                        effect("mstore", [constant(32), constant(code)]),
                        effect("mstore", [constant(0), constant(0x4E487B71)]),
                    ],
                    "terminator": {
                        "kind": "revert",
                        "args": [constant(28), constant(36)],
                    },
                }
            )
        return panics[code]

    def expand_block(index):
        nonlocal next_register, lowered
        current = index
        instructions = blocks[index]["instructions"]
        end = blocks[index]["terminator"]
        blocks[index]["instructions"] = []

        def emit(op, ty, register=None):
            nonlocal next_register
            if register is None:
                register = next_register
                next_register += 1
            blocks[current]["instructions"].append(
                {"type": ty, "result": register, "operation": op}
            )
            return {"kind": "local", "index": register, "type": ty}

        def opcode(name, args, ty=WORD, register=None):
            if (
                name in {"lt", "gt"}
                and args[0]["kind"] == "constant"
                and args[1]["kind"] != "constant"
            ):
                name = "gt" if name == "lt" else "lt"
                args = list(reversed(args))
            return emit({"kind": "opcode", "name": name, "args": args}, ty, register)

        def extend(value):
            return emit({"kind": "word", "args": [value]}, WORD)

        def guard(flag, error, *, success=False):
            nonlocal current
            failure = panic(error)
            ok = push({"phis": [], "instructions": [], "terminator": None})
            yes, no = (ok, failure) if success else (failure, ok)
            blocks[current]["terminator"] = {
                "kind": "branch",
                "args": [flag],
                "then": yes,
                "else": no,
            }
            current = ok

        for inst in instructions:
            op = inst["operation"]
            if op["kind"] != "checked":
                blocks[current]["instructions"].append(inst)
                continue
            lowered += 1
            if op["signed"] or op["bits"] not in (128, 256) or inst["result"] is None:
                raise Unsupported(
                    "checked lowering needs unsigned 128- or 256-bit operations"
                )
            left, right = op["args"]
            width = op["bits"]
            dst = inst["result"]
            ty = inst["type"]
            if (
                ty.get("kind") != "word"
                or ty.get("bits") != width
                or left["type"] != ty
                or right["type"] != ty
            ):
                raise Unsupported("checked operands have different carrier types")
            if width < 256:
                left, right = extend(left), extend(right)
            kind = op["operation"]
            if kind == "Div" and width == 256:
                valid = emit({"kind": "ne", "args": [right, constant(0)]}, BIT)
                guard(valid, 18, success=True)
                opcode("div", [left, right], register=dst)
                continue
            if kind not in ("Add", "Sub", "Mul") or kind == "Mul" and width != 256:
                raise Unsupported("unmodeled unsigned arithmetic operation")
            value = opcode(
                kind.lower(), [left, right], register=dst if width == 256 else None
            )
            if kind == "Add":
                if width == 256:
                    flag = opcode("lt", [value, left], BIT)
                else:
                    flag = opcode("gt", [value, constant(2**width - 1)], BIT)
            elif kind == "Sub":
                flag = opcode("lt", [left, right], BIT)
            else:
                zero = opcode("eq", [right, constant(0)], BIT)
                quotient = opcode("div", [value, right])
                exact = opcode("eq", [quotient, left], BIT)
                valid = opcode("or", [zero, exact], BIT)
                flag = opcode("eq", [extend(valid), constant(0)], BIT)
            guard(flag, 17)
            if width != 256:
                emit(
                    {"kind": "low_bits", "args": [value], "bits": width},
                    ty,
                    dst,
                )
        blocks[current]["terminator"] = end
        return current

    final_predecessor = {index: expand_block(index) for index in range(count)}

    for block in blocks[:count]:
        for phi in block["phis"]:
            phi["incoming"] = [
                [final_predecessor[index], value] for index, value in phi["incoming"]
            ]
    return body, lowered


SOURCE_OPERATION = re.compile(r"= checked_(?:add|sub|mul|div)\b")


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
            lowering="lower-arithmetic",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Checked arithmetic proof: {error}", file=sys.stderr)
        return None
