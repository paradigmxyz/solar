//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Calls with missing bodies, recursive cycles,
//! checks, or external termination remain conservative; lowered multi-return buffer writes also
//! prevent removal. Run on either representation, sharing cached module summaries. A use-count
//! worklist removes dead chains in linear time; cyclic uses remain live. Reachability is computed
//! once.

use super::egraph::use_counts;
use crate::mir::{
    Callee, Function, InstId, InstKind, Module, Value,
    analysis::MemoryCallSummaries,
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
        let changed = run_function_pass_with_cfg(module, analyses, |func, analyses| {
            invalidate_unreachable_blocks(func, analyses.cfg().reachable())
                + remove_dead_chains(func, &summaries)
                != 0
        });
        // Removing operations and unreachable blocks cannot add call effects.
        analyses.preserve_call_summaries();
        changed
    }
}

fn remove_dead_chains(func: &mut Function, summaries: &MemoryCallSummaries) -> usize {
    let mut uses = use_counts(func);

    let mut discardable = DenseBitSet::new_empty(func.num_insts());
    let mut pending = Vec::new();
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let discardable_call = matches!(&inst.kind, InstKind::ICall { function: Callee::Function(function), .. }
            if summaries.get(*function).is_some_and(|summary| summary.can_discard_call()));
        if !inst.kind.has_side_effects() || discardable_call {
            discardable.insert(inst_id);
            if inst.result().is_none_or(|result| uses[result] == 0) {
                pending.push(inst_id);
            }
        }
    }

    let mut dead = DenseBitSet::<InstId>::new_empty(func.num_insts());
    while let Some(inst) = pending.pop() {
        if !dead.insert(inst) {
            continue;
        }
        func.inst(inst).kind.visit_operands(|operand| {
            uses[operand] -= 1;
            if uses[operand] == 0
                && let Value::Inst(producer) = *func.value(operand)
                && discardable.contains(producer)
            {
                pending.push(producer);
            }
        });
    }
    if !dead.is_empty() {
        for block in &mut func.blocks {
            // unused, discardable instruction -> removed
            block.instructions.retain(|&inst| !dead.contains(inst));
        }
    }
    dead.count()
}
