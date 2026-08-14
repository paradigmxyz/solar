//! Lower semantic memory-object operations to physical word operations.

use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        AllocationAlignment, AllocationKind, AllocationSemantics, Function, FunctionBuilder,
        InstKind, MemoryObjectKind, MemoryObjectLayout, MirPhase, MirType, Module, Value,
    },
    pass::MirPass,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

/// Lowers semantic object layouts under the selected physical memory policy.
pub(crate) struct LowerMemoryObjects;

impl MirPass for LowerMemoryObjects {
    fn name(&self) -> &'static str {
        "lower-memory-objects"
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
        if module.phase >= MirPhase::MemoryLowered {
            return false;
        }
        let mut stats = LowerMemoryObjectsStats::default();
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function::<EvmMemoryLayout>(func, &mut stats);
        }
        if module.phase == MirPhase::Dispatch {
            module.advance_phase(MirPhase::MemoryLowered);
            changed = true;
        }
        changed
    }
}

/// Statistics from semantic memory-object lowering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LowerMemoryObjectsStats {
    /// Object allocations changed to raw physical allocations.
    allocations: usize,
    /// Semantic accesses expanded or erased.
    accesses: usize,
    /// Nominal object types erased to physical pointers.
    types: usize,
}

fn lower_function<P: MemoryLayoutPolicy>(
    func: &mut Function,
    stats: &mut LowerMemoryObjectsStats,
) -> bool {
    let is_object_value = |value| match func.value(value) {
        Value::Arg(_) | Value::Undef(_) => {
            func.value_ty(value).as_ref().is_some_and(is_object_type)
        }
        Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => false,
    };
    let has_objects = func.arg_indices().any(|index| is_object_type(&func.arg_ty(index)))
        || func.returns.iter().any(is_object_type)
        || func.live_values().any(is_object_value)
        || func.instructions().any(|inst_id| {
            let inst = func.inst(inst_id);
            inst.result_ty.as_ref().is_some_and(is_object_type)
                || matches!(
                    inst.kind,
                    InstKind::MemoryObjectLen(_, _)
                        | InstKind::SetMemoryObjectLen(_, _, _)
                        | InstKind::MemoryObjectData(_, _)
                        | InstKind::MemoryObjectFieldAddr { .. }
                        | InstKind::MemoryObjectElementAddr { .. }
                        | InstKind::MemoryObjectLoadField { .. }
                        | InstKind::MemoryObjectStoreField { .. }
                        | InstKind::MemoryObjectLoadElement { .. }
                        | InstKind::MemoryObjectLoadByte { .. }
                        | InstKind::MemoryObjectStoreElement { .. }
                        | InstKind::MemoryObjectStoreByte { .. }
                        | InstKind::MemoryObjectStoreWord { .. }
                        | InstKind::MemorySliceLoadWord { .. }
                        | InstKind::CalldataSliceLoadWord { .. }
                        | InstKind::MemoryObjectCopyFromSlice { .. }
                        | InstKind::MemoryObjectCopyFromSliceAt { .. }
                        | InstKind::MemoryObjectCopy { .. }
                        | InstKind::Keccak256Bytes(_)
                        | InstKind::Alloc { kind: AllocationKind::Object(_), .. }
                )
        });
    if !has_objects {
        return false;
    }

    materialize_mixed_byte_phis(func);
    let mut replacements = FxHashMap::default();
    let mut removed = FxHashSet::default();
    let blocks: Vec<_> = func.blocks.indices().collect();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let kind = builder.func().inst(inst).kind.clone();
            match kind {
                InstKind::Alloc { size, kind: AllocationKind::Object(_), semantics } => {
                    let instruction = builder.func_mut().inst_mut(inst);
                    instruction.kind =
                        InstKind::Alloc { size, kind: AllocationKind::Raw, semantics };
                    stats.allocations += 1;
                }
                InstKind::MemoryObjectLen(object, kind) => {
                    let Some(offset) = P::object_length_offset(kind) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::SetMemoryObjectLen(object, len, kind) => {
                    let Some(offset) = P::object_length_offset(kind) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, len);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectData(object, kind) => {
                    let offset = P::object_data_offset(kind);
                    if offset == 0 {
                        if let Some(result) = builder.func().inst_result_value(inst) {
                            replacements.insert(result, object);
                        }
                        removed.insert(inst);
                    } else {
                        let offset = builder.imm_u64(offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                    let Some(offset) = P::field_offset(layout, field) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    if offset == 0 {
                        if let Some(result) = builder.func().inst_result_value(inst) {
                            replacements.insert(result, object);
                        }
                        removed.insert(inst);
                    } else {
                        let offset = builder.imm_u64(offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::Keccak256Bytes(object) => {
                    let kind = crate::mir::MemoryObjectKind::Bytes;
                    let Some(length_offset) = P::object_length_offset(kind) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let length_address = offset_address(&mut builder, object, length_offset);
                    let len = builder.mload(length_address);
                    let data = offset_address(&mut builder, object, P::object_data_offset(kind));
                    builder.func_mut().inst_mut(inst).kind = InstKind::Keccak256(data, len);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectElementAddr { object, layout, index } => {
                    let Some(stride) = P::element_stride(layout) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
                    let base_offset = P::object_data_offset(layout.kind());
                    let kind = if let Some(index) = builder.func().value_u64(index)
                        && let Some(offset) = index.checked_mul(stride)
                        && let Some(offset) = base_offset.checked_add(offset)
                    {
                        let offset = builder.imm_u64(offset);
                        InstKind::Add(object, offset)
                    } else {
                        let base = offset_address(&mut builder, object, base_offset);
                        let stride = builder.imm_u64(stride);
                        let offset = builder.mul(index, stride);
                        InstKind::Add(base, offset)
                    };
                    builder.func_mut().inst_mut(inst).kind = kind;
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectLoadField { object, layout, field } => {
                    let Some(offset) = P::field_offset(layout, field) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectStoreField { object, layout, field, value } => {
                    let Some(offset) = P::field_offset(layout, field) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectLoadElement { object, layout, index } => {
                    let Some(stride) = P::element_stride(layout) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
                    let base_offset = P::object_data_offset(layout.kind());
                    let address = if let Some(index) = builder.func().value_u64(index)
                        && let Some(offset) = index.checked_mul(stride)
                        && let Some(offset) = base_offset.checked_add(offset)
                    {
                        offset_address(&mut builder, object, offset)
                    } else {
                        let base = offset_address(&mut builder, object, base_offset);
                        let stride = builder.imm_u64(stride);
                        let offset = builder.mul(index, stride);
                        builder.add(base, offset)
                    };
                    builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectLoadByte { object, index } => {
                    let base = offset_address(
                        &mut builder,
                        object,
                        P::object_data_offset(crate::mir::MemoryObjectKind::Bytes),
                    );
                    let address = builder.add(base, index);
                    let word = builder.mload(address);
                    let zero = builder.imm_u64(0);
                    let byte = builder.byte(zero, word);
                    if let Some(result) = builder.func().inst_result_value(inst) {
                        replacements.insert(result, byte);
                    }
                    removed.insert(inst);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectStoreElement { object, layout, index, value } => {
                    let Some(stride) = P::element_stride(layout) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
                    let base_offset = P::object_data_offset(layout.kind());
                    let address = if let Some(index) = builder.func().value_u64(index)
                        && let Some(offset) = index.checked_mul(stride)
                        && let Some(offset) = base_offset.checked_add(offset)
                    {
                        offset_address(&mut builder, object, offset)
                    } else {
                        let base = offset_address(&mut builder, object, base_offset);
                        let stride = builder.imm_u64(stride);
                        let offset = builder.mul(index, stride);
                        builder.add(base, offset)
                    };
                    builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectStoreByte { object, index, value } => {
                    let base = offset_address(
                        &mut builder,
                        object,
                        P::object_data_offset(crate::mir::MemoryObjectKind::Bytes),
                    );
                    let address = builder.add(base, index);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MStore8(address, value);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectStoreWord { object, offset, value } => {
                    let base = offset_address(
                        &mut builder,
                        object,
                        P::object_data_offset(crate::mir::MemoryObjectKind::Bytes),
                    );
                    let address = builder.add(base, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    stats.accesses += 1;
                }
                InstKind::MemorySliceLoadWord { slice, offset } => {
                    let source = builder.slice_ptr(slice);
                    let address = dynamic_offset_address(&mut builder, source, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::CalldataSliceLoadWord { slice, offset } => {
                    let source = builder.slice_ptr(slice);
                    let address = dynamic_offset_address(&mut builder, source, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::CalldataLoad(address);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectCopyFromSlice { object, kind, source } => {
                    let destination =
                        offset_address(&mut builder, object, P::object_data_offset(kind));
                    let Some(physical) = lower_slice_copy::<P>(&mut builder, destination, source)
                    else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    builder.func_mut().inst_mut(inst).kind = physical;
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectCopyFromSliceAt { object, kind, offset, source } => {
                    let base = offset_address(&mut builder, object, P::object_data_offset(kind));
                    let destination = builder.add(base, offset);
                    let Some(physical) = lower_slice_copy::<P>(&mut builder, destination, source)
                    else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    builder.func_mut().inst_mut(inst).kind = physical;
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectCopy {
                    destination,
                    destination_kind,
                    source,
                    source_kind,
                    length,
                } => {
                    let destination = offset_address(
                        &mut builder,
                        destination,
                        P::object_data_offset(destination_kind),
                    );
                    let source =
                        offset_address(&mut builder, source, P::object_data_offset(source_kind));
                    builder.func_mut().inst_mut(inst).kind =
                        InstKind::MCopy(destination, source, length);
                    stats.accesses += 1;
                }
                _ => {}
            }
            if !removed.contains(&inst) {
                builder.func_mut().blocks[block].instructions.push(inst);
            }
        }
    }

    if !replacements.is_empty() {
        func.replace_uses_canonicalized(&replacements);
    }
    erase_object_types(func, stats);
    true
}

/// Materializes a calldata slice before it enters a memory-object phi.
///
/// A conditional assignment such as `bytes memory x = condition ? bytes(0) :
/// msg.data[...]` has one memory-object incoming edge and one slice incoming
/// edge. The memory-object users after this pass expect the same header/data
/// representation on both edges, so copy the slice into a fresh bytes object
/// before forming the phi.
fn materialize_mixed_byte_phis(func: &mut Function) {
    let blocks: Vec<_> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = func.blocks[block].instructions.clone();
        for inst in instructions {
            let Some(result) = func.inst_result_value(inst) else { continue };
            if !matches!(
                func.value_ty(result),
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
            ) {
                continue;
            }
            let InstKind::Phi(incoming) = func.inst(inst).kind.clone() else { continue };
            if !incoming
                .iter()
                .any(|(_, value)| matches!(func.value_ty(*value), Some(MirType::Slice(_))))
                || !incoming.iter().all(|(_, value)| {
                    matches!(
                        func.value_ty(*value),
                        Some(MirType::Slice(_) | MirType::MemoryObject(MemoryObjectKind::Bytes),)
                    )
                })
            {
                continue;
            }

            let mut lowered = Vec::with_capacity(incoming.len());
            for (predecessor, value) in incoming {
                if !matches!(func.value_ty(value), Some(MirType::Slice(_))) {
                    lowered.push((predecessor, value));
                    continue;
                }

                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(predecessor);
                let length = builder.slice_len(value);
                let word = builder.imm_u64(EvmMemoryLayout::WORD_SIZE);
                let size = builder.add(length, word);
                let semantics = AllocationSemantics {
                    alignment: AllocationAlignment::Word,
                    ..AllocationSemantics::SOLIDITY_UNINITIALIZED
                };
                let object = builder.alloc_object(size, MemoryObjectLayout::Bytes, semantics);
                builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
                builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, value);
                lowered.push((predecessor, object));
            }
            func.inst_mut(inst).kind = InstKind::Phi(lowered);
        }
    }
}

fn offset_address(
    builder: &mut FunctionBuilder<'_>,
    base: crate::mir::ValueId,
    offset: u64,
) -> crate::mir::ValueId {
    if offset == 0 {
        base
    } else {
        let offset = builder.imm_u64(offset);
        builder.add(base, offset)
    }
}

fn dynamic_offset_address(
    builder: &mut FunctionBuilder<'_>,
    base: crate::mir::ValueId,
    offset: crate::mir::ValueId,
) -> crate::mir::ValueId {
    if builder.func().value_u64(offset) == Some(0) { base } else { builder.add(base, offset) }
}

fn lower_slice_copy<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    destination: crate::mir::ValueId,
    source: crate::mir::ValueId,
) -> Option<InstKind> {
    match builder.func().value_ty(source)? {
        MirType::MemoryObject(kind) => {
            let length_offset = P::object_length_offset(kind)?;
            let source_ptr = offset_address(builder, source, P::object_data_offset(kind));
            let length_address = offset_address(builder, source, length_offset);
            let length = builder.mload(length_address);
            Some(InstKind::MCopy(destination, source_ptr, length))
        }
        MirType::Slice(location) => {
            let source_ptr = builder.slice_ptr(source);
            let length = builder.slice_len(source);
            Some(match location {
                crate::mir::SliceLocation::Memory => {
                    InstKind::MCopy(destination, source_ptr, length)
                }
                crate::mir::SliceLocation::Calldata => {
                    InstKind::CalldataCopy(destination, source_ptr, length)
                }
                crate::mir::SliceLocation::Returndata => {
                    InstKind::ReturnDataCopy(destination, source_ptr, length)
                }
            })
        }
        _ => None,
    }
}

fn erase_object_types(func: &mut Function, stats: &mut LowerMemoryObjectsStats) {
    let arg_indices: Vec<_> = func.arg_indices().collect();
    for index in arg_indices {
        let mut ty = func.arg_ty(index);
        erase_object_type(&mut ty, stats);
        func.set_arg_ty(index, ty);
    }
    for ty in &mut func.returns {
        erase_object_type(ty, stats);
    }
    let mut values = DenseBitSet::new_empty(func.num_values());
    for value in func.live_values() {
        values.insert(value);
    }
    for value in values.iter() {
        match func.value_mut(value) {
            Value::Undef(ty) => erase_object_type(ty, stats),
            Value::Arg(_) | Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => {}
        }
    }
    func.for_each_instruction_mut(|_, inst| {
        if let Some(ty) = &mut inst.result_ty {
            erase_object_type(ty, stats);
        }
    });
}

fn erase_object_type(ty: &mut MirType, stats: &mut LowerMemoryObjectsStats) {
    if matches!(ty, MirType::MemoryObject(_)) {
        *ty = MirType::MemPtr;
        stats.types += 1;
    }
}

fn is_object_type(ty: &MirType) -> bool {
    matches!(ty, MirType::MemoryObject(_))
}
