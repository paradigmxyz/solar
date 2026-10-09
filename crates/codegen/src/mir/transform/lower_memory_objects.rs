//! Lower semantic memory-object operations to physical word operations.
//!
//! The selected memory-layout policy supplies object headers, field offsets, and element strides.
//! Semantic accesses and allocations become raw pointer arithmetic, loads, stores, and allocation
//! operations. Mixed slice/object merges are materialized first so later operations still use the
//! correct representation. Unreachable blocks are removed before substitution: their definitions
//! need not obey SSA and can contain cast cycles.
//!
//! This runs after SSA structs and mutable frame slots have been lowered, and rejects modules
//! that still have live SSA structs. The module stays semantic until the final conversion
//! verifies all backend representation requirements.
//! An earlier simplification can replace a zero-offset object projection with
//! its slice operand. Such a function still needs slice-load lowering even if
//! it no longer contains object operations. Unsupported layouts
//! and address spaces retain their operations for the subsequent phase checks.

use crate::mir::{
    AllocationAlignment, AllocationKind, AllocationSemantics, Function, FunctionBuilder, Immediate,
    InstKind, MemoryObjectLayout, MirPhase, MirType, Module, SliceLocation, Value,
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{index::IndexVec, map::FxHashMap};
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
        gcx: Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        if module.has_struct_values() {
            analyses.fail(
                gcx.dcx()
                    .err(
                        "`lower-memory-objects` requires scalar structs; run `lower-structs` first",
                    )
                    .emit(),
            );
            return false;
        }
        if module.phase() == MirPhase::Lowered {
            return false;
        }
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function::<EvmMemoryLayout>(func);
        }
        changed
    }
}

