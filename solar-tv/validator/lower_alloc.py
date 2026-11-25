"""Propose the typed expansions for free-memory reads and reservations."""

import re
import subprocess
import sys
from collections import Counter
from copy import deepcopy
from itertools import product

from . import mir_cfg, prune, scalar_checks
from .common import Unsupported
from .lower_arithmetic import BIT, WORD, constant, effect
from .lower_checks import scratch

HELPER = "alloc"
HELPER_HEADER = "fn @alloc(arg0: i256) -> ptr<memory> {"
SOURCE_OPERATION = re.compile(r"(?m)^v\d+ = (?:fmp|alloc)\b")
CHECKED_ALLOCATION = re.compile(r"= alloc (?:zeroed )?checked\b")

POINTER = {"bits": 256, "kind": "pointer", "location": "memory", "name": "ptr<memory>"}


def local(index, ty=WORD):
    return {"kind": "local", "type": ty, "index": index}


def instruction(operation, result, ty):
    return {"operation": operation, "result": result, "type": ty}


def new_block(instructions=(), terminator=None):
    return {"instructions": list(instructions), "phis": [], "terminator": terminator}


def checked_ops(word, size, index):
    summed, wrapped, capped, bad = (
        local(index),
        local(index + 1, BIT),
        local(index + 2, BIT),
        local(index + 3, BIT),
    )
    return (
        [
            effect("add", [word, size], WORD, index),
            effect("lt", [summed, word], BIT, index + 1),
            effect("gt", [summed, constant(2**64 - 1)], BIT, index + 2),
            effect("or", [wrapped, capped], BIT, index + 3),
            instruction({"kind": "word", "args": [bad]}, index + 4, WORD),
        ],
        summed,
        bad,
    )


def branch(flag, ok, panic):
    return {"kind": "branch", "args": [flag], "then": panic, "else": ok}


def expected_helper(index):
    word, pointer = local(2), local(3, POINTER)
    argument = {"kind": "arg", "type": WORD, "index": 0}
    ops, end, bad = checked_ops(word, argument, 4)
    begin = [
        effect("mload", [constant(64)], WORD, 2),
        instruction({"kind": "word", "args": [word]}, 3, POINTER),
        *ops,
    ]
    commit = new_block(
        [effect("mstore", [constant(64), end])], {"kind": "return", "args": [pointer]}
    )
    return {
        "id": index,
        "return_buffer": HELPER,
        "params": [WORD],
        "returns": [POINTER],
        "blocks": [
            new_block(begin, branch(bad, 1, 2)),
            commit,
            scratch(0x4E487B71, [constant(65)]),
        ],
    }


def graph(body):
    return mir_cfg.import_graph(scalar_checks.normalize(body)[0])


