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

use crate::{
    mir::{BlockId, Function, InstKind, Terminator},
    pass::FunctionPass,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

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

/// Function pass for jump threading.
pub struct JumpThreadingPass;

impl FunctionPass for JumpThreadingPass {
    fn name(&self) -> &str {
        "jump-threading"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        JumpThreader::new().run_to_fixpoint(func).total_threaded() != 0
    }
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
            let Some(mut term) = func.blocks[block_id].terminator.clone() else {
                continue;
            };
            self.thread_terminator(func, &mut term, final_targets);
            func.blocks[block_id].terminator = Some(term);
        }
    }

    /// Threads a single terminator's targets.
    fn thread_terminator(
        &mut self,
        func: &Function,
        term: &mut Terminator,
        final_targets: &FxHashMap<BlockId, BlockId>,
    ) {
        match term {
            Terminator::Jump(target) => {
                if let Some(final_target) = Self::threaded_target(func, *target, final_targets) {
                    *target = final_target;
                    self.stats.jumps_threaded += 1;
                    self.stats.gas_saved += 8;
                }
            }

            Terminator::Branch { then_block, else_block, .. } => {
                let mut changed = false;
                if let Some(final_target) = Self::threaded_target(func, *then_block, final_targets)
                {
                    *then_block = final_target;
                    changed = true;
                }
                if let Some(final_target) = Self::threaded_target(func, *else_block, final_targets)
                {
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
                if let Some(final_target) = Self::threaded_target(func, *default, final_targets) {
                    *default = final_target;
                    changed = true;
                }
                for (_, target) in cases.iter_mut() {
                    if let Some(final_target) = Self::threaded_target(func, *target, final_targets)
                    {
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
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => {}
        }
    }

    fn threaded_target(
        func: &Function,
        target: BlockId,
        final_targets: &FxHashMap<BlockId, BlockId>,
    ) -> Option<BlockId> {
        let final_target = final_targets.get(&target).copied()?;
        (!Self::block_has_phi(func, final_target)).then_some(final_target)
    }

    fn block_has_phi(func: &Function, block_id: BlockId) -> bool {
        func.blocks[block_id]
            .instructions
            .iter()
            .any(|&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
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
