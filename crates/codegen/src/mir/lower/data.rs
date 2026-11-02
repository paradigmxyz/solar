//! Constant program data lowering.

use crate::mir::{FunctionBuilder, ValueId, memory::EvmMemoryLayout};
use alloy_primitives::U256;

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

/// Writes constant data and clears its padding through `padded_size` with word stores. `pack-data`
/// prices each run of them against a copy.
pub(crate) fn copy_data_to_memory(
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
