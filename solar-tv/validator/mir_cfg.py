"""Propose path and live-local correspondence for independently exported MIR."""

import hashlib
import tempfile
from collections import Counter, defaultdict, deque
from dataclasses import dataclass
from itertools import batched
from pathlib import Path

from . import (
    audit,
    dead_ssa,
    entry_proof,
    guarded_widths,
    heap_cursor,
    mir_state,
    native,
    phase_carriers,
    phi_edges,
    private_cursor,
    prune,
    scalar_checks,
    scalar_reuse,
    state_read_reuse,
    type_prune,
    width_guards,
)
from .common import Program, Unsupported


@dataclass(frozen=True)
class IncrementGuard:
    argument: int
    sum: int
    guard: int
    uses_previous: bool


@dataclass(frozen=True)
class HighMask:
    argument: int
    result: int
    complement: bool


@dataclass(frozen=True)
class RoundedSize:
    length: int
    result: int
    combined: bool


@dataclass(frozen=True)
class RoundedExtent:
    length: int
    results: tuple[int, int, int, int]
    division: bool
    checked: bool


WORD = {"kind": "word", "bits": 256, "name": "i256"}
FLAG = {"kind": "word", "bits": 1, "name": "i1"}


def word_constant(value):
    return {"kind": "constant", "type": WORD, "value": str(value)}


def word_local(index):
    return {"kind": "local", "type": WORD, "index": index}


def increment_guard(instructions, position):
    """Bind both outputs of an adjacent i256 increment and overflow test."""
    if position + 1 >= len(instructions):
        return None
    add, guard = instructions[position : position + 2]
    if word_key(add) != ("add", 1) or guard["type"] != FLAG or guard["result"] is None:
        return None
    op = guard["operation"]
    if op["kind"] != "opcode":
        return None
    original = add["operation"]["args"][0]
    wrapped, zero = word_local(add["result"]), word_constant(0)
    if op["name"] == "lt" and op["args"] == [wrapped, original]:
        uses_previous = True
    elif op["name"] == "eq" and op["args"] in ([wrapped, zero], [zero, wrapped]):
        uses_previous = False
    else:
        return None
    return IncrementGuard(
        original["index"], add["result"], guard["result"], uses_previous
    )


def word_operands(data, name):
    op = data["operation"]
    if (
        data["type"] == WORD
        and data["result"] is not None
        and op["kind"] == "opcode"
        and op["name"] == name
        and all(operand["type"] == WORD for operand in op["args"])
    ):
        # The schema gives shifts in EVM operand order, with the count first.
        return op["args"][::-1] if name in {"shl", "shr"} else op["args"]
    return ()


def high_mask(instructions, position, uses):
    """Match a high-bit mask with two private temporaries in its complement form."""
    first = instructions[position]
    operands = word_operands(first, "shl")
    if len(operands) != 2 or operands[1]["kind"] != "local":
        return None
    base, exponent = operands
    if base == word_constant(2**256 - 1):
        return HighMask(exponent["index"], first["result"], False), 1
    if base != word_constant(1) or position + 2 >= len(instructions):
        return None
    subtract, invert = instructions[position + 1 : position + 3]
    if uses[first["result"]] != 1:
        return None
    if word_operands(subtract, "sub") != [
        word_local(first["result"]),
        word_constant(1),
    ]:
        return None
    if uses[subtract["result"]] == 1 and word_operands(invert, "not") == [
        word_local(subtract["result"])
    ]:
        return HighMask(exponent["index"], invert["result"], True), 3
    # Egraph can leave the old total scalar temporaries for a later DCE.
    if uses[subtract["result"]] == 0 and word_operands(invert, "shl") == [
        word_constant(2**256 - 1),
        exponent,
    ]:
        return HighMask(exponent["index"], invert["result"], False), 3
    return None


def rounded_size(instructions, position, uses):
    """Recognize either word-rounded size with no live intermediate."""
    if position + 1 >= len(instructions):
        return None
    add, mask = instructions[position : position + 2]
    operands = word_operands(add, "add")
    if len(operands) != 2:
        return None
    length, amount = operands
    if length["kind"] != "local" or uses[add["result"]] != 1:
        return None
    rounded, low_clear = word_local(add["result"]), word_constant(2**256 - 32)
    if word_operands(mask, "and") not in ([rounded, low_clear], [low_clear, rounded]):
        return None
    if amount == word_constant(63):
        return RoundedSize(length["index"], mask["result"], True), 2
    if amount != word_constant(31) or position + 2 >= len(instructions):
        return None
    extent = instructions[position + 2]
    if uses[mask["result"]] == 1 and word_operands(extent, "add") == [
        word_local(mask["result"]),
        word_constant(32),
    ]:
        return RoundedSize(length["index"], extent["result"], False), 3
    return None


def shifted_length(value, definitions):
    source = definitions.get(value.get("index")) if value["kind"] == "local" else None
    if source is None:
        return False
    shift = word_operands(source, "shr")
    return (
        len(shift) == 2
        and shift[1]["kind"] == "constant"
        and int(shift[1]["value"]) > 0
    )


def incoming_edges(blocks):
    parents = defaultdict(list)
    for index, block in enumerate(blocks):
        for target in phi_edges.targets(block["terminator"]):
            parents[target].append(index)
    return parents


def edge_definition(name, parents):
    edges = ",".join(
        f"({parent},{target})"
        for target, sources in parents.items()
        for parent in sources
    )
    return f"def {name}Edges : List (Nat × Nat) := [{edges}]\n"


def false_disjuncts(body, name):
    """Bind a false OR operand through a chain of sole predecessors."""
    blocks = body["blocks"]
    parents = incoming_edges(blocks)
    updated = list(blocks)
    proofs = []
    for index, block in enumerate(blocks):
        term = block["terminator"]
        if term["kind"] != "branch" or term["args"][0]["kind"] != "local":
            continue
        (condition,) = term["args"]
        disjunction = next(
            (
                inst["operation"]
                for inst in block["instructions"]
                if inst["result"] == condition["index"] and inst["type"] == FLAG
            ),
            {},
        )
        if disjunction.get("kind") != "opcode" or disjunction.get("name") != "or":
            continue
        args = disjunction["args"]
        if len(args) != 2 or any(
            arg["type"] != FLAG or arg["kind"] != "local" for arg in args
        ):
            continue
        cursor = index
        visited = {index}
        chain = []
        while cursor != 0 and len(parents[cursor]) == 1:
            (parent,) = parents[cursor]
            if parent in visited:
                break
            visited.add(parent)
            chain.append((parent, cursor))
            previous = blocks[parent]["terminator"]
            if (
                previous["kind"] == "branch"
                and previous["else"] == cursor
                and previous["then"] != cursor
                and previous["args"][0] in args
                and any(
                    inst["result"] == previous["args"][0]["index"]
                    for inst in blocks[parent]["instructions"]
                )
            ):
                left = args[0] == previous["args"][0]
                updated[index] = {
                    **block,
                    "terminator": {**term, "args": [args[int(left)]]},
                }
                site = f"{name}_{index}"
                path = ",".join(f"({a},{b})" for a, b in chain)
                proofs.append(f"""
theorem {site}Path : ([{path}] : List (Nat × Nat)).all
    (fun (a,b) => {name}Edges.filter (fun edge => edge.2 == b) == [(a,b)]) = true := by decide
theorem {site} {{Result : Type}} (flag : BitVec 1) (yes no : Result) :
    MIR.branch (MIR.eval .or {"0 flag" if left else "flag 0"}) yes no =
      MIR.branch flag yes no :=
  MIR.ScalarChecks.false_disjunct flag {str(left).lower()} yes no
#print axioms {site}Path
#print axioms {site}
""")
                break
            cursor = parent
    return (
        {**body, "blocks": updated},
        len(proofs),
        edge_definition(name, parents) + "".join(proofs) if proofs else "",
    )


