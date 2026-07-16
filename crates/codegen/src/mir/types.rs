//! MIR type system.

use std::fmt;

/// Address space containing a dynamically-sized MIR slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SliceLocation {
    /// EVM memory.
    Memory,
    /// Call input data.
    Calldata,
}

/// The semantic shape carried by a one-word memory-object reference.
///
/// The physical representation is selected by the memory model during late
/// lowering. Keeping the shape in MIR prevents Solidity-compatible headers
/// and field layouts from being inferred from an untyped pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryObjectKind {
    /// Dynamically sized bytes or string data.
    Bytes,
    /// A dynamically sized array.
    DynamicArray,
    /// A statically sized array.
    FixedArray,
    /// A struct value.
    Struct,
}

impl fmt::Display for MemoryObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes => write!(f, "memorybytes"),
            Self::DynamicArray => write!(f, "memoryarray"),
            Self::FixedArray => write!(f, "memoryfixedarray"),
            Self::Struct => write!(f, "memorystruct"),
        }
    }
}

impl fmt::Display for SliceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Calldata => write!(f, "calldata"),
        }
    }
}

/// Types used in MIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MirType {
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
    /// Reference to a semantically shaped memory object.
    MemoryObject(MemoryObjectKind),
    /// Storage pointer.
    StoragePtr,
    /// Calldata pointer.
    CalldataPtr,
    /// A `(pointer, length)` pair in the given address space.
    Slice(SliceLocation),
    /// Function type.
    Function,
    /// Void/unit type (for functions that don't return).
    Void,
}

impl MirType {
    /// Returns whether this is a raw address or a semantic memory-object reference.
    #[must_use]
    pub const fn is_memory_reference(self) -> bool {
        matches!(self, Self::MemPtr | Self::MemoryObject(_))
    }

    /// Returns the uint256 type.
    #[must_use]
    pub(crate) const fn uint256() -> Self {
        Self::UInt(256)
    }

    /// Returns the int256 type.
    #[must_use]
    pub(crate) const fn int256() -> Self {
        Self::Int(256)
    }

    /// Returns the bytes32 type.
    #[must_use]
    pub(crate) const fn bytes32() -> Self {
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
            Self::MemoryObject(kind) => write!(f, "{kind}"),
            Self::StoragePtr => write!(f, "storageptr"),
            Self::CalldataPtr => write!(f, "calldataptr"),
            Self::Slice(location) => write!(f, "{location}slice"),
            Self::Function => write!(f, "function"),
            Self::Void => write!(f, "void"),
        }
    }
}
