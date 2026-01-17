//! Jump Threading optimization pass.
//!
//! This pass eliminates unnecessary jumps by threading through blocks that only contain
//! an unconditional jump. Each eliminated JUMP instruction saves 8 gas.
//!
//! ## Optimizations performed:
//!
//! 1. **JUMP to JUMP threading**: If block A jumps to block B, and B only contains an unconditional
//!    jump to C, rewrite A to jump directly to C.
//!
//! 2. **JUMPI to JUMP threading**: If a conditional branch targets a block that only contains an
//!    unconditional jump, thread through to the final target.
//!
//! 3. **Empty block elimination**: Blocks containing only a JUMPDEST and JUMP are eliminated by
//!    updating all references to point to the final target.

use crate::mir::{BlockId, Function, Terminator};
use rustc_hash::{FxHashMap, FxHashSet};

/// Statistics from jump threading optimization.
#[derive(Debug, Default, Clone)]
pub struct JumpThreadingStats {
    /// Number of unconditional jumps threaded.
    pub jumps_threaded: usize,
    /// Number of conditional branch targets threaded.
    pub branches_threaded: usize,
    /// Number of switch case targets threaded.
    pub switches_threaded: usize,
    /// Estimated gas saved (8 gas per eliminated jump).
    pub gas_saved: usize,
}

impl JumpThreadingStats {
    /// Returns the total number of threading operations performed.
    #[must_use]
    pub fn total_threaded(&self) -> usize {
        self.jumps_threaded + self.branches_threaded + self.switches_threaded
    }
}

/// Jump threading optimization pass.
#[derive(Debug, Default)]
pub struct JumpThreader {
    /// Statistics from the last run.
    pub stats: JumpThreadingStats,
}

impl JumpThreader {
    /// Creates a new jump threader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs jump threading on a function.
    /// Returns the number of threading operations performed.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.stats = JumpThreadingStats::default();

        // Build a map of blocks that are "forwarders" - blocks that only jump unconditionally
        let forwarders = self.find_forwarder_blocks(func);

        if forwarders.is_empty() {
            return 0;
        }

        // Resolve the final target for each forwarder (following chains)
        let final_targets = self.resolve_final_targets(&forwarders);

        // Update all terminators to use final targets
        self.thread_jumps(func, &final_targets);

        // Update predecessor/successor information
        self.update_cfg_edges(func);

