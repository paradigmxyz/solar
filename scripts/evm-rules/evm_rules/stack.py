"""Prove the compiled pure stack-rule subset for every legal physical depth.

The model compares the entire touched stack and checks input and peak-depth
requirements. It assumes well-formed input code with sufficient stack and gas;
it does not preserve out-of-gas or underflow behavior of malformed code. An
arbitrary deeper prefix is untouched by either sequence. A window may also hold
pushes of literal words and the pure word operations of `OPERATIONS`; a guard
fixes the pushed word of a literal predicate. Rust extraction, edit
application, profitability guards and opcode lowering remain explicit trusted
contracts.
"""

from itertools import product

from .expr import Expr, Unsupported
from .isle import Rule, forms

# Pure word operations of a window, with the words each pops; each pushes one.
OPERATIONS = {"not": 1, "iszero": 1, "shl": 2, "or": 2, "add": 2, "sub": 2, "eq": 2}
# Guards that fix a pushed word, and guards that only price a rewrite.
LITERAL_GUARDS = {"u256_is_one": 1}
PRICE_GUARDS = {"complement_odd_dominates"}


def instruction(pattern, bindings):
    name, *args = pattern
    if name in ("dup", "swap", "exchange"):
        depths = tuple(
            bindings[arg] if not arg.isdecimal() else int(arg) for arg in args
        )
        if len(depths) != (2 if name == "exchange" else 1) or any(
            not 1 <= depth <= 235 for depth in depths
        ):
            raise Unsupported("invalid physical depth")
        return name, depths
    if name == "pop" and not args:
        return "pop", ()
    if name == "push" and len(args) == 1:
        return "push", (bindings.get(args[0], args[0]),)
    if name == "opcode" and len(args) == 1:
        opcode = args[0].removeprefix("$").lower()
        if opcode in ("pop", *OPERATIONS):
            return opcode, ()
        for kind in ("dup", "swap"):
            if opcode.startswith(kind) and opcode[len(kind) :].isdecimal():
                depth = int(opcode[len(kind) :])
                if 1 <= depth <= 16:
                    return kind, (depth,)
    raise Unsupported(f"unmodeled physical instruction: {pattern}")


def requirements(sequence):
    minimum = height = peak = 0
    for name, args in sequence:
        required = {"pop": 1, "push": 0, **OPERATIONS}.get(name)
        if name == "dup":
            required = args[0]
        elif name in ("swap", "exchange"):
            required = max(args) + 1
        if required is None:
            raise Unsupported(f"unmodeled stack operation: {name}")
        minimum = max(minimum, required - height)
        height += {"dup": 1, "push": 1, "pop": -1, "swap": 0, "exchange": 0}.get(
            name, 1 - OPERATIONS.get(name, 0)
        )
        peak = max(peak, height)
    return minimum, peak, height


def execute(sequence, inputs):
    stack = list(inputs)
    for name, args in sequence:
        if name == "dup":
            stack.append(stack[-args[0]])
        elif name == "push":
            literal = args[0]
            stack.append(
                Expr.const(literal) if isinstance(literal, int) else Expr.var(literal)
            )
        elif name == "swap":
            index = -args[0] - 1
            stack[-1], stack[index] = stack[index], stack[-1]
        elif name == "exchange":
            a, b = (-index - 1 for index in args)
            stack[a], stack[b] = stack[b], stack[a]
        elif name == "pop":
            stack.pop()
        elif OPERATIONS.get(name) == 2:
            a, b = stack.pop(), stack.pop()
            stack.append(Expr(name, (a, b)))
        else:
            stack[-1] = Expr(name, (stack[-1],))
    return stack


def tail_class_root(pattern):
    """Returns the peephole term for rules whose last instruction matches `pattern`."""
    head, *args = pattern
    if head in ("dup", "swap", "exchange", "pop"):
        return f"peep_{head}"
    if head == "opcode" and len(args) == 1:
        opcode = args[0].removeprefix("$")
        if opcode == "POP":
            return "peep_pop"
        for name in ("DUP", "SWAP"):
            if opcode.startswith(name) and opcode[len(name) :].isdecimal():
                return f"peep_{name.lower()}"
    return "peep_op"


# Trusted contracts of every physical stack rule; reports list them with the proofs.
CONTRACTS = [
    "canonical physical stack facets and legal depth encodings",
    "unprotected_last4 and unprotected_last5 reject protected boundaries",
    "Edit.Keep truncates; Edit.OverwriteOne replaces the matched window",
    "Edit.RemoveFirstKeepTwo retains window instructions one and two",
    "Edit.OddWord replaces a four-instruction window with DUP1 NOT SWAP1 SUB",
    "u256_is_one holds exactly for the word one; complement_odd_dominates only prices the rewrite",
    "sufficient input stack and gas; untouched deeper stack prefix",
    "target lowering preserves physical stack operation semantics",
]


