"""State rule obligations as Lean theorems over the EVM semantics in `lean/EvmRules`.

Words become `EvmRules.Word` terms built from the `Evm` definitions, and preconditions
become hypotheses. A rule holds when its theorem `hypotheses → lhs = rhs` does. Z3
plays no part: this reads the solver-independent terms of `expr.py` directly. Free
Boolean flags that a rule forces to one value are substituted before the statement is
printed, so hypotheses state only word properties.
"""

import re

from .expr import Cond, Expr, Unsupported

PRELUDE = """import EvmRules

open EvmRules

set_option maxHeartbeats 0
set_option maxRecDepth 100000
set_option linter.unusedVariables false
"""

# Every word operation the semantics define, with its arity.
OPERATIONS = {
    **dict.fromkeys(["not", "iszero", "clz"], 1),
    **dict.fromkeys(
        [
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "sdiv",
            "smod",
            "exp",
            "and",
            "or",
            "xor",
            "shl",
            "shr",
            "sar",
            "lt",
            "gt",
            "slt",
            "sgt",
            "eq",
            "ne",
            "byte",
            "signextend",
        ],
        2,
    ),
    **dict.fromkeys(["addmod", "mulmod", "select"], 3),
}
ADDRESS = "@environment:address"
BALANCES = "@environment:balances"
KEYWORDS = {
    "at",
    "by",
    "do",
    "else",
    "end",
    "fun",
    "have",
    "if",
    "in",
    "let",
    "match",
    "open",
    "show",
    "then",
    "with",
    "where",
    "from",
    "for",
    "theorem",
    "example",
    "Word",
    "Evm",
    "EvmRules",
}


def lean_name(name):
    """A Lean identifier for an ISLE or proof variable; guillemets quote the rest."""
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_']*", name) and name not in KEYWORDS:
        return name
    if "«" in name or "»" in name:
        raise Unsupported(f"variable name cannot be quoted in Lean: {name}")
    return f"«{name}»"


def literal(value):
    return str(value) if value < 1 << 32 else hex(value)


def term(expr):
    """The Lean text of a word."""
    match expr.op, expr.args:
        case "var", (name,):
            return lean_name(name)
        case "const", (value,):
            return literal(value)
        case "address", ():
            return f"(Evm.address {lean_name(ADDRESS)})"
        case "balance", (account,):
            return f"(Evm.balance {lean_name(BALANCES)} {term(account)})"
        case "selfbalance", ():
            return f"(Evm.selfbalance {lean_name(BALANCES)} {lean_name(ADDRESS)})"
        case op, args if OPERATIONS.get(op) == len(args):
            return f"(Evm.{op} {' '.join(term(arg) for arg in args)})"
    raise Unsupported(f"no Lean semantics for {expr.op}/{len(expr.args)}")


def prop(cond):
    """The Lean text of a precondition."""
    match cond.op, cond.args:
        case "const", (value,):
            return "True" if value else "False"
        case "flag", (name,):
            return f"({lean_name(name)} = true)"
        case "eq", (a, b):
            return f"({term(a)} = {term(b)})"
        case "ne", (a, b):
            return f"({term(a)} ≠ {term(b)})"
        case "ult", (a, b):
            return f"({term(a)} < {term(b)})"
        case "ule", (a, b):
            return f"({term(a)} ≤ {term(b)})"
        case "ugt", (a, b):
            return f"({term(b)} < {term(a)})"
        case "uge", (a, b):
            return f"({term(b)} ≤ {term(a)})"
        case "msb", (a,):
            return f"({term(a)}.msb = true)"
        case "not", (a,):
            return f"(¬ {prop(a)})"
        case "implies", (a, b):
            return f"({prop(a)} → {prop(b)})"
        case "iff", (a, b):
            return f"({prop(a)} ↔ {prop(b)})"
        case "bool_word", (symbol, a):
            return f"({term(symbol)} = if {prop(a)} then 1 else 0)"
    raise Unsupported(f"no Lean meaning for condition {cond.op}/{len(cond.args)}")


def substitute_flags(cond, values):
    if cond.op == "flag" and cond.args[0] in values:
        return Cond.const(values[cond.args[0]])
    if cond.op in ("not", "implies", "iff"):
        args = tuple(substitute_flags(arg, values) for arg in cond.args)
        match cond.op, args:
            case "not", (Cond("const", (a,)),):
                return Cond.const(not a)
            case "implies", (Cond("const", (True,)), b):
                return b
            case "implies", (Cond("const", (False,)), _):
                return Cond.const(True)
            case "iff", (Cond("const", (a,)), Cond("const", (b,))):
                return Cond.const(a == b)
            case "iff", (Cond("const", (True,)), b) | (b, Cond("const", (True,))):
                return b
            case "iff", (Cond("const", (False,)), b) | (b, Cond("const", (False,))):
                return substitute_flags(Cond("not", (b,)), values)
        return Cond(cond.op, args)
    return cond


