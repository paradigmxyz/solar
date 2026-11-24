"""Propose path and live-local correspondence for independently exported MIR."""

import hashlib
import tempfile
from collections import Counter, deque
from dataclasses import dataclass
from itertools import batched
from pathlib import Path

from . import (
    audit,
    dead_ssa,
    entry_proof,
    phase_carriers,
    prune,
    scalar_checks,
    type_prune,
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


def rounded_extent(block, position, uses, definitions):
    """Check a block suffix that computes allocation size and dispatches on overflow."""
    term = block["terminator"]
    instructions = block["instructions"]
    if term["kind"] != "branch" or not 4 <= len(instructions) - position <= 6:
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
    if term["args"] != [expected]:
        return None
    results = (sum_["result"], quotient["result"], aligned["result"], extent["result"])
    return RoundedExtent(length["index"], results, division, bool(guard))


def import_graph(body):
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
            next = point(index, None, 0)
            nodes.append(
                ("action", phis, [next]) if phis else ("forward", None, [next])
            )
        elif position < len(block["instructions"]):
            instructions = block["instructions"]
            if guard := increment_guard(instructions, position):
                nodes.append(("action", guard, [point(index, None, position + 2)]))
            elif mask := high_mask(instructions, position, uses):
                data, length = mask
                nodes.append(("action", data, [point(index, None, position + length)]))
            elif extent := rounded_extent(block, position, uses, definitions):
                nodes.append(
                    ("action", extent, [point(block["terminator"]["else"], index)])
                )
            else:
                inst = instructions[position]
                nodes.append(("action", inst, [point(index, None, position + 1)]))
        else:
            term = block["terminator"]
            match term["kind"]:
                case "jump":
                    nodes.append(("forward", None, [point(term["target"], index)]))
                case "branch":
                    keys = {k: v for k, v in term.items() if k not in {"then", "else"}}
                    edges = [point(term[k], index) for k in ("then", "else")]
                    (condition,) = term["args"]
                    if (
                        condition["kind"] == "constant"
                        and condition["type"] == FLAG
                        and condition["value"] in {"0", "1"}
                    ):
                        nodes.append(("literal", condition["value"] == "1", edges))
                    else:
                        nodes.append(("select", keys, edges))
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
    if isinstance(data, RoundedExtent):
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
    if isinstance(data, HighMask):
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
                case "highMask":
                    source = f".highMask {str(value).lower()}"
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
    return f"""
open MIR.RecursiveCalls (ProcedurePair runRecursive procedures)
def relatedProcedures : List ProcedurePair := [{",".join(pairs)}]
theorem matchedCalls {{Value World Observation : Type}}
    (handlers : MIR.RecursiveCalls.Handlers Value World Observation)
    (argumentLocals : Nat → List Value → Locals Value) (work callLimit : Nat)
    (args : List Value) (state : World) :
    runRecursive (procedures relatedProcedures false) handlers argumentLocals work callLimit
        {entry} args state =
      runRecursive (procedures relatedProcedures true) handlers argumentLocals work callLimit
        {entry} args state :=
  MIR.RecursiveCalls.simulation relatedProcedures handlers argumentLocals work callLimit
    {entry} args state
#print axioms matchedCalls
"""


def check(paths, solar, executable, version, timeout, function, *, lowering=None):
    from .check import PRELUDE, ROOT, build_model, run

    if lowering == "lower-mapping-slots":
        from .lower_mapping_slots import materialize
    elif lowering == "lower-memory-objects":
        from .lower_memory_objects import materialize, semantic_header
    elif lowering == "lower-arithmetic":
        from .lower_arithmetic import materialize
    elif lowering == "lower-slices":
        from .lower_slices import materialize

    texts, models, interfaces = [], [], []
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
        signatures = [entry["text"][0] for entry in entries.values()]
        if lowering == "lower-memory-objects":
            signatures = list(map(semantic_header, signatures))
        interfaces.append((header, signatures))
    if interfaces[0] != interfaces[1] or {
        k: v for k, v in models[0].items() if k != "functions"
    } != {k: v for k, v in models[1].items() if k != "functions"}:
        raise Unsupported("MIR CFG proof changed shared declarations or interfaces")
    tables = [
        {body["return_buffer"]: body for body in model["functions"]} for model in models
    ]
    if any(
        len(table) != len(model["functions"])
        for table, model in zip(tables, models, strict=True)
    ):
        raise Unsupported("MIR CFG proof requires unambiguous names")
    code = (
        PRELUDE
        + """
open MIR.CfgFlow
set_option maxRecDepth 16384
set_option maxHeartbeats 2000000
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
    work = [function]
    checked = {}
    calls = {}
    procedure_pairs = []
    if lowering == "lower-mapping-slots":
        code += """
#print axioms MIR.LowerMappingSlots.array
#print axioms MIR.LowerMappingSlots.mapping
"""
    elif lowering == "lower-memory-objects":
        code += "#print axioms MIR.PhaseShaping.pointer_add\n"
    elif lowering == "lower-arithmetic":
        code += """
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
        code += """
#print axioms MIR.LowerSlices.fields
#print axioms MIR.LowerSlices.defined
"""
    axioms_checks = code.count("#print axioms ")
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
        if lowering is not None:
            source, expanded = materialize(a)
            if lowering != "lower-memory-objects" and name == function and not expanded:
                raise Unsupported("selected function has no matching source operation")
            if expanded:
                field = {
                    "lower-mapping-slots": "fixed_slot_expansions",
                    "lower-memory-objects": "pointer_add_expansions",
                    "lower-arithmetic": "checked_arithmetic_expansions",
                    "lower-slices": "slice_projections",
                }[lowering]
                expansions[field] = expanded
        (first, first_checks), (second, second_checks) = map(
            scalar_checks.normalize, (source, b)
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
        for side, graph in enumerate((old, new)):
            products = product_certificate(f"products{i}_{side}", graph)
            product_initialization.append(bool(products))
            if products:
                code += products
                axioms_checks += 1
        literal_branches = [
            sum(node[0] == "literal" for node in graph) for graph in (old, new)
        ]
        if old == new:
            code += graph_source(i, old, new)
            code += f"""
theorem identical{i} : before{i} = after{i} := by rfl
#print axioms identical{i}
"""
            axioms_checks += 1
            checked[name] = {
                "proof": "identity",
                "source_points": len(old),
                "literal_branches": literal_branches,
            }
            limit = len(old) + 1
            matched = (
                f"fun handle work locals state => congrArg "
                f"(fun graph => run graph {limit} handle work 0 locals state) identical{i}"
            )
        else:
            try:
                points, edges, live, limit, word_rewrites, increment_rewrites = pairing(
                    old, new
                )
            except Unsupported as error:
                try:
                    proof, copies, offsets = phase_carriers.paired_proof(i, source, b)
                except Unsupported:
                    raise error from None
                code += proof
                axioms_checks += 1 + bool(offsets)
                checked[name] = {
                    "proof": "carrier-copies" if copies else "word-offsets",
                    "copies": copies,
                    **({"offsets": offsets} if offsets else {}),
                    "product_initialization": product_initialization,
                    **expansions,
                }
                continue
            witnesses = [
                render_point(point, targets, locals_)
                for point, targets, locals_ in zip(points, edges, live, strict=True)
            ]
            code += certificate(i, old, new, witnesses, limit)
            matched = f"fun handle work locals state => preserved{i} handle work locals locals state"
            axioms_checks += 2
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
            }
        checked[name]["product_initialization"] = product_initialization
        checked[name].update(expansions)
        if first_checks or second_checks:
            checked[name]["scalar_checks"] = [first_checks, second_checks]
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
    nested = any(calls.values()) and len(procedure_pairs) == len(checked)
    recursive_calls = recursive(calls)
    if recursive_calls and not nested:
        raise Unsupported(
            "recursive MIR requires path certificates for all direct callees"
        )
    if nested:
        code += call_proof(procedure_pairs, tables[0][function]["id"])
        axioms_checks += 1
    entry_proof.success(build_model(timeout), "MIR CFG proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-cfg-") as folder:
        output = entry_proof.success(
            entry_proof.lean(Path(folder), "Cfg", code, timeout), "MIR CFG certificate"
        )
    entry_proof.check_axioms(output.splitlines(), axioms_checks)
    evidence = {
        "kind": {
            "lower-mapping-slots": "kernel-checked-fixed-storage-slots",
            "lower-memory-objects": "kernel-checked-memory-object-carriers",
            "lower-arithmetic": "kernel-checked-unsigned-arithmetic",
            "lower-slices": "kernel-checked-slice-projections",
        }.get(lowering, "kernel-checked-typed-jump-chain"),
        "functions": sorted(checked),
        "correspondence": checked,
        "proof_sha256": hashlib.sha256(code.encode()).hexdigest(),
        "boundary": "trusted structural exporter and action-constructor binding; unpriced shared effect handlers including calls and GAS; independent caller/ABI and backend correspondence remain separate",
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