def bounded_increments(body, name):
    """Project increment guards on a unique false edge of a typed size check."""
    blocks = body["blocks"]
    projected = list(blocks)
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    proofs, sites = [], []
    for index, (parent, comparison, _, _) in scalar_checks.preceding_comparisons(
        blocks
    ).items():
        block, predecessor = blocks[index], blocks[parent]
        edge, term = predecessor["terminator"], block["terminator"]
        if edge["else"] != index or edge["then"] == index:
            continue
        room, length = comparison["args"]
        if room["kind"] != "local" or length["kind"] != "local":
            continue
        shift = next(
            (
                word_operands(inst, "shr")
                for inst in predecessor["instructions"]
                if inst["result"] == room["index"]
            ),
            (),
        )
        if (
            len(shift) != 2
            or shift[0]["kind"] == "undefined"
            or shift[1]["kind"] != "constant"
            or not 0 < int(shift[1]["value"]) < 2**256
        ):
            continue
        instructions = block["instructions"]
        if not (
            len(instructions) >= 2
            and (guard := increment_guard(instructions, len(instructions) - 2))
            and not guard.uses_previous
            and guard.argument == length["index"]
        ):
            continue
        if term["kind"] == "branch" and (
            term["args"] == [scalar_checks.local(guard.guard, FLAG)]
            and uses[guard.guard] == 1
        ):
            amount = int(shift[1]["value"])
            site = f"{name}_{index}"
            proofs.append(f"""
theorem {site} {{Result : Type}} (word length : Word) (rejected overflow ok : Result) :
    MIR.branch (MIR.compare .lt (MIR.shift .shr word {amount}) length) rejected
      (MIR.branch (MIR.compare .eq (MIR.eval .add length 1) 0) overflow ok) =
    MIR.branch (MIR.compare .lt (MIR.shift .shr word {amount}) length) rejected ok :=
  MIR.ScalarChecks.bounded_increment word {amount} length (by decide) rejected overflow ok
theorem {site}Edge : {name}Edges.filter (fun edge => edge.2 == {index}) = [({parent},{index})] := by decide
#print axioms {site}
#print axioms {site}Edge
""")
            sites.append([parent, index, amount])
            term = {"kind": "jump", "args": [], "target": term["else"]}
        elif term["kind"] != "jump" or uses[guard.guard]:
            continue
        projected[index] = {
            **block,
            "instructions": instructions[:-1],
            "terminator": term,
        }
    code = edge_definition(name, incoming_edges(blocks)) if proofs else ""
    return {**body, "blocks": projected}, sites, code + "".join(proofs)


def checks_flag(inst, flag, *, zero=False):
    op = inst["operation"]
    return (
        inst["result"] is None
        and op["kind"] == "check"
        and op["zero"] == zero
        and op["args"] == [flag]
    )


def entry_length_limits(instructions):
    limits = {}
    for position, head in enumerate(instructions[:-2]):
        guard = increment_guard(instructions, position)
        if (
            not guard
            or not guard.uses_previous
            or not checks_flag(
                instructions[position + 2], scalar_checks.local(guard.guard, FLAG)
            )
        ):
            continue
        result = word_local(head["result"])
        for comparison, check in zip(
            instructions[position + 3 :], instructions[position + 4 :]
        ):
            flag, operation = comparison["result"], comparison["operation"]
            if (
                flag is not None
                and comparison["type"] == FLAG
                and operation.get("name") == "gt"
                and len(operation["args"]) == 2
                and operation["args"][0] == result
                and (limit := operation["args"][1])["kind"] == "constant"
                and limit["type"] == WORD
                and 0 <= int(limit["value"]) < 2**255
                and checks_flag(check, scalar_checks.local(flag, FLAG))
            ):
                limits[guard.argument] = limit
                break
    return limits


def bounded_indices(body, name):
    """Retain entry checks and use their size bound on the loop's true edge."""
    blocks = body["blocks"]
    parents = incoming_edges(blocks)
    entry = blocks[0]
    if parents[0] or entry["terminator"]["kind"] != "jump":
        return body, 0, ""
    limits = entry_length_limits(entry["instructions"])
    if not limits:
        return body, 0, ""
    header_id = entry["terminator"]["target"]
    header = blocks[header_id]
    term = header["terminator"]
    if term["kind"] != "branch" or term["then"] in (0, header_id):
        return body, 0, ""
    cursor_id = term["then"]
    if parents[cursor_id] != [header_id]:
        return body, 0, ""
    (flag,) = term["args"]
    test = next(
        (
            inst["operation"]
            for inst in header["instructions"]
            if flag == scalar_checks.local(inst["result"], FLAG)
            and inst["type"] == FLAG
        ),
        {},
    )
    if test.get("name") != "lt" or len(test["args"]) != 2:
        return body, 0, ""
    index, length = test["args"]
    limit = (
        limits.get(length.get("index"))
        if length["kind"] == "local" and length["type"] == WORD
        else None
    )
    if not limit or index["kind"] != "local" or index["type"] != WORD:
        return body, 0, ""
    block = blocks[cursor_id]
    too_wide = {
        inst["result"]
        for inst in block["instructions"]
        if inst["result"] is not None
        and inst["type"] == FLAG
        and inst["operation"]
        == {"kind": "opcode", "name": "gt", "args": [index, limit]}
    }
    instructions, changes = [], 0
    for inst in block["instructions"]:
        if checks_flag(inst, flag, zero=True) or any(
            checks_flag(inst, scalar_checks.local(result, FLAG)) for result in too_wide
        ):
            changes += 1
            continue
        args = [index, word_constant(1)]
        if inst["type"] == WORD and inst["operation"] == {
            "kind": "checked",
            "operation": "Add",
            "signed": False,
            "bits": 256,
            "args": args,
        }:
            inst = {
                **inst,
                "operation": {"kind": "opcode", "name": "add", "args": args},
            }
            changes += 1
        instructions.append(inst)
    if not changes:
        return body, 0, ""
    projected = list(blocks)
    projected[cursor_id] = {**block, "instructions": instructions}
    code = (
        edge_definition(name, parents)
        + f"""
theorem {name}Edge : {name}Edges.filter (fun edge => edge.2 == 0) = [] ∧
    {name}Edges.filter (fun edge => edge.2 == {cursor_id}) = [({header_id},{cursor_id})] := by decide
theorem {name}Limit : ( {limit["value"]}#256 ).toNat < 2^255 := by decide
#print axioms {name}Edge
#print axioms {name}Limit
#print axioms MIR.ScalarChecks.bounded_index
"""
    )
    return {**body, "blocks": projected}, changes, code


