"""Read the actual ISLE rules and lower a deliberately small, checked subset.

Value identity is overapproximated by word equality. Structural predicates may
restrict matching but never imply unequal words. Rust range predicates remain
explicit trusted contracts; their analysis implementations are not proved here.
"""

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path

from .expr import MASK, Cond, Expr, Unsupported
from .memory import MemoryAddresses

ROOT = Path(__file__).resolve().parents[3]
ISLE = ROOT / "crates/codegen/isle"


def forms(source):
    """S-expression reader with source lines, comments and balanced-delimiter checks."""
    tokens = []
    for line, text in enumerate(source.splitlines(), 1):
        for token in re.findall(r"[^\s()]+|[()]", text.split(";;", 1)[0]):
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
    for form, _ in forms((ISLE / "mir/extractors.isle").read_text()):
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
        bindings[name] = (
            opcode.removeprefix("$").lower(),
            len(operands),
            shape,
            all(operand == "_" for operand in operands),
        )
    return bindings


@dataclass(frozen=True)
class Rule:
    form: tuple
    line: int
    source: str

    @property
    def digest(self):
        return hashlib.sha256(repr(self.form).encode()).hexdigest()


CALL_OPERANDS = {
    "call": (
        "gas",
        "addr",
        "value",
        "args_offset",
        "args_size",
        "ret_offset",
        "ret_size",
    ),
    "callcode": (
        "gas",
        "addr",
        "value",
        "args_offset",
        "args_size",
        "ret_offset",
        "ret_size",
    ),
    "staticcall": ("gas", "addr", "args_offset", "args_size", "ret_offset", "ret_size"),
    "delegatecall": (
        "gas",
        "addr",
        "args_offset",
        "args_size",
        "ret_offset",
        "ret_size",
    ),
}


EFFECT_OPERANDS = {
    **CALL_OPERANDS,
    **{
        f"log{count}": ("offset", "size", *(f"topic{i + 1}" for i in range(count)))
        for count in range(5)
    },
}


