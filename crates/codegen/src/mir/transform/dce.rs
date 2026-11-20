//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Calls with missing bodies, recursive cycles,
//! checks, or external termination remain conservative; lowered multi-return buffer writes also
//! prevent removal. Run on either representation, sharing cached module summaries. A mark phase
//! from instructions that must execute and from terminator operands finds the live values in
//! linear time, so dead chains and dead phi cycles both go. An unused deferred allocation goes too:
//! it cannot fail, and the backend places it in a static region or with a bump. A module that
//! loads the pointer's slot at a constant address keeps them, since that load sees the bump.
//! Reachability is computed once.

use crate::mir::{
    AllocationSemantics, Callee, Function, InstId, InstKind, Module, PtrLocation, Value,
    analysis::MemoryCallSummaries,
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass_with_cfg},
    utils::invalidate_unreachable_blocks,
};
use solar_data_structures::bit_set::DenseBitSet;

/// Function pass for dead code elimination.
///
/// Empties unreachable blocks and removes instructions whose results are unused and whose
/// execution `InstKind::has_side_effects` does not require, together with internal calls whose
/// summaries allow discarding them.
pub(crate) struct Dce;

impl MirPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let summaries = analyses.call_summaries(module);
        let reads_fmp_slot = module.functions.iter().any(reads_fmp_slot);
        let changed = run_function_pass_with_cfg(module, analyses, |func, analyses| {
            invalidate_unreachable_blocks(func, analyses.cfg().reachable())
                + remove_dead_chains(func, &summaries, reads_fmp_slot)
                != 0
        });
        // Removing operations and unreachable blocks cannot add call effects.
        analyses.preserve_call_summaries();
        changed
    }

    fn preserves_debugify_sites(&self) -> bool {
        true
    }
}

fn remove_dead_chains(
    func: &mut Function,
    summaries: &MemoryCallSummaries,
    reads_fmp_slot: bool,
) -> usize {
    let mut discardable = DenseBitSet::new_empty(func.num_insts());
    let mut live = DenseBitSet::<InstId>::new_empty(func.num_insts());
    let mut pending = Vec::new();
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let discardable_call = matches!(&inst.kind, InstKind::ICall { function: Callee::Function(function), .. }
            if summaries.get(*function).is_some_and(|summary| summary.can_discard_call()));
        // A deferred allocation never fails, so it has no effect once nothing uses its pointer,
        // unless the backend lowers it to a bump that a raw read of the slot observes.
        let deferred_alloc = !reads_fmp_slot
            && matches!(
                inst.kind,
                InstKind::Alloc { semantics: AllocationSemantics::INTERNAL, .. }
            )
            && inst.metadata.deferred_alloc();
        if !inst.kind.has_side_effects() || discardable_call || deferred_alloc {
            discardable.insert(inst_id);
        } else if live.insert(inst_id) {
            pending.push(inst_id);
        }
    }

    let mut mark_operand = |func: &Function, operand, pending: &mut Vec<InstId>| {
        if let Value::Inst(producer) = *func.value(operand)
            && live.insert(producer)
        {
            pending.push(producer);
        }
    };
    for block in func.blocks.iter() {
        if let Some(term) = &block.terminator {
            term.visit_operands(|operand| mark_operand(func, operand, &mut pending));
        }
    }
    while let Some(inst) = pending.pop() {
        func.inst(inst).kind.visit_operands(|operand| mark_operand(func, operand, &mut pending));
    }

    // LLVM's dropLocation rule applies to discarded computations, not to their surviving users.
    // Keep that decision on the removed instruction for debugify; never transfer its location.
    for inst in discardable.iter() {
        if !live.contains(inst) {
            func.inst_mut(inst).metadata.drop_debug_loc(solar_evmir::debug::DebugLocKind::Dropped);
        }
    }

    let mut removed = 0;
    for block in &mut func.blocks {
        // unused, discardable instruction -> removed
        let len = block.instructions.len();
        block.instructions.retain(|&inst| live.contains(inst) || !discardable.contains(inst));
        removed += len - block.instructions.len();
    }
    removed
}

/// Whether `func` loads the free-memory pointer's slot at its constant address, as assembly does.
/// The loads that allocation lowering emits carry `alloc_ptr` and do not count.
fn reads_fmp_slot(func: &Function) -> bool {
    func.instructions().any(|inst| {
        matches!(func.inst(inst).kind, InstKind::Load(address, PtrLocation::Memory)
            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                && !func.inst(inst).metadata.alloc_ptr())
    })
}