def rounded_extent(block, position, uses, definitions):
    """Check a block suffix that computes allocation size and dispatches on overflow."""
    term = block["terminator"]
    instructions = block["instructions"]
    if (
        term["kind"] not in {"branch", "jump"}
        or not 4 <= len(instructions) - position <= 6
    ):
        return None
    sum_, quotient, *tail = instructions[position:]
    total = word_operands(sum_, "add")
    if len(total) != 2 or total[0]["kind"] != "local" or total[1] != word_constant(31):
        return None
    length = total[0]
    source = definitions.get(length["index"])
    if source is None or not (
        shifted_length(length, definitions)
        or any(
            shifted_length(operand, definitions)
            for operand in word_operands(source, "and")
        )
    ):
        return None

    rounded = word_local(sum_["result"])
    division = word_operands(quotient, "div") == [rounded, word_constant(32)]
    if not division and word_operands(quotient, "shr") != [rounded, word_constant(5)]:
        return None
    # The division and alignment may share the sum or compute it twice.
    if word_operands(tail[0], "add") == total:
        if len(tail) < 3 or uses[tail[0]["result"]] != 1:
            return None
        rounded = word_local(tail.pop(0)["result"])
    aligned, extent, *guard = tail
    if word_operands(aligned, "and") != [rounded, word_constant(2**256 - 32)] or (
        word_operands(extent, "add")
        != [word_local(aligned["result"]), word_constant(32)]
    ):
        return None
    if term["kind"] == "jump" and guard:
        return None
    if len(guard) == 1:
        comparison = guard[0]
        result = comparison["result"]
        if comparison["type"] != FLAG or result is None or uses[result] != 1:
            return None
        expected = {"kind": "local", "type": FLAG, "index": result}
        if comparison["operation"] != {
            "kind": "opcode",
            "name": "lt",
            "args": [word_local(extent["result"]), length],
        }:
            return None
    elif not guard:
        expected = {"kind": "constant", "type": FLAG, "value": "0"}
    else:
        return None
    if term["kind"] == "branch" and term["args"] != [expected]:
        return None
    results = (sum_["result"], quotient["result"], aligned["result"], extent["result"])
    return RoundedExtent(length["index"], results, division, bool(guard))


def import_graph(body, *, combine=True, threaded=()):
    """Keep predecessor-selected phi inputs as one simultaneous action."""
    blocks = body["blocks"]
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    definitions = {
        inst["result"]: inst
        for block in blocks
        for inst in block["instructions"]
        if inst["result"] is not None
    }
    points, nodes = {(0, None, -1): 0}, []
    pending = deque(points)

    def point(block, predecessor, position=-1):
        if not blocks[block]["phis"] or position >= 0:
            predecessor = None
        location = (block, predecessor, position)
        if location not in points:
            if len(points) >= 4096:
                raise Unsupported("MIR control-flow graph exceeds 4096 points")
            points[location] = len(points)
            pending.append(location)
        return points[location]

    def branch_node(term, index):
        edges = [point(term[k], index) for k in ("then", "else")]
        (condition,) = term["args"]
        if (
            condition["kind"] == "constant"
            and condition["type"] == FLAG
            and condition["value"] in {"0", "1"}
        ):
            return "literal", condition["value"] == "1", edges
        keys = {k: v for k, v in term.items() if k not in {"then", "else"}}
        return "select", keys, edges

    while pending:
        index, predecessor, position = pending.popleft()
        block = blocks[index]
        if position == -1:
            phis = []
            for phi in block["phis"]:
                choices = [
                    value for source, value in phi["incoming"] if source == predecessor
                ]
                if len(choices) != 1:
                    raise Unsupported("MIR control flow has an unresolved phi input")
                phis.append({**phi, "incoming": choices[0]})
            if index in threaded:
                (guard,) = phis
                term = {**block["terminator"], "args": [guard["incoming"]]}
                nodes.append(branch_node(term, index))
                continue
            next = point(index, None, 0)
            nodes.append(
                ("action", phis, [next]) if phis else ("forward", None, [next])
            )
        elif position < len(block["instructions"]):
            instructions = block["instructions"]
            if combine and (guard := increment_guard(instructions, position)):
                nodes.append(("action", guard, [point(index, None, position + 2)]))
            elif combine and (mask := high_mask(instructions, position, uses)):
                data, length = mask
                nodes.append(("action", data, [point(index, None, position + length)]))
            elif combine and (
                extent := rounded_extent(block, position, uses, definitions)
            ):
                term = block["terminator"]
                successor = term["else"] if term["kind"] == "branch" else term["target"]
                nodes.append(("action", extent, [point(successor, index)]))
            elif combine and (size := rounded_size(instructions, position, uses)):
                data, length = size
                nodes.append(("action", data, [point(index, None, position + length)]))
            else:
                inst = instructions[position]
                nodes.append(("action", inst, [point(index, None, position + 1)]))
        else:
            term = block["terminator"]
            match term["kind"]:
                case "jump":
                    nodes.append(("forward", None, [point(term["target"], index)]))
                case "branch":
                    nodes.append(branch_node(term, index))
                case "switch":
                    keys = {**term, "cases": [key for key, _ in term["cases"]]}
                    del keys["default"]
                    targets = [
                        point(term["default"], index),
                        *(point(dest, index) for _, dest in term["cases"]),
                    ]
                    nodes.append(("select", keys, targets))
                case _:
                    nodes.append(("terminal", term, []))
    return nodes


def threadable_guards(body):
    reads = Counter(dead_ssa.reads(body["blocks"], reject_undefined=False))
    guards = {}
    for index, block in enumerate(body["blocks"]):
        term = block["terminator"]
        if len(block["phis"]) != 1 or block["instructions"] or term["kind"] != "branch":
            continue
        (phi,) = block["phis"]
        result = phi["result"]
        if (
            phi["type"] == FLAG
            and reads[result] == 1
            and term["args"] == [{"kind": "local", "index": result, "type": FLAG}]
            and all(
                operand["type"] == FLAG and operand["kind"] != "undefined"
                for _, operand in phi["incoming"]
            )
        ):
            guards[index] = phi
    return guards, reads


def phi_guard_certificate(name, guards, reads):
    source = ""
    for index, phi in guards.items():
        tag = f"{name}_{index}"
        incoming = [
            (pred, mir_state.operand(operand)) for pred, operand in phi["incoming"]
        ]
        edges = ",".join(f"({pred},{operand})" for pred, operand in incoming)
        result = phi["result"]
        kept_reads = reads.copy()
        kept_reads[result] -= 1
        other_reads = sorted(local for local, count in kept_reads.items() if count)
        source += f"""
def {tag} : MIR.State.Phi := ⟨{result}, .word 1, [{edges}]⟩
theorem {tag}_unread (registers : Nat → MIR.State.Value) (value : MIR.State.Value) :
    {other_reads}.map (fun id => if id = {result} then value else registers id) =
      {other_reads}.map registers :=
  MIR.PhiThread.untouched_reads registers {result} value {other_reads} (by decide +kernel)
#print axioms {tag}_unread
"""
        for arm, (pred, operand) in enumerate(incoming):
            theorem = f"{tag}_input{arm}"
            source += f"""
theorem {theorem} (frame : MIR.State.Frame) :
    (do let next ← MIR.State.enter (some {pred}) [{tag}] frame
        MIR.State.readBits 1 next ⟨.local {result}, .word 1⟩) =
    MIR.State.readBits 1 frame ({operand}) :=
  MIR.PhiThread.read_branch frame {pred} {tag} ({operand}) (by rfl) rfl rfl
#print axioms {theorem}
"""
    return source


def flat_insert(data):
    if (
        not isinstance(data, dict)
        or data.get("operation", {}).get("kind") != "insert_value"
    ):
        return None
    op = data["operation"]
    ty, index = op["aggregate_type"], op["index"]
    base, value = op["args"]
    if (
        ty["kind"] == "product"
        and data["type"] == base["type"] == ty
        and all(field["kind"] == "word" for field in ty["fields"])
        and 0 <= index < len(ty["fields"])
        and value["type"] == ty["fields"][index]
        and base["kind"] in {"undefined", "local"}
    ):
        return base, value, len(ty["fields"]), index
    return None


def reads(data):
    if isinstance(data, (IncrementGuard, HighMask)):
        return [data.argument]
    if isinstance(data, (RoundedExtent, RoundedSize)):
        return [data.length]
    if word_key(data) == ("double", 1):
        return [data["operation"]["args"][0]["index"]]
    insert = flat_insert(data)
    if insert is not None and insert[0]["kind"] == "undefined":
        return dead_ssa.reads(insert[1])
    return dead_ssa.reads(data)


