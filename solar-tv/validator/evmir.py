"""Import one empty-entry-stack branch window and its two stop exits."""

from .common import Program, Unsupported, lines, literal


def translate(text: str) -> Program:
    src = lines(text)
    if src[:1] != ["bb0:"] or src[-5:] != ["push bb1", "jumpi", "stop", "bb1:", "stop"]:
        raise Unsupported(
            "expected bb0 ending in push bb1 / jumpi / stop, with bb1: stop"
        )
    ops = []
    height = 0
    for line in src[1:-5]:
        if line in {"calldatasize", "returndatasize"}:
            ops.append(f".{line}")
            height += 1
        elif line.startswith("push "):
            ops.append(f".push {literal(line[5:])}")
            height += 1
        elif line in {"eq", "sub", "iszero"}:
            if height < (1 if line == "iszero" else 2):
                raise Unsupported("stack underflow in branch region")
            ops.append(f".{line}")
            height -= int(line != "iszero")
        else:
            raise Unsupported(f"unmodeled EVM IR instruction: {line}")
    if height < 1:
        raise Unsupported("missing branch predicate")
    # Only this final PUSH is a label. The two distinct tags identify outgoing
    # edges and never enter arithmetic. The 512-line input bound precludes overflow.
    ops.append(".push 1")
    return Program(
        ("branch",),
        ["calldatasize", "returndatasize"],
        "Option (Word × EVM.Stack)",
        "EVM.run ⟨calldatasize, returndatasize⟩ 0 [" + ", ".join(ops) + "] []",
    )
