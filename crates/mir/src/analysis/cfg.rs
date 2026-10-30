//! Shared control-flow analysis utilities for MIR passes.
//!
//! The transformation passes need the same basic CFG facts over and over:
//! reachable blocks, reverse postorder, immediate dominators, dominator-tree
//! children, and path reachability. Keeping those in one place avoids subtle
//! differences between passes when unreachable predecessors or critical-edge
//! rewrites are involved.

use crate::{BlockId, Function, utils::IndexLists};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{BitMatrix, BitMatrixRow, DenseBitSet},
    graph::sccs,
    index::{IndexVec, index_vec},
};
use std::{cell::OnceCell, collections::BinaryHeap};

/// Control-flow facts for one MIR function.
#[derive(Clone, Debug)]
pub struct CfgInfo {
    successors: IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    predecessors: OnceCell<BlockLists>,
    reachable: OnceCell<DenseBitSet<BlockId>>,
    rpo: OnceCell<Vec<BlockId>>,
    cyclic_blocks: OnceCell<DenseBitSet<BlockId>>,
    dominators: OnceCell<DominatorTree>,
    dominance_frontiers: OnceCell<IndexVec<BlockId, Vec<BlockId>>>,
    reachability: OnceCell<BitMatrix<BlockId, BlockId>>,
}

impl CfgInfo {
    /// Snapshots the control-flow graph for `func`.
    #[must_use]
    pub fn new(func: &Function) -> Self {
        let successors = func
            .blocks
            .iter()
            .map(|block| {
                block.terminator.as_ref().map(|term| term.successors()).unwrap_or_default()
            })
            .collect();
        Self {
            successors,
            predecessors: OnceCell::new(),
            reachable: OnceCell::new(),
            rpo: OnceCell::new(),
            cyclic_blocks: OnceCell::new(),
            dominators: OnceCell::new(),
            dominance_frontiers: OnceCell::new(),
            reachability: OnceCell::new(),
        }
    }

    /// Returns the number of blocks in the snapshot.
    pub fn num_blocks(&self) -> usize {
        self.successors.len()
    }

    /// Returns successor blocks for `block`.
    #[must_use]
    pub fn successors(&self, block: BlockId) -> &[BlockId] {
        &self.successors[block]
    }

    /// Returns predecessor blocks for `block`, once per edge.
    #[must_use]
    pub fn predecessors(&self, block: BlockId) -> &[BlockId] {
        self.all_predecessors().get(block)
    }

    fn all_predecessors(&self) -> &BlockLists {
        self.predecessors.get_or_init(|| predecessor_lists(&self.successors))
    }

    /// Returns the blocks reachable from the entry.
    #[must_use]
    pub fn reachable(&self) -> &DenseBitSet<BlockId> {
        let _ = self.rpo();
        self.reachable.get().expect("RPO initializes reachability")
    }

    /// Returns true if `block` is reachable from the entry.
    #[must_use]
    pub fn is_reachable(&self, block: BlockId) -> bool {
        self.reachable().contains(block)
    }

    /// Returns blocks that belong to a control-flow cycle.
    #[must_use]
    pub fn cyclic_blocks(&self) -> &DenseBitSet<BlockId> {
        self.cyclic_blocks.get_or_init(|| {
            let mut cyclic = DenseBitSet::new_empty(self.successors.len());
            for scc in
                sccs(self.successors.len(), |block: BlockId| self.successors[block].iter().copied())
            {
                // A component is cyclic with several blocks or one block that branches to itself.
                if scc.len() > 1 || self.successors[scc[0]].contains(&scc[0]) {
                    for block in scc {
                        cyclic.insert(block);
                    }
                }
            }
            cyclic
        })
    }

    /// Returns reachable blocks in reverse postorder.
    #[must_use]
    pub fn rpo(&self) -> &[BlockId] {
        self.rpo.get_or_init(|| {
            let mut reachable = DenseBitSet::new_empty(self.successors.len());
            let mut rpo = Vec::with_capacity(self.successors.len());
            if self.successors.is_empty() {
                let _ = self.reachable.set(reachable);
                return rpo;
            }
            let mut stack = Vec::new();
            stack.push((BlockId::ENTRY, 0usize));
            reachable.insert(BlockId::ENTRY);
            while let Some((block, next)) = stack.last_mut() {
                if let Some(&succ) = self.successors[*block].get(*next) {
                    *next += 1;
                    if reachable.insert(succ) {
                        stack.push((succ, 0));
                    }
                } else {
                    rpo.push(*block);
                    stack.pop();
                }
            }
            rpo.reverse();
            let _ = self.reachable.set(reachable);
            rpo
        })
    }

