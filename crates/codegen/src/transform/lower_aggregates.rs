//! Lower semantic memory/storage aggregate operations to word operations.

use crate::{
    mir::{
        Function, FunctionBuilder, InstKind, Module, PackedValue, StorageField, StorageLayout,
        ValueId,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
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

enum AggregateOp {
    StorageToMemory { storage: ValueId, memory: ValueId, layout: Arc<StorageLayout> },
    MemoryToStorage { memory: ValueId, storage: ValueId, layout: Arc<StorageLayout> },
    ClearStorage { storage: ValueId, layout: Arc<StorageLayout> },
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

    let blocks: Vec<_> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let op = match &builder.func().inst(inst).kind {
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
            // Packed neighbors share a slot; fields are in placement order, so
            // one cached load serves every extraction from the same slot.
            let mut loaded: Option<(u64, ValueId)> = None;
            for (index, field) in fields.iter().enumerate() {
                let dest = builder.memory_object_field_addr(
                    memory,
                    crate::mir::MemoryObjectLayout::structure(fields.len() as u64),
                    index as u64,
                );
                match &field.shape {
                    StorageField::Word => {
                        let slot = offset_value(builder, storage, field.slot);
                        let value = builder.sload(slot);
                        builder.mstore(dest, value);
                    }
                    StorageField::Packed(value) => {
                        let word = match loaded {
                            Some((slot, word)) if slot == field.slot => word,
                            _ => {
                                let slot = offset_value(builder, storage, field.slot);
                                let word = builder.sload(slot);
                                loaded = Some((field.slot, word));
                                word
                            }
                        };
                        let extracted = builder.extract_packed(word, *value, field.offset);
                        builder.mstore(dest, extracted);
                    }
                    StorageField::Aggregate(nested) => {
                        let slot = offset_value(builder, storage, field.slot);
                        lower_nested_storage_to_memory(builder, nested, slot, dest);
                    }
                }
            }
        }
        StorageLayout::Array { element, len } => match element {
            StorageField::Packed(value) => {
                let per_slot = u64::from(value.per_slot());
                let mut loaded: Option<(u64, ValueId)> = None;
                for index in 0..*len {
                    let dest = array_element_addr(builder, memory, *len, index);
                    let slot_offset = index / per_slot;
                    let byte_offset = ((index % per_slot) * u64::from(value.size)) as u8;
                    let word = match loaded {
                        Some((slot, word)) if slot == slot_offset => word,
                        _ => {
                            let slot = offset_value(builder, storage, slot_offset);
                            let word = builder.sload(slot);
                            loaded = Some((slot_offset, word));
                            word
                        }
                    };
                    let extracted = builder.extract_packed(word, *value, byte_offset);
                    builder.mstore(dest, extracted);
                }
            }
            _ => {
                let stride = element.storage_slots();
                for index in 0..*len {
                    let dest = array_element_addr(builder, memory, *len, index);
                    let slot = offset_value(builder, storage, index * stride);
                    match element {
                        StorageField::Word => {
                            let value = builder.sload(slot);
                            builder.mstore(dest, value);
                        }
                        StorageField::Aggregate(nested) => {
                            lower_nested_storage_to_memory(builder, nested, slot, dest);
                        }
                        StorageField::Packed(_) => unreachable!(),
                    }
                }
            }
        },
    }
}

/// Allocates a nested memory object, fills it from storage, and stores its
/// pointer into the parent word at `dest`.
fn lower_nested_storage_to_memory(
    builder: &mut FunctionBuilder<'_>,
    layout: &Arc<StorageLayout>,
    slot: ValueId,
    dest: ValueId,
) {
    let size = builder.imm_u64(layout.memory_words() * 32);
    let object_layout = match layout.as_ref() {
        StorageLayout::Struct(fields) => {
            crate::mir::MemoryObjectLayout::Struct { fields: fields.len() as u64 }
        }
        StorageLayout::Array { len, .. } => {
            crate::mir::MemoryObjectLayout::FixedArray { len: *len, element_words: 1 }
        }
    };
    let nested =
        builder.alloc_object(size, object_layout, crate::mir::AllocationSemantics::INTERNAL);
    lower_storage_to_memory(builder, layout, slot, nested);
    builder.mstore(dest, nested);
}

fn lower_memory_to_storage(
    builder: &mut FunctionBuilder<'_>,
    layout: &StorageLayout,
    memory: ValueId,
    storage: ValueId,
) {
    match layout {
        StorageLayout::Struct(fields) => {
            let mut index = 0;
            while index < fields.len() {
                let field = &fields[index];
                match &field.shape {
                    StorageField::Word => {
                        let source = struct_field_addr(builder, memory, fields.len(), index);
                        let value = builder.mload(source);
                        let slot = offset_value(builder, storage, field.slot);
                        builder.sstore(slot, value);
                        index += 1;
                    }
                    StorageField::Aggregate(nested) => {
                        let source = struct_field_addr(builder, memory, fields.len(), index);
                        let pointer = builder.mload(source);
                        let slot = offset_value(builder, storage, field.slot);
                        lower_memory_to_storage(builder, nested, pointer, slot);
                        index += 1;
                    }
                    StorageField::Packed(_) => {
                        let group_len = fields[index..]
                            .iter()
                            .take_while(|other| {
                                other.slot == field.slot
                                    && matches!(other.shape, StorageField::Packed(_))
                            })
                            .count();
                        let sources = (index..index + group_len)
                            .map(|i| struct_field_addr(builder, memory, fields.len(), i))
                            .collect::<Vec<_>>();
                        let group = &fields[index..index + group_len];
                        // A struct copy updates members like member assignments
                        // do: bytes of the slot outside the members survive.
                        store_packed_group(
                            builder,
                            storage,
                            field.slot,
                            group.iter().map(|f| (f.offset, packed(&f.shape))),
                            &sources,
                            true,
                        );
                        index += group_len;
                    }
                }
            }
        }
        StorageLayout::Array { element, len } => match element {
            StorageField::Packed(value) => {
                let per_slot = u64::from(value.per_slot());
                let mut index = 0u64;
                while index < *len {
                    let slot_offset = index / per_slot;
                    let group_len = per_slot.min(*len - index);
                    let sources = (index..index + group_len)
                        .map(|i| array_element_addr(builder, memory, *len, i))
                        .collect::<Vec<_>>();
                    let placements = (0..group_len)
                        .map(|i| ((i * u64::from(value.size)) as u8, *value))
                        .collect::<Vec<_>>();
                    // An array copy rebuilds each data slot from its elements,
                    // zero-filling bytes past the packed elements like solc.
                    store_packed_group(
                        builder,
                        storage,
                        slot_offset,
                        placements.iter().copied(),
                        &sources,
                        false,
                    );
                    index += group_len;
                }
            }
            _ => {
                let stride = element.storage_slots();
                for index in 0..*len {
                    let source = array_element_addr(builder, memory, *len, index);
                    let slot = offset_value(builder, storage, index * stride);
                    match element {
                        StorageField::Word => {
                            let value = builder.mload(source);
                            builder.sstore(slot, value);
                        }
                        StorageField::Aggregate(nested) => {
                            let pointer = builder.mload(source);
                            lower_memory_to_storage(builder, nested, pointer, slot);
                        }
                        StorageField::Packed(_) => unreachable!(),
                    }
                }
            }
        },
    }
}

fn packed(shape: &StorageField) -> PackedValue {
    match shape {
        StorageField::Packed(value) => *value,
        _ => unreachable!("packed group contains a non-packed field"),
    }
}

/// Composes one storage slot from packed member values loaded from memory and
/// stores it. `preserve` keeps slot bytes not covered by any member; without
/// it uncovered bytes are zero-filled.
fn store_packed_group(
    builder: &mut FunctionBuilder<'_>,
    storage: ValueId,
    slot_offset: u64,
    placements: impl Iterator<Item = (u8, PackedValue)> + Clone,
    sources: &[ValueId],
    preserve: bool,
) {
    let mut covered = U256::ZERO;
    for (offset, value) in placements.clone() {
        covered |= value.mask() << (usize::from(offset) * 8);
    }
    let slot = offset_value(builder, storage, slot_offset);
    let mut acc = if !preserve || covered == U256::MAX {
        None
    } else {
        let word = builder.sload(slot);
        let keep = builder.imm_u256(!covered);
        Some(builder.and(word, keep))
    };
    for ((offset, value), &source) in placements.zip(sources) {
        let loaded = builder.mload(source);
        let prepared = builder.prepare_packed(loaded, value);
        let shifted = if offset == 0 {
            prepared
        } else {
            let shift = builder.imm_u64(u64::from(offset) * 8);
            builder.shl(shift, prepared)
        };
        acc = Some(match acc {
            Some(acc) => builder.or(acc, shifted),
            None => shifted,
        });
    }
    if let Some(acc) = acc {
        builder.sstore(slot, acc);
    }
}

fn struct_field_addr(
    builder: &mut FunctionBuilder<'_>,
    memory: ValueId,
    fields: usize,
    index: usize,
) -> ValueId {
    builder.memory_object_field_addr(
        memory,
        crate::mir::MemoryObjectLayout::structure(fields as u64),
        index as u64,
    )
}

fn array_element_addr(
    builder: &mut FunctionBuilder<'_>,
    memory: ValueId,
    len: u64,
    index: u64,
) -> ValueId {
    let index_value = builder.imm_u64(index);
    builder.memory_object_element_addr(
        memory,
        crate::mir::MemoryObjectLayout::word_fixed_array(len),
        index_value,
    )
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
