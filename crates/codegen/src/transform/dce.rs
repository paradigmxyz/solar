//! Dead Code Elimination (DCE) optimization pass.
//!
//! This pass removes MIR instructions whose results are never used and have no side effects.

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, InstId, Terminator, Value, ValueId, utils::repair_reachability_phis},
    pass::FunctionPass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

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
pub struct DeadCodeEliminator {
    /// Number of instructions eliminated in the last run.
    pub eliminated_count: usize,
    /// Number of unreachable blocks removed.
    pub blocks_removed: usize,
    /// Number of unused parameters detected.
    pub unused_params: usize,
}

/// Statistics for a DCE run.
#[derive(Debug, Default, Clone, Copy)]
pub struct DceStats {
    /// Instructions eliminated (unused results).
    pub dead_instructions: usize,
    /// Unreachable blocks removed.
    pub unreachable_blocks: usize,
    /// Unused function parameters detected.
    pub unused_parameters: usize,
}

/// Function pass for dead code elimination.
pub struct DcePass;

impl FunctionPass for DcePass {
    fn name(&self) -> &str {
        "dce"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = DeadCodeEliminator::new().run_to_fixpoint(func) != 0;
        repair_reachability_phis(func);
        changed
    }
}

impl DceStats {
    /// Returns total eliminations.
    pub fn total(&self) -> usize {
        self.dead_instructions + self.unreachable_blocks + self.unused_parameters
    }
}

impl DeadCodeEliminator {
    /// Creates a new dead code eliminator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs dead code elimination on a function.
    /// Returns the number of instructions eliminated.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;
        self.blocks_removed = 0;
        self.unused_params = 0;

        // Phase 1: Remove unreachable blocks
        self.eliminate_unreachable_blocks(func);

        // Phase 2: Find and count unused parameters
        self.unused_params = self.find_unused_parameters(func).len();

        // Phase 3: Precompute InstId → ValueId map (O(V) once, replaces
        // the O(V)-per-instruction linear scan in find_result_value).
        let inst_to_value: FxHashMap<InstId, ValueId> = func
            .values
            .iter_enumerated()
            .filter_map(
                |(vid, val)| {
                    if let Value::Inst(iid) = val { Some((*iid, vid)) } else { None }
                },
            )
            .collect();

        // Phase 4: Find all used values
        let used_values = self.collect_used_values(func);

        // Phase 5: Find dead instructions
        let dead_instructions = self.find_dead_instructions(func, &used_values, &inst_to_value);

        // Remove dead instructions from blocks
        for (block_id, inst_id) in &dead_instructions {
            let block = func.block_mut(*block_id);
            block.instructions.retain(|&id| id != *inst_id);
            self.eliminated_count += 1;
        }

        self.eliminated_count
    }

    /// Runs dead code elimination with full statistics.
    pub fn run_with_stats(&mut self, func: &mut Function) -> DceStats {
        let eliminated = self.run(func);
        DceStats {
            dead_instructions: eliminated,
            unreachable_blocks: self.blocks_removed,
            unused_parameters: self.unused_params,
        }
    }

    /// Runs dead code elimination iteratively until no more changes.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total_eliminated = 0;
        loop {
            let eliminated = self.run(func);
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
        let reachable = cfg.reachable();

        // Collect unreachable block IDs
        let unreachable: Vec<BlockId> = func
            .blocks
            .iter_enumerated()
            .filter_map(|(id, _)| if !reachable.contains(&id) { Some(id) } else { None })
            .collect();

        self.blocks_removed = unreachable.len();

        // Clear unreachable blocks (we can't actually remove from IndexVec,
        // but we can clear their contents to prevent codegen)
        for block_id in &unreachable {
            let block = func.block_mut(*block_id);
            block.instructions.clear();
            block.terminator = Some(Terminator::Invalid);
            block.predecessors.clear();
        }
    }

    /// Finds unused function parameters.
    /// Returns the indices of parameters that are never used.
    pub fn find_unused_parameters(&self, func: &Function) -> Vec<u32> {
        // Collect all used argument indices
        let mut used_args = FxHashSet::default();

        // Collect from all values used in instructions
        for (_, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                for val_id in inst.kind.operands() {
                    if let Value::Arg { index, .. } = &func.values[val_id] {
                        used_args.insert(*index);
                    }
                }
            }

            // Collect from terminators
            if let Some(ref term) = block.terminator {
                for val_id in self.get_terminator_operands(term) {
                    if let Value::Arg { index, .. } = &func.values[val_id] {
                        used_args.insert(*index);
                    }
                }
            }
        }

        // Find unused parameter indices
        (0..func.params.len() as u32).filter(|idx| !used_args.contains(idx)).collect()
    }

    /// Gets operands from a terminator.
    fn get_terminator_operands(&self, term: &Terminator) -> Vec<ValueId> {
        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => vec![],
            Terminator::Branch { condition, .. } => vec![*condition],
            Terminator::Switch { value, cases, .. } => {
                let mut ops = vec![*value];
                for (case_val, _) in cases {
                    ops.push(*case_val);
                }
                ops
            }
            Terminator::Return { values } => values.to_vec(),
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                vec![*offset, *size]
            }
            Terminator::SelfDestruct { recipient } => vec![*recipient],
            Terminator::TailCall { args, .. } => args.to_vec(),
        }
    }

    /// Collects all values that are used (appear in instructions or terminators).
    fn collect_used_values(&self, func: &Function) -> FxHashSet<ValueId> {
        let mut used = FxHashSet::default();

        // Add values used in terminators
        for (_, block) in func.blocks.iter_enumerated() {
            if let Some(term) = &block.terminator {
                self.collect_terminator_uses(term, &mut used);
            }
        }

        // Add values used as operands in instructions
        for (_, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                for val in inst.kind.operands() {
                    used.insert(val);
                }
            }
        }

        used
    }

    /// Collects values used by a terminator.
    fn collect_terminator_uses(&self, term: &Terminator, used: &mut FxHashSet<ValueId>) {
        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => {
                used.insert(*condition);
            }
            Terminator::Switch { value, cases, .. } => {
                used.insert(*value);
                for (case_val, _) in cases {
                    used.insert(*case_val);
                }
            }
            Terminator::Return { values } => {
                for val in values {
                    used.insert(*val);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                used.insert(*offset);
                used.insert(*size);
            }
            Terminator::SelfDestruct { recipient } => {
                used.insert(*recipient);
            }
            Terminator::TailCall { args, .. } => {
                for arg in args {
                    used.insert(*arg);
                }
            }
        }
    }

    /// Finds instructions that are dead (unused result, no side effects).
    fn find_dead_instructions(
        &self,
        func: &Function,
        used_values: &FxHashSet<ValueId>,
        inst_to_value: &FxHashMap<InstId, ValueId>,
    ) -> Vec<(BlockId, InstId)> {
        let mut dead = Vec::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];

                // Instructions with side effects are always kept.
                if inst.kind.has_side_effects() {
                    continue;
                }

                // O(1) lookup via precomputed map (was O(V) linear scan).
                if let Some(&result) = inst_to_value.get(&inst_id)
                    && !used_values.contains(&result)
                {
                    dead.push((block_id, inst_id));
                }
            }
        }

        dead
    }
}
