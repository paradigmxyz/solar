//! Physical EVM memory policy used by late lowering and the backend.
//!
//! High-level MIR refers to memory objects, slices, and allocations without
//! embedding these addresses. Inline assembly can still access the conventional
//! words directly, so alias analysis and backend lowering share this policy.
//!
//! Compiler-owned memory must remain disjoint from source-accessible memory
//! for the lifetime of the stored value. The free-memory pointer is writable
//! source state; its current value alone does not establish ownership.
//!
//! Memory-safe assembly promises to respect source allocations, scratch space,
//! and the heap boundary. Without that promise, we accept accesses proved to
//! stay within scratch space and reads of the public free-memory pointer.
//! Unknown ranges and writes to the allocator state supply no ownership proof.
//! Their call context must keep compiler state on the stack and
//! reject emission that still needs memory for frames, spills, return buffers,
//! or immutable staging. Constructor and runtime memory have separate lifetimes.
//! Textual MIR declares the same obligation with `unrestricted_memory`; raw
//! memory instructions alone do not infer source ownership after HIR lowering.
//!
//! Physical MIR accesses to compiler-owned memory carry `compiler_memory`.
//! This requirement survives operand substitution and cloning independently of
//! alias facts and debug information. The MIR-to-EVM boundary checks both these
//! accesses and memory operations introduced by stack scheduling. Removing an
//! unused frame reservation is allowed; emitting an unchecked frame access is not.

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
    /// Header size for dynamically sized memory objects.
    pub(crate) const DYNAMIC_HEADER_SIZE: u64 = Self::WORD_SIZE;
    /// Header reserved by an internal-call frame before arguments and returns.
    pub(crate) const INTERNAL_FRAME_HEADER_SIZE: u64 = 2 * Self::WORD_SIZE;
    /// Maximum memory pointer accepted by Solidity-compatible allocation.
    pub(crate) const MAX_ALLOCATION_END: u64 = u64::MAX;
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
            // Byte objects are addressed in word chunks for bulk materialization
            // and zeroing. Byte-indexed access uses the dedicated byte helpers.
            MemoryObjectLayout::Bytes => 1,
            MemoryObjectLayout::Struct { .. } => return None,
        };
        u64::from(words).checked_mul(Self::WORD_SIZE)
    }
}
