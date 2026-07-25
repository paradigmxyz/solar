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
        BlockId, Function, InstKind, Module, Terminator, Value, ValueId,
        utils::repair_reachability_phis,
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec, map::FxHashMap};

/// Function pass for jump threading.
pub(crate) struct JumpThreading;

impl MirPass for JumpThreading {
    fn name(&self) -> &'static str {
        "jump-threading"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            JumpThreader::new().run_to_fixpoint(func).total_threaded() != 0
        })
    }
}

/// Statistics from jump threading optimization.
#[derive(Debug, Default, Clone)]
struct JumpThreadingStats {
    /// Number of unconditional jumps threaded.
    jumps_threaded: usize,
    /// Number of conditional branch targets threaded.
    branches_threaded: usize,
    /// Number of switch case targets threaded.
    switches_threaded: usize,
    /// Estimated gas saved (8 gas per eliminated jump).
    gas_saved: usize,
}

impl JumpThreadingStats {
    /// Returns the total number of threading operations performed.
    #[must_use]
    fn total_threaded(&self) -> usize {
        self.jumps_threaded + self.branches_threaded + self.switches_threaded
    }
}

/// Jump threading optimization pass.
#[derive(Debug, Default)]
struct JumpThreader {
    /// Statistics from the last run.
    stats: JumpThreadingStats,
}

impl JumpThreader {
    /// Creates a new jump threader.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Runs jump threading on a function.
    /// Returns the number of MIR mutations performed.
    fn run(&mut self, func: &mut Function) -> usize {
        self.stats = JumpThreadingStats::default();
        let mut changed = 0;

        // Build a map of blocks that are "forwarders" - blocks that only jump unconditionally
        let forwarders = self.find_forwarder_blocks(func);

        if !forwarders.is_empty() {
            // Resolve the final target for each forwarder (following chains)
            let mut final_targets = self.resolve_final_targets(&forwarders, func.blocks.len());
            final_targets.retain(|block, target| block != target && !func.block_has_phi(*target));

            // Update all terminators to use final targets
            self.thread_jumps(func, &final_targets);
            changed += self.stats.total_threaded();
        }

        changed += self.thread_phi_constant_edges(func);

        if changed == 0 {
            return 0;
        }

        changed += usize::from(repair_reachability_phis(func));

        changed
    }

    /// Runs jump threading iteratively until no more changes.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> JumpThreadingStats {
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
            if block.predecessors.is_empty() {
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
        block_count: usize,
    ) -> FxHashMap<BlockId, BlockId> {
        let mut final_targets = FxHashMap::default();
        let mut path = Vec::new();
        let mut positions = index_vec![usize::MAX; block_count];

        for &block_id in forwarders.keys() {
            if final_targets.contains_key(&block_id) {
                continue;
            }
            path.clear();
            let mut current = block_id;
            let final_target = loop {
                if let Some(&target) = final_targets.get(&current) {
                    break target;
                }
                if positions[current] != usize::MAX {
                    let cycle_start = positions[current];
                    for &cycle_block in &path[cycle_start..] {
                        final_targets.insert(cycle_block, cycle_block);
                    }
                    break current;
                }
                let Some(&next) = forwarders.get(&current) else {
                    break current;
                };
                positions[current] = path.len();
                path.push(current);
                current = next;
            };
            for &path_block in &path {
                final_targets.entry(path_block).or_insert(final_target);
                positions[path_block] = usize::MAX;
            }
        }

        final_targets
    }

    /// Updates all terminators to use the final targets.
    fn thread_jumps(&mut self, func: &mut Function, final_targets: &FxHashMap<BlockId, BlockId>) {
        for block in &mut func.blocks {
            let Some(term) = &mut block.terminator else {
                continue;
            };
            self.thread_terminator(term, final_targets);
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
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::TailCall { .. }
            | Terminator::Invalid => {}
        }
    }

    fn externally_used_phi_results(func: &Function) -> DenseBitSet<ValueId> {
        let mut owners = index_vec![BlockId::MAX; func.values.len()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !func.block_has_only_phis(block_id) {
                continue;
            }
            for &inst_id in &block.instructions {
                if let Some(value) = func.inst_result_value(inst_id) {
                    owners[value] = block_id;
                }
            }
        }

        let mut external = DenseBitSet::new_empty(func.values.len());
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                for operand in func.inst(inst_id).kind.operands() {
                    if owners[operand] != BlockId::MAX && owners[operand] != block_id {
                        external.insert(operand);
                    }
                }
            }
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    if owners[operand] != BlockId::MAX && owners[operand] != block_id {
                        external.insert(operand);
                    }
                }
            }
        }

        external
    }

    fn thread_phi_constant_edges(&mut self, func: &mut Function) -> usize {
        let mut rewrites = Vec::new();
        let externally_used_phis = Self::externally_used_phi_results(func);
        let mut phi_incoming = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !func.block_has_phi(block_id) || !func.block_has_only_phis(block_id) {
                continue;
            }
            for &inst_id in &block.instructions {
                let Some(result) = func.inst_result_value(inst_id) else { continue };
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
                for &(pred, value) in incoming {
                    phi_incoming.insert((block_id, result, pred), value);
                }
            }
        }

        for block_id in func.blocks.indices() {
            if !func.block_has_phi(block_id) || !func.block_has_only_phis(block_id) {
                continue;
            }
            if func.blocks[block_id]
                .instructions
                .iter()
                .filter_map(|&inst_id| func.inst_result_value(inst_id))
                .any(|value| externally_used_phis.contains(value))
            {
                continue;
            }

            let Some(term) = &func.blocks[block_id].terminator else {
                continue;
            };
            let predecessors = func.unique_predecessors(block_id);
            if predecessors.is_empty() {
                continue;
            }

            for pred in predecessors {
                if pred == block_id || Self::successor_count(func, pred, block_id) != 1 {
                    continue;
                }
                let Some(target) =
                    self.phi_constant_target_for_pred(func, block_id, term, pred, &phi_incoming)
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
        phi_incoming: &FxHashMap<(BlockId, ValueId, BlockId), ValueId>,
    ) -> Option<BlockId> {
        match term {
            Terminator::Branch { condition, then_block, else_block } => {
                let incoming =
                    Self::incoming_value_for_pred(func, block_id, *condition, pred, phi_incoming)?;
                let condition = func.value_u256(incoming)?;
                Some(if condition.is_zero() { *else_block } else { *then_block })
            }
            Terminator::Switch { value, default, cases } => {
                let incoming =
                    Self::incoming_value_for_pred(func, block_id, *value, pred, phi_incoming)?;
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
        phi_incoming: &FxHashMap<(BlockId, ValueId, BlockId), ValueId>,
    ) -> Option<ValueId> {
        let Value::Inst(_) = func.value(value) else {
            return Some(value);
        };
        phi_incoming.get(&(block_id, value, pred)).copied()
    }

    fn successor_count(func: &Function, pred: BlockId, target: BlockId) -> usize {
        match func.blocks[pred].terminator.as_ref() {
            Some(Terminator::Jump(successor)) => usize::from(*successor == target),
            Some(Terminator::Branch { then_block, else_block, .. }) => {
                usize::from(*then_block == target) + usize::from(*else_block == target)
            }
            Some(Terminator::Switch { default, cases, .. }) => {
                usize::from(*default == target)
                    + cases.iter().filter(|(_, successor)| *successor == target).count()
            }
            _ => 0,
        }
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
}
