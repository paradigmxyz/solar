"""Project only proved total scalar identities out of typed MIR action graphs.

The compiler has checked SSA dominance and defined result widths. This importer
checks the reused operand and type bindings and proposes independent path
witnesses for cross-block reuse. GAS, calls, and mutable observations remain
actions with all their operands, including actions with dead results.
"""

from collections import Counter, defaultdict, deque

from . import dead_ssa, mir_effects, phi_edges, scalar_checks
from .common import Unsupported

WORD, FLAG = scalar_checks.WORD, scalar_checks.FLAG
local, literal, substitute = (
    scalar_checks.local,
    scalar_checks.literal,
    scalar_checks.substitute,
)

AXIOMS = """
#print axioms MIR.ScalarReuse.unsigned_nonzero
#print axioms MIR.ScalarReuse.unsigned_flip
#print axioms MIR.ScalarReuse.same_width
#print axioms MIR.ScalarReuse.round_trip
#print axioms MIR.ScalarReuse.literal_cast
#print axioms MIR.ScalarReuse.clean_mask
#print axioms MIR.ScalarReuse.clean_shift
#print axioms MIR.ScalarReuse.zero_identities
#print axioms MIR.ScalarReuse.closed_comparison
#print axioms MIR.ScalarReuse.bit_test
#print axioms MIR.ScalarReuse.flip_branch
#print axioms MIR.ScalarReuse.complement_equality
#print axioms MIR.ScalarReuse.complement_cut
#print axioms MIR.ScalarReuse.caller_value
#print axioms MIR.ScalarReuse.phi_read
"""


def reachable(successors, removed=None):
    if removed == 0:
        return set()
    reached = {0}
    pending = deque([0])
    while pending:
        for target in successors[pending.popleft()]:
            if target != removed and target not in reached:
                reached.add(target)
                pending.append(target)
    return reached


def dominance(successors):
    reached = reachable(successors)
    parents = defaultdict(set)
    for origin in reached:
        for target in successors[origin]:
            parents[target].add(origin)
    dom = {index: set(reached) if index else {0} for index in reached}
    changed = True
    while changed:
        changed = False
        for index in sorted(reached - {0}):
            updated = {index} | set.intersection(
                *(dom[pred] for pred in parents[index])
            )
            if updated != dom[index]:
                dom[index] = updated
                changed = True
    return dom, parents


def total(inst, definitions, fork):
    op = inst["operation"]
    if (
        inst["result"] is None
        or any(arg["kind"] == "undefined" for arg in op["args"])
        or op["kind"] == "opcode"
        and not definitions[op["name"]]["forks"][fork]["available"]
    ):
        return False
    if op == {"kind": "opcode", "name": "caller", "args": []}:
        return inst["type"]["kind"] == "word" and inst["type"]["bits"] in {160, 256}
    try:
        bits = mir_effects.scalar_width(inst["type"])
        args = [("value", mir_effects.scalar_width(arg["type"])) for arg in op["args"]]
        return mir_effects.pure(op, args, bits, definitions) is not None
    except Unsupported:
        return False


def definition(operand, retained):
    if operand["kind"] == "local" and operand["index"] in retained:
        return retained[operand["index"]]["operation"]
    return {}


def widened_input(op, operand):
    if (
        op.get("kind") == "word"
        and len(op["args"]) == 1
        and op["args"][0]["type"]["kind"] == "word"
        and operand["type"]["kind"] == "word"
        and op["args"][0]["type"]["bits"] <= operand["type"]["bits"]
    ):
        return op["args"][0]
    return None


