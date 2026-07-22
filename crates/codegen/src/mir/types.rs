//! MIR type system.

use std::fmt;

/// Address space containing a dynamically-sized MIR slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SliceLocation {
    /// EVM memory.
    Memory,
    /// Call input data.
    Calldata,
    /// The most recent external call's return data. Unlike memory and
    /// calldata, this buffer is volatile: any subsequent call, create, or
    /// low-level `.call` overwrites it, so a returndata slice is only valid
    /// until the next such instruction and must be materialized into memory
    /// before it can be retained.
    Returndata,
}

/// The semantic shape carried by a one-word memory-object reference.
///
/// The physical representation is selected by the memory model during late
/// lowering. Keeping the shape in MIR prevents Solidity-compatible headers
/// and field layouts from being inferred from an untyped pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemoryObjectKind {
    /// Dynamically sized bytes or string data.
    Bytes,
    /// A dynamically sized array.
    DynamicArray,
    /// A statically sized array.
    FixedArray,
    /// A struct value.
    Struct,
}

/// Semantic layout of a one-word-referenced memory object.
///
/// Offsets are expressed in logical words rather than bytes. The selected
/// memory-layout policy owns the physical word width and dynamic-object
/// header, so high-level MIR does not bake EVM addresses into object access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemoryObjectLayout {
    /// Dynamically sized bytes or string data.
    Bytes,
    /// A dynamically sized array with the given element stride in words.
    DynamicArray {
        /// Number of logical words occupied by one inline element.
        element_words: u32,
    },
    /// A fixed-size array.
    FixedArray {
        /// Number of elements.
        len: u64,
        /// Number of logical words occupied by one inline element.
        element_words: u32,
    },
    /// A struct with one logical word per direct field.
    Struct {
        /// Number of direct fields.
        fields: u64,
    },
}

impl MemoryObjectLayout {
    /// Dynamic array whose direct elements occupy one logical word.
    pub(crate) const WORD_ARRAY: Self = Self::DynamicArray { element_words: 1 };

    /// Creates a fixed array whose direct elements occupy one logical word.
    #[must_use]
    pub(crate) const fn word_fixed_array(len: u64) -> Self {
        Self::FixedArray { len, element_words: 1 }
    }

    /// Creates a direct-field struct layout.
    #[must_use]
    pub(crate) const fn structure(fields: u64) -> Self {
        Self::Struct { fields }
    }

    /// Returns the nominal object kind represented by this layout.
    #[must_use]
    pub(crate) const fn kind(self) -> MemoryObjectKind {
        match self {
            Self::Bytes => MemoryObjectKind::Bytes,
            Self::DynamicArray { .. } => MemoryObjectKind::DynamicArray,
            Self::FixedArray { .. } => MemoryObjectKind::FixedArray,
            Self::Struct { .. } => MemoryObjectKind::Struct,
        }
    }
}

impl fmt::Display for MemoryObjectLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes => write!(f, "memorybytes"),
            Self::DynamicArray { element_words } => {
                write!(f, "memoryarray<{element_words}>")
            }
            Self::FixedArray { len, element_words } => {
                write!(f, "memoryfixedarray<{len}, {element_words}>")
            }
            Self::Struct { fields } => write!(f, "memorystruct<{fields}>"),
        }
    }
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
            Self::Returndata => write!(f, "returndata"),
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
    pub(crate) const fn is_memory_reference(self) -> bool {
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
