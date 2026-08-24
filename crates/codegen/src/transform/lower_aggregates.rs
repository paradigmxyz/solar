//! Lower semantic memory/storage aggregate operations to word operations.

use crate::{
    mir::{
        Function, FunctionBuilder, InstKind, MemoryObjectLayout, Module, StorageField,
        StorageLayout, ValueId,
    },
    pass::MirPass,
};
use solar_sema::Gcx;
use std::sync::Arc;

/// Lowers aggregate copies and clears after the main optimization pipeline.
pub(crate) struct LowerAggregates;

impl MirPass for LowerAggregates {
    fn name(&self) -> &'static str {
        "lower-aggregates"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function(func);
        }
        changed
    }
}

#[derive(Clone, Copy)]
enum MemoryObjectAccess {
    Field { object: ValueId, layout: MemoryObjectLayout, field: u64 },
    Element { object: ValueId, layout: MemoryObjectLayout, index: ValueId },
}

fn store_memory_object_word(
    builder: &mut FunctionBuilder<'_>,
    destination: MemoryObjectAccess,
    value: ValueId,
) {
    match destination {
        MemoryObjectAccess::Field { object, layout, field } => {
            builder.memory_object_store_field(object, layout, field, value)
        }
        MemoryObjectAccess::Element { object, layout, index } => {
            builder.memory_object_store_element(object, layout, index, value)
        }
    }
}

fn load_memory_object_word(
    builder: &mut FunctionBuilder<'_>,
    source: MemoryObjectAccess,
) -> ValueId {
    match source {
        MemoryObjectAccess::Field { object, layout, field } => {
            builder.memory_object_load_field(object, layout, field)
        }
        MemoryObjectAccess::Element { object, layout, index } => {
            builder.memory_object_load_element(object, layout, index)
        }
    }
}

fn memory_object_layout(layout: &StorageLayout) -> MemoryObjectLayout {
    match layout {
        StorageLayout::Struct(fields) => MemoryObjectLayout::structure(fields.len() as u64),
        StorageLayout::Array { len, .. } => MemoryObjectLayout::word_fixed_array(*len),
    }
}

fn lower_function(func: &mut Function) -> bool {
    enum AggregateOp {
        StorageToMemory(ValueId, ValueId, Arc<StorageLayout>),
        MemoryToStorage(ValueId, ValueId, Arc<StorageLayout>),
        ClearStorage(ValueId, Arc<StorageLayout>),
    }

    let has_aggregates = func.instructions().any(|inst| {
        matches!(
            func.inst(inst).kind,
            InstKind::StorageToMemory { .. }
                | InstKind::MemoryToStorage { .. }
                | InstKind::ClearStorage { .. }
        )
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
            let op = match &builder.func().inst(inst).kind {
                InstKind::StorageToMemory { storage, memory, layout } => {
                    AggregateOp::StorageToMemory(*storage, *memory, Arc::clone(layout))
                }
                InstKind::MemoryToStorage { memory, storage, layout } => {
                    AggregateOp::MemoryToStorage(*memory, *storage, Arc::clone(layout))
                }
                InstKind::ClearStorage { storage, layout } => {
                    AggregateOp::ClearStorage(*storage, Arc::clone(layout))
                }
                _ => {
                    builder.func_mut().blocks[block].instructions.push(inst);
                    continue;
                }
            };
            match op {
                AggregateOp::StorageToMemory(storage, memory, layout) => {
                    lower_storage_to_memory(&mut builder, &layout, storage, memory);
                }
                AggregateOp::MemoryToStorage(memory, storage, layout) => {
                    lower_memory_to_storage(&mut builder, &layout, memory, storage);
                }
                AggregateOp::ClearStorage(storage, layout) => {
                    lower_clear_storage(&mut builder, &layout, storage);
                }
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
    visit_storage_fields(builder, layout, memory, |builder, field, offset, destination| {
        lower_storage_field_to_memory(builder, field, storage, offset, destination);
    });
}

fn visit_storage_fields(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    memory: ValueId,
    mut visit: impl FnMut(&mut FunctionBuilder<'_>, &StorageField, u64, MemoryObjectAccess),
) {
    let memory_layout = memory_object_layout(layout);
    match layout {
        StorageLayout::Struct(fields) => {
            let mut storage_offset = 0;
            for (index, field) in fields.iter().enumerate() {
                visit(
                    builder,
                    field,
                    storage_offset,
                    MemoryObjectAccess::Field {
                        object: memory,
                        layout: memory_layout,
                        field: index as u64,
                    },
                );
                storage_offset += field.storage_slots();
            }
        }
        StorageLayout::Array { element, len } => {
            let mut storage_offset = 0;
            for index in 0..*len {
                let index_value = builder.imm_u64(index);
                visit(
                    builder,
                    element,
                    storage_offset,
                    MemoryObjectAccess::Element {
                        object: memory,
                        layout: memory_layout,
                        index: index_value,
                    },
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
    dest: MemoryObjectAccess,
) {
    let slot = builder.add_u64_offset(storage, storage_offset);
    match field {
        StorageField::Word => {
            let value = builder.sload(slot);
            store_memory_object_word(builder, dest, value);
        }
        StorageField::Aggregate(layout) => {
            let size = builder.imm_u64(layout.memory_words() * 32);
            let nested = builder.alloc_object(
                size,
                memory_object_layout(layout),
                crate::mir::AllocationSemantics::INTERNAL,
            );
            lower_storage_to_memory(builder, layout, slot, nested);
            store_memory_object_word(builder, dest, nested);
        }
    }
}

fn lower_memory_to_storage(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    memory: ValueId,
    storage: ValueId,
) {
    visit_storage_fields(builder, layout, memory, |builder, field, offset, source| {
        lower_memory_field_to_storage(builder, field, source, storage, offset);
    });
}

fn lower_memory_field_to_storage(
    builder: &mut FunctionBuilder<'_>,
    field: &StorageField,
    source: MemoryObjectAccess,
    storage: ValueId,
    storage_offset: u64,
) {
    let slot = builder.add_u64_offset(storage, storage_offset);
    let value = load_memory_object_word(builder, source);
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
        let slot = builder.add_u64_offset(storage, offset);
        builder.sstore(slot, zero);
    }
}