def materialize(body, choices, helper=None):
    body = deepcopy(body)
    blocks = body["blocks"]
    next_local = 1 + max(
        (
            inst["result"]
            for b in blocks
            for inst in (*b["phis"], *b["instructions"])
            if inst["result"] is not None
        ),
        default=0,
    )
    count = len(blocks)
    counts = Counter()
    chosen = iter(choices)
    final = {}

    def emit(seq, op, ty=WORD, index=None):
        nonlocal next_local
        if index is None:
            index = next_local
            next_local += 1
        seq.append(instruction(op, index, ty))
        return local(index, ty)

    def cursor(seq, mode, out):
        if mode == "floor":
            op = {
                "kind": "layout",
                "query": "heap_floor",
                "function": body["return_buffer"],
                "args": [],
            }
        else:
            op = {"kind": "opcode", "name": "mload", "args": [constant(64)]}
        value = emit(seq, op)
        pointer = emit(seq, {"kind": "word", "args": [value]}, POINTER, out)
        return value, pointer

    for index in range(count):
        block = blocks[index]
        current = index
        seq, original = [], block["instructions"]
        term = block["terminator"]
        for inst in original:
            op = inst["operation"]
            kind = op["kind"]
            if kind not in {"allocate", "free_pointer"}:
                seq.append(inst)
                continue
            mode = next(chosen)
            if mode == "keep":
                seq.append(inst)
                continue
            out = inst["result"]
            if inst["type"] != POINTER or out is None:
                raise Unsupported("unmodeled memory pointer")
            counts[mode + ("-read" if kind == "free_pointer" else "-bump")] += 1
            if kind == "free_pointer":
                if op["args"] or mode == "helper":
                    raise Unsupported("unmodeled cursor read")
                cursor(seq, mode, out)
                continue
            if len(op["args"]) != 1 or op["args"][0]["type"] != WORD:
                raise Unsupported("allocation size must be a word")
            (size,) = op["args"]
            if not op["checked"]:
                counts["infallible"] += 1
            if mode == "helper":
                if helper is None or not op["checked"]:
                    raise Unsupported("unmodeled shared allocation")
                pointer = emit(
                    seq,
                    {"kind": "call", "function": helper, "args": [size]},
                    POINTER,
                    out,
                )
            else:
                word, pointer = cursor(seq, mode, out)
                if op["checked"]:
                    more, end, invalid = checked_ops(word, size, next_local)
                    seq.extend(more)
                    next_local += 5
                    ok, panic = len(blocks), len(blocks) + 1
                    blocks[current].update(
                        instructions=seq, terminator=branch(invalid, ok, panic)
                    )
                    blocks.extend([new_block(), scratch(0x4E487B71, [constant(65)])])
                    current, seq = ok, []
                else:
                    end = emit(
                        seq, {"kind": "opcode", "name": "add", "args": [word, size]}
                    )
                seq.append(effect("mstore", [constant(64), end]))
            if op["zeroed"]:
                seq.append(
                    instruction(
                        {"kind": "memory_zero", "args": [pointer, size]}, None, None
                    )
                )
        blocks[current].update(instructions=seq, terminator=term)
        final[index] = current
    for block in blocks[:count]:
        for phi in block["phis"]:
            phi["incoming"] = [[final[pred], val] for pred, val in phi["incoming"]]
    return body, counts


def nonfresh_activations(module):
    """Creation can lay out constructor arguments below the live pointer."""
    functions = {body["id"]: body for body in module["functions"]}
    constructor = module["constructor"]
    pending = [] if constructor is None else [constructor]
    creation = set()
    while pending:
        index = pending.pop()
        if index in creation:
            continue
        if index not in functions:
            raise Unsupported("MIR creation calls an unknown allocation context")
        creation.add(index)
        for block in functions[index]["blocks"]:
            pending.extend(
                op["function"]
                for inst in block["instructions"]
                if (op := inst["operation"])["kind"] == "call"
            )
            term = block["terminator"]
            if term["kind"] == "tail_call":
                pending.append(term["function"])
    if (dispatch := module["dispatch_entry"]) is not None:
        creation.add(dispatch)
    return creation


def materialize_pair(before, after, helper, *, allow_floor):
    if before == after:
        return before, Counter()
    choices = []
    for block in before["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "free_pointer":
                choices.append(["fmp", "keep"])
            if op["kind"] == "allocate":
                choices.append(
                    ["fmp"]
                    + (["floor"] if allow_floor else [])
                    + (["helper"] if op["checked"] and helper is not None else [])
                    + ["keep"]
                )
    if len(choices) > 6:
        raise Unsupported("too many choices")
    goal = graph(after)
    for plan in product(*choices):
        try:
            candidate, counts = materialize(before, plan, helper)
            mir_cfg.pairing(graph(candidate), goal)
        except Unsupported:
            continue
        return candidate, counts
    raise Unsupported("no allocation expansion matches the target")


def helper_addition(before, after):
    return after.keys() - before.keys() == {HELPER} and any(
        CHECKED_ALLOCATION.search(line)
        for function in before.values()
        for line in function["text"]
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
    ):
        raise Unsupported(
            "shared allocation helper does not match its checked interface"
        )
    expected = expected_helper(helper["id"])
    if {k: v for k, v in helper.items() if k != "blocks"} != {
        k: v for k, v in expected.items() if k != "blocks"
    }:
        raise Unsupported("shared allocation helper changed typed signature")
    mir_cfg.pairing(graph(expected), graph(helper))
    return helper


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
            paths, solar, replay, version, timeout, function, lowering="lower-alloc"
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Allocation lowering proof: {error}", file=sys.stderr)
        return None
