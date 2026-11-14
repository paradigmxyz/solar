"""Import complete acyclic EVM regions, preserving stack/memory across transfers.

Follow every syntactic successor through terminal outcomes, including shared
tails and physical JUMPI continuations. No artificial outgoing frontier or stack
correspondence is assumed inside the region. Bounded expansion rejects cycles,
dynamic/escaping labels and operations outside the existing memory fragment.
"""

import re

from . import evmir, memory, regions
from .common import Program, Unsupported

UNFOLD = "EVM.Region.execute, EVM.Region.instructions"


def translate(text: str, entry: str, height: int) -> Program:
    bodies = regions.blocks(text)
    if entry not in bodies or not 0 <= height <= 1024:
        raise Unsupported("missing region entry or invalid stack height")
    # Preserve the original physical successor even when reachable blocks are
    # non-contiguous or the selected entry is not first in layout order.
    labels = list(bodies)
    following = dict(zip(labels, [*labels[1:], None], strict=True))
    visited = set()
    expansions = 0
    instruction_count = 0

    def block(label, constants, path):
        nonlocal expansions, instruction_count
        if label not in bodies:
            raise Unsupported("missing region successor")
        if label in path:
            raise Unsupported("cyclic EVM regions require loop invariants")
        if len(path) >= 128:
            raise Unsupported("region path depth exceeds 128 blocks")
        expansions += 1
        instruction_count += len(bodies[label])
        if expansions > 256 or instruction_count > 2048:
            raise Unsupported("region expansion budget exceeded")
        visited.add(label)
        return sequence(label, bodies[label], list(constants), {*path, label})

    def sequence(label, body, constants, path):
        ops = []
        index = 0
        while index < len(body):
            line = body[index]
            if pushed := re.fullmatch(r"push (bb\d+)", line):
                if index + 1 == len(body) or body[index + 1] not in {"jump", "jumpi"}:
                    raise Unsupported("region label escapes its static transfer")
                target = pushed[1]
                if body[index + 1] == "jump":
                    if index + 2 != len(body):
                        raise Unsupported(
                            "instructions after unconditional region transfer"
                        )
                    flow = f".jump 1 ({block(target, constants, path)})"
                else:
                    if not constants:
                        raise Unsupported("region branch lacks its condition")
                    yes = block(target, constants[1:], path)
                    no = sequence(label, body[index + 2 :], constants[1:], path)
                    flow = f".branch ({yes}) ({no})"
                break
            if jump := re.fullmatch(r"jump (bb\d+)", line):
                if index + 1 != len(body):
                    raise Unsupported("instructions after region terminator")
                target = jump[1]
                flow = f".jump {int(target != following[label])} ({block(target, constants, path)})"
                break
            if branch := re.fullmatch(r"jumpi (bb\d+), (bb\d+)", line):
                if index + 1 != len(body) or not constants:
                    raise Unsupported("invalid region branch")
                yes = block(branch[1], constants[1:], path)
                no = block(branch[2], constants[1:], path)
                flow = f".branch ({yes}) ({no})"
                break
            if line in {"return", "revert", "stop", "invalid"}:
                if index + 1 != len(body):
                    raise Unsupported("instructions after region halt")
                if line in {"return", "revert"}:
                    if len(constants) < 2:
                        raise Unsupported("region output lacks offset and size")
                    memory.active_words(constants[0], constants[1])
                    terminal = f".output {'true' if line == 'revert' else 'false'}"
                else:
                    terminal = f".control .{line}"
                flow = f".terminal ({terminal})"
                break
            try:
                added, constants, _, _ = evmir.instructions(
                    [line], constants, memory=True
                )
            except IndexError:
                raise Unsupported(
                    "region entry stack cannot execute its instructions"
                ) from None
            ops.extend(added)
            index += 1
        else:
            target = following[label]
            flow = (
                f".jump 0 ({block(target, constants, path)})"
                if target is not None
                else ".terminal (.control .stop)"
            )
        return f".instructions [{','.join(ops)}] ({flow})" if ops else flow

    code = block(entry, [None] * height, set())
    stack = [f"stack{i}" for i in range(height)]
    inputs = ["calldatasize", "returndatasize", *stack, *memory.INPUTS]
    projection = (
        "@module Region\n"
        + "\n".join(
            f"{label}:\n" + "\n".join(bodies[label])
            for label in labels
            if label in visited
        )
        + "\n"
    )
    if len(projection.encode()) > 128 * 1024:
        raise Unsupported("selected region exceeds 128 KiB")
    return Program(
        ("evm-region", str(height), "memory"),
        inputs,
        "EVM.Memory.Execution",
        f"EVM.Region.execute ⟨calldatasize, returndatasize⟩ (__IMPORT___code) [{','.join(stack)}] ({memory.INITIAL})",
        (*(256 for _ in range(2 + height)), *memory.WIDTHS),
        canonical_ir=projection,
        memory=True,
        replay_ir=text,
        selector=entry,
        scope="region",
        region_code=code,
    )


def cyclic(text, entry, height):
    """Keep an unbounded source graph; acceptance needs graph correspondence."""
    return Program(
        ("evm-cyclic-region", str(height)),
        ["calldatasize", "returndatasize"],
        "Correspondence.Graph EVM.Region.State EVM.Memory.Execution",
        "__IMPORT___graph ⟨calldatasize,returndatasize⟩",
        (256, 256),
        canonical_ir=text,
        replay_ir=text,
        selector=entry,
        scope="cyclic-region",
    )
