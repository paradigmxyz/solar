//! MIR values.

use super::{BlockId, InstId, MirType, ValueId};
use alloy_primitives::U256;
use std::fmt;

/// An SSA value in the MIR.
#[derive(Clone, Debug)]
pub enum Value {
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
    /// Phi node (SSA join point).
    Phi {
        /// Type of the phi node.
        ty: MirType,
        /// Incoming values from predecessor blocks.
        incoming: Vec<(BlockId, ValueId)>,
    },
    /// Undefined value (used for uninitialized variables).
    Undef(MirType),
}

impl Value {
    /// Returns the type of this value.
    #[must_use]
    pub fn ty(&self) -> MirType {
        match self {
            Self::Inst(_) => MirType::uint256(),
            Self::Arg { ty, .. } | Self::Phi { ty, .. } | Self::Undef(ty) => *ty,
            Self::Immediate(imm) => imm.ty(),
        }
    }

    /// Returns true if this is an immediate value.
    #[must_use]
    pub const fn is_immediate(&self) -> bool {
        matches!(self, Self::Immediate(_))
    }

    /// Returns true if this is a phi node.
    #[must_use]
    pub const fn is_phi(&self) -> bool {
        matches!(self, Self::Phi { .. })
    }

    /// Returns this value as an immediate, if it is one.
    #[must_use]
    pub const fn as_immediate(&self) -> Option<&Immediate> {
        match self {
            Self::Immediate(imm) => Some(imm),
            _ => None,
        }
    }
}

/// An immediate constant value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Immediate {
    /// Boolean constant.
    Bool(bool),
    /// Unsigned integer constant.
    UInt(U256, u16),
    /// Signed integer constant.
    Int(U256, u16),
    /// Address constant.
    Address([u8; 20]),
    /// Fixed bytes constant.
    FixedBytes(Vec<u8>, u8),
}

impl Immediate {
    /// Returns the type of this immediate.
    #[must_use]
    pub const fn ty(&self) -> MirType {
        match self {
            Self::Bool(_) => MirType::Bool,
            Self::UInt(_, bits) => MirType::UInt(*bits),
            Self::Int(_, bits) => MirType::Int(*bits),
            Self::Address(_) => MirType::Address,
            Self::FixedBytes(_, n) => MirType::FixedBytes(*n),
        }
    }

    /// Creates a new uint256 immediate from a U256 value.
    #[must_use]
    pub const fn uint256(value: U256) -> Self {
        Self::UInt(value, 256)
    }

    /// Creates a new boolean immediate.
    #[must_use]
    pub const fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    /// Creates a zero immediate of the given type.
    #[must_use]
    pub fn zero(ty: MirType) -> Self {
        match ty {
            MirType::Bool => Self::Bool(false),
            MirType::UInt(bits) => Self::UInt(U256::ZERO, bits),
            MirType::Int(bits) => Self::Int(U256::ZERO, bits),
            MirType::Address => Self::Address([0u8; 20]),
            MirType::FixedBytes(n) => Self::FixedBytes(vec![0u8; n as usize], n),
            _ => Self::UInt(U256::ZERO, 256),
        }
    }

    /// Returns the value as a U256, if applicable.
    #[must_use]
    pub fn as_u256(&self) -> Option<U256> {
        match self {
            Self::Bool(b) => Some(U256::from(*b as u64)),
            Self::UInt(v, _) | Self::Int(v, _) => Some(*v),
            Self::Address(addr) => Some(U256::from_be_slice(addr)),
            Self::FixedBytes(bytes, _) => {
                let mut padded = [0u8; 32];
                let len = bytes.len().min(32);
                padded[..len].copy_from_slice(&bytes[..len]);
                Some(U256::from_be_bytes(padded))
            }
        }
    }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::UInt(v, _) | Self::Int(v, _) => write!(f, "{v}"),
            Self::Address(addr) => write!(f, "0x{}", alloy_primitives::hex::encode(addr)),
            Self::FixedBytes(bytes, _) => write!(f, "0x{}", alloy_primitives::hex::encode(bytes)),
        }
    }
}
