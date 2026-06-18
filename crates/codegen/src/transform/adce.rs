//! Aggressive dead-code elimination for side-effect-free control regions.
//!
//! This pass removes control decisions whose alternatives execute only dead pure
//! instructions and reconverge at the same phi-free target. It is deliberately
//! conservative: memory/storage/call effects, phis at the reconvergence target,
//! and values escaping a candidate dead block all prevent rewriting.

use crate::{
    mir::{BlockId, Function, InstId, Terminator, ValueId},
    pass::FunctionPass,
    transform::DeadCodeEliminator,
    utils::{mir as mir_utils, repair_reachability_phis},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Statistics for aggressive dead-code elimination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdceStats {
    /// Number of control terminators replaced with unconditional jumps.
    pub control_edges_removed: usize,
    /// Number of instructions removed by cleanup DCE after control rewrites.
    pub instructions_removed: usize,
}

impl AdceStats {
    /// Returns the total number of MIR edits made by this pass.
    pub const fn total(self) -> usize {
        self.control_edges_removed + self.instructions_removed
    }
}

/// Aggressive dead-code eliminator.
#[derive(Debug, Default)]
pub struct AggressiveDeadCodeEliminator {
    stats: AdceStats,
}

/// Function pass for aggressive dead-code elimination.
pub struct AdcePass;

impl FunctionPass for AdcePass {
    fn name(&self) -> &str {
        "adce"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = AggressiveDeadCodeEliminator::new().run(func).total() != 0;
        repair_reachability_phis(func);
        changed
    }
}

#[derive(Debug)]
struct AdceContext {
    inst_results: FxHashMap<InstId, ValueId>,
    value_uses: FxHashMap<ValueId, FxHashSet<BlockId>>,
}

/// Shared state for one transparent-target search sweep over an unmodified CFG.
#[derive(Debug, Default)]
struct TargetSearch {
    /// Blocks on the current depth-first search path, used to detect cycles.
    visiting: FxHashSet<BlockId>,
    /// Memoized transparent target per fully explored block.
    targets: FxHashMap<BlockId, Option<BlockId>>,
}

impl AggressiveDeadCodeEliminator {
    /// Creates a new ADCE pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the statistics from the most recent run.
    pub const fn stats(&self) -> AdceStats {
        self.stats
    }

    /// Runs aggressive dead-code elimination once to a fixed point.
    pub fn run(&mut self, func: &mut Function) -> AdceStats {
        self.stats = AdceStats::default();

        loop {
            let ctx = AdceContext::new(func);
            let rewrites = self.rewrite_dead_control(func, &ctx);
            if rewrites == 0 {
                break;
            }
            self.stats.control_edges_removed += rewrites;
            repair_reachability_phis(func);
        }

        let removed = DeadCodeEliminator::new().run_to_fixpoint(func);
        self.stats.instructions_removed += removed;
        self.stats
    }

    fn rewrite_dead_control(&self, func: &mut Function, ctx: &AdceContext) -> usize {
        let mut rewrites = Vec::new();
        let mut search = TargetSearch::default();
        for block_id in func.blocks.indices() {
            let Some(term) = &func.blocks[block_id].terminator else {
                continue;
            };
            if !matches!(term, Terminator::Branch { .. } | Terminator::Switch { .. }) {
                continue;
            }
            let Some(target) =
                self.common_transparent_target(func, ctx, term.successors(), &mut search)
            else {
                continue;
            };
            if target == block_id || mir_utils::block_has_phi(func, target) {
                continue;
            }
            rewrites.push((block_id, target));
        }

        for (block_id, target) in &rewrites {
            self.rewrite_to_jump(func, *block_id, *target);
        }

        rewrites.len()
    }

    fn common_transparent_target(
        &self,
        func: &Function,
        ctx: &AdceContext,
        successors: impl IntoIterator<Item = BlockId>,
        search: &mut TargetSearch,
    ) -> Option<BlockId> {
        let mut common = None;
        for successor in successors {
            let target = self.transparent_target(func, ctx, successor, search)?;
            match common {
                Some(existing) if existing != target => return None,
                Some(_) => {}
                None => common = Some(target),
            }
        }
        common
    }

    fn transparent_target(
        &self,
        func: &Function,
        ctx: &AdceContext,
        block_id: BlockId,
        search: &mut TargetSearch,
    ) -> Option<BlockId> {
        if let Some(&target) = search.targets.get(&block_id) {
            return target;
        }
        // Re-entry along the current search path means a pure cycle: there is
        // no reconvergence target, so the cycle result is not memoized.
        if !search.visiting.insert(block_id) {
            return None;
        }
        let target = self.compute_transparent_target(func, ctx, block_id, search);
        search.visiting.remove(&block_id);
        search.targets.insert(block_id, target);
        target
    }

    fn compute_transparent_target(
        &self,
        func: &Function,
        ctx: &AdceContext,
        block_id: BlockId,
        search: &mut TargetSearch,
    ) -> Option<BlockId> {
        if mir_utils::block_has_phi(func, block_id)
            || self.block_has_effect(func, block_id)
            || self.block_def_escapes(func, ctx, block_id)
        {
            return Some(block_id);
        }

        let term = func.blocks[block_id].terminator.as_ref()?;
        match term {
            Terminator::Jump(target) => self.transparent_target(func, ctx, *target, search),
            Terminator::Branch { .. } | Terminator::Switch { .. } => {
                self.common_transparent_target(func, ctx, term.successors(), search)
            }
            Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => Some(block_id),
        }
    }

    fn block_has_effect(&self, func: &Function, block_id: BlockId) -> bool {
        func.blocks[block_id]
            .instructions
            .iter()
            .any(|&inst_id| func.instructions[inst_id].kind.has_side_effects())
    }

    fn block_def_escapes(&self, func: &Function, ctx: &AdceContext, block_id: BlockId) -> bool {
        func.blocks[block_id].instructions.iter().any(|&inst_id| {
            let Some(&value) = ctx.inst_results.get(&inst_id) else {
                return false;
            };
            ctx.value_uses
                .get(&value)
                .is_some_and(|uses| uses.iter().any(|&use_block| use_block != block_id))
        })
    }

    fn rewrite_to_jump(&self, func: &mut Function, block_id: BlockId, target: BlockId) {
        let old_successors = func.blocks[block_id]
            .terminator
            .as_ref()
            .map(|term| term.successors())
            .unwrap_or_default();

        for successor in old_successors {
            func.blocks[successor].predecessors.retain(|pred| *pred != block_id);
        }
        if !func.blocks[target].predecessors.contains(&block_id) {
            func.blocks[target].predecessors.push(block_id);
        }

        func.blocks[block_id].terminator = Some(Terminator::Jump(target));
    }
}

impl AdceContext {
    fn new(func: &Function) -> Self {
        let inst_results = mir_utils::inst_results(func);
        let value_uses = Self::value_uses(func);
        Self { inst_results, value_uses }
    }

    fn value_uses(func: &Function) -> FxHashMap<ValueId, FxHashSet<BlockId>> {
        let mut uses: FxHashMap<ValueId, FxHashSet<BlockId>> = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                for operand in func.instructions[inst_id].kind.operands() {
                    uses.entry(operand).or_default().insert(block_id);
                }
            }
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    uses.entry(operand).or_default().insert(block_id);
                }
            }
        }
        uses
    }
}
