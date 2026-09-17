//! Constant program data lowering.

use crate::{
    backend::evm::{
        ir::immediate_materialization_cost,
        op::{WORD_BYTES, push_len},
    },
    link::{LibraryRelocation, RelocatableBytecode},
    mir::{FunctionBuilder, Module, ValueId, memory::EvmMemoryLayout},
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_interface::{Symbol, diagnostics::DiagCtxt};
use solar_sema::{Gcx, hir::ContractId};
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

#[derive(Clone, Debug, Default)]
pub struct ContractBytecodes {
    /// Deployment bytecode, including the initcode prefix.
    deployment: RelocatableBytecode,
    /// Deployed runtime bytecode.
    runtime: RelocatableBytecode,
}

impl ContractBytecodes {
    /// Creates bytecode metadata from a generated artifact and its relocations.
    pub fn new(deployment: RelocatableBytecode, runtime: RelocatableBytecode) -> Self {
        Self { deployment, runtime }
    }

    /// Validates library identities and address ranges before bytecode embedding.
    pub fn validate(&self, dcx: &DiagCtxt) -> solar_interface::Result {
        let mut error = None;
        for (kind, bytecode) in [("deployment", &self.deployment), ("runtime", &self.runtime)] {
            for relocation in &bytecode.relocations {
                if bytecode.libraries.get(relocation.library).is_none() {
                    error = Some(
                        dcx.err(format!(
                            "{kind} bytecode relocation references nonexistent library {}",
                            relocation.library.index()
                        ))
                        .emit(),
                    );
                }
                if relocation.offset.checked_add(20).is_none_or(|end| end > bytecode.bytes.len()) {
                    error = Some(
                        dcx.err(format!("{kind} bytecode relocation exceeds bytecode bounds"))
                            .emit(),
                    );
                }
            }
        }
        error.map_or(Ok(()), Err)
    }

    /// Returns the deployment bytecode, when codegen produced it.
    pub(crate) fn deployment(&self) -> Option<&RelocatableBytecode> {
        (!self.deployment.bytes.is_empty()).then_some(&self.deployment)
    }

    /// Returns the runtime bytecode, when codegen produced it.
    pub(crate) fn runtime(&self) -> Option<&RelocatableBytecode> {
        (!self.runtime.bytes.is_empty()).then_some(&self.runtime)
    }
}

/// Copies embedded bytecode, remapping its library relocations into this module.
pub(super) fn copy_bytecode_to_memory(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    bytecode: &RelocatableBytecode,
    padded_size: usize,
    name: Symbol,
) {
    let data = &bytecode.bytes;
    debug_assert!(padded_size >= data.len());
    if bytecode.relocations.is_empty() {
        copy_data_to_memory(gcx, module, builder, dest, data, padded_size, Some(name));
        return;
    }
    // memory_zero dest + floor(size / 32) * 32, padded_size - floor(size / 32) * 32
    // data_copy linked_bytecode, dest, size
    if padded_size > data.len() {
        let tail = builder.add_u64_offset(dest, (data.len() / WORD_BYTES * WORD_BYTES) as u64);
        let size = builder.imm((padded_size - data.len() / WORD_BYTES * WORD_BYTES) as u64);
        builder.memory_zero(tail, size);
    }
    let size = builder.imm(data.len() as u64);
    let relocations = bytecode
        .relocations
        .iter()
        .map(|reloc| LibraryRelocation {
            offset: reloc.offset,
            library: module
                .libraries
                .intern(*bytecode.libraries.get(reloc.library).expect("valid embedded library ID")),
        })
        .collect();
    let data = module.intern_linked_data(bytecode.bytes.clone(), Some(name), relocations);
    builder.data_copy(data, dest, size);
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
    use crate::link::{Library, LibraryTable};
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, Session, sym};

    #[test]
    fn contract_bytecodes_validate_relocations() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        sess.enter(|| {
            let mut libraries = LibraryTable::default();
            let library = libraries.intern(Library { source: sym::Test, name: sym::Test });
            let valid = RelocatableBytecode {
                libraries,
                bytes: vec![0; 20].into(),
                relocations: vec![LibraryRelocation { offset: 0, library }],
            };
            let bytecodes = ContractBytecodes::new(valid.clone(), valid.clone());
            assert!(bytecodes.validate(&sess.dcx).is_ok());
            assert_eq!(bytecodes.deployment(), Some(&valid));
            assert_eq!(bytecodes.runtime(), Some(&valid));

            for invalid in [
                RelocatableBytecode { libraries: LibraryTable::default(), ..valid.clone() },
                RelocatableBytecode { bytes: vec![0; 19].into(), ..valid.clone() },
                RelocatableBytecode { bytes: Default::default(), ..valid.clone() },
                RelocatableBytecode {
                    relocations: vec![LibraryRelocation { offset: usize::MAX, library }],
                    ..valid.clone()
                },
            ] {
                let deployment = ContractBytecodes::new(invalid.clone(), valid.clone());
                assert!(deployment.validate(&sess.dcx).is_err());
                let runtime = ContractBytecodes::new(valid.clone(), invalid);
                assert!(runtime.validate(&sess.dcx).is_err());
            }
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: deployment bytecode relocation references nonexistent library 0

error: runtime bytecode relocation references nonexistent library 0

error: deployment bytecode relocation exceeds bytecode bounds

error: runtime bytecode relocation exceeds bytecode bounds


"#]]
            );
        });
    }

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