def stack_rules(path):
    """Read the rules of a stack proof file, failing on any other form."""
    source = path.read_text()
    rules = []
    for form, line in forms(source):
        if form[0] != "rule":
            raise Unsupported("stack proof files may only contain rules")
        rules.append(Rule(form, line, str(path)))
    if not rules:
        raise ValueError("stack proof file contains no rules")
    return source, rules


def stack_variants(rule):
    """Return every legal depth binding with the word difference it must zero.

    The difference ORs the XOR of every touched stack slot whose results are
    not structurally identical, so it is zero exactly when both sequences
    leave the same stack.
    """
    form = rule.form
    body = form[2:] if isinstance(form[1], str) and form[1].isdecimal() else form[1:]
    if len(body) < 2:
        raise Unsupported("unmodeled stack rule shape")
    (root, window), *guards, (rewrite, skip, edit) = body
    name, *patterns = window
    patterns = [
        tuple(
            f"wildcard_{i}_{j}" if arg == "_" else arg for j, arg in enumerate(pattern)
        )
        for i, pattern in enumerate(patterns)
    ]
    if (
        root != tail_class_root(patterns[-1])
        or name not in (f"last{len(patterns)}", f"unprotected_last{len(patterns)}")
        or rewrite != "rewrite"
        or int(skip) != len(patterns)
    ):
        raise Unsupported("unmodeled window or rewrite extent")
    literals = {}
    pushed = {pattern[1] for pattern in patterns if pattern[0] == "push"}
    for guard in guards:
        match guard:
            case ("if-let", "true", (predicate, word)) if (
                LITERAL_GUARDS.get(predicate) is not None and word in pushed
            ):
                literals[word] = LITERAL_GUARDS[predicate]
            case ("if-let", "true", (predicate,)) if predicate in PRICE_GUARDS:
                pass
            case _:
                raise Unsupported(f"unmodeled stack rule guard: {guard}")
    variables = sorted(
        {
            arg
            for pattern in patterns
            if pattern[0] in ("dup", "swap", "exchange")
            for arg in pattern[1:]
            if not arg.isdecimal()
        }
    )
    if len(variables) > 2:
        raise Unsupported("stack rule has more than two depth bindings")
    variants = []
    # DUPN/SWAPN encode depths 1..235. EXCHANGE admits n < m,
    # n + m <= 30; classic forks lower its subset to three swaps.
    for values in product(range(1, 236), repeat=len(variables)):
        bindings = {**literals, **dict(zip(variables, values))}
        before = [instruction(pattern, bindings) for pattern in patterns]
        if any(
            name == "exchange" and not (1 <= args[0] < args[1] and sum(args) <= 30)
            for name, args in before
        ):
            continue
        if edit[0] == "Edit.Keep" and len(edit) == 2 and edit[1].isdecimal():
            keep = int(edit[1])
            if keep > len(before):
                raise Unsupported("edit retains instructions outside the window")
            after = before[:keep]
        elif edit == ("Edit.RemoveFirstKeepTwo",) and len(before) >= 3:
            after = before[1:3]
        elif edit[0] == "Edit.OverwriteOne" and len(edit) == 2:
            after = [instruction(("opcode", edit[1]), bindings)]
        elif edit == ("Edit.OddWord",):
            after = [
                instruction(("opcode", f"${opcode}"), bindings)
                for opcode in ("DUP1", "NOT", "SWAP1", "SUB")
            ]
        else:
            raise Unsupported(f"unmodeled stack edit: {edit}")
        needed, peak, delta = requirements(before)
        new_needed, new_peak, new_delta = requirements(after)
        if new_needed > needed or new_peak > peak or new_delta != delta:
            raise Unsupported(
                "replacement increases stack requirements or changes height"
            )
        inputs = [Expr.var(f"s{i}") for i in range(needed)]
        lhs, rhs = execute(before, inputs), execute(after, inputs)
        difference = Expr.const(0)
        for a, b in zip(lhs, rhs):
            if a != b:
                difference = Expr("or", (difference, Expr("xor", (a, b))))
        variants.append(
            {
                "bindings": bindings,
                "minimum_stack": needed,
                "peak_growth": peak,
                "difference": difference,
            }
        )
    if not variants:
        raise Unsupported("no applicable physical stack bindings")
    return variants
