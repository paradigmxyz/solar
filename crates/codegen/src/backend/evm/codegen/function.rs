//! Block order, cold blocks, phi edge splitting, and function labels for emission.

use super::{
    BlockId, CfgInfo, DenseBitSet, EvmCodegen, Function, FunctionId, InstId, InstKind, Label,
    Liveness, LoopAnalyzer, OptimizationMode, Terminator,
};
use crate::mir::Callee;
use std::cell::LazyCell;

impl<'gcx> EvmCodegen<'gcx> {
    /// Splits phi-carrying edges out of multi-successor predecessors when a
    /// phi destination is still read before or on a sibling path.
    ///
    /// A loop header may test its old phi result in the latch terminator or
    /// keep it live on the exit, so the backedge's phi inputs must not replace
    /// it before the branch. Rerouting such an edge through a fresh jump-only
    /// block gives those inputs a home after the predecessor's branch.
    pub(super) fn split_phi_critical_edges(func: &mut Function) {
        let has_phis = func.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        });
        if !has_phis {
            return;
        }
        // Only an edge from a block with other successors needs liveness.
        let liveness = LazyCell::new(|| Liveness::compute(func));

        let mut splits: Vec<(BlockId, BlockId)> = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
                let Some(dst) = func.inst_result_value(inst_id) else { continue };
                for &(pred, src) in incoming {
                    if src == dst || splits.contains(&(pred, block_id)) {
                        continue;
                    }
                    let Some(terminator) = func.blocks[pred].terminator.as_ref() else { continue };
                    let successors = terminator.successors();
                    if terminator.operands().contains(&dst)
                        || successors
                            .iter()
                            .any(|&succ| succ != block_id && liveness.live_in(succ).contains(dst))
                    {
                        splits.push((pred, block_id));
                    }
                }
            }
        }
        drop(liveness);

        for (pred, succ) in splits {
            let edge = func.alloc_block();
            func.blocks[edge].terminator = Some(Terminator::Jump(succ));
            func.blocks[edge].predecessors.push(pred);
            match func.blocks[pred].terminator.as_mut() {
                Some(Terminator::Branch { then_block, else_block, .. }) => {
                    if *then_block == succ {
                        *then_block = edge;
                    }
                    if *else_block == succ {
                        *else_block = edge;
                    }
                }
                Some(Terminator::Switch { default, cases, .. }) => {
                    if *default == succ {
                        *default = edge;
                    }
                    for (_, target) in cases {
                        if *target == succ {
                            *target = edge;
                        }
                    }
                }
                _ => continue,
            }
            for pred_entry in &mut func.blocks[succ].predecessors {
                if *pred_entry == pred {
                    *pred_entry = edge;
                }
            }
            let phi_insts: Vec<InstId> = func.blocks[succ]
                .instructions
                .iter()
                .copied()
                .filter(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .collect();
            for inst_id in phi_insts {
                if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
                    for (incoming_pred, _) in incoming {
                        if *incoming_pred == pred {
                            *incoming_pred = edge;
                        }
                    }
                }
            }
        }
    }

    /// Finds blocks that abort directly or can only reach other cold blocks.
    pub(super) fn collect_cold_blocks(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut cold = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = Vec::new();
        for block_id in func.blocks.indices() {
            if self.block_aborts(func, block_id) {
                cold.insert(block_id);
                worklist.push(block_id);
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return cold;
        }

        while let Some(block_id) = worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if cold.contains(predecessor) {
                    continue;
                }
                let Some(term) = func.blocks[predecessor].terminator.as_ref() else {
                    continue;
                };
                let successors = term.successors();
                if !successors.is_empty()
                    && successors.iter().all(|&successor| cold.contains(successor))
                {
                    cold.insert(predecessor);
                    worklist.push(predecessor);
                }
            }
        }
        cold
    }

    /// Returns true when a block aborts directly or calls a function whose
    /// reachable exits all abort.
    fn block_aborts(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];
        matches!(
            block.terminator,
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
        ) || matches!(
            block.terminator,
            Some(Terminator::TailCall { function, .. })
                if self.cold_functions.contains(function)
        ) || block.instructions.iter().any(|&inst_id| {
            matches!(
                func.inst(inst_id).kind,
                InstKind::ICall { function: Callee::Function(function), .. }
                    if self.cold_functions.contains(function)
            )
        })
    }

    pub(super) fn block_is_cold(&self, block_id: BlockId) -> bool {
        self.cold_blocks.contains(block_id)
    }

    pub(super) fn new_function_label(&mut self, function: FunctionId) -> Label {
        let label = self.asm.new_label();
        if self.cold_functions.contains(function) {
            self.asm.mark_label_cold(label);
        }
        label
    }

    pub(super) fn block_layout_order(&self, func: &Function, cfg: &CfgInfo) -> Vec<BlockId> {
        // Visit predecessors before private continuations, including blocks appended by lowering.
        let reachable = cfg.reachable();
        let mut order = Vec::with_capacity(func.blocks.len());
        let mut placed = DenseBitSet::new_empty(func.blocks.len());

        self.append_layout_chain(func, BlockId::ENTRY, reachable, &mut placed, &mut order);
        // Reverse postorder emits every block after its forward predecessors. The search enters
        // a loop's own blocks before its exits: the successor a search leaves last is the first
        // one emitted after the block, so a header is followed by its exit and its body comes
        // later. The header's branch then falls through to the exit and jumps into the body,
        // and the EVM IR loop layout places the latch before the header; a header followed by
        // its body folds the body into a self-loop that pays a jump on every iteration. The EVM
        // IR layout passes choose the physical order later, so this order only decides which
        // arm each branch falls through to.
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = if cfg.cyclic_blocks().is_empty() {
            None
        } else {
            Some(loop_analyzer.analyze_structure(func))
        };
        let stays_in_loop = |block: BlockId, successor: BlockId| {
            loop_info
                .as_ref()
                .and_then(|info| {
                    info.block_to_loop.get(&block).and_then(|header| info.loops.get(header))
                })
                .is_some_and(|loop_data| loop_data.blocks.contains(successor))
        };
        // Successors are popped from the end, so a loop's own blocks go last.
        let search_order = |block: BlockId| {
            let successors = cfg.successors(block);
            let mut ordered = Vec::with_capacity(successors.len());
            ordered.extend(successors.iter().rev().filter(|&&succ| !stays_in_loop(block, succ)));
            ordered.extend(successors.iter().rev().filter(|&&succ| stays_in_loop(block, succ)));
            ordered
        };
        let mut visited = DenseBitSet::new_empty(func.blocks.len());
        let mut postorder = Vec::with_capacity(func.blocks.len());
        let mut search = Vec::new();
        search.push((BlockId::ENTRY, search_order(BlockId::ENTRY)));
        visited.insert(BlockId::ENTRY);
        while let Some((block, successors)) = search.last_mut() {
            if let Some(succ) = successors.pop() {
                if visited.insert(succ) {
                    search.push((succ, search_order(succ)));
                }
            } else {
                postorder.push(*block);
                search.pop();
            }
        }
        for &block_id in postorder.iter().rev() {
            self.append_layout_chain(func, block_id, reachable, &mut placed, &mut order);
        }

        order
    }

    fn append_layout_chain(
        &self,
        func: &Function,
        mut block_id: BlockId,
        reachable: &DenseBitSet<BlockId>,
        placed: &mut DenseBitSet<BlockId>,
        order: &mut Vec<BlockId>,
    ) {
        loop {
            if !reachable.contains(block_id) || !placed.insert(block_id) {
                return;
            }
            order.push(block_id);

            let target = match func.blocks[block_id].terminator.as_ref() {
                Some(Terminator::Jump(target))
                    if func.blocks[*target].predecessors.as_slice() == [block_id] =>
                {
                    *target
                }
                Some(Terminator::Branch { then_block, else_block, .. })
                    if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) =>
                {
                    match (self.block_is_cold(*then_block), self.block_is_cold(*else_block)) {
                        (true, false) => *else_block,
                        (false, true) => *then_block,
                        // jumpi condition, then; else ... exit; then ... exit
                        (false, false)
                            if self.gcx.sess.opts.optimization.is_gas()
                                && [*then_block, *else_block].into_iter().all(|target| {
                                    func.blocks[target]
                                        .terminator
                                        .as_ref()
                                        .is_some_and(|term| !term.has_successors())
                                }) =>
                        {
                            *else_block
                        }
                        _ => return,
                    }
                }
                _ => return,
            };
            if placed.contains(target) {
                return;
            }

            block_id = target;
        }
    }
}
