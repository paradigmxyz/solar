//! Constant program data lowering.

use crate::{
    backend::evm::{
        ir::immediate_materialization_cost,
        op::{WORD_BYTES, push_len},
    },
    mir::{FunctionBuilder, Module, PtrLocation, ValueId, memory::EvmMemoryLayout},
    target::Target,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_interface::Symbol;
use solar_sema::Gcx;
use std::borrow::Cow;

/// Returns the encoded size and runtime gas of one program-data copy site.
pub(crate) fn data_copy_cost(evm_version: EvmVersion, size: usize) -> (usize, usize) {
    (push_len(evm_version, U256::from(size)) + 6, data_copy_gas(size))
}

/// Returns the runtime gas of one program-data copy site.
pub(crate) fn data_copy_gas(size: usize) -> usize {
    12 + 3 * size.div_ceil(WORD_BYTES)
}

/// Returns whether a program-data copy improves the selected objective.
pub(crate) fn data_copy_is_profitable(
    optimization: OptimizationMode,
    runtime_gas_saving: i128,
    byte_saving: i128,
) -> bool {
    if optimization.is_gas() { runtime_gas_saving > 0 && byte_saving >= 0 } else { byte_saving > 0 }
}

/// A destination address as a base plus a constant offset, so each word store addresses the
/// base directly rather than through another addition.
#[derive(Clone, Copy)]
pub(super) struct DataDest {
    pub(super) base: ValueId,
    pub(super) offset: u64,
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

/// Copies constant data and clears its padding through `padded_size`.
pub(super) fn copy_data_to_memory(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    dest: DataDest,
    data: &[u8],
    padded_size: usize,
    name: Option<Symbol>,
) {
    debug_assert!(padded_size >= data.len());
    if padded_size == 0 {
        return;
    }
    if !data.is_empty() && padded_size <= EvmMemoryLayout::WORD_SIZE as usize {
        store_data_words(builder, dest, data);
        return;
    }
    if data.iter().all(|&byte| byte == 0) {
        let size = builder.imm(padded_size as u64);
        let start = dest.at(builder, 0);
        builder.memory_zero(start, size);
        return;
    }
    let separate_tail = (name.is_some() || gcx.sess.opts.optimization.is_size())
        && padded_size > data.len()
        && padded_size == data.len().next_multiple_of(EvmMemoryLayout::WORD_SIZE as usize);
    let data = if separate_tail || padded_size == data.len() {
        Cow::Borrowed(data)
    } else {
        let mut padded = Vec::with_capacity(padded_size);
        padded.extend_from_slice(data);
        padded.resize(padded_size, 0);
        Cow::Owned(padded)
    };
    if copy_splat_to_memory(gcx, builder, dest, &data, separate_tail) {
        return;
    }
    if !data_copy_is_profitable_for(gcx, &data, separate_tail) {
        store_data_words(builder, dest, &data);
        return;
    }
    if separate_tail {
        let word_size = EvmMemoryLayout::WORD_SIZE as usize;
        let tail_offset = data.len() / word_size * word_size;
        let tail = dest.at(builder, tail_offset as u64);
        let zero = builder.imm(0);
        builder.store(tail, zero);
    }
    let size = builder.imm(data.len() as u64);
    let data = module.intern_data(data, name);
    let start = dest.at(builder, 0);
    builder.data_copy(data, start, size);
}

fn data_copy_is_profitable_for(gcx: Gcx<'_>, data: &[u8], separate_tail: bool) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    let mut old_size = 0;
    let mut old_gas = 0;
    for (index, chunk) in data.chunks(word_size).enumerate() {
        let value = U256::from_be_bytes(padded_data_word(chunk));
        let (value_size, value_gas) = immediate_materialization_cost(evm_version, value);
        if index == 0 {
            old_size += value_size + 2;
            old_gas += value_gas + 6;
        } else {
            let (offset_size, offset_gas) =
                immediate_materialization_cost(evm_version, U256::from(index * word_size));
            old_size += offset_size + value_size + 4;
            old_gas += offset_gas + value_gas + 12;
        }
    }

    // Reserve PUSH3 for the unresolved data address so final relocation
    // cannot turn a selected rewrite into code growth.
    let (copy_size, copy_gas) = data_copy_cost(evm_version, data.len());
    let mut new_size = data.len() + copy_size;
    let mut new_gas = copy_gas;
    if separate_tail {
        let tail_offset = data.len() / word_size * word_size;
        let (offset_size, offset_gas) =
            immediate_materialization_cost(evm_version, U256::from(tail_offset));
        let (zero_size, zero_gas) = immediate_materialization_cost(evm_version, U256::ZERO);
        new_size += offset_size + zero_size + 3;
        new_gas += offset_gas + zero_gas + 9;
    }

    data_copy_is_profitable(
        gcx.sess.opts.optimization,
        old_gas as i128 - new_gas as i128,
        old_size as i128 - new_size as i128,
    )
}

/// Stores data as words for short values and word-level constant pooling.
fn store_data_words(builder: &mut FunctionBuilder<'_>, dest: DataDest, data: &[u8]) {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    for (index, chunk) in data.chunks(word_size).enumerate() {
        let value = builder.imm(U256::from_be_bytes(padded_data_word(chunk)));
        dest.store(builder, (index * word_size) as u64, value);
    }
}

/// Expands a repeated word with logarithmically many memory copies.
fn copy_splat_to_memory(
    gcx: Gcx<'_>,
    builder: &mut FunctionBuilder<'_>,
    dest: DataDest,
    data: &[u8],
    clear_tail: bool,
) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    // Partial copies need a masked merge before Cancun; retain the ordinary data lowering.
    if !(gcx.sess.opts.optimization.is_size()
        && is_repeated_word(data)
        && (gcx.sess.opts.evm_version.has_mcopy() || data.len().is_multiple_of(word_size)))
    {
        return false;
    }
    let word = U256::from_be_bytes(padded_data_word(&data[..word_size]));
    if !gcx.sess.opts.evm_version.has_mcopy()
        && !Target::new(gcx).legacy_splat_is_profitable(
            word,
            data.len(),
            data_copy_is_profitable_for(gcx, data, clear_tail),
        )
    {
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

fn padded_data_word(data: &[u8]) -> [u8; EvmMemoryLayout::WORD_SIZE as usize] {
    let mut word = [0; EvmMemoryLayout::WORD_SIZE as usize];
    word[..data.len()].copy_from_slice(data);
    word
}

fn is_repeated_word(data: &[u8]) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    if data.len() < word_size {
        return false;
    }
    let (word, rest) = data.split_at(word_size);
    rest.chunks(word_size).all(|chunk| chunk == &word[..chunk.len()])
}
