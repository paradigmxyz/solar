//! Deployment-time immutable staging layout.
//!
//! Immutable assignments are lowered to ordinary MIR memory stores, while the
//! deployment postlude reads the same words to patch runtime `PUSH<N>`
//! placeholders. This module keeps both sides on one layout.

use crate::{
    memory::EvmMemoryLayout,
    mir::{ImmutableId, Module},
};

/// Returns the first byte after every constructor-local slot.
fn constructor_local_memory_end(module: &Module) -> u64 {
    module.functions.iter().find(|func| func.attributes.is_constructor).map_or(
        EvmMemoryLayout::HEAP_START,
        |func| {
            EvmMemoryLayout::HEAP_START
                .checked_add(func.internal_frame_size)
                .expect("constructor local-memory size overflow")
        },
    )
}

/// Returns the first immutable staging word, above every constructor-owned
/// fixed memory word.
pub(crate) fn immutable_staging_base(module: &Module) -> u64 {
    let fixed_end = constructor_local_memory_end(module)
        .max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE);
    EvmMemoryLayout::align_word(fixed_end).expect("constructor immutable staging address overflow")
}

/// Returns the constructor-memory address assigned to an immutable.
pub(crate) fn immutable_staging_addr(base: u64, id: ImmutableId) -> u64 {
    let offset = u64::try_from(id.index())
        .ok()
        .and_then(|index| index.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging offset overflow");
    base.checked_add(offset).expect("constructor immutable staging address overflow")
}

/// Returns the first constructor-memory address after all immutable words.
pub(crate) fn immutable_staging_end(base: u64, count: usize) -> u64 {
    let size = u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .expect("constructor immutable staging size overflow");
    base.checked_add(size).expect("constructor immutable staging end overflow")
}