fn lower_function<P: MemoryLayoutPolicy>(func: &mut Function) -> bool {
    let is_pointer = |ty: MirType| ty == MirType::MemPtr;
    let needs_lowering = func.arg_indices().any(|index| is_pointer(func.arg_ty(index)))
        || func.return_components().iter().copied().any(is_pointer)
        || func.live_values().any(|value| func.value_ty(value).is_some_and(is_pointer))
        || func.instructions().any(|inst_id| {
            let kind = &func.inst(inst_id).kind;
            kind.is_memory_object_op()
                || matches!(kind, InstKind::MLoad(object) if func.value_slice_location(*object).is_some())
        });
    if !needs_lowering {
        return false;
    }

    // unreachable definitions -> removed blocks and phi inputs
    let _ = super::cfg_simplify::remove_unreachable_blocks(func);
    materialize_mixed_byte_phis(func);
    let views = local_views(func);
    let mut replacements = FxHashMap::default();
    let blocks = func.blocks.indices();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for &inst in &instructions {
            if builder
                .func()
                .inst_result_value(inst)
                .is_some_and(|value| replacements.contains_key(&value))
            {
                continue;
            }
            let kind = builder.func().inst(inst).kind.clone();
            let keep = (|| {
                match kind {
                    InstKind::Alloc { size, kind: AllocationKind::Object(_), semantics } => {
                        let instruction = builder.func_mut().inst_mut(inst);
                        instruction.kind =
                            InstKind::Alloc { size, kind: AllocationKind::Raw, semantics };
                    }
                    // A view whose uses all lower here becomes its length load; each use
                    // recomputes its data address from the object.
                    InstKind::MemorySlice(object)
                        if builder
                            .func()
                            .inst_result_value(inst)
                            .is_some_and(|view| views.contains_key(&view)) =>
                    {
                        // header = ptrtoint object to i256
                        // view = mload header
                        let header = builder.cast(object, MirType::I256);
                        let instruction = builder.func_mut().inst_mut(inst);
                        instruction.kind = InstKind::MLoad(header);
                        instruction.result_ty = Some(MirType::I256);
                    }
                    InstKind::SliceLen(slice) if views.contains_key(&slice) => {
                        if let Some(result) = builder.func().inst_result_value(inst) {
                            replacements.insert(result, slice);
                        }
                        return false;
                    }
                    InstKind::MemorySlice(object) => {
                        let (ptr, len) = memory_slice_parts::<P>(&mut builder, object);
                        builder.func_mut().inst_mut(inst).kind =
                            InstKind::MakeSlice { ptr, len, location: SliceLocation::Memory };
                    }
                    InstKind::SlicePtr(slice) if let Some(&object) = views.get(&slice) => {
                        // data = add (ptrtoint object), P::DYNAMIC_HEADER_SIZE
                        let header = builder.cast(object, MirType::I256);
                        let offset = builder.imm(P::DYNAMIC_HEADER_SIZE);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(header, offset);
                    }
                    InstKind::MLoad(object) => {
                        let Some(location) = builder.func().value_slice_location(object) else {
                            return true;
                        };
                        // %source = slice_ptr %object
                        // %value = mload %source | calldataload %source
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
                                if !multiple_users && let Some(mut user) = user {
                                    let mut address_value = result;
                                    let mut cast_result = None;
                                    if let InstKind::PtrToInt(value, 256) =
                                        builder.func().inst(user).kind
                                        && value == result
                                    {
                                        address_value =
                                            builder.func().inst_result_value(user).unwrap();
                                        cast_result = Some(address_value);
                                        let mut uses = instructions.iter().copied().filter(|&id| {
                                            builder
                                                .func()
                                                .inst(id)
                                                .operands()
                                                .contains(&address_value)
                                        });
                                        if let Some(load) = uses.next()
                                            && uses.next().is_none()
                                        {
                                            user = load;
                                        }
                                    }
                                    if matches!(builder.func().inst(user).kind, InstKind::MLoad(value) if value == address_value)
                                    {
                                        // address = slice_ptr calldata_slice + field_offset
                                        // value = calldataload address
                                        let base = builder.slice_ptr(object);
                                        let address = builder.add_u64_offset(base, offset);
                                        builder.func_mut().inst_mut(user).kind =
                                            InstKind::CalldataLoad(address);
                                        if let Some(cast_result) = cast_result {
                                            replacements.insert(cast_result, address);
                                        }
                                        return false;
                                    }
                                }
                            }
                            if location != SliceLocation::Memory {
                                return true;
                            }
                            let base = builder.slice_ptr(object);
                            let address = builder.add_u64_offset(base, offset);
                            // address = inttoptr byte_offset to memptr
                            let address = builder.cast(address, MirType::MemPtr);
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
                        let (data, len) = memory_slice_parts::<P>(&mut builder, object);
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
                    InstKind::SliceLoadElement { slice, index } => {
                        let Some(location) = slice_location(builder.func(), &views, slice) else {
                            return true;
                        };
                        let address =
                            slice_element_address::<P>(&mut builder, &views, slice, index);
                        let Some(kind) = slice_load_kind(location, address) else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = kind;
                    }
                    InstKind::MemoryObjectLoadElement { object, layout, index } => {
                        let Some(address) =
                            memory_element_address::<P>(&mut builder, object, index, layout)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::SliceLoadByte { slice, index } => {
                        let Some(location) = slice_location(builder.func(), &views, slice) else {
                            return true;
                        };
                        let source = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, source, index);
                        let Some(load) = slice_load_kind(location, address) else {
                            return true;
                        };
                        let word = builder.emit_inst(load, Some(MirType::I256));
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
                    InstKind::SliceStoreElement { slice, index, value } => {
                        let address =
                            slice_element_address::<P>(&mut builder, &views, slice, index);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::SliceStoreByte { slice, index, value } => {
                        let base = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, base, index);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore8(address, value);
                    }
                    InstKind::SliceStoreWord { slice, offset, value } => {
                        let base = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, base, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::MemorySliceLoadWord { slice, offset } => {
                        let source = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, source, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MLoad(address);
                    }
                    InstKind::CalldataSliceLoadWord { slice, offset } => {
                        let source = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, source, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::CalldataLoad(address);
                    }
                    InstKind::SliceCopy { destination, offset, source } => {
                        let base = slice_data::<P>(&mut builder, &views, destination);
                        let destination = dynamic_offset_address(&mut builder, base, offset);
                        let Some(physical) =
                            lower_slice_copy::<P>(&mut builder, &views, destination, source)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = physical;
                    }
                    _ => {}
                }
                true
            })();
            if keep {
                let mut kind = builder.func().inst(inst).kind.clone();
                if kind.evm_opcode().is_some()
                    && !kind.scalar_types_match(builder.func(), builder.func().inst(inst).result_ty)
                {
                    // scalar_operand = zext scalar_operand to i256
                    kind.visit_operands_mut(|value| {
                        if matches!(
                            builder.func().value_ty(*value),
                            Some(MirType::Int(bits)) if bits.get() < 256
                        ) {
                            *value = builder.cast_word(*value);
                        }
                    });
                    builder.func_mut().inst_mut(inst).kind = kind;
                }
                builder.func_mut().blocks[block].instructions.push(inst);
            }
        }
    }

    func.replace_uses_canonicalized(&replacements);
    func.attributes.may_return_memory |=
        func.params.iter().chain(func.return_components()).any(|ty| ty.is_memory_reference());
    coalesce_constant_allocations(func);
    normalize_pointer_operands(func);
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

            let mut allocations = Vec::new();
            allocations.push((inst_id, first, false));
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
                    InstKind::PtrToInt(base, 256) => owners[*base],
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
            let preserves_fmp =
                allocations.iter().any(|(inst, _, _)| func.inst(*inst).metadata.preserves_fmp());
            let base = allocations[0].1.result;
            let size = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(total))));
            let mut offset = 0_u64;
            // base = alloc total !preserves_fmp(any member)
            // remaining members = base + offset
            for (index, (allocation_id, allocation, _)) in allocations.iter().enumerate() {
                if index == 0 {
                    let inst = func.inst_mut(*allocation_id);
                    inst.kind = InstKind::Alloc {
                        size,
                        kind: AllocationKind::Raw,
                        semantics: AllocationSemantics::INTERNAL,
                    };
                    inst.metadata.set_preserves_fmp(preserves_fmp);
                } else {
                    let offset_value =
                        func.alloc_value(Value::Immediate(Immediate::I256(U256::from(offset))));
                    let inst = func.inst_mut(*allocation_id);
                    inst.kind = InstKind::Add(base, offset_value);
                    inst.metadata.set_effect(Some(inst.kind.effect_kind()));
                    inst.metadata.set_memory_region(None);
                    inst.metadata.set_storage_alias(None);
                    inst.metadata.clear_deferred_alloc();
                    inst.metadata.set_preserves_fmp(false);
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
///
/// NOTE: pointers carry no object kind, so this treats every pointer phi with a
/// slice input as bytes; only `bytes` and `string` values join slices in source.
fn materialize_mixed_byte_phis(func: &mut Function) {
    let blocks = func.blocks.indices();
    for block in blocks {
        let phis: Vec<_> = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|&inst| {
                let Some(result) = func.inst_result_value(inst) else { return false };
                matches!(func.value_ty(result), Some(MirType::MemPtr))
                    && matches!(func.inst(inst).kind, InstKind::Phi(_))
            })
            .collect();
        for inst in phis {
            let InstKind::Phi(incoming) = func.inst(inst).kind.clone() else { continue };
            if !incoming
                .iter()
                .any(|(_, value)| matches!(func.value_ty(*value), Some(MirType::Slice(_))))
                || !incoming.iter().all(|(_, value)| {
                    matches!(func.value_ty(*value), Some(MirType::Slice(_) | MirType::MemPtr))
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
                let object = builder.alloc_object(size, MemoryObjectLayout::Bytes, semantics);
                builder.set_memory_len(object, length);
                builder.memory_copy_from_slice(object, value);
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

/// Expands a dynamic object's header into its payload address and length.
fn memory_slice_parts<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    object: crate::mir::ValueId,
) -> (crate::mir::ValueId, crate::mir::ValueId) {
    // header = ptrtoint object to i256
    // len = mload header
    // data = add header, P::DYNAMIC_HEADER_SIZE
    let header = builder.cast(object, MirType::I256);
    let len = builder.mload(header);
    let data = builder.add_u64_offset(header, P::DYNAMIC_HEADER_SIZE);
    (data, len)
}

/// Returns the `memory_slice` views whose every use lowers in this pass, by their objects.
fn local_views(func: &Function) -> FxHashMap<crate::mir::ValueId, crate::mir::ValueId> {
    let mut views: FxHashMap<_, _> = func
        .instructions()
        .filter_map(|inst| match func.inst(inst).kind {
            InstKind::MemorySlice(object) => Some((func.inst_result_value(inst)?, object)),
            _ => None,
        })
        .collect();
    for inst in func.instructions() {
        let kind = &func.inst(inst).kind;
        let local = matches!(
            kind,
            InstKind::SliceLen(_)
                | InstKind::SlicePtr(_)
                | InstKind::SliceLoadElement { .. }
                | InstKind::SliceLoadByte { .. }
                | InstKind::SliceStoreElement { .. }
                | InstKind::SliceStoreByte { .. }
                | InstKind::SliceStoreWord { .. }
                | InstKind::MemorySliceLoadWord { .. }
                | InstKind::SliceCopy { .. }
        );
        if !local {
            kind.visit_operands(|operand| {
                views.remove(&operand);
            });
        }
    }
    for block in &func.blocks {
        if let Some(terminator) = &block.terminator {
            terminator.visit_operands(|operand| {
                views.remove(&operand);
            });
        }
    }
    views
}

fn slice_location(
    func: &Function,
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    slice: crate::mir::ValueId,
) -> Option<SliceLocation> {
    if views.contains_key(&slice) {
        Some(SliceLocation::Memory)
    } else {
        func.value_slice_location(slice)
    }
}

/// Returns a slice's data address, recomputing it at the use for a `memory_slice` view.
fn slice_data<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    slice: crate::mir::ValueId,
) -> crate::mir::ValueId {
    let Some(&object) = views.get(&slice) else { return builder.slice_ptr(slice) };
    let header = builder.cast(object, MirType::I256);
    builder.add_u64_offset(header, P::DYNAMIC_HEADER_SIZE)
}

fn slice_element_address<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    slice: crate::mir::ValueId,
    index: crate::mir::ValueId,
) -> crate::mir::ValueId {
    if let Some(&object) = views.get(&slice)
        && let Some(offset) = builder
            .func()
            .value_u64(index)
            .and_then(|index| index.checked_mul(P::WORD_SIZE)?.checked_add(P::DYNAMIC_HEADER_SIZE))
    {
        // address = add (ptrtoint object), header + index * stride
        let header = builder.cast(object, MirType::I256);
        return builder.add_u64_offset(header, offset);
    }
    let base = slice_data::<P>(builder, views, slice);
    if let Some(index) = builder.func().value_u64(index)
        && let Some(offset) = index.checked_mul(P::WORD_SIZE)
    {
        builder.add_u64_offset(base, offset)
    } else {
        let stride = builder.imm(P::WORD_SIZE);
        let offset = builder.mul(index, stride);
        builder.add(base, offset)
    }
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
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    destination: crate::mir::ValueId,
    source: crate::mir::ValueId,
) -> Option<InstKind> {
    let location = slice_location(builder.func(), views, source)?;
    let source_ptr = slice_data::<P>(builder, views, source);
    // A local view becomes its own length load.
    let length = if views.contains_key(&source) { source } else { builder.slice_len(source) };
    Some(match location {
        SliceLocation::Memory => InstKind::MCopy(destination, source_ptr, length),
        SliceLocation::Calldata => InstKind::CalldataCopy(destination, source_ptr, length),
        SliceLocation::Returndata => InstKind::ReturnDataCopy(destination, source_ptr, length),
    })
}

/// Materializes integer operands and pointer results at the physical opcode boundary.
pub(super) fn normalize_pointer_operands(func: &mut Function) {
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for id in instructions {
            let inst = builder.func().inst(id).clone();
            if inst.kind.evm_opcode().is_some()
                && !inst.kind.scalar_types_match(builder.func(), inst.result_ty)
            {
                builder.set_debug_context(&inst.metadata);
                let mut kind = inst.kind;
                // integer_operand = ptrtoint pointer_operand to i256
                kind.visit_operands_mut(|value| *value = builder.cast(*value, MirType::I256));
                if inst.result_ty == Some(MirType::MemPtr) {
                    // integer_result = opcode integer_operands
                    // pointer_result = inttoptr integer_result to memptr
                    let value = builder.emit_inst(kind, Some(MirType::I256));
                    if let Value::Inst(emitted) = *builder.func().value(value) {
                        builder.func_mut().inst_mut(emitted).metadata = inst.metadata.clone();
                    }
                    let instruction = builder.func_mut().inst_mut(id);
                    instruction.kind = InstKind::IntToPtr(value);
                    instruction.metadata.set_effect(None);
                    instruction.metadata.set_memory_region(None);
                } else {
                    builder.func_mut().inst_mut(id).kind = kind;
                }
            }
            builder.func_mut().blocks[block].instructions.push(id);
        }
    }
}
