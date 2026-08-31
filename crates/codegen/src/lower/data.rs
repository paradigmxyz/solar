//! Constant program data lowering.

use super::Lowerer;
use crate::{
    backend::evm::{data_copy_cost, data_copy_is_profitable, ir::immediate_materialization_cost},
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, ValueId},
};
use alloy_primitives::{Bytes, U256};
use solar_interface::Symbol;
use solar_sema::hir::ContractId;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct ContractBytecodes {
    /// Deployment bytecode, including the initcode prefix.
    pub deployment: Bytes,
    /// Deployed runtime bytecode.
    pub runtime: Bytes,
}

impl<'gcx> Lowerer<'gcx> {
    /// Registers a contract's deployment and runtime bytecode.
    pub(crate) fn register_contract_bytecodes(
        &mut self,
        contract_id: ContractId,
        bytecodes: ContractBytecodes,
    ) {
        self.contract_bytecodes.insert(contract_id, bytecodes);
    }

    /// Copies constant data and clears its padding through `padded_size`.
    pub(super) fn copy_data_to_memory(
        &mut self,
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
            self.store_data_words(builder, dest, data);
            return;
        }
        if data.iter().all(|&byte| byte == 0) {
            let size = builder.imm_u64(padded_size as u64);
            builder.memory_zero(dest, size);
            return;
        }
        let separate_tail = name.is_some()
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
        if self.copy_splat_to_memory(builder, dest, &data) {
            return;
        }
        if !self.data_copy_is_profitable(&data, separate_tail) {
            self.store_data_words(builder, dest, &data);
            return;
        }
        if separate_tail {
            let word_size = EvmMemoryLayout::WORD_SIZE as usize;
            let tail_offset = data.len() / word_size * word_size;
            let tail = if tail_offset == 0 {
                dest
            } else {
                let offset = builder.imm_u64(tail_offset as u64);
                builder.add(dest, offset)
            };
            let zero = builder.imm_u64(0);
            builder.mstore(tail, zero);
        }
        let size = builder.imm_u64(data.len() as u64);
        let data = self.module.intern_data(data, name);
        builder.data_copy(data, dest, size);
    }

    fn data_copy_is_profitable(&self, data: &[u8], separate_tail: bool) -> bool {
        let evm_version = self.gcx.sess.opts.evm_version;
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
            self.gcx.sess.opts.optimization,
            old_gas as i128 - new_gas as i128,
            old_size as i128 - new_size as i128,
        )
    }

    /// Stores data as words for short values and word-level constant pooling.
    pub(super) fn store_data_words(
        &self,
        builder: &mut FunctionBuilder<'_>,
        dest: ValueId,
        data: &[u8],
    ) {
        let word_size = EvmMemoryLayout::WORD_SIZE as usize;
        for (index, chunk) in data.chunks(word_size).enumerate() {
            let value = builder.imm_u256(U256::from_be_bytes(padded_data_word(chunk)));
            let address = if index == 0 {
                dest
            } else {
                let offset = builder.imm_u64((index * word_size) as u64);
                builder.add(dest, offset)
            };
            builder.mstore(address, value);
        }
    }

    /// Expands a repeated word with logarithmically many `MCOPY` operations.
    fn copy_splat_to_memory(
        &self,
        builder: &mut FunctionBuilder<'_>,
        dest: ValueId,
        data: &[u8],
    ) -> bool {
        let word_size = EvmMemoryLayout::WORD_SIZE as usize;
        if !self.gcx.sess.opts.optimization.is_size()
            || !self.gcx.sess.opts.evm_version.has_mcopy()
            || !is_repeated_word(data)
        {
            return false;
        }

        let value = builder.imm_u256(U256::from_be_bytes(padded_data_word(&data[..word_size])));
        builder.mstore(dest, value);
        let mut filled = word_size;
        while filled < data.len() {
            let chunk = filled.min(data.len() - filled);
            let offset = builder.imm_u64(filled as u64);
            let target = builder.add(dest, offset);
            let size = builder.imm_u64(chunk as u64);
            builder.mcopy(target, dest, size);
            filled += chunk;
        }
        true
    }

    pub(super) fn contract_bytecode_data_name(
        &self,
        contract_id: ContractId,
        creation: bool,
    ) -> Symbol {
        let kind = if creation { "initcode" } else { "runtime_code" };
        Symbol::intern(&format!("{}_{kind}", self.gcx.hir.contract(contract_id).name))
    }
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
