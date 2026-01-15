//! Dead Code Elimination (DCE) optimization pass.
//!
//! This pass removes MIR instructions whose results are never used and have no side effects.

use crate::mir::{BlockId, Function, InstId, Terminator, Value, ValueId};
use rustc_hash::FxHashSet;

/// Dead Code Elimination pass.
///
/// Removes instructions that:
/// 1. Have a result that is never used
/// 2. Have no side effects
///
/// Side-effect instructions (SSTORE, MSTORE, CALL, LOG, etc.) are always kept.
#[derive(Debug, Default)]
pub struct DeadCodeEliminator {
    /// Number of instructions eliminated in the last run.
    pub eliminated_count: usize,
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

        // Find all used values by collecting values referenced in terminators and instructions
        let used_values = self.collect_used_values(func);

        // Find dead instructions
        let dead_instructions = self.find_dead_instructions(func, &used_values);

        // Remove dead instructions from blocks
        for (block_id, inst_id) in &dead_instructions {
            let block = func.block_mut(*block_id);
            block.instructions.retain(|&id| id != *inst_id);
            self.eliminated_count += 1;
        }

        self.eliminated_count
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
                    used.insert(*val);
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
            Terminator::Revert { offset, size } => {
                used.insert(*offset);
                used.insert(*size);
            }
            Terminator::SelfDestruct { recipient } => {
                used.insert(*recipient);
            }
        }
    }

    /// Finds instructions that are dead (unused result, no side effects).
    fn find_dead_instructions(
        &self,
        func: &Function,
        used_values: &FxHashSet<ValueId>,
    ) -> Vec<(BlockId, InstId)> {
        let mut dead = Vec::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];

                // Instructions with side effects are always kept
                if inst.kind.has_side_effects() {
                    continue;
                }

                // Find the result value for this instruction
                let result_value = self.find_result_value(func, inst_id);

                // If the instruction has a result and it's not used, mark as dead
                if let Some(result) = result_value {
                    if !used_values.contains(&result) {
                        dead.push((block_id, inst_id));
                    }
                }
            }
        }

        dead
    }

    /// Finds the ValueId that represents the result of an instruction.
    fn find_result_value(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        // Search through values to find one that references this instruction
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(id) = value {
                if *id == inst_id {
                    return Some(value_id);
                }
            }
        }
        None
    }
}

