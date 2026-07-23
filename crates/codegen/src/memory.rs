//! Physical EVM memory policy used by late lowering and the backend.
//!
//! High-level MIR refers to memory objects, slices, and allocations without
//! embedding these addresses. Inline assembly can still access the conventional
//! words directly, so alias analysis and backend lowering share this policy.

use crate::mir::{MemoryObjectKind, MemoryObjectLayout};

/// Policy interface between semantic memory-object MIR and a physical target
/// memory representation.
pub(crate) trait MemoryLayoutPolicy {
    /// Target word size in bytes.
    const WORD_SIZE: u64;

    /// Returns the byte offset of an object's logical length word.
    fn object_length_offset(kind: MemoryObjectKind) -> Option<u64>;

    /// Returns the byte offset of the first payload byte.
    fn object_data_offset(kind: MemoryObjectKind) -> u64;

    /// Returns the byte offset of a direct struct field.
    fn field_offset(layout: MemoryObjectLayout, field: u64) -> Option<u64>;

    /// Returns the byte stride of an array element.
    fn element_stride(layout: MemoryObjectLayout) -> Option<u64>;
}

/// The selected physical EVM memory layout.
pub(crate) struct EvmMemoryLayout;

impl EvmMemoryLayout {
    /// EVM word size in bytes.
    pub(crate) const WORD_SIZE: u64 = 32;
    /// Scratch word used to publish ephemeral multi-return buffers.
    pub(crate) const MULTI_RETURN_BUFFER_PTR_SLOT: u64 = 0x20;
    /// Scratch word containing the free-memory pointer.
    pub(crate) const FMP_SLOT: u64 = 0x40;
    /// Permanently zero scratch word used by the Solidity ABI convention.
    pub(crate) const ZERO_SLOT: u64 = 0x60;
    /// First byte outside the reserved low-memory area.
    pub(crate) const HEAP_START: u64 = 0x80;
    /// Runtime word that holds the current internal-frame pointer.
    pub(crate) const INTERNAL_FRAME_PTR_SLOT: u64 = 0xa0;
    /// Base of the absolute scheduler spill area.
    pub(crate) const SPILL_BASE: u64 = 0x1000;
    /// Constructor staging area for immutable words.
    pub(crate) const IMMUTABLE_SCRATCH_BASE: u64 = 0x2000;
    /// Header size for dynamically sized memory objects.
    pub(crate) const DYNAMIC_HEADER_SIZE: u64 = Self::WORD_SIZE;
    /// Header reserved by an internal-call frame before arguments and returns.
    pub(crate) const INTERNAL_FRAME_HEADER_SIZE: u64 = 2 * Self::WORD_SIZE;
    /// Maximum memory pointer accepted by Solidity-compatible allocation.
    pub(crate) const MAX_ALLOCATION_END: u64 = u64::MAX;
    /// Historical constructor heap floor, raised further when spills require it.
    pub(crate) const CONSTRUCTOR_HEAP_FLOOR: u64 = 0x4000;

    /// Returns whether an absolute address is in reserved low memory.
    #[must_use]
    pub(crate) const fn is_reserved(address: u64) -> bool {
        address < Self::ZERO_SLOT + Self::WORD_SIZE
    }

    /// Aligns a constant size to the next EVM word, if it fits.
    #[must_use]
    pub(crate) const fn align_word(size: u64) -> Option<u64> {
        match size.checked_add(Self::WORD_SIZE - 1) {
            Some(value) => Some(value & !(Self::WORD_SIZE - 1)),
            None => None,
        }
    }
}

impl MemoryLayoutPolicy for EvmMemoryLayout {
    const WORD_SIZE: u64 = Self::WORD_SIZE;

    fn object_length_offset(kind: MemoryObjectKind) -> Option<u64> {
        match kind {
            MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => Some(0),
            MemoryObjectKind::FixedArray | MemoryObjectKind::Struct => None,
        }
    }

    fn object_data_offset(kind: MemoryObjectKind) -> u64 {
        match kind {
            MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => Self::DYNAMIC_HEADER_SIZE,
            MemoryObjectKind::FixedArray | MemoryObjectKind::Struct => 0,
        }
    }

    fn field_offset(layout: MemoryObjectLayout, field: u64) -> Option<u64> {
        let MemoryObjectLayout::Struct { fields } = layout else { return None };
        (field < fields).then(|| field.saturating_mul(Self::WORD_SIZE))
    }

    fn element_stride(layout: MemoryObjectLayout) -> Option<u64> {
        let words = match layout {
            MemoryObjectLayout::DynamicArray { element_words }
            | MemoryObjectLayout::FixedArray { element_words, .. } => element_words,
            MemoryObjectLayout::Bytes | MemoryObjectLayout::Struct { .. } => return None,
        };
        u64::from(words).checked_mul(Self::WORD_SIZE)
    }
}
