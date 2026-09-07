"""Total 256-bit EVM word semantics, independent of optimizer implementation.

Operand order is MIR / EVM pop order, including (amount, value) for shifts.
Memory, storage, calls, exceptions, gas and CFG motion are outside this model.
Unsupported operations raise: they are never unconstrained functions.
"""

from dataclasses import dataclass
import time

import z3

WIDTH = 256
MODULUS = 1 << WIDTH
MASK = MODULUS - 1
SIGN = 1 << (WIDTH - 1)


class Unsupported(ValueError):
    """An obligation cannot be modeled soundly by this checker."""


@dataclass(frozen=True)
class Expr:
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
        return 0 if self.op in ("var", "const") else 1 + sum(a.operators() for a in self.args)

    def text(self):
        if self.op == "var":
            return self.args[0]
        if self.op == "const":
            return hex(self.args[0])
        return f"({self.op} {' '.join(a.text() for a in self.args)})"


def word(value):
    return z3.BitVecVal(value, WIDTH)


def boolean(value):
    return z3.If(value, word(1), word(0))


def signed(value):
    return value - MODULUS if value & SIGN else value


class Model:
    def __init__(self):
        self.cache = {}

    def eval(self, expr):
        if expr not in self.cache:
            self.cache[expr] = self._eval(expr)
        return self.cache[expr]

    def _eval(self, expr):
        op, args = expr.op, expr.args
        if op == "var":
            return z3.BitVec(args[0], WIDTH)
        if op == "const":
            return word(args[0])
        return self.apply(op, tuple(self.eval(a) for a in args))

    @staticmethod
    def apply(op, args):
        match op, args:
            case "add", (a, b): return a + b
            case "sub", (a, b): return a - b
            case "mul", (a, b): return a * b
            # SMT division by zero differs from the EVM, for signed and unsigned words.
            case "div", (a, b): return z3.If(b == 0, word(0), z3.UDiv(a, b))
            case "mod", (a, b): return z3.If(b == 0, word(0), z3.URem(a, b))
            case "sdiv", (a, b): return z3.If(b == 0, word(0), a / b)
            # SRem takes the dividend's sign; SMT bvsmod / Python % do not.
            case "smod", (a, b): return z3.If(b == 0, word(0), z3.SRem(a, b))
            case "addmod" | "mulmod", (a, b, n):
                wide_a, wide_b, wide_n = (z3.ZeroExt(WIDTH, x) for x in (a, b, n))
                wide = wide_a + wide_b if op == "addmod" else wide_a * wide_b
                return z3.If(n == 0, word(0), z3.Extract(255, 0, z3.URem(wide, wide_n)))
            case "exp", (a, b):
                exponent = z3.simplify(b)
                if not z3.is_bv_value(exponent):
                    raise Unsupported("symbolic EXP is outside the current solver budget")
                result, power = word(1), a
                for bit in bin(exponent.as_long())[2:][::-1]:
                    if bit == "1":
                        result = result * power
                    power = power * power
                return result
            case "and", (a, b): return a & b
            case "or", (a, b): return a | b
            case "xor", (a, b): return a ^ b
            case "not", (a,): return ~a
            # SMT shifts use the full amount and saturate at the word width, as the EVM does.
            case "shl", (amount, value): return value << amount
            case "shr", (amount, value): return z3.LShR(value, amount)
            case "sar", (amount, value): return value >> amount
            case "lt", (a, b): return boolean(z3.ULT(a, b))
            case "gt", (a, b): return boolean(z3.UGT(a, b))
            case "slt", (a, b): return boolean(a < b)
            case "sgt", (a, b): return boolean(a > b)
            case "eq", (a, b): return boolean(a == b)
            case "iszero", (a,): return boolean(a == 0)
            case "select", (condition, a, b): return z3.If(condition != 0, a, b)
            case "byte", (index, value):
                return z3.If(z3.ULT(index, word(32)), z3.LShR(value, (31 - index) * 8) & 255, word(0))
            case "signextend", (index, value):
                shift = 248 - index * 8
                return z3.If(z3.ULT(index, word(31)), (value << shift) >> shift, value)
            case "clz", (value,):
                result = word(WIDTH)
                for bit in range(WIDTH):
                    result = z3.If(z3.Extract(bit, bit, value) != 0, word(255 - bit), result)
                return result
        raise Unsupported(f"unmodeled operation or arity: {op}/{len(args)}")


