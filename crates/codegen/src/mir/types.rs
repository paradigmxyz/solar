//! MIR type system.

use std::fmt;

pub(crate) use solar_ast::TypeSize;

/// How an immutable's typed value is encoded in a `PUSH<N>` immediate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ImmutableEncoding {
    /// A zero-extended, right-aligned value.
    Unsigned(TypeSize),
    /// A sign-extended, right-aligned value.
    Signed(TypeSize),
    /// A left-aligned fixed-bytes value.
    LeftAligned(TypeSize),
}

impl ImmutableEncoding {
    /// Returns the canonical encoded type size.
    #[must_use]
    pub(crate) const fn type_size(self) -> TypeSize {
        let size = match self {
            Self::Unsigned(size) | Self::Signed(size) | Self::LeftAligned(size) => size,
        };
        TypeSize::new_int_bits(size.bits())
    }

    /// Returns whether loading this encoding needs a post-`PUSH<N>` adjustment.
    #[must_use]
    pub(crate) const fn needs_runtime_normalization(self) -> bool {
        self.type_size().bytes() < 32 && !matches!(self, Self::Unsigned(_))
    }
}

/// Types used in MIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MirType {
    /// Unsigned integer with a given bit width (8, 16, 32, ..., 256).
    UInt(TypeSize),
    /// Signed integer with a given bit width.
    Int(TypeSize),
    /// Boolean type.
    Bool,
    /// Address type (20 bytes).
    Address,
    /// Fixed-size byte array.
    FixedBytes(TypeSize),
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
    /// Returns this value type's semantic size, or `None` for `void`.
    #[must_use]
    pub(crate) const fn type_size(self) -> Option<TypeSize> {
        match self {
            Self::Bool => Some(TypeSize::new_int_bits(8)),
            Self::UInt(size) | Self::Int(size) | Self::FixedBytes(size) => {
                Some(TypeSize::new_int_bits(size.bits()))
            }
            Self::Address => Some(TypeSize::new_int_bits(160)),
            Self::MemPtr | Self::StoragePtr | Self::CalldataPtr => {
                Some(TypeSize::new_int_bits(256))
            }
            Self::Function => Some(TypeSize::new_int_bits(192)),
            Self::Void => None,
        }
    }

    /// Returns the compact immutable encoding for this type.
    #[must_use]
    pub(crate) const fn immutable_encoding(self) -> ImmutableEncoding {
        let Some(size) = self.type_size() else { panic!("void has no immutable encoding") };
        match self {
            Self::Int(_) => ImmutableEncoding::Signed(size),
            Self::FixedBytes(_) => ImmutableEncoding::LeftAligned(size),
            // Internal function pointers do not yet have a stable narrow ABI in MIR.
            Self::Function => ImmutableEncoding::Unsigned(TypeSize::new_int_bits(256)),
            _ => ImmutableEncoding::Unsigned(size),
        }
    }

    /// Returns the uint256 type.
    #[must_use]
    pub(crate) const fn uint256() -> Self {
        Self::UInt(TypeSize::new_int_bits(256))
    }

    /// Returns the int256 type.
    #[must_use]
    pub(crate) const fn int256() -> Self {
        Self::Int(TypeSize::new_int_bits(256))
    }

    /// Returns the bytes32 type.
    #[must_use]
    pub(crate) const fn bytes32() -> Self {
        Self::FixedBytes(TypeSize::new_fb_bytes(32))
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UInt(size) => write!(f, "u{}", size.bits()),
            Self::Int(size) => write!(f, "i{}", size.bits()),
            Self::Bool => write!(f, "bool"),
            Self::Address => write!(f, "address"),
            Self::FixedBytes(size) => write!(f, "bytes{}", size.bytes()),
            Self::MemPtr => write!(f, "memptr"),
            Self::StoragePtr => write!(f, "storageptr"),
            Self::CalldataPtr => write!(f, "calldataptr"),
            Self::Function => write!(f, "function"),
            Self::Void => write!(f, "void"),
        }
    }
}
