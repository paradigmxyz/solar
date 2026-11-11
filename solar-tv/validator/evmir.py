"""Import a checked EVM block window with symbolic words at its declared entry height.

Static label pushes followed by JUMP/JUMPI expose an outgoing edge and live
stack. STOP, INVALID and empty RETURN/REVERT expose a terminal outcome. Other
blocks are unchanged stop sentinels, never interpreted as arbitrary code.
"""

import re

from . import gas as pricing
from . import memory as mem
from .common import Program, Unsupported, lines, literal


def translate(
    text: str,
    *,
    memory: bool = False,
    gas: str | None = None,
    evm_version: str = "cancun",
    entry: str = "bb0",
    following: str | None = None,
    scoped: bool = False,
) -> Program:
    memory |= gas is not None
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
    if not blocks or next(iter(blocks)) != entry or not blocks[entry]:
        raise Unsupported("expected a nonempty EVM entry block")
    if any(body != ["stop"] for label, body in blocks.items() if label != entry):
        raise Unsupported("EVM successor blocks must be stop sentinels")

    body = list(blocks[entry])
    boundaries = set(re.findall(r"\bbb\d+\b", " ".join(body))) if scoped else set()
    terminal = body.pop()
    estimate = 0
    words = 0
    memory |= any(line in {"mload", "mstore", "mstore8", "msize"} for line in body)
    output = terminal in {"return", "revert"}
    structured = re.fullmatch(r"jump (bb\d+)|jumpi (bb\d+), (bb\d+)", terminal)
    if structured:
        if not scoped:
            raise Unsupported("structured transfers require a scoped window")
        if gas is not None:
            raise Unsupported("gas replay requires explicit opcode transfers")
        targets = [target for target in structured.groups() if target]
        if any(target == entry or target not in blocks for target in targets):
            raise Unsupported("unmodeled window successor")
        terminal = (
            f"(.jump {targets[0][2:]})"
            if len(targets) == 1
            else f"(.branch {targets[0][2:]} {targets[1][2:]})"
        )
    elif terminal == "stop" and body and body[-1] == "jumpi":
        body.pop()
        target = body.pop() if body else ""
        jump = re.fullmatch(r"push (bb\d+)", target)
        if not jump or jump[1] == entry or jump[1] not in blocks:
            raise Unsupported("expected a valid static JUMPI destination")
        terminal = (
            f"(.branchStop {jump[1][2:]})" if scoped else f"(.branch {jump[1][2:]} 0)"
        )
        estimate += 13
    elif terminal == "jump":
        target = body.pop() if body else ""
        jump = re.fullmatch(r"push (bb\d+)", target)
        if not jump or jump[1] == entry or jump[1] not in blocks:
            raise Unsupported("expected a valid static JUMP destination")
        terminal = f"(.jump {jump[1][2:]})"
        estimate += 11
    elif terminal in {"return", "revert"}:
        # The top word is offset, the second is size. These exact forms prove
        # size zero without introducing assumptions about symbolic memory.
        empty = not (
            len(body) < 2
            or body[-2] != "push 0"
            or not (
                re.fullmatch(r"push (?:0x[0-9a-fA-F]+|[0-9]+)", body[-1])
                or body[-1] in {"dup 1", "calldatasize", "returndatasize"}
            )
        )
        memory |= not empty
        if memory:
            terminal = f"(.output {'true' if terminal == 'revert' else 'false'})"
        else:
            terminal = ".returnEmpty" if terminal == "return" else ".revertEmpty"
    elif terminal in {"stop", "invalid"}:
        terminal = f".{terminal}"
    else:
        raise Unsupported("unmodeled EVM terminal or dynamic transfer")

    ops = []
    constants: list[int | None] = [None] * heights[entry]
    for line in body:
        if gas is not None:
            estimate += pricing.estimate(line)
        if line == "gas":
            if gas is None:
                raise Unsupported("GAS requires --gas=exact or --gas=refine")
            ops.append(".gas")
            constants.insert(0, None)
            continue
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
            operation = f".{line}"
            if line in {"calldatasize", "returndatasize"}:
                constants.insert(0, None)
            elif line in {"iszero", "not", "pop"}:
                constants = constants[1:]
                if line != "pop":
                    constants.insert(0, None)
            else:
                constants = [None, *constants[2:]]
        elif stack_op := re.fullmatch(r"(dup|swap) (\d+)", line):
            depth = int(stack_op[2])
            if not 1 <= depth <= 16:
                raise Unsupported("only DUP1..16 and SWAP1..16 are modeled")
            operation = f".{stack_op[1]} {depth - 1}"
            if stack_op[1] == "dup":
                constants.insert(0, constants[depth - 1])
            else:
                constants[0], constants[depth] = constants[depth], constants[0]
        elif line.startswith("push "):
            value = literal(line[5:])
            operation = f".push {value}"
            constants.insert(0, int(value.split("#", 1)[0]))
        elif line in {"mload", "mstore", "mstore8", "msize"}:
            if line == "msize":
                constants.insert(0, None)
                operation = ".size"
            else:
                words = max(
                    words,
                    mem.active_words(constants[0], 1 if line == "mstore8" else 32),
                )
                if line == "mload":
                    constants[0] = None
                    operation = ".load"
                else:
                    constants = constants[2:]
                    operation = ".storeByte" if line == "mstore8" else ".store"
            ops.append(f".instruction ({operation})" if gas is not None else operation)
            continue
        else:
            raise Unsupported(f"unmodeled EVM IR instruction: {line}")
        operation = f".stack ({operation})" if memory else operation
        ops.append(f".instruction ({operation})" if gas is not None else operation)

    if structured and structured[1] == following and len(constants) >= 1024:
        raise Unsupported(
            "full-stack layout fallthrough needs distinct transfer semantics"
        )
    if memory and output:
        words = max(words, mem.active_words(constants[0], constants[1]))
    if memory and not output:
        terminal = f"(.control {terminal})"

    height = heights[entry]
    stack = [f"stack{i}" for i in range(height)]
    inputs = ["calldatasize", "returndatasize", *stack]
    if memory:
        inputs.extend(mem.INPUTS)
    if gas is not None:
        inputs.append("gas")
    function = (
        "EVM.Gas.execute"
        if gas is not None
        else "EVM.Memory.execute"
        if memory
        else "EVM.execute"
    )
    return Program(
        (
            "evm-window",
            str(height),
            *blocks.keys(),
            *(f"boundary:{label}" for label in sorted(boundaries)),
            *(("memory",) if memory else ()),
            *((gas, evm_version) if gas is not None else ()),
        ),
        inputs,
        "EVM.Gas.Result"
        if gas is not None
        else "EVM.Memory.Execution"
        if memory
        else "EVM.Execution",
        function
        + " ⟨calldatasize, returndatasize⟩ ["
        + ", ".join(ops)
        + "] "
        + terminal
        + " ["
        + ", ".join(stack)
        + "]"
        + (f" ({mem.INITIAL})" if memory else "")
        + (" gas.toNat" if gas is not None else ""),
        (*((256,) * (2 + height)), *mem.WIDTHS, *((256,) if gas is not None else ()))
        if memory
        else (),
        memory=memory,
        gas=gas,
        gas_cost=estimate + 3 * words + words * words // 512,
        evm_version=evm_version if gas is not None else None,
    )
