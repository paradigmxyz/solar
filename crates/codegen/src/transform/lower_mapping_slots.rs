//! Lower mapping-slot and storage-array slot builtins to physical hashing.
//!
//! Keeping storage-location hashing as one MIR instruction lets dominator-tree
//! CSE reuse repeated accesses without teaching HIR lowering about scratch
//! memory. This pass expands the builtins at the memory boundary. Variable-size
//! hash inputs use the free-memory pointer as transient scratch; fixed-width
//! mapping and storage-array hashes use reserved scratch.

use crate::{
    mir::{BlockId, FunctionBuilder, InstKind, MemoryObjectKind, Module, SliceLocation},
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
                            lower_word_mapping_slot(&mut builder, key, slot)
                        }
                        InstKind::MappingSlotMemory(key, slot) => {
                            let location = match builder.func().value_ty(key) {
                                Some(crate::mir::MirType::Slice(SliceLocation::Calldata)) => {
                                    SliceLocation::Calldata
                                }
                                _ => SliceLocation::Memory,
                            };
                            lower_slice_mapping_slot(&mut builder, location, key, slot)
                        }
                        InstKind::MappingSlotCalldata(key, slot) => lower_slice_mapping_slot(
                            &mut builder,
                            SliceLocation::Calldata,
                            key,
                            slot,
                        ),
                        InstKind::StorageArrayDataSlot(slot) => {
                            lower_storage_array_data_slot(&mut builder, slot)
                        }
                        InstKind::StorageArrayElementSlot { slot, index, element_slots } => {
                            lower_storage_array_element_slot(
                                &mut builder,
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

fn lower_slice_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    location: SliceLocation,
    value: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let len = match location {
        SliceLocation::Memory => builder.memory_object_len(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_len(value),
    };
    let word_size = builder.imm_u64(32);
    let payload_size = builder.add(len, word_size);
    let scratch = builder.fmp();
    let source = match location {
        SliceLocation::Memory => builder.memory_object_data(value, MemoryObjectKind::Bytes),
        SliceLocation::Calldata | SliceLocation::Returndata => builder.slice_ptr(value),
    };
    builder.copy_slice_data(location, scratch, source, len);
    let slot_address = builder.add(scratch, len);
    builder.mstore(slot_address, slot);
    builder.keccak256(scratch, payload_size)
}
