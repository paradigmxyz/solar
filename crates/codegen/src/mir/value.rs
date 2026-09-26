//! MIR values.

use super::{ArgIdx, InstId, MirType};
use alloy_primitives::U256;
use solar_interface::diagnostics::ErrorGuaranteed;
use std::{cmp::Ordering, fmt, num::NonZeroU32};

/// An SSA value in the MIR.
#[derive(Clone, Debug)]
pub(crate) enum Value {
    /// Result of an instruction.
    Inst(InstId),
    /// Function argument.
    Arg(ArgIdx),
    /// Immediate constant.
    Immediate(Immediate),
    /// Undefined value (used for uninitialized variables).
    Undef(MirType),
    /// Error sentinel: lowering already reported a diagnostic for this value.
    ///
    /// Mirrors HIR's error types: instead of panicking or silently producing
    /// zero, error paths lower to this value, which carries the emitted
    /// diagnostic's guarantee. Compilation fails before bytecode is produced,
    /// so backends only need a defensive placeholder for it.
    Error(ErrorGuaranteed),
}

impl Value {
    /// Returns this value as an immediate, if it is one.
    #[must_use]
    pub(crate) const fn as_immediate(&self) -> Option<&Immediate> {
        match self {
            Self::Immediate(imm) => Some(imm),
            _ => None,
        }
    }
}

/// An immediate constant value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Immediate {
    /// A one-bit integer constant.
    I1(bool),
    /// An integer constant with an explicit bit width.
    Int(U256, NonZeroU32),
    /// A 256-bit integer constant.
    I256(U256),
    /// A constant pointer; its type implies no validity or aliasing guarantee.
    Pointer(U256, MirType),
}

impl Immediate {
    /// Creates a scalar or pointer immediate carrying `value`.
    ///
    /// Integer values must fit their declared width; booleans must be zero or one.
    /// Types without a scalar or pointer payload use `i256`.
    #[must_use]
    pub(crate) fn for_type(ty: Option<MirType>, value: U256) -> Self {
        match ty {
            Some(MirType::I1) => {
                assert!(value <= U256::ONE, "boolean immediate must be zero or one");
                Self::I1(!value.is_zero())
            }
            Some(MirType::I256) => Self::I256(value),
            Some(MirType::Int(bits)) => {
                assert!(
                    value.bit_len() <= bits.get() as usize,
                    "integer immediate must fit its width"
                );
                Self::Int(value, bits)
            }
            Some(ty @ (MirType::MemPtr | MirType::MemoryObject(_))) => Self::Pointer(value, ty),
            _ => Self::I256(value),
        }
    }

    /// Returns the type of this immediate.
    #[must_use]
    pub(crate) const fn ty(&self) -> MirType {
        match self {
            Self::I1(_) => MirType::I1,
            Self::I256(_) => MirType::I256,
            Self::Int(_, bits) => MirType::Int(*bits),
            Self::Pointer(_, ty) => *ty,
        }
    }

    /// Returns the value as a U256, if applicable.
    #[must_use]
    pub(crate) fn as_u256(&self) -> Option<U256> {
        match self {
            Self::I1(b) => Some(U256::from(*b as u64)),
            Self::I256(v) | Self::Int(v, _) | Self::Pointer(v, _) => Some(*v),
        }
    }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I1(b) => write!(f, "{b}"),
            Self::I256(v) | Self::Int(v, _) | Self::Pointer(v, _) => {
                write!(f, "{v}")
            }
        }
    }
}

impl Ord for Immediate {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |value: &Self| match value {
            Self::I1(_) => 0,
            Self::I256(_) => 1,
            Self::Pointer(_, _) => 2,
            Self::Int(_, _) => 3,
        };
        let pointer_rank = |ty| match ty {
            MirType::MemPtr => 2,
            MirType::MemoryObject(kind) => 3 + kind as u8,
            _ => unreachable!("pointer immediate has a pointer type"),
        };
        rank(self).cmp(&rank(other)).then_with(|| match (self, other) {
            (Self::I1(a), Self::I1(b)) => a.cmp(b),
            (Self::I256(a), Self::I256(b)) => a.cmp(b),
            (Self::Int(a, a_bits), Self::Int(b, b_bits)) => {
                a_bits.cmp(b_bits).then_with(|| a.cmp(b))
            }
            (Self::Pointer(a, a_ty), Self::Pointer(b, b_ty)) => {
                pointer_rank(*a_ty).cmp(&pointer_rank(*b_ty)).then_with(|| a.cmp(b))
            }
            _ => Ordering::Equal,
        })
    }
}

impl PartialOrd for Immediate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_immediates_preserve_width() {
        for bits in (8..256).step_by(8) {
            let width = NonZeroU32::new(bits).unwrap();
            let value = U256::MAX >> (256 - bits);
            let immediate = Immediate::for_type(Some(MirType::Int(width)), value);
            assert_eq!(immediate, Immediate::Int(value, width));
            assert_eq!(immediate.ty(), MirType::Int(width));
            assert_eq!(immediate.as_u256(), Some(value));
        }
    }
}
