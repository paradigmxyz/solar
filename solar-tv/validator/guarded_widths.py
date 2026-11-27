"""Project word cleanups justified by an unsigned guard on every incoming path."""

from collections import Counter, defaultdict, deque
from dataclasses import dataclass

from . import dead_ssa, phi_edges, scalar_checks

WORD, FLAG = scalar_checks.WORD, scalar_checks.FLAG
literal = scalar_checks.literal
DEFINED_WORDS = {"arg", "local", "calldata_word"}


def compare(op):
    if op.get("kind") != "opcode" or op.get("name") not in {"lt", "gt"}:
        return None
    if len(op["args"]) != 2 or any(arg["type"] != WORD for arg in op["args"]):
        return None
    return op["args"] if op["name"] == "lt" else op["args"][::-1]


def word_constant(value):
    if value["kind"] == "constant" and value["type"] == WORD:
        amount = int(value["value"])
        if 0 <= amount < 2**256:
            return amount
    return None


def masked(op):
    if op.get("kind") != "opcode" or op.get("name") != "and":
        return None
    if len(op["args"]) != 2 or any(arg["type"] != WORD for arg in op["args"]):
        return None
    args = op["args"]
    for reverse in (False, True):
        value, mask = args[::-1] if reverse else args
        if (
            value["kind"] in DEFINED_WORDS
            and (amount := word_constant(mask)) is not None
        ):
            return value, amount, reverse
    return None


@dataclass(frozen=True)
class Bound:
    expression: str
    inputs: str
    premises: tuple[str, ...]
    proof: tuple[str, ...]
    sites: tuple[tuple, ...] = ()


class Guards:
    def __init__(self, blocks):
        self.successors = [phi_edges.targets(block["terminator"]) for block in blocks]
        self.definitions = {}
        self.unsigned = defaultdict(list)
        self.subtract = defaultdict(list)
        self.cuts = {}
        for index, block in enumerate(blocks):
            for phi in block["phis"]:
                self.definitions[phi["result"]] = (index, {})
            for item in block["instructions"]:
                if item["result"] is not None:
                    self.definitions[item["result"]] = (index, item["operation"])
        for index, block in enumerate(blocks):
            term = block["terminator"]
            if term["kind"] != "branch" or term["then"] == term["else"]:
                continue
            (flag,) = term["args"]
            owner, op = self.definition(flag)
            if owner != index or flag["type"] != FLAG or not (args := compare(op)):
                continue
            lower, upper = args
            rejected = (index, term["else"])
            if (
                lower["kind"] in DEFINED_WORDS
                and (limit := word_constant(upper)) is not None
            ):
                self.unsigned[dead_ssa.stable(lower)].append(
                    ((index, term["then"]), limit, True)
                )
            if (
                upper["kind"] in DEFINED_WORDS
                and (limit := word_constant(lower)) is not None
            ):
                self.unsigned[dead_ssa.stable(upper)].append(
                    (rejected, limit + 1, False)
                )
            if all(arg["kind"] != "undefined" for arg in args):
                self.subtract[dead_ssa.stable(args)].append(rejected)

    def definition(self, operand):
        if operand["kind"] == "local":
            return self.definitions.get(operand["index"], (0, {}))
        return 0, {}

    def required(self, definition, use, edge):
        key = (definition, edge)
        if key not in self.cuts:
            outside = {0, definition}
            pending = deque(outside)
            while pending:
                start = pending.popleft()
                for target in self.successors[start]:
                    if (start, target) != edge and target not in outside:
                        outside.add(target)
                        pending.append(target)
            self.cuts[key] = outside
        reachable = self.cuts[key]
        if use not in reachable:
            return (definition, use, edge, tuple(sorted(reachable)))
        return None

    def direct(self, operand, use, bound):
        owner, _ = self.definition(operand)
        for edge, limit, strict in self.unsigned[dead_ssa.stable(operand)]:
            site = self.required(owner, use, edge)
            if limit > bound or site is None:
                continue
            original = limit if strict else limit - 1
            compare = (
                f"word {original}#256 = 1" if strict else f"{original}#256 word = 0"
            )
            rule = "upper_of_lt" if strict else "upper_of_not_lt"
            return Bound(
                "word",
                "(word : Word)",
                (f"(accepted : MIR.compare .lt {compare})",),
                (
                    f"have upper := MIR.GuardedWidths.{rule} word {original}#256 accepted",
                    "simp only [BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod] at upper",
                ),
                (site,),
            )
        return None

    def static(self, operand, bound):
        _, op = self.definition(operand)
        if low := masked(op):
            _, mask, _ = low
            if mask < bound:
                return Bound(
                    f"(MIR.eval .and word {mask}#256)",
                    "(word : Word)",
                    (),
                    (
                        f"have upper := MIR.GuardedWidths.masked_upper word {mask}#256",
                        "simp only [BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod] at upper",
                    ),
                )
        return None

    def bound(self, operand, use, limit):
        if known := self.direct(operand, use, limit) or self.static(operand, limit):
            return known
        owner, op = self.definition(operand)
        if (
            op.get("kind") != "opcode"
            or op.get("name") != "sub"
            or len(op["args"]) != 2
        ):
            return None
        left, right = op["args"]
        if left["type"] != WORD or right["type"] != WORD:
            return None
        left_bound = self.static(left, limit) or self.direct(left, owner, limit)
        if left_bound is None:
            return None
        for edge in self.subtract[dead_ssa.stable(op["args"])]:
            # The comparison reads the same SSA values as the subtraction.
            # Keeping both in one block also pins their dynamic activation.
            if edge[0] != owner or (site := self.required(owner, use, edge)) is None:
                continue
            word = left_bound.expression
            return Bound(
                f"(MIR.eval .sub {word} right)",
                f"{left_bound.inputs} (right : Word)",
                (*left_bound.premises, f"(noWrap : MIR.compare .lt {word} right = 0)"),
                (
                    *left_bound.proof,
                    f"have range := MIR.GuardedWidths.sub_upper {word} right noWrap",
                ),
                (*left_bound.sites, site),
            )
        return None

    def aliases(self, inst, block):
        if inst["type"] != WORD or inst["result"] is None:
            return None
        op = inst["operation"]
        if low := masked(op):
            value, mask, reverse = low
            if mask & (mask + 1):
                return None
            bits = mask.bit_length()
            if bits < 256 and (bound := self.bound(value, block, mask + 1)):
                operands = (
                    f"{mask}#256 {bound.expression}"
                    if reverse
                    else f"{bound.expression} {mask}#256"
                )
                return value, bound, bits, f"MIR.eval .and {operands}", reverse
        if op["kind"] == "word" and len(op["args"]) == 1:
            flag = op["args"][0]
            _, test = self.definition(flag)
            if flag["type"] == FLAG and test.get("kind") == "ne":
                a, b = test["args"]
                value = b if a == literal(0, WORD) else a
                if (
                    value["kind"] in DEFINED_WORDS
                    and value["type"] == WORD
                    and literal(0, WORD) in (a, b)
                    and (bound := self.bound(value, block, 2))
                ):
                    return (
                        value,
                        bound,
                        1,
                        f"(MIR.compare .ne {bound.expression} 0).setWidth 256",
                        False,
                    )
        return None


