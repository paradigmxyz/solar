//! Physical EVM memory policy used by late lowering and the backend.
//!
//! High-level MIR refers to memory objects, slices, and allocations without
//! embedding these addresses. Inline assembly can still access the conventional
//! words directly, so alias analysis and backend lowering share this policy.

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
