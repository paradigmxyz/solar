//! MIR type system.

use super::StructId;
use std::{fmt, num::NonZeroU32};

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

/// Physical base used by a semantic mutable-local frame slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FrameMode {
    /// The function uses the shared low-memory region for its locals.
    External,
    /// The function uses its internal-call frame for its locals.
    Internal,
    /// The ephemeral buffer used by multi-value calls.
    MultiReturn,
}

/// Logical value representation stored in a mutable-local frame slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FrameSlotKind {
    /// A single Solidity word.
    Word,
    /// A two-word pointer/length slice in the given address space.
    Slice(SliceLocation),
}

impl FrameSlotKind {
    /// Returns the MIR type produced by a frame load.
    #[must_use]
    pub(crate) const fn result_type(self) -> MirType {
        match self {
            Self::Word => MirType::I256,
            Self::Slice(location) => MirType::Slice(location),
        }
    }
}

/// The semantic shape carried by a one-word memory-object reference.
///
/// The physical representation is selected by the memory model during late
/// lowering. Keeping the shape in MIR prevents Solidity-compatible headers
/// and field layouts from being inferred from an untyped pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemoryObjectKind {
    /// Dynamically sized bytes or string data, addressed in word chunks.
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

/// A fixed aggregate of MIR values, with fields in declaration order.
///
/// Structs are SSA values, not references to Solidity memory objects. Nested
/// structs refer to earlier declarations, keeping their layouts finite.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StructType {
    pub(crate) fields: Box<[MirType]>,
}

/// SSA value types. Integers have a bit width but no signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MirType {
    /// An integer bit pattern that fits within its nonzero bit width.
    ///
    /// Bits above this width are always zero in the EVM word representation,
    /// including for arguments, loads, call results, and phi inputs. Raw words
    /// must be truncated or validated before acquiring a narrower type.
    /// Signed operations interpret the top bit; signedness is not part of the type.
    Int(NonZeroU32),
    /// A raw memory pointer, with no implied validity or heap provenance.
    MemPtr,
    /// Reference to a semantically shaped memory object.
    MemoryObject(MemoryObjectKind),
    /// A pointer/length pair in the given address space.
    Slice(SliceLocation),
    /// A fixed aggregate declared in the module type table.
    Struct(StructId),
    /// Absence of a function result.
    Void,
}

impl MirType {
    /// A one-bit integer: zero or one.
    pub(crate) const I1: Self = Self::Int(NonZeroU32::new(1).unwrap());
    /// A 160-bit integer, used for addresses.
    pub(crate) const I160: Self = Self::Int(NonZeroU32::new(160).unwrap());
    /// A 256-bit integer.
    pub(crate) const I256: Self = Self::Int(NonZeroU32::new(256).unwrap());

    /// Returns the integer bit width, excluding pointers and aggregates.
    pub(crate) const fn integer_bits(self) -> Option<u32> {
        match self {
            Self::Int(bits) => Some(bits.get()),
            _ => None,
        }
    }

    /// Returns the full-width layout when no narrower source contract was supplied.
    pub(crate) const fn value_layout(self) -> ValueLayout {
        match self {
            Self::I1 => ValueLayout::Bool,
            Self::I160 => ValueLayout::Address,
            Self::Int(_) => ValueLayout::uint256(),
            Self::MemPtr => ValueLayout::MemPtr,
            Self::MemoryObject(kind) => ValueLayout::MemoryObject(kind),
            Self::Slice(location) => ValueLayout::Slice(location),
            Self::Struct(id) => ValueLayout::Struct(id),
            Self::Void => ValueLayout::Void,
        }
    }

    pub(crate) const fn is_pointer(self) -> bool {
        matches!(self, Self::MemPtr | Self::MemoryObject(_))
    }

    pub(crate) const fn is_word(self) -> bool {
        match self {
            Self::Int(bits) => bits.get() <= 256,
            Self::MemPtr | Self::MemoryObject(_) => true,
            _ => false,
        }
    }

    pub(crate) const fn is_memory_reference(self) -> bool {
        matches!(self, Self::MemPtr | Self::MemoryObject(_) | Self::Slice(SliceLocation::Memory))
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(bits) => write!(f, "i{bits}"),
            Self::MemPtr => f.write_str("memptr"),
            Self::MemoryObject(kind) => write!(f, "{kind}"),
            Self::Slice(location) => write!(f, "{location}slice"),
            Self::Struct(id) => write!(f, "struct{}", id.index()),
            Self::Void => f.write_str("void"),
        }
    }
}

