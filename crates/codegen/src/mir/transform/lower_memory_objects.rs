//! Lower semantic memory-object and slice operations to physical word operations.
//!
//! The selected memory-layout policy supplies the dynamic-object header and word size. Slice
//! accesses and allocations become raw pointer arithmetic, loads, stores, and allocation
//! operations. A `memory_slice` view becomes `{object + header, mload object}`, and a fixed
//! array or struct view `make_memory_slice object, len` gets its `memptr` cast to a word. When
//! every use of a view lowers here, the view becomes only its length load, or nothing when no use
//! reads the length, and each use recomputes its data address from the object, so the address does
//! not stay live from the view to its uses. Unreachable blocks are removed before substitution:
//! their definitions need not obey SSA and can contain cast cycles.
//!
//! This runs after SSA structs and mutable frame slots have been lowered, and rejects modules
//! that still have live SSA structs. The module stays semantic until the final conversion
//! verifies all backend representation requirements.
//! Unsupported layouts and address spaces retain their operations for the
//! subsequent phase checks.

use crate::mir::{
    AllocationKind, AllocationSemantics, Function, FunctionBuilder, Immediate, InstKind, MirPhase,
    MirType, Module, SliceLocation, Value,
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
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
        || func.instructions().any(|inst_id| func.inst(inst_id).kind.is_memory_object_op());
    if !needs_lowering {
        return false;
    }

    // unreachable definitions -> removed blocks and phi inputs
    let _ = super::cfg_simplify::remove_unreachable_blocks(func);
    let (views, length_reads) = local_views(func);
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
                    // A view whose uses all lower here becomes its length load, or nothing when
                    // no use reads the length; each use recomputes its data address from the
                    // object.
                    InstKind::MemorySlice(object)
                        if let Some(view) = builder.func().inst_result_value(inst)
                            && views.contains_key(&view) =>
                    {
                        if !length_reads.contains(&view) {
                            return false;
                        }
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
                    InstKind::MakeSlice { ptr, len, location }
                        if builder.func().value_ty(ptr) == Some(MirType::MemPtr) =>
                    {
                        // pointer = ptrtoint object to i256
                        // view = make_memory_slice pointer, len
                        let ptr = builder.cast(ptr, MirType::I256);
                        builder.func_mut().inst_mut(inst).kind =
                            InstKind::MakeSlice { ptr, len, location };
                    }
                    InstKind::SlicePtr(slice) if let Some(&object) = views.get(&slice) => {
                        // data = add (ptrtoint object), P::DYNAMIC_HEADER_SIZE
                        let header = builder.cast(object, MirType::I256);
                        let offset = builder.imm(P::DYNAMIC_HEADER_SIZE);
                        builder.func_mut().inst_mut(inst).kind = InstKind::Add(header, offset);
                    }
                    InstKind::SliceLoadByte { slice, index } => {
                        let Some(load) = slice_word_load::<P>(&mut builder, &views, slice, index)
                        else {
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
                    InstKind::SliceStoreByte { slice, index, value } => {
                        let base = slice_data::<P>(&mut builder, &views, slice);
                        let address = dynamic_offset_address(&mut builder, base, index);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore8(address, value);
                    }
                    InstKind::SliceStoreWord { slice, offset, value } => {
                        let address = slice_word_address::<P>(&mut builder, &views, slice, offset);
                        builder.func_mut().inst_mut(inst).kind = InstKind::MStore(address, value);
                    }
                    InstKind::SliceLoadWord { slice, offset } => {
                        let Some(kind) = slice_word_load::<P>(&mut builder, &views, slice, offset)
                        else {
                            return true;
                        };
                        builder.func_mut().inst_mut(inst).kind = kind;
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
            // The member each address belongs to, and whether it is an integer cast of the
            // member, which only addresses its header word.
            let mut owners = IndexVec::from_vec(vec![None; func.num_values()]);
            owners[first.result] = Some((0, false));
            let mut scan = position + 1;
            while scan < instructions.len() {
                let next_id = instructions[scan];
                if let Some(allocation) = constant_raw_allocation(func, next_id) {
                    if allocation.deferred_alloc != first.deferred_alloc {
                        break;
                    }
                    let index = allocations.len();
                    allocations.push((next_id, allocation, false));
                    owners[allocation.result] = Some((index, false));
                    scan += 1;
                    continue;
                }

                let member =
                    |value: crate::mir::ValueId| owners[value].filter(|&(_, header)| !header);
                let derived = match func.inst(next_id).kind {
                    InstKind::MStore(address, _) | InstKind::MStore8(address, _) => {
                        let Some((owner, _)) = owners[address] else { break };
                        allocations[owner].2 = true;
                        scan += 1;
                        continue;
                    }
                    InstKind::Add(lhs, rhs) if func.value_u64(rhs).is_some() => member(lhs),
                    InstKind::Add(lhs, rhs) if func.value_u64(lhs).is_some() => member(rhs),
                    InstKind::PtrToInt(base, 256) => member(base).map(|(owner, _)| (owner, true)),
                    _ => None,
                };
                let (Some(derived), Some(result)) = (derived, func.inst_result_value(next_id))
                else {
                    break;
                };
                owners[result] = Some(derived);
                scan += 1;
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

/// Returns the `memory_slice` views whose every use lowers in this pass, by their objects, and
/// the views whose length some use reads.
fn local_views(
    func: &Function,
) -> (FxHashMap<crate::mir::ValueId, crate::mir::ValueId>, FxHashSet<crate::mir::ValueId>) {
    let mut views: FxHashMap<_, _> = func
        .instructions()
        .filter_map(|inst| match func.inst(inst).kind {
            InstKind::MemorySlice(object) => Some((func.inst_result_value(inst)?, object)),
            _ => None,
        })
        .collect();
    let mut length_reads = FxHashSet::default();
    for inst in func.instructions() {
        let kind = &func.inst(inst).kind;
        if let InstKind::SliceLen(view) | InstKind::SliceCopy { source: view, .. } = *kind {
            length_reads.insert(view);
        }
        let local = matches!(
            kind,
            InstKind::SliceLen(_)
                | InstKind::SlicePtr(_)
                | InstKind::SliceLoadByte { .. }
                | InstKind::SliceStoreByte { .. }
                | InstKind::SliceStoreWord { .. }
                | InstKind::SliceLoadWord { .. }
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
    (views, length_reads)
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
    let Some(&object) = views.get(&slice) else {
        if let Value::Inst(inst) = *builder.func().value(slice)
            && let InstKind::MakeSlice { ptr, .. } = builder.func().inst(inst).kind
        {
            return builder.cast(ptr, MirType::I256);
        }
        return builder.slice_ptr(slice);
    };
    let header = builder.cast(object, MirType::I256);
    builder.add_u64_offset(header, P::DYNAMIC_HEADER_SIZE)
}

/// Returns the load of the word at a byte offset into a memory or calldata slice.
fn slice_word_load<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    slice: crate::mir::ValueId,
    offset: crate::mir::ValueId,
) -> Option<InstKind> {
    let location = slice_location(builder.func(), views, slice)?;
    let address = slice_word_address::<P>(builder, views, slice, offset);
    slice_load_kind(location, address)
}

/// Returns the address of the word at a byte offset into a slice. A local view at a constant
/// offset addresses its object directly, so its data address does not stay live.
fn slice_word_address<P: MemoryLayoutPolicy>(
    builder: &mut FunctionBuilder<'_>,
    views: &FxHashMap<crate::mir::ValueId, crate::mir::ValueId>,
    slice: crate::mir::ValueId,
    offset: crate::mir::ValueId,
) -> crate::mir::ValueId {
    if let Some(&object) = views.get(&slice)
        && let Some(offset) = builder
            .func()
            .value_u64(offset)
            .and_then(|offset| offset.checked_add(P::DYNAMIC_HEADER_SIZE))
    {
        // address = add (ptrtoint object), header + offset
        let header = builder.cast(object, MirType::I256);
        return builder.add_u64_offset(header, offset);
    }
    let base = slice_data::<P>(builder, views, slice);
    dynamic_offset_address(builder, base, offset)
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
            // `mload` and `mstore` accept a `memptr` address, which `scalar_types_match` allows;
            // cast it too, so every use of the object's address shares one stack value.
            let pointer_address = matches!(
                inst.kind,
                InstKind::MLoad(address) | InstKind::MStore(address, _)
                    if builder.func().value_ty(address) == Some(MirType::MemPtr)
            );
            if inst.kind.evm_opcode().is_some()
                && (pointer_address
                    || !inst.kind.scalar_types_match(builder.func(), inst.result_ty))
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
