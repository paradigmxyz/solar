"""Import a checked EVM block window with symbolic words at its declared entry height.

Static label pushes followed by JUMP/JUMPI expose an outgoing edge and live
stack. STOP, INVALID and empty RETURN/REVERT expose a terminal outcome. Other
blocks are unchanged stop sentinels, never interpreted as arbitrary code.
"""

import re

from .common import Program, Unsupported, lines, literal


def translate(text: str) -> Program:
    blocks: dict[str, list[str]] = {}
    heights = {}
    for line in lines(text):
        if header := re.fullmatch(r"(bb\d+)(?: \[([^]]+)\])?:", line):
            label = header[1]
            if label in blocks:
                raise Unsupported("duplicate EVM block")
            height = 0
            for attribute in header[2].split(", ") if header[2] else []:
                if attribute.startswith("stack=") and attribute[6:].isdigit():
                    height = int(attribute[6:])
                elif attribute != "cold":
                    raise Unsupported(f"unmodeled EVM block attribute {attribute}")
            if height > 1024:
                raise Unsupported("illegal EVM entry stack height")
            heights[label] = height
            blocks[label] = []
        elif blocks:
            blocks[label].append(line)
        else:
            raise Unsupported("expected an EVM block")
    if not blocks or next(iter(blocks)) != "bb0" or not blocks["bb0"]:
        raise Unsupported("expected a nonempty EVM entry block")
    if any(body != ["stop"] for label, body in blocks.items() if label != "bb0"):
        raise Unsupported("EVM successor blocks must be stop sentinels")

    body = list(blocks["bb0"])
    terminal = body.pop()
    if terminal == "stop" and body and body[-1] == "jumpi":
        body.pop()
        target = body.pop() if body else ""
        jump = re.fullmatch(r"push (bb\d+)", target)
        if not jump or jump[1] == "bb0" or jump[1] not in blocks:
            raise Unsupported("expected a valid static JUMPI destination")
        terminal = f"(.branch {jump[1][2:]} 0)"
    elif terminal == "jump":
        target = body.pop() if body else ""
        jump = re.fullmatch(r"push (bb\d+)", target)
        if not jump or jump[1] == "bb0" or jump[1] not in blocks:
            raise Unsupported("expected a valid static JUMP destination")
        terminal = f"(.jump {jump[1][2:]})"
    elif terminal in {"return", "revert"}:
        # The top word is offset, the second is size. These exact forms prove
        # size zero without introducing assumptions about symbolic memory.
        if (
            len(body) < 2
            or body[-2] != "push 0"
            or not (
                re.fullmatch(r"push (?:0x[0-9a-fA-F]+|[0-9]+)", body[-1])
                or body[-1] in {"dup 1", "calldatasize", "returndatasize"}
            )
        ):
            raise Unsupported("only statically empty EVM return/revert data is modeled")
        terminal = ".returnEmpty" if terminal == "return" else ".revertEmpty"
    elif terminal in {"stop", "invalid"}:
        terminal = f".{terminal}"
    else:
        raise Unsupported("unmodeled EVM terminal or dynamic transfer")

    ops = []
    for line in body:
        if line in {
            "calldatasize",
            "returndatasize",
            "eq",
            "sub",
            "add",
            "mul",
            "and",
            "or",
            "xor",
            "iszero",
            "not",
            "pop",
        }:
            ops.append(f".{line}")
        elif stack_op := re.fullmatch(r"(dup|swap) (\d+)", line):
            depth = int(stack_op[2])
            if not 1 <= depth <= 16:
                raise Unsupported("only DUP1..16 and SWAP1..16 are modeled")
            ops.append(f".{stack_op[1]} {depth - 1}")
        elif line.startswith("push "):
            ops.append(f".push {literal(line[5:])}")
        else:
            raise Unsupported(f"unmodeled EVM IR instruction: {line}")

    height = heights["bb0"]
    stack = [f"stack{i}" for i in range(height)]
    return Program(
        ("evm-window", str(height), *blocks.keys()),
        ["calldatasize", "returndatasize", *stack],
        "EVM.Execution",
        "EVM.execute ⟨calldatasize, returndatasize⟩ ["
        + ", ".join(ops)
        + "] "
        + terminal
        + " ["
        + ", ".join(stack)
        + "]",
    )
