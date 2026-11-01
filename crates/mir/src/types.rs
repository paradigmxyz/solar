//! MIR type system.

use super::StructId;
use std::{fmt, num::NonZeroU32};

pub use solar_ast::TypeSize;

/// How an immutable's typed value is encoded in a `PUSH<N>` immediate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImmutableEncoding {
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
    pub const fn type_size(self) -> TypeSize {
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

/// Address space a pointer or slice refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PtrLocation {
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
    /// Contract storage, addressed by 256-bit slot.
    Storage,
    /// Transient storage, addressed by 256-bit slot.
    Transient,
}

/// Physical base used by a semantic mutable-local frame slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameMode {
    /// The function uses the shared low-memory region for its locals.
    External,
    /// The function uses its internal-call frame for its locals.
    Internal,
    /// The ephemeral buffer used by multi-value calls.
    MultiReturn,
}

/// Logical value representation stored in a mutable-local frame slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameSlotKind {
    /// A single Solidity word.
    Word,
    /// A two-word pointer/length slice in the given address space.
    Slice(PtrLocation),
}

impl FrameSlotKind {
    /// Returns the MIR type produced by a frame load.
    #[must_use]
    pub const fn result_type(self) -> MirType {
        match self {
            Self::Word => MirType::I256,
            Self::Slice(location) => MirType::Slice(location),
        }
    }
}

/// The semantic shape of a memory object, named by the operations that access it.
///
/// The physical representation is selected by the memory model during late
/// lowering. Naming the shape at each access prevents Solidity-compatible headers
/// and field layouts from being inferred from an opaque pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryObjectKind {
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
pub enum MemoryObjectLayout {
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
    pub const WORD_ARRAY: Self = Self::DynamicArray { element_words: 1 };

    /// Creates a fixed array whose direct elements occupy one logical word.
    #[must_use]
    pub const fn word_fixed_array(len: u64) -> Self {
        Self::FixedArray { len, element_words: 1 }
    }

    /// Returns the nominal object kind represented by this layout.
    #[must_use]
    pub const fn kind(self) -> MemoryObjectKind {
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

impl fmt::Display for PtrLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Calldata => write!(f, "calldata"),
            Self::Returndata => write!(f, "returndata"),
            Self::Storage => write!(f, "storage"),
            Self::Transient => write!(f, "transient"),
        }
    }
}

/// A fixed aggregate of MIR values, with fields in declaration order.
///
/// Structs are SSA values, not references to Solidity memory objects. Nested
/// structs refer to earlier declarations, keeping their layouts finite.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructType {
    pub fields: Box<[MirType]>,
}

/// SSA value types. Integers have a bit width but no signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MirType {
    /// An integer bit pattern that fits within its nonzero bit width.
    ///
    /// Bits above this width are always zero in the EVM word representation,
    /// including for arguments, loads, call results, and phi inputs. Raw words
    /// must be truncated or validated before acquiring a narrower type.
    /// Signed operations interpret the top bit; signedness is not part of the type.
    Int(NonZeroU32),
    /// An opaque pointer into an address space, with no implied validity or heap provenance.
    ///
    /// Memory objects are memory pointers too, accessed through `ptradd` and `mload`/`mstore`.
    Ptr(PtrLocation),
    /// A pointer/length pair in the given address space.
    Slice(PtrLocation),
    /// A fixed aggregate declared in the module type table.
    Struct(StructId),
    /// Absence of a function result.
    Void,
}

impl MirType {
    /// A one-bit integer: zero or one.
    pub const I1: Self = Self::Int(NonZeroU32::new(1).unwrap());
    /// A 160-bit integer, used for addresses.
    pub const I160: Self = Self::Int(NonZeroU32::new(160).unwrap());
    /// A 256-bit integer.
    pub const I256: Self = Self::Int(NonZeroU32::new(256).unwrap());
    /// A memory pointer.
    pub const MEMPTR: Self = Self::Ptr(PtrLocation::Memory);
    /// The field index of a slice's pointer.
    pub const SLICE_PTR_FIELD: u32 = 0;
    /// The field index of a slice's length word.
    pub const SLICE_LEN_FIELD: u32 = 1;

    /// Whether an integer width has a supported native representation.
    pub const fn valid_integer_width(bits: u32) -> bool {
        bits == 1 || (bits != 0 && bits <= 256 && bits.is_multiple_of(8))
    }

    /// Returns the integer bit width, excluding pointers and aggregates.
    pub const fn integer_bits(self) -> Option<u32> {
        match self {
            Self::Int(bits) => Some(bits.get()),
            _ => None,
        }
    }

