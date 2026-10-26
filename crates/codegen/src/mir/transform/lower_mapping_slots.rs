//! Lower mapping-slot and storage-array slot builtins to physical hashing.
//!
//! This pass expands storage-location hashing at the memory boundary. Variable-size
//! hash inputs use the free-memory pointer as transient scratch; fixed-width
//! mapping and storage-array hashes use reserved scratch. The semantic instructions
//! declare these writes so earlier passes cannot forward stale scratch reads.
//! CSE can share fixed-width hashes only while their inputs and scratch writes
//! remain unchanged. Variable-width hashes require physical alias proofs.
//! Constant fixed-width hashes use a target-priced immediate when it is cheaper. Keep their
//! scratch stores so assembly reads and memory expansion retain the same effects; ordinary
//! memory DSE removes those stores only when they are unobserved.

use super::egraph::use_counts;
use crate::{
    backend::evm::op,
    mir::{
        FunctionBuilder, InstKind, Module, PtrLocation, ValueId,
        pass::{MirPass, run_function_pass},
    },
    target::Target,
};
use alloy_primitives::{U256, keccak256};
use solar_data_structures::map::FxHashMap;

/// Lowers mapping-slot hash builtins at the memory boundary.
pub(crate) struct LowerMappingSlots;

impl MirPass for LowerMappingSlots {
    fn name(&self) -> &'static str {
        "lower-mapping-slots"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            let target = analyses.target(gcx);
            let has_mapping_slots = func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::MappingSlot(_, _)
                        | InstKind::MappingSlotBytes(_, _)
                        | InstKind::StorageArrayDataSlot(_)
                        | InstKind::StorageArrayElementSlot { .. }
                )
            });
            if !has_mapping_slots {
                return false;
            }

            let uses = use_counts(func);
            let mut replacements = FxHashMap::default();
            let block_ids = func.blocks.indices();
            for block_id in block_ids {
                let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block_id);
                for inst_id in instructions {
                    let result = builder.func().inst_result_value(inst_id);
                    let count = result.and_then(|value| uses.get(value).copied()).unwrap_or(1);
                    let replacement = match builder.func().inst(inst_id).kind {
                        InstKind::MappingSlot(key, slot) => {
                            lower_fixed_slot_hash(&mut builder, &target, &[key, slot], count)
                        }
                        InstKind::MappingSlotBytes(key, slot) => {
                            lower_bytes_mapping_slot(&mut builder, key, slot)
                        }
                        InstKind::StorageArrayDataSlot(slot) => {
                            lower_fixed_slot_hash(&mut builder, &target, &[slot], count)
                        }
                        InstKind::StorageArrayElementSlot { slot, index, element_slots } => {
                            lower_storage_array_element_slot(
                                &mut builder,
                                &target,
                                slot,
                                index,
                                element_slots,
                            )
                        }
                        _ => {
                            builder.func_mut().blocks[block_id].instructions.push(inst_id);
                            continue;
                        }
                    };
                    let result = builder
                        .func()
                        .inst_result_value(inst_id)
                        .expect("mapping slot must produce a value");
                    replacements.insert(result, replacement);
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn lower_fixed_slot_hash(
    builder: &mut FunctionBuilder<'_>,
    target: &Target,
    words: &[ValueId],
    uses: u32,
) -> crate::mir::ValueId {
    // store(32 * i, words[i])
    for (index, &word) in words.iter().enumerate() {
        let offset = builder.imm(32 * index as u64);
        builder.store(offset, word);
    }
    if target.optimization() != solar_config::OptimizationMode::None
        && let Some(constants) =
            words.iter().map(|&word| builder.func().value_u256(word)).collect::<Option<Vec<_>>>()
    {
        let bytes = constants.iter().flat_map(|word| word.to_be_bytes::<32>()).collect::<Vec<_>>();
        let hash = U256::from_be_bytes(keccak256(&bytes).0);
        let size = U256::from(bytes.len());
        // The scratch stores are common to both shapes. A constant can be rematerialized at
        // each use; charge those copies rather than assuming it remains on the stack.
        let uses = uses.max(1);
        let hashed = target.push(size)
            + target.push(U256::ZERO)
            + target.opcode_with_immediates(op::KECCAK256, &[Some(U256::ZERO), Some(size)])
            + target.dup().times(uses - 1);
        let pushed = target.push(hash).times(uses);
        if target.cmp_lifetime(pushed, hashed).is_lt() {
            // result = constant_hash(words)
            return builder.imm(hash);
        }
    }
    // result = keccak256(0, 32 * words.len())
    let size = builder.imm(32 * words.len() as u64);
    let zero = builder.imm(0);
    builder.keccak256(zero, size)
}

fn lower_storage_array_element_slot(
    builder: &mut FunctionBuilder<'_>,
    target: &Target,
    slot: crate::mir::ValueId,
    index: crate::mir::ValueId,
    element_slots: u64,
) -> crate::mir::ValueId {
    // data_slot = storage_array_data_slot(slot)
    // result = data_slot + index * element_slots
    let data_slot = lower_fixed_slot_hash(builder, target, &[slot], 1);
    let offset = if element_slots <= 1 {
        index
    } else {
        let stride = builder.imm(element_slots);
        builder.mul(index, stride)
    };
    builder.add(data_slot, offset)
}

/// Hash a `bytes` key, from memory or calldata, and its slot in free memory.
fn lower_bytes_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    value: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    // length = len(value)
    // scratch = fmp
    // copy(scratch, data(value), length)
    // mstore(scratch + length, slot)
    // result = keccak256(scratch, length + 32)
    let location = builder.func().value_slice_location(value).unwrap_or(PtrLocation::Memory);
    let len = builder.memory_len(value);
    let word_size = builder.imm(32);
    let payload_size = builder.add(len, word_size);
    let scratch = builder.fmp();
    let source = builder.memory_data(value);
    builder.copy_heap(scratch, source, len, location);
    let slot_address = builder.add(scratch, len);
    builder.store(slot_address, slot);
    builder.keccak256(scratch, payload_size)
}