def product_certificate(name, nodes):
    """Bind the checked field masks to inserts and every other local read."""
    seeds, children = [], {}
    for kind, data, _ in nodes:
        insert = flat_insert(data) if kind == "action" else None
        if insert is not None:
            base = insert[0]
            if base["kind"] == "undefined":
                seeds.append((data, insert))
            else:
                children.setdefault(base["index"], []).append((data, insert))
    if not seeds:
        return ""
    inserts, constructing, pending = [], set(), deque(seeds)
    while pending:
        data, (base, _, arity, index) = pending.popleft()
        result = data["result"]
        if result in constructing:
            raise Unsupported("MIR product construction repeats a result")
        constructing.add(result)
        source = "none" if base["kind"] == "undefined" else f"some {base['index']}"
        inserts.append(f"⟨{result},{source},{arity},{index}⟩")
        pending.extend(children.get(result, ()))
    observed = set()
    for kind, data, _ in nodes:
        if kind in {"forward", "literal"}:
            continue
        if (
            kind == "action"
            and isinstance(data, dict)
            and data["result"] in constructing
        ):
            # An insert copies its base without observing undefined fields.
            # Its scalar operand must be initialized.
            observed.update(dead_ssa.reads(data["operation"]["args"][1]))
        else:
            observed.update(reads(data))
    return f"""
theorem {name} : MIR.ProductInit.safe [{",".join(inserts)}] {sorted(observed)} = true :=
  by decide +kernel
#print axioms {name}
"""


def writes(data):
    if isinstance(data, IncrementGuard):
        return [data.sum, data.guard]
    if isinstance(data, (HighMask, RoundedSize)):
        return [data.result]
    if isinstance(data, RoundedExtent):
        return list(data.results)
    if isinstance(data, list):
        return [phi["result"] for phi in data]
    result = data.get("result")
    return [] if result is None else [result]


def word_key(data):
    if isinstance(data, HighMask):
        return "highMask", data.complement
    if isinstance(data, RoundedSize):
        return "roundedSize", data.combined
    if not isinstance(data, dict) or "operation" not in data:
        return None
    op = data["operation"].get("name")
    if op not in {"add", "sub", "mul", "div", "shl", "shr"}:
        return None
    operands = word_operands(data, op)
    if len(operands) != 2 or operands[0]["kind"] != "local":
        return None
    if op == "add" and operands[0] == operands[1]:
        return "double", 1
    if operands[1]["kind"] != "constant":
        return None
    value = int(operands[1]["value"])
    if not 0 <= value < 1 << 256:
        raise Unsupported("MIR word constant is outside i256")
    if op in {"mul", "div"}:
        if not value or value & (value - 1):
            return None
        value = value.bit_length() - 1
    if op in {"shl", "shr"} and value >= 256:
        return None
    return op, value


def action_signature(data):
    if isinstance(data, IncrementGuard):
        return ("increment-guard",)
    if isinstance(data, RoundedExtent):
        return ("rounded-extent",)
    key = word_key(data)
    if key is None:
        return dead_ssa.signature(data)
    op, value = key
    match op:
        case "add":
            return ("word-add", value)
        case "sub":
            return ("word-add", -value % (1 << 256))
        case "mul" | "shl" | "double":
            return ("word-shl", value)
        case "div" | "shr":
            return ("word-shr", value)
        case "highMask":
            return ("word-high-mask",)
        case "roundedSize":
            return ("word-rounded-size",)
    raise AssertionError(f"unrecognized word rewrite {op}")


def pairing(old, new):
    points = [(0, 0)]
    locations = {(0, 0): 0}
    targets, live, definitions = [], [], []
    jump_limit = 1
    word_rewrites = 0
    increment_rewrites = 0

    def index(pair):
        if pair not in locations:
            if len(points) >= 4096:
                raise Unsupported("paired MIR control flow exceeds 4096 points")
            locations[pair] = len(points)
            points.append(pair)
        return locations[pair]

    def follow(nodes, point):
        hops = 1
        while nodes[point][0] in {"forward", "literal"}:
            kind, condition, edges = nodes[point]
            point = edges[int(kind == "literal" and not condition)]
            hops += 1
            if hops > len(nodes):
                raise Unsupported(
                    "MIR CFG contains a cycle of only unconditional transfers"
                )
        return hops, nodes[point]

    for before, after in points:
        left_hops, (left_kind, left, left_edges) = follow(old, before)
        right_hops, (right_kind, right, right_edges) = follow(new, after)
        jump_limit = max(jump_limit, left_hops, right_hops)
        a, b = writes(left), writes(right)
        if (
            left_kind != right_kind
            or len(a) != len(b)
            or len(left_edges) != len(right_edges)
            or action_signature(left) != action_signature(right)
        ):
            raise Unsupported("MIR control-flow comparison changed a typed action")
        word_rewrites += word_key(left) != word_key(right)
        if isinstance(left, IncrementGuard) and isinstance(right, IncrementGuard):
            increment_rewrites += left.uses_previous != right.uses_previous
        targets.append(
            [index(edge) for edge in zip(left_edges, right_edges, strict=True)]
        )
        live.append(set(zip(reads(left), reads(right), strict=True)))
        definitions.append(
            tuple({local: i for i, local in enumerate(side)} for side in (a, b))
        )

    # At a matched use, either a dominating action writes both values at the
    # same result position or the pair must agree at the previous cursor too.
    predecessors = [[] for _ in points]
    for source, edges in enumerate(targets):
        for dest in set(edges):
            predecessors[dest].append(source)
    pending = deque((i, pair) for i, row in enumerate(live) for pair in row)
    facts = len(pending)
    while pending:
        if facts > 65536:
            raise Unsupported("MIR CFG local correspondence exceeds 65536 pairs")
        dest, pair = pending.popleft()
        for source in predecessors[dest]:
            left, right = definitions[source]
            a, b = left.get(pair[0]), right.get(pair[1])
            if a != b:
                raise Unsupported("MIR CFG writes disagree on a live local pair")
            if a is None and pair not in live[source]:
                facts += 1
                live[source].add(pair)
                pending.append((source, pair))
    if live[0]:
        raise Unsupported(
            "MIR CFG entry depends on a local without a matched definition"
        )
    return points, targets, live, jump_limit, word_rewrites, increment_rewrites


def render(nodes, symbols):
    def row(kind, data, edges):
        if kind == "forward":
            return f".forward {edges[0]}"
        if kind == "literal":
            return f".literalBranch {str(data).lower()} {edges[0]} {edges[1]}"
        word = word_key(data)
        if isinstance(data, IncrementGuard):
            key = f"(.incrementGuard {str(data.uses_previous).lower()})"
        elif isinstance(data, RoundedExtent):
            key = f"(.roundedExtent {str(data.division).lower()} {str(data.checked).lower()})"
        elif word:
            op, value = word
            match op:
                case "add" | "sub":
                    source = f".{op}Constant {value}#256"
                case "double":
                    source = ".double"
                case "highMask" | "roundedSize":
                    source = f".{op} {str(value).lower()}"
                case _:
                    source = f".scale .{op} {value}#8"
            key = f"(.word ({source}))"
        else:
            symbol = symbols.setdefault(
                (kind, dead_ssa.stable(dead_ssa.signature(data))), len(symbols)
            )
            operation = data.get("operation", data) if isinstance(data, dict) else {}
            if operation.get("kind") in {"call", "tail_call"}:
                tail = str(operation["kind"] == "tail_call").lower()
                key = f"(.directCall {operation['function']} {symbol} {tail})"
            else:
                key = f"(.opaque {symbol})"
        return f".action ⟨{key},{reads(data)},{writes(data)},{edges}⟩"

    return "#[" + ",".join(row(*node) for node in nodes) + "]"


