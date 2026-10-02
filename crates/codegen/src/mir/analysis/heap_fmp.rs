//! Free memory pointer facts that hold while inline assembly only grows the pointer.
//!
//! The free memory pointer starts at the heap floor in every ABI wrapper. These
//! helpers decide whether a function can only keep it in the heap from there, and
//! recognize a write that may overwrite its slot harmlessly: return data copied to
//! scratch right before a revert that returns it, as bubbling a failed call does.

use crate::mir::{
    BasicBlock, Callee, EffectKind, Function, FunctionId, InstId, InstKind, Module, Terminator,
    Value, ValueId, analysis::AliasAnalysis, memory::EvmMemoryLayout,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};
use std::cell::OnceCell;

/// Returns whether `func` itself keeps a heap free memory pointer in the heap.
///
/// Inline assembly can move the pointer into reserved memory, so every write that may
/// replace it must store a heap pointer or grow one; see [`heap_values`]. Callers
/// check internal calls themselves.
pub(crate) fn fmp_grows_in_heap(func: &Function) -> bool {
    let heap = OnceCell::new();
    let heap = |value| heap.get_or_init(|| heap_values(func)).contains(value);
    func.blocks.iter().all(|block| {
        block.instructions.iter().all(|&inst_id| {
            match func.inst(inst_id).kind {
                InstKind::MStore(address, value)
                    if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    heap(value)
                }
                InstKind::SetFmp(value) => heap(value),
                // Writes from the heap up never reach the reserved slot.
                InstKind::MStore(dest, _)
                | InstKind::MStore8(dest, _)
                | InstKind::MemoryZero(dest, _)
                | InstKind::MCopy(dest, _, _)
                | InstKind::CalldataCopy(dest, _, _)
                | InstKind::CodeCopy(dest, _, _)
                | InstKind::DataCopy(_, dest, _)
                | InstKind::ReturnDataCopy(dest, _, _)
                | InstKind::ExtCodeCopy(_, dest, _, _)
                    if heap(dest) =>
                {
                    true
                }
                // Bubbled return data may overwrite the slot, but only the revert reads it.
                InstKind::ReturnDataCopy(..)
                    if bubbled_return_data_copy(func, block) == Some(inst_id) =>
                {
                    true
                }
                // Mapping hashes write their key from the pointer up.
                InstKind::ICall { function: Callee::Function(_), .. }
                | InstKind::MappingSlotMemory(..)
                | InstKind::MappingSlotCalldata(..) => true,
                _ => !AliasAnalysis::instruction_may_reset_fmp_with_summaries(func, inst_id, None),
            }
        })
    })
}

/// Returns the values that are heap pointers or grow one.
///
/// This is a greatest fixpoint, so a pointer stepped through a loop stays derived
/// from its heap base while every other phi input is. A constant addend must fit in
/// 64 bits: rewrites fold subtractions into additions of wrapped constants.
///
/// NOTE: a heap pointer plus a runtime offset, as in an allocation bump `fmp + size`
/// or a write into an allocation, counts as staying in the heap. Only assembly that
/// adds a wrapped runtime value can move it below the heap that way.
fn heap_values(func: &Function) -> DenseBitSet<ValueId> {
    let mut heap = DenseBitSet::new_empty(func.num_values());
    let mut candidates = Vec::new();
    let mut seen = DenseBitSet::new_empty(func.num_values());
    for value in func.live_values().filter(|&value| seen.insert(value)) {
        if AliasAnalysis::pointer_lower_bound(func, value, 0)
            .is_some_and(|bound| bound >= EvmMemoryLayout::HEAP_START)
        {
            heap.insert(value);
        } else if let Value::Inst(inst) = func.value(value)
            && matches!(
                func.inst(*inst).kind,
                InstKind::Add(..)
                    | InstKind::PtrToInt(..)
                    | InstKind::IntToPtr(_)
                    | InstKind::Phi(_)
            )
        {
            heap.insert(value);
            candidates.push((value, *inst));
        }
    }
    let small = |value| func.value_u256(value).is_none_or(|value| value <= U256::from(u64::MAX));
    loop {
        let mut changed = false;
        for &(value, inst) in &candidates {
            let derived = match &func.inst(inst).kind {
                &InstKind::Add(a, b) => {
                    (heap.contains(a) && small(b)) || (heap.contains(b) && small(a))
                }
                &InstKind::PtrToInt(operand, _) | &InstKind::IntToPtr(operand) => {
                    heap.contains(operand)
                }
                InstKind::Phi(incoming) => incoming.iter().all(|&(_, value)| heap.contains(value)),
                _ => false,
            };
            if !derived && heap.remove(value) {
                changed = true;
            }
        }
        if !changed {
            return heap;
        }
    }
}

