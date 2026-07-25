//! Aggressive dead-code elimination for side-effect-free control regions.
//!
//! This pass removes control decisions whose alternatives execute only dead pure
//! instructions and reconverge at the same phi-free target. It is deliberately
//! conservative: memory/storage/call effects, phis at the reconvergence target,
//! and values escaping a candidate dead block all prevent rewriting.

use crate::{
    mir::{BlockId, Function, Module, Terminator, Value, ValueId, utils::remove_predecessors},
    pass::{MirPass, run_function_pass_no_analyses},
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};

/// Function pass for aggressive dead-code elimination.
pub(crate) struct Adce;

impl MirPass for Adce {
    fn name(&self) -> &'static str {
        "adce"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_no_analyses(module, analyses, |func| {
            AggressiveDeadCodeEliminator::new().run(func).total() != 0
        })
    }
}

/// Statistics for aggressive dead-code elimination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AdceStats {
    /// Number of control terminators replaced with unconditional jumps.
    control_edges_removed: usize,
    /// Number of instructions removed by cleanup DCE after control rewrites.
    instructions_removed: usize,
}

impl AdceStats {
    /// Returns the total number of MIR edits made by this pass.
    const fn total(self) -> usize {
        self.control_edges_removed + self.instructions_removed
    }
}

/// Aggressive dead-code eliminator.
#[derive(Debug, Default)]
struct AggressiveDeadCodeEliminator {
    stats: AdceStats,
}

#[derive(Debug)]
struct AdceContext {
    escaping_values: DenseBitSet<ValueId>,
}

/// Shared state for one transparent-target search sweep over an unmodified CFG.
#[derive(Debug)]
struct TargetSearch {
    /// Blocks on the current depth-first search path, used to detect cycles.
    visiting: DenseBitSet<BlockId>,
    /// Memoized transparent target per fully explored block.
    targets: IndexVec<BlockId, Option<Option<BlockId>>>,
}

impl TargetSearch {
    fn new(block_count: usize) -> Self {
        Self {
            visiting: DenseBitSet::new_empty(block_count),
            targets: index_vec![None; block_count],
        }
    }
}

impl AggressiveDeadCodeEliminator {
    /// Creates a new ADCE pass.
    fn new() -> Self {
        Self::default()
    }

    /// Runs aggressive dead-code elimination once to a fixed point.
    fn run(&mut self, func: &mut Function) -> AdceStats {
        self.stats = AdceStats::default();

        loop {
            let ctx = AdceContext::new(func);
            let rewrites = self.rewrite_dead_control(func, &ctx);
            if rewrites == 0 {
                break;
            }
            self.stats.control_edges_removed += rewrites;
        }

        let mut dce = super::dce::DeadCodeEliminator::new();
        let removed = dce.run_to_fixpoint(func);
        self.stats.instructions_removed += removed;
        self.stats
    }

    fn rewrite_dead_control(&self, func: &mut Function, ctx: &AdceContext) -> usize {
        let mut rewrites = Vec::new();
        let mut search = TargetSearch::new(func.blocks.len());
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
            if target == block_id || func.block_has_phi(target) {
                continue;
            }
            rewrites.push((block_id, target));
        }

        self.rewrite_to_jumps(func, &rewrites);

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
        if let Some(target) = search.targets[block_id] {
            return target;
        }

        struct Frame {
            block: BlockId,
            successors: SmallVec<[BlockId; 2]>,
            next_successor: usize,
            common: Option<BlockId>,
        }

