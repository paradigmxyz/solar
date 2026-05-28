//! MIR type system.

use std::fmt;

/// Types used in MIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MirType {
    /// Unsigned integer with a given bit width (8, 16, 32, ..., 256).
    UInt(u16),
    /// Signed integer with a given bit width.
    Int(u16),
    /// Boolean type.
    Bool,
    /// Address type (20 bytes).
    Address,
    /// Fixed-size byte array.
    FixedBytes(u8),
    /// Memory pointer.
    MemPtr,
    /// Storage pointer.
    StoragePtr,
    /// Calldata pointer.
    CalldataPtr,
    /// Function type.
    Function,
    /// Void/unit type (for functions that don't return).
    Void,
}

impl MirType {
    /// Returns the size in bytes for this type on the stack.
    #[must_use]
    pub const fn stack_size(&self) -> usize {
        match self {
            Self::Bool => 1,
            Self::UInt(bits) | Self::Int(bits) => (*bits as usize).div_ceil(8),
            Self::Address => 20,
            Self::FixedBytes(n) => *n as usize,
            Self::MemPtr | Self::StoragePtr | Self::CalldataPtr => 32,
            Self::Function => 24,
            Self::Void => 0,
        }
    }

    /// Returns true if this type fits in a single EVM word (32 bytes).
    #[must_use]
    pub const fn fits_in_word(&self) -> bool {
        self.stack_size() <= 32
    }

    /// Returns the uint256 type.
    #[must_use]
    pub const fn uint256() -> Self {
        Self::UInt(256)
    }

    /// Returns the int256 type.
    #[must_use]
    pub const fn int256() -> Self {
        Self::Int(256)
    }

    /// Returns the bytes32 type.
    #[must_use]
    pub const fn bytes32() -> Self {
        Self::FixedBytes(32)
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UInt(bits) => write!(f, "u{bits}"),
            Self::Int(bits) => write!(f, "i{bits}"),
            Self::Bool => write!(f, "bool"),
            Self::Address => write!(f, "address"),
            Self::FixedBytes(n) => write!(f, "bytes{n}"),
            Self::MemPtr => write!(f, "memptr"),
            Self::StoragePtr => write!(f, "storageptr"),
            Self::CalldataPtr => write!(f, "calldataptr"),
            Self::Function => write!(f, "function"),
            Self::Void => write!(f, "void"),
        }
    }
}
