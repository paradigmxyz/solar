"""Expand semantic guards for structural comparison with the pinned MIR."""

import re
import subprocess
import sys
from copy import deepcopy

from . import mir_cfg, prune
from .common import Unsupported
from .lower_arithmetic import BIT, WORD, constant, effect

HELPER = "revert_error"
HELPER_HEADER = "fn @revert_error(arg0: i256, arg1: i256) [cold] {"
SHORT = "icall require<short_string>,"
SOURCE_OPERATION = re.compile(
    r"\bicall (?:require<(?:short_string|custom_error)\b|(?:panic|revert)_if(?:_zero)?<)"
)


def scratch(selector, words):
    return {
        "phis": [],
        "instructions": [
            *(
                effect("mstore", [constant(32 * index), word])
                for index, word in enumerate(words, 1)
            ),
            effect("mstore", [constant(0), constant(selector)]),
        ],
        "terminator": {
            "kind": "revert",
            "args": [constant(28), constant(4 + 32 * len(words))],
        },
    }


def expected_helper(index):
    def argument(index):
        return {"index": index, "kind": "arg", "type": WORD}

    return {
        "id": index,
        "return_buffer": HELPER,
        "params": [WORD, WORD],
        "returns": [],
        "blocks": [scratch(0x08C379A0, [constant(32), argument(0), argument(1)])],
    }


def materialize(body, helper=None):
    body = deepcopy(body)
    blocks = body["blocks"]
    count = len(blocks)
    panics = {}
    lowered = 0

    def push(block):
        index = len(blocks)
        blocks.append(block)
        return index

    def expand_block(index):
        nonlocal lowered
        instructions = blocks[index]["instructions"]
        end = blocks[index]["terminator"]
        blocks[index]["instructions"] = []
        for inst in instructions:
            op = inst["operation"]
            kind = op["kind"]
            if kind not in {"check", "require_short", "require_encode"}:
                blocks[index]["instructions"].append(inst)
                continue
            if not op["args"] or inst["type"] is not None or inst["result"] is not None:
                raise Unsupported("conditional check has an unexpected result or arity")
            condition, *args = op["args"]
            if condition["type"] != BIT:
                raise Unsupported("non-boolean conditional check")
            if kind == "check":
                code = op["panic"]
                if (
                    args
                    or code not in {None, 65}
                    or (code is not None and op["message"] is not None)
                ):
                    raise Unsupported("unmodeled builtin conditional check")
                if code not in panics:
                    panics[code] = push(
                        scratch(0x4E487B71, [constant(code)])
                        if code is not None
                        else {
                            "phis": [],
                            "instructions": [],
                            "terminator": {
                                "kind": "revert",
                                "args": [constant(0), constant(0)],
                            },
                        }
                    )
                failed = panics[code]
                success_first = op["zero"]
            elif kind == "require_short":
                if (
                    helper is None
                    or len(args) != 2
                    or any(arg["type"] != WORD for arg in args)
                ):
                    raise Unsupported("unbound short-string helper")
                failed = push(
                    {
                        "phis": [],
                        "instructions": [
                            {
                                "type": None,
                                "result": None,
                                "operation": {
                                    "kind": "call",
                                    "function": helper,
                                    "args": args,
                                },
                            }
                        ],
                        "terminator": {"kind": "invalid"},
                    }
                )
                success_first = True
            else:
                if len(args) not in {2, 4}:
                    raise Unsupported(
                        "custom error must have one or three static words"
                    )
                selector, *words = args
                if (
                    not op["custom"]
                    or op["memory_slices"] != [False] * len(words)
                    or len(op["shapes"]) != len(words)
                    or any(shape["kind"] != "word" for shape in op["shapes"])
                    or selector["kind"] != "constant"
                    or any(arg["type"] != WORD for arg in args)
                ):
                    raise Unsupported("custom error does not have static word fields")
                value = int(selector["value"])
                if not 0 <= value < 2**256 or value % 2**224:
                    raise Unsupported("custom error selector is not left-aligned")
                failed = push(scratch(value >> 224, words))
                success_first = True
            continued = push({"phis": [], "instructions": [], "terminator": None})
            yes, no = (continued, failed) if success_first else (failed, continued)
            blocks[index]["terminator"] = {
                "kind": "branch",
                "args": [condition],
                "then": yes,
                "else": no,
            }
            index = continued
            lowered += 1
        blocks[index]["terminator"] = end
        return index

    pred = {index: expand_block(index) for index in range(count)}
    for block in blocks[:count]:
        for phi in block["phis"]:
            phi["incoming"] = [[pred[index], value] for index, value in phi["incoming"]]
    return body, lowered


def helper_addition(before, after):
    return after.keys() - before.keys() == {HELPER} and any(
        SHORT in line for function in before.values() for line in function["text"]
    )


def bind_helper(tables, declarations):
    before, after = tables
    source, target = declarations
    if not helper_addition(source, target):
        return None
    helper = after.get(HELPER)
    if (
        helper is None
        or HELPER in before
        or target[HELPER]["root"]
        or target[HELPER]["text"][0] != HELPER_HEADER
        or helper != expected_helper(helper["id"])
    ):
        raise Unsupported("short-string helper does not match its checked body")
    return helper


def materialize_pair(before, _after, helper):
    return materialize(before, helper)


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
            paths, solar, replay, version, timeout, function, lowering="lower-checks"
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Conditional check proof: {error}", file=sys.stderr)
        return None
