//! Dead Code Elimination (DCE) optimization pass.
//!
//! This pass removes MIR instructions whose results are never used and have no side effects.

use crate::{
    analysis::CfgInfo,
    mir::{
        BlockId, Function, InstId, Module, Terminator, Value, ValueId, utils::remove_predecessors,
    },
    pass::{MirPass, run_function_pass_no_analyses},
};
use solar_data_structures::{
    bit_set::GrowableBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};

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
        let mut eliminator = DeadCodeEliminator::new();
        run_function_pass_no_analyses(module, analyses, |func| {
            eliminator.run_to_fixpoint(func) != 0
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
#[derive(Debug)]
pub(crate) struct DeadCodeEliminator {
    /// Number of instructions eliminated in the last run.
    eliminated_count: usize,
    /// Number of active uses of each value.
    use_counts: IndexVec<ValueId, usize>,
    /// Instructions whose result just became unused.
    worklist: Vec<InstId>,
    /// Dead instructions found in one run.
    dead: GrowableBitSet<InstId>,
}

impl DeadCodeEliminator {
    /// Creates a new dead code eliminator.
    pub(crate) fn new() -> Self {
        Self {
            eliminated_count: 0,
            use_counts: IndexVec::new(),
            worklist: Vec::new(),
            dead: GrowableBitSet::new_empty(),
        }
    }

    fn run_once(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        // Phase 1: Remove unreachable blocks.
        let unreachable_removed = self.eliminate_unreachable_blocks(func);
        self.eliminated_count += unreachable_removed;

        // Phase 2: Count uses, then propagate liveness losses backwards from
        // initially-unused results.
        self.collect_use_counts(func);
        self.find_dead_instructions(func);

        // Remove dead instructions from blocks.
        self.eliminated_count += self.dead.count();
        if !self.dead.is_empty() {
            for block in &mut func.blocks {
                block.instructions.retain(|&inst_id| !self.dead.contains(inst_id));
            }
        }

        self.eliminated_count
    }

    /// Runs dead code elimination to a fixed point.
    pub(crate) fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        self.run_once(func)
    }

    /// Eliminates unreachable blocks using CFG reachability analysis.
    fn eliminate_unreachable_blocks(&mut self, func: &mut Function) -> usize {
        if func.blocks.len() == 1 {
            return 0;
        }
        let cfg = CfgInfo::new(func);

        // Collect unreachable block IDs
        let unreachable: Vec<BlockId> = func
            .blocks
            .iter_enumerated()
            .filter_map(|(id, _)| if !cfg.is_reachable(id) { Some(id) } else { None })
            .collect();

        let mut removed_predecessors = FxHashMap::default();
        for &block_id in &unreachable {
            let successors = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(Terminator::successors)
                .unwrap_or_default();
            for successor in successors {
                removed_predecessors
                    .entry(successor)
                    .or_insert_with(FxHashSet::default)
                    .insert(block_id);
            }
        }
        for (successor, removed) in removed_predecessors {
            remove_predecessors(func, successor, &removed);
        }

        // Clear unreachable blocks (we can't actually remove from IndexVec,
        // but we can clear their contents to prevent codegen)
        let mut changed = 0;
        for block_id in unreachable {
            let block = func.block_mut(block_id);
            changed += usize::from(
                !block.instructions.is_empty()
                    || !matches!(block.terminator, Some(Terminator::Invalid))
                    || !block.predecessors.is_empty(),
            );
            block.instructions.clear();
            block.terminator = Some(Terminator::Invalid);
            block.predecessors.clear();
        }
        changed
    }

    /// Counts all active instruction and terminator uses.
    fn collect_use_counts(&mut self, func: &Function) {
        self.use_counts.clear();
        self.use_counts.resize(func.values.len(), 0);

        for block in &func.blocks {
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    self.use_counts[operand] += 1;
                }
            }
            for &inst_id in &block.instructions {
                for operand in func.inst(inst_id).kind.operands() {
                    self.use_counts[operand] += 1;
                }
            }
        }
    }

    /// Finds dead instructions and propagates each removed use to its operands.
    fn find_dead_instructions(&mut self, func: &Function) {
        self.dead.truncate(0);
        self.worklist.clear();

        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);
                if let Some(result) = func.inst_result_value(inst_id)
                    && self.use_counts[result] == 0
                    && !inst.kind.has_side_effects()
                {
                    self.worklist.push(inst_id);
                }
            }
        }

        while let Some(inst_id) = self.worklist.pop() {
            if !self.dead.insert(inst_id) {
                continue;
            }
            for operand in func.inst(inst_id).kind.operands() {
                debug_assert_ne!(self.use_counts[operand], 0);
                self.use_counts[operand] -= 1;
                if self.use_counts[operand] == 0
                    && let Value::Inst(def) = func.value(operand)
                    && !func.inst(*def).kind.has_side_effects()
                {
                    self.worklist.push(*def);
                }
            }
        }
    }
}
