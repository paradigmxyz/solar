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
    /// Boolean constant.
    Bool(bool),
    /// A 160-bit integer constant.
    I160(U256),
    /// An integer constant with a syntax-only width.
    Int(U256, NonZeroU32),
    /// A 256-bit word constant.
    Word(U256),
    /// A constant pointer; its type implies no validity or aliasing guarantee.
    Pointer(U256, MirType),
}

impl Immediate {
    /// Creates a scalar or pointer immediate carrying `value`.
    ///
    /// Boolean values must be zero or one. Types without a scalar or pointer
    /// payload use `i256`.
    #[must_use]
    pub(crate) fn for_type(ty: Option<MirType>, value: U256) -> Self {
        match ty {
            Some(MirType::I1) => {
                assert!(value <= U256::ONE, "boolean immediate must be zero or one");
                Self::Bool(!value.is_zero())
            }
            Some(MirType::I160) => {
                assert!(value.bit_len() <= 160, "i160 immediate must fit in 160 bits");
                Self::I160(value)
            }
            Some(MirType::I256) => Self::uint256(value),
            Some(MirType::Int(bits)) => {
                assert!(
                    value.bit_len() <= bits.get() as usize,
                    "integer immediate must fit its width"
                );
                Self::Int(value, bits)
            }
            Some(ty @ (MirType::MemPtr | MirType::MemoryObject(_))) => Self::Pointer(value, ty),
            _ => Self::uint256(value),
        }
    }

    /// Returns the type of this immediate.
    #[must_use]
    pub(crate) const fn ty(&self) -> MirType {
        match self {
            Self::Bool(_) => MirType::I1,
            Self::I160(_) => MirType::I160,
            Self::Word(_) => MirType::I256,
            Self::Int(_, bits) => MirType::Int(*bits),
            Self::Pointer(_, ty) => *ty,
        }
    }

    /// Creates a new uint256 immediate from a U256 value.
    #[must_use]
    pub(crate) const fn uint256(value: U256) -> Self {
        Self::Word(value)
    }

    /// Creates a new boolean immediate.
    #[must_use]
    pub(crate) const fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    /// Returns the value as a U256, if applicable.
    #[must_use]
    pub(crate) fn as_u256(&self) -> Option<U256> {
        match self {
            Self::Bool(b) => Some(U256::from(*b as u64)),
            Self::Word(v) | Self::I160(v) | Self::Int(v, _) | Self::Pointer(v, _) => Some(*v),
        }
    }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Word(v) | Self::I160(v) | Self::Int(v, _) | Self::Pointer(v, _) => {
                write!(f, "{v}")
            }
        }
    }
}

impl Ord for Immediate {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |value: &Self| match value {
            Self::Bool(_) => 0,
            Self::Word(_) => 1,
            Self::I160(_) => 2,
            Self::Pointer(_, _) => 3,
            Self::Int(_, _) => 4,
        };
        let pointer_rank = |ty| match ty {
            MirType::MemPtr => 2,
            MirType::MemoryObject(kind) => 3 + kind as u8,
            _ => unreachable!("pointer immediate has a pointer type"),
        };
        rank(self).cmp(&rank(other)).then_with(|| match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::Word(a), Self::Word(b)) | (Self::I160(a), Self::I160(b)) => a.cmp(b),
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
