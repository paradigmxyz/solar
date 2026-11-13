"""Translate acyclic pure SSA with typed integers and edge-selected phis to Lean.

Expand each path with its own SSA environment. A phi reads the predecessor's
incoming value before any of the block's phi results are bound. Inactive
incoming values can be unavailable on this path and are represented as zero;
the concrete predecessor makes them unreachable in the Lean selector. Reject
cycles and excessive path expansion instead of making a bounded proof claim.
Native BitVec widths constrain every symbolic input and intermediate value.
"""

import re

from . import memory as mem
from . import operations as ops
from .common import Program, Unsupported, lines, literal

OPERAND = r"(?:i\d+ )?\w+"
BINARY = rf"(v\d+) = ({ops.names(ops.BINARY | {'ne'})}) ({OPERAND}), ({OPERAND})"
UNARY = rf"(v\d+) = (not|clz) ({OPERAND})"
TERNARY = rf"(v\d+) = (addmod|mulmod|select) ({OPERAND}), ({OPERAND}), ({OPERAND})"
CAST = rf"(v\d+) = (trunc|zext|sext) i(\d+) ({OPERAND}) to i(\d+)"
PHI = r"(v\d+) = phi (.+)"
JUMP = r"jump (bb\d+)"
BRANCH = rf"jumpi ({OPERAND}), (bb\d+), (bb\d+)"
SWITCH = rf"switch ({OPERAND}), default (bb\d+), \[(.*)\]"
RETURN = rf"ret ({OPERAND})"
CHECKED = rf"(v\d+) = (checked_add|checked_sub|checked_mul|checked_div|wrapping_div|checked_rem|checked_pow) (u?int)(\d+), ({OPERAND}), ({OPERAND})"
CHECK = rf"icall (panic|revert)_if(_zero)?<([^>]+)>, ({OPERAND})"
REQUIRE = rf"icall require<empty_string>, ({OPERAND})"
LOAD = rf"(v\d+) = mload ({OPERAND})"
STORE = rf"(mstore|mstore8) ({OPERAND}), ({OPERAND})"
MSIZE = r"(v\d+) = msize"
COPY = rf"mcopy ({OPERAND}), ({OPERAND}), ({OPERAND})"
HASH = rf"(v\d+) = keccak256 ({OPERAND}), ({OPERAND})"


def width_of(text: str) -> int:
    width = int(text)
    if not 1 <= width <= 256:
        raise Unsupported(f"unmodeled MIR integer width i{width}")
    return width


