//! Lower semantic memory/storage aggregate operations to word operations.

use crate::{
    mir::{Function, FunctionBuilder, InstKind, Module, StorageField, StorageLayout, ValueId},
    pass::ModulePass,
};
use solar_sema::Gcx;
use std::sync::Arc;

/// Lowers aggregate copies and clears after the main optimization pipeline.
pub struct LowerAggregatesPass;

impl ModulePass for LowerAggregatesPass {
    fn run(&mut self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function(func);
        }
        changed
    }
}

enum AggregateOp {
    StorageToMemory { storage: ValueId, memory: ValueId, layout: Arc<StorageLayout> },
    MemoryToStorage { memory: ValueId, storage: ValueId, layout: Arc<StorageLayout> },
    ClearStorage { storage: ValueId, layout: Arc<StorageLayout> },
}

fn lower_function(func: &mut Function) -> bool {
    let has_aggregates = func.blocks.iter().any(|block| {
        block.instructions.iter().any(|&inst| {
            matches!(
                func.instructions[inst].kind,
                InstKind::StorageToMemory { .. }
                    | InstKind::MemoryToStorage { .. }
                    | InstKind::ClearStorage { .. }
            )
        })
    });
    if !has_aggregates {
        return false;
    }

    let blocks: Vec<_> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let op = match &builder.func().instructions[inst].kind {
                InstKind::StorageToMemory { storage, memory, layout } => {
                    Some(AggregateOp::StorageToMemory {
                        storage: *storage,
                        memory: *memory,
                        layout: Arc::clone(layout),
                    })
                }
                InstKind::MemoryToStorage { memory, storage, layout } => {
                    Some(AggregateOp::MemoryToStorage {
                        memory: *memory,
                        storage: *storage,
                        layout: Arc::clone(layout),
                    })
                }
                InstKind::ClearStorage { storage, layout } => Some(AggregateOp::ClearStorage {
                    storage: *storage,
                    layout: Arc::clone(layout),
                }),
                _ => None,
            };
            match op {
                Some(AggregateOp::StorageToMemory { storage, memory, layout }) => {
                    lower_storage_to_memory(&mut builder, &layout, storage, memory);
                }
                Some(AggregateOp::MemoryToStorage { memory, storage, layout }) => {
                    lower_memory_to_storage(&mut builder, &layout, memory, storage);
                }
                Some(AggregateOp::ClearStorage { storage, layout }) => {
                    lower_clear_storage(&mut builder, &layout, storage);
                }
                None => builder.func_mut().blocks[block].instructions.push(inst),
            }
        }
    }
    true
}

fn lower_storage_to_memory(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    storage: ValueId,
    memory: ValueId,
) {
    match layout {
        StorageLayout::Struct(fields) => {
            let mut storage_offset = 0;
            for (index, field) in fields.iter().enumerate() {
                lower_storage_field_to_memory(
                    builder,
                    field,
                    storage,
                    storage_offset,
                    memory,
                    index as u64 * 32,
                );
                storage_offset += field.storage_slots();
            }
        }
        StorageLayout::Array { element, len } => {
            let mut storage_offset = 0;
            for index in 0..*len {
                lower_storage_field_to_memory(
                    builder,
                    element,
                    storage,
                    storage_offset,
                    memory,
                    index * 32,
                );
                storage_offset += element.storage_slots();
            }
        }
    }
}

fn lower_storage_field_to_memory(
    builder: &mut FunctionBuilder<'_>,
    field: &StorageField,
    storage: ValueId,
    storage_offset: u64,
    memory: ValueId,
    memory_offset: u64,
) {
    let slot = offset_value(builder, storage, storage_offset);
    let dest = offset_value(builder, memory, memory_offset);
    match field {
        StorageField::Word => {
            let value = builder.sload(slot);
            builder.mstore(dest, value);
        }
        StorageField::Aggregate(layout) => {
            let size = builder.imm_u64(layout.memory_words() * 32);
            let kind = match layout.as_ref() {
                StorageLayout::Struct(_) => crate::mir::MemoryObjectKind::Struct,
                StorageLayout::Array { .. } => crate::mir::MemoryObjectKind::FixedArray,
            };
            let nested =
                builder.alloc_object(size, kind, crate::mir::AllocationSemantics::INTERNAL);
            lower_storage_to_memory(builder, layout, slot, nested);
            builder.mstore(dest, nested);
        }
    }
}

fn lower_memory_to_storage(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    memory: ValueId,
    storage: ValueId,
) {
    match layout {
        StorageLayout::Struct(fields) => {
            let mut storage_offset = 0;
            for (index, field) in fields.iter().enumerate() {
                lower_memory_field_to_storage(
                    builder,
                    field,
                    memory,
                    index as u64 * 32,
                    storage,
                    storage_offset,
                );
                storage_offset += field.storage_slots();
            }
        }
        StorageLayout::Array { element, len } => {
            let mut storage_offset = 0;
            for index in 0..*len {
                lower_memory_field_to_storage(
                    builder,
                    element,
                    memory,
                    index * 32,
                    storage,
                    storage_offset,
                );
                storage_offset += element.storage_slots();
            }
        }
    }
}

fn lower_memory_field_to_storage(
    builder: &mut FunctionBuilder<'_>,
    field: &StorageField,
    memory: ValueId,
    memory_offset: u64,
    storage: ValueId,
    storage_offset: u64,
) {
    let source = offset_value(builder, memory, memory_offset);
    let slot = offset_value(builder, storage, storage_offset);
    let value = builder.mload(source);
    match field {
        StorageField::Word => builder.sstore(slot, value),
        StorageField::Aggregate(layout) => {
            lower_memory_to_storage(builder, layout, value, slot);
        }
    }
}

fn lower_clear_storage(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    storage: ValueId,
) {
    let zero = builder.imm_u64(0);
    for offset in 0..layout.storage_slots() {
        let slot = offset_value(builder, storage, offset);
        builder.sstore(slot, zero);
    }
}

fn offset_value(builder: &mut FunctionBuilder<'_>, base: ValueId, offset: u64) -> ValueId {
    if offset == 0 {
        base
    } else {
        let offset = builder.imm_u64(offset);
        builder.add(base, offset)
    }
}