        let mut frames = Vec::new();
        let mut next_block = Some(block_id);
        let mut completed = None;
        loop {
            if let Some(block) = next_block.take() {
                if let Some(target) = search.targets[block] {
                    completed = Some(target);
                } else if !search.visiting.insert(block) {
                    // Re-entry along the current search path means a pure
                    // cycle with no reconvergence target.
                    completed = Some(None);
                } else if func.block_has_phi(block)
                    || self.block_has_effect(func, block)
                    || self.block_def_escapes(func, ctx, block)
                {
                    search.visiting.remove(block);
                    search.targets[block] = Some(Some(block));
                    completed = Some(Some(block));
                } else {
                    let Some(term) = func.blocks[block].terminator.as_ref() else {
                        search.visiting.remove(block);
                        search.targets[block] = Some(None);
                        completed = Some(None);
                        continue;
                    };
                    match term {
                        Terminator::Jump(_)
                        | Terminator::Branch { .. }
                        | Terminator::Switch { .. } => {
                            let successors = term.successors();
                            let successor = successors[0];
                            frames.push(Frame {
                                block,
                                successors,
                                next_successor: 1,
                                common: None,
                            });
                            next_block = Some(successor);
                            continue;
                        }
                        Terminator::Return { .. }
                        | Terminator::Revert { .. }
                        | Terminator::ReturnData { .. }
                        | Terminator::Stop
                        | Terminator::SelfDestruct { .. }
                        | Terminator::TailCall { .. }
                        | Terminator::Invalid => {
                            search.visiting.remove(block);
                            search.targets[block] = Some(Some(block));
                            completed = Some(Some(block));
                        }
                    }
                }
            }

            let target = completed.take().expect("a transparent search completed");
            let Some(frame) = frames.last_mut() else {
                return target;
            };
            if target.is_none() || frame.common.is_some_and(|common| Some(common) != target) {
                let frame = frames.pop().expect("checked last frame");
                search.visiting.remove(frame.block);
                search.targets[frame.block] = Some(None);
                completed = Some(None);
            } else {
                frame.common = target;
                if let Some(&successor) = frame.successors.get(frame.next_successor) {
                    frame.next_successor += 1;
                    next_block = Some(successor);
                } else {
                    let frame = frames.pop().expect("checked last frame");
                    search.visiting.remove(frame.block);
                    search.targets[frame.block] = Some(frame.common);
                    completed = Some(frame.common);
                }
            }
        }
    }

    fn block_has_effect(&self, func: &Function, block_id: BlockId) -> bool {
        func.blocks[block_id]
            .instructions
            .iter()
            .any(|&inst_id| func.inst(inst_id).kind.has_side_effects())
    }

    fn block_def_escapes(&self, func: &Function, ctx: &AdceContext, block_id: BlockId) -> bool {
        func.blocks[block_id].instructions.iter().any(|&inst_id| {
            let Some(value) = func.inst_result_value(inst_id) else {
                return false;
            };
            ctx.escaping_values.contains(value)
        })
    }

    fn rewrite_to_jumps(&self, func: &mut Function, rewrites: &[(BlockId, BlockId)]) {
        let mut removed_predecessors = FxHashMap::default();
        for &(block_id, _) in rewrites {
            let successors = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(Terminator::successors)
                .unwrap_or_default();
            for successor in successors {
                removed_predecessors
                    .entry(successor)
                    .or_insert_with(FxHashSet::default)
                    .insert(block_id);
            }
        }
        for &(block_id, target) in rewrites {
            func.blocks[block_id].terminator = Some(Terminator::Jump(target));
        }
        for (successor, removed) in removed_predecessors {
            remove_predecessors(func, successor, &removed);
        }
        for &(block_id, target) in rewrites {
            func.blocks[target].predecessors.push(block_id);
        }
    }
}

impl AdceContext {
    fn new(func: &Function) -> Self {
        let mut inst_blocks = index_vec![None; func.num_insts()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                inst_blocks[inst_id] = Some(block_id);
            }
        }

        let mut escaping_values = DenseBitSet::new_empty(func.values.len());
        let mut visit_operand = |block_id, operand| {
            if let Value::Inst(inst_id) = func.value(operand)
                && inst_blocks[*inst_id] != Some(block_id)
            {
                escaping_values.insert(operand);
            }
        };
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                for operand in func.inst(inst_id).kind.operands() {
                    visit_operand(block_id, operand);
                }
            }
            if let Some(term) = &block.terminator {
                for operand in term.operands() {
                    visit_operand(block_id, operand);
                }
            }
        }
        Self { escaping_values }
    }
}
