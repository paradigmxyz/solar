//! Solidity arithmetic semantics retained before scalar expansion.

use super::MirType;
use solar_ast::TypeSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ArithmeticKind {
    Unsigned(u16),
    Signed(u16),
}

impl ArithmeticKind {
    pub(crate) fn ty(self) -> MirType {
        match self {
            Self::Unsigned(bits) => MirType::UInt(TypeSize::new_int_bits(bits)),
            Self::Signed(bits) => MirType::Int(TypeSize::new_int_bits(bits)),
        }
    }
}

/// Wrapping division still rejects zero; signed minimum divided by -1 wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CheckedOp {
    Add,
    Sub,
    Mul,
    Div,
    WrappingDiv,
    Rem,
    Pow,
}

impl CheckedOp {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Add => "checked_add",
            Self::Sub => "checked_sub",
            Self::Mul => "checked_mul",
            Self::Div => "checked_div",
            Self::WrappingDiv => "wrapping_div",
            Self::Rem => "checked_rem",
            Self::Pow => "checked_pow",
        }
    }
}
