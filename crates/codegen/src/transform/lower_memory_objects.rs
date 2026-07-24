//! Lower semantic memory-object operations to physical word operations.

use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{AllocationKind, Function, FunctionBuilder, InstKind, MirPhase, MirType, Module, Value},
    pass::MirPass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
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
        for func in module.functions_mut() {
            changed |= lower_function::<EvmMemoryLayout>(func, &mut stats);
        }
        if module.phase == MirPhase::Dispatch {
            module.advance_phase(MirPhase::MemoryLowered);
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
    let has_objects = func.params.iter().chain(&func.returns).any(is_object_type)
        || func.values().any(|value| match value {
            Value::Arg { ty, .. } | Value::Undef(ty) => is_object_type(ty),
            Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => false,
        })
        || func.instructions().any(|inst| {
            inst.result_ty.as_ref().is_some_and(is_object_type)
                || matches!(
                    inst.kind,
                    InstKind::MemoryObjectLen(_, _)
                        | InstKind::SetMemoryObjectLen(_, _, _)
                        | InstKind::MemoryObjectData(_, _)
                        | InstKind::MemoryObjectFieldAddr { .. }
                        | InstKind::MemoryObjectElementAddr { .. }
                        | InstKind::Keccak256Bytes(_)
                        | InstKind::Alloc { kind: AllocationKind::Object(_), .. }
                )
        });
    if !has_objects {
        return false;
    }

    let inst_results = func.inst_results();
    let mut replacements = FxHashMap::default();
    let mut removed = FxHashSet::default();
    let blocks: Vec<_> = func.block_ids().collect();

    for block in blocks {
        let instructions = std::mem::take(&mut func.block_mut(block).instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let kind = builder.func().instruction(inst).kind.clone();
            match kind {
                InstKind::Alloc { size, kind: AllocationKind::Object(_), semantics } => {
                    let instruction = builder.func_mut().instruction_mut(inst);
                    instruction.kind =
                        InstKind::Alloc { size, kind: AllocationKind::Raw, semantics };
                    stats.allocations += 1;
                }
                InstKind::MemoryObjectLen(object, kind) => {
                    let Some(offset) = P::object_length_offset(kind) else {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().instruction_mut(inst).kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::SetMemoryObjectLen(object, len, kind) => {
                    let Some(offset) = P::object_length_offset(kind) else {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().instruction_mut(inst).kind = InstKind::MStore(address, len);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectData(object, kind) => {
                    let offset = P::object_data_offset(kind);
                    if offset == 0 {
                        if let Some(&result) = inst_results.get(&inst) {
                            replacements.insert(result, object);
                        }
                        removed.insert(inst);
                    } else {
                        let offset = builder.imm_u64(offset);
                        builder.func_mut().instruction_mut(inst).kind =
                            InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                    let Some(offset) = P::field_offset(layout, field) else {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    };
                    if offset == 0 {
                        if let Some(&result) = inst_results.get(&inst) {
                            replacements.insert(result, object);
                        }
                        removed.insert(inst);
                    } else {
                        let offset = builder.imm_u64(offset);
                        builder.func_mut().instruction_mut(inst).kind =
                            InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::Keccak256Bytes(object) => {
                    let kind = crate::mir::MemoryObjectKind::Bytes;
                    let Some(length_offset) = P::object_length_offset(kind) else {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    };
                    let length_address = offset_address(&mut builder, object, length_offset);
                    let len = builder.mload(length_address);
                    let data = offset_address(&mut builder, object, P::object_data_offset(kind));
                    builder.func_mut().instruction_mut(inst).kind = InstKind::Keccak256(data, len);
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectElementAddr { object, layout, index } => {
                    let Some(stride) = P::element_stride(layout) else {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    };
                    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
                    let base =
                        offset_address(&mut builder, object, P::object_data_offset(layout.kind()));
                    let stride = builder.imm_u64(stride);
                    let offset = builder.mul(index, stride);
                    builder.func_mut().instruction_mut(inst).kind = InstKind::Add(base, offset);
                    stats.accesses += 1;
                }
                _ => {}
            }
            if !removed.contains(&inst) {
                builder.func_mut().block_mut(block).instructions.push(inst);
            }
        }
    }

    if !replacements.is_empty() {
        func.replace_uses_canonicalized(&replacements);
    }
    erase_object_types(func, stats);
    true
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

fn erase_object_types(func: &mut Function, stats: &mut LowerMemoryObjectsStats) {
    for ty in func.params.iter_mut().chain(&mut func.returns) {
        erase_object_type(ty, stats);
    }
    for value in func.values_mut() {
        match value {
            Value::Arg { ty, .. } | Value::Undef(ty) => erase_object_type(ty, stats),
            Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => {}
        }
    }
    for inst in func.instructions_mut() {
        if let Some(ty) = &mut inst.result_ty {
            erase_object_type(ty, stats);
        }
    }
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