def render_point(point, targets, live):
    pairs = ",".join(f"({a},{b})" for a, b in sorted(live))
    return f"⟨{point[0]},{point[1]},[{pairs}],{targets}⟩"


def graph_source(i, old, new):
    symbols = {}
    return f"""
def before{i} : Graph := {render(old, symbols)}
def after{i} : Graph := {render(new, symbols)}
"""


def identity_certificate(i, graph, limit):
    code = f"""
def before{i} : Graph := {render(graph, {})}
def after{i} : Graph := before{i}
theorem identical{i} : before{i} = after{i} := by rfl
#print axioms identical{i}
"""
    matched = (
        f"fun handle work locals state => congrArg "
        f"(fun graph => run graph {limit} handle work 0 locals state) identical{i}"
    )
    return code, matched


def certificate(i, old, new, witnesses, limit):
    code = graph_source(i, old, new)
    parts = []
    checks = []
    proof = ""
    predicate = f"(checkAt before{i} after{i} points{i} {limit})"
    # Reduce bounded chunks in the kernel. Checking a large array all at once
    # retains the reductions for every action and its live-locals check.
    for index, part in enumerate(batched(witnesses, 12)):
        name = f"points{i}_{index}"
        theorem = f"valid{i}_{index}"
        code += f"def {name} : Array Point := #[{','.join(part)}]\n"
        parts.append(name)
        checks.append(
            f"theorem {theorem} : {name}.all {predicate} = true := by decide +kernel"
        )
        proof = f"(append_checked {proof} {theorem})" if proof else theorem
    # Keep random access to the full table independent of proof partitioning.
    code += f"def points{i} : Array Point := #[{','.join(witnesses)}]\n"
    code += "\n".join(checks) + "\n"
    return (
        code
        + f"""
theorem valid{i} : Valid before{i} after{i} points{i} {limit} := by
  have partition : points{i} = {" ++ ".join(parts)} := by rfl
  exact Eq.mp (congrArg (fun rows => rows.all {predicate} = true) partition.symm)
    {proof}
theorem preserved{i} {{V S T : Type}} (handle : Handler V S T) (work : Nat)
    (left right : Locals V) (state : S) :
    MIR.CfgFlow.run before{i} {limit} handle work 0 left state =
    MIR.CfgFlow.run after{i} {limit} handle work 0 right state :=
  simulation before{i} after{i} points{i} {limit} valid{i} handle work 0
    {witnesses[0]} (by rfl) left right state (by simp [agrees])
#print axioms valid{i}
#print axioms preserved{i}
"""
    )


def recursive(calls):
    remaining = set(calls)
    while remaining:
        leaves = {i for i in remaining if not calls[i] & remaining}
        if not leaves:
            return True
        remaining -= leaves
    return False


def call_proof(pairs, entry):
    entry_parameter = " (entry : Nat)" if entry is None else ""
    entry = "entry" if entry is None else entry
    return f"""
open MIR.RecursiveCalls (ProcedurePair runRecursive procedures)
def relatedProcedures : List ProcedurePair := [{",".join(pairs)}]
theorem matchedCalls {{Value World Observation : Type}}
    (handlers : MIR.RecursiveCalls.Handlers Value World Observation)
    (argumentLocals : Nat → List Value → Locals Value) (work callLimit : Nat){entry_parameter}
    (args : List Value) (state : World) :
    runRecursive (procedures relatedProcedures false) handlers argumentLocals work callLimit
        {entry} args state =
      runRecursive (procedures relatedProcedures true) handlers argumentLocals work callLimit
        {entry} args state :=
  MIR.RecursiveCalls.simulation relatedProcedures handlers argumentLocals work callLimit
    {entry} args state
#print axioms matchedCalls
"""


def proof_sources(prefix, setup, bodies, composition):
    """Check large procedure tables without keeping all reductions live at once."""
    module_limit = 100_000
    single = setup + "".join(bodies) + composition
    if len(single) <= module_limit:
        return [("Cfg", single)]

    sources, batch = [], ""
    for body in bodies:
        if batch and len(batch) + len(body) > module_limit:
            sources.append((f"CfgProcedures{len(sources)}", prefix + batch))
            batch = ""
        batch += body
    if batch:
        sources.append((f"CfgProcedures{len(sources)}", prefix + batch))
    imports = "".join(f"import {name}\n" for name, _ in sources)
    return [*sources, ("Cfg", imports + setup + composition)]


def project_reads(
    i,
    bodies,
    definitions,
    fork,
    empty_targets,
    read_only,
    tables,
    *,
    use_heap,
    rewrite_stores,
):
    projected, state_stats, scalar_stats, heap_stats = [], [], [], []
    reduced_bodies, guarded_stats = [], []
    code = ""
    for side, body in enumerate(bodies):
        heap = None
        heap_flows = {}
        if use_heap:
            functions = {entry["id"]: entry for entry in tables[side].values()}
            heap_flows = heap_cursor.with_callees(
                body, functions, definitions, fork, read_only[side]
            )
            if heap_flows:
                heap = heap_flows[body["id"]].addresses
        reduced, edges, reuses, stores, stop_stores = state_read_reuse.normalize(
            body,
            definitions,
            fork,
            read_only=read_only[side],
            heap=heap,
            rewrite_stores=rewrite_stores,
        )
        scalar = {}
        if reuses:
            code += state_read_reuse.certificate(
                f"reads{i}_{side}", edges, reuses, stores
            )
        if stop_stores:
            code += state_read_reuse.stop_store_certificate(
                f"reads{i}_{side}", stop_stores
            )
        guarded = {}
        if reuses or stop_stores:
            # A forwarded load can expose an operand already bounded by an ABI guard.
            untrimmed, guarded, width_code = guarded_widths.normalize(
                reduced, f"readWidths{i}_{side}"
            )
            code += width_code
            reduced, scalar, sites = scalar_reuse.normalize(
                untrimmed, definitions, fork, empty_targets=empty_targets
            )
            code += scalar_reuse.certificate(f"readScalars{i}_{side}", untrimmed, sites)
        for identity, flow in heap_flows.items():
            code += heap_cursor.certificate(f"heap{i}_{side}_{identity}", flow)
        reduced_bodies.append(reduced)
        projected.append(import_graph(reduced))
        stats = dict(Counter(space for space, _ in reuses))
        if stop_stores:
            stats["terminal_mstore"] = sum(map(len, stop_stores.values()))
        state_stats.append(stats)
        scalar_stats.append(scalar)
        guarded_stats.append(guarded)
        heap_stats.append(
            {
                str(identity): {
                    "known_addresses": len(flow.addresses),
                    "cursor_edges": len(flow.transfers),
                    "normal_return": flow.summary,
                    **(
                        {"raw_cursor_loads": len(flow.raw_loads)}
                        if flow.raw_loads
                        else {}
                    ),
                    **(
                        {"raw_cursor_stores": len(flow.raw_stores)}
                        if flow.raw_stores
                        else {}
                    ),
                }
                for identity, flow in heap_flows.items()
            }
        )
    if not any(state_stats) or use_heap and not any(heap_stats):
        raise Unsupported("MIR CFG proof found no applicable state read reuse")
    if (rewrite_stores or use_heap) and dead_ssa.same_definitions(*reduced_bodies):
        projected[1] = projected[0]
    return projected, code, state_stats, scalar_stats, heap_stats, guarded_stats


