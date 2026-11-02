//! Constant program data lowering.

use crate::{
    mir::{
        FunctionBuilder, MirModuleTarget, Module, PtrLocation, ValueId, memory::EvmMemoryLayout,
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

/// A destination address as a base plus a constant offset, so each word store addresses the
/// base directly rather than through another addition.
#[derive(Clone, Copy)]
pub(crate) struct DataDest {
    pub(crate) base: ValueId,
    pub(crate) offset: u64,
}

impl DataDest {
    fn at(self, builder: &mut FunctionBuilder<'_>, offset: u64) -> ValueId {
        builder.address_value(self.base, self.offset.saturating_add(offset))
    }

    fn store(self, builder: &mut FunctionBuilder<'_>, offset: u64, value: ValueId) {
        // mstore (base + offset + k), value
        let address = self.at(builder, offset);
        builder.store(address, value);
    }
}

/// Writes constant data and clears its padding through `padded_size`: with word stores, which
/// `pack-data` prices against a copy, or with a splat when the data repeats one word.
pub(crate) fn copy_data_to_memory(
    gcx: Gcx<'_>,
    module: &Module,
    builder: &mut FunctionBuilder<'_>,
    dest: DataDest,
    data: &[u8],
    padded_size: usize,
) {
    debug_assert!(padded_size >= data.len());
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    if padded_size == 0 {
        return;
    }
    // A zero word is a store that later loads can forward; longer zero fills are one operation.
    let one_word = !data.is_empty() && padded_size <= word_size;
    if !one_word && data.iter().all(|&byte| byte == 0) {
        let size = builder.imm(padded_size as u64);
        let start = dest.at(builder, 0);
        builder.memory_zero(start, size);
        return;
    }
    let target = module.target_of(gcx, builder.func());
    if copy_splat_to_memory(target, builder, dest, data, padded_size) {
        return;
    }
    // mstore dest + 32 k, word k
    for (index, chunk) in data.chunks(word_size).enumerate() {
        let mut word = [0; EvmMemoryLayout::WORD_SIZE as usize];
        word[..chunk.len()].copy_from_slice(chunk);
        let value = builder.imm(U256::from_be_bytes(word));
        dest.store(builder, (index * word_size) as u64, value);
    }
    let written = data.len().next_multiple_of(word_size);
    if padded_size > written {
        let size = builder.imm((padded_size - written) as u64);
        let start = dest.at(builder, written as u64);
        builder.memory_zero(start, size);
    }
}

/// Expands a repeated word with logarithmically many memory copies. No later pass rebuilds a
/// repeated word from stores, so it stays here.
fn copy_splat_to_memory(
    target: Target,
    builder: &mut FunctionBuilder<'_>,
    dest: DataDest,
    data: &[u8],
    padded_size: usize,
) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    // A partial last word is cleared before the copies, as long as only it is padding.
    let clear_tail = padded_size > data.len();
    // Partial copies need a masked merge before Cancun; leave those to word stores.
    if !(target.optimization().is_size()
        && is_repeated_word(data)
        && (!clear_tail || padded_size == data.len().next_multiple_of(word_size))
        && (target.evm_version().has_mcopy() || data.len().is_multiple_of(word_size)))
    {
        return false;
    }
    let word = U256::from_be_slice(&data[..word_size]);
    if !target.evm_version().has_mcopy() && !target.legacy_splat_is_profitable(word, data.len()) {
        return false;
    }

    if clear_tail {
        let tail_offset = data.len() / word_size * word_size;
        let zero = builder.imm(0);
        dest.store(builder, tail_offset as u64, zero);
    }
    let value = builder.imm(word);
    dest.store(builder, 0, value);
    let start = dest.at(builder, 0);
    let mut filled = word_size;
    if data.len() >= word_size * 2 {
        dest.store(builder, word_size as u64, value);
        filled += word_size;
    }
    while filled < data.len() {
        let chunk = filled.min(data.len() - filled);
        let target = dest.at(builder, filled as u64);
        let size = builder.imm(chunk as u64);
        builder.copy(target, start, size, PtrLocation::Memory);
        filled += chunk;
    }
    true
}

fn is_repeated_word(data: &[u8]) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    if data.len() < word_size {
        return false;
    }
    let (word, rest) = data.split_at(word_size);
    rest.chunks(word_size).all(|chunk| chunk == &word[..chunk.len()])
}
