"""Project only proved total scalar identities out of typed MIR action graphs.

The compiler has checked SSA dominance and defined result widths. This importer
checks the reused operand and type bindings and proposes independent path
witnesses for cross-block reuse. GAS, calls, and mutable observations remain
actions, including actions with dead results. The message projection compares
effective destinations and ignores offsets only for empty ranges.
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
#print axioms MIR.ScalarReuse.packed_low
#print axioms BitVec.or_comm
#print axioms MIR.ScalarReuse.clean_shift
#print axioms MIR.ScalarReuse.shift_mask
#print axioms MIR.ScalarReuse.zero_identities
#print axioms MIR.ScalarReuse.offset_difference
#print axioms MIR.ScalarReuse.add_literals
#print axioms MIR.ScalarReuse.closed_comparison
#print axioms MIR.ScalarReuse.bit_test
#print axioms MIR.ScalarReuse.below_one
#print axioms MIR.ScalarReuse.widened_zero_test
#print axioms MIR.ScalarReuse.negate_equality
#print axioms MIR.ScalarReuse.negate_cut
#print axioms MIR.ScalarReuse.widened_sub_xor
#print axioms MIR.ScalarReuse.widened_xor
#print axioms MIR.ScalarReuse.flip_branch
#print axioms MIR.ScalarReuse.complement_equality
#print axioms MIR.ScalarReuse.complement_cut
#print axioms MIR.ScalarReuse.caller_value
#print axioms MIR.ScalarReuse.timestamp_value
#print axioms MIR.ScalarReuse.returndata_guard
#print axioms MIR.ScalarReuse.phi_read
#print axioms MIR.LowerSlices.fields
#print axioms MIR.CallOperands.target_width
#print axioms MIR.CallOperands.call_ranges_with_value
#print axioms MIR.CallOperands.call_ranges_without_value
#print axioms MIR.CallOperands.code_size
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
    if op == {"kind": "opcode", "name": "timestamp", "args": []}:
        return inst["type"] == WORD
    if op["kind"] in {"make_slice", "extract_value"}:
        return dead_ssa.erasable(inst, definitions)
    if op["kind"] == "low_bits" and op["bits"] == 256 and inst["type"] == WORD:
        (source,) = op["args"]
        if source["type"]["kind"] == "pointer" and source["type"]["bits"] == 256:
            return True
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


def significant_bits(operand, retained):
    width = operand["type"]["bits"]
    producer = definition(operand, retained)
    original = widened_input(producer, operand)
    if original is not None:
        return original["type"]["bits"]
    if producer.get("name") == "shr" and producer["args"][0]["kind"] == "constant":
        return max(0, width - int(producer["args"][0]["value"]))
    return width


def low_part(operand, mask, retained):
    """Recognize a low mask over a narrow value or a packed high/low word."""
    ty = operand["type"]
    if ty != mask["type"] or mask["kind"] != "constant":
        return None
    size = int(mask["value"]) + 1
    bits = size.bit_length() - 1
    if size <= 0 or size != 2**bits or bits > ty["bits"]:
        return None
    if significant_bits(operand, retained) <= bits:
        return operand
    producer = definition(operand, retained)
    if producer.get("name") == "or":
        args = producer["args"]
        high_mask = literal(2 ** ty["bits"] - size, ty)
        for high, low in (args, args[::-1]):
            masked = definition(high, retained)
            if (
                masked.get("name") == "and"
                and high_mask in masked["args"]
                and significant_bits(low, retained) <= bits
            ):
                return low
    return None


def scalar_alias(inst, retained):
    ty, op = inst["type"], inst["operation"]
    args = op["args"]
    if op["kind"] == "extract_value" and op["aggregate_type"]["kind"] == "slice":
        producer = definition(args[0], retained)
        if (
            producer.get("kind") == "make_slice"
            and producer["location"] == op["aggregate_type"]["location"]
            and 0 <= op["index"] < len(producer["args"])
            and producer["args"][op["index"]]["type"] == ty
        ):
            return producer["args"][op["index"]]
    if op["kind"] in {"word", "low_bits"}:
        (operand,) = args
        if ty == operand["type"]:
            return operand
        if (
            operand["kind"] == "constant"
            and ty["kind"] == "word"
            and (op["kind"] == "low_bits" or operand["type"]["bits"] <= ty["bits"])
        ):
            return literal(int(operand["value"]) % (2 ** ty["bits"]), ty)
        if op["kind"] == "low_bits":
            original = widened_input(definition(operand, retained), operand)
            if original is not None and original["type"] == ty:
                return original
    if op.get("name") == "and":
        for operand, mask in (args, args[::-1]):
            if ty == operand["type"] and (part := low_part(operand, mask, retained)):
                return part
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
        if name == "add" and left["kind"] == right["kind"] == "constant":
            combined = (int(left["value"]) + int(right["value"])) % (2 ** ty["bits"])
            return literal(combined, ty)
        if name in {"add", "or", "xor", "sub"} and right == zero:
            return left
        if name in {"add", "or", "xor"} and left == zero:
            return right
        if name == "sub" and left == right:
            return zero
        if name == "sub":
            producer = definition(left, retained)
            if producer.get("name") == "add":
                first, second = producer["args"]
                if first == right:
                    return second
                if second == right:
                    return first
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


def zero_test(op, retained):
    """Keep comparisons in the operand width and eliminate a proved inversion."""
    name = "ne" if op["kind"] == "ne" else op.get("name")
    if name not in {"eq", "ne", "lt"}:
        return op, None
    left, right = op["args"]
    if left["type"] != right["type"] or left["type"]["kind"] != "word":
        return op, None
    if name == "lt":
        if right == literal(1, left["type"]):
            return {
                **op,
                "name": "eq",
                "args": [left, literal(0, left["type"])],
            }, "one_bounds"
        return op, None
    if left == literal(0, right["type"]):
        left, right = right, left
    if right != literal(0, left["type"]):
        return op, None

    narrowed = widened_input(definition(left, retained), left)
    widened_flag = narrowed is not None and narrowed["type"] == FLAG
    if widened_flag:
        left, right = narrowed, literal(0, FLAG)
    producer = definition(left, retained)
    kind, opcode = producer.get("kind"), producer.get("name")
    if name == "eq" and left["type"] == FLAG:
        args = producer.get("args", [])
        if kind == "ne":
            return {"kind": "opcode", "name": "eq", "args": args}, "inverted_predicates"
        if opcode == "eq":
            return {"kind": "ne", "args": args}, "inverted_predicates"
        if opcode == "lt" and args[0]["kind"] == "constant":
            lower, word = args
            bound = int(lower["value"]) + 1
            if bound < 2 ** lower["type"]["bits"]:
                return {
                    **producer,
                    "args": [word, literal(bound, lower["type"])],
                }, "inverted_predicates"
    if name == "ne" and opcode in {"sub", "xor"}:
        a, b = (
            widened_input(definition(arg, retained), arg) for arg in producer["args"]
        )
        if a is not None and b is not None and a["type"] == b["type"] == FLAG:
            return {
                "kind": "opcode",
                "name": "xor",
                "args": [a, b],
            }, "boolean_differences"
    if widened_flag:
        return {**op, "args": [left, right]}, "widened_bit_tests"
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


def returned_word(inst, opcode):
    return (
        inst["result"] is not None
        and inst["type"] == WORD
        and inst["operation"] == {"kind": "opcode", "name": opcode, "args": []}
    )


def rounded_operand(result, retained, name, constant):
    source = definition(result, retained)
    if source.get("name") != name or result["type"] != WORD:
        return None
    first, second = source["args"]
    if first == literal(constant, WORD):
        first, second = second, first
    if first["type"] == WORD and second == literal(constant, WORD):
        return first
    return None


def returndata_guard(op, retained, return_sizes):
    if op["kind"] != "opcode" or op["name"] != "lt":
        return False
    extent, length = op["args"]
    if length["kind"] != "local" or length["index"] not in return_sizes:
        return False
    aligned = rounded_operand(extent, retained, "add", 32)
    if aligned is None:
        return False
    rounded = rounded_operand(aligned, retained, "and", 2**256 - 32)
    return (
        rounded is not None and rounded_operand(rounded, retained, "add", 31) == length
    )


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


def effective_call(inst, retained, definitions, fork, stats):
    op = inst["operation"]
    if op["kind"] != "opcode":
        return inst
    name = op["name"]
    if name in {"call", "callcode"}:
        input_offset = 3
    elif name in {"delegatecall", "staticcall"}:
        input_offset = 2
    else:
        return inst
    args = list(op["args"])
    if (
        len(args) != input_offset + 4
        or inst["type"] != FLAG
        or any(
            arg["kind"] == "undefined"
            or arg["type"] != WORD
            and (
                index not in (input_offset, input_offset + 2)
                or arg["type"]
                != {
                    "kind": "pointer",
                    "name": "ptr<memory>",
                    "location": "memory",
                    "bits": 256,
                }
            )
            for index, arg in enumerate(args)
        )
        or not definitions[name]["forks"][fork]["available"]
    ):
        return inst
    destination = args[1]
    low = widened_input(definition(destination, retained), destination)
    if low is not None and low["type"]["bits"] == 160:
        cut = definition(low, retained)
        if (
            cut.get("kind") == "low_bits"
            and cut["bits"] == 160
            and cut["args"][0]["type"] == WORD
        ):
            args[1] = cut["args"][0]
            stats["effective_call_targets"] += 1
    zero = literal(0, WORD)
    for offset in (input_offset, input_offset + 2):
        if args[offset + 1] == zero and args[offset] != zero:
            args[offset] = zero
            stats["empty_call_ranges"] += 1
    return {**inst, "operation": {**op, "args": args}}


def erase_dead(blocks, retained, stats):
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    live = set(retained)
    pending = deque(retained)
    while pending:
        result = pending.popleft()
        if result not in live or uses[result]:
            continue
        live.remove(result)
        inst = retained[result]
        if inst["operation"].get("name") == "codesize":
            stats["discarded_code_sizes"] += 1
        else:
            stats["dead_scalars"] += 1
        for operand in dead_ssa.reads(inst, reject_undefined=False):
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
    updated, stats, sites, return_sizes = {}, Counter(), set(), set()
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
                if (
                    returned_word(inst, "returndatasize")
                    and definitions["returndatasize"]["forks"][fork]["available"]
                ):
                    return_sizes.add(result)
                instructions.append(
                    effective_call(inst, retained, definitions, fork, stats)
                )
                continue
            if inst["type"] == FLAG:
                if returndata_guard(inst["operation"], retained, return_sizes):
                    aliases[result] = literal(0, FLAG)
                    stats["returndata_extent_guards"] += 1
                    continue
                op, rewrite = unsigned_test(inst["operation"], retained)
                if rewrite:
                    inst = {**inst, "operation": op}
                    stats[rewrite] += 1
                op, rewrite = zero_test(op, retained)
                if rewrite:
                    inst = {**inst, "operation": op}
                    stats[rewrite] += 1
            if operand := scalar_alias(inst, retained):
                aliases[result] = operand
                stats["scalar_aliases"] += 1
                continue
            op = inst["operation"]
            if op.get("name") == "or":
                left, right = op["args"]
                if significant_bits(right, retained) < significant_bits(left, retained):
                    inst = {**inst, "operation": {**op, "args": [right, left]}}
                    stats["ordered_bitwise_operands"] += 1
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
    if definitions["codesize"]["forks"][fork]["available"]:
        for block in result:
            for inst in block["instructions"]:
                if returned_word(inst, "codesize"):
                    retained[inst["result"]] = inst
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