    /// Returns an unsigned layout for integer values without source signedness.
    pub const fn value_layout(self) -> ValueLayout {
        match self {
            Self::I1 => ValueLayout::Bool,
            Self::Int(bits) => ValueLayout::UInt(TypeSize::new_int_bits(bits.get() as u16)),
            Self::MEMPTR => ValueLayout::MemPtr,
            Self::Ptr(PtrLocation::Calldata) => ValueLayout::CalldataPtr,
            Self::Ptr(PtrLocation::Returndata | PtrLocation::Storage | PtrLocation::Transient) => {
                ValueLayout::uint256()
            }
            Self::Slice(location) => ValueLayout::Slice(location),
            Self::Struct(id) => ValueLayout::Struct(id),
            Self::Void => ValueLayout::Void,
        }
    }

    pub const fn is_pointer(self) -> bool {
        matches!(self, Self::Ptr(_))
    }

    pub const fn is_word(self) -> bool {
        match self {
            Self::Int(bits) => Self::valid_integer_width(bits.get()),
            Self::Ptr(_) => true,
            _ => false,
        }
    }

    pub const fn is_memory_reference(self) -> bool {
        matches!(self, Self::MEMPTR | Self::Slice(PtrLocation::Memory))
    }

    /// Returns the field types of a slice in `location`: its pointer and its length.
    pub const fn slice_fields(location: PtrLocation) -> [Self; 2] {
        [Self::Ptr(location), Self::I256]
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(bits) => write!(f, "i{bits}"),
            Self::Ptr(location) => write!(f, "ptr<{location}>"),
            Self::Slice(location) => write!(f, "slice<{location}>"),
            Self::Struct(id) => write!(f, "struct{}", id.index()),
            Self::Void => f.write_str("void"),
        }
    }
}

/// Source representation metadata for ABI, storage, and immutable layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueLayout {
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
    /// Pointer to a memory object of the given shape.
    MemoryObject(MemoryObjectKind),
    /// Storage pointer.
    StoragePtr,
    /// Calldata pointer.
    CalldataPtr,
    /// A `(pointer, length)` pair in the given address space.
    Slice(PtrLocation),
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
    pub const fn is_word(self) -> bool {
        !matches!(self, Self::Struct(_) | Self::Slice(_) | Self::Void)
    }

    /// Returns this value type's semantic size, or `None` for `void`.
    #[must_use]
    pub const fn type_size(self) -> Option<TypeSize> {
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
    pub const fn immutable_encoding(self) -> Option<ImmutableEncoding> {
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
    pub const fn is_full_abi_word(self) -> bool {
        matches!(
            self,
            Self::UInt(size) if size.bits() == 256
        ) || matches!(self, Self::Int(size) if size.bits() == 256)
            || matches!(self, Self::FixedBytes(size) if size.bytes() == 32)
    }

    /// Returns the uint256 type.
    #[must_use]
    pub const fn uint256() -> Self {
        Self::UInt(TypeSize::new_int_bits(256))
    }
}

impl fmt::Display for ValueLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UInt(size) => write!(f, "uint{}", size.bits()),
            Self::Int(size) => write!(f, "int{}", size.bits()),
            Self::Bool => write!(f, "bool"),
            Self::Address => write!(f, "address"),
            Self::FixedBytes(size) => write!(f, "bytes{}", size.bytes()),
            Self::MemPtr => MirType::MEMPTR.fmt(f),
            Self::MemoryObject(kind) => write!(f, "{kind}"),
            Self::StoragePtr => write!(f, "storageptr"),
            Self::CalldataPtr => MirType::Ptr(PtrLocation::Calldata).fmt(f),
            Self::Slice(location) => MirType::Slice(*location).fmt(f),
            Self::Function => write!(f, "function"),
            Self::Struct(id) => write!(f, "struct{}", id.index()),
            Self::Void => write!(f, "void"),
        }
    }
}

impl ValueLayout {
    /// Returns the SSA carrier with the source integer width.
    pub const fn mir_type(self) -> MirType {
        match self {
            Self::Bool => MirType::I1,
            Self::Address => MirType::I160,
            Self::UInt(size) | Self::Int(size) => {
                MirType::Int(NonZeroU32::new(size.bits() as u32).unwrap())
            }
            Self::MemPtr | Self::MemoryObject(_) => MirType::MEMPTR,
            Self::Slice(location) => MirType::Slice(location),
            Self::Struct(id) => MirType::Struct(id),
            Self::Void => MirType::Void,
            _ => MirType::I256,
        }
    }
}
