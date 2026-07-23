//! Dead Code Elimination (DCE) optimization pass.
//!
//! This pass removes MIR instructions whose results are never used and have no side effects.

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, InstId, Terminator, ValueId, utils::repair_reachability_phis},
    pass::FunctionPass,
};
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};

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
    /// Number of instructions eliminated in the last run.
    pub eliminated_count: usize,
    /// Scratch reused across runs: values used by instructions or terminators.
    used_values: GrowableBitSet<ValueId>,
    /// Scratch reused across runs: dead instructions found in one iteration.
    dead: Vec<(BlockId, InstId)>,
}

/// Function pass for dead code elimination.
#[derive(Default)]
pub(crate) struct DcePass {
    eliminator: DeadCodeEliminator,
}

impl FunctionPass for DcePass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = self.eliminator.run_to_fixpoint(func) != 0;
        repair_reachability_phis(func);
        changed
    }
}

impl DeadCodeEliminator {
    /// Creates a new dead code eliminator.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn run_with_inst_results(
        &mut self,
        func: &mut Function,
        inst_to_value: &FxHashMap<InstId, ValueId>,
    ) -> usize {
        self.eliminated_count = 0;

        // Phase 1: Remove unreachable blocks
        self.eliminate_unreachable_blocks(func);

        // Phase 2: Find all used values
        self.collect_used_values(func);

        // Phase 3: Find dead instructions
        self.find_dead_instructions(func, inst_to_value);

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
        let inst_to_value = func.inst_results();
        loop {
            let eliminated = self.run_with_inst_results(func, &inst_to_value);
            if eliminated == 0 {
                break;
            }
            total_eliminated += eliminated;
        }
        total_eliminated
    }

    /// Eliminates unreachable blocks using CFG reachability analysis.
    fn eliminate_unreachable_blocks(&mut self, func: &mut Function) {
        let cfg = CfgInfo::new(func);

        // Collect unreachable block IDs
        let unreachable: Vec<BlockId> = func
            .blocks
            .iter_enumerated()
            .filter_map(|(id, _)| if !cfg.is_reachable(id) { Some(id) } else { None })
            .collect();

        // Clear unreachable blocks (we can't actually remove from IndexVec,
        // but we can clear their contents to prevent codegen)
        for block_id in &unreachable {
            let block = func.block_mut(*block_id);
            block.instructions.clear();
            block.terminator = Some(Terminator::Invalid);
            block.predecessors.clear();
        }
    }

    /// Collects all values that are used (appear in instructions or terminators).
    fn collect_used_values(&mut self, func: &Function) {
        self.used_values.clear();
        self.used_values.ensure(func.values.len());

        // Add values used in terminators
        for (_, block) in func.blocks.iter_enumerated() {
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    self.used_values.insert(operand);
                }
            }
        }

        // Add values used as operands in instructions
        for (_, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                for val in inst.kind.operands() {
                    self.used_values.insert(val);
                }
            }
        }
    }

    /// Finds instructions that are dead (unused result, no side effects).
    fn find_dead_instructions(&mut self, func: &Function, inst_to_value: &FxHashMap<InstId, ValueId>) {
        self.dead.clear();

        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];

                // Instructions with side effects are always kept.
                if inst.kind.has_side_effects() {
                    continue;
                }

                // O(1) lookup via precomputed map (was O(V) linear scan).
                if let Some(&result) = inst_to_value.get(&inst_id)
                    && !self.used_values.contains(result)
                {
                    self.dead.push((block_id, inst_id));
                }
            }
        }
    }
}
