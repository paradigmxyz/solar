//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Memory reads remain live when `msize` can observe
//! their expansion. Calls with missing bodies, recursive cycles, checks, or external termination
//! remain conservative; lowered multi-return buffer writes also prevent removal. Run on either
//! representation, with a fresh module summary shared across the function-local fixed points.

use crate::{
    analysis::{CfgInfo, MemoryCallSummaries, may_observe_msize},
    mir::{
        BlockId, Function, InstId, InstKind, Module, ValueId, utils::invalidate_unreachable_block,
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::bit_set::GrowableBitSet;
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
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let summaries = Arc::new(MemoryCallSummaries::new(module));
        run_function_pass(module, analyses, |func, _| {
            DeadCodeEliminator {
                call_summaries: Some(Arc::clone(&summaries)),
                ..Default::default()
            }
            .run_to_fixpoint(func)
                != 0
        })
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
    /// Number of instructions eliminated in the last run.
    eliminated_count: usize,
    /// Values used by instructions or terminators.
    used_values: GrowableBitSet<ValueId>,
    /// Dead instructions found in one iteration.
    dead: Vec<(BlockId, InstId)>,
}

impl DeadCodeEliminator {
    /// Creates a new dead code eliminator.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn run_once(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        // Phase 1: Remove unreachable blocks
        self.eliminated_count += self.eliminate_unreachable_blocks(func);

        // Phase 2: Find all used values
        self.collect_used_values(func);

        // Phase 3: Find dead instructions
        self.find_dead_instructions(func);

        // Remove dead instructions from blocks
        self.eliminated_count += self.dead.len();
        for &(block_id, inst_id) in &self.dead {
            let block = func.block_mut(block_id);
            block.instructions.retain(|&id| id != inst_id);
        }

        self.eliminated_count
    }

    /// Runs dead code elimination iteratively until no more changes.
    pub(crate) fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total_eliminated = 0;
        loop {
            let eliminated = self.run_once(func);
            if eliminated == 0 {
                break;
            }
            total_eliminated += eliminated;
        }
        total_eliminated
    }

    /// Eliminates unreachable blocks using CFG reachability analysis.
    fn eliminate_unreachable_blocks(&mut self, func: &mut Function) -> usize {
        let cfg = CfgInfo::new(func);

        // Collect unreachable block IDs
        let unreachable: Vec<BlockId> = func
            .blocks
            .iter_enumerated()
            .filter_map(|(id, _)| if !cfg.is_reachable(id) { Some(id) } else { None })
            .collect();

        // Clear unreachable blocks (we can't actually remove from IndexVec,
        // but we can clear their contents to prevent codegen)
        let mut changed = 0;
        for block_id in unreachable {
            // unreachable block -> invalid
            changed += usize::from(invalidate_unreachable_block(func, block_id));
        }
        changed
    }

    /// Collects all values that are used (appear in instructions or terminators).
    fn collect_used_values(&mut self, func: &Function) {
        self.used_values.clear();
        self.used_values.ensure(func.num_values());

        // Add values used in terminators
        for (_, block) in func.blocks.iter_enumerated() {
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    self.used_values.insert(operand);
                }
            }
        }

        // Add values used as operands in instructions
        for inst_id in func.instructions() {
            let inst = func.inst(inst_id);
            for val in inst.kind.operands() {
                self.used_values.insert(val);
            }
        }
    }

    /// Finds instructions that are dead (unused result, no side effects).
    fn find_dead_instructions(&mut self, func: &Function) {
        self.dead.clear();
        // A memory read can expand the EVM memory high-water mark, which a later `msize`
        // observes even when the loaded value is discarded. Keep reads in such functions; a
        // tighter path-sensitive proof is not worth risking a silent semantic change here.
        let observes_msize = may_observe_msize(func, self.call_summaries.as_deref());

        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);

                // Instructions with side effects are always kept.
                let discardable_call = !inst.metadata.abi_validation()
                    && matches!(&inst.kind, InstKind::ICall { function, .. }
                        if self.call_summaries.as_ref().and_then(|summaries| summaries.get(*function))
                            .is_some_and(|summary| summary.can_discard_call(observes_msize)));
                if inst.must_execute(observes_msize) && !discardable_call {
                    continue;
                }

                if func
                    .inst_result_value(inst_id)
                    .is_none_or(|result| !self.used_values.contains(result))
                {
                    self.dead.push((block_id, inst_id));
                }
            }
        }
    }
}