    /// Returns immediate-dominator information.
    #[must_use]
    pub fn dominators(&self) -> &DominatorTree {
        self.dominators.get_or_init(|| DominatorTree::compute(self.all_predecessors(), self.rpo()))
    }

    /// Returns the dominance frontier of `block`: the join blocks where its dominance ends, in
    /// ascending block order. Unreachable blocks have an empty frontier.
    #[must_use]
    pub fn dominance_frontier(&self, block: BlockId) -> &[BlockId] {
        let frontiers = self.dominance_frontiers.get_or_init(|| {
            let dominators = self.dominators();
            let mut frontiers = index_vec![Vec::new(); self.successors.len()];
            let mut preds = Vec::new();
            for block in self.successors.indices().filter(|&block| self.is_reachable(block)) {
                preds.clear();
                preds.extend(
                    self.predecessors(block)
                        .iter()
                        .copied()
                        .filter(|&pred| self.is_reachable(pred)),
                );
                let Some(idom) = dominators.idom(block).filter(|_| preds.len() >= 2) else {
                    continue;
                };
                for &pred in &preds {
                    let mut runner = pred;
                    while runner != idom {
                        if frontiers[runner].last() != Some(&block) {
                            frontiers[runner].push(block);
                        }
                        let Some(next) = dominators.idom(runner) else { break };
                        if next == runner {
                            break;
                        }
                        runner = next;
                    }
                }
            }
            frontiers
        });
        frontiers.get(block).map_or(&[], Vec::as_slice)
    }

    /// Returns the blocks reachable from `block` through at least one CFG edge, or `None`
    /// for a block outside the snapshot.
    ///
    /// The table is computed lazily because only memory/state-aware passes need
    /// this more expensive transitive query.
    pub fn transitive_reachability(&self, block: BlockId) -> Option<BitMatrixRow<'_, BlockId>> {
        let reachability = self.reachability.get_or_init(|| {
            let num_blocks = self.successors.len();
            let mut reachability = BitMatrix::new(num_blocks, num_blocks);
            let mut stack = Vec::new();
            for (from, successors) in self.successors.iter_enumerated() {
                stack.extend_from_slice(successors);
                while let Some(block) = stack.pop() {
                    if reachability.insert(from, block) {
                        stack.extend_from_slice(&self.successors[block]);
                    }
                }
            }
            reachability
        });
        (block.index() < self.successors.len()).then(|| reachability.row(block))
    }
}

/// Predecessor count above which [`DominatorTree::compute`] merges idom chains with a heap instead
/// of intersecting predecessors pairwise. Typical blocks have few predecessors, and the heap
/// costs more than the pairwise fold there.
const MANY_PREDECESSORS: usize = 32;

/// Immediate-dominator tree for one MIR function.
#[derive(Clone, Debug)]
pub struct DominatorTree {
    idoms: IndexVec<BlockId, Option<BlockId>>,
    children: BlockLists,
    /// Preorder intervals make repeated dominance queries independent of tree
    /// depth. Build them only when a consumer asks about dominance.
    intervals: OnceCell<IndexVec<BlockId, (u32, u32)>>,
}

impl DominatorTree {
    fn compute(predecessors: &BlockLists, rpo: &[BlockId]) -> Self {
        let block_count = predecessors.len();
        let mut rpo_numbers = index_vec![usize::MAX; block_count];
        for (number, &block) in rpo.iter().enumerate() {
            rpo_numbers[block] = number;
        }

        let mut idoms = index_vec![None; block_count];
        idoms[BlockId::ENTRY] = Some(BlockId::ENTRY);
        let mut heap = BinaryHeap::new();
        let mut changed = true;
        while changed {
            changed = false;
            for &block in rpo {
                let block_predecessors = predecessors.get(block);
                if block_predecessors.is_empty() {
                    continue;
                }
                let new_idom = if block_predecessors.len() > MANY_PREDECESSORS {
                    Self::intersect_many(&idoms, rpo, &rpo_numbers, block_predecessors, &mut heap)
                } else {
                    let mut new_idom: Option<BlockId> = None;
                    for &pred in block_predecessors {
                        if idoms[pred].is_none() {
                            continue;
                        }
                        new_idom = Some(match new_idom {
                            None => pred,
                            Some(current) => Self::intersect(&idoms, &rpo_numbers, pred, current),
                        });
                    }
                    new_idom
                };
                if let Some(new_idom) = new_idom
                    && idoms[block] != Some(new_idom)
                {
                    idoms[block] = Some(new_idom);
                    changed = true;
                }
            }
        }

        let children = BlockLists::new(
            block_count,
            idoms.iter_enumerated().filter_map(|(block, &idom)| {
                idom.filter(|&idom| idom != block).map(|idom| (idom, block))
            }),
        );
        Self { idoms, children, intervals: OnceCell::new() }
    }

