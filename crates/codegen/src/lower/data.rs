//! Constant program data lowering.

use crate::{
    backend::evm::{data_copy_cost, data_copy_is_profitable, ir::immediate_materialization_cost},
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, Module, ValueId},
};
use alloy_primitives::{Bytes, U256};
use solar_interface::Symbol;
use solar_sema::{Gcx, hir::ContractId};
use std::borrow::Cow;

#[derive(Clone, Debug, Default)]
pub struct ContractBytecodes {
    /// Deployment bytecode, including the initcode prefix.
    deployment: Option<Bytes>,
    /// Deployed runtime bytecode.
    runtime: Option<Bytes>,
}

impl ContractBytecodes {
    /// Creates bytecode metadata from a generated artifact.
    pub fn new(deployment: Bytes, runtime: Bytes) -> Self {
        Self {
            deployment: (!deployment.is_empty()).then_some(deployment),
            runtime: (!runtime.is_empty()).then_some(runtime),
        }
    }

    /// Returns the deployment bytecode, when codegen produced it.
    pub fn deployment(&self) -> Option<&Bytes> {
        self.deployment.as_ref()
    }

    /// Returns the runtime bytecode, when codegen produced it.
    pub fn runtime(&self) -> Option<&Bytes> {
        self.runtime.as_ref()
    }
}

/// Copies constant data and clears its padding through `padded_size`.
pub(super) fn copy_data_to_memory(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
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
        builder.memory_zero(dest, size);
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
        let tail = builder.add_u64_offset(dest, tail_offset as u64);
        let zero = builder.imm(0);
        builder.mstore(tail, zero);
    }
    let size = builder.imm(data.len() as u64);
    let data = module.intern_data(data, name);
    builder.data_copy(data, dest, size);
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
pub(super) fn store_data_words(builder: &mut FunctionBuilder<'_>, dest: ValueId, data: &[u8]) {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    for (index, chunk) in data.chunks(word_size).enumerate() {
        let value = builder.imm(U256::from_be_bytes(padded_data_word(chunk)));
        let address = builder.add_u64_offset(dest, (index * word_size) as u64);
        builder.mstore(address, value);
    }
}

/// Expands a repeated word with logarithmically many `MCOPY` operations.
fn copy_splat_to_memory(
    gcx: Gcx<'_>,
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    data: &[u8],
    clear_tail: bool,
) -> bool {
    let word_size = EvmMemoryLayout::WORD_SIZE as usize;
    if !gcx.sess.opts.optimization.is_size()
        || !gcx.sess.opts.evm_version.has_mcopy()
        || !is_repeated_word(data)
    {
        return false;
    }

    if clear_tail {
        let tail_offset = data.len() / word_size * word_size;
        let tail = builder.add_u64_offset(dest, tail_offset as u64);
        let zero = builder.imm(0);
        builder.mstore(tail, zero);
    }
    let value = builder.imm(U256::from_be_bytes(padded_data_word(&data[..word_size])));
    builder.mstore(dest, value);
    let mut filled = word_size;
    if data.len() >= word_size * 2 {
        let target = builder.add_u64_offset(dest, word_size as u64);
        builder.mstore(target, value);
        filled += word_size;
    }
    while filled < data.len() {
        let chunk = filled.min(data.len() - filled);
        let target = builder.add_u64_offset(dest, filled as u64);
        let size = builder.imm(chunk as u64);
        builder.mcopy(target, dest, size);
        filled += chunk;
    }
    true
}

pub(super) fn contract_bytecode_data_name(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    creation: bool,
) -> Symbol {
    let kind = if creation { "initcode" } else { "runtime_code" };
    Symbol::intern(&format!("{}_{kind}", gcx.hir.contract(contract_id).name))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_word() {
        let word = std::array::from_fn::<_, 32, _>(|index| index as u8);

        assert!(!is_repeated_word(&word[..31]));
        assert!(is_repeated_word(&word));
        assert!(is_repeated_word(&word.repeat(3)));

        let mut partial = word.repeat(2);
        partial.extend_from_slice(&word[..7]);
        assert!(is_repeated_word(&partial));

        partial[35] ^= 1;
        assert!(!is_repeated_word(&partial));
        partial[35] ^= 1;
        *partial.last_mut().unwrap() ^= 1;
        assert!(!is_repeated_word(&partial));
    }
}
