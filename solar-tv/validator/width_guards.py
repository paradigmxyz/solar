"""Normalize typed width predicates while preserving all effectful producers."""

import re
from collections import Counter

from . import dead_ssa, mir_effects, prune, scalar_checks, scalar_reuse
from .common import Unsupported
from .common import literal as bitvec

FLAG, WORD = scalar_checks.FLAG, scalar_checks.WORD
literal, local = scalar_checks.literal, scalar_checks.local


COMPARISON = re.compile(r"\s*=\s+(gt|lt)\s+([^,]+),\s*(0x[0-9a-fA-F]+|[0-9]+)(?:\s|$)")
SHIFT = re.compile(r"\s*=\s+shr\s+[^,]+,\s*(0x[0-9a-fA-F]+|[0-9]+)(?:\s|$)")


def matches(paths, function):
    """Choose the width certificate before expensive scalar solving."""
    bodies = []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            return False
        try:
            header, entries = prune.declarations(path.read_text())
        except Unsupported:
            return False
        if "@phase lowered" not in header or function not in entries:
            return False
        bodies.append(entries[function]["text"])
    before, after = bodies
    source = Counter()
    target = Counter()
    for lines, bounds in ((before, source), (after, target)):
        for line in lines:
            if comparison := COMPARISON.search(line):
                op, operand, text = comparison.groups()
                bound = int(text, 16 if text.startswith("0x") else 10) + (op == "gt")
                if 1 < bound < 2**256 and not bound & (bound - 1):
                    bounds[operand, bound.bit_length() - 1] += 1
    removed = {amount for _, amount in source - target}
    return bool(removed) and any(
        int(shift[1], 16 if shift[1].startswith("0x") else 10) in removed
        for line in after
        if (shift := SHIFT.search(line))
    )


def zero_operand(op):
    name = "ne" if op["kind"] == "ne" else op.get("name")
    if name not in {"ne", "eq"} or len(op["args"]) != 2:
        return None
    left, right = op["args"]
    if left["type"] != right["type"] or left["type"] not in (FLAG, WORD):
        return None
    zero = literal(0, left["type"])
    if left == zero:
        left, right = right, left
    if right != zero or left["kind"] == "undefined":
        return None
    return left, name == "eq"


def boundary_test(op):
    if op.get("name") not in {"lt", "gt"} or len(op["args"]) != 2:
        return None
    left, right = op["args"] if op["name"] == "lt" else op["args"][::-1]
    if left["type"] != WORD or right["type"] != WORD:
        return None
    if left["kind"] == "constant" and right["kind"] not in {"constant", "undefined"}:
        operand, bound, fits = right, int(left["value"]) + 1, False
    elif right["kind"] == "constant" and left["kind"] not in {"constant", "undefined"}:
        operand, bound, fits = left, int(right["value"]), True
    else:
        return None
    if not 1 < bound < 2**256 or bound & (bound - 1):
        return None
    return operand, bound, fits


def join(kind, *children):
    flattened = [
        leaf
        for child in children
        for leaf in (child[1] if child[0] == kind else [child])
    ]
    # Canonical association and order for guards with the same input leaves.
    return kind, sorted(flattened, key=dead_ssa.stable)


def negate(predicate):
    kind, value = predicate
    if kind in {"and", "or"}:
        return join("and" if kind == "or" else "or", *(negate(p) for p in value))
    if kind == "not":
        return value
    return "not", predicate


class BlockPredicates:
    def __init__(self, instructions, definitions, fork):
        self.definitions = definitions
        self.fork = fork
        self.producers = {
            inst["result"]: inst
            for inst in instructions
            if scalar_reuse.total(inst, definitions, fork)
        }
        self.narrowed = set()
        self.visited = set()

    def producer(self, operand):
        if operand["kind"] == "local":
            return self.producers.get(operand["index"])
        return None

    def flag(self, operand):
        inst = self.producer(operand)
        if inst is None or inst["type"] != FLAG:
            return "flag", operand
        index, op = inst["result"], inst["operation"]
        args = op["args"]
        if match := boundary_test(op):
            word, bound, fits = match
            self.narrowed.add(index)
            self.visited.add(index)
            base = "wide", (word, bound)
            return negate(base) if fits else base
        if test := zero_operand(op):
            value, inverted = test
            self.visited.add(index)
            pred = self.flag(value) if value["type"] == FLAG else self.nonzero(value)
            return negate(pred) if inverted else pred
        if op.get("name") in {"and", "or"} and [v["type"] for v in args] == [
            FLAG,
            FLAG,
        ]:
            self.visited.add(index)
            return join(op["name"], *(self.flag(value) for value in args))
        return "flag", operand

    def nonzero(self, operand):
        inst = self.producer(operand)
        if inst is None or inst["type"] != WORD:
            return "nonzero", operand
        index, op = inst["result"], inst["operation"]
        args = op["args"]
        if op["kind"] == "word" and [v["type"] for v in args] == [FLAG]:
            self.visited.add(index)
            return self.flag(args[0])
        if op.get("name") == "or" and [v["type"] for v in args] == [WORD, WORD]:
            self.visited.add(index)
            return join("or", *(self.nonzero(value) for value in args))
        if op.get("name") == "shr" and [v["type"] for v in args] == [WORD, WORD]:
            amount, value = args
            if amount["kind"] == "constant" and 0 < int(amount["value"]) < 256:
                self.narrowed.add(index)
                self.visited.add(index)
                return "wide", (value, 1 << int(amount["value"]))
        return "nonzero", operand


