"""Bounded expression enumeration, concrete fingerprints, SMT and ISLE emission.

Samples only propose equivalences. A representative can replace an expression
in the enumeration frontier only after UNSAT. SAT witnesses refine the sample
set; unknown results remain distinct. Prices come from a Rust Target snapshot.
They estimate trees with resident inputs, not complete scheduled EVM programs.
"""

from dataclasses import dataclass
import hashlib
from itertools import product
from pathlib import Path
import random
import re

from .isle import ROOT, extractor_definitions, forms, verify_file
from .semantics import Expr, MASK, Model, Unsupported, check, concrete

COSTS = ROOT / "crates/codegen/src/word_rule_costs.snap"


@dataclass(frozen=True)
class Cost:
    gas: int
    bytes: int

    def __add__(self, other):
        return Cost(self.gas + other.gas, self.bytes + other.bytes)


class Prices:
    def __init__(self, fork, objective="gas", runs=200):
        self.objective, self.runs = objective, runs
        self.ops, self.constants = {}, {}
        active = False
        for line in COSTS.read_text().splitlines():
            row = line.split()
            if row[0] == "deposit":
                self.deposit = int(row[1])
            elif row[0] == "fork":
                active = row[1] == fork
            elif active and row[0] == "constant":
                value = MASK if row[1] == "max" else int(row[1])
                self.constants[value] = Cost(*map(int, row[2:]))
            elif active and row[0] == "variable":
                self.variable = Cost(*map(int, row[1:]))
            elif active and row[0] == "op":
                self.ops[row[1]] = (int(row[2]), Cost(int(row[3]), int(row[4])), row[5] == "commutative")
        if not self.ops:
            raise ValueError(f"no exported target costs for fork {fork}")

    def cost(self, expr):
        if expr.op == "var":
            return self.variable
        if expr.op == "const":
            return self.constants[expr.args[0]]
        return sum((self.cost(a) for a in expr.args), self.ops[expr.op][1])

    def key(self, cost):
        if self.objective == "size":
            return cost.bytes, cost.gas
        if self.objective == "lifetime":
            return cost.gas * self.runs + cost.bytes * self.deposit, cost.gas, cost.bytes
        return cost.gas, cost.bytes


def samples(variables):
    edges = [0, 1, 2, 255, 256, (1 << 255) - 1, 1 << 255, MASK]
    result = [dict(zip(variables, values)) for values in product(edges, repeat=len(variables))]
    rng = random.Random(0)
    result.extend({v: rng.getrandbits(256) for v in variables} for _ in range(24))
    return result


