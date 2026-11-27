"""Relate counted MIR loops to two carried cursors and an address sentinel."""

from dataclasses import dataclass
from itertools import count

from . import dead_ssa, guarded_widths, scalar_checks, scalar_reuse

WORD, FLAG = scalar_checks.WORD, scalar_checks.FLAG
literal, local = scalar_checks.literal, scalar_checks.local
word_constant = guarded_widths.word_constant


@dataclass(frozen=True)
class Address:
    base: dict | None
    bias: int
    stride: int
    expression: str
    erased: frozenset[int] = frozenset()


def affine(value, index, latch, invariant, budget=12):
    if value["type"] != WORD or budget == 0:
        return None
    if value == index:
        return Address(None, 0, 1, "index")
    if (word := word_constant(value)) is not None:
        return Address(None, word, 0, f"{word}#256")
    if invariant(value):
        return Address(value, 0, 0, "base")
    op = latch.get(value.get("index")) if value["kind"] == "local" else None
    if not op or op["kind"] != "opcode" or op["name"] not in {"add", "shl"}:
        return None
    left, right = op["args"]
    if op["name"] == "shl":
        amount = word_constant(left)
        inner = affine(right, index, latch, invariant, budget - 1)
        if amount != 5 or inner is None or inner.base is not None:
            return None
        return Address(
            None,
            inner.bias * 32,
            inner.stride * 32,
            f"(MIR.shift .shl {inner.expression} 5)",
            inner.erased | {value["index"]},
        )
    a = affine(left, index, latch, invariant, budget - 1)
    b = affine(right, index, latch, invariant, budget - 1)
    if a is None or b is None or a.base is not None and b.base is not None:
        return None
    return Address(
        a.base or b.base,
        a.bias + b.bias,
        a.stride + b.stride,
        f"(MIR.eval .add {a.expression} {b.expression})",
        a.erased | b.erased | {value["index"]},
    )


def accepted_count(blocks, guards, length, preheader, stride):
    if stride == 1:
        return ()
    definition, _ = guards.definition(length)
    for origin, block in enumerate(blocks):
        term = block["terminator"]
        if term["kind"] != "branch" or term["then"] == term["else"]:
            continue
        owner, condition = guards.definition(term["args"][0])
        if (
            owner != origin
            or condition.get("name") != "lt"
            or condition.get("kind") != "opcode"
            or condition["args"][1] != length
        ):
            continue
        room, _ = condition["args"]
        owner, shift = guards.definition(room)
        if (
            owner == origin
            and shift.get("kind") == "opcode"
            and shift.get("name") == "shr"
            and shift["args"][0] == literal(5, WORD)
            and shift["args"][1]["kind"] != "undefined"
        ):
            site = guards.required(definition, preheader, (origin, term["else"]))
            if site is not None:
                return site
    return None


def certificate(name, cursors, guards, header, preheader, latch, site):
    stride, stride2 = [cursor.stride for cursor in cursors]
    edges = ",".join(
        f"({parent},{target})"
        for parent, targets in enumerate(guards.successors)
        for target in targets
    )
    incoming = ",".join(f"({parent},{header})" for parent in sorted((preheader, latch)))
    code = f"""
def {name}Edges : List MIR.GuardedWidths.Edge := [{edges}]
theorem {name}EntryEdges :
    {name}Edges.filter (fun e => e.2 == {header}) = [{incoming}] ∧
    {name}Edges.filter (fun e => e.2 == {latch}) = [({header},{latch})] := by decide +kernel
#print axioms {name}EntryEdges
"""
    premise = ""
    fits = "(by simpa using limit.isLt)"
    if site:
        definition, use, (source, target), outside = site
        code += f"""
def {name}Site : MIR.GuardedWidths.Site := ⟨{definition},{use},({source},{target}),{list(outside)}⟩
theorem {name}BoundPath : ¬ MIR.GuardedWidths.Bypass {name}Edges {name}Site {use} :=
  MIR.GuardedWidths.sound {name}Edges {name}Site (by decide +kernel)
#print axioms {name}BoundPath
"""
        premise = """ (remaining : Word)
    (admitted : MIR.compare .lt (MIR.shift .shr remaining 5) limit = 0)"""
        fits = "(by exact MIR.AffineLoop.calldataStrideBound remaining limit admitted)"
    for i, cursor in enumerate(cursors):
        tag = f"{name}Address{i}"
        code += f"""
theorem {tag} (base index : Word) :
    {cursor.expression} = MIR.AffineLoop.address (base + {cursor.bias}#256) {cursor.stride} index := by
  simp [MIR.AffineLoop.address, MIR.eval, MIR.shift, Word.shift,
    BitVec.shiftLeft_eq_mul_twoPow, BitVec.add_mul, -BitVec.mul_twoPow_eq_shiftLeft]
  <;> ac_rfl
#print axioms {tag}
"""
    code += f"""
theorem {name}Walk {{State Event Failure : Type}} (base second limit : Word){premise}
    (effect : MIR.AffineLoop.PairedEffect State Event Failure) (fuel : Nat) (state : State) :
    let bases : MIR.AffineLoop.CursorPair := ⟨base + {cursors[0].bias}#256,second + {cursors[1].bias}#256⟩
    let strides : MIR.AffineLoop.CursorPair := ⟨{stride},{stride2}⟩
    MIR.LoopSimulation.observe (MIR.AffineLoop.indexedPair bases strides limit effect)
        fuel ⟨0,bases,state⟩ =
      MIR.LoopSimulation.observe
        (MIR.AffineLoop.walkedPair strides (MIR.AffineLoop.address bases.first strides.first limit) effect)
        fuel ⟨0,bases,state⟩ :=
  MIR.AffineLoop.pairedEntry _ _ limit (by decide) {fits} effect fuel state
#print axioms {name}Walk
"""
    return code


