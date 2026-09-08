"""Mine bounded pure trees from printed MIR for offline, verified discovery.

This is a candidate reader, not a MIR parser or an optimizer. Only direct word
operations from instruction selection are recognized. Everything else ends a
region. Shared definitions and values outside the region become independent
inputs; we never credit deletion of a shared producer. The resulting identities
still require SMT proof and scheduled-code benchmarks before integration.
"""

from collections import Counter
import hashlib
import re

from .discovery import Prices, pseudocode
from .isle import ISLE, opcode_bindings
from .semantics import Expr, Model, Unsupported


VALUE = r"(?:v\d+|arg\d+)"
INSTRUCTION = re.compile(r"\s*(v\d+) = ([a-z]+)(?: (.*))?")
OPERAND = re.compile(rf"(?:{VALUE}|0x[0-9a-fA-F]+|[0-9]+)")


def tree_json(expr):
    if expr.op in ("var", "const"):
        return expr.args[0]
    return [expr.op, *(tree_json(child) for child in expr.args)]


def mine(paths, *, fork="osaka", objective="gas", runs=200, max_ops=8, max_seeds=128):
    if not 2 <= max_ops <= 16 or not 1 <= max_seeds <= 128:
        raise ValueError("mining requires two to 16 operations and one to 128 seeds")
    prices = Prices(fork, objective, runs)
    bindings = opcode_bindings((ISLE / "select.isle").read_text())
    supported = {}
    for name, (opcode, arity, shape, direct) in bindings.items():
        if (direct and opcode in prices.ops and name[3:].lower() == opcode
                and shape in ("OpcodeLowering.Unary", "OpcodeLowering.Binary")):
            try:
                Model().eval(Expr(opcode, (Expr.var("x"),) * arity))
            except Unsupported:
                continue
            supported[opcode] = arity
    candidates, sources = {}, []
    skipped = Counter()
    for path in sorted(set(paths)):
        data = path.read_bytes()
        sources.append(dict(path=str(path), sha256=hashlib.sha256(data).hexdigest()))
        lines = data.decode().splitlines()
        # Value IDs are function-local. Count every textual use, including
        # terminators and unsupported instructions, before choosing tree edges.
        functions = []
        for line_number, line in enumerate(lines, 1):
            if line.startswith("fn "):
                functions.append((line, []))
            elif functions:
                functions[-1][1].append((line_number, line))
        for function, body in functions:
            uses = Counter()
            for _, line in body:
                rhs = line.split(" = ", 1)[-1]
                uses.update(re.findall(rf"\b{VALUE}\b", rhs))
            definitions, block = {}, ""
            for line_number, line in body:
                if re.fullmatch(r"\s*bb\d+:.*", line):
                    block = line.strip()
                match = INSTRUCTION.fullmatch(line)
                if not match or match[2] not in supported:
                    definitions.clear()
                    continue
                value, opcode, operands = match.groups()
                operands = tuple((operands or "").split(", "))
                if (len(operands) != supported[opcode]
                        or any(not OPERAND.fullmatch(operand) for operand in operands)):
                    definitions.clear()
                    skipped["unrecognized_operands"] += 1
                    continue
                definitions[value] = opcode, operands
                variables, budget = {}, [max_ops]

                def expand(value, root=False):
                    if value in definitions and (root or uses[value] == 1):
                        budget[0] -= 1
                        if budget[0] < 0:
                            raise ValueError("operation_bound")
                        op, args = definitions[value]
                        return Expr(op, tuple(expand(arg) for arg in args))
                    if value[0].isdigit():
                        constant = int(value, 16 if value.startswith("0x") else 10)
                        if constant in prices.constants:
                            return Expr.const(constant)
                    if value not in variables:
                        if len(variables) == 3:
                            raise ValueError("variable_bound")
                        variables[value] = "xyz"[len(variables)]
                    return Expr.var(variables[value])

                try:
                    expr = expand(value, root=True)
                except ValueError as error:
                    skipped[str(error)] += 1
                    continue
                if expr.operators() < 2:
                    continue
                entry = candidates.setdefault(expr, dict(
                    tree=tree_json(expr), pattern=pseudocode(expr), occurrences=0,
                    estimated_cost=prices.cost(expr).__dict__, examples=[]))
                entry["occurrences"] += 1
                if len(entry["examples"]) < 4:
                    entry["examples"].append(dict(source=str(path), function=function,
                                                  block=block, line=line_number, value=value))
    ranked = sorted(candidates.values(), key=lambda row: (
        -row["occurrences"] * prices.key(prices.cost(_expr(row["tree"])))[0],
        -row["occurrences"], row["pattern"]))
    return dict(sources=sources, candidates=ranked[:max_seeds],
                summary=dict(unique_trees=len(ranked), selected=min(len(ranked), max_seeds),
                             skipped=dict(skipped)),
                bounds=dict(max_ops=max_ops, max_seeds=max_seeds, max_variables=3),
                fork=fork, objective=objective, expected_executions=runs,
                pricing="Occurrence-weighted tree cost, not measured dynamic frequency or scheduled savings")


def _expr(tree):
    if isinstance(tree, str):
        return Expr.var(tree)
    if isinstance(tree, int):
        return Expr.const(tree)
    return Expr(tree[0], tuple(_expr(child) for child in tree[1:]))
