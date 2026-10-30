//! Constant program data lowering.

use crate::{
    backend::evm::op,
    mir::{
        FunctionBuilder, MirModuleTarget, Module, PtrLocation, ValueId, memory::EvmMemoryLayout,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_interface::Symbol;
use solar_sema::Gcx;
use std::borrow::Cow;

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

/// Copies constant data and clears its padding through `padded_size`.
pub(crate) fn copy_data_to_memory(
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
    let target = module.target_of(gcx, builder.func());
    let (data, separate_tail) = padded_copy_data(target, data, padded_size, name);
    if copy_splat_to_memory(target, builder, dest, &data, separate_tail) {
        return;
    }
    if !data_copy_is_profitable_for(target, &data, separate_tail) {
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

fn data_copy_is_profitable_for(target: Target, data: &[u8], separate_tail: bool) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    // push value; dup base; mstore
    let store = target.dup() + target.opcode(op::MSTORE);
    // push value; push offset; dup base; add; swap 1; mstore
    let offset_store = store + target.opcode(op::ADD) + target.opcode(op::SWAP1);
    let mut old = Cost::ZERO;
    for (index, chunk) in data.chunks(word_size).enumerate() {
        old += target.push(U256::from_be_bytes(padded_data_word(chunk)));
        old += if index == 0 {
            store
        } else {
            target.push(U256::from(index * word_size)) + offset_store
        };
    }

    // push size; push data; dup base; codecopy
    // Reserve PUSH3 for the unresolved data address so final relocation
    // cannot turn a selected rewrite into code growth.
    let mut new = target.push(U256::from(data.len()))
        + target.opcode(op::PUSH3)
        + target.dup()
        + target.opcode(op::CODECOPY)
        + target.copied_words(data.len())
        + Cost::new(0, u32::try_from(data.len()).unwrap_or(u32::MAX));
    if separate_tail {
        let tail_offset = data.len() / word_size * word_size;
        // push 0; push offset; dup base; add; mstore
        new += target.push(U256::from(tail_offset))
            + target.push(U256::ZERO)
            + target.dup()
            + target.opcode(op::ADD)
            + target.opcode(op::MSTORE);
    }

    target.improves(
        i64::from(target.runtime_gas(old)) - i64::from(target.runtime_gas(new)),
        i64::from(old.bytes) - i64::from(new.bytes),
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
    target: Target,
    builder: &mut FunctionBuilder<'_>,
    dest: DataDest,
    data: &[u8],
    clear_tail: bool,
) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    // Partial copies need a masked merge before Cancun; retain the ordinary data lowering.
    if !(target.optimization().is_size()
        && is_repeated_word(data)
        && (target.evm_version().has_mcopy() || data.len().is_multiple_of(word_size)))
    {
        return false;
    }
    let word = U256::from_be_bytes(padded_data_word(&data[..word_size]));
    if !target.evm_version().has_mcopy()
        && !target.legacy_splat_is_profitable(
            word,
            data.len(),
            data_copy_is_profitable_for(target, data, clear_tail),
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

/// Returns whether `copy_data_to_memory` stores the unnamed `data` as words, as it does short
/// data, instead of copying it from the data section.
// NOTE: This does not model the `-Osize` splat branch of `copy_data_to_memory`, which also leaves
// no data copy; callers lose an optimization there, not correctness.
pub(crate) fn stores_data_as_words(target: Target, data: &[u8]) -> bool {
    let padded_size = data.len().next_multiple_of(EvmMemoryLayout::WORD_SIZE as usize);
    let (data, separate_tail) = padded_copy_data(target, data, padded_size, None);
    padded_size <= EvmMemoryLayout::WORD_SIZE as usize
        || data.iter().any(|&byte| byte != 0)
            && !data_copy_is_profitable_for(target, &data, separate_tail)
}

/// Returns the bytes to copy and whether their tail word is cleared separately, rather than
/// copied zero-padded.
fn padded_copy_data<'a>(
    target: Target,
    data: &'a [u8],
    padded_size: usize,
    name: Option<Symbol>,
) -> (Cow<'a, [u8]>, bool) {
    let separate_tail = (name.is_some() || target.optimization().is_size())
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
    (data, separate_tail)
}