def project(before, after, name):
    blocks = before["blocks"]
    if len(blocks) != len(after["blocks"]) or len(blocks) > 1024:
        return None
    guards = guarded_widths.Guards(blocks)
    dominators, predecessors = scalar_reuse.dominance(guards.successors)
    for header, head in enumerate(blocks):
        if result := match_loop(
            before, after, name, guards, dominators, predecessors, header, head
        ):
            return result
    return None


def match_loop(before, after, name, guards, dominators, predecessors, header, head):
    blocks = before["blocks"]
    term = head["terminator"]
    if (
        term["kind"] != "branch"
        or len(head["phis"]) != 1
        or len(head["instructions"]) != 1
    ):
        return None
    latch = term["then"]
    (phi,) = head["phis"]
    (guard,) = head["instructions"]
    compare = guard["operation"]
    if (
        phi["type"] != WORD
        or guard["type"] != FLAG
        or compare.get("kind") != "opcode"
        or compare.get("name") != "lt"
        or term["args"] != [local(guard["result"], FLAG)]
        or latch in (header, term["else"], 0)
        or header == 0
        or header not in dominators
        or predecessors[latch] != {header}
    ):
        return None
    incoming = dict(phi["incoming"])
    entries = incoming.keys() - {latch}
    if (
        len(entries) != 1
        or predecessors[header] != incoming.keys()
        or latch not in incoming
    ):
        return None
    (preheader,) = entries
    if (
        incoming[preheader] != literal(0, WORD)
        or {preheader: guards.successors[preheader], latch: guards.successors[latch]}
        != {preheader: [header], latch: [header]}
        or blocks[latch]["phis"]
    ):
        return None
    index, length = compare["args"]
    if index != local(phi["result"], WORD):
        return None

    def invariant(operand):
        if operand["kind"] == "arg" or operand["kind"] == "constant":
            return True
        return (
            operand["kind"] == "local"
            and operand["index"] in guards.definitions
            and guards.definition(operand)[0] in dominators[preheader]
            and guards.definition(operand)[0] not in (header, latch)
        )

    if length["type"] != WORD or not invariant(length):
        return None
    step = incoming[latch]
    updates = {
        inst["result"]: inst["operation"]
        for inst in blocks[latch]["instructions"]
        if inst["result"] is not None
    }
    if (
        step["kind"] != "local"
        or step["type"] != WORD
        or updates.get(step["index"])
        != {
            "kind": "opcode",
            "name": "add",
            "args": [index, literal(1, WORD)],
        }
    ):
        return None

    fresh = count(max(guards.definitions, default=0) + 1)
    prelude, phis, advances = [], [], []
    retained, cursors = [], []
    removed = {step["index"], index["index"], guard["result"]}

    def emit(into, kind, opcode, args, ty=WORD):
        dst = next(fresh)
        op = {"kind": kind, "args": args}
        if opcode is not None:
            op["name"] = opcode
        into.append({"result": dst, "type": ty, "operation": op})
        return local(dst, ty)

    for inst in blocks[latch]["instructions"]:
        op = inst["operation"]
        if op["kind"] == "opcode" and op["name"] in {"calldataload", "sload", "mstore"}:
            address = affine(op["args"][0], index, updates, invariant)
            if address is not None and address.stride:
                if (
                    address.base is None
                    or address.stride not in (1, 32)
                    or len(cursors) == 2
                ):
                    return None
                cursors.append(address)
                removed.update(address.erased)
                start = address.base
                if address.bias:
                    start = emit(
                        prelude, "opcode", "add", [start, literal(address.bias, WORD)]
                    )
                dst = next(fresh)
                current = local(dst, WORD)
                advance = emit(
                    advances, "opcode", "add", [current, literal(address.stride, WORD)]
                )
                phis.append(
                    {
                        "result": dst,
                        "type": WORD,
                        "incoming": [[preheader, start], [latch, advance]],
                    }
                )
                inst = {**inst, "operation": {**op, "args": [current, *op["args"][1:]]}}
        retained.append(inst)
    if len(cursors) != 2 or cursors[0].bias:
        return None
    site = accepted_count(blocks, guards, length, preheader, cursors[0].stride)
    if site is None:
        return None

    span = (
        length
        if cursors[0].stride == 1
        else emit(prelude, "opcode", "shl", [literal(5, WORD), length])
    )
    end = emit(prelude, "opcode", "add", [cursors[0].base, span])
    tests = []
    test = emit(tests, "ne", None, [end, local(phis[0]["result"], WORD)], FLAG)
    projected = list(blocks)
    projected[preheader] = {
        **blocks[preheader],
        "instructions": blocks[preheader]["instructions"] + prelude,
    }
    projected[header] = {
        "phis": phis,
        "instructions": tests,
        "terminator": {**term, "args": [test]},
    }
    projected[latch] = {
        **blocks[latch],
        "instructions": [inst for inst in retained if inst["result"] not in removed]
        + advances,
    }
    if removed.intersection(dead_ssa.reads(projected, reject_undefined=False)):
        return None
    if not dead_ssa.same_definitions({**before, "blocks": projected}, after):
        return None
    return (
        {
            "header": header,
            "latch": latch,
            "strides": [cursor.stride for cursor in cursors],
        },
        certificate(name, cursors, guards, header, preheader, latch, site),
    )
