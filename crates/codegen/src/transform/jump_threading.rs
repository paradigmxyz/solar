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
    mir::{
        BlockId, Function, InstKind, Terminator, Value, ValueId, utils::repair_reachability_phis,
    },
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
        let mut changed = 0;

        // Build a map of blocks that are "forwarders" - blocks that only jump unconditionally
        let forwarders = self.find_forwarder_blocks(func);

        if !forwarders.is_empty() {
            // Resolve the final target for each forwarder (following chains)
            let final_targets = self.resolve_final_targets(&forwarders);

            // Update all terminators to use final targets
            self.thread_jumps(func, &final_targets);
            changed += self.stats.total_threaded();
        }

        changed += self.thread_phi_constant_edges(func);

        if changed == 0 {
            return 0;
        }

        // Update predecessor/successor information
        self.update_cfg_edges(func);
        repair_reachability_phis(func);

        changed
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

            // Only fully empty blocks are forwarders: bypassing a block that
            // contains a phi would sever the phi's incoming edges.
            if !block.instructions.is_empty() {
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
            | Terminator::TailCall { .. }
            | Terminator::Invalid => {}
        }
    }

    fn threaded_target(
        func: &Function,
        target: BlockId,
        final_targets: &FxHashMap<BlockId, BlockId>,
    ) -> Option<BlockId> {
        let final_target = final_targets.get(&target).copied()?;
        (!func.block_has_phi(final_target)).then_some(final_target)
    }

    fn block_phi_results_have_external_uses(func: &Function, block_id: BlockId) -> bool {
        let phi_results = func.block_phi_results(block_id);
        if phi_results.is_empty() {
            return false;
        }

        for (other_block, block) in func.blocks.iter_enumerated() {
            if other_block != block_id {
                for &inst_id in &block.instructions {
                    if func.instructions[inst_id]
                        .kind
                        .operands()
                        .iter()
                        .any(|operand| phi_results.contains(operand))
                    {
                        return true;
                    }
                }
            }

            if other_block == block_id {
                continue;
            }
            if let Some(term) = &block.terminator
                && term.operands().iter().any(|operand| phi_results.contains(operand))
            {
                return true;
            }
        }

        false
    }

    fn thread_phi_constant_edges(&mut self, func: &mut Function) -> usize {
        let mut rewrites = Vec::new();
        let block_ids: Vec<_> = func.blocks.indices().collect();

        for block_id in block_ids {
            if !func.block_has_only_phis(block_id) {
                continue;
            }
            if Self::block_phi_results_have_external_uses(func, block_id) {
                continue;
            }

            let Some(term) = &func.blocks[block_id].terminator else {
                continue;
            };
            let predecessors = func.blocks[block_id].predecessors.clone();
            if predecessors.is_empty() {
                continue;
            }

            for pred in predecessors {
                if pred == block_id || Self::successor_count(func, pred, block_id) != 1 {
                    continue;
                }
                let Some(target) = self.phi_constant_target_for_pred(func, block_id, term, pred)
                else {
                    continue;
                };
                if target == block_id || func.block_has_phi(target) {
                    continue;
                }
                rewrites.push((pred, block_id, target));
            }
        }

        let mut threaded = 0;
        for (pred, old_target, new_target) in rewrites {
            if Self::replace_successor(func, pred, old_target, new_target) {
                threaded += 1;
            }
        }

        if threaded != 0 {
            self.stats.branches_threaded += threaded;
            self.stats.gas_saved += threaded * 8;
        }

        threaded
    }

    fn phi_constant_target_for_pred(
        &self,
        func: &Function,
        block_id: BlockId,
        term: &Terminator,
        pred: BlockId,
    ) -> Option<BlockId> {
        match term {
            Terminator::Branch { condition, then_block, else_block } => {
                let incoming = Self::incoming_value_for_pred(func, block_id, *condition, pred)?;
                let condition = func.value_u256(incoming)?;
                Some(if condition.is_zero() { *else_block } else { *then_block })
            }
            Terminator::Switch { value, default, cases } => {
                let incoming = Self::incoming_value_for_pred(func, block_id, *value, pred)?;
                let value = func.value_u256(incoming)?;
                for (case, target) in cases {
                    if func.value_u256(*case)? == value {
                        return Some(*target);
                    }
                }
                Some(*default)
            }
            _ => None,
        }
    }

    fn incoming_value_for_pred(
        func: &Function,
        block_id: BlockId,
        value: ValueId,
        pred: BlockId,
    ) -> Option<ValueId> {
        let Value::Inst(inst_id) = func.value(value) else {
            return Some(value);
        };
        if !func.blocks[block_id].instructions.contains(inst_id) {
            return None;
        }
        let InstKind::Phi(incoming) = &func.instructions[*inst_id].kind else {
            return None;
        };
        incoming.iter().find_map(|(incoming_block, incoming_value)| {
            (*incoming_block == pred).then_some(*incoming_value)
        })
    }

    fn successor_count(func: &Function, pred: BlockId, target: BlockId) -> usize {
        func.blocks[pred]
            .terminator
            .as_ref()
            .map(|term| term.successors().into_iter().filter(|&succ| succ == target).count())
            .unwrap_or_default()
    }

    fn replace_successor(
        func: &mut Function,
        pred: BlockId,
        old_target: BlockId,
        new_target: BlockId,
    ) -> bool {
        let Some(term) = &mut func.blocks[pred].terminator else {
            return false;
        };
        match term {
            Terminator::Jump(target) => {
                if *target == old_target {
                    *target = new_target;
                    true
                } else {
                    false
                }
            }
            Terminator::Branch { then_block, else_block, .. } => {
                let mut changed = false;
                if *then_block == old_target {
                    *then_block = new_target;
                    changed = true;
                }
                if *else_block == old_target {
                    *else_block = new_target;
                    changed = true;
                }
                changed
            }
            Terminator::Switch { default, cases, .. } => {
                let mut changed = false;
                if *default == old_target {
                    *default = new_target;
                    changed = true;
                }
                for (_, target) in cases {
                    if *target == old_target {
                        *target = new_target;
                        changed = true;
                    }
                }
                changed
            }
            Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::TailCall { .. }
            | Terminator::Invalid => false,
        }
    }

    /// Updates CFG edges after threading.
    fn update_cfg_edges(&self, func: &mut Function) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in &block_ids {
            func.blocks[*block_id].predecessors.clear();
        }

        for block_id in block_ids {
            let successors = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(|t| t.successors())
                .unwrap_or_default();

            for succ in successors {
                func.blocks[succ].predecessors.push(block_id);
            }
        }
    }
}