        self.stats.total_threaded()
    }

    /// Runs jump threading iteratively until no more changes.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> JumpThreadingStats {
        let mut total_stats = JumpThreadingStats::default();
        loop {
            let changed = self.run(func);
            if changed == 0 {
                break;
            }
            total_stats.jumps_threaded += self.stats.jumps_threaded;
            total_stats.branches_threaded += self.stats.branches_threaded;
            total_stats.switches_threaded += self.stats.switches_threaded;
            total_stats.gas_saved += self.stats.gas_saved;
        }
        total_stats
    }

    /// Finds blocks that only contain an unconditional jump (forwarder blocks).
    fn find_forwarder_blocks(&self, func: &Function) -> FxHashMap<BlockId, BlockId> {
        let mut forwarders = FxHashMap::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            // Skip the entry block
            if block_id == func.entry_block {
                continue;
            }

            // Check if block has no real instructions (phi nodes don't count)
            let has_real_instructions = block.instructions.iter().any(|&inst_id| {
                !matches!(func.instructions[inst_id].kind, crate::mir::InstKind::Phi(_))
            });

            if has_real_instructions {
                continue;
            }

            // Check if terminator is an unconditional jump
            if let Some(Terminator::Jump(target)) = &block.terminator {
                // Don't thread self-loops
                if *target != block_id {
                    forwarders.insert(block_id, *target);
                }
            }
        }

        forwarders
    }

    /// Resolves chains of forwarders to find the final target.
    fn resolve_final_targets(
        &self,
        forwarders: &FxHashMap<BlockId, BlockId>,
    ) -> FxHashMap<BlockId, BlockId> {
        let mut final_targets = FxHashMap::default();

        for &block_id in forwarders.keys() {
            let final_target = self.follow_chain(block_id, forwarders);
            if final_target != block_id {
                final_targets.insert(block_id, final_target);
            }
        }

        final_targets
    }

    /// Follows a chain of forwarders to find the final non-forwarder target.
    fn follow_chain(&self, start: BlockId, forwarders: &FxHashMap<BlockId, BlockId>) -> BlockId {
        let mut visited = FxHashSet::default();
        let mut current = start;

        while let Some(&next) = forwarders.get(&current) {
            if !visited.insert(current) {
                break;
            }
            current = next;
        }

        current
    }

    /// Updates all terminators to use the final targets.
    fn thread_jumps(&mut self, func: &mut Function, final_targets: &FxHashMap<BlockId, BlockId>) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in block_ids {
            if let Some(term) = &mut func.blocks[block_id].terminator {
                self.thread_terminator(term, final_targets);
            }
        }
    }

    /// Threads a single terminator's targets.
    fn thread_terminator(
        &mut self,
        term: &mut Terminator,
        final_targets: &FxHashMap<BlockId, BlockId>,
    ) {
        match term {
            Terminator::Jump(target) => {
                if let Some(&final_target) = final_targets.get(target) {
                    *target = final_target;
                    self.stats.jumps_threaded += 1;
                    self.stats.gas_saved += 8;
                }
            }

            Terminator::Branch { then_block, else_block, .. } => {
                let mut changed = false;
                if let Some(&final_target) = final_targets.get(then_block) {
                    *then_block = final_target;
                    changed = true;
                }
                if let Some(&final_target) = final_targets.get(else_block) {
                    *else_block = final_target;
                    changed = true;
                }
                if changed {
                    self.stats.branches_threaded += 1;
                    self.stats.gas_saved += 8;
                }
            }

            Terminator::Switch { default, cases, .. } => {
                let mut changed = false;
                if let Some(&final_target) = final_targets.get(default) {
                    *default = final_target;
                    changed = true;
                }
                for (_, target) in cases.iter_mut() {
                    if let Some(&final_target) = final_targets.get(target) {
                        *target = final_target;
                        changed = true;
                    }
                }
                if changed {
                    self.stats.switches_threaded += 1;
                    self.stats.gas_saved += 8;
                }
            }

            Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => {}
        }
    }

    /// Updates CFG edges after threading.
    fn update_cfg_edges(&self, func: &mut Function) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in &block_ids {
            func.blocks[*block_id].predecessors.clear();
            func.blocks[*block_id].successors.clear();
        }

        for block_id in block_ids {
            let successors = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(|t| t.successors())
                .unwrap_or_default();

            for succ in successors {
                func.blocks[block_id].successors.push(succ);
                func.blocks[succ].predecessors.push(block_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, FunctionBuilder};
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    fn get_jump_target(func: &Function, block_id: BlockId) -> Option<BlockId> {
        match &func.blocks[block_id].terminator {
            Some(Terminator::Jump(target)) => Some(*target),
            _ => None,
        }
    }

    fn get_branch_targets(func: &Function, block_id: BlockId) -> Option<(BlockId, BlockId)> {
        match &func.blocks[block_id].terminator {
            Some(Terminator::Branch { then_block, else_block, .. }) => {
                Some((*then_block, *else_block))
            }
            _ => None,
        }
    }

    #[test]
    fn test_simple_jump_threading() {
        let mut func = make_test_func();
        let bb1 = func.alloc_block();
        let bb2 = func.alloc_block();

        func.blocks[func.entry_block].terminator = Some(Terminator::Jump(bb1));
        func.blocks[func.entry_block].successors.push(bb1);
        func.blocks[bb1].predecessors.push(func.entry_block);

        func.blocks[bb1].terminator = Some(Terminator::Jump(bb2));
        func.blocks[bb1].successors.push(bb2);
        func.blocks[bb2].predecessors.push(bb1);

        func.blocks[bb2].terminator = Some(Terminator::Stop);

        let mut threader = JumpThreader::new();
        let changed = threader.run(&mut func);

        assert_eq!(changed, 1);
        assert_eq!(threader.stats.jumps_threaded, 1);
        assert_eq!(threader.stats.gas_saved, 8);
        assert_eq!(get_jump_target(&func, func.entry_block), Some(bb2));
    }

    #[test]
    fn test_chain_threading() {
        let mut func = make_test_func();
        let bb1 = func.alloc_block();
        let bb2 = func.alloc_block();
        let bb3 = func.alloc_block();

        func.blocks[func.entry_block].terminator = Some(Terminator::Jump(bb1));
        func.blocks[func.entry_block].successors.push(bb1);
        func.blocks[bb1].predecessors.push(func.entry_block);

        func.blocks[bb1].terminator = Some(Terminator::Jump(bb2));
        func.blocks[bb1].successors.push(bb2);
        func.blocks[bb2].predecessors.push(bb1);

        func.blocks[bb2].terminator = Some(Terminator::Jump(bb3));
        func.blocks[bb2].successors.push(bb3);
        func.blocks[bb3].predecessors.push(bb2);

        func.blocks[bb3].terminator = Some(Terminator::Stop);

        let mut threader = JumpThreader::new();
        let stats = threader.run_to_fixpoint(&mut func);

        assert_eq!(get_jump_target(&func, func.entry_block), Some(bb3));
        assert!(stats.total_threaded() > 0);
    }

    #[test]
    fn test_branch_threading() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let cond = builder.imm_u64(1);
        let bb1 = builder.create_block();
        let bb2 = builder.create_block();
        let bb3 = builder.create_block();
        let bb4 = builder.create_block();

        builder.branch(cond, bb1, bb2);

        builder.switch_to_block(bb1);
        builder.jump(bb3);

        builder.switch_to_block(bb2);
        builder.jump(bb4);

        builder.switch_to_block(bb3);
        builder.stop();

        builder.switch_to_block(bb4);
        builder.stop();

        drop(builder);

        let entry = func.entry_block;
        let mut threader = JumpThreader::new();
        threader.run(&mut func);

        let (then_target, else_target) = get_branch_targets(&func, entry).unwrap();
        assert_eq!(then_target, bb3);
        assert_eq!(else_target, bb4);
    }

    #[test]
    fn test_no_self_loop_threading() {
        let mut func = make_test_func();
        let bb1 = func.alloc_block();

        func.blocks[func.entry_block].terminator = Some(Terminator::Jump(bb1));
        func.blocks[bb1].terminator = Some(Terminator::Jump(bb1));

        let mut threader = JumpThreader::new();
        let changed = threader.run(&mut func);

        assert_eq!(changed, 0);
    }

    #[test]
    fn test_block_with_instructions_not_threaded() {
        let mut func = make_test_func();
        let mut builder = FunctionBuilder::new(&mut func);

        let bb1 = builder.create_block();
        let bb2 = builder.create_block();

        builder.jump(bb1);

        builder.switch_to_block(bb1);
        let slot = builder.imm_u64(0);
        let _val = builder.sload(slot);
        builder.jump(bb2);

        builder.switch_to_block(bb2);
        builder.stop();

        drop(builder);

        let mut threader = JumpThreader::new();
        let changed = threader.run(&mut func);

        assert_eq!(changed, 0);
    }
}
