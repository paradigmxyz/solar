//! Dead Code Elimination (DCE) optimization pass.
//!
//! This pass removes MIR instructions whose results are never used and have no side effects.

use crate::mir::{BlockId, Function, InstId, Terminator, Value, ValueId};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

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

        // Phase 3: Find all used values
        let used_values = self.collect_used_values(func);

        // Phase 4: Find dead instructions
        let dead_instructions = self.find_dead_instructions(func, &used_values);

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
        let reachable = self.find_reachable_blocks(func);

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
            block.successors.clear();
        }

        // Update successor/predecessor lists of reachable blocks
        for block_id in &reachable {
            let block = func.block_mut(*block_id);
            block.successors.retain(|succ| reachable.contains(succ));
        }
    }

    /// Finds all reachable blocks from the entry using BFS.
    fn find_reachable_blocks(&self, func: &Function) -> FxHashSet<BlockId> {
        let mut reachable = FxHashSet::default();
        let mut worklist = VecDeque::new();

        worklist.push_back(func.entry_block);
        reachable.insert(func.entry_block);

        while let Some(block_id) = worklist.pop_front() {
            let block = func.block(block_id);

            // Add successors from terminator
            if let Some(ref term) = block.terminator {
                for succ in term.successors() {
                    if reachable.insert(succ) {
                        worklist.push_back(succ);
                    }
                }
            }
        }

        reachable
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
            Terminator::Revert { offset, size } => vec![*offset, *size],
            Terminator::SelfDestruct { recipient } => vec![*recipient],
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
                if let Some(result) = result_value
                    && !used_values.contains(&result)
                {
                    dead.push((block_id, inst_id));
                }
            }
        }

        dead
    }

    /// Finds the ValueId that represents the result of an instruction.
    fn find_result_value(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        // Search through values to find one that references this instruction
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(id) = value
                && *id == inst_id
            {
                return Some(value_id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use solar_interface::Ident;

    fn make_test_func() -> crate::mir::Function {
        crate::mir::Function::new(Ident::DUMMY)
    }

    #[test]
    fn test_dce_eliminates_unused_instruction() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // Create values that are never used
        let v0 = builder.imm_u64(10);
        let v1 = builder.imm_u64(20);
        let _unused = builder.add(v0, v1); // This should be eliminated

        // Create a value that IS used
        let v2 = builder.imm_u64(42);
        builder.ret([v2]);

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run(&mut func);

        assert_eq!(eliminated, 1, "Should eliminate one unused ADD instruction");
    }

    #[test]
    fn test_dce_keeps_side_effect_instructions() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let slot = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.sstore(slot, value); // Side-effect, should be kept
        builder.stop();

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run(&mut func);

        assert_eq!(eliminated, 0, "Should not eliminate SSTORE");
        assert_eq!(func.block(func.entry_block).instructions.len(), 1);
    }

    #[test]
    fn test_dce_unreachable_block_elimination() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // Entry block jumps directly to exit
        let exit_block = builder.create_block();
        let unreachable_block = builder.create_block();

        // Entry jumps to exit (unreachable_block is never reached)
        builder.jump(exit_block);

        // Unreachable block with some code
        builder.switch_to_block(unreachable_block);
        let v0 = builder.imm_u64(100);
        let v1 = builder.imm_u64(200);
        let _sum = builder.add(v0, v1);
        builder.stop();

        // Exit block
        builder.switch_to_block(exit_block);
        builder.stop();

        let mut dce = DeadCodeEliminator::new();
        let stats = dce.run_with_stats(&mut func);

        assert_eq!(stats.unreachable_blocks, 1, "Should detect one unreachable block");
        // The unreachable block's instructions should be cleared
        assert!(
            func.block(unreachable_block).instructions.is_empty(),
            "Unreachable block should have no instructions"
        );
    }

    #[test]
    fn test_dce_unused_parameter_detection() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // Add 3 parameters, only use the second one
        let _param0 = builder.add_param(MirType::uint256()); // Unused
        let param1 = builder.add_param(MirType::uint256()); // Used
        let _param2 = builder.add_param(MirType::uint256()); // Unused

        builder.ret([param1]);

        let mut dce = DeadCodeEliminator::new();
        let stats = dce.run_with_stats(&mut func);

        assert_eq!(stats.unused_parameters, 2, "Should detect 2 unused parameters");
    }

    #[test]
    fn test_dce_all_params_used() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let param0 = builder.add_param(MirType::uint256());
        let param1 = builder.add_param(MirType::uint256());
        let sum = builder.add(param0, param1);
        builder.ret([sum]);

        let mut dce = DeadCodeEliminator::new();
        let stats = dce.run_with_stats(&mut func);

        assert_eq!(stats.unused_parameters, 0, "All parameters are used");
    }

    #[test]
    fn test_dce_chain_elimination() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // Create a chain: v2 = v0 + v1, v3 = v2 * 2 (all unused)
        let v0 = builder.imm_u64(10);
        let v1 = builder.imm_u64(20);
        let v2 = builder.add(v0, v1);
        let two = builder.imm_u64(2);
        let _v3 = builder.mul(v2, two);

        let ret_val = builder.imm_u64(0);
        builder.ret([ret_val]);

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run_to_fixpoint(&mut func);

        // Both add and mul should be eliminated
        assert_eq!(eliminated, 2, "Should eliminate entire dead chain");
    }

    #[test]
    fn test_dce_preserves_used_chain() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let v0 = builder.imm_u64(10);
        let v1 = builder.imm_u64(20);
        let v2 = builder.add(v0, v1);
        let two = builder.imm_u64(2);
        let v3 = builder.mul(v2, two);
        builder.ret([v3]);

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run(&mut func);

        assert_eq!(eliminated, 0, "Should not eliminate used chain");
    }

    #[test]
    fn test_dce_branch_both_sides_reachable() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let cond = builder.imm_bool(true);
        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let merge_block = builder.create_block();

        builder.branch(cond, then_block, else_block);

        // Then block
        builder.switch_to_block(then_block);
        builder.jump(merge_block);

        // Else block
        builder.switch_to_block(else_block);
        builder.jump(merge_block);

        // Merge block
        builder.switch_to_block(merge_block);
        builder.stop();

        let mut dce = DeadCodeEliminator::new();
        let stats = dce.run_with_stats(&mut func);

        assert_eq!(stats.unreachable_blocks, 0, "All blocks should be reachable");
    }

    #[test]
    fn test_dce_diamond_cfg() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // Diamond: entry -> (then, else) -> merge
        let cond = builder.imm_bool(true);
        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let merge_block = builder.create_block();
        let unreachable = builder.create_block(); // Never referenced

        builder.branch(cond, then_block, else_block);

        builder.switch_to_block(then_block);
        let v1 = builder.imm_u64(1);
        builder.ret([v1]);

        builder.switch_to_block(else_block);
        let v2 = builder.imm_u64(2);
        builder.ret([v2]);

        builder.switch_to_block(merge_block);
        builder.stop();

        builder.switch_to_block(unreachable);
        builder.stop();

        let mut dce = DeadCodeEliminator::new();
        let stats = dce.run_with_stats(&mut func);

        // merge_block and unreachable are not reachable since then/else both return
        assert_eq!(stats.unreachable_blocks, 2, "Should detect 2 unreachable blocks");
    }

    #[test]
    fn test_dce_keeps_log_instructions() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let offset = builder.imm_u64(0);
        let size = builder.imm_u64(32);
        builder.log0(offset, size);
        builder.stop();

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run(&mut func);

        assert_eq!(eliminated, 0, "Should not eliminate LOG0");
    }

    #[test]
    fn test_dce_fixpoint_complex() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        // v0 = 1
        // v1 = 2
        // v2 = v0 + v1 (dead after v3 is eliminated)
        // v3 = v2 * 3 (dead after v4 is eliminated)
        // v4 = v3 - 1 (dead, unused)
        // return 42

        let v0 = builder.imm_u64(1);
        let v1 = builder.imm_u64(2);
        let v2 = builder.add(v0, v1);
        let three = builder.imm_u64(3);
        let v3 = builder.mul(v2, three);
        let one = builder.imm_u64(1);
        let _v4 = builder.sub(v3, one);

        let ret = builder.imm_u64(42);
        builder.ret([ret]);

        let initial_inst_count = func.block(func.entry_block).instructions.len();
        assert_eq!(initial_inst_count, 3, "Should have 3 instructions initially");

        let mut dce = DeadCodeEliminator::new();
        let eliminated = dce.run_to_fixpoint(&mut func);

        assert_eq!(eliminated, 3, "Should eliminate all 3 dead instructions");
        assert!(func.block(func.entry_block).instructions.is_empty());
    }

    #[test]
    fn test_dce_param_used_in_terminator() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let param = builder.add_param(MirType::uint256());
        builder.ret([param]); // param used in terminator

        let dce = DeadCodeEliminator::new();
        let unused = dce.find_unused_parameters(&func);

        assert!(unused.is_empty(), "Parameter used in terminator should not be unused");
    }

    #[test]
    fn test_dce_param_used_in_branch_condition() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let param = builder.add_param(MirType::Bool);
        let then_block = builder.create_block();
        let else_block = builder.create_block();

        builder.branch(param, then_block, else_block);

        builder.switch_to_block(then_block);
        builder.stop();

        builder.switch_to_block(else_block);
        builder.stop();

        let dce = DeadCodeEliminator::new();
        let unused = dce.find_unused_parameters(&func);

        assert!(unused.is_empty(), "Parameter used in branch should not be unused");
    }
}
