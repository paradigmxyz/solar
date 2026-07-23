//! Lower mapping-slot hash builtins to ordinary memory operations.
//!
//! Keeping mapping-slot computation as one MIR instruction lets dominator-tree
//! CSE reuse repeated accesses without teaching HIR lowering about control-flow
//! scopes or memory invalidation. This pass expands the builtin immediately
//! after CSE so the remaining pipeline can optimize the concrete memory ops.

use crate::{
    mir::{BlockId, FunctionBuilder, InstKind, Module},
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::map::FxHashMap;

/// Lowers mapping-slot hash builtins after mapping-aware CSE.
pub(crate) struct LowerMappingSlots;

impl MirPass for LowerMappingSlots {
    fn name(&self) -> &'static str {
        "lower-mapping-slots"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        run_function_pass(module, |func| {
            let has_mapping_slots = func.blocks.iter().any(|block| {
                block.instructions.iter().any(|&inst_id| {
                    matches!(
                        func.instructions[inst_id].kind,
                        InstKind::MappingSlot(_, _)
                            | InstKind::MappingSlotMemory(_, _)
                            | InstKind::MappingSlotCalldata(_, _)
                    )
                })
            });
            if !has_mapping_slots {
                return false;
            }

            let inst_results = func.inst_results();
            let mut replacements = FxHashMap::default();
            let block_ids: Vec<BlockId> = func.blocks.indices().collect();
            for block_id in block_ids {
                let instructions = std::mem::take(&mut func.blocks[block_id].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block_id);
                for inst_id in instructions {
                    let replacement = match builder.func().instructions[inst_id].kind {
                        InstKind::MappingSlot(key, slot) => {
                            Some(lower_word_mapping_slot(&mut builder, key, slot))
                        }
                        InstKind::MappingSlotMemory(key, slot) => {
                            Some(lower_memory_mapping_slot(&mut builder, key, slot))
                        }
                        InstKind::MappingSlotCalldata(key, slot) => {
                            Some(lower_calldata_mapping_slot(&mut builder, key, slot))
                        }
                        _ => {
                            builder.func_mut().blocks[block_id].instructions.push(inst_id);
                            None
                        }
                    };
                    if let Some(replacement) = replacement {
                        replacements.insert(inst_results[&inst_id], replacement);
                    }
                }
            }
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn lower_word_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    key: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let zero = builder.imm_u64(0);
    builder.mstore(zero, key);
    let slot_offset = builder.imm_u64(32);
    builder.mstore(slot_offset, slot);
    let hash_size = builder.imm_u64(64);
    builder.keccak256(zero, hash_size)
}

fn lower_memory_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    ptr: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let len = builder.mload(ptr);
    let word_size = builder.imm_u64(32);
    let data_start = builder.add(ptr, word_size);
    let scratch = builder.fmp();
    builder.mcopy(scratch, data_start, len);
    let slot_addr = builder.add(scratch, len);
    builder.mstore(slot_addr, slot);
    let hash_len = builder.add(len, word_size);
    builder.keccak256(scratch, hash_len)
}

fn lower_calldata_mapping_slot(
    builder: &mut FunctionBuilder<'_>,
    slice: crate::mir::ValueId,
    slot: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let len = builder.slice_len(slice);
    let data_start = builder.slice_ptr(slice);
    let word_size = builder.imm_u64(32);
    let scratch = builder.fmp();
    builder.calldatacopy(scratch, data_start, len);
    let slot_addr = builder.add(scratch, len);
    builder.mstore(slot_addr, slot);
    let hash_len = builder.add(len, word_size);
    builder.keccak256(scratch, hash_len)
}
