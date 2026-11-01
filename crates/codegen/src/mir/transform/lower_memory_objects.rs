//! Lower semantic memory objects and pointer arithmetic to physical word operations.
//!
//! `ptradd` becomes an integer `add` between pointer casts. Every memory access is already a
//! `mload`, `mstore`, or `mcopy` at an address, so nothing else changes here. An EVM opcode whose
//! operands do not match its types, as an integer `add` of a pointer, gets word operands and a word
//! result cast back to the pointer, and the casts this leaves unused are removed;
//! `lower-evm-shaped` later erases pointer types altogether. Unreachable blocks are removed first:
//! their definitions need not obey SSA and can contain cast cycles.
//!
//! Within a block, a run of constant-size internal allocations separated only by stores into them
//! and their address arithmetic becomes one allocation and constant offsets from it, provided
//! every member is written, so the group pays for one free-memory-pointer bump. `coalesce-allocs`
//! later fuses the wider groups this conservative scan leaves, once slice lowering has run. A
//! function whose signature carries a memory reference is marked as possibly returning memory.
//!
//! This runs after SSA structs and mutable frame slots have been lowered, and rejects modules
//! that still have live SSA structs. The module stays semantic until the final conversion
//! verifies all backend representation requirements.
//! Unsupported layouts and address spaces retain their operations for the
//! subsequent phase checks.

use super::utils::expand_block;
use crate::{
    backend::evm::select::InstKindOpcode,
    mir::{
        AllocationSemantics, Function, FunctionBuilder, Immediate, InstId, InstKind, MirType,
        Module, PtrLocation, Value,
        pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
    },
};
use alloy_primitives::U256;
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

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
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
        run_lowering_function_pass(module, analyses, |func, _| lower_function(func))
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
        let mut builder = FunctionBuilder::new(func);
        expand_block(&mut builder, block, |builder, inst| {
            let InstKind::PtrAdd(ptr, offset) = builder.func().inst(inst).kind else {
                return false;
            };
            // address = add (ptrtoint ptr), offset
            // result = inttoptr address
            let base = builder.cast(ptr, MirType::I256);
            let address = builder.add(base, offset);
            if let Value::Inst(add) = *builder.func().value(address) {
                builder.func_mut().inst_mut(add).metadata.set_checked_object_offset();
            }
            builder.func_mut().inst_mut(inst).kind = InstKind::IntToPtr(address);
            false
        });
    }

    fold_word_casts(func);

    func.attributes.may_return_memory |=
        func.params.iter().chain(func.return_components()).any(|ty| ty.is_memory_reference());
    coalesce_constant_allocations(func);
    normalize_pointer_operands(func);
    remove_unused_pointer_casts(func);
    true
}