def masked_shift(inst, guards, tag):
    op = inst["operation"]
    if (
        inst["type"] != WORD
        or op.get("kind") != "opcode"
        or op.get("name") != "shl"
        or len(op["args"]) != 2
    ):
        return inst, ""
    count, operand = op["args"]
    amount = word_constant(count)
    _, producer = guards.definition(operand)
    if amount is None or not (source := masked(producer)):
        return inst, ""
    value, mask, reverse = source
    bits = mask.bit_length()
    if mask & (mask + 1) or bits + amount < 256:
        return inst, ""
    masked_word = f"{mask}#256 word" if reverse else f"word {mask}#256"
    proof = (
        f"MIR.GuardedWidths.shl_mask word {amount}#256 {bits} (by decide) (by decide)"
    )
    code = f"""
theorem {tag} (word : Word) :
    MIR.shift .shl (MIR.eval .and {masked_word}) {amount}#256 =
      MIR.shift .shl word {amount}#256 := by
  {"simpa [MIR.eval, BitVec.and_comm] using" if reverse else "exact"} {proof}
#print axioms {tag}
"""
    return {**inst, "operation": {**op, "args": [count, value]}}, code


def equation(tag, bound, bits, original, reverse):
    expression = bound.expression
    facts = "\n".join(f"  {line}" for line in bound.proof)
    mask = f"MIR.GuardedWidths.mask {expression} {bits} (by decide) small"
    if original.startswith("MIR.eval .and"):
        proof = (
            f"simpa [MIR.eval, BitVec.and_comm] using {mask}"
            if reverse
            else f"exact {mask}"
        )
    else:
        proof = f"exact MIR.GuardedWidths.widen_bool {expression} small"
    return f"""
theorem {tag} {bound.inputs} {" ".join(bound.premises)} :
    {original} = {expression} := by
{facts}
  have small : {expression}.toNat < {1 << bits} := by omega
  {proof}
#print axioms {tag}
"""


