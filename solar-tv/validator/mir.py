"""Translate acyclic pure i256 SSA, including edge-selected phis, to Lean.

Expand each path with its own SSA environment. A phi reads the predecessor's
incoming value before any of the block's phi results are bound. Inactive
incoming values can be unavailable on this path and are represented as zero;
the concrete predecessor makes them unreachable in the Lean selector. Reject
cycles and excessive path expansion instead of making a bounded proof claim.
"""

import re

from .common import Program, Unsupported, lines, literal


def translate(text: str) -> Program:
    src = lines(text)
    phase = src.pop(0) if src and src[0] == "@phase lowered" else "@phase semantic"
    header = re.fullmatch(r"fn @(\w+)\((.*?)\) -> i256 \{", src[0] if src else "")
    if not header or src[-1] != "}":
        raise Unsupported("expected one i256-returning MIR function")
    params = header[2].split(", ") if header[2] else []
    inputs = [f"arg{i}" for i in range(len(params))]
    if params != [f"{arg}: i256" for arg in inputs]:
        raise Unsupported("expected i256 arguments")
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
    # Still reject unsupported operations even in an unreachable block.
    for body in blocks.values():
        for line in body[:-1]:
            if not re.fullmatch(
                r"v\d+ = (?:(?:add|sub|mul|and|or|xor) \w+, \w+|phi .+)", line
            ):
                raise Unsupported(f"unmodeled MIR instruction: {line}")
        if not re.fullmatch(
            r"(?:ret \w+|jump bb\d+|switch \w+, default bb\d+, \[.*\])", body[-1]
        ):
            raise Unsupported(f"unmodeled MIR terminator: {body[-1]}")

    expansions = 0

    def word(token: str, values: set[str]) -> str:
        return token if token in values else literal(token)

    def enter(label: str, predecessor: str, values: set[str], path: tuple) -> str:
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
        current = set(values)
        for line in blocks[label][:-1]:
            if phi := re.fullmatch(r"(v\d+) = phi (.+)", line):
                incoming = []
                selected = 0
                for arm in phi[2].split(", "):
                    match = re.fullmatch(r"\[(bb\d+): (\w+)\]", arm)
                    if not match:
                        raise Unsupported("malformed phi incoming value")
                    pred, value = match.groups()
                    active = pred == predecessor
                    selected += active
                    value = word(value, values) if active else "0#256"
                    incoming.append(f"({pred[2:]}, {value})")
                if selected != 1:
                    raise Unsupported("phi must select exactly one incoming value")
                code.append(
                    f"let {phi[1]} := (MIR.phi {predecessor[2:]} "
                    f"[{', '.join(incoming)}]).getD 0#256"
                )
                current.add(phi[1])
            elif inst := re.fullmatch(
                r"(v\d+) = (add|sub|mul|and|or|xor) (\w+), (\w+)", line
            ):
                dest, op, a, b = inst.groups()
                code.append(
                    f"let {dest} := MIR.eval .{op} {word(a, current)} {word(b, current)}"
                )
                current.add(dest)

        def edge(target: str) -> str:
            return f"(\n{enter(target, label, current, path)}\n)"

        term = blocks[label][-1]
        if jump := re.fullmatch(r"jump (bb\d+)", term):
            result = edge(jump[1])
        elif switch := re.fullmatch(r"switch (\w+), default (bb\d+), \[(.*)\]", term):
            result = edge(switch[2])
            key = word(switch[1], current)
            for arm in reversed(switch[3].split(", ") if switch[3] else []):
                case = re.fullmatch(r"(\w+) => (bb\d+)", arm)
                if not case:
                    raise Unsupported("malformed switch case")
                result = (
                    f"(MIR.choose {key} {word(case[1], current)} "
                    f"{edge(case[2])} {result})"
                )
        else:
            result = word(term.removeprefix("ret "), current)
        code.append(result)
        return "\n".join(code)

    body = enter("bb0", "entry", set(inputs), ())
    return Program((header[1], phase, *params), inputs, "Word", body)