def project_width_guards(i, bodies, definitions, fork):
    normalized = [
        width_guards.normalize(body, definitions, fork, f"width{i}_{side}")
        for side, body in enumerate(bodies)
    ]
    projected, stats, certificates = zip(*normalized, strict=True)
    if not any(stats):
        raise Unsupported("MIR CFG proof found no applicable width guards")
    return tuple(map(import_graph, projected)), list(stats), "".join(certificates)


def changed_control(paths, function):
    """Select a typed CFG retry after the symbolic importer runs out of time."""
    layouts = []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            return False
        header, entries = prune.declarations(path.read_text())
        if function not in entries:
            return False
        interface, *lines = entries[function]["text"]
        control = tuple(
            line
            for line in lines
            if line.startswith(("jump ", "jumpi ", "switch "))
            or (line.startswith("bb") and line.endswith(":"))
        )
        layouts.append(((header, interface), control))
    before, after = layouts
    return before[0] == after[0] and before[1] != after[1]


def check(
    paths,
    solar,
    executable,
    version,
    timeout,
    function,
    *,
    lowering=None,
    valid_heap=False,
    width_checks=False,
):
    from .check import PRELUDE, ROOT, build_model, run

    helper_adapter = None
    if lowering == "lower-mapping-slots":
        from .lower_mapping_slots import materialize
    elif lowering == "lower-memory-objects":
        from .lower_memory_objects import materialize, semantic_header
    elif lowering == "lower-arithmetic":
        from .lower_arithmetic import materialize
    elif lowering == "lower-slices":
        from .lower_slices import materialize
    elif lowering == "lower-memory-zero":
        from .lower_memory_zero import materialize
    elif lowering == "lower-checks":
        from . import lower_checks

        helper_adapter = lower_checks
    elif lowering == "lower-alloc":
        from . import lower_alloc

        helper_adapter = lower_alloc

    texts, models, interfaces, declarations = [], [], [], []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("MIR CFG proof requires MIR files of at most 1 MiB")
        parsed = run(
            [
                solar,
                path.resolve(),
                "-Zmir-pipeline=none",
                "-Zvalidate-ir=true",
                f"--evm-version={version}",
            ],
            ROOT,
            timeout,
        )
        if parsed.returncode:
            raise Unsupported(parsed.stderr)
        texts.append(parsed.stdout)
        models.append(type_prune.export(parsed.stdout, executable, timeout))
        header, entries = prune.declarations(parsed.stdout)
        declarations.append(entries)
        signatures = [entry["text"][0] for entry in entries.values()]
        if lowering == "lower-memory-objects":
            signatures = list(map(semantic_header, signatures))
        interfaces.append((header, signatures))
    if {k: v for k, v in models[0].items() if k != "functions"} != {
        k: v for k, v in models[1].items() if k != "functions"
    }:
        raise Unsupported("MIR CFG proof changed shared module bindings")
    tables = [
        {body["return_buffer"]: body for body in model["functions"]} for model in models
    ]
    if any(
        len(table) != len(model["functions"])
        for table, model in zip(tables, models, strict=True)
    ):
        raise Unsupported("MIR CFG proof requires unambiguous names")
    empty_targets = {
        body["id"]: body["params"]
        for name, body in tables[0].items()
        if body == tables[1].get(name)
        and not body["returns"]
        and body["blocks"]
        == [
            {
                "phis": [],
                "instructions": [],
                "terminator": {"kind": "return", "args": []},
            }
        ]
    }
    helper = None
    if helper_adapter is not None:
        helper = helper_adapter.bind_helper(tables, declarations)
        if helper is not None:
            tables[0][helper_adapter.HELPER] = helper_adapter.expected_helper(
                helper["id"]
            )
            interfaces[1][1].remove(helper_adapter.HELPER_HEADER)
        if function is None and helper is None:
            raise Unsupported("MIR module proof needs the checked private helper")

    if interfaces[0] != interfaces[1]:
        raise Unsupported("MIR CFG proof changed shared declarations or interfaces")
    nonfresh = (
        lower_alloc.nonfresh_activations(models[0])
        if lowering == "lower-alloc"
        else set()
    )
    prefix = (
        PRELUDE
        + """
open MIR.CfgFlow
set_option Elab.async false
set_option maxRecDepth 16384
set_option maxHeartbeats 2000000
"""
    )
    setup = (
        prefix
        + """
#print axioms MIR.CfgFlow.typed_jump
#print axioms MIR.CfgFlow.typed_enter_empty
#print axioms MIR.CfgFlow.typed_literal_branch
#print axioms MIR.WordRewrite.normalized
#print axioms MIR.CfgFlow.normalized_increment_result
#print axioms MIR.RoundedExtent.shifted_length
#print axioms MIR.RoundedExtent.masked_length
#print axioms MIR.RoundedExtent.normalized
#print axioms MIR.PhaseShaping.pointer_carrier
#print axioms MIR.PhaseShaping.full_word_casts
#print axioms MIR.PhaseShaping.calldata_size_copy
#print axioms MIR.ProductInit.construct_initialized
#print axioms MIR.ProductInit.observed_initialized
#print axioms MIR.ScalarChecks.argument_projection
#print axioms MIR.ScalarChecks.identities
#print axioms MIR.ScalarChecks.widen_bit
#print axioms MIR.ScalarChecks.inactive
#print axioms MIR.ScalarChecks.negated
#print axioms MIR.ScalarChecks.repeated_comparison
"""
    )
    work = list(tables[0]) if function is None else [function]
    checked = {}
    calls = {}
    procedure_pairs = []
    if lowering == "lower-mapping-slots":
        setup += """
#print axioms MIR.LowerMappingSlots.array
#print axioms MIR.LowerMappingSlots.mapping
"""
    elif lowering == "lower-memory-objects":
        setup += "#print axioms MIR.PhaseShaping.pointer_add\n"
    elif lowering == "lower-arithmetic":
        setup += """
#print axioms MIR.LowerUnsignedArithmetic.add256
#print axioms MIR.LowerUnsignedArithmetic.sub256
#print axioms MIR.LowerUnsignedArithmetic.add128
#print axioms MIR.LowerUnsignedArithmetic.sub128
#print axioms MIR.LowerUnsignedArithmetic.mul256
#print axioms MIR.LowerUnsignedArithmetic.div256
#print axioms MIR.LowerUnsignedArithmetic.panic_17
#print axioms MIR.LowerUnsignedArithmetic.panic_18
"""
    elif lowering == "lower-slices":
        setup += """
#print axioms MIR.LowerSlices.fields
#print axioms MIR.LowerSlices.defined
"""
    elif lowering == "lower-memory-zero":
        setup += """
#print axioms MIR.LowerMemoryZero.padding_copy
#print axioms MIR.LowerMemoryZero.zero_word_store
"""
    elif lowering == "lower-checks":
        setup += """
#print axioms MIR.LowerChecks.empty_check
#print axioms MIR.LowerChecks.panic65_check
#print axioms MIR.LowerChecks.short_check
#print axioms MIR.LowerChecks.custom_one
#print axioms MIR.LowerChecks.custom_three
"""
    elif lowering == "lower-alloc":
        setup += """
#print axioms MIR.LowerAlloc.free_pointer_request
#print axioms MIR.LowerAlloc.Stage.bump_guard
#print axioms MIR.LowerAlloc.Stage.load_bump
#print axioms MIR.LowerAlloc.Stage.floor_bump
"""
        if helper is not None:
            setup += """
#print axioms MIR.LowerAlloc.Stage.checked_helper
#print axioms MIR.LowerAlloc.Stage.helper_start
#print axioms MIR.LowerAlloc.Stage.helper_return
"""
    bodies = []
    scalar_definitions = None
    read_only = None
    heap_used = False
    private_cursor_used = False
    while work:
        name = work.pop()
        if name in checked:
            continue
        try:
            a, b = [table[name] for table in tables]
        except KeyError as error:
            raise Unsupported("MIR CFG proof has an unknown callee") from error
        if {k: v for k, v in a.items() if k != "blocks"} != {
            k: v for k, v in b.items() if k != "blocks"
        }:
            raise Unsupported("MIR CFG proof changed a function binding")
        if not a["blocks"] or not b["blocks"]:
            raise Unsupported("MIR CFG proof requires defined function bodies")
        i = len(checked)
        source = a
        expansions = {}
        private_projection = None
        if valid_heap and lowering is None and a != b and name == function:
            if scalar_definitions is None:
                scalar_definitions = {
                    row["name"]: row
                    for row in native.catalog(executable, timeout)["evmir"]
                }
            private_projection = private_cursor.project(
                a, b, models, scalar_definitions, version
            )
            if private_projection is not None:
                (block, point), inventories = private_projection
                source = b
                private_cursor_used = True
                work.extend(caller for caller in inventories if caller != name)
                expansions["private_cursor_store"] = {
                    "block": block,
                    "instruction": point,
                    "checked_read_ranges": {
                        caller: [vars(read) for read in reads]
                        for caller, reads in inventories.items()
                    },
                }
        if lowering is not None:
            if lowering == "lower-alloc":
                source, expanded = lower_alloc.materialize_pair(
                    a,
                    b,
                    helper["id"] if helper else None,
                    allow_floor=a["id"] not in nonfresh,
                )
            elif helper_adapter is not None:
                source, expanded = helper_adapter.materialize_pair(
                    a, b, helper["id"] if helper else None
                )
            else:
                source, expanded = materialize(a)
            if lowering != "lower-memory-objects" and name == function and not expanded:
                raise Unsupported("selected function has no matching source operation")
            if expanded:
                field = {
                    "lower-mapping-slots": "fixed_slot_expansions",
                    "lower-memory-objects": "pointer_add_expansions",
                    "lower-arithmetic": "checked_arithmetic_expansions",
                    "lower-slices": "slice_projections",
                    "lower-memory-zero": "memory_zero_expansions",
                    "lower-checks": "guard_expansions",
                    "lower-alloc": "allocation_expansions",
                }[lowering]
                expansions[field] = expanded
        (first, first_checks), (second, second_checks) = map(
            scalar_checks.normalize, (source, b)
        )
        first, first_disjuncts, first_disjunct_code = false_disjuncts(
            first, f"disjunction{i}_0"
        )
        second, second_disjuncts, second_disjunct_code = false_disjuncts(
            second, f"disjunction{i}_1"
        )
        first, first_increments, first_increment_code = bounded_increments(
            first, f"increment{i}_0"
        )
        second, second_increments, second_increment_code = bounded_increments(
            second, f"increment{i}_1"
        )
        first, first_indices, first_index_code = bounded_indices(first, f"index{i}_0")
        second, second_indices, second_index_code = bounded_indices(
            second, f"index{i}_1"
        )
        old, new = map(import_graph, (first, second))
        calls[a["id"]] = set()
        for kind, data, _ in old:
            if kind == "action" and isinstance(data, dict):
                op = data["operation"]
            elif kind == "terminal":
                op = data
            else:
                continue
            if op["kind"] in {"call", "tail_call"}:
                index = op["function"]
                calls[a["id"]].add(index)
                targets = [
                    name for name, body in tables[0].items() if body["id"] == index
                ]
                if len(targets) != 1:
                    raise Unsupported("MIR CFG proof has an ambiguous call binding")
                work.extend(targets)
        product_initialization = []
        body_code = (
            first_disjunct_code
            + second_disjunct_code
            + first_increment_code
            + second_increment_code
            + first_index_code
            + second_index_code
        )
        if private_projection is not None:
            body_code += private_cursor.certificate(i, private_projection[1])
        for side, graph in enumerate((old, new)):
            products = product_certificate(f"products{i}_{side}", graph)
            product_initialization.append(bool(products))
            if products:
                body_code += products
        literal_branches = [
            sum(node[0] == "literal" for node in graph) for graph in (old, new)
        ]
        if old == new:
            limit = len(old) + 1
            certificate_code, matched = identity_certificate(i, old, limit)
            body_code += certificate_code
            checked[name] = {
                "proof": "identity",
                "source_points": len(old),
                "literal_branches": literal_branches,
            }
        else:
            scalar_stats, state_stats, heap_stats, width_stats = [], [], [], []
            guarded_stats = []
            threaded_guards = []
            try:
                try:
                    pairs = pairing(old, new)
                except Unsupported:
                    proposed = list(map(threadable_guards, (first, second)))
                    if not any(guards for guards, _ in proposed):
                        raise
                    projected = [
                        import_graph(body, threaded=guards)
                        for body, (guards, _) in zip(
                            (first, second), proposed, strict=True
                        )
                    ]
                    pairs = pairing(*projected)
                    old, new = projected
                    threaded_guards = [len(guards) for guards, _ in proposed]
                    literal_branches = [
                        sum(node[0] == "literal" for node in graph)
                        for graph in projected
                    ]
                    for side, (guards, reads) in enumerate(proposed):
                        body_code += phi_guard_certificate(
                            f"phiGuard{i}_{side}", guards, reads
                        )
                points, edges, live, limit, word_rewrites, increment_rewrites = pairs
            except Unsupported as error:
                try:
                    proof, copies, offsets = phase_carriers.paired_proof(i, source, b)
                except Unsupported:
                    if lowering is not None:
                        raise error from None
                else:
                    bodies.append(body_code + proof)
                    checked[name] = {
                        "proof": "carrier-copies" if copies else "word-offsets",
                        "copies": copies,
                        **({"offsets": offsets} if offsets else {}),
                        "product_initialization": product_initialization,
                        **expansions,
                    }
                    continue
                if scalar_definitions is None:
                    scalar_definitions = {
                        row["name"]: row
                        for row in native.catalog(executable, timeout)["evmir"]
                    }
                    setup += scalar_reuse.AXIOMS
                normalized = []
                for side, body in enumerate((first, second)):
                    projected, guarded, width_code = guarded_widths.normalize(
                        body, f"guarded{i}_{side}"
                    )
                    guarded_stats.append(guarded)
                    body_code += width_code
                    reduced, stats, sites = scalar_reuse.normalize(
                        projected,
                        scalar_definitions,
                        version,
                        empty_targets=empty_targets,
                    )
                    normalized.append(reduced)
                    scalar_stats.append(stats)
                    body_code += scalar_reuse.certificate(
                        f"scalar{i}_{side}", projected, sites
                    )
                old, new = map(import_graph, normalized)
                try:
                    pairs = pairing(old, new)
                except Unsupported:
                    if width_checks:
                        projected, width_stats, width_code = project_width_guards(
                            i, normalized, scalar_definitions, version
                        )
                        pairs = pairing(*projected)
                    else:
                        pairs = None
                if width_stats:
                    old, new = projected
                    body_code += width_code
                if pairs is None:
                    first_state_use = read_only is None
                    if first_state_use:
                        read_only = [
                            state_read_reuse.read_only_functions(
                                table.values(), scalar_definitions, version
                            )
                            for table in tables
                        ]
                        setup += state_read_reuse.AXIOMS
                    alternatives = (False, True) if valid_heap else (False,)
                    candidates = [
                        (use_heap, rewrite_stores)
                        for rewrite_stores in (False, True)
                        for use_heap in alternatives
                    ]
                    for use_heap, rewrite_stores in candidates:
                        try:
                            candidate = project_reads(
                                i,
                                normalized,
                                scalar_definitions,
                                version,
                                empty_targets,
                                read_only,
                                tables,
                                use_heap=use_heap,
                                rewrite_stores=rewrite_stores,
                            )
                            (
                                projected,
                                projected_code,
                                state_stats,
                                more_scalars,
                                heap_stats,
                                more_guards,
                            ) = candidate
                            pairs = pairing(*projected)
                        except Unsupported:
                            if (use_heap, rewrite_stores) == candidates[-1]:
                                raise
                        else:
                            break
                    old, new = projected
                    body_code += projected_code
                    scalar_stats = [
                        dict(Counter(initial) + Counter(extra))
                        for initial, extra in zip(
                            scalar_stats, more_scalars, strict=True
                        )
                    ]
                    guarded_stats = [
                        dict(Counter(initial) + Counter(extra))
                        for initial, extra in zip(
                            guarded_stats, more_guards, strict=True
                        )
                    ]
                    if any(heap_stats) and not heap_used:
                        setup += heap_cursor.AXIOMS
                        heap_used = True
                points, edges, live, limit, word_rewrites, increment_rewrites = pairs
            witnesses = [
                render_point(point, targets, locals_)
                for point, targets, locals_ in zip(points, edges, live, strict=True)
            ]
            if old == new:
                certificate_code, matched = identity_certificate(i, old, limit)
                body_code += certificate_code
            else:
                body_code += certificate(i, old, new, witnesses, limit)
                matched = f"fun handle work locals state => preserved{i} handle work locals locals state"
            checked[name] = {
                "proof": "path-correspondence",
                "paired_points": len(points),
                "reused_points": [
                    len(points) - len({point[side] for point in points})
                    for side in (0, 1)
                ],
                "live_pairs": sum(map(len, live)),
                "word_rewrites": word_rewrites,
                "increment_guard_rewrites": increment_rewrites,
                "literal_branches": literal_branches,
                **({"scalar_reuse": scalar_stats} if any(scalar_stats) else {}),
                **(
                    {"threaded_phi_guards": threaded_guards}
                    if any(threaded_guards)
                    else {}
                ),
                **({"state_read_reuse": state_stats} if any(state_stats) else {}),
                **({"heap_cursor": heap_stats} if any(heap_stats) else {}),
                **({"width_guards": width_stats} if width_stats else {}),
                **({"guarded_widths": guarded_stats} if any(guarded_stats) else {}),
            }
        checked[name]["product_initialization"] = product_initialization
        checked[name].update(expansions)
        if first_checks or second_checks:
            checked[name]["scalar_checks"] = [first_checks, second_checks]
        if first_increments or second_increments:
            checked[name]["bounded_increments"] = [first_increments, second_increments]
        if first_disjuncts or second_disjuncts:
            checked[name]["false_disjuncts"] = [first_disjuncts, second_disjuncts]
        if first_indices or second_indices:
            checked[name]["bounded_indices"] = [first_indices, second_indices]
        if any(
            isinstance(data, RoundedExtent)
            for graph in (old, new)
            for _, data, _ in graph
        ):
            checked[name]["rounded_extent_guards"] = [
                sum(
                    isinstance(data, RoundedExtent) and data.checked
                    for _, data, _ in graph
                )
                for graph in (old, new)
            ]
        procedure_pairs.append(
            f"⟨{a['id']},⟨before{i},{limit}⟩,⟨after{i},{limit}⟩,{matched}⟩"
        )
        bodies.append(body_code)
    complete = len(procedure_pairs) == len(checked)
    nested = any(calls.values()) and complete
    recursive_calls = recursive(calls)
    if recursive_calls and not nested:
        raise Unsupported(
            "recursive MIR requires path certificates for all direct callees"
        )
    if function is None and not complete:
        raise Unsupported("MIR module proof needs every function's call certificate")
    composition = ""
    if nested or function is None:
        entry = (
            tables[0][function]["id"]
            if function is not None and not private_cursor_used
            else None
        )
        composition = call_proof(procedure_pairs, entry)
    caller_heaps = []
    caller_proofs = []
    if checked.get(function, {}).get("heap_cursor"):
        selected = tables[0][function]["id"]
        caller_entries = [
            heap_cursor.callers(
                selected, model, scalar_definitions, version, read_only[side]
            )
            for side, model in enumerate(models)
        ]
        if all(caller_entries) and caller_entries[0].keys() == caller_entries[1].keys():
            for side, entries in enumerate(caller_entries):
                evidence_side = {}
                for name, (identity, selector, sites, flows) in entries.items():
                    tag = f"callerHeap{side}_{identity}"
                    code = heap_cursor.caller_certificate(
                        tag, flows[identity], selected, sites
                    )
                    for callee, flow in flows.items():
                        if callee != identity:
                            code += heap_cursor.certificate(
                                f"{tag}Callee{callee}", flow
                            )
                    caller_proofs.append(
                        (f"CallerHeap{side}_{identity}", prefix + code)
                    )
                    evidence_side[name] = {
                        "selector": selector,
                        "calls": [
                            {
                                "block": block,
                                "instruction": instruction,
                                "cursor_offset": offset,
                            }
                            for block, instruction, _, offset in sites
                        ],
                        "summarized_callees": sorted(
                            entry["return_buffer"]
                            for entry in models[side]["functions"]
                            if entry["id"] in flows and entry["id"] != identity
                        ),
                    }
                caller_heaps.append(evidence_side)
    sources = proof_sources(prefix, setup, bodies, composition) + caller_proofs
    entry_proof.success(build_model(timeout), "MIR CFG proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-cfg-") as folder:
        for name, code in sources:
            output = entry_proof.success(
                entry_proof.lean(
                    Path(folder),
                    name,
                    code,
                    timeout,
                    module=name.startswith("CfgProcedures"),
                ),
                f"MIR CFG certificate {name}",
            )
            entry_proof.check_axioms(output.splitlines(), code.count("#print axioms "))
    evidence = {
        "kind": {
            "lower-mapping-slots": "kernel-checked-fixed-storage-slots",
            "lower-memory-objects": "kernel-checked-memory-object-carriers",
            "lower-arithmetic": "kernel-checked-unsigned-arithmetic",
            "lower-slices": "kernel-checked-slice-projections",
            "lower-memory-zero": "kernel-checked-memory-zero",
            "lower-checks": "kernel-checked-conditional-checks",
            "lower-alloc": "kernel-checked-allocation-lowering",
        }.get(
            lowering,
            "kernel-checked-private-cursor-store"
            if private_cursor_used
            else "kernel-checked-valid-heap-read-reuse"
            if heap_used
            else "kernel-checked-typed-jump-chain",
        ),
        "functions": sorted(checked),
        "correspondence": checked,
        **({"caller_heaps": caller_heaps} if caller_heaps else {}),
        "proof_sha256": hashlib.sha256(
            "".join(code for _, code in sources).encode()
        ).hexdigest(),
        **({"proof_modules": len(sources)} if len(sources) > 1 else {}),
        "boundary": "trusted structural exporter and action-constructor binding; unpriced shared effect handlers including calls and GAS; independent caller/ABI and backend correspondence remain separate",
        **(
            {
                "private_revert_helper"
                if lowering == "lower-checks"
                else "private_allocation_helper": helper["id"]
            }
            if helper is not None
            else {}
        ),
        **(
            {"nested_calls": {"recursive": recursive_calls, "functions": sorted(calls)}}
            if nested
            else {}
        ),
    }
    programs = tuple(
        Program(
            ("mir-cfg-flow", function),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="mir-cfg-flow",
            evm_version=version,
            import_evidence=evidence,
        )
        for text in texts
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