/// Returns the `returndatacopy` whose range `block` then reverts with, if any.
///
/// The block copies the latest call's return data and reverts with exactly that range,
/// as bubbling a failed call does, with no call that could replace the return data and
/// nothing after the copy but `returndatasize`. Sizes read in other blocks may follow
/// another call, so they must be the same value.
pub(crate) fn bubbled_return_data_copy(func: &Function, block: &BasicBlock) -> Option<InstId> {
    let Some(Terminator::Revert { offset, size }) = block.terminator else { return None };
    let same = |a: ValueId, b: ValueId| {
        a == b || func.value_u256(a).is_some_and(|value| func.value_u256(b) == Some(value))
    };
    // Two `returndatasize` reads agree when no call runs between them; the block has none.
    let returndatasize = |value| {
        matches!(func.value(value), Value::Inst(inst)
            if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize)
                && block.instructions.contains(inst))
    };
    let copies_range = |dest, len| {
        same(dest, offset) && (same(len, size) || (returndatasize(len) && returndatasize(size)))
    };
    let index = block.instructions.iter().position(|&inst| match func.inst(inst).kind {
        InstKind::ReturnDataCopy(dest, _, len) => copies_range(dest, len),
        _ => false,
    })?;
    let calls = block.instructions.iter().any(|&inst| {
        matches!(
            func.inst(inst).kind.effect_kind(),
            EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
        )
    });
    let tail = block.instructions[index + 1..]
        .iter()
        .all(|&inst| matches!(func.inst(inst).kind, InstKind::ReturnDataSize));
    (!calls && tail).then_some(block.instructions[index])
}

/// Returns the functions whose free memory pointer is always in the heap.
///
/// ABI wrappers start from the entry's heap floor, since they run only from the
/// dispatcher. Another function starts in the heap when every caller does and keeps
/// it there. Every such function must also keep the pointer in the heap, as must every
/// function it calls; see [`fmp_grows_in_heap`]. A tail call counts as a call when its
/// target may return to the caller's caller.
pub(crate) fn heap_fmp_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let is_root =
        |func: &Function| func.attributes.is_abi_wrapper && !func.attributes.is_constructor;
    let mut grows = DenseBitSet::new_empty(module.functions.len());
    // Before ABI lowering no function is a root.
    if !module.functions.iter().any(is_root) {
        return grows;
    }
    let returning = module.returning_functions();
    let mut callers = index_vec![Vec::new(); module.functions.len()];
    let mut calls = index_vec![Vec::new(); module.functions.len()];
    for (func_id, func) in module.functions.iter_enumerated() {
        if fmp_grows_in_heap(func) {
            grows.insert(func_id);
        }
        for inst in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(inst).kind
            {
                callers[callee].push(func_id);
                calls[func_id].push(callee);
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                callers[function].push(func_id);
                if returning.contains(function) {
                    calls[func_id].push(function);
                }
            }
        }
    }
    remove_until_fixpoint(&mut grows, |grows, func_id| {
        calls[func_id].iter().all(|&callee| grows.contains(callee))
    });
    let mut heap = grows;
    remove_until_fixpoint(&mut heap, |heap, func_id| {
        is_root(module.function(func_id))
            || (!callers[func_id].is_empty()
                && callers[func_id].iter().all(|&caller| heap.contains(caller)))
    });
    heap
}

/// Removes members of `set` that fail `keep` until every remaining member passes.
fn remove_until_fixpoint(
    set: &mut DenseBitSet<FunctionId>,
    keep: impl Fn(&DenseBitSet<FunctionId>, FunctionId) -> bool,
) {
    loop {
        let removed = set.iter().filter(|&func_id| !keep(set, func_id)).collect::<Vec<_>>();
        if removed.is_empty() {
            return;
        }
        for func_id in removed {
            set.remove(func_id);
        }
    }
}