def scalar_alias(inst, retained):
    ty, op = inst["type"], inst["operation"]
    args = op["args"]
    if op["kind"] in {"word", "low_bits"}:
        (operand,) = args
        if ty == operand["type"]:
            return operand
        if (
            operand["kind"] == "constant"
            and ty["kind"] == operand["type"]["kind"] == "word"
            and (op["kind"] == "low_bits" or operand["type"]["bits"] <= ty["bits"])
        ):
            return literal(int(operand["value"]) % (2 ** ty["bits"]), ty)
        if op["kind"] == "low_bits":
            original = widened_input(definition(operand, retained), operand)
            if original is not None and original["type"] == ty:
                return original
    if op.get("name") == "and":
        for operand, mask in (args, args[::-1]):
            original = widened_input(definition(operand, retained), operand)
            if (
                original is not None
                and ty == operand["type"] == mask["type"]
                and mask["kind"] == "constant"
                and int(mask["value"]) == 2 ** original["type"]["bits"] - 1
            ):
                return operand
    if op.get("name") == "shr":
        amount, shifted = args
        producer = definition(shifted, retained)
        if (
            amount["kind"] == "constant"
            and producer.get("name") == "shl"
            and producer["args"][0] == amount
        ):
            operand = producer["args"][1]
            original = widened_input(definition(operand, retained), operand)
            if original is not None and ty == operand["type"]:
                bits, shift = ty["bits"], int(amount["value"])
                if 0 <= shift < bits and original["type"]["bits"] + shift <= bits:
                    return operand
    if len(args) != 2:
        return None
    left, right = args
    name = "ne" if op["kind"] == "ne" else op.get("name")
    if left["type"] != right["type"]:
        return None
    if ty == left["type"] and ty["kind"] == "word":
        zero = literal(0, ty)
        if name in {"add", "or", "xor", "sub"} and right == zero:
            return left
        if name in {"add", "or", "xor"} and left == zero:
            return right
        if name == "sub" and left == right:
            return zero
    if ty == FLAG and name in {"eq", "ne"}:
        if left == right:
            return literal(int(name == "eq"), FLAG)
        if left["kind"] == right["kind"] == "constant":
            same = int(left["value"]) == int(right["value"])
            return literal(int(same == (name == "eq")), FLAG)
        if left["type"] == FLAG:
            identity = literal(int(name == "eq"), FLAG)
            if right == identity:
                return left
            if left == identity:
                return right
    return None


def unsigned_test(op, retained):
    name = op.get("name")
    if name not in {"gt", "lt"}:
        return op, None
    lower, higher = op["args"] if name == "lt" else op["args"][::-1]
    if lower == literal(0, higher["type"]):
        return {"kind": "ne", "args": [higher, lower]}, "nonzero_tests"
    source = definition(lower, retained)
    one = literal(1, WORD)
    if (
        higher["type"] == lower["type"] == WORD
        and source.get("name") == "add"
        and source["args"] in ([higher, one], [one, higher])
    ):
        return {
            **op,
            "name": "eq",
            "args": [lower, literal(0, WORD)],
        }, "increment_tests"
    if name == "gt":
        return {**op, "name": "lt", "args": [lower, higher]}, "unsigned_comparisons"
    return op, None


def complemented_branch(term, retained):
    if term["kind"] != "branch":
        return None
    comparison = definition(term["args"][0], retained)
    if comparison.get("name") != "eq":
        return None
    condition, zero = comparison["args"]
    if condition == literal(0, zero["type"]):
        condition, zero = zero, condition
    if zero != literal(0, condition["type"]):
        return None
    if condition["type"] != FLAG:
        condition = widened_input(definition(condition, retained), condition)
        if condition is None or condition["type"] != FLAG:
            return None
    return {**term, "args": [condition], "then": term["else"], "else": term["then"]}


def phi_source(phi, predecessors, aliases):
    incoming = phi["incoming"]
    if not incoming or len(incoming) != len(predecessors):
        return None
    if {pred for pred, _ in incoming} != predecessors:
        return None
    values = [substitute(arg, aliases) for _, arg in incoming]
    operand = values[0]
    if (
        operand["kind"] in {"arg", "constant", "local"}
        and operand["type"] == phi["type"]
        and all(value == operand for value in values)
    ):
        return operand
    return None


def canonicalize_branches(blocks, retained, stats, definitions, fork):
    """Complement a predicate only when its branch is its entire use."""
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    edits = {}
    for block in blocks:
        term = block["terminator"]
        if term["kind"] != "branch":
            continue
        flag = term["args"][0]
        if flag["kind"] != "local" or uses[flag["index"]] != 1:
            continue
        result = flag["index"]
        op = definition(flag, retained)
        if op.get("kind") == "ne" and definitions["eq"]["forks"][fork]["available"]:
            replacement = {**op, "kind": "opcode", "name": "eq"}
        elif op.get("name") == "lt" and op["args"][0]["kind"] == "constant":
            constant, word = op["args"]
            bits, bound = constant["type"]["bits"], int(constant["value"]) + 1
            if word["kind"] == "constant" or bound >= 2**bits:
                continue
            replacement = {**op, "args": [word, literal(bound, constant["type"])]}
        else:
            continue
        edits[result] = {**retained[result], "operation": replacement}
        block["terminator"] = {**term, "then": term["else"], "else": term["then"]}
        stats["branch_cuts"] += 1
    for block in blocks:
        block["instructions"] = [
            edits.get(inst["result"], inst) for inst in block["instructions"]
        ]
    retained.update(edits)


