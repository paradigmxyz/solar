//! Lower semantic memory objects and pointer arithmetic to physical word operations.
//!
//! `ptradd` becomes an integer `add` between pointer casts. Every memory access is already a
//! `mload`, `mstore`, or `mcopy` at an address, so nothing else changes here. An EVM opcode whose
//! operands do not match its types, as an integer `add` of a pointer, gets word operands and a word
//! result cast back to the pointer, and the casts this leaves unused are removed;
//! `lower-evm-shaped` later erases pointer types altogether. Unreachable blocks are removed first:
//! their definitions need not obey SSA and can contain cast cycles.
//!
//! A function whose signature carries a memory reference is marked as possibly returning memory.
//! Allocations stay as they are; `coalesce-allocs` fuses constant-size runs after slice lowering.
//!
//! This runs after SSA structs and mutable frame slots have been lowered, and rejects modules
//! that still have live SSA structs. The module stays semantic until the final conversion
//! verifies all backend representation requirements.
//! Unsupported layouts and address spaces retain their operations for the
//! subsequent phase checks.

use crate::mir::{
    Function, FunctionBuilder, InstId, InstKind, MirType, Module, Value, pass::MirPass,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
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
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function(func);
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
    let is_pointer = MirType::is_pointer;
    let needs_lowering = func.arg_indices().any(|index| is_pointer(func.arg_ty(index)))
        || func.return_components().iter().copied().any(is_pointer)
        || func.live_values().any(|value| func.value_ty(value).is_some_and(is_pointer))
        || func
            .instructions()
            .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::PtrAdd(..)));
    if !needs_lowering {
        return false;
    }

    // unreachable definitions -> removed blocks and phi inputs
    let _ = super::cfg_simplify::remove_unreachable_blocks(func);
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match builder.func().inst(inst).kind {
                InstKind::PtrAdd(ptr, offset) => {
                    // address = add (ptrtoint ptr), offset
                    // result = inttoptr address
                    let base = builder.cast(ptr, MirType::I256);
                    let address = builder.add(base, offset);
                    builder.func_mut().inst_mut(inst).kind = InstKind::IntToPtr(address);
                }
                ref kind
                    if kind.evm_opcode().is_some()
                        && !kind.scalar_types_match(
                            builder.func(),
                            builder.func().inst(inst).result_ty,
                        ) =>
                {
                    // scalar_operand = zext scalar_operand to i256
                    let mut kind = kind.clone();
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
                _ => {}
            }
            builder.func_mut().blocks[block].instructions.push(inst);
        }
    }

    fold_word_casts(func);

    func.attributes.may_return_memory |=
        func.params.iter().chain(func.return_components()).any(|ty| ty.is_memory_reference());
    normalize_pointer_operands(func);
    remove_unused_pointer_casts(func);
    true
}

/// Replaces each word cast of a pointer cast of a word with that word, so one value names it.
fn fold_word_casts(func: &mut Function) {
    // address = ptrtoint (inttoptr word) => word
    let replacements: FxHashMap<_, _> = func
        .instructions()
        .filter_map(|inst| {
            let InstKind::PtrToInt(ptr, 256) = func.inst(inst).kind else { return None };
            let Value::Inst(def) = *func.value(ptr) else { return None };
            let InstKind::IntToPtr(word) = func.inst(def).kind else { return None };
            Some((func.inst_result_value(inst)?, word))
        })
        .collect();
    func.replace_uses_canonicalized(&replacements);
}

/// Removes the pointer casts that folding address casts left without uses, as the `inttoptr` of a
/// lowered `ptradd` whose every user takes the address word.
fn remove_unused_pointer_casts(func: &mut Function) {
    let cast_operand = |func: &Function, inst: InstId| match func.inst(inst).kind {
        InstKind::IntToPtr(operand) | InstKind::PtrToInt(operand, _) => Some(operand),
        _ => None,
    };
    let mut uses = super::egraph::use_counts(func);
    let mut worklist = func
        .instructions()
        .filter(|&inst| {
            cast_operand(func, inst).is_some()
                && func.inst_result_value(inst).is_some_and(|value| uses[value] == 0)
        })
        .collect::<Vec<_>>();
    if worklist.is_empty() {
        return;
    }
    let mut dead = DenseBitSet::new_empty(func.num_insts());
    while let Some(inst) = worklist.pop() {
        dead.insert(inst);
        let operand = cast_operand(func, inst).expect("only casts are removed");
        uses[operand] -= 1;
        if uses[operand] == 0
            && let Value::Inst(def) = *func.value(operand)
            && cast_operand(func, def).is_some()
        {
            worklist.push(def);
        }
    }
    for block in func.blocks.iter_mut() {
        block.instructions.retain(|&inst| !dead.contains(inst));
    }
}

/// Gives an EVM opcode with a pointer operand, or whose operands or result do not match its types,
/// word operands, and a word result cast back to the pointer it stands for.
pub(super) fn normalize_pointer_operands(func: &mut Function) {
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for id in instructions {
            let inst = builder.func().inst(id).clone();
            // Late word passes match addresses as integer arithmetic, so a pointer address becomes
            // its word here too; `lower-evm-shaped` erases the remaining casts.
            let mut pointer_operand = false;
            inst.kind.visit_operands(|value| {
                pointer_operand |= builder.func().value_ty(value).is_some_and(MirType::is_pointer);
            });
            if inst.kind.evm_opcode().is_some()
                && (pointer_operand
                    || !inst.kind.scalar_types_match(builder.func(), inst.result_ty))
            {
                builder.set_debug_context(&inst.metadata);
                let mut kind = inst.kind;
                // integer_operand = ptrtoint pointer_operand to i256
                kind.visit_operands_mut(|value| *value = builder.cast(*value, MirType::I256));
                if inst.result_ty.is_some_and(MirType::is_pointer) {
                    // integer_result = opcode integer_operands
                    // pointer_result = inttoptr integer_result to ptr<memory>
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