def translate(text: str, *, memory: bool = False) -> Program:
    src = lines(text)
    phase = src.pop(0) if src and src[0] == "@phase lowered" else "@phase semantic"
    header = re.fullmatch(
        r"fn @([^ (]+)\((.*?)\) -> i(\d+)(?: \[(.*)\])? \{", src[0] if src else ""
    )
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
        if match := re.fullmatch(r"(bb\d+)(?: \[cold\])?:", line):
            block = match[1]
            if block in blocks:
                raise Unsupported("duplicate block")
            blocks[block] = []
        elif blocks:
            blocks[block].append(line)
        else:
            raise Unsupported("expected a block")
    if not blocks or next(iter(blocks)) != "bb0" or not all(blocks.values()):
        raise Unsupported("expected a nonempty entry block")
    memory |= any(
        re.search(r"\b(mload|mstore|mstore8|msize|mcopy|keccak256)\b", line)
        for line in src
    )

    # The compiler validates SSA, dominance and phi predecessor sets before import.
    # Reject unsupported operations and undefined operands even in unreachable code.
    for body in blocks.values():
        if any(re.search(r"\bundef\b", line) for line in body):
            raise Unsupported("undefined MIR operands")
        for line in body[:-1]:
            if not any(
                re.fullmatch(pattern, line)
                for pattern in (
                    BINARY,
                    UNARY,
                    TERNARY,
                    CAST,
                    PHI,
                    CHECKED,
                    CHECK,
                    REQUIRE,
                    LOAD,
                    STORE,
                    MSIZE,
                    COPY,
                    HASH,
                )
            ):
                raise Unsupported(f"unmodeled MIR instruction: {line}")
        if not any(
            re.fullmatch(pattern, body[-1])
            for pattern in (RETURN, JUMP, BRANCH, SWITCH, r"revert 0, 0", r"invalid")
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
        continuations = 0
        for line in blocks[label][:-1]:
            if load := re.fullmatch(LOAD, line):
                offset = mem.address(load[2], 32)
                code.append(f"let {load[1]} := Memory.load memory {offset}")
                code.append(f"let memory := Memory.expand memory {offset} 32")
                current[load[1]] = 256
            elif store := re.fullmatch(STORE, line):
                size = 32 if store[1] == "mstore" else 1
                offset = mem.address(store[2], size)
                value, _ = operand(store[3], current, 256)
                op = "store" if size == 32 else "storeByte"
                code.append(f"let memory := Memory.{op} memory {offset} {value}")
            elif size := re.fullmatch(MSIZE, line):
                code.append(f"let {size[1]} := Memory.msize memory")
                current[size[1]] = 256
            elif copy := re.fullmatch(COPY, line):
                count = mem.address(copy[3], 0)
                destination = mem.address(copy[1], count)
                source = mem.address(copy[2], count)
                code.append(
                    f"let memory := Memory.copy memory {destination} {source} {count}"
                )
            elif hash_ := re.fullmatch(HASH, line):
                count = mem.address(hash_[3], 0)
                offset = mem.address(hash_[2], count)
                code.append(f"let {hash_[1]} := Memory.hash memory {offset} {count}")
                code.append(f"let memory := Memory.expand memory {offset} {count}")
                current[hash_[1]] = 256
            elif checked := re.fullmatch(CHECKED, line):
                dest, op, kind, bits, a, b = checked.groups()
                op = op.removeprefix("checked_")
                width = width_of(bits)
                a, _ = operand(a, current, width)
                b, _ = operand(b, current, 256 if op == "pow" else width)
                signed = "true" if kind == "int" else "false"
                expression = (
                    f"MIR.checkedPower {signed} {a} ({b}).toNat"
                    if op == "pow"
                    else f"MIR.checked .{'wrappingDiv' if op == 'wrapping_div' else op} {signed} {a} {b}"
                )
                code.append(f"Outcome.bind ({expression}) (fun {dest} =>")
                continuations += 1
                current[dest] = width
            elif check := re.fullmatch(CHECK, line):
                kind, is_zero, reason, value = check.groups()
                value, _ = operand(value, current)
                if kind == "panic":
                    failure = f"(.panic {literal(reason)})"
                elif reason == "empty":
                    failure = ".empty"
                else:
                    raise Unsupported("unmodeled revert payload")
                test = "==" if is_zero else "!="
                code.append(f"MIR.guard ({value} {test} 0) {failure} (\n")
                continuations += 1
            elif require := re.fullmatch(REQUIRE, line):
                value, _ = operand(require[1], current)
                code.append(f"MIR.guard ({value} == 0) .empty (\n")
                continuations += 1
            elif phi := re.fullmatch(PHI, line):
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
                comparison = op in ops.COMPARISONS
                operation = (
                    "compare"
                    if comparison
                    else "shift"
                    if op in ops.SHIFTS
                    else "wordBinary"
                    if op in ops.WORD_BINARY
                    else "eval"
                )
                if op in ops.WORD_BINARY and width != 256:
                    raise Unsupported("word operation requires i256 operands")
                code.append(f"let {dest} := MIR.{operation} .{op} {a} {b}")
                current[dest] = (
                    1 if comparison else 256 if op in ops.WORD_BINARY else width
                )
            elif inst := re.fullmatch(UNARY, line):
                dest, op, value = inst.groups()
                value, width = operand(value, current)
                code.append(f"let {dest} := MIR.unary .{op} {value}")
                current[dest] = width
            elif inst := re.fullmatch(TERNARY, line):
                dest, op, a, b, c = inst.groups()
                if op == "select":
                    a, _ = operand(a, current, 1)
                    b, width = operand(b, current)
                    c, _ = operand(c, current, width)
                    code.append(f"let {dest} := MIR.select {a} {b} {c}")
                else:
                    a, _ = operand(a, current, 256)
                    b, _ = operand(b, current, 256)
                    c, _ = operand(c, current, 256)
                    width = 256
                    code.append(f"let {dest} := MIR.ternary .{op} {a} {b} {c}")
                current[dest] = width

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
        elif term == "revert 0, 0":
            result = "Outcome.reverted .empty"
        elif term == "invalid":
            result = "Outcome.invalid"
        else:
            value, _ = operand(term.removeprefix("ret "), current, result_width)
            result = (
                f"Outcome.returned ⟨{value}, memory⟩"
                if memory
                else f"Outcome.returned {value}"
            )
        code.append(result)
        code.extend(")" for _ in range(continuations))
        return "\n".join(code)

    body = enter("bb0", "entry", dict(zip(inputs, input_widths, strict=True)), ())
    if memory:
        body = f"let memory := {mem.INITIAL}\n" + body
        inputs.extend(mem.INPUTS)
        input_widths.extend(mem.WIDTHS)
    return Program(
        (
            header[1],
            phase,
            *params,
            f"i{result_width}",
            interface_attributes(header[4] or ""),
            *(("memory",) if memory else ()),
        ),
        inputs,
        f"Outcome (Memory.Returned {result_width})"
        if memory
        else f"Outcome (BitVec {result_width})",
        body,
        tuple(input_widths),
        memory=memory,
    )


def interface_attributes(text: str) -> str:
    # Coldness is inferred from a body's outcomes, not part of its calling interface.
    # Split only top-level attributes; ABI tuples and arrays retain their contents.
    parts = []
    start = depth = 0
    for index, char in enumerate(text):
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    parts.append(text[start:].strip())
    return ", ".join(
        part for part in parts if part and part not in {"cold", "cold_hint"}
    )