def enumerate_rules(prices, variables, ops, max_ops, max_expressions, timeout_ms, initial_samples=None, include_constants=False,
                    max_rhs_ops=1, constants=None):
    if not 1 <= len(variables) <= 3 or len(set(variables)) != len(variables):
        raise ValueError("discovery requires one to three distinct variables")
    if any(not re.fullmatch(r"[a-z][a-z0-9_]*", v) or v in ("true", "false") for v in variables):
        raise ValueError("discovery variables must be lowercase ISLE identifiers")
    if max_ops < 1 or max_expressions < 1:
        raise ValueError("search bounds must be positive")
    if not 1 <= max_rhs_ops <= max_ops:
        raise ValueError("replacement bound must be between one and max-ops")
    constants = list(dict.fromkeys(constants if constants is not None else [0, 1, MASK]))
    if any(c not in prices.constants for c in constants):
        raise ValueError("constant has no exported Target price")
    for op in ops:
        if op not in prices.ops or prices.ops[op][0] not in (1, 2):
            raise ValueError(f"unsupported search operation on selected fork: {op}")
        Model.apply(op, tuple(Model().eval(Expr.var(v)) for v in variables[:1]) * prices.ops[op][0])
    inputs = initial_samples if initial_samples is not None else samples(variables)
    inputs_constants = set(constants)
    # Canonical identities remain available as results even in a specialized
    # input domain, so zero is never represented by an expensive expression.
    leaves = [Expr.var(v) for v in variables] + [Expr.const(v) for v in dict.fromkeys([0, 1, MASK, *constants])]
    frontier = [[e for e in leaves if e.op == "var" or include_constants and e.args[0] in inputs_constants]] + [[] for _ in range(max_ops)]
    representatives = list(leaves)
    buckets = {}
    model = Model()
    statistics = dict(expressions=0, smt_queries=0, proved=0, counterexamples=0, unknown=0)
    discovered = []

    def fingerprint(expr):
        return tuple(concrete(expr, row) for row in inputs)

    def insert(expr):
        buckets.setdefault(fingerprint(expr), []).append(expr)

    for expr in leaves:
        insert(expr)
    bounded = False
    for count in range(1, max_ops + 1):
        for op in ops:
            arity, _, commutative = prices.ops[op]
            partitions = [(count - 1,)] if arity == 1 else [(i, count - 1 - i) for i in range(count)]
            for partition in partitions:
                for children in product(*(frontier[n] for n in partition)):
                    if commutative and children[0].text() > children[1].text():
                        continue
                    if statistics["expressions"] >= max_expressions:
                        bounded = True
                        break
                    expr = Expr(op, children)
                    statistics["expressions"] += 1
                    equal = None
                    candidates = sorted(buckets.get(fingerprint(expr), []), key=lambda e: (prices.key(prices.cost(e)), e.text()))
                    for candidate in candidates:
                        statistics["smt_queries"] += 1
                        result, _ = check(expr, candidate, timeout_ms=timeout_ms, model=model)
                        statistics["counterexamples" if result["status"] == "counterexample" else result["status"]] += 1
                        if result["status"] == "proved":
                            equal = candidate
                            before, after = prices.cost(expr), prices.cost(candidate)
                            if candidate.operators() <= max_rhs_ops and prices.key(after) < prices.key(before):
                                discovered.append((expr, candidate, before, after))
                            if prices.key(before) < prices.key(after):
                                if count <= max_rhs_ops and candidate.operators() >= 1:
                                    discovered.append((candidate, expr, after, before))
                                # Replace the representative only after proving equivalence.
                                # Otherwise an expensive early spelling (xor MAX x) would
                                # suppress a later, cheaper one (not x) in every larger tree.
                                representatives.remove(candidate)
                                if candidate in frontier[candidate.operators()]:
                                    frontier[candidate.operators()].remove(candidate)
                                buckets[fingerprint(expr)].remove(candidate)
                                representatives.append(expr)
                                frontier[count].append(expr)
                                insert(expr)
                            break
                        if result["status"] == "counterexample":
                            inputs.append({v: int(result["inputs"].get(v, "0"), 16) for v in variables})
                            buckets.clear()
                            for previous in representatives:
                                insert(previous)
                    if equal is None:
                        frontier[count].append(expr)
                        representatives.append(expr)
                        insert(expr)
                if bounded:
                    break
            if bounded:
                break
        if bounded:
            break
    statistics.update(samples=len(inputs), representatives=len(representatives), budget_exhausted=bounded)
    return discovered, statistics


def spell(expr, top=False, literals=None, recipe=False):
    if expr.op == "var":
        return expr.args[0]
    if expr.op == "const":
        value = expr.args[0]
        if recipe:
            return f"(imm {constant_source(value)})"
        if value in (0, 1, MASK):
            return {0: "(zero)", 1: "(one)", MASK: "(all_ones)"}[value]
        if literals is None:
            raise Unsupported("literal pattern requires an equality guard")
        return f"(iconst {literals.setdefault(value, f'c{len(literals)}')})"
    aliases = extractor_definitions()
    for name, (_, body) in aliases.items():
        if body[0] == "inst" and body[1][0].startswith("Op.") and body[1][0][3:].lower() == expr.op:
            operator = body[1][0] if top or recipe else name
            node = f"({operator} {' '.join(spell(a, literals=literals, recipe=recipe) for a in expr.args)})"
            return f"(make {node})" if recipe and not top else node
    raise Unsupported(f"no schema-generated extractor for {expr.op}")


def constant_source(value):
    if value == MASK:
        return "(u256_max)"
    if value < 1 << 64:
        return f"(u256 {value})"
    limbs = [(value >> (64 * i)) & ((1 << 64) - 1) for i in range(4)]
    return f"(u256_from_limbs {' '.join(map(str, limbs))})"


def emit_rule(lhs, rhs, prices=None):
    if lhs.operators() == 0:
        raise Unsupported("emission requires an operation root")
    names = {}

    def rename(expr):
        if expr.op == "var":
            return Expr.var(names.setdefault(expr.args[0], f"v{len(names)}"))
        if expr.op == "const":
            return expr
        return Expr(expr.op, tuple(rename(a) for a in expr.args))

    lhs, rhs = rename(lhs), rename(rhs)
    prices = prices or Prices("osaka")
    def canonical(expr):
        if expr.op in ("var", "const"):
            return expr
        children = tuple(canonical(a) for a in expr.args)
        if expr.op in prices.ops and prices.ops[expr.op][2]:
            children = tuple(sorted(children, key=Expr.text))
        return Expr(expr.op, children)

    rhs = canonical(rhs)
    if rhs.op == "const":
        value = constant_source(rhs.args[0])
        # A rewrite returning an Op avoids priority conflicts with existing simplify rules.
        # Add with zero simplifies immediately, without allocating another instruction.
        replacement = f"(Op.Add (imm {value}) (imm (u256 0)))"
    elif rhs.op == "var":
        replacement = f"(Op.Add {rhs.args[0]} (imm (u256 0)))"
    else:
        replacement = spell(rhs, top=True, recipe=True)
    literals = {}
    pattern = spell(lhs, top=True, literals=literals)
    guards = ''.join(f" (if-let true (u256_same {name} {constant_source(value)}))"
                     for value, name in literals.items())
    root = "sequence_rewrite" if rhs.operators() > 1 else "rewrite"
    if rhs.operators() > 1:
        replacement = f"(sequence {replacement})"
        stages, replacement = stage_recipe(forms(replacement)[0][0])
        guards += ''.join(f" (if-let {name} {expression})" for name, expression in stages)
    return f"(rule ({root} {pattern}){guards} {replacement})"