class Context:
    def __init__(self, selection_source=None, integer_bits=None):
        self.values = {}
        self.assumptions = []
        self.contracts = set()
        self.extractors = extractor_definitions()
        self.bindings = opcode_bindings(
            selection_source
            if selection_source is not None
            else (ISLE / "mir-to-evm/select.isle").read_text()
        )
        self.fresh_id = 0
        self.memory = MemoryAddresses(self)
        self.integer_width = None if integer_bits is None else Expr.const(integer_bits)
        self.matching_integer = False

    def operation(self, name, args):
        if self.integer_width is not None:
            comparisons = {"Eq", "Ne", "Lt", "Gt", "SLt", "SGt"}
            arithmetic = {
                "Add",
                "Sub",
                "Mul",
                "Div",
                "Mod",
                "And",
                "Or",
                "Xor",
                "Shl",
                "Shr",
                "Sar",
                "SDiv",
                "SMod",
                "Not",
                "Exp",
                "Clz",
            }
            opcode = name.removeprefix("Op.")
            if not name.startswith("Op.") or opcode not in comparisons | arithmetic:
                raise Unsupported(f"unmodeled narrow integer operation: {name}")
            width = self.integer_width
            shift = Expr("sub", (Expr.const(256), width))
            mask = Expr("shr", (shift, Expr.const(MASK)))
            if self.matching_integer:
                self.assumptions.extend(Cond("ule", (arg, mask)) for arg in args)
            if opcode in {"SLt", "SGt", "Sar", "SDiv", "SMod"}:
                args = [
                    arg
                    if opcode == "Sar" and index == 0
                    else Expr("sar", (shift, Expr("shl", (shift, arg))))
                    for index, arg in enumerate(args)
                ]
            self.integer_width = None
            try:
                result = self.operation(name, args)
            finally:
                self.integer_width = width
            if opcode == "Clz":
                result = Expr("sub", (result, shift))
            return (
                result
                if opcode in comparisons | {"Div", "Mod", "And", "Or", "Xor", "Shr"}
                else Expr("and", (result, mask))
            )

        if name.startswith("Op.") and name[3:].lower() in EFFECT_OPERANDS:
            opcode = name[3:].lower()
            declarations = [
                form[3][1:]
                for form, _ in forms((ISLE / "mir/prelude.isle").read_text())
                if form[:3] == ("type", "Op", "extern")
            ]
            shapes = {
                "Op." + variant[0]: variant[1:]
                for variants in declarations
                for variant in variants
            }
            expected = tuple((field, "Value") for field in EFFECT_OPERANDS[opcode])
            if shapes.get(name) != expected or len(args) != len(expected):
                raise Unsupported(f"unmodeled or changed call operand schema: {name}")
            return Expr(opcode, tuple(args))
        if name in MemoryAddresses.SHAPES:
            # Semantic projections have no single-opcode selector. Check their
            # schema-generated field names/types before applying the model.
            declarations = [
                form[3][1:]
                for form, _ in forms((ISLE / "mir/prelude.isle").read_text())
                if form[:3] == ("type", "Op", "extern")
            ]
            shapes = {
                "Op." + variant[0]: variant[1:]
                for variants in declarations
                for variant in variants
            }
            if shapes.get(name) != MemoryAddresses.SHAPES[name]:
                raise Unsupported(f"unmodeled or changed memory address schema: {name}")
            return self.memory.operation(name, tuple(args))
        if name in ("Op.Ne", "Op.Zext", "Op.IntToPtr"):
            self.contracts.add(
                f"{name}: trusted MIR word inequality or bit-preserving scalar cast"
            )
            if name == "Op.Ne" and len(args) == 2:
                return Expr("ne", tuple(args))
            if name in ("Op.Zext", "Op.IntToPtr") and len(args) == 1:
                return args[0]
            raise Unsupported(f"invalid scalar operation arity: {name}")
        if name in ("Op.PtrToInt", "Op.Trunc"):
            if len(args) != 2:
                raise Unsupported("invalid narrowing cast arity")
            value, bits = args
            mask = Expr("sub", (Expr("shl", (bits, Expr.const(1))), Expr.const(1)))
            self.contracts.add(f"{name}: truncate or zero-extend a 256-bit value")
            return Expr("and", (value, mask))
        if name == "Op.Sext":
            if len(args) != 3:
                raise Unsupported("invalid sign extension arity")
            value, source, target = args
            width = Expr.const(256)
            capped = Expr("select", (Expr("lt", (source, width)), source, width))
            shift = Expr("sub", (width, capped))
            extended = Expr("sar", (shift, Expr("shl", (shift, value))))
            mask = Expr("sub", (Expr("shl", (target, Expr.const(1))), Expr.const(1)))
            self.contracts.add(
                "Sext: sign-extend the source bit width and retain target bits"
            )
            return Expr("and", (extended, mask))
        if name == "Op.Select":
            self.contracts.add(
                "Select: trusted MIR semantics select the true arm for any nonzero word"
            )
            return Expr("select", tuple(args))
        binding = self.bindings.get(name)
        shape = {
            0: "OpcodeLowering.Nullary",
            1: "OpcodeLowering.Unary",
            2: "OpcodeLowering.Binary",
            3: "OpcodeLowering.Nary",
        }
        if binding is None or binding != (
            name[3:].lower(),
            len(args),
            shape.get(len(args)),
            True,
        ):
            raise Unsupported(f"unmodeled or changed instruction selection: {name}")
        if name in ("Op.Address", "Op.Balance", "Op.SelfBalance"):
            self.contracts.add(
                "environment reads: one executing account and one balance snapshot; no state changes, gas or access-list effects"
            )
        return Expr(binding[0], tuple(args))

    def fresh(self):
        self.fresh_id += 1
        return Expr.var(f"@fresh:{self.fresh_id}")

    def pattern(self, node):
        if isinstance(node, str):
            # Names starting with `@` belong to the checker's own variables.
            if node.startswith("@"):
                raise Unsupported("reserved proof-variable prefix")
            if node == "_":
                return self.fresh()
            if node in ("true", "false"):
                return Cond.const(node == "true")
            if re.fullmatch(r"-?(?:[0-9]+|0x[0-9a-fA-F]+)", node):
                return Expr.const(int(node, 0))
            return self.values.setdefault(node, Expr.var(node))
        name, *args = node
        if name == "and":
            if len(args) < 2:
                raise Unsupported("and-pattern requires multiple patterns")
            value = self.pattern(args[0])
            for other in args[1:]:
                self.assumptions.append(Cond("eq", (value, self.pattern(other))))
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
                self.assumptions.append(Cond("ne", (value, Expr.const(0))))
            return value
        if not args and name in ("zero", "one", "all_ones"):
            if name == "all_ones":
                return self.constructor(("u256_max",))
            return Expr.const({"zero": 0, "one": 1}[name])
        if name == "current_address" and not args:
            self.contracts.add(
                "current_address: Rust extractor matches an ADDRESS producer in this execution context"
            )
            return self.operation("Op.Address", [])
        if name == "integer_bits" and len(args) == 1:
            bits = self.pattern(args[0])
            value = self.fresh()
            self.assumptions.extend(
                (
                    Cond("uge", (bits, Expr.const(1))),
                    Cond("ule", (bits, Expr.const(256))),
                )
            )
            # value & ((1 << bits) - 1) == value
            mask = Expr("sub", (Expr("shl", (bits, Expr.const(1))), Expr.const(1)))
            self.assumptions.append(Cond("eq", (Expr("and", (value, mask)), value)))
            self.contracts.add(
                "integer_bits: the Rust extractor returns a canonical integer width in 1..=256"
            )
            return value
        if name == "bool_value" and not args:
            value = self.fresh()
            self.assumptions.append(Cond("ule", (value, Expr.const(1))))
            self.contracts.add(
                "bool_value: the Rust extractor establishes a canonical 0/1 word"
            )
            return value
        raise Unsupported(f"unmodeled extractor: {name}")

    def constructor(self, node):
        if isinstance(node, str):
            if (
                node in self.values
                or node in ("true", "false")
                or re.fullmatch(r"[0-9]+", node)
            ):
                return self.pattern(node)
            raise Unsupported(f"unbound constructor variable: {node}")
        name, *args = node
        if name.startswith("Op."):
            return self.operation(name, [self.constructor(a) for a in args])
        values = [self.constructor(a) for a in args]
        if name in ("object_data_offset", "field_offset", "layout_kind"):
            return self.memory.constructor(name, tuple(values))
        if name == "integer_imm" and len(values) == 2:
            if self.integer_width is None or values[0] != self.integer_width:
                raise Unsupported(
                    "integer immediate must use the matched integer width"
                )
            shift = Expr("sub", (Expr.const(256), self.integer_width))
            return Expr("and", (values[1], Expr("shr", (shift, Expr.const(MASK)))))
        if name in ("imm", "u256", "resident", "make", "sequence") and len(values) == 1:
            if name == "resident":
                self.contracts.add(
                    "resident: available value has the matched expression's word semantics"
                )
            if name == "imm" and self.integer_width is not None:
                return Expr("and", (values[0], self.constructor(("u256_max",))))
            return values[0]
        if name == "imm_bool" and len(values) == 1:
            value = values[0]
            # Boolean results become word expressions so concrete replay stays independent.
            symbol = self.fresh()
            self.assumptions.append(Cond("bool_word", (symbol, value)))
            return symbol
        if (
            name == "zero_value"
            and not values
            or name == "imm_zero_like"
            and len(values) == 1
        ):
            return Expr.const(0)
        if name == "integer_width" and not values:
            return self.integer_width or Expr.const(256)
        if name == "integer_sign_bit" and not values:
            return Expr("sub", (self.integer_width or Expr.const(256), Expr.const(1)))
        if name == "is_i256" and not values:
            return (
                Cond.const(True)
                if self.integer_width is None
                else Cond("eq", (self.integer_width, Expr.const(256)))
            )
        if name == "u256_max" and not values:
            if self.integer_width is None:
                return Expr.const(MASK)
            return Expr(
                "shr",
                (Expr("sub", (Expr.const(256), self.integer_width)), Expr.const(MASK)),
            )
        if name == "u256_from_limbs" and len(values) == 4:
            if any(v.op != "const" or v.args[0] >= 1 << 64 for v in values):
                raise Unsupported("constant limbs must be literal u64 values")
            return Expr.const(sum(v.args[0] << (64 * i) for i, v in enumerate(values)))
        unary = {"u256_not": "not", "u256_neg": "sub"}
        if name in unary and len(values) == 1:
            result = Expr(
                unary[name],
                tuple(([Expr.const(0)] if name == "u256_neg" else []) + values),
            )
            return (
                result
                if self.integer_width is None
                else Expr("and", (result, self.constructor(("u256_max",))))
            )
        binary = {
            "u256_add": "add",
            "u256_mul": "mul",
            "u256_or": "or",
            "u256_xor": "xor",
            "u256_div": "div",
            "u256_sub": "sub",
            "u256_and": "and",
            "u256_shl": "shl",
            "u256_shr": "shr",
            "u256_byte": "byte",
        }
        if name in binary and len(values) == 2:
            result = Expr(binary[name], tuple(values))
            if self.integer_width is not None and name in (
                "u256_add",
                "u256_mul",
                "u256_sub",
                "u256_shl",
            ):
                result = Expr("and", (result, self.constructor(("u256_max",))))
            return result
        if (
            name in ("u256_is_zero", "u256_is_one", "u256_is_all_ones")
            and len(values) == 1
        ):
            literal = {
                "u256_is_zero": Expr.const(0),
                "u256_is_one": Expr.const(1),
                "u256_is_all_ones": self.constructor(("u256_max",)),
            }
            return Cond("eq", (values[0], literal[name]))
        predicates = {
            "u32_lt": "ult",
            "u32_le": "ule",
            "u256_gt": "ugt",
            "u256_ge": "uge",
            "u256_lt": "ult",
            "u256_same": "eq",
            "u256_le": "ule",
            "u256_eq": "eq",
        }
        if name in predicates and len(values) == 2:
            return Cond(predicates[name], tuple(values))
        if name == "u256_has_bits" and len(values) == 2:
            return Cond("eq", (Expr("and", tuple(values)), values[1]))
        if name == "u256_mul_fits" and len(values) == 2:
            # a * b <= MAX: b == 0, or a <= MAX / b
            a, b = values
            return Cond(
                "implies",
                (
                    Cond("ne", (b, Expr.const(0))),
                    Cond("ule", (a, Expr("div", (self.constructor(("u256_max",)), b)))),
                ),
            )
        if name == "u256_add_fits" and len(values) == 2:
            # a + b <= MAX: a <= MAX - b
            a, b = values
            return Cond("ule", (a, Expr("sub", (self.constructor(("u256_max",)), b))))
        if name == "u256_min" and len(values) == 2:
            return Expr("select", (Expr("lt", tuple(values)), *values))
        if name == "shift_sum" and len(values) == 2:
            limit = self.integer_width or Expr.const(256)
            capped = tuple(
                Expr("select", (Expr("lt", (v, limit)), v, limit)) for v in values
            )
            total = Expr("add", capped)
            return Expr("select", (Expr("lt", (total, limit)), total, limit))
        if name == "sign_byte" and len(values) == 1:
            if self.integer_width is not None:
                self.assumptions.append(
                    Cond("eq", (self.integer_width, Expr.const(256)))
                )
            self.assumptions.extend(
                [
                    Cond("ult", (values[0], Expr.const(256))),
                    Cond(
                        "eq", (Expr("and", (values[0], Expr.const(7))), Expr.const(0))
                    ),
                ]
            )
            return Expr(
                "sub", (Expr.const(31), Expr("shr", (Expr.const(3), values[0])))
            )
        if name == "power_of_two_shift" and len(values) == 1:
            shift = self.fresh()
            self.assumptions.extend(
                [
                    Cond("ugt", (shift, Expr.const(0))),
                    Cond("ult", (shift, Expr.const(256))),
                    Cond("eq", (values[0], Expr("shl", (shift, Expr.const(1))))),
                ]
            )
            return shift
        if name in (
            "differ",
            "has_bitwise_shifting",
            "has_self_balance",
            "in_current_block",
            "single_use",
            "push_not_larger",
            "optimize_for_size",
        ):
            # In particular, different ValueIds must NOT imply different word values.
            self.contracts.add(f"{name}: structural/fork condition is overapproximated")
            return Cond.flag(f"structural_{name}_{node!r}")
        guarantees = {
            "is_zero_or_one": lambda: Cond("ule", (values[0], Expr.const(1))),
            "has_known_sign_bit": lambda: Cond(
                "ne",
                (
                    Expr(
                        "and",
                        (
                            values[0],
                            Expr(
                                "shl",
                                (
                                    self.constructor(("integer_sign_bit",)),
                                    Expr.const(1),
                                ),
                            ),
                        ),
                    ),
                    Expr.const(0),
                ),
            ),
            "below_const": lambda: Cond("ult", tuple(values)),
            "at_most_const": lambda: Cond("ule", tuple(values)),
            "mask_covers": lambda: Cond("eq", (Expr("and", tuple(values)), values[1])),
            "masks_clean_address": lambda: Cond(
                "eq", (Expr("and", tuple(values)), values[1])
            ),
            "shifted_out": lambda: Cond(
                "eq", (Expr("shr", tuple(values)), Expr.const(0))
            ),
            "sign_clear": lambda: Cond(
                "eq", (Expr("signextend", tuple(values)), values[1])
            ),
            "same_value": lambda: Cond("eq", tuple(values)),
        }
        if name in guarantees:
            arity = 1 if name in ("is_zero_or_one", "has_known_sign_bit") else 2
            if len(values) != arity:
                raise Unsupported(f"extractor contract arity: {name}")
            flag = Cond.flag(f"contract_{name}_{node!r}")
            self.assumptions.append(Cond("implies", (flag, guarantees[name]())))
            self.contracts.add(
                f"{name}: trusted Rust extractor contract (true implies word property)"
            )
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
        if (
            root not in ("rewrite", "simplify", "stack_rewrite", "sequence_rewrite")
            or len(inputs) != 1
        ):
            raise Unsupported(f"unmodeled root: {root}")
        self.matching_integer = self.integer_width is not None
        lhs = self.pattern(inputs[0])
        self.matching_integer = False
        for clause in parts[1:-1]:
            if len(clause) != 3 or clause[0] != "if-let":
                raise Unsupported("only explicit if-let clauses are supported")
            _, pattern, expression = clause
            if isinstance(pattern, str) and pattern.startswith("@"):
                raise Unsupported("reserved proof-variable prefix")
            value = self.constructor(expression)
            if (
                isinstance(pattern, str)
                and pattern not in self.values
                and pattern not in ("true", "false", "_")
                and not re.fullmatch(r"[0-9]+", pattern)
            ):
                self.values[pattern] = value
            elif pattern != "_":
                other = self.pattern(pattern)
                self.assumptions.append(
                    Cond("eq" if isinstance(value, Expr) else "iff", (other, value))
                )
        rhs = self.constructor(parts[-1])

        def validate_snapshot_root(expr):
            if expr.op in ("var", "const"):
                return
            for child in expr.args:
                if child.op in EFFECT_OPERANDS:
                    raise Unsupported("calls are only modeled at instruction roots")
                if child.op in ("balance", "selfbalance"):
                    raise Unsupported(
                        "balance reads are only modeled at instruction roots; nested producers may observe another state"
                    )
                validate_snapshot_root(child)

        # Account state is shared by the two replacements of this instruction,
        # not by arbitrary earlier producers reached through operand extractors.
        validate_snapshot_root(lhs)
        validate_snapshot_root(rhs)
        if lhs.op in EFFECT_OPERANDS or rhs.op in EFFECT_OPERANDS:
            if lhs.op != rhs.op or len(lhs.args) != len(rhs.args):
                raise Unsupported("a call rewrite must preserve its opcode and effect")
            self.contracts.add(
                "classic CALL-family: preserve the instruction and every effective operand; "
                "address bits above 160 and memory starts for zero-length regions are ignored. "
                "LOG topics and effects are preserved. No effectful result is modeled as a pure value; "
                "the rewrite driver preserves effect order. Gas accounting is outside this model"
            )
            difference = Expr.const(0)
            for index, (before, after) in enumerate(
                zip(lhs.args, rhs.args, strict=True)
            ):
                if lhs.op in CALL_OPERANDS and index == 1:
                    mask = Expr.const((1 << 160) - 1)
                    before = Expr("and", (before, mask))
                    after = Expr("and", (after, mask))
                fields = EFFECT_OPERANDS[lhs.op]
                size_field = {
                    "args_offset": "args_size",
                    "ret_offset": "ret_size",
                    "offset": "size",
                }.get(fields[index])
                if size_field is not None:
                    size_index = fields.index(size_field)
                    before = Expr(
                        "select", (lhs.args[size_index], before, Expr.const(0))
                    )
                    after = Expr("select", (rhs.args[size_index], after, Expr.const(0)))
                difference = Expr("or", (difference, Expr("xor", (before, after))))
            return difference, Expr.const(0)
        return lhs, rhs


def rule_sources(path):
    """Read a rule file or every ISLE module in a rule-set directory."""
    paths = sorted(path.glob("*.isle")) if path.is_dir() else [path]
    if not paths:
        raise ValueError(f"no ISLE modules in {path}")
    return [(module, module.read_text()) for module in paths]
