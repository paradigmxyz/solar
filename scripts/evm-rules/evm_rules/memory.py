"""Pure address projections under the selected EVM memory-layout policy.

Object operands denote their leading pointer word, including slices. This is
an address-equality model, not a proof of typed slice erasure, memory effects,
allocation, or bounds checks. Valid layout/field constraints are structural
preconditions; the Rust policy and lowering remain fingerprinted trusted code.
"""

from dataclasses import dataclass

import z3

from .semantics import Expr, Unsupported


def expr(op, *args):
    return Expr(op, tuple(Expr.const(arg) if isinstance(arg, int) else arg for arg in args))


@dataclass(frozen=True)
class Layout:
    kind: Expr
    element_words: Expr
    fields: Expr


class MemoryAddresses:
    """Model all four object kinds and every u32/u64 layout parameter."""

    SHAPES = {
        "Op.MemoryObjectData": (("object", "Value"), ("kind", "MemoryObjectKind")),
        "Op.MemoryObjectFieldAddr": (("object", "Value"), ("layout", "MemoryObjectLayout"), ("field", "u64")),
        "Op.MemoryObjectElementAddr": (("object", "Value"), ("layout", "MemoryObjectLayout"), ("index", "Value")),
    }

    def __init__(self, context):
        self.context = context
        self.layouts = {}
        self.kinds = set()
        self.slices = {}

    def bounded(self, value, maximum):
        self.context.assumptions.append(z3.ULE(self.context.model.eval(value), maximum))
        return value

    def kind(self, value):
        if value in self.layouts:
            raise Unsupported("memory layout used as an object kind")
        self.kinds.add(value)
        # 0 = bytes, 1 = dynamic array, 2 = fixed array, 3 = struct.
        return self.bounded(value, 3)

    def layout(self, value):
        if value in self.kinds:
            raise Unsupported("memory object kind used as a layout")
        if value not in self.layouts:
            cx = self.context
            self.layouts[value] = Layout(self.kind(cx.fresh()),
                                         self.bounded(cx.fresh(), (1 << 32) - 1),
                                         self.bounded(cx.fresh(), (1 << 64) - 1))
        return self.layouts[value]

    def data_offset(self, kind):
        # Bytes and dynamic arrays have one header word; fixed arrays and
        # structs start directly at their payload. No equality is assumed.
        return expr("select", expr("lt", self.kind(kind), 2), 32, 0)

    def field_offset(self, layout, field):
        layout = self.layout(layout)
        cx = self.context
        self.bounded(field, (1 << 64) - 1)
        cx.assumptions.extend((cx.model.eval(layout.kind) == 3,
                               z3.ULT(cx.model.eval(field), cx.model.eval(layout.fields))))
        # Rust field_offset uses u64::saturating_mul, not wrapping arithmetic.
        total = expr("mul", field, 32)
        maximum = (1 << 64) - 1
        return expr("select", expr("lt", total, maximum), total, maximum)

    def operation(self, name, args):
        cx = self.context
        cx.contracts.add("memory addresses: leading pointer words under EvmMemoryLayout; valid layouts and typed lowering are trusted")
        match name, args:
            case "Op.MemoryObjectData", (object, kind):
                if object not in self.slices:
                    self.slices[object] = self.bounded(cx.fresh(), 1)
                # A slice already carries its payload pointer. Object references
                # include their layout's header before the payload.
                return expr("select", self.slices[object], object, expr("add", object, self.data_offset(kind)))
            case "Op.MemoryObjectFieldAddr", (object, layout, field):
                return expr("add", object, self.field_offset(layout, field))
            case "Op.MemoryObjectElementAddr", (object, layout, index):
                layout = self.layout(layout)
                cx.assumptions.append(cx.model.eval(layout.kind) != 3)
                words = expr("select", expr("eq", layout.kind, 0), 1, layout.element_words)
                # base + header + index * stride, with full-width wrapping
                # arithmetic. Layout length does not enter this calculation.
                return expr("add", expr("add", object, self.data_offset(layout.kind)),
                            expr("mul", index, expr("mul", words, 32)))
        raise Unsupported(f"unmodeled memory address operation or arity: {name}/{len(args)}")

    def constructor(self, name, args):
        match name, args:
            case "object_data_offset", (kind,): return self.data_offset(kind)
            case "field_offset", (layout, field): return self.field_offset(layout, field)
            case "layout_kind", (layout,): return self.layout(layout).kind
        raise Unsupported(f"unmodeled memory layout constructor or arity: {name}/{len(args)}")