/// Replaces each word cast of a pointer cast of a word with that word, so one value names it.
///
/// A pointer cast of a narrower integer keeps its casts: its result word is the zero-extended
/// integer, which `lower-evm-shaped` materializes.
fn fold_word_casts(func: &mut Function) {
    // address = ptrtoint (inttoptr word) => word
    let replacements: FxHashMap<_, _> = func
        .instructions()
        .filter_map(|inst| {
            let InstKind::PtrToInt(ptr, 256) = func.inst(inst).kind else { return None };
            let Value::Inst(def) = *func.value(ptr) else { return None };
            let InstKind::IntToPtr(word) = func.inst(def).kind else { return None };
            if func.value_ty(word) != Some(MirType::I256) {
                return None;
            }
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

/// Combines adjacent constant-size internal allocations before their first observable
/// operation. Lowering a call's literal arguments commonly emits one small
/// allocation per argument; one bump for the whole group keeps the same
/// disjoint ranges while removing repeated free-memory-pointer updates.
fn coalesce_constant_allocations(func: &mut Function) {
    // The member each address belongs to, and whether it is an integer cast of the member, which
    // only addresses its header word.
    let mut owners = FxHashMap::default();
    let mut allocations = Vec::new();
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut position = 0;
        while position < instructions.len() {
            let inst_id = instructions[position];
            let Some(first) = constant_allocation(func, inst_id) else {
                position += 1;
                continue;
            };

            allocations.clear();
            allocations.push((inst_id, first, false));
            owners.clear();
            owners.insert(first.result, (0, false));
            let mut scan = position + 1;
            while scan < instructions.len() {
                let next_id = instructions[scan];
                if let Some(allocation) = constant_allocation(func, next_id) {
                    if allocation.deferred_alloc != first.deferred_alloc {
                        break;
                    }
                    let index = allocations.len();
                    allocations.push((next_id, allocation, false));
                    owners.insert(allocation.result, (index, false));
                    scan += 1;
                    continue;
                }

                let member = |value: crate::mir::ValueId| {
                    owners.get(&value).copied().filter(|&(_, header)| !header)
                };
                let derived = match func.inst(next_id).kind {
                    InstKind::Store(address, .., PtrLocation::Memory)
                    | InstKind::Store8(address, ..) => {
                        let Some(&(owner, _)) = owners.get(&address) else { break };
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
                owners.insert(result, derived);
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
                    inst.kind = InstKind::Alloc { size, semantics: AllocationSemantics::INTERNAL };
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
        func.blocks[block].instructions = instructions;
    }
}

#[derive(Clone, Copy)]
struct ConstantAllocation {
    result: crate::mir::ValueId,
    size: u64,
    deferred_alloc: bool,
}

fn constant_allocation(func: &Function, inst_id: crate::mir::InstId) -> Option<ConstantAllocation> {
    let InstKind::Alloc { size, semantics } = func.inst(inst_id).kind else {
        return None;
    };
    if semantics != AllocationSemantics::INTERNAL {
        return None;
    }
    Some(ConstantAllocation {
        result: func.inst_result_value(inst_id)?,
        size: func.value_u64(size)?,
        deferred_alloc: func.inst(inst_id).metadata.deferred_alloc(),
    })
}

/// Gives an EVM opcode with a pointer operand, or whose operands or result do not match its types,
/// word operands, and a word result cast back to the pointer it stands for.
pub(super) fn normalize_pointer_operands(func: &mut Function) {
    for block in func.blocks.indices() {
        let mut builder = FunctionBuilder::new(func);
        expand_block(&mut builder, block, |builder, id| {
            let inst = builder.func().inst(id);
            // Late word passes match addresses as integer arithmetic, so a pointer address becomes
            // its word here too; `lower-evm-shaped` erases the remaining casts.
            let mut pointer_operand = false;
            inst.kind.visit_operands(|value| {
                pointer_operand |= builder.func().value_ty(value).is_some_and(MirType::is_pointer);
            });
            if inst.kind.evm_opcode().is_none()
                || (!pointer_operand
                    && inst.kind.scalar_types_match(builder.func(), inst.result_ty))
            {
                return false;
            }
            let inst = inst.clone();
            builder.set_debug_context(&inst.metadata);
            let mut kind = inst.kind;
            // integer_operand = ptrtoint pointer_operand to i256
            kind.visit_operands_mut(|value| *value = builder.cast(*value, MirType::I256));
            if inst.result_ty.is_some_and(MirType::is_pointer) {
                // integer_result = opcode integer_operands
                // pointer_result = inttoptr integer_result to ptr<memory>
                let value = builder.emit_inst(kind, Some(MirType::I256));
                if let Value::Inst(emitted) = *builder.func().value(value) {
                    builder.func_mut().inst_mut(emitted).metadata = inst.metadata;
                }
                let instruction = builder.func_mut().inst_mut(id);
                instruction.kind = InstKind::IntToPtr(value);
                instruction.metadata.set_effect(None);
                instruction.metadata.set_memory_region(None);
            } else {
                builder.func_mut().inst_mut(id).kind = kind;
            }
            false
        });
    }
}
