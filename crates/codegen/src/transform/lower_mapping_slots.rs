//! Lower mapping-slot and storage-array slot builtins to physical hashing.
//!
//! Keeping storage-location hashing as one MIR instruction lets dominator-tree
//! CSE reuse repeated accesses without teaching HIR lowering about scratch
//! memory. This pass expands the builtins at the memory boundary. Variable-size
//! hash inputs use the semantic allocation policy; fixed-width mapping and
//! storage-array hashes use reserved scratch.

use crate::{
    mir::{BlockId, FunctionBuilder, InstKind, Module},
    pass::{MirPass, run_function_pass},
};
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
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let has_mapping_slots = func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::MappingSlot(_, _)
                        | InstKind::MappingSlotMemory(_, _)
                        | InstKind::MappingSlotCalldata(_, _)
                        | InstKind::StorageArrayDataSlot(_)
                        | InstKind::StorageArrayElementSlot { .. }
                )
            });
            if !has_mapping_slots {
                return false;
            }

            let mut replacements = FxHashMap::default();
            let block_ids: Vec<BlockId> = func.blocks.indices().collect();
            for block_id in block_ids {
                let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block_id);
                for inst_id in instructions {
                    let replacement = match builder.func().inst(inst_id).kind {
                        InstKind::MappingSlot(key, slot) => {
                            Some(lower_word_mapping_slot(&mut builder, key, slot))
                        }
                        InstKind::MappingSlotMemory(key, slot) => {
                            Some(lower_memory_mapping_slot(&mut builder, key, slot))
                        }
                        InstKind::MappingSlotCalldata(key, slot) => {
                            Some(lower_calldata_mapping_slot(&mut builder, key, slot))
                        }
                        InstKind::StorageArrayDataSlot(slot) => {
                            Some(lower_storage_array_data_slot(&mut builder, slot))
                        }
                        InstKind::StorageArrayElementSlot { slot, index, element_slots } => {
                            Some(lower_storage_array_element_slot(
                                &mut builder,
                                slot,
                                index,
                                element_slots,
                            ))
                        }
                        _ => {
                            builder.func_mut().blocks[block_id].instructions.push(inst_id);
                            None
                        }
                    };
                    if let Some(replacement) = replacement {
                        let result = builder
                            .func()
                            .inst_result_value(inst_id)
                            .expect("mapping slot must produce a value");
                        replacements.insert(result, replacement);
                    }
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn lower_storage_array_data_slot(
    builder: &mut FunctionBuilder<'_>,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let word = builder.imm_u64(32);
    let zero = builder.imm_u64(0);
    builder.mstore(zero, slot);
    builder.keccak256(zero, word)
}

fn lower_storage_array_element_slot(
    builder: &mut FunctionBuilder<'_>,
    slot: crate::mir::ValueId,
    index: crate::mir::ValueId,
    element_slots: u64,
) -> crate::mir::ValueId {
    let data_slot = lower_storage_array_data_slot(builder, slot);
    let offset = if element_slots <= 1 {
        index
    } else {
        let stride = builder.imm_u64(element_slots);
        builder.mul(index, stride)
    };
    builder.add(data_slot, offset)
}

/// Hash a fixed-width mapping key in the reserved scratch region.
fn lower_word_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    key: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let zero = builder.imm_u64(0);
    let word = builder.imm_u64(32);
    let size = builder.imm_u64(64);
    builder.mstore(zero, key);
    builder.mstore(word, slot);
    builder.keccak256(zero, size)
}

fn lower_memory_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    ptr: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let len = builder.memory_object_len(ptr, crate::mir::MemoryObjectKind::Bytes);
    let word_size = builder.imm_u64(32);
    let payload_size = builder.add(len, word_size);
    let object_size = builder.add(payload_size, word_size);
    let object = builder.alloc_object(
        object_size,
        crate::mir::MemoryObjectLayout::Bytes,
        crate::mir::AllocationSemantics::INTERNAL,
    );
    builder.set_memory_object_len(object, payload_size, crate::mir::MemoryObjectKind::Bytes);
    builder.memory_object_copy(
        object,
        crate::mir::MemoryObjectKind::Bytes,
        ptr,
        crate::mir::MemoryObjectKind::Bytes,
        len,
    );
    builder.memory_object_store_word(object, len, slot);
    builder.keccak256_bytes(object)
}

fn lower_calldata_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    slice: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let len = builder.slice_len(slice);
    let word_size = builder.imm_u64(32);
    let payload_size = builder.add(len, word_size);
    let object_size = builder.add(payload_size, word_size);
    let object = builder.alloc_object(
        object_size,
        crate::mir::MemoryObjectLayout::Bytes,
        crate::mir::AllocationSemantics::INTERNAL,
    );
    builder.set_memory_object_len(object, payload_size, crate::mir::MemoryObjectKind::Bytes);
    builder.memory_object_copy_from_slice(object, crate::mir::MemoryObjectKind::Bytes, slice);
    builder.memory_object_store_word(object, len, slot);
    builder.keccak256_bytes(object)
}
