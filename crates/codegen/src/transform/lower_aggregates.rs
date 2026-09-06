//! Lower semantic memory/storage aggregate operations to word operations.
//!
//! Walk each function after the main optimization pipeline and expand aggregate
//! copies, loads, and clears using their recorded storage layouts. Packed fields
//! are grouped by slot; partial groups preserve uncovered bytes. Expansions can
//! introduce loops, so the original control transfer and its source context move
//! together to the final continuation block. Layouts remain explicit rather than
//! being inferred again from physical memory operations.

use crate::{
    mir::{
        Function, FunctionBuilder, InstKind, MemoryObjectLayout, Module, StorageField,
        StorageLayout, ValueId,
    },
    pass::MirPass,
};
use solar_sema::Gcx;

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

    let blocks = func.blocks.indices();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let (terminator, terminator_metadata) = func.blocks[block].take_terminator();
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match &builder.func().inst(inst).kind {
                InstKind::StorageToMemory { storage, memory, layout } => {
                    let (storage, memory, layout) = (*storage, *memory, layout.clone());
                    lower_storage_to_memory(&mut builder, &layout, storage, memory);
                }
                InstKind::MemoryToStorage { memory, storage, layout } => {
                    let (memory, storage, layout) = (*memory, *storage, layout.clone());
                    lower_memory_to_storage(&mut builder, &layout, memory, storage);
                }
                InstKind::ClearStorage { storage, layout } => {
                    let (storage, layout) = (*storage, layout.clone());
                    lower_clear_storage(&mut builder, &layout, storage);
                }
                _ => {
                    let current = builder.current_block();
                    builder.func_mut().blocks[current].instructions.push(inst);
                    continue;
                }
            }
        }
        let current = builder.current_block();
        // final_block: lowered_aggregate_ops; terminator !metadata(original block)
        builder.func_mut().blocks[current].terminator = terminator;
        builder.func_mut().blocks[current].terminator_metadata = terminator_metadata;
    }
    true
}

fn lower_storage_to_memory(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    storage: ValueId,
    memory: ValueId,
) {
    visit_storage_fields(builder, layout, storage, memory, |builder, field, slot, destination| {
        lower_storage_field_to_memory(builder, field, slot, destination);
    });
}

fn visit_storage_fields(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    storage: ValueId,
    memory: ValueId,
    mut visit: impl FnMut(&mut FunctionBuilder<'_>, &StorageField, ValueId, MemoryObjectAccess),
) {
    let memory_layout = memory_object_layout(layout);
    match layout {
        StorageLayout::Struct(fields) => {
            let mut storage_offset = 0;
            for (index, field) in fields.iter().enumerate() {
                let slot = builder.add_u64_offset(storage, storage_offset);
                visit(
                    builder,
                    field,
                    slot,
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
            let length = builder.imm(*len);
            let stride = builder.imm(element.storage_slots());
            builder.counted_loop(length, |builder, index| {
                let offset = builder.mul(index, stride);
                let slot = builder.add(storage, offset);
                visit(
                    builder,
                    element,
                    slot,
                    MemoryObjectAccess::Element { object: memory, layout: memory_layout, index },
                );
            });
        }
    }
}

fn lower_storage_field_to_memory(
    builder: &mut FunctionBuilder<'_>,
    field: &StorageField,
    slot: ValueId,
    dest: MemoryObjectAccess,
) {
    match field {
        StorageField::Word => {
            let value = builder.sload(slot);
            store_memory_object_word(builder, dest, value);
        }
        StorageField::Aggregate(layout) => {
            let size = builder.imm(layout.memory_words() * 32);
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
    visit_storage_fields(builder, layout, storage, memory, |builder, field, slot, source| {
        lower_memory_field_to_storage(builder, field, source, slot);
    });
}

fn lower_memory_field_to_storage(
    builder: &mut FunctionBuilder<'_>,
    field: &StorageField,
    source: MemoryObjectAccess,
    slot: ValueId,
) {
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
    let zero = builder.imm(0);
    let slots = builder.imm(layout.storage_slots());
    builder.counted_loop(slots, |builder, offset| {
        let slot = builder.add(storage, offset);
        builder.sstore(slot, zero);
    });
}
