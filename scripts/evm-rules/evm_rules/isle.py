"""Read the actual ISLE rules and lower a deliberately small, checked subset.

Value identity is overapproximated by word equality. Structural predicates may
restrict matching but never imply unequal words. Rust range predicates remain
explicit trusted contracts; their analysis implementations are not proved here.
"""

from dataclasses import dataclass
import hashlib
from pathlib import Path
import re

import z3

from .semantics import Expr, MASK, Model, Unsupported, check, partition_bits, partition_shift, word
from .memory import MemoryAddresses

ROOT = Path(__file__).resolve().parents[3]
ISLE = ROOT / "crates/codegen/isle"


def forms(source):
    """S-expression reader with source lines, comments and balanced-delimiter checks."""
    tokens = []
    for line, text in enumerate(source.splitlines(), 1):
        for token in re.findall(r'[^\s()]+|[()]', text.split(";;", 1)[0]):
            tokens.append((token, line))
    stack, result = [], []
    for token, line in tokens:
        if token == "(":
            stack.append(([], line))
        elif token == ")":
            if not stack:
                raise ValueError(f"unexpected closing parenthesis at line {line}")
            items, start = stack.pop()
            if not items and not stack:
                raise ValueError(f"empty form at line {start}")
            if stack:
                stack[-1][0].append(tuple(items))
            else:
                result.append((tuple(items), start))
        elif stack:
            stack[-1][0].append(token)
        else:
            raise ValueError(f"unexpected token at line {line}: {token}")
    if stack:
        raise ValueError(f"unclosed form at line {stack[-1][1]}")
    return result


def substitute(tree, bindings):
    if isinstance(tree, str):
        return bindings.get(tree, tree)
    return tuple(substitute(x, bindings) for x in tree)


def extractor_definitions():
    definitions = {}
    for form, _ in forms((ISLE / "extractors.isle").read_text()):
        if form[0] == "extractor":
            _, (name, *params), body = form
            definitions[name] = (params, body)
    return definitions


def opcode_bindings(source):
    """Read direct opcode selection; reject changes to the modeled operand shape."""
    bindings = {}
    for form, _ in forms(source):
        if form[0] != "rule" or len(form) != 3 or form[1][0] != "select_opcode":
            continue
        _, (_, pattern), replacement = form
        name, *operands = pattern
        shape, opcode = replacement
        if name in bindings:
            raise Unsupported(f"multiple instruction-selection rules for {name}")
        bindings[name] = (opcode.removeprefix("$").lower(), len(operands), shape,
                          all(operand == "_" for operand in operands))
    return bindings


@dataclass(frozen=True)
class Rule:
    form: tuple
    line: int
    source: str

    @property
    def digest(self):
        return hashlib.sha256(repr(self.form).encode()).hexdigest()


