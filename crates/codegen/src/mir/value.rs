//! MIR values.

use super::{ArgIdx, InstId, MirType, TypeSize};
use alloy_primitives::U256;
use solar_interface::diagnostics::ErrorGuaranteed;
use std::{cmp::Ordering, fmt};

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
    /// Unsigned integer constant.
    UInt(U256, TypeSize),
    /// Signed integer constant.
    Int(U256, TypeSize),
    /// A constant pointer; its type implies no validity or aliasing guarantee.
    Pointer(U256, MirType),
}

impl Immediate {
    /// Creates an immediate carrying `value` with the given integer type.
    ///
    /// Falls back to `uint256` when the type has no plain integer payload or
    /// cannot represent the value.
    #[must_use]
    pub(crate) fn for_type(ty: Option<MirType>, value: U256) -> Self {
        match ty {
            Some(MirType::Bool) if value <= U256::from(1) => Self::Bool(!value.is_zero()),
            Some(MirType::UInt(size)) if fits_unsigned(value, size) => Self::UInt(value, size),
            Some(MirType::Int(size)) if fits_signed(value, size) => Self::Int(value, size),
            Some(
                ty @ (MirType::MemPtr
                | MirType::CalldataPtr
                | MirType::StoragePtr
                | MirType::MemoryObject(_)),
            ) => Self::Pointer(value, ty),
            _ => Self::uint256(value),
        }
    }

    /// Returns the type of this immediate.
    #[must_use]
    pub(crate) const fn ty(&self) -> MirType {
        match self {
            Self::Bool(_) => MirType::Bool,
            Self::UInt(_, bits) => MirType::UInt(*bits),
            Self::Int(_, bits) => MirType::Int(*bits),
            Self::Pointer(_, ty) => *ty,
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
            Self::UInt(v, _) | Self::Int(v, _) | Self::Pointer(v, _) => Some(*v),
        }
    }
}

fn fits_unsigned(value: U256, size: TypeSize) -> bool {
    let bits = size.bits();
    bits >= 256 || value.bit_len() <= usize::from(bits)
}

fn fits_signed(value: U256, size: TypeSize) -> bool {
    let bits = size.bits();
    if bits >= 256 || bits == 0 {
        return bits >= 256;
    }
    let bits = usize::from(bits);
    if value.bit(bits - 1) { (!value).bit_len() < bits } else { value.bit_len() < bits }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::UInt(v, _) | Self::Int(v, _) | Self::Pointer(v, _) => write!(f, "{v}"),
        }
    }
}

impl Ord for Immediate {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |value: &Self| match value {
            Self::Bool(_) => 0,
            Self::UInt(_, _) => 1,
            Self::Int(_, _) => 2,
            Self::Pointer(_, _) => 3,
        };
        let pointer_rank = |ty| match ty {
            MirType::MemPtr => 0,
            MirType::StoragePtr => 1,
            MirType::CalldataPtr => 2,
            MirType::MemoryObject(kind) => 3 + kind as u8,
            _ => unreachable!("pointer immediate has a pointer type"),
        };
        rank(self).cmp(&rank(other)).then_with(|| match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::UInt(a, a_bits), Self::UInt(b, b_bits))
            | (Self::Int(a, a_bits), Self::Int(b, b_bits)) => {
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