def packed_blend(inst, guards):
    op = inst["operation"]
    if inst["type"] != WORD or op.get("name") != "xor" or op["kind"] != "opcode":
        return None
    incoming, replaced = op["args"]
    if incoming["kind"] not in DEFINED_WORDS or incoming["type"] != WORD:
        return None
    _, mask_op = guards.definition(replaced)
    field = masked(mask_op)
    if field is None:
        return None
    difference, keep, reversed_mask = field
    _, xor = guards.definition(difference)
    if reversed_mask or xor.get("name") != "xor" or xor["kind"] != "opcode":
        return None
    old, same = xor["args"]
    if same != incoming or old["kind"] not in DEFINED_WORDS or old["type"] != WORD:
        return None
    return old, incoming, keep


def blend_equation(tag, keep):
    low = (2**256 - 1) ^ keep
    return f"""
theorem {tag} (old incoming : Word) :
    MIR.eval .xor incoming (MIR.eval .and (MIR.eval .xor old incoming) {keep}#256) =
      MIR.eval .or (MIR.eval .and old {keep}#256) (MIR.eval .and incoming {low}#256) :=
  MIR.WordRewrite.blend old incoming {keep}#256
#print axioms {tag}
"""


def normalize(body, name):
    blocks = body["blocks"]
    if len(blocks) > 1024:
        return body, {}, ""
    guards = Guards(blocks)
    aliases, updated, proofs, sites = {}, [], [], set()
    stats = Counter()
    fresh = max(guards.definitions, default=0) + 1
    for index, block in enumerate(blocks):
        instructions = []
        for inst in block["instructions"]:
            if blend := packed_blend(inst, guards):
                old, incoming, keep = blend
                low = (2**256 - 1) ^ keep
                instructions.extend(
                    [
                        {
                            "result": fresh,
                            "type": WORD,
                            "operation": {
                                "kind": "opcode",
                                "name": "and",
                                "args": [incoming, literal(low, WORD)],
                            },
                        },
                        {
                            "result": fresh + 1,
                            "type": WORD,
                            "operation": {
                                "kind": "opcode",
                                "name": "and",
                                "args": [old, literal(keep, WORD)],
                            },
                        },
                        {
                            **inst,
                            "operation": {
                                "kind": "opcode",
                                "name": "or",
                                "args": [
                                    scalar_checks.local(fresh + 1, WORD),
                                    scalar_checks.local(fresh, WORD),
                                ],
                            },
                        },
                    ]
                )
                fresh += 2
                proofs.append(blend_equation(f"{name}Blend{len(proofs)}", keep))
                stats["packed_blends"] += 1
                continue
            inst, shift_proof = masked_shift(inst, guards, f"{name}Width{len(proofs)}")
            if shift_proof:
                proofs.append(shift_proof)
                stats["shifted_masks"] += 1
            edit = guards.aliases(inst, index)
            if edit is None:
                instructions.append(inst)
                continue
            value, bound, bits, original, reverse = edit
            aliases[inst["result"]] = value
            sites.update(bound.sites)
            proofs.append(
                equation(f"{name}Width{len(proofs)}", bound, bits, original, reverse)
            )
            stats[
                "low_masks" if original.startswith("MIR.eval .and") else "boolean_words"
            ] += 1
        updated.append({**block, "instructions": instructions})
    if not proofs:
        return body, {}, ""
    for key, value in aliases.items():
        while value["kind"] == "local" and value["index"] in aliases:
            value = aliases[value["index"]]
        aliases[key] = value
    rows = ",".join(
        f"⟨{owner},{use},({edge[0]},{edge[1]}),{list(outside)}⟩"
        for owner, use, edge, outside in sorted(sites)
    )
    edges = ",".join(
        f"({origin},{target})"
        for origin, targets in enumerate(guards.successors)
        for target in targets
    )
    support = (
        f"""
def {name}Edges : List MIR.GuardedWidths.Edge := [{edges}]
def {name}Sites : List MIR.GuardedWidths.Site := [{rows}]
theorem {name}Sound (site : MIR.GuardedWidths.Site) (member : site ∈ {name}Sites) :
    ¬ MIR.GuardedWidths.Bypass {name}Edges site site.useBlock :=
  MIR.GuardedWidths.sites_sound {name}Edges {name}Sites (by decide +kernel) site member
#print axioms {name}Sound
"""
        if sites
        else ""
    )
    return (
        {**body, "blocks": scalar_checks.substitute(updated, aliases)},
        dict(stats),
        support + "".join(proofs),
    )
