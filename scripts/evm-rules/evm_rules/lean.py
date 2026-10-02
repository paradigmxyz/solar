"""Emit Lean proofs of the existing ISLE reader's bitvector obligations.

Z3 supplies syntax and applicability witnesses, never an equivalence verdict.
The reader, semantic model and this translation remain trusted. Lean checks
both the witness and the conditional equality; unsupported syntax fails closed.
"""

import z3

from .isle import Context, Rule, forms
from .semantics import Unsupported

BINARY = {
    z3.Z3_OP_BADD: "+",
    z3.Z3_OP_BSUB: "-",
    z3.Z3_OP_BMUL: "*",
    z3.Z3_OP_BAND: "&&&",
    z3.Z3_OP_BOR: "|||",
    z3.Z3_OP_BXOR: "^^^",
    z3.Z3_OP_EQ: "==",
    z3.Z3_OP_DISTINCT: "!=",
    z3.Z3_OP_ULT: "<",
    z3.Z3_OP_ULEQ: "≤",
    z3.Z3_OP_UGT: ">",
    z3.Z3_OP_UGEQ: "≥",
}

FUNCTIONS = {
    z3.Z3_OP_BSHL: "BitVec.shiftLeft",
    z3.Z3_OP_BLSHR: "BitVec.ushiftRight",
    z3.Z3_OP_BASHR: "BitVec.sshiftRight",
    z3.Z3_OP_BUDIV: "BitVec.udiv",
    z3.Z3_OP_BUREM: "BitVec.umod",
    z3.Z3_OP_BSDIV: "BitVec.sdiv",
    z3.Z3_OP_BSREM: "BitVec.srem",
}


def lean_type(sort):
    if sort.kind() == z3.Z3_BOOL_SORT:
        return "Bool"
    if sort.kind() == z3.Z3_BV_SORT:
        return f"BitVec {sort.size()}"
    raise Unsupported(f"unsupported Lean sort: {sort}")


class Lean:
    def __init__(self):
        self.variables = {}

    def term(self, value):
        if z3.is_true(value):
            return "true"
        if z3.is_false(value):
            return "false"
        if z3.is_bv_value(value):
            return f"({value.as_long()} : BitVec {value.size()})"
        if not z3.is_app(value):
            raise Unsupported("Lean requires quantifier-free expressions")
        kind = value.decl().kind()
        if kind == z3.Z3_OP_UNINTERPRETED and value.num_args() == 0:
            lean_type(value.sort())
            if value not in self.variables:
                self.variables[value] = f"v{len(self.variables)}"
            return self.variables[value]
        args = [self.term(child) for child in value.children()]
        if kind in BINARY and len(args) == 2:
            result = f"({args[0]} {BINARY[kind]} {args[1]})"
            return (
                f"(decide {result})"
                if kind in (z3.Z3_OP_ULT, z3.Z3_OP_ULEQ, z3.Z3_OP_UGT, z3.Z3_OP_UGEQ)
                else result
            )
        if kind in FUNCTIONS and len(args) == 2:
            if kind in (z3.Z3_OP_BSHL, z3.Z3_OP_BLSHR, z3.Z3_OP_BASHR):
                width = value.size()
                shifted = f"({FUNCTIONS[kind]} {args[0]} {args[1]}.toNat)"
                saturated = (
                    f"(BitVec.sshiftRight {args[0]} {width})"
                    if kind == z3.Z3_OP_BASHR
                    else f"(0 : BitVec {width})"
                )
                return f"(if {args[1]} < ({width} : BitVec {value.arg(1).size()}) then {shifted} else {saturated})"
            return f"({FUNCTIONS[kind]} {args[0]} {args[1]})"
        if kind == z3.Z3_OP_BNOT and len(args) == 1:
            return f"(~~~{args[0]})"
        if kind == z3.Z3_OP_NOT and len(args) == 1:
            return f"(!{args[0]})"
        if kind in (z3.Z3_OP_AND, z3.Z3_OP_OR):
            operator = " && " if kind == z3.Z3_OP_AND else " || "
            return (
                "(" + operator.join(args) + ")"
                if args
                else ("true" if kind == z3.Z3_OP_AND else "false")
            )
        if kind == z3.Z3_OP_IMPLIES and len(args) == 2:
            return f"(!{args[0]} || {args[1]})"
        if kind == z3.Z3_OP_ITE and len(args) == 3:
            return f"(if {args[0]} then {args[1]} else {args[2]})"
        if kind == z3.Z3_OP_EXTRACT and len(args) == 1:
            high, low = value.decl().params()
            return f"(BitVec.extractLsb' {low} {high - low + 1} {args[0]})"
        if kind in (z3.Z3_OP_ZERO_EXT, z3.Z3_OP_SIGN_EXT) and len(args) == 1:
            function = "setWidth" if kind == z3.Z3_OP_ZERO_EXT else "signExtend"
            return f"(BitVec.{function} {value.size()} {args[0]})"
        raise Unsupported(f"unsupported Lean operation: {value.decl()}")


def rule_source(rule):
    context = Context()
    lhs, rhs = context.obligation(rule)
    left, right = context.model.eval(lhs), context.model.eval(rhs)
    emitter = Lean()
    guard = z3.And(*context.assumptions)
    condition = emitter.term(guard)
    equality = f"{emitter.term(left)} = {emitter.term(right)}"
    binders = " ".join(
        f"({name} : {lean_type(value.sort())})"
        for value, name in emitter.variables.items()
    )
    solver = context.model.solver()
    solver.set(timeout=5000)
    solver.add(guard)
    if solver.check() != z3.sat:
        raise Unsupported(f"no applicability witness for {rule.source}:{rule.line}")
    witness = solver.model()
    concrete_guard = z3.substitute(
        guard,
        *[
            (value, witness.eval(value, model_completion=True))
            for value in emitter.variables
        ],
    )
    applicability = Lean().term(concrete_guard)
    return (
        f"-- ISLE line {rule.line}, SHA-256 {rule.digest}.\n"
        f"example : {applicability} = true := by decide\n"
        f"theorem rule_{rule.line} {binders} (_h : {condition} = true) :\n"
        f"    {equality} := by\n"
        "  bv_decide (timeout := 60)\n"
    )


def generate(path):
    rules = [
        Rule(form, line, str(path))
        for form, line in forms(path.read_text())
        if form[0] == "rule"
    ]
    if not rules:
        raise Unsupported("Lean input contains no rules")
    return (
        "import Std.Tactic.BVDecide\n\nset_option maxRecDepth 4096\nset_option exponentiation.threshold 512\nset_option maxHeartbeats 0\n\n"
        + "\n".join(rule_source(rule) for rule in rules),
        len(rules),
    )
