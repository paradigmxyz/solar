//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Memory reads remain live when `msize` can observe
//! their expansion. Calls with missing bodies, recursive cycles, checks, or external termination
//! remain conservative; lowered multi-return buffer writes also prevent removal. Run on either
//! representation, sharing cached module summaries. A use-count worklist removes dead chains in
//! linear time; cyclic uses remain live. Reachability is computed once, and memory reads are
//! reconsidered only when deleting the last `msize` observer makes them discardable.

use crate::mir::{
    Function, InstId, InstKind, Module, Value, ValueId,
    analysis::{CfgInfo, MemoryCallSummaries, may_observe_msize},
    pass::{MirPass, run_function_pass},
    utils::invalidate_unreachable_block,
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
    ) -> solar_interface::Result<bool> {
        let summaries = analyses.call_summaries(module);
        Ok(run_function_pass(module, analyses, |func, _| {
            DeadCodeEliminator { call_summaries: Some(Arc::clone(&summaries)) }
                .run_to_fixpoint(func)
                != 0
        }))
    }
}

/// Dead Code Elimination pass.
///
/// Removes instructions that:
/// 1. Have a result that is never used
/// 2. Have no side effects
/// 3. Are in unreachable blocks
/// 4. Are instructions after a terminator (unreachable code)
///
/// Side-effect instructions (SSTORE, MSTORE, CALL, LOG, etc.) are always kept.
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
        let mut removed = 0;
        for block in func.blocks.indices() {
            if !cfg.is_reachable(block) {
                // unreachable block -> invalid
                removed += usize::from(invalidate_unreachable_block(func, block));
            }
        }

        let observes_msize = may_observe_msize(func, self.call_summaries.as_deref());
        removed += self.remove_dead_chains(func, observes_msize);
        if observes_msize && !may_observe_msize(func, self.call_summaries.as_deref()) {
            removed += self.remove_dead_chains(func, false);
        }
        removed
    }

    fn remove_dead_chains(&self, func: &mut Function, observes_msize: bool) -> usize {
        let mut uses = IndexVec::<ValueId, usize>::from_vec(vec![0; func.num_values()]);
        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    uses[operand] += 1;
                }
            }
        }
        for inst in func.instructions() {
            for operand in func.inst(inst).kind.operands() {
                uses[operand] += 1;
            }
        }

        let mut discardable = DenseBitSet::new_empty(func.num_insts());
        let mut pending = Vec::new();
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            let discardable_call = matches!(&inst.kind, InstKind::ICall { function, .. }
                if self.call_summaries.as_ref().and_then(|summaries| summaries.get(*function))
                    .is_some_and(|summary| summary.can_discard_call(observes_msize)));
            if !inst.must_execute(observes_msize) || discardable_call {
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
            for operand in func.inst(inst).kind.operands() {
                uses[operand] -= 1;
                if uses[operand] == 0
                    && let Value::Inst(producer) = *func.value(operand)
                    && discardable.contains(producer)
                {
                    pending.push(producer);
                }
            }
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
