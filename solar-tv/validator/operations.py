"""Value-operation vocabulary; checked against the compiler's reflected schema."""

UNARY = {"not", "clz", "iszero"}
BINARY = {
    "add",
    "sub",
    "mul",
    "div",
    "sdiv",
    "mod",
    "smod",
    "exp",
    "and",
    "or",
    "xor",
    "eq",
    "lt",
    "gt",
    "slt",
    "sgt",
    "shl",
    "shr",
    "sar",
    "byte",
    "signextend",
}
TERNARY = {"addmod", "mulmod"}
COMPARISONS = {"eq", "ne", "lt", "gt", "slt", "sgt"}
SHIFTS = {"shl", "shr", "sar"}
WORD_BINARY = {"byte", "signextend"}


def names(operations: set[str]) -> str:
    return "|".join(sorted(operations))