def concrete(expr, values):
    """Independent integer evaluator used to replay solver counterexamples."""
    if expr.op == "var":
        return values[expr.args[0]] & MASK
    if expr.op == "const":
        return expr.args[0]
    args = tuple(concrete(a, values) for a in expr.args)
    match expr.op, args:
        case "add", (a, b): result = a + b
        case "sub", (a, b): result = a - b
        case "mul", (a, b): result = a * b
        case "div", (a, b): result = a // b if b else 0
        case "mod", (a, b): result = a % b if b else 0
        case "sdiv", (a, b):
            a, b = signed(a), signed(b)
            result = (abs(a) // abs(b)) * (-1 if (a < 0) != (b < 0) else 1) if b else 0
        case "smod", (a, b):
            a, b = signed(a), signed(b)
            result = (abs(a) % abs(b)) * (-1 if a < 0 else 1) if b else 0
        case "addmod", (a, b, n): result = (a + b) % n if n else 0
        case "mulmod", (a, b, n): result = (a * b) % n if n else 0
        case "exp", (a, b): result = pow(a, b, MODULUS)
        case "and", (a, b): result = a & b
        case "or", (a, b): result = a | b
        case "xor", (a, b): result = a ^ b
        case "not", (a,): result = ~a
        case "shl", (s, a): result = a << s if s < WIDTH else 0
        case "shr", (s, a): result = a >> s if s < WIDTH else 0
        case "sar", (s, a): result = signed(a) >> min(s, WIDTH)
        case "lt", (a, b): result = int(a < b)
        case "gt", (a, b): result = int(a > b)
        case "slt", (a, b): result = int(signed(a) < signed(b))
        case "sgt", (a, b): result = int(signed(a) > signed(b))
        case "eq", (a, b): result = int(a == b)
        case "iszero", (a,): result = int(a == 0)
        case "select", (c, a, b): result = a if c else b
        case "byte", (i, a): result = (a >> (8 * (31 - i))) & 255 if i < 32 else 0
        case "signextend", (i, a):
            bits = min(8 * (i + 1), WIDTH)
            low = a & ((1 << bits) - 1)
            result = low - (1 << bits) if low & (1 << (bits - 1)) else low
        case "clz", (a,): result = WIDTH - a.bit_length()
        case _: raise Unsupported(f"unmodeled concrete operation: {expr.op}/{len(args)}")
    return result & MASK


def check(lhs, rhs, assumptions=(), timeout_ms=5000, model=None):
    """Only UNSAT proves equivalence; SAT must replay and UNKNOWN stays incomplete."""
    model = model or Model()
    left, right = model.eval(lhs), model.eval(rhs)
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=timeout_ms)
    solver.add(*assumptions)
    applicability = solver.check()
    if applicability != z3.sat:
        return {"status": "inapplicable" if applicability == z3.unsat else "unknown",
                "reason": "preconditions are unsatisfiable or could not be established"}, ""
    solver.add(left != right)
    query = solver.to_smt2()
    result = solver.check()
    if result == z3.unsat:
        return {"status": "proved"}, query
    if result == z3.unknown:
        return {"status": "unknown", "reason": solver.reason_unknown()}, query
    witness = solver.model()
    values = {name: witness.eval(z3.BitVec(name, WIDTH), model_completion=True).as_long()
              for name in sorted(lhs.variables() | rhs.variables())}
    actual = (concrete(lhs, values), concrete(rhs, values))
    expected = tuple(witness.eval(x, model_completion=True).as_long() for x in (left, right))
    if actual != expected or actual[0] == actual[1]:
        raise RuntimeError("SMT/concrete semantics disagree on a counterexample")
    return {"status": "counterexample", "inputs": {k: hex(v) for k, v in values.items()},
            "lhs_value": hex(actual[0]), "rhs_value": hex(actual[1]), "replayed": True}, query


def partition_shift(lhs, rhs, assumptions, timeout_ms, model):
    """Exhaust a single symbolic shift count: 0..255 and the saturating range.

    Called only after applicability was SAT and the unsplit equality timed out.
    Every saved subquery is a counterexample query. Coverage is checked too;
    sampled counts or a partially completed partition can never prove a rule.
    """
    shifts = set()

    def visit(expr):
        if expr.op in ("var", "const"):
            return
        if expr.op in ("shl", "shr", "sar") and expr.args[0].op == "var":
            shifts.add(expr.args[0])
        for child in expr.args:
            visit(child)

    visit(lhs)
    visit(rhs)
    if len(shifts) != 1:
        return {"status": "unknown", "reason": "no single symbolic shift partition"}, []
    shift = model.eval(next(iter(shifts)))
    conditions = [shift == word(i) for i in range(WIDTH)] + [z3.UGE(shift, word(WIDTH))]
    deadline = time.monotonic() + timeout_ms / 1000
    queries = []
    for index in range(-1, len(conditions)):
        remaining = int((deadline - time.monotonic()) * 1000)
        if remaining <= 0:
            return {"status": "unknown", "reason": "shift partition budget exhausted"}, queries
        solver = z3.SolverFor("QF_BV")
        solver.set(timeout=remaining)
        if index == -1:
            solver.add(z3.Not(z3.Or(conditions)))
        else:
            obligation = z3.And(*assumptions, conditions[index], model.eval(lhs) != model.eval(rhs))
            if index < WIDTH:
                obligation = z3.substitute(obligation, (shift, word(index)))
            solver.add(z3.simplify(obligation))
        query = solver.to_smt2()
        result = solver.check()
        queries.append((f"case-{index + 1}", query))
        if result == z3.sat:
            if index == -1:
                raise RuntimeError("shift partition does not cover all words")
            replay, _ = check(lhs, rhs, [*assumptions, conditions[index]], remaining, model)
            return replay, queries
        if result != z3.unsat:
            return {"status": "unknown", "reason": solver.reason_unknown()}, queries
    return {"status": "proved", "proof_method": "exhaustive-shift-partition", "cases": len(conditions)}, queries