def stage_recipe(tree):
    """Bind fallible temporary constructors before the total recipe result."""
    stages = []

    def visit(node):
        if isinstance(node, str):
            return node
        name, *args = node
        expression = f"({name} {' '.join(visit(a) for a in args)})"
        if name in ("make", "imm"):
            binding = f"r{len(stages)}"
            stages.append((binding, expression))
            return binding
        return expression

    result = visit(tree)
    return stages, result


def variants(expr, prices):
    if expr.op in ("var", "const"):
        return [expr]
    result = set()
    for children in product(*(variants(a, prices) for a in expr.args)):
        result.add(Expr(expr.op, children))
        if prices.ops[expr.op][2]:
            result.add(Expr(expr.op, tuple(reversed(children))))
    return sorted(result, key=Expr.text)


def discover_rules(args):
    if args.runs < 0 or args.max_rules <= 0:
        raise ValueError("runs must be nonnegative and max-rules positive")
    prices = Prices(args.evm_version, args.objective, args.runs)
    result_ops = getattr(args, "result_ops", None)
    if result_ops and any(op not in prices.ops and op not in ("var", "const") for op in result_ops):
        raise ValueError("unsupported replacement root filter")
    rules, summary = enumerate_rules(prices, args.variables, args.ops, args.max_ops,
                                     args.max_expressions, args.timeout_ms, include_constants=args.include_constants,
                                     max_rhs_ops=getattr(args, "max_rhs_ops", 1), constants=getattr(args, "constants", None))
    # A repeated identical child is handled by existing generic idempotence/
    # cancellation rules; keep discovery proposals focused on mixed shapes.
    rules = [r for r in rules if not (len(r[0].args) == 2 and r[0].args[0] == r[0].args[1])]
    if result_ops:
        rules = [r for r in rules if r[1].op in result_ops]
    # Prefer reductions involving all requested inputs; keep deterministic tie-breaking.
    rules.sort(key=lambda r: (-len(r[0].variables()), -(r[0].operators() - r[1].operators()),
                             -(r[2].gas - r[3].gas), -(r[2].bytes - r[3].bytes), r[0].text()))
    selected, emitted, seen = [], [], set()
    for lhs, rhs, before, after in rules:
        for variant in variants(lhs, prices):
            source = emit_rule(variant, rhs, prices)
            if source in seen:
                continue
            seen.add(source)
            emitted.append(source)
            selected.append(dict(lhs=variant.text(), rhs=rhs.text(), estimated_before=before.__dict__,
                                 estimated_after=after.__dict__, isle=source))
            if len(emitted) >= args.max_rules:
                break
        if len(emitted) >= args.max_rules:
            break
    report = dict(summary=summary, candidates=selected, fork=args.evm_version,
                  objective=args.objective, expected_executions=args.runs,
                  result_ops=result_ops,
                  bounds=dict(max_ops=args.max_ops, max_expressions=args.max_expressions,
                              include_constants=args.include_constants, max_rhs_ops=getattr(args, "max_rhs_ops", 1),
                              constants=getattr(args, "constants", None)),
                  costs_sha256=hashlib.sha256(COSTS.read_bytes()).hexdigest(),
                  pricing="Rust Target snapshot; tree estimate with resident variable inputs")
    if args.emit_isle:
        args.emit_isle.parent.mkdir(parents=True, exist_ok=True)
        args.emit_isle.write_text(";; Offline-discovered candidates; benchmark before integrating.\n\n" + "\n".join(emitted) + "\n")
        # Verify the exact emitted source, not just the internal expressions that inspired it.
        if emitted:
            report["emitted_verification"] = verify_file(args.emit_isle, args.timeout_ms)
            report["accepted"] = all(r["status"] == "proved" for r in report["emitted_verification"]["rules"])
        else:
            report["emitted_verification"] = {"status": "no_candidates", "rules": []}
    return report
