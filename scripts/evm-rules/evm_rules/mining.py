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
from .semantics import Expr, MASK, Model, Unsupported


VALUE = r"(?:v\d+|arg\d+)"
INSTRUCTION = re.compile(r"\s*(v\d+) = ([a-z]+)(?: (.*))?")
OPERAND = re.compile(rf"(?:{VALUE}|0x[0-9a-fA-F]+|[0-9]+)")


def tree_json(expr):
    if expr.op in ("var", "const"):
        return expr.args[0]
    return [expr.op, *(tree_json(child) for child in expr.args)]


def abstract_patterns(expr):
    """Replace one operation subtree or literal by an independent input.

    All occurrences of the chosen subtree receive the same input. At most one
    cut is made per pattern, and at most three distinct inputs are retained.
    Alpha-renaming canonicalizes the result. Keep 0, 1 and MAX literal so the
    search can use their identities. Universal verification remains mandatory.
    """
    subtrees = set()

    def collect(node):
        for child in node.args if node.op not in ("var", "const") else ():
            if child.op not in ("var", "const"):
                subtrees.add(child)
                collect(child)
            elif child.op == "const" and child.args[0] not in (0, 1, MASK):
                subtrees.add(child)

    collect(expr)
    seen = {expr}
    for subtree in sorted(subtrees, key=Expr.text):
        variables = {}

        def replace(node):
            if node == subtree:
                node = Expr.var("@abstract")
            if node.op == "var":
                if node.args[0] not in variables:
                    if len(variables) == 3:
                        raise ValueError("variable_bound")
                    variables[node.args[0]] = "xyz"[len(variables)]
                return Expr.var(variables[node.args[0]])
            if node.op == "const":
                return node
            return Expr(node.op, tuple(replace(child) for child in node.args))

        try:
            pattern = replace(expr)
        except ValueError:
            continue
        if pattern.operators() >= 2 and pattern not in seen:
            seen.add(pattern)
            yield pattern


def mine(paths, *, fork="osaka", objective="gas", runs=200, max_ops=8, max_seeds=128,
         abstract_subtrees=False):
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
                patterns = [(expr, False)]
                if abstract_subtrees:
                    patterns.extend((pattern, True) for pattern in abstract_patterns(expr))
                for pattern, abstracted in patterns:
                    entry = candidates.setdefault(pattern, dict(
                        tree=tree_json(pattern), pattern=pseudocode(pattern), occurrences=0,
                        abstract_occurrences=0,
                        estimated_cost=prices.cost(pattern).__dict__, examples=[]))
                    entry["occurrences"] += 1
                    entry["abstract_occurrences"] += abstracted
                    if len(entry["examples"]) < 4:
                        entry["examples"].append(dict(source=str(path), function=function,
                                                      block=block, line=line_number, value=value,
                                                      abstracted=abstracted))
    ranked = sorted(candidates.values(), key=lambda row: (
        -row["occurrences"] * prices.key(prices.cost(_expr(row["tree"])))[0],
        -row["occurrences"], row["pattern"]))
    return dict(sources=sources, candidates=ranked[:max_seeds],
                summary=dict(unique_trees=len(ranked), selected=min(len(ranked), max_seeds),
                             skipped=dict(skipped)),
                bounds=dict(max_ops=max_ops, max_seeds=max_seeds, max_variables=3,
                            max_subtree_cuts=int(abstract_subtrees)),
                fork=fork, objective=objective, expected_executions=runs,
                pricing="Occurrence-weighted tree cost, not measured dynamic frequency or scheduled savings")


def _expr(tree):
    if isinstance(tree, str):
        return Expr.var(tree)
    if isinstance(tree, int):
        return Expr.const(tree)
    return Expr(tree[0], tuple(_expr(child) for child in tree[1:]))