    fn intersect(
        idoms: &IndexVec<BlockId, Option<BlockId>>,
        rpo_numbers: &IndexVec<BlockId, usize>,
        a: BlockId,
        b: BlockId,
    ) -> BlockId {
        let (mut a, mut b) = (a, b);
        while a != b {
            while rpo_numbers[a] > rpo_numbers[b] {
                a = idoms[a].expect("processed block has an immediate dominator");
            }
            while rpo_numbers[b] > rpo_numbers[a] {
                b = idoms[b].expect("processed block has an immediate dominator");
            }
        }
        a
    }

    /// Returns the nearest common dominator of the processed `preds`, or `None` if there are none.
    ///
    /// Folding [`Self::intersect`] over many predecessors re-walks the same idom chain once per
    /// predecessor. Instead, advance one finger per predecessor, always the one with the largest
    /// reverse postorder number, and merge fingers that meet. Each tree node is visited at most
    /// once, so the cost follows the size of the walked subtree, not the number of predecessors.
    fn intersect_many(
        idoms: &IndexVec<BlockId, Option<BlockId>>,
        rpo: &[BlockId],
        rpo_numbers: &IndexVec<BlockId, usize>,
        preds: &[BlockId],
        heap: &mut BinaryHeap<usize>,
    ) -> Option<BlockId> {
        heap.clear();
        heap.extend(
            preds.iter().filter(|&&pred| idoms[pred].is_some()).map(|&pred| rpo_numbers[pred]),
        );
        while let Some(number) = heap.pop() {
            while heap.peek() == Some(&number) {
                heap.pop();
            }
            let block = rpo[number];
            if heap.is_empty() {
                return Some(block);
            }
            let idom = idoms[block].expect("processed block has an immediate dominator");
            heap.push(rpo_numbers[idom]);
        }
        None
    }

    /// Returns the immediate dominator of `block`, if reachable.
    #[must_use]
    pub fn idom(&self, block: BlockId) -> Option<BlockId> {
        self.idoms.get(block).copied().flatten()
    }

    /// Returns true if `dominator` dominates `block`.
    #[must_use]
    pub fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        // Preserve reflexive queries for unreachable and out-of-domain blocks.
        if dominator == block {
            return true;
        }
        let intervals = self.intervals.get_or_init(|| self.preorder_intervals());
        let (Some(&(start, end)), Some(&(position, _))) =
            (intervals.get(dominator), intervals.get(block))
        else {
            return false;
        };
        start <= position && position < end
    }

    /// A node dominates precisely the nodes in its preorder subtree. Unreachable
    /// nodes have empty intervals beyond every reachable position. Use an
    /// explicit traversal stack so deeply nested control flow cannot recurse.
    fn preorder_intervals(&self) -> IndexVec<BlockId, (u32, u32)> {
        let mut intervals = index_vec![(u32::MAX, u32::MAX); self.idoms.len()];
        let mut pending = Vec::new();
        pending.push((BlockId::ENTRY, 0));
        let mut position = 0;
        while let Some((block, child)) = pending.last_mut() {
            if *child == 0 {
                intervals[*block].0 = position;
                position += 1;
            }
            if let Some(&next) = self.children.get(*block).get(*child) {
                *child += 1;
                pending.push((next, 0));
            } else {
                intervals[*block].1 = position;
                pending.pop();
            }
        }
        intervals
    }

    /// Returns dominator-tree children of `block`.
    #[must_use]
    pub fn children(&self, block: BlockId) -> &[BlockId] {
        self.children.get(block)
    }

    /// Returns `block`, then its immediate dominators up to the entry.
    #[must_use]
    pub fn self_and_dominators(&self, block: BlockId) -> Vec<BlockId> {
        let mut out = Vec::new();
        let mut current = Some(block);
        while let Some(block) = current {
            out.push(block);
            current = self.idom(block).filter(|&idom| idom != block);
        }
        out
    }
}

/// Per-block lists of blocks.
type BlockLists = IndexLists<BlockId, BlockId>;

fn predecessor_lists(successors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>) -> BlockLists {
    BlockLists::new(
        successors.len(),
        successors.iter_enumerated().flat_map(|(block, successors)| {
            successors.iter().map(move |&successor| (successor, block))
        }),
    )
}
