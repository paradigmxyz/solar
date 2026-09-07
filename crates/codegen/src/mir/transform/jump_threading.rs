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
//!
//! Threading can leave a phi-only branch block with one predecessor. When that predecessor
//! jumps unconditionally and no phi result escapes, move the branch onto the predecessor and
//! substitute the selected inputs. This exposes nested short-circuit checks within the same
//! fixpoint, without waiting for another CFG cleanup pass. Targets with phis and cyclic phi
//! inputs stay unchanged.

use crate::mir::{
    BlockId, Function, InstKind, Module, Terminator, Value, ValueId,
    pass::{MirPass, run_function_pass},
    utils::replace_terminator,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

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
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> solar_interface::Result<bool> {
        Ok(run_function_pass(module, analyses, |func, _| {
            JumpThreader::new().run_to_fixpoint(func).total_threaded() != 0
        }))
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
            let final_targets = self.resolve_final_targets(&forwarders, func.blocks.len());

            // Update all terminators to use final targets
            self.thread_jumps(func, &final_targets);
            changed += self.stats.total_threaded();
        }

        changed += self.thread_phi_constant_edges(func);
        changed += self.fold_single_predecessor_phi_branches(func);

        if changed == 0 {
            return 0;
        }

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

        for &block_id in forwarders.keys() {
            let final_target = self.follow_chain(block_id, forwarders, block_count);
            if final_target != block_id {
                final_targets.insert(block_id, final_target);
            }
        }

        final_targets
    }

    /// Follows a chain of forwarders to find the final non-forwarder target.
    fn follow_chain(
        &self,
        start: BlockId,
        forwarders: &FxHashMap<BlockId, BlockId>,
        block_count: usize,
    ) -> BlockId {
        let mut visited = DenseBitSet::new_empty(block_count);
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
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let Some(mut term) = func.blocks[block_id].terminator.clone() else {
                continue;
            };
            self.thread_terminator(func, &mut term, final_targets);
            // jump/branch/switch through forwarders -> their final targets
            replace_terminator(func, block_id, term);
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
            | Terminator::RevertReturndata
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
                    if func
                        .inst(inst_id)
                        .kind
                        .operands()
                        .iter()
                        .any(|&operand| phi_results.contains(operand))
                    {
                        return true;
                    }
                }
            }

            if other_block == block_id {
                continue;
            }
            if let Some(term) = &block.terminator
                && term.operands().iter().any(|&operand| phi_results.contains(operand))
            {
                return true;
            }
        }

        false
    }

    fn thread_phi_constant_edges(&mut self, func: &mut Function) -> usize {
        let mut rewrites = Vec::new();
        let block_ids = func.blocks.indices();

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

    fn fold_single_predecessor_phi_branches(&mut self, func: &mut Function) -> usize {
        let mut folded = 0;
        for block in func.blocks.indices() {
            if !func.block_has_phi(block) || !func.block_has_only_phis(block) {
                continue;
            }
            let [pred] = func.blocks[block].predecessors.as_slice() else { continue };
            let pred = *pred;
            if pred == block
                || !matches!(func.blocks[pred].terminator, Some(Terminator::Jump(target)) if target == block)
            {
                continue;
            }
            let Some(mut term @ Terminator::Branch { .. }) = func.blocks[block].terminator.clone()
            else {
                continue;
            };
            if term.successors().iter().any(|&target| func.block_has_phi(target))
                || Self::block_phi_results_have_external_uses(func, block)
            {
                continue;
            }
            let mut replacements = FxHashMap::default();
            for &id in &func.blocks[block].instructions {
                let InstKind::Phi(incoming) = &func.inst(id).kind else { unreachable!() };
                if let [(incoming_pred, value)] = incoming.as_slice()
                    && *incoming_pred == pred
                    && let Some(result) = func.inst_result_value(id)
                {
                    replacements.insert(result, *value);
                }
            }
            if replacements.len() != func.blocks[block].instructions.len()
                || replacements.values().any(|value| replacements.contains_key(value))
            {
                continue;
            }
            // pred: jump block -> branch selected_phi_input, then, else
            // block: phi inputs; branch -> invalid
            let Terminator::Branch { condition, .. } = &mut term else { unreachable!() };
            if let Some(&replacement) = replacements.get(condition) {
                *condition = replacement;
            }
            let context = func.blocks[block].terminator_metadata.debug_context();
            func.replace_uses(&replacements);
            func.blocks[block].instructions.clear();
            replace_terminator(func, block, Terminator::Invalid);
            replace_terminator(func, pred, term);
            func.blocks[pred].terminator_metadata.merge_debug_context(&context);
            folded += 1;
        }
        self.stats.branches_threaded += folded;
        self.stats.gas_saved += folded * 8;
        folded
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
        let InstKind::Phi(incoming) = &func.inst(*inst_id).kind else {
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
        let Some(mut term) = func.blocks[pred].terminator.clone() else {
            return false;
        };
        let changed = match &mut term {
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
            | Terminator::RevertReturndata
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::TailCall { .. }
            | Terminator::Invalid => false,
        };
        if changed {
            // pred -> old_target -> new_target => pred -> new_target
            replace_terminator(func, pred, term);
        }
        changed
    }
}
