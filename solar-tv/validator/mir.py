"""Translate pure i256 SSA and ordered switches to SolarTV.MIR operations."""

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
    values = set(inputs)

    def word(token: str) -> str:
        return token if token in values else literal(token)

    code = []
    for line in blocks["bb0"][:-1]:
        inst = re.fullmatch(r"(v\d+) = (add|sub|mul|and|or|xor) (\w+), (\w+)", line)
        if not inst:
            raise Unsupported(f"unmodeled MIR instruction: {line}")
        dest, op, a, b = inst.groups()
        code.append(f"let {dest} := MIR.eval .{op} {word(a)} {word(b)}")
        values.add(dest)

    def returned(line: str) -> str:
        ret = re.fullmatch(r"ret (\w+)", line)
        if not ret:
            raise Unsupported(f"expected scalar return, got {line}")
        return word(ret[1])

    exits = {}
    for label, body in list(blocks.items())[1:]:
        if len(body) != 1:
            raise Unsupported("successor must return directly")
        exits[label] = returned(body[0])

    def edge(label: str) -> str:
        if label not in exits:
            raise Unsupported(f"unsupported control flow to {label}")
        return exits[label]

    term = blocks["bb0"][-1]
    if jump := re.fullmatch(r"jump (bb\d+)", term):
        result = edge(jump[1])
    elif switch := re.fullmatch(r"switch (\w+), default (bb\d+), \[(.*)\]", term):
        result = edge(switch[2])
        key = word(switch[1])
        for arm in reversed(switch[3].split(", ") if switch[3] else []):
            case = re.fullmatch(r"(\w+) => (bb\d+)", arm)
            if not case:
                raise Unsupported("malformed switch case")
            result = f"(MIR.choose {key} {word(case[1])} {edge(case[2])} {result})"
    else:
        result = returned(term)
    code.append(result)
    return Program((header[1], phase, *params), inputs, "Word", "\n  ".join(code))
