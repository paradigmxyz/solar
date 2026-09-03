//! Lower semantic memory-object operations to physical word operations.

use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        AllocationAlignment, AllocationKind, AllocationSemantics, Function, FunctionBuilder,
        Immediate, InstKind, MemoryObjectKind, MemoryObjectLayout, MirPhase, MirType, Module,
        SliceLocation, Value,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
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
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function::<EvmMemoryLayout>(func);
        }
        if module.phase == MirPhase::Dispatch {
            module.advance_phase(MirPhase::MemoryLowered);
            changed = true;
        }
        changed
    }
}

fn lower_function<P: MemoryLayoutPolicy>(func: &mut Function) -> bool {
    let is_object_value = |value| func.value_ty(value).as_ref().is_some_and(is_object_type);
    let has_objects = func.arg_indices().any(|index| is_object_type(&func.arg_ty(index)))
        || func.returns.iter().any(is_object_type)
        || func.live_values().any(is_object_value)
        || func.instructions().any(|inst_id| func.inst(inst_id).kind.is_memory_object_op());
    if !has_objects {
        return false;
    }

    materialize_mixed_byte_phis(func);
    let mut replacements = FxHashMap::default();
    let blocks = func.blocks.indices();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for &inst in &instructions {
            let kind = builder.func().inst(inst).kind.clone();
            let keep = (|| {
                match kind {
                    InstKind::Alloc { size, kind: AllocationKind::Object(_), semantics } => {
                        let instruction = builder.func_mut().inst_mut(inst);
                        instruction.kind =
                            InstKind::Alloc { size, kind: AllocationKind::Raw, semantics };
                    }
                    InstKind::MemoryObjectLen(object, kind) => {
                        if matches!(builder.func().value_ty(object), Some(MirType::Slice(_))) {
                            builder.func_mut().inst_mut(inst).kind = InstKind::SliceLen(object);
                            return true;
                        }
                        let Some(offset) = P::object_length_offset(kind) else {
                            return true;
                        };
                        let address = builder.add_u64_offset(object, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::SetMemoryObjectLen(object, len, kind) => {
                        let Some(offset) = P::object_length_offset(kind) else {
                            return true;
                        };
                        let address = builder.add_u64_offset(object, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, len);
                    }
                    InstKind::MemoryObjectData(object, kind) => {
                        if matches!(builder.func().value_ty(object), Some(MirType::Slice(_))) {
                            let Some(result) = builder.func().inst_result_value(inst) else {
                                return false;
                            };
                            let pointer = builder.slice_ptr(object);
                            replacements.insert(result, pointer);
                            return false;
                        }
                        let offset = P::object_data_offset(kind);
                        if offset == 0 {
                            if let Some(result) = builder.func().inst_result_value(inst) {
                                replacements.insert(result, object);
                            }
                            return false;
                        }
                        let offset = builder.imm(offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(object, offset);
                    }
                    InstKind::MLoad(object) => {
                        let Some(location) = builder.func().value_slice_location(object) else {
                            return true;
                        };
                        let source = builder.slice_ptr(object);
                        let Some(kind) = slice_load_kind(location, source) else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = kind;
                    }
                    InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                        let Some(offset) = P::field_offset(layout, field) else {
                            return true;
                        };
                        if let Some(location) = builder.func().value_slice_location(object) {
                            if location == SliceLocation::Calldata {
                                let Some(result) = builder.func().inst_result_value(inst) else {
                                    return true;
                                };
                                let mut user = None;
                                let mut multiple_users = false;
                                for &candidate in &instructions {
                                    if !builder.func().inst(candidate).operands().contains(&result)
                                    {
                                        continue;
                                    }
                                    if user.replace(candidate).is_some() {
                                        multiple_users = true;
                                        break;
                                    }
                                }
                                if !multiple_users
                                    && let Some(user) = user
                                    && matches!(builder.func().inst(user).kind, InstKind::MLoad(value) if value == result)
                                {
                                    let base = builder.slice_ptr(object);
                                    let address = builder.add_u64_offset(base, offset);
                                    builder.func_mut().inst_mut(user).kind =
                                        InstKind::CalldataLoad(address);
                                    return false;
                                }
                            }
                            if location != SliceLocation::Memory {
                                return true;
                            }
                            let base = builder.slice_ptr(object);
                            let address = builder.add_u64_offset(base, offset);
                            if let Some(result) = builder.func().inst_result_value(inst) {
                                replacements.insert(result, address);
                            }
                            return false;
                        }
                        if offset == 0 {
                            if let Some(result) = builder.func().inst_result_value(inst) {
                                replacements.insert(result, object);
                            }
                            return false;
                        }
                        let offset = builder.imm(offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(object, offset);
                    }
                    InstKind::Keccak256Bytes(object) => {
                        if let Some(MirType::Slice(location)) = builder.func().value_ty(object) {
                            let source = builder.slice_ptr(object);
                            let len = builder.slice_len(object);
                            let data = match location {
                                SliceLocation::Memory => source,
                                location
                                @ (SliceLocation::Calldata | SliceLocation::Returndata) => {
                                    let data = builder.fmp();
                                    builder.copy_slice_data(location, data, source, len);
                                    data
                                }
                            };
                            builder.func_mut().inst_mut(inst).kind = InstKind::Keccak256(data, len);
                            return true;
                        }
                        let kind = crate::mir::MemoryObjectKind::Bytes;
                        let Some(length_offset) = P::object_length_offset(kind) else {
                            return true;
                        };
                        let length_address = builder.add_u64_offset(object, length_offset);
                        let len = builder.mload(length_address);
                        let data = builder.add_u64_offset(object, P::object_data_offset(kind));
                        builder.func_mut().inst_mut(inst).kind = InstKind::Keccak256(data, len);
                    }
                    InstKind::MemoryObjectElementAddr { object, layout, index } => {
                        // The rewritten `Add` keeps the `MemPtr` result type. This is safe because
                        // Solidity bounds-checks the index before forming the address.
                        let Some((base, offset)) =
                            memory_element_address_parts::<P>(&mut builder, object, index, layout)
                        else {
                            return true;
                        };
                        let offset = offset.unwrap_or_else(|| builder.imm(0));
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(base, offset);
                    }
                    InstKind::MemoryObjectLoadField { object, layout, field } => {
                        let Some(offset) = P::field_offset(layout, field) else {
                            return true;
                        };
                        if let Some(location) = builder.func().value_slice_location(object) {
                            let base = builder.slice_ptr(object);
                            let address = builder.add_u64_offset(base, offset);
                            let Some(kind) = slice_load_kind(location, address) else {
                                return true;
                            };
                            builder.func_mut().inst_mut(inst).kind = kind;
                            return true;
                        }
                        let address = builder.add_u64_offset(object, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::MemoryObjectStoreField { object, layout, field, value } => {
                        let Some(offset) = P::field_offset(layout, field) else {
                            return true;
                        };
                        let address = builder.add_u64_offset(object, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::MemoryObjectLoadElement { object, layout, index } => {
                        if let Some(location) = builder.func().value_slice_location(object) {
                            if !matches!(
                                layout,
                                MemoryObjectLayout::DynamicArray { element_words: 1 }
                                    | MemoryObjectLayout::FixedArray { element_words: 1, .. }
                            ) {
                                return true;
                            }
                            let Some(stride) = P::element_stride(layout) else {
                                return true;
                            };
                            let base = builder.slice_ptr(object);
                            let address = if let Some(index) = builder.func().value_u64(index)
                                && let Some(offset) = index.checked_mul(stride)
                            {
                                builder.add_u64_offset(base, offset)
                            } else {
                                let stride = builder.imm(stride);
                                let offset = builder.mul(index, stride);
                                builder.add(base, offset)
                            };
                            let Some(kind) = slice_load_kind(location, address) else {
                                return true;
                            };
                            builder.func_mut().inst_mut(inst).kind = kind;
                            return true;
                        }
                        let Some(address) =
                            memory_element_address::<P>(&mut builder, object, index, layout)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::MemoryObjectLoadByte { object, index } => {
                        let word = if let Some(location) =
                            builder.func().value_slice_location(object)
                        {
                            let source = builder.slice_ptr(object);
                            let address = dynamic_offset_address(&mut builder, source, index);
                            match location {
                                crate::mir::SliceLocation::Calldata => {
                                    builder.calldataload(address)
                                }
                                crate::mir::SliceLocation::Memory => builder.mload(address),
                                crate::mir::SliceLocation::Returndata => return true,
                            }
                        } else {
                            let address = memory_bytes_address::<P>(&mut builder, object, index);
                            builder.mload(address)
                        };
                        let zero = builder.imm(0);
                        let byte = builder.byte(zero, word);
                        if let Some(result) = builder.func().inst_result_value(inst) {
                            replacements.insert(result, byte);
                        }
                        return false;
                    }
                    InstKind::MemoryObjectStoreElement { object, layout, index, value } => {
                        let Some(address) =
                            memory_element_address::<P>(&mut builder, object, index, layout)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::MemoryObjectStoreByte { object, index, value } => {
                        let address = memory_bytes_address::<P>(&mut builder, object, index);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore8(address, value);
                    }
                    InstKind::MemoryObjectStoreWord { object, offset, value } => {
                        let address = memory_bytes_address::<P>(&mut builder, object, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::MemorySliceLoadWord { slice, offset } => {
                        let source = builder.slice_ptr(slice);
                        let address = dynamic_offset_address(&mut builder, source, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::CalldataSliceLoadWord { slice, offset } => {
                        let source = builder.slice_ptr(slice);
                        let address = dynamic_offset_address(&mut builder, source, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::CalldataLoad(address);
                    }
                    InstKind::MemoryObjectCopyFromSlice { object, kind, source } => {
                        let Some(physical) =
                            lower_object_copy::<P>(&mut builder, object, kind, None, source)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = physical;
                    }
                    InstKind::MemoryObjectCopyFromSliceAt { object, kind, offset, source } => {
                        let Some(physical) = lower_object_copy::<P>(
                            &mut builder,
                            object,
                            kind,
                            Some(offset),
                            source,
                        ) else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = physical;
                    }
                    InstKind::MemoryObjectCopy {
                        destination,
                        destination_kind,
                        source,
                        source_kind,
                        length,
                    } => {
                        let destination = builder
                            .add_u64_offset(destination, P::object_data_offset(destination_kind));
                        let source =
                            builder.add_u64_offset(source, P::object_data_offset(source_kind));
                        builder.func_mut().inst_mut(inst).kind =
                            InstKind::MCopy(destination, source, length);
                    }
                    _ => {}
                }
                true
            })();
            if keep {
                builder.func_mut().blocks[block].instructions.push(inst);
            }
        }
    }

    func.replace_uses_canonicalized(&replacements);
    erase_object_types(func);
    coalesce_constant_allocations(func);
    true
}

/// Combines adjacent exact internal allocations before their first observable
/// operation. Lowering a call's literal arguments commonly emits one small
/// allocation per argument; one bump for the whole group keeps the same
/// disjoint ranges while removing repeated free-memory-pointer updates.
fn coalesce_constant_allocations(func: &mut Function) {
    for block in func.blocks.indices() {
        let instructions = func.blocks[block].instructions.clone();
        let mut position = 0;
        while position < instructions.len() {
            let inst_id = instructions[position];
            let Some(first) = constant_raw_allocation(func, inst_id) else {
                position += 1;
                continue;
            };

            let mut allocations = vec![(inst_id, first, false)];
            let mut owners = IndexVec::from_vec(vec![None; func.num_values()]);
            owners[first.result] = Some(0);
            let mut scan = position + 1;
            while scan < instructions.len() {
                let next_id = instructions[scan];
                if let Some(allocation) = constant_raw_allocation(func, next_id) {
                    if allocation.deferred_alloc != first.deferred_alloc {
                        break;
                    }
                    let index = allocations.len();
                    allocations.push((next_id, allocation, false));
                    owners[allocation.result] = Some(index);
                    scan += 1;
                    continue;
                }

                let result = func.inst_result_value(next_id);
                let derived_owner = match &func.inst(next_id).kind {
                    InstKind::Add(lhs, rhs) if func.value_u64(*rhs).is_some() => owners[*lhs],
                    InstKind::Add(lhs, rhs) if func.value_u64(*lhs).is_some() => owners[*rhs],
                    _ => None,
                };
                let store_owner = match &func.inst(next_id).kind {
                    InstKind::MStore(address, _) | InstKind::MStore8(address, _) => {
                        owners[*address]
                    }
                    _ => None,
                };
                if let Some(owner) = store_owner {
                    allocations[owner].2 = true;
                }
                if let Some(owner) = derived_owner
                    && let Some(result) = result
                {
                    owners[result] = Some(owner);
                }
                if store_owner.is_some() || derived_owner.is_some() {
                    scan += 1;
                    continue;
                }
                break;
            }

            if allocations.len() < 2 || allocations.iter().any(|(_, _, stored)| !stored) {
                position += 1;
                continue;
            }

            let Some(total) = allocations
                .iter()
                .try_fold(0_u64, |total, (_, allocation, _)| total.checked_add(allocation.size))
            else {
                position = scan;
                continue;
            };
            let base = allocations[0].1.result;
            let size = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(total))));
            let mut offset = 0_u64;
            for (index, (allocation_id, allocation, _)) in allocations.iter().enumerate() {
                if index == 0 {
                    let inst = func.inst_mut(*allocation_id);
                    inst.kind = InstKind::Alloc {
                        size,
                        kind: AllocationKind::Raw,
                        semantics: AllocationSemantics::INTERNAL,
                    };
                } else {
                    let offset_value =
                        func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(offset))));
                    let inst = func.inst_mut(*allocation_id);
                    inst.kind = InstKind::Add(base, offset_value);
                    inst.metadata.set_effect(Some(inst.kind.effect_kind()));
                    inst.metadata.set_memory_region(None);
                    inst.metadata.set_storage_alias(None);
                    inst.metadata.clear_deferred_alloc();
                }
                offset = offset.saturating_add(allocation.size);
            }
            position = scan;
        }
    }
}

#[derive(Clone, Copy)]
struct ConstantRawAllocation {
    result: crate::mir::ValueId,
    size: u64,
    deferred_alloc: bool,
}

fn constant_raw_allocation(
    func: &Function,
    inst_id: crate::mir::InstId,
) -> Option<ConstantRawAllocation> {
    let InstKind::Alloc { size, kind: AllocationKind::Raw, semantics } = func.inst(inst_id).kind
    else {
        return None;
    };
    if semantics != AllocationSemantics::INTERNAL {
        return None;
    }
    Some(ConstantRawAllocation {
        result: func.inst_result_value(inst_id)?,
        size: func.value_u64(size)?,
        deferred_alloc: func.inst(inst_id).metadata.deferred_alloc(),
    })
}

fn slice_load_kind(location: SliceLocation, address: crate::mir::ValueId) -> Option<InstKind> {
    match location {
        SliceLocation::Calldata => Some(InstKind::CalldataLoad(address)),
        SliceLocation::Memory => Some(InstKind::MLoad(address)),
        SliceLocation::Returndata => None,
    }
}

/// Materializes a calldata slice before it enters a memory-object phi.
///
/// A conditional assignment such as `bytes memory x = condition ? bytes(0) :
/// msg.data[...]` has one memory-object incoming edge and one slice incoming
/// edge. The memory-object users after this pass expect the same header/data
/// representation on both edges, so copy the slice into a fresh bytes object
/// before forming the phi.
fn materialize_mixed_byte_phis(func: &mut Function) {
    let blocks = func.blocks.indices();
    for block in blocks {
        let phis: Vec<_> = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|&inst| {
                let Some(result) = func.inst_result_value(inst) else { return false };
                matches!(
                    func.value_ty(result),
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
                ) && matches!(func.inst(inst).kind, InstKind::Phi(_))
            })
            .collect();
        for inst in phis {
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
                let size = builder.add_u64_offset(length, EvmMemoryLayout::WORD_SIZE);
                let semantics = AllocationSemantics {
                    alignment: AllocationAlignment::Word,
                    ..AllocationSemantics::SOLIDITY_UNINITIALIZED
                };
                let layout = MemoryObjectLayout::Bytes;
                let object = builder.alloc_object(size, layout, semantics);
                builder.set_memory_object_len(object, length, layout.kind());
                builder.memory_object_copy_from_slice(object, layout.kind(), value);
                lowered.push((predecessor, object));
            }
            func.inst_mut(inst).kind = InstKind::Phi(lowered);
        }
    }
}

fn dynamic_offset_address(
    builder: &mut FunctionBuilder<'_>,
    base: crate::mir::ValueId,
    offset: crate::mir::ValueId,
) -> crate::mir::ValueId {
    if builder.func().value_u64(offset) == Some(0) { base } else { builder.add(base, offset) }
}

fn memory_bytes_address<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    object: crate::mir::ValueId,
    offset: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let base = builder.add_u64_offset(object, P::object_data_offset(MemoryObjectKind::Bytes));
    builder.add(base, offset)
}

fn memory_element_address<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    object: crate::mir::ValueId,
    index: crate::mir::ValueId,
    layout: MemoryObjectLayout,
) -> Option<crate::mir::ValueId> {
    let (base, offset) = memory_element_address_parts::<P>(builder, object, index, layout)?;
    Some(offset.map_or(base, |offset| dynamic_offset_address(builder, base, offset)))
}

fn memory_element_address_parts<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    object: crate::mir::ValueId,
    index: crate::mir::ValueId,
    layout: MemoryObjectLayout,
) -> Option<(crate::mir::ValueId, Option<crate::mir::ValueId>)> {
    let stride = P::element_stride(layout)?;
    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
    let base_offset = P::object_data_offset(layout.kind());
    if let Some(index) = builder.func().value_u64(index)
        && let Some(offset) = index.checked_mul(stride)
        && let Some(offset) = base_offset.checked_add(offset)
    {
        Some((object, (offset != 0).then(|| builder.imm(offset))))
    } else {
        let base = builder.add_u64_offset(object, base_offset);
        let stride = builder.imm(stride);
        let offset = builder.mul(index, stride);
        Some((base, Some(offset)))
    }
}

fn lower_slice_copy<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    destination: crate::mir::ValueId,
    source: crate::mir::ValueId,
) -> Option<InstKind> {
    match builder.func().value_ty(source)? {
        MirType::MemoryObject(kind) => {
            let length_offset = P::object_length_offset(kind)?;
            let source_ptr = builder.add_u64_offset(source, P::object_data_offset(kind));
            let length_address = builder.add_u64_offset(source, length_offset);
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

fn lower_object_copy<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    object: crate::mir::ValueId,
    kind: MemoryObjectKind,
    offset: Option<crate::mir::ValueId>,
    source: crate::mir::ValueId,
) -> Option<InstKind> {
    let base = builder.add_u64_offset(object, P::object_data_offset(kind));
    let destination = offset.map_or(base, |offset| dynamic_offset_address(builder, base, offset));
    lower_slice_copy::<P>(builder, destination, source)
}

fn erase_object_types(func: &mut Function) {
    for index in func.arg_indices() {
        let mut ty = func.arg_ty(index);
        erase_object_type(&mut ty);
        func.set_arg_ty(index, ty);
    }
    for ty in &mut func.returns {
        erase_object_type(ty);
    }
    let mut values = DenseBitSet::new_empty(func.num_values());
    for value in func.live_values() {
        values.insert(value);
    }
    for value in values.iter() {
        match func.value_mut(value) {
            Value::Undef(ty) => erase_object_type(ty),
            Value::Arg(_) | Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => {}
        }
    }
    func.for_each_instruction_mut(|_, inst| {
        if let Some(ty) = &mut inst.result_ty {
            erase_object_type(ty);
        }
    });
}

fn erase_object_type(ty: &mut MirType) {
    if is_object_type(ty) {
        *ty = MirType::MemPtr;
    }
}

fn is_object_type(ty: &MirType) -> bool {
    matches!(ty, MirType::MemoryObject(_))
}
