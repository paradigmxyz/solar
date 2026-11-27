"""Keep typed scalar identities and checked guards in the same CFG proof."""

from collections import Counter, defaultdict

from . import dead_ssa, phi_edges

WORD = {"kind": "word", "bits": 256, "name": "i256"}
FLAG = {"kind": "word", "bits": 1, "name": "i1"}


def local(index, ty):
    return {"kind": "local", "index": index, "type": ty}


def literal(value, ty):
    return {"kind": "constant", "value": str(value), "type": ty}


def inactive(op):
    return op["kind"] == "check" and op["args"] == [literal(int(op["zero"]), FLAG)]


def argument_length(inst):
    op = inst["operation"]
    if (
        inst["result"] is not None
        and inst["type"] == WORD
        and op["kind"] == "extract_value"
        and op["aggregate_type"]["kind"] == "slice"
        and op["index"] == 1
        and len(op["args"]) == 1
        and op["args"][0]["kind"] == "arg"
    ):
        return dead_ssa.stable(op)
    return None


def substitute(value, aliases):
    if isinstance(value, list):
        return [substitute(item, aliases) for item in value]
    if isinstance(value, dict):
        if value.get("kind") == "local":
            return aliases.get(value["index"], value)
        return {key: substitute(item, aliases) for key, item in value.items()}
    return value


def identity(inst, defined):
    op = inst["operation"]
    args = op["args"]
    ty = inst["type"]
    if inst["result"] is None:
        return None
    if (
        ty == WORD
        and op["kind"] == "word"
        and args
        in (
            [literal(0, FLAG)],
            [literal(1, FLAG)],
        )
    ):
        return literal(args[0]["value"], WORD)
    if op["kind"] != "opcode" or len(args) != 2:
        return None
    left, right = args
    if not (
        left["kind"] == "constant"
        or left["kind"] == "local"
        and left["index"] in defined
    ):
        return None
    if (
        ty == WORD
        and op["name"] in {"mul", "div"}
        and left["type"] == WORD
        and right == literal(1, WORD)
    ):
        return left
    if (
        ty == FLAG
        and op["name"] == "or"
        and left["type"] == FLAG
        and right == literal(0, FLAG)
    ):
        return left
    if ty == FLAG and op["name"] == "eq" and left["type"] == WORD:
        if left == right:
            return literal(1, FLAG)
        if left == literal(1, WORD) and right == literal(0, WORD):
            return literal(0, FLAG)
    return None


def polarity(negation, check, uses):
    """Combine a private boolean complement with its check."""
    op, effect = negation["operation"], check["operation"]
    if (
        effect["kind"] != "check"
        or negation["type"] != FLAG
        or negation["result"] is None
        or op["kind"] != "opcode"
        or op["name"] != "eq"
        or len(op["args"]) != 2
    ):
        return None
    flag, zero = op["args"]
    if flag["type"] != FLAG or zero != literal(0, FLAG):
        return None
    count = uses[negation["result"]]
    if (
        not effect["zero"]
        and count == 1
        and effect["args"] == [local(negation["result"], FLAG)]
    ):
        return {**check, "operation": {**effect, "args": [flag], "zero": True}}
    if effect["zero"] and count == 0 and effect["args"] == [flag]:
        return check
    return None


def preceding_comparisons(blocks):
    parents = defaultdict(set)
    for index, block in enumerate(blocks):
        for target in phi_edges.targets(block["terminator"]):
            parents[target].add(index)
    comparisons = {}
    for index, sources in parents.items():
        if index == 0 or len(sources) != 1:
            continue
        (parent,) = sources
        if parent == index:
            continue
        term = blocks[parent]["terminator"]
        if term["kind"] != "branch":
            continue
        (flag,) = term["args"]
        if flag["kind"] != "local" or flag["type"] != FLAG:
            continue
        for inst in blocks[parent]["instructions"]:
            op = inst["operation"]
            if (
                inst["result"] == flag["index"]
                and inst["type"] == FLAG
                and op["kind"] == "opcode"
                and op["name"] == "lt"
                and len(op["args"]) == 2
                and all(arg["type"] == WORD for arg in op["args"])
            ):
                operands = set(dead_ssa.reads(op, reject_undefined=False))
                if any(arg["kind"] == "undefined" for arg in op["args"]):
                    continue
                comparisons[index] = (parent, op, flag, operands | {flag["index"]})
    return comparisons


def reuse_comparisons(blocks, stats):
    aliases = {}
    comparisons = preceding_comparisons(blocks)
    kept_flags = {flag["index"] for _, _, flag, _ in comparisons.values()}
    for index, (_, operation, flag, dependencies) in comparisons.items():
        block = blocks[index]
        if dependencies.intersection(phi["result"] for phi in block["phis"]):
            continue
        retained = []
        available = True
        for inst in block["instructions"]:
            result = inst["result"]
            if (
                available
                and result not in kept_flags
                and inst["operation"] == operation
                and inst["type"] == FLAG
            ):
                aliases[result] = flag
                stats["reused_comparisons"] += 1
                continue
            retained.append(inst)
            available = available and result not in dependencies
        block["instructions"] = retained
    return substitute(blocks, aliases)


def normalize(body):
    blocks = body["blocks"]
    projections = {}
    for inst in blocks[0]["instructions"]:
        if key := argument_length(inst):
            projections.setdefault(key, local(inst["result"], WORD))
    defined = {value["index"] for value in projections.values()}
    aliases, stats = {}, Counter()
    normalized = []
    uses = Counter(dead_ssa.reads(blocks, reject_undefined=False))
    for block in blocks:
        retained = []
        instructions = iter(block["instructions"])
        for original in instructions:
            if key := argument_length(original):
                first = projections.get(key)
                if first is not None and first["index"] != original["result"]:
                    aliases[original["result"]] = first
                    stats["argument_projections"] += 1
                    continue
            inst = substitute(original, aliases)
            if replacement := identity(inst, defined):
                aliases[inst["result"]] = replacement
                stats["scalar_identities"] += 1
                continue
            op = inst["operation"]
            if inactive(op):
                stats["inactive_checks"] += 1
                continue
            if retained and (canonical := polarity(retained[-1], inst, uses)):
                retained.pop()
                inst = canonical
                stats["check_polarities"] += 1
            retained.append(inst)
        normalized.append({**block, "instructions": retained})
    updated = reuse_comparisons(substitute(normalized, aliases), stats)
    return {**body, "blocks": updated}, dict(stats)