def simplify(assumptions):
    """Substitute every flag a precondition forces, then drop the trivial ones.

    `true = flag` and `flag = true` fix a flag; so do their `false` forms. A
    substituted flag leaves an equivalent set of preconditions over words, and
    `True ↔ p` and `False ↔ p` become `p` and `¬p`. Repeated preconditions
    appear once.
    """
    conditions, values = list(assumptions), {}
    while True:
        forced = {}
        for cond in conditions:
            if cond.op == "iff":
                a, b = cond.args
                for fixed, flag in ((a, b), (b, a)):
                    if fixed.op == "const" and flag.op == "flag":
                        forced.setdefault(flag.args[0], fixed.args[0])
        if not forced:
            break
        values.update(forced)
        conditions = [substitute_flags(cond, values) for cond in conditions]
    conditions = [substitute_flags(cond, values) for cond in conditions]
    return list(dict.fromkeys(c for c in conditions if c != Cond.const(True)))


def word_variables(item, found):
    """Collect variables and environment reads in first-use order."""
    if isinstance(item, Cond):
        if item.op == "flag":
            found.setdefault(item.args[0], "Bool")
        for arg in item.args:
            if isinstance(arg, (Expr, Cond)):
                word_variables(arg, found)
        return found
    match item.op:
        case "var":
            found.setdefault(item.args[0], "Word")
        case "address":
            found.setdefault(ADDRESS, "BitVec 160")
        case "balance":
            found.setdefault(BALANCES, "BitVec 160 → Word")
        case "selfbalance":
            found.setdefault(BALANCES, "BitVec 160 → Word")
            found.setdefault(ADDRESS, "BitVec 160")
    for arg in item.args:
        if isinstance(arg, Expr):
            word_variables(arg, found)
    return found


def binders(variables):
    groups, text = [], []
    for name, sort in variables.items():
        if groups and groups[-1][1] == sort:
            groups[-1][0].append(lean_name(name))
        else:
            groups.append(([lean_name(name)], sort))
    for names, sort in groups:
        text.append(f"({' '.join(names)} : {sort})")
    return " ".join(text)


def hypotheses(assumptions):
    # Subscripts cannot clash with ASCII ISLE variable names.
    digits = str.maketrans("0123456789", "₀₁₂₃₄₅₆₇₈₉")
    return " ".join(
        f"(h{str(index).translate(digits)} : {prop(cond)})"
        for index, cond in enumerate(assumptions, 1)
    )


def indent(tactic):
    return "\n".join(
        f"  {line}" if line else "" for line in tactic.strip("\n").splitlines()
    )


def theorem(name, lhs, rhs, assumptions, tactic):
    """A theorem that holds exactly when `assumptions` imply `lhs = rhs`."""
    found = {}
    for item in (lhs, rhs, *assumptions):
        word_variables(item, found)
    header = " ".join(
        part for part in (binders(found), hypotheses(assumptions)) if part
    )
    return (
        f"theorem {' '.join(part for part in (name, header) if part)} :\n"
        f"    {term(lhs)} = {term(rhs)} := by\n{indent(tactic)}\n"
    )


def applicability(name, assumptions, timeout):
    """A theorem that the preconditions are contradictory; it must fail with a witness."""
    found = {}
    for cond in assumptions:
        word_variables(cond, found)
    if ADDRESS in found or BALANCES in found:
        raise Unsupported("preconditions over the environment are not searched")
    return (
        f"theorem {name} {binders(found)} {hypotheses(assumptions)} :\n    False := by\n"
        f"  evm_decide {timeout}\n"
    ), found


def witness(output, variables):
    """Read the assignment `bv_decide` reports for a satisfiable goal."""
    names = {lean_name(name): name for name in variables}
    values = {}
    for line in output.splitlines():
        # Quoted names may contain spaces, so match the value at the end.
        match = re.fullmatch(r"(.+?) = (?:(\d+)#\d+|(true|false))", line.strip())
        if match and match[1] in names:
            word, flag = match[2], match[3]
            values[names[match[1]]] = int(word) if word else flag == "true"
    return values


def canonical(expr, names=None):
    """Rename variables in first-use order, so equal shapes share one theorem."""
    names = {} if names is None else names
    if expr.op == "var":
        return Expr.var(names.setdefault(expr.args[0], f"s{len(names)}"))
    if expr.op == "const":
        return expr
    return Expr(expr.op, tuple(canonical(arg, names) for arg in expr.args))