def equation(
    name, original, rewritten, producers, replacements, complemented, definitions
):
    """Bind the projected guard to the exact typed producer operands on one side."""
    variables = {}

    def term(value, available):
        width = mir_effects.scalar_width(value["type"])
        if value["kind"] == "constant":
            return bitvec(value["value"], width), width
        if value["kind"] == "undefined":
            raise Unsupported("undefined width guard operand")
        inst = available.get(value.get("index")) if value["kind"] == "local" else None
        if inst is None:
            key = dead_ssa.stable(value)
            argument, ty = variables.setdefault(key, (f"input{len(variables)}", width))
            return argument, ty
        op = inst["operation"]
        args = [term(operand, available) for operand in op["args"]]
        expression = mir_effects.pure(op, args, width, definitions)
        if expression is None:
            raise Unsupported("width guard projection includes an effect")
        return f"({expression})", width

    existing = term(original, producers)[0]
    new = term(rewritten, replacements)[0]
    binders = " ".join(f"({arg} : BitVec {width})" for arg, width in variables.values())
    branches = "no yes" if complemented else "yes no"
    bridge = "flip_branch" if complemented else "same_branch"
    return f"""
set_option linter.unusedSimpArgs false in
theorem {name} {{Outcome : Type}} (yes no : Outcome) {binders} :
    MIR.branch {existing} yes no = MIR.branch {new} {branches} := by
  refine MIR.WidthTestFacts.{bridge} _ _ _ _ ?_
  simp [MIR.WidthTestFacts.cmp_eq,MIR.WidthTestFacts.cmp_ne,MIR.WidthTestFacts.cmp_lt,MIR.WidthTestFacts.cmp_gt,
    MIR.eval,MIR.shift,Word.shift,Word.binary,BitVec.ofNatLT_eq_ofNat,
    BitVec.or_eq_zero_iff,MIR.WidthTestFacts.zero_shift,MIR.WidthTestFacts.widen_zero,
    MIR.WidthTestFacts.flag_zero,MIR.WidthTestFacts.flag_or,MIR.WidthTestFacts.flag_and,
    Classical.not_and_iff_not_or_not,not_or,Classical.not_not,
    or_comm,or_left_comm,or_assoc,and_comm,and_left_comm,and_assoc] <;> first | omega | grind
#print axioms {name}
"""


def normalize(body, definitions, fork, prefix):
    """Project recognized pure conditions at the point where their branch reads them."""
    originals = body["blocks"]
    blocks = []
    defs = {}
    stats = Counter()
    proofs = []
    index = (
        max(
            (
                inst["result"]
                for block in originals
                for inst in block["instructions"] + block["phis"]
                if inst["result"] is not None
            ),
            default=0,
        )
        + 1
    )

    for block_index, block in enumerate(originals):
        instructions = list(block["instructions"])
        term = block["terminator"]
        if term["kind"] != "branch":
            blocks.append({**block, "instructions": instructions})
            continue
        logic = BlockPredicates(instructions, definitions, fork)
        formula = logic.flag(term["args"][0])
        if not logic.narrowed:
            blocks.append({**block, "instructions": instructions})
            continue

        opposite = negate(formula)
        complemented = dead_ssa.stable(opposite) < dead_ssa.stable(formula)
        if complemented:
            formula = opposite
            term = {**term, "then": term["else"], "else": term["then"]}

        def emit(op, instructions=instructions):
            nonlocal index
            result = local(index, FLAG)
            instructions.append({"result": index, "type": FLAG, "operation": op})
            index += 1
            return result

        def build(expr):
            kind, value = expr
            match kind:
                case "flag":
                    return value
                case "wide":
                    word, bound = value
                    return emit(
                        {
                            "kind": "opcode",
                            "name": "lt",
                            "args": [literal(bound - 1, WORD), word],
                        }
                    )
                case "nonzero":
                    return emit({"kind": "ne", "args": [value, literal(0, WORD)]})
                case "not":
                    return emit(
                        {
                            "kind": "opcode",
                            "name": "eq",
                            "args": [build(value), literal(0, FLAG)],
                        }
                    )
                case "and" | "or":
                    args = [build(item) for item in value]
                    result = args[0]
                    for other in args[1:]:
                        result = emit(
                            {"kind": "opcode", "name": kind, "args": [result, other]}
                        )
                    return result
                case _:
                    raise AssertionError(f"invalid predicate kind: {kind}")

        original_producers = {key: logic.producers[key] for key in logic.visited}
        initial_count = len(instructions)
        condition = build(formula)
        generated = {inst["result"]: inst for inst in instructions[initial_count:]}
        proofs.append(
            equation(
                f"{prefix}Block{block_index}",
                block["terminator"]["args"][0],
                condition,
                original_producers,
                generated,
                complemented,
                definitions,
            )
        )
        defs.update(original_producers)
        blocks.append(
            {
                **block,
                "instructions": instructions,
                "terminator": {**term, "args": [condition]},
            }
        )
        stats["width_leaves"] += len(logic.narrowed)
        stats["width_branches"] += 1

    scalar_reuse.erase_dead(blocks, defs, stats)
    return {**body, "blocks": blocks}, dict(stats), "".join(proofs)
