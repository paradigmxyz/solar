//! Lower semantic memory-object operations to physical word operations.

use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{AllocationKind, Function, FunctionBuilder, InstKind, MirPhase, MirType, Module, Value},
    pass::ModulePass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

/// Statistics from semantic memory-object lowering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LowerMemoryObjectsStats {
    /// Object allocations changed to raw physical allocations.
    pub allocations: usize,
    /// Semantic accesses expanded or erased.
    pub accesses: usize,
    /// Nominal object types erased to physical pointers.
    pub types: usize,
}

/// Lowers semantic object layouts under the selected physical memory policy.
#[derive(Debug, Default)]
pub(crate) struct LowerMemoryObjectsPass {
    stats: LowerMemoryObjectsStats,
}

impl LowerMemoryObjectsPass {}

impl ModulePass for LowerMemoryObjectsPass {
    fn run(&mut self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        self.stats = LowerMemoryObjectsStats::default();
        if module.phase >= MirPhase::MemoryLowered {
            return false;
        }
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function::<EvmMemoryLayout>(func, &mut self.stats);
        }
        if module.phase == MirPhase::Dispatch {
            module.advance_phase(MirPhase::MemoryLowered);
        }
        changed
    }
}

fn lower_function<P: MemoryLayoutPolicy>(
    func: &mut Function,
    stats: &mut LowerMemoryObjectsStats,
) -> bool {
    let has_objects = func.params.iter().chain(&func.returns).any(is_object_type)
        || func.values.iter().any(|value| match value {
            Value::Arg { ty, .. } | Value::Undef(ty) => is_object_type(ty),
            Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => false,
        })
        || func.instructions.iter().any(|inst| {
            inst.result_ty.as_ref().is_some_and(is_object_type)
                || matches!(
                    inst.kind,
                    InstKind::MemoryObjectLen(_, _)
                        | InstKind::SetMemoryObjectLen(_, _, _)
                        | InstKind::MemoryObjectData(_, _)
                        | InstKind::MemoryObjectFieldAddr { .. }
                        | InstKind::MemoryObjectElementAddr { .. }
                        | InstKind::Alloc { kind: AllocationKind::Object(_), .. }
                )
        });
    if !has_objects {
        return false;
    }

    let inst_results = func.inst_results();
    let mut replacements = FxHashMap::default();
    let mut removed = FxHashSet::default();
    let blocks: Vec<_> = func.blocks.indices().collect();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let kind = builder.func().instructions[inst].kind.clone();
            match kind {
                InstKind::Alloc { size, kind: AllocationKind::Object(_), semantics } => {
                    let instruction = &mut builder.func_mut().instructions[inst];
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
                    builder.func_mut().instructions[inst].kind = InstKind::MLoad(address);
                    stats.accesses += 1;
                }
                InstKind::SetMemoryObjectLen(object, len, kind) => {
                    let Some(offset) = P::object_length_offset(kind) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    let address = offset_address(&mut builder, object, offset);
                    builder.func_mut().instructions[inst].kind = InstKind::MStore(address, len);
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
                        builder.func_mut().instructions[inst].kind = InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                    let Some(offset) = P::field_offset(layout, field) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    if offset == 0 {
                        if let Some(&result) = inst_results.get(&inst) {
                            replacements.insert(result, object);
                        }
                        removed.insert(inst);
                    } else {
                        let offset = builder.imm_u64(offset);
                        builder.func_mut().instructions[inst].kind = InstKind::Add(object, offset);
                    }
                    stats.accesses += 1;
                }
                InstKind::MemoryObjectElementAddr { object, layout, index } => {
                    let Some(stride) = P::element_stride(layout) else {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    };
                    debug_assert!(stride.is_multiple_of(P::WORD_SIZE));
                    let base =
                        offset_address(&mut builder, object, P::object_data_offset(layout.kind()));
                    let stride = builder.imm_u64(stride);
                    let offset = builder.mul(index, stride);
                    builder.func_mut().instructions[inst].kind = InstKind::Add(base, offset);
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
    for value in func.values.iter_mut() {
        match value {
            Value::Arg { ty, .. } | Value::Undef(ty) => erase_object_type(ty, stats),
            Value::Inst(_) | Value::Immediate(_) | Value::Error(_) => {}
        }
    }
    for inst in func.instructions.iter_mut() {
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