/// Source representation metadata for ABI, storage, and immutable layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueLayout {
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
    /// A fixed aggregate declared in the module type table.
    Struct(StructId),
    /// Void/unit type (for functions that don't return).
    Void,
}

impl ValueLayout {
    /// Returns whether a value occupies one word rather than an SSA aggregate or no value.
    #[must_use]
    pub(crate) const fn is_word(self) -> bool {
        !matches!(self, Self::Struct(_) | Self::Slice(_) | Self::Void)
    }

    /// Returns this value type's semantic size, or `None` for `void`.
    #[must_use]
    pub(crate) const fn type_size(self) -> Option<TypeSize> {
        match self {
            Self::Bool => Some(TypeSize::new_int_bits(8)),
            Self::UInt(size) | Self::Int(size) | Self::FixedBytes(size) => {
                Some(TypeSize::new_int_bits(size.bits()))
            }
            Self::Address => Some(TypeSize::new_int_bits(160)),
            Self::MemPtr
            | Self::MemoryObject(_)
            | Self::StoragePtr
            | Self::CalldataPtr
            | Self::Slice(_) => Some(TypeSize::new_int_bits(256)),
            Self::Function => Some(TypeSize::new_int_bits(192)),
            Self::Struct(_) | Self::Void => None,
        }
    }

    /// Returns the compact immutable encoding for scalar types that can be immutable.
    #[must_use]
    pub(crate) const fn immutable_encoding(self) -> Option<ImmutableEncoding> {
        let Some(size) = self.type_size() else { return None };
        Some(match self {
            Self::Int(_) => ImmutableEncoding::Signed(size),
            Self::FixedBytes(_) => ImmutableEncoding::LeftAligned(size),
            // Internal function pointers do not yet have a stable narrow ABI in MIR.
            Self::Function => ImmutableEncoding::Unsigned(TypeSize::new_int_bits(256)),
            Self::Bool | Self::UInt(_) | Self::Address => ImmutableEncoding::Unsigned(size),
            Self::MemPtr
            | Self::MemoryObject(_)
            | Self::StoragePtr
            | Self::CalldataPtr
            | Self::Slice(_)
            | Self::Struct(_)
            | Self::Void => return None,
        })
    }

    /// Returns whether this scalar occupies a complete ABI word without padding.
    #[must_use]
    pub(crate) const fn is_full_abi_word(self) -> bool {
        matches!(
            self,
            Self::UInt(size) if size.bits() == 256
        ) || matches!(self, Self::Int(size) if size.bits() == 256)
            || matches!(self, Self::FixedBytes(size) if size.bytes() == 32)
    }

    /// Returns the uint256 type.
    #[must_use]
    pub(crate) const fn uint256() -> Self {
        Self::UInt(TypeSize::new_int_bits(256))
    }
}

impl fmt::Display for ValueLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UInt(size) => write!(f, "u{}", size.bits()),
            Self::Int(size) => write!(f, "i{}", size.bits()),
            Self::Bool => write!(f, "bool"),
            Self::Address => write!(f, "address"),
            Self::FixedBytes(size) => write!(f, "bytes{}", size.bytes()),
            Self::MemPtr => write!(f, "memptr"),
            Self::MemoryObject(kind) => write!(f, "{kind}"),
            Self::StoragePtr => write!(f, "storageptr"),
            Self::CalldataPtr => write!(f, "calldataptr"),
            Self::Slice(location) => write!(f, "{location}slice"),
            Self::Function => write!(f, "function"),
            Self::Struct(id) => write!(f, "struct{}", id.index()),
            Self::Void => write!(f, "void"),
        }
    }
}

impl ValueLayout {
    /// Returns the SSA carrier; source widths and signedness stay in this layout.
    pub(crate) const fn mir_type(self) -> MirType {
        match self {
            Self::Bool => MirType::I1,
            Self::Address => MirType::I160,
            Self::MemPtr => MirType::MemPtr,
            Self::MemoryObject(kind) => MirType::MemoryObject(kind),
            Self::Slice(location) => MirType::Slice(location),
            Self::Struct(id) => MirType::Struct(id),
            Self::Void => MirType::Void,
            _ => MirType::I256,
        }
    }
}
