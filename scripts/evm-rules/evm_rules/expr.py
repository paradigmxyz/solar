"""Solver-independent words, conditions and their integer semantics.

Rules are read into these terms once. The SMT backend (`semantics.py`) and
the Lean backend (`lean.py`) each give them a meaning: Z3 terms, or Lean
terms over the definitions in `lean/EvmRules/Word.lean`. The integer
evaluator here is a third, independent meaning that replays counterexamples
and cross-checks both.

Operand order is MIR / EVM pop order, including (amount, value) for shifts.
Memory, storage, calls, exceptions, gas and CFG motion are outside this model.
"""

from dataclasses import dataclass

WIDTH = 256
MODULUS = 1 << WIDTH
MASK = MODULUS - 1
SIGN = 1 << (WIDTH - 1)


class Unsupported(ValueError):
    """An obligation cannot be modeled soundly by this checker."""


@dataclass(frozen=True)
class Expr:
    """A 256-bit word."""

    op: str
    args: tuple

    @staticmethod
    def var(name):
        return Expr("var", (name,))

    @staticmethod
    def const(value):
        return Expr("const", (value & MASK,))

    def variables(self):
        if self.op == "var":
            return {self.args[0]}
        if self.op == "const":
            return set()
        return set().union(*(arg.variables() for arg in self.args))

    def operators(self):
        return (
            0
            if self.op in ("var", "const")
            else 1 + sum(a.operators() for a in self.args)
        )

    def text(self):
        if self.op == "var":
            return self.args[0]
        if self.op == "const":
            return hex(self.args[0])
        return f"({self.op} {' '.join(a.text() for a in self.args)})"


@dataclass(frozen=True)
class Cond:
    """A precondition: a Boolean over words.

    `const` is a literal, `flag` a free Boolean such as a structural
    predicate, and `msb` tests the sign bit. `bool_word` states that a word
    is 1 when the condition holds and 0 otherwise. The comparisons are
    unsigned.
    """

    op: str
    args: tuple

    @staticmethod
    def const(value):
        return Cond("const", (bool(value),))

    @staticmethod
    def flag(name):
        return Cond("flag", (name,))


def signed(value):
    return value - MODULUS if value & SIGN else value


def concrete(expr, values, environment=None):
    """Independent integer evaluator used to replay solver counterexamples."""
    if expr.op == "var":
        return values[expr.args[0]] & MASK
    if expr.op == "const":
        return expr.args[0]
    args = tuple(concrete(a, values, environment) for a in expr.args)
    if expr.op in ("address", "selfbalance", "balance"):
        if environment is None or len(args) != (1 if expr.op == "balance" else 0):
            raise Unsupported(
                "environment operation requires a snapshot and correct arity"
            )
        if expr.op == "address":
            return environment["address"]
        address = (args[0] if expr.op == "balance" else environment["address"]) & (
            (1 << 160) - 1
        )
        return environment["balances"].get(address, 0)
    match expr.op, args:
        case "add", (a, b):
            result = a + b
        case "sub", (a, b):
            result = a - b
        case "mul", (a, b):
            result = a * b
        case "div", (a, b):
            result = a // b if b else 0
        case "mod", (a, b):
            result = a % b if b else 0
        case "sdiv", (a, b):
            a, b = signed(a), signed(b)
            result = (abs(a) // abs(b)) * (-1 if (a < 0) != (b < 0) else 1) if b else 0
        case "smod", (a, b):
            a, b = signed(a), signed(b)
            result = (abs(a) % abs(b)) * (-1 if a < 0 else 1) if b else 0
        case "addmod", (a, b, n):
            result = (a + b) % n if n else 0
        case "mulmod", (a, b, n):
            result = (a * b) % n if n else 0
        case "exp", (a, b):
            result = pow(a, b, MODULUS)
        case "and", (a, b):
            result = a & b
        case "or", (a, b):
            result = a | b
        case "xor", (a, b):
            result = a ^ b
        case "not", (a,):
            result = ~a
        case "shl", (s, a):
            result = a << s if s < WIDTH else 0
        case "shr", (s, a):
            result = a >> s if s < WIDTH else 0
        case "sar", (s, a):
            result = signed(a) >> min(s, WIDTH)
        case "lt", (a, b):
            result = int(a < b)
        case "gt", (a, b):
            result = int(a > b)
        case "slt", (a, b):
            result = int(signed(a) < signed(b))
        case "sgt", (a, b):
            result = int(signed(a) > signed(b))
        case "eq", (a, b):
            result = int(a == b)
        case "ne", (a, b):
            result = int(a != b)
        case "iszero", (a,):
            result = int(a == 0)
        case "select", (c, a, b):
            result = a if c else b
        case "byte", (i, a):
            result = (a >> (8 * (31 - i))) & 255 if i < 32 else 0
        case "signextend", (i, a):
            bits = min(8 * (i + 1), WIDTH)
            low = a & ((1 << bits) - 1)
            result = low - (1 << bits) if low & (1 << (bits - 1)) else low
        case "clz", (a,):
            result = WIDTH - a.bit_length()
        case _:
            raise Unsupported(f"unmodeled concrete operation: {expr.op}/{len(args)}")
    return result & MASK


def holds(cond, values, flags=None, environment=None):
    """Evaluate a precondition on integer words and Boolean flags."""

    def word(expr):
        return concrete(expr, values, environment)

    match cond.op, cond.args:
        case "const", (value,):
            return value
        case "flag", (name,):
            return (flags or {})[name]
        case "eq", (a, b):
            return word(a) == word(b)
        case "ne", (a, b):
            return word(a) != word(b)
        case "ult", (a, b):
            return word(a) < word(b)
        case "ule", (a, b):
            return word(a) <= word(b)
        case "ugt", (a, b):
            return word(a) > word(b)
        case "uge", (a, b):
            return word(a) >= word(b)
        case "msb", (a,):
            return bool(word(a) & SIGN)
        case "not", (a,):
            return not holds(a, values, flags, environment)
        case "implies", (a, b):
            return not holds(a, values, flags, environment) or holds(
                b, values, flags, environment
            )
        case "iff", (a, b):
            return holds(a, values, flags, environment) == holds(
                b, values, flags, environment
            )
        case "bool_word", (symbol, a):
            return word(symbol) == int(holds(a, values, flags, environment))
    raise Unsupported(f"unmodeled condition: {cond.op}/{len(cond.args)}")