class Context:
    def __init__(self, selection_source=None):
        self.values = {}
        self.assumptions = []
        self.contracts = set()
        self.model = Model()
        self.extractors = extractor_definitions()
        self.bindings = opcode_bindings(selection_source if selection_source is not None
                                       else (ISLE / "select.isle").read_text())
        self.fresh_id = 0
        self.memory = MemoryAddresses(self)

    def operation(self, name, args):
        if name in MemoryAddresses.SHAPES:
            # Semantic projections have no single-opcode selector. Check their
            # schema-generated field names/types before applying the model.
            declarations = [form[3][1:] for form, _ in forms((ISLE / "prelude.isle").read_text())
                            if form[:3] == ("type", "Op", "extern")]
            shapes = {"Op." + variant[0]: variant[1:] for variants in declarations for variant in variants}
            if shapes.get(name) != MemoryAddresses.SHAPES[name]:
                raise Unsupported(f"unmodeled or changed memory address schema: {name}")
            return self.memory.operation(name, tuple(args))
        if name == "Op.Select":
            self.contracts.add("Select: trusted MIR semantics select the true arm for any nonzero word")
            return Expr("select", tuple(args))
        binding = self.bindings.get(name)
        shape = {0: "OpcodeLowering.Nullary", 1: "OpcodeLowering.Unary", 2: "OpcodeLowering.Binary", 3: "OpcodeLowering.Nary"}
        if binding is None or binding != (name[3:].lower(), len(args), shape.get(len(args)), True):
            raise Unsupported(f"unmodeled or changed instruction selection: {name}")
        if name in ("Op.Address", "Op.Balance", "Op.SelfBalance"):
            self.contracts.add("environment reads: one executing account and one balance snapshot; no state changes, gas or access-list effects")
        return Expr(binding[0], tuple(args))

    def fresh(self):
        self.fresh_id += 1
        return Expr.var(f"@fresh:{self.fresh_id}")

    def pattern(self, node):
        if isinstance(node, str):
            if node.startswith(("@fresh:", "@environment:")):
                raise Unsupported("reserved proof-variable prefix")
            if node == "_":
                return self.fresh()
            if node in ("true", "false"):
                return z3.BoolVal(node == "true")
            if re.fullmatch(r"-?(?:[0-9]+|0x[0-9a-fA-F]+)", node):
                return Expr.const(int(node, 0))
            return self.values.setdefault(node, Expr.var(node))
        name, *args = node
        if name == "and":
            if len(args) < 2:
                raise Unsupported("and-pattern requires multiple patterns")
            value = self.pattern(args[0])
            for other in args[1:]:
                self.assumptions.append(self.model.eval(value) == self.model.eval(self.pattern(other)))
            return value
        if name.startswith("Op."):
            return self.operation(name, [self.pattern(a) for a in args])
        if name == "inst" and len(args) == 1:
            return self.pattern(args[0])
        if name in self.extractors:
            params, body = self.extractors[name]
            if len(params) != len(args):
                raise Unsupported(f"extractor arity: {name}")
            return self.pattern(substitute(body, dict(zip(params, args))))
        if name in ("iconst", "nonzero_const") and len(args) == 1:
            value = self.pattern(args[0])
            if name == "nonzero_const":
                self.assumptions.append(self.model.eval(value) != 0)
            return value
        if not args and name in ("zero", "one", "all_ones"):
            return Expr.const({"zero": 0, "one": 1, "all_ones": MASK}[name])
        if name == "current_address" and not args:
            self.contracts.add("current_address: Rust extractor matches an ADDRESS producer in this execution context")
            return self.operation("Op.Address", [])
        if name == "bool_value" and not args:
            value = self.fresh()
            self.assumptions.append(z3.ULE(self.model.eval(value), word(1)))
            self.contracts.add("bool_value: the Rust extractor establishes a canonical 0/1 word")
            return value
        raise Unsupported(f"unmodeled extractor: {name}")

    def constructor(self, node):
        if isinstance(node, str):
            if node in self.values or node in ("true", "false") or re.fullmatch(r"[0-9]+", node):
                return self.pattern(node)
            raise Unsupported(f"unbound constructor variable: {node}")
        name, *args = node
        if name.startswith("Op."):
            return self.operation(name, [self.constructor(a) for a in args])
        values = [self.constructor(a) for a in args]
        if name in ("object_data_offset", "field_offset", "layout_kind"):
            return self.memory.constructor(name, tuple(values))
        if name in ("imm", "u256", "resident", "make", "sequence") and len(values) == 1:
            if name == "resident":
                self.contracts.add("resident: available value has the matched expression's word semantics")
            return values[0]
        if name == "imm_bool" and len(values) == 1:
            value = values[0]
            # Boolean results become word expressions so concrete replay stays independent.
            symbol = self.fresh()
            self.assumptions.append(self.model.eval(symbol) == z3.If(value, word(1), word(0)))
            return symbol
        if name == "u256_max" and not values:
            return Expr.const(MASK)
        if name == "u256_from_limbs" and len(values) == 4:
            if any(v.op != "const" or v.args[0] >= 1 << 64 for v in values):
                raise Unsupported("constant limbs must be literal u64 values")
            return Expr.const(sum(v.args[0] << (64 * i) for i, v in enumerate(values)))
        unary = {"u256_not": "not", "u256_neg": "sub"}
        if name in unary and len(values) == 1:
            return Expr(unary[name], tuple(([Expr.const(0)] if name == "u256_neg" else []) + values))
        binary = {"u256_add": "add", "u256_sub": "sub", "u256_and": "and",
                  "u256_shl": "shl", "u256_shr": "shr", "u256_byte": "byte"}
        if name in binary and len(values) == 2:
            return Expr(binary[name], tuple(values))
        smt = [self.model.eval(v) for v in values]
        if name in ("u256_is_zero", "u256_is_one", "u256_is_all_ones") and len(smt) == 1:
            return smt[0] == {"u256_is_zero": 0, "u256_is_one": 1, "u256_is_all_ones": MASK}[name]
        predicates = {"u256_gt": z3.UGT, "u256_ge": z3.UGE, "u256_lt": z3.ULT, "u256_same": lambda a, b: a == b,
                      "u256_le": z3.ULE, "u256_eq": lambda a, b: a == b}
        if name in predicates and len(smt) == 2:
            return predicates[name](*smt)
        if name == "u256_has_bits" and len(smt) == 2:
            return smt[0] & smt[1] == smt[1]
        if name == "u256_min" and len(values) == 2:
            return Expr("select", (Expr("lt", tuple(values)), *values))
        if name == "shift_sum" and len(values) == 2:
            limit = Expr.const(256)
            capped = tuple(Expr("select", (Expr("lt", (v, limit)), v, limit)) for v in values)
            total = Expr("add", capped)
            return Expr("select", (Expr("lt", (total, limit)), total, limit))
        if name == "sign_byte" and len(values) == 1:
            self.assumptions.extend([z3.ULT(smt[0], word(256)), smt[0] & 7 == 0])
            return Expr("sub", (Expr.const(31), Expr("shr", (Expr.const(3), values[0]))))
        if name == "power_of_two_shift" and len(values) == 1:
            shift = self.fresh()
            self.assumptions.extend([z3.UGT(self.model.eval(shift), word(0)),
                                     z3.ULT(self.model.eval(shift), word(256)),
                                     smt[0] == word(1) << self.model.eval(shift)])
            return shift
        if name in ("is_const", "differ", "has_bitwise_shifting", "has_self_balance", "in_current_block", "single_use"):
            # In particular, different ValueIds must NOT imply different word values.
            self.contracts.add(f"{name}: structural/fork condition is overapproximated")
            return z3.Bool(f"structural_{name}_{repr(node)}")
        guarantees = {
            "is_zero_or_one": lambda: z3.ULE(smt[0], word(1)),
            "has_known_sign_bit": lambda: z3.Extract(255, 255, smt[0]) == 1,
            "below_const": lambda: z3.ULT(smt[0], smt[1]),
            "at_most_const": lambda: z3.ULE(smt[0], smt[1]),
            "mask_covers": lambda: smt[0] & smt[1] == smt[1],
            "masks_clean_address": lambda: smt[0] & smt[1] == smt[1],
            "shifted_out": lambda: z3.LShR(smt[1], smt[0]) == 0,
            "sign_clear": lambda: Model.apply("signextend", tuple(smt)) == smt[1],
            "same_value": lambda: smt[0] == smt[1],
        }
        if name in guarantees:
            arity = 1 if name in ("is_zero_or_one", "has_known_sign_bit") else 2
            if len(smt) != arity:
                raise Unsupported(f"extractor contract arity: {name}")
            flag = z3.Bool(f"contract_{name}_{repr(node)}")
            self.assumptions.append(z3.Implies(flag, guarantees[name]()))
            self.contracts.add(f"{name}: trusted Rust extractor contract (true implies word property)")
            return flag
        raise Unsupported(f"unmodeled constructor: {name}")

    def obligation(self, rule):
        parts = list(rule.form[1:])
        if parts and isinstance(parts[0], str):
            if not re.fullmatch(r"-?[0-9]+", parts[0]):
                raise Unsupported("named rules are not supported by this reader")
            parts.pop(0)
        if len(parts) < 2:
            raise Unsupported("rule lacks a left or right side")
        root, *inputs = parts[0]
        if root not in ("rewrite", "simplify", "stack_rewrite", "sequence_rewrite") or len(inputs) != 1:
            raise Unsupported(f"unmodeled root: {root}")
        lhs = self.pattern(inputs[0])
        for clause in parts[1:-1]:
            if len(clause) != 3 or clause[0] != "if-let":
                raise Unsupported("only explicit if-let clauses are supported")
            _, pattern, expression = clause
            value = self.constructor(expression)
            if isinstance(pattern, str) and pattern not in self.values and pattern not in ("true", "false", "_") and not re.fullmatch(r"[0-9]+", pattern):
                self.values[pattern] = value
            elif pattern != "_":
                other = self.pattern(pattern)
                self.assumptions.append(self.model.eval(other) == self.model.eval(value)
                                        if isinstance(value, Expr) else other == value)
        rhs = self.constructor(parts[-1])

        def validate_snapshot_root(expr):
            if expr.op in ("var", "const"):
                return
            for child in expr.args:
                if child.op in ("balance", "selfbalance"):
                    raise Unsupported("balance reads are only modeled at instruction roots; nested producers may observe another state")
                validate_snapshot_root(child)

        # Account state is shared by the two replacements of this instruction,
        # not by arbitrary earlier producers reached through operand extractors.
        validate_snapshot_root(lhs)
        validate_snapshot_root(rhs)
        return lhs, rhs


