//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Calls with missing bodies, recursive cycles,
//! checks, or external termination remain conservative; lowered multi-return buffer writes also
//! prevent removal. Run on either representation, sharing cached module summaries. A use-count
//! worklist removes dead chains in linear time; cyclic uses remain live. Reachability is computed
//! once.

use crate::mir::{
    Callee, Function, InstId, InstKind, Module, Value, ValueId,
    analysis::{CfgInfo, MemoryCallSummaries},
    pass::{MirPass, run_function_pass_with_cfg},
    utils::invalidate_unreachable_blocks,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use std::sync::Arc;

/// Function pass for dead code elimination.
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
            DeadCodeEliminator { call_summaries: Some(Arc::clone(&summaries)) }
                .run_with_cfg(func, analyses.cfg())
                != 0
        });
        // Removing operations and unreachable blocks cannot add call effects.
        analyses.preserve_call_summaries();
        changed
    }
}

/// Dead Code Elimination pass.
///
/// Empties unreachable blocks and removes instructions whose results are unused and whose
/// execution `InstKind::has_side_effects` does not require, together with internal calls whose
/// summaries allow discarding them.
#[derive(Debug, Default)]
pub(crate) struct DeadCodeEliminator {
    call_summaries: Option<Arc<MemoryCallSummaries>>,
}

impl DeadCodeEliminator {
    /// Creates a new dead code eliminator.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Removes unreachable blocks and unused instructions to a fixed point.
    pub(crate) fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let cfg = CfgInfo::new(func);
        self.run_with_cfg(func, &cfg)
    }

    fn run_with_cfg(&mut self, func: &mut Function, cfg: &CfgInfo) -> usize {
        invalidate_unreachable_blocks(func, cfg.reachable()) + self.remove_dead_chains(func)
    }

    fn remove_dead_chains(&self, func: &mut Function) -> usize {
        let mut uses = IndexVec::<ValueId, usize>::from_vec(vec![0; func.num_values()]);
        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                term.visit_operands(|operand| {
                    uses[operand] += 1;
                });
            }
        }
        for inst in func.instructions() {
            func.inst(inst).kind.visit_operands(|operand| {
                uses[operand] += 1;
            });
        }

        let mut discardable = DenseBitSet::new_empty(func.num_insts());
        let mut pending = Vec::new();
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            let discardable_call = matches!(&inst.kind, InstKind::ICall { function: Callee::Function(function), .. }
                if self.call_summaries.as_ref().and_then(|summaries| summaries.get(*function))
                    .is_some_and(|summary| summary.can_discard_call()));
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
}
