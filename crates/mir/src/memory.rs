//! Physical EVM memory policy used by late lowering and the backend.
//!
//! High-level MIR refers to memory objects, slices, and allocations without
//! embedding these addresses. Inline assembly can still access the conventional
//! words directly, so alias analysis and backend lowering share this policy.

/// The selected physical EVM memory layout.
pub struct EvmMemoryLayout;

const _: () = assert!(
    EvmMemoryLayout::MAX_ALLOCATION_END
        == u64::MAX >> EvmMemoryLayout::MAX_ALLOCATION_END.leading_zeros(),
    "the allocation limit must be `2^k - 1`"
);

impl EvmMemoryLayout {
    /// EVM word size in bytes.
    pub const WORD_SIZE: u64 = 32;
    /// Scratch word for short-lived call outputs, such as precompile results.
    pub const SCRATCH_SLOT: u64 = 0;
    /// Scratch word used to publish ephemeral multi-return buffers.
    pub const MULTI_RETURN_BUFFER_PTR_SLOT: u64 = 0x20;
    /// Scratch word containing the free-memory pointer.
    pub const FMP_SLOT: u64 = 0x40;
    /// Permanently zero scratch word used by the Solidity ABI convention.
    pub const ZERO_SLOT: u64 = 0x60;
    /// First byte outside the reserved low-memory area.
    pub const HEAP_START: u64 = 0x80;
    /// Runtime word that holds the current internal-frame pointer.
    pub const INTERNAL_FRAME_PTR_SLOT: u64 = 0xa0;
    /// Header size for dynamically sized memory objects.
    pub const DYNAMIC_HEADER_SIZE: u64 = Self::WORD_SIZE;
    /// Header reserved by an internal-call frame before arguments and returns.
    pub const INTERNAL_FRAME_HEADER_SIZE: u64 = 2 * Self::WORD_SIZE;
    /// Maximum memory pointer accepted by Solidity-compatible allocation.
    pub const MAX_ALLOCATION_END: u64 = u64::MAX;
    /// The width of [`Self::MAX_ALLOCATION_END`], which is `2^k - 1`: an allocation end passes the
    /// limit exactly when it has a bit at or above this width.
    pub const MAX_ALLOCATION_END_BITS: u32 = Self::MAX_ALLOCATION_END.count_ones();
    /// Returns the word a word-aligned byte offset addresses.
    #[must_use]
    pub const fn word_slot(offset: u64) -> Option<u64> {
        if offset.is_multiple_of(Self::WORD_SIZE) { Some(offset / Self::WORD_SIZE) } else { None }
    }

    /// Returns whether an absolute address is in reserved low memory.
    #[must_use]
    pub const fn is_reserved(address: u64) -> bool {
        address < Self::ZERO_SLOT + Self::WORD_SIZE
    }

    /// Aligns a constant size to the next EVM word, if it fits.
    #[must_use]
    pub const fn align_word(size: u64) -> Option<u64> {
        match size.checked_add(Self::WORD_SIZE - 1) {
            Some(value) => Some(value & !(Self::WORD_SIZE - 1)),
            None => None,
        }
    }
}