def verify_file(path, timeout_ms, artifacts=None, partition_shifts=False, fallback=None, bit_partition_timeout_ms=0,
                index_partition_timeout_ms=0):
    source = path.read_text()
    rules = [Rule(form, line, str(path)) for form, line in forms(source) if form[0] == "rule"]
    if not rules:
        raise ValueError(f"no rules in {path}")
    results = []
    for rule in rules:
        context = Context()
        query = ""
        partitions = []
        try:
            lhs, rhs = context.obligation(rule)
            result, query = check(lhs, rhs, context.assumptions, timeout_ms, context.model)
            if constants := result.get("constant_specializations"):
                context.model = Model({name: int(value, 16) for name, value in constants.items()})
            if query and (result["status"] == "unknown" or partition_shifts and result["status"] == "proved"):
                partitioned, partitions = partition_shift(lhs, rhs, context.assumptions,
                                                          index_partition_timeout_ms or timeout_ms, context.model)
                if partitions:
                    if constants:
                        partitioned["constant_specializations"] = constants
                    result = partitioned
            if query and result["status"] == "unknown" and fallback is not None:
                # Prove the complete original obligation. A successful fallback
                # replaces partial partitions, never promotes their proved prefix.
                attempt = fallback.solve(query)
                if attempt["status"] == "unsat":
                    result = {"status": "proved", "proof_method": "solver-fallback",
                              "fallback": attempt}
                    if constants:
                        result["constant_specializations"] = constants
                    partitions = []
                else:
                    result["fallback"] = attempt
                    if attempt["status"] == "sat":
                        result["reason"] = "cvc5 reported SAT; no independently replayed counterexample"
                    else:
                        reason = result.get("reason", "Z3 verification incomplete")
                        result["reason"] = f"{reason}; cvc5 fallback returned {attempt['status']}"
                    if partitions:
                        partitions.append(("word", query))
            if (query and result["status"] == "unknown" and bit_partition_timeout_ms > 0
                    and result.get("fallback", {}).get("status", "unknown") in ("unknown", "timeout")):
                # All output bits must agree under the complete original guards.
                # Do not hide a fallback solver's SAT result or process failure.
                previous_fallback = result.get("fallback")
                result, partitions = partition_bits(lhs, rhs, context.assumptions,
                                                    bit_partition_timeout_ms, context.model)
                if constants:
                    result["constant_specializations"] = constants
                if previous_fallback is not None:
                    result["fallback"] = previous_fallback
                if result["status"] != "proved":
                    partitions.append(("word", query))
        except Unsupported as error:
            result = {"status": "unsupported", "reason": str(error)}
        result.update(line=rule.line, rule_sha256=rule.digest, contracts=sorted(context.contracts))
        if query and artifacts is not None:
            artifacts.mkdir(parents=True, exist_ok=True)
            paths = []
            for suffix, text in partitions or [("word", query)]:
                query_path = artifacts / f"{path.stem}-{rule.line}-{rule.digest[:12]}-{suffix}.smt2"
                query_path.write_text(text)
                paths.append(str(query_path))
            result["smt2"] = paths
        results.append(result)
    return {"source": str(path), "source_sha256": hashlib.sha256(source.encode()).hexdigest(), "rules": results}