def erase_dead(blocks, retained, stats):
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    live = set(retained)
    pending = deque(retained)
    while pending:
        result = pending.popleft()
        if result not in live or uses[result]:
            continue
        live.remove(result)
        stats["dead_scalars"] += 1
        for operand in dead_ssa.reads(retained[result], reject_undefined=False):
            uses[operand] -= 1
            if operand in live and not uses[operand]:
                pending.append(operand)
    dead = retained.keys() - live
    for block in blocks:
        block["instructions"] = [
            inst for inst in block["instructions"] if inst["result"] not in dead
        ]


def normalize(body, definitions, fork):
    """Return the normalized body and the sites the kernel must check."""
    blocks = body["blocks"]
    if len(blocks) > 1024:
        raise Unsupported("scalar reuse requires at most 1024 blocks")
    successors = [phi_edges.targets(block["terminator"]) for block in blocks]
    dom, parents = dominance(successors)
    owners = {
        inst["result"]: index
        for index, block in enumerate(blocks)
        for inst in (*block["phis"], *block["instructions"])
        if inst["result"] is not None
    }
    cached, aliases, retained = defaultdict(list), {}, {}
    updated, stats, sites = {}, Counter(), set()
    for index in sorted(dom, key=lambda block: (len(dom[block]), block)):
        block = blocks[index]
        phis, instructions = [], []
        for phi in block["phis"]:
            operand = phi_source(phi, parents[index], aliases) if index else None
            if operand is None:
                phis.append(phi)
                continue
            if operand["kind"] == "local":
                owner = owners[operand["index"]]
                if owner not in dom[index] or owner == index:
                    phis.append(phi)
                    continue
                sites.add((owner, index))
            aliases[phi["result"]] = operand
            stats["uniform_phis"] += 1

        for original in block["instructions"]:
            inst = substitute(original, aliases)
            result = inst["result"]
            if not total(inst, definitions, fork):
                instructions.append(inst)
                continue
            if inst["type"] == FLAG:
                op, rewrite = unsigned_test(inst["operation"], retained)
                if rewrite:
                    inst = {**inst, "operation": op}
                    stats[rewrite] += 1
            if operand := scalar_alias(inst, retained):
                aliases[result] = operand
                stats["scalar_aliases"] += 1
                continue
            key = dead_ssa.stable((inst["type"], inst["operation"]))
            for owner, producer in reversed(cached[key]):
                if owner in dom[index]:
                    aliases[result] = local(producer, inst["type"])
                    stats["reused_values"] += 1
                    if owner != index:
                        sites.add((owner, index))
                    break
            else:
                cached[key].append((index, result))
                retained[result] = inst
                instructions.append(inst)
        term = substitute(block["terminator"], aliases)
        if flipped := complemented_branch(term, retained):
            term = flipped
            stats["branch_complements"] += 1
        updated[index] = {
            **block,
            "phis": phis,
            "instructions": instructions,
            "terminator": term,
        }
    result = substitute(
        [updated.get(index, block) for index, block in enumerate(blocks)], aliases
    )
    canonicalize_branches(result, retained, stats, definitions, fork)
    erase_dead(result, retained, stats)
    return {**body, "blocks": result}, dict(stats), sites


def certificate(name, body, sites):
    if not sites:
        return ""
    successors = [phi_edges.targets(block["terminator"]) for block in body["blocks"]]
    edges = ",".join(
        f"({origin},{target})"
        for origin, targets in enumerate(successors)
        for target in targets
    )
    outside = {
        producer: sorted(reachable(successors, producer)) for producer, _ in sites
    }
    rows = ",".join(
        f"⟨{producer},{consumer},{outside[producer]}⟩"
        for producer, consumer in sorted(sites)
    )
    return f"""
def {name}Edges : MIR.ScalarReuse.Edges := [{edges}]
def {name}Sites : List MIR.ScalarReuse.ReuseSite := [{rows}]
theorem {name}Sound (site : MIR.ScalarReuse.ReuseSite)
    (member : site ∈ {name}Sites) :
    ¬ MIR.ScalarReuse.Avoids {name}Edges site.producer site.consumer :=
  MIR.ScalarReuse.sites_sound {name}Edges {name}Sites (by decide +kernel) site member
#print axioms {name}Sound
"""
