"""Pure address projections under the selected EVM memory-layout policy.

Object operands denote their leading pointer word. This is
an address-equality model, not a proof of typed slice erasure, memory effects,
allocation, or bounds checks. Valid layout/field constraints are structural
preconditions; the Rust policy and lowering remain fingerprinted trusted code.
"""

from dataclasses import dataclass
from typing import ClassVar

from .expr import Cond, Expr, Unsupported


def expr(op, *args):
    return Expr(
        op, tuple(Expr.const(arg) if isinstance(arg, int) else arg for arg in args)
    )


@dataclass(frozen=True)
class Layout:
    kind: Expr
    element_words: Expr
    fields: Expr


class MemoryAddresses:
    """Model all four object kinds and every u32/u64 layout parameter."""

    SHAPES: ClassVar[dict[str, tuple[tuple[str, str], ...]]] = {
        "Op.MemoryObjectFieldAddr": (
            ("object", "Value"),
            ("layout", "MemoryObjectLayout"),
            ("field", "u64"),
        ),
        "Op.MemoryObjectElementAddr": (
            ("object", "Value"),
            ("layout", "MemoryObjectLayout"),
            ("index", "Value"),
        ),
    }

    def __init__(self, context):
        self.context = context
        self.layouts = {}

    def bounded(self, value, maximum):
        self.context.assumptions.append(Cond("ule", (value, Expr.const(maximum))))
        return value

    def layout(self, value):
        if value not in self.layouts:
            cx = self.context
            self.layouts[value] = Layout(
                # 0 = bytes, 1 = dynamic array, 2 = fixed array, 3 = struct.
                self.bounded(cx.fresh(), 3),
                self.bounded(cx.fresh(), (1 << 32) - 1),
                self.bounded(cx.fresh(), (1 << 64) - 1),
            )
        return self.layouts[value]

    def field_offset(self, layout, field):
        layout = self.layout(layout)
        cx = self.context
        self.bounded(field, (1 << 64) - 1)
        cx.assumptions.extend(
            (
                Cond("eq", (layout.kind, Expr.const(3))),
                Cond("ult", (field, layout.fields)),
            )
        )
        # Rust field_offset uses u64::saturating_mul, not wrapping arithmetic.
        total = expr("mul", field, 32)
        maximum = (1 << 64) - 1
        return expr("select", expr("lt", total, maximum), total, maximum)

    def operation(self, name, args):
        cx = self.context
        cx.contracts.add(
            "memory addresses: leading pointer words under EvmMemoryLayout; valid layouts and typed lowering are trusted"
        )
        match name, args:
            case "Op.MemoryObjectFieldAddr", (object, layout, field):
                return expr("add", object, self.field_offset(layout, field))
            case "Op.MemoryObjectElementAddr", (object, layout, index):
                layout = self.layout(layout)
                # Element addresses exist only for fixed arrays, which have no
                # header; dynamic objects are addressed through slice views.
                cx.assumptions.append(Cond("eq", (layout.kind, Expr.const(2))))
                # base + index * stride, with full-width wrapping arithmetic.
                # Layout length does not enter this calculation.
                return expr(
                    "add",
                    object,
                    expr("mul", index, expr("mul", layout.element_words, 32)),
                )
        raise Unsupported(
            f"unmodeled memory address operation or arity: {name}/{len(args)}"
        )

    def constructor(self, name, args):
        match name, args:
            case "field_offset", (layout, field):
                return self.field_offset(layout, field)
        raise Unsupported(
            f"unmodeled memory layout constructor or arity: {name}/{len(args)}"
        )
