"""Translate acyclic pure SSA with typed integers and edge-selected phis to Lean.

Expand each path with its own SSA environment. A phi reads the predecessor's
incoming value before any of the block's phi results are bound. Inactive
incoming values can be unavailable on this path and are represented as zero;
the concrete predecessor makes them unreachable in the Lean selector. Reject
cycles and excessive path expansion instead of making a bounded proof claim.
Native BitVec widths constrain every symbolic input and intermediate value.
"""

import re

from .common import Program, Unsupported, lines, literal

OPERAND = r"(?:i\d+ )?\w+"
BINARY = (
    rf"(v\d+) = (add|sub|mul|and|or|xor|eq|ne|lt|gt|slt|sgt) ({OPERAND}), ({OPERAND})"
)
CAST = rf"(v\d+) = (trunc|zext|sext) i(\d+) ({OPERAND}) to i(\d+)"
PHI = r"(v\d+) = phi (.+)"
JUMP = r"jump (bb\d+)"
BRANCH = rf"jumpi ({OPERAND}), (bb\d+), (bb\d+)"
SWITCH = rf"switch ({OPERAND}), default (bb\d+), \[(.*)\]"
RETURN = rf"ret ({OPERAND})"


def width_of(text: str) -> int:
    width = int(text)
    if width not in {1, 160, 256}:
        raise Unsupported(f"unmodeled MIR integer width i{width}")
    return width


def translate(text: str) -> Program:
    src = lines(text)
    phase = src.pop(0) if src and src[0] == "@phase lowered" else "@phase semantic"
    header = re.fullmatch(r"fn @(\w+)\((.*?)\) -> i(\d+) \{", src[0] if src else "")
    if not header or src[-1] != "}":
        raise Unsupported("expected one integer-returning MIR function")
    params = header[2].split(", ") if header[2] else []
    inputs = [f"arg{i}" for i in range(len(params))]
    input_widths = []
    for arg, parameter in zip(inputs, params, strict=True):
        match = re.fullmatch(rf"{arg}: i(\d+)", parameter)
        if not match:
            raise Unsupported("expected integer arguments")
        input_widths.append(width_of(match[1]))
    result_width = width_of(header[3])
    blocks: dict[str, list[str]] = {}
    for line in src[1:-1]:
        if re.fullmatch(r"bb\d+:", line):
            block = line[:-1]
            if block in blocks:
                raise Unsupported("duplicate block")
            blocks[block] = []
        elif blocks:
            blocks[block].append(line)
        else:
            raise Unsupported("expected a block")
    if not blocks or next(iter(blocks)) != "bb0" or not all(blocks.values()):
        raise Unsupported("expected a nonempty entry block")

    # The compiler validates SSA, dominance and phi predecessor sets before import.
    # Reject unsupported operations and undefined operands even in unreachable code.
    for body in blocks.values():
        if any(re.search(r"\bundef\b", line) for line in body):
            raise Unsupported("undefined MIR operands")
        for line in body[:-1]:
            if not any(re.fullmatch(pattern, line) for pattern in (BINARY, CAST, PHI)):
                raise Unsupported(f"unmodeled MIR instruction: {line}")
        if not any(
            re.fullmatch(pattern, body[-1])
            for pattern in (RETURN, JUMP, BRANCH, SWITCH)
        ):
            raise Unsupported(f"unmodeled MIR terminator: {body[-1]}")

    expansions = 0

    def operand(
        token: str, values: dict[str, int], expected: int | None = None
    ) -> tuple[str, int]:
        if token in values:
            expression, width = token, values[token]
        elif token in {"true", "false"}:
            expression, width = f"{int(token == 'true')}#1", 1
        else:
            typed = re.fullmatch(r"i(\d+) (\w+)", token)
            width = width_of(typed[1]) if typed else expected or 256
            expression = literal(typed[2] if typed else token, width)
        if expected is not None and width != expected:
            raise Unsupported("MIR operand width mismatch")
        return expression, width

    def enter(label: str, predecessor: str, values: dict[str, int], path: tuple) -> str:
        nonlocal expansions
        expansions += 1
        if label in path:
            raise Unsupported("cyclic MIR control flow")
        if len(path) >= 128 or expansions > 512:
            raise Unsupported("MIR path expansion limit")
        if label not in blocks:
            raise Unsupported(f"unknown block {label}")
        path = (*path, label)
        code = []
        current = dict(values)
        for line in blocks[label][:-1]:
            if phi := re.fullmatch(PHI, line):
                incoming = []
                for arm in phi[2].split(", "):
                    match = re.fullmatch(rf"\[(bb\d+): ({OPERAND})\]", arm)
                    if not match:
                        raise Unsupported("malformed phi incoming value")
                    incoming.append(match.groups())
                selected = [value for pred, value in incoming if pred == predecessor]
                if len(selected) != 1:
                    raise Unsupported("phi must select exactly one incoming value")
                value, width = operand(selected[0], values)
                terms = [
                    f"({pred[2:]}, {value if pred == predecessor else f'0#{width}'})"
                    for pred, _ in incoming
                ]
                code.append(
                    f"let {phi[1]} := (MIR.phi {predecessor[2:]} "
                    f"[{', '.join(terms)}]).getD 0#{width}"
                )
                current[phi[1]] = width
            elif cast := re.fullmatch(CAST, line):
                dest, kind, source, value, target = cast.groups()
                source_width, target_width = width_of(source), width_of(target)
                value, _ = operand(value, current, source_width)
                code.append(f"let {dest} := MIR.cast .{kind} {target_width} {value}")
                current[dest] = target_width
            elif inst := re.fullmatch(BINARY, line):
                dest, op, a, b = inst.groups()
                a, width = operand(a, current)
                b, _ = operand(b, current, width)
                comparison = op in {"eq", "ne", "lt", "gt", "slt", "sgt"}
                operation = "compare" if comparison else "eval"
                code.append(f"let {dest} := MIR.{operation} .{op} {a} {b}")
                current[dest] = 1 if comparison else width

        def edge(target: str) -> str:
            return f"(\n{enter(target, label, current, path)}\n)"

        term = blocks[label][-1]
        if jump := re.fullmatch(JUMP, term):
            result = edge(jump[1])
        elif branch := re.fullmatch(BRANCH, term):
            condition, _ = operand(branch[1], current, 1)
            result = f"(MIR.branch {condition} {edge(branch[2])} {edge(branch[3])})"
        elif switch := re.fullmatch(SWITCH, term):
            result = edge(switch[2])
            key, width = operand(switch[1], current)
            for arm in reversed(switch[3].split(", ") if switch[3] else []):
                case = re.fullmatch(rf"({OPERAND}) => (bb\d+)", arm)
                if not case:
                    raise Unsupported("malformed switch case")
                value, _ = operand(case[1], current, width)
                result = f"(MIR.choose {key} {value} {edge(case[2])} {result})"
        else:
            result, _ = operand(term.removeprefix("ret "), current, result_width)
        code.append(result)
        return "\n".join(code)

    body = enter("bb0", "entry", dict(zip(inputs, input_widths, strict=True)), ())
    return Program(
        (header[1], phase, *params, f"i{result_width}"),
        inputs,
        f"BitVec {result_width}",
        body,
        tuple(input_widths),
    )
