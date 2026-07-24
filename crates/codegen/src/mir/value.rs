//! MIR values.

use super::{InstId, MirType, TypeSize};
use alloy_primitives::U256;
use solar_interface::diagnostics::ErrorGuaranteed;
use std::fmt;

/// An SSA value in the MIR.
#[derive(Clone, Debug)]
pub(crate) enum Value {
    /// Result of an instruction.
    Inst(InstId),
    /// Function argument.
    Arg {
        /// Argument index.
        index: u32,
        /// Argument type.
        ty: MirType,
    },
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
    /// Returns the type of this value.
    #[must_use]
    pub(crate) fn ty(&self) -> MirType {
        match self {
            Self::Inst(_) | Self::Error(_) => MirType::uint256(),
            Self::Arg { ty, .. } | Self::Undef(ty) => *ty,
            Self::Immediate(imm) => imm.ty(),
        }
    }

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
    /// Unsigned integer constant.
    UInt(U256, TypeSize),
    /// Signed integer constant.
    Int(U256, TypeSize),
}

impl Immediate {
    /// Returns the type of this immediate.
    #[must_use]
    pub(crate) const fn ty(&self) -> MirType {
        match self {
            Self::Bool(_) => MirType::Bool,
            Self::UInt(_, bits) => MirType::UInt(*bits),
            Self::Int(_, bits) => MirType::Int(*bits),
        }
    }

    /// Creates a new uint256 immediate from a U256 value.
    #[must_use]
    pub(crate) const fn uint256(value: U256) -> Self {
        Self::UInt(value, TypeSize::new_int_bits(256))
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
            Self::UInt(v, _) | Self::Int(v, _) => Some(*v),
        }
    }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::UInt(v, _) | Self::Int(v, _) => write!(f, "{v}"),
        }
    }
}
