//! Shared control-flow analysis utilities for MIR passes.
//!
//! The transformation passes need the same basic CFG facts over and over:
//! reachable blocks, reverse postorder, immediate dominators, dominator-tree
//! children, and path reachability. Keeping those in one place avoids subtle
//! differences between passes when unreachable predecessors or critical-edge
//! rewrites are involved.

use std::cell::OnceCell;

use crate::mir::{BlockId, Function};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Control-flow facts for one MIR function.
#[derive(Clone, Debug)]
pub(crate) struct CfgInfo {
    successors: IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    reachable: OnceCell<DenseBitSet<BlockId>>,
    rpo: OnceCell<Vec<BlockId>>,
    dominators: OnceCell<DominatorTree>,
    reachability: OnceCell<FxHashMap<BlockId, DenseBitSet<BlockId>>>,
}

impl CfgInfo {
    /// Snapshots the control-flow graph for `func`.
    #[must_use]
    pub(crate) fn new(func: &Function) -> Self {
        let successors = func
            .blocks()
            .map(|block| {
                block.terminator.as_ref().map(|term| term.successors()).unwrap_or_default()
            })
            .collect();
        Self {
            successors,
            reachable: OnceCell::new(),
            rpo: OnceCell::new(),
            dominators: OnceCell::new(),
            reachability: OnceCell::new(),
        }
    }

    /// Returns successor blocks for `block`.
    #[must_use]
    pub(crate) fn successors(&self, block: BlockId) -> &[BlockId] {
        &self.successors[block]
    }

    /// Returns the blocks reachable from the entry.
    #[must_use]
    pub(crate) fn reachable(&self) -> &DenseBitSet<BlockId> {
        self.reachable.get_or_init(|| {
            let mut reachable = DenseBitSet::new_empty(self.successors.len());
            let mut stack = Vec::new();
            stack.push(BlockId::ENTRY);
            while let Some(block) = stack.pop() {
                if reachable.insert(block) {
                    stack.extend(self.successors[block].iter().copied());
                }
            }
            reachable
        })
    }

    /// Returns true if `block` is reachable from the entry.
    #[must_use]
    pub(crate) fn is_reachable(&self, block: BlockId) -> bool {
        self.reachable().contains(block)
    }

    /// Returns reachable blocks in reverse postorder.
    #[must_use]
    pub(crate) fn rpo(&self) -> &[BlockId] {
        self.rpo.get_or_init(|| {
            let mut reachable = DenseBitSet::new_empty(self.successors.len());
            let mut rpo = Vec::with_capacity(self.successors.len());
            let mut stack = vec![(BlockId::ENTRY, 0usize)];
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
    pub(crate) fn dominators(&self) -> &DominatorTree {
        self.dominators.get_or_init(|| DominatorTree::compute(&self.successors, self.rpo()))
    }

    /// Returns block-to-block reachability through at least one CFG edge.
    ///
    /// The map is computed lazily because only memory/state-aware passes need
    /// this more expensive transitive query.
    pub(crate) fn transitive_reachability(&self) -> &FxHashMap<BlockId, DenseBitSet<BlockId>> {
        self.reachability.get_or_init(|| {
            let mut reachability = FxHashMap::default();
            let mut stack = Vec::new();
            for block_id in self.successors.indices() {
                let mut reachable = DenseBitSet::new_empty(self.successors.len());
                stack.clear();
                stack.extend(self.successors[block_id].iter().copied());
                while let Some(block) = stack.pop() {
                    if reachable.insert(block) {
                        stack.extend(self.successors[block].iter().copied());
                    }
                }
                reachability.insert(block_id, reachable);
            }
            reachability
        })
    }
}

/// Immediate-dominator tree for one MIR function.
#[derive(Clone, Debug)]
pub(crate) struct DominatorTree {
    idoms: IndexVec<BlockId, Option<BlockId>>,
    children: IndexVec<BlockId, Vec<BlockId>>,
}

impl DominatorTree {
    fn compute(successors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>, rpo: &[BlockId]) -> Self {
        let block_count = successors.len();
        let mut predecessors = index_vec![Vec::new(); block_count];
        for (block, block_successors) in successors.iter_enumerated() {
            for &successor in block_successors {
                predecessors[successor].push(block);
            }
        }
        let mut rpo_numbers = index_vec![usize::MAX; block_count];
        for (number, &block) in rpo.iter().enumerate() {
            rpo_numbers[block] = number;
        }

        let mut idoms = index_vec![None; block_count];
        idoms[BlockId::ENTRY] = Some(BlockId::ENTRY);
        let mut changed = true;
        while changed {
            changed = false;
            for &block in rpo {
                let block_predecessors = &predecessors[block];
                if block_predecessors.is_empty() {
                    continue;
                }
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
                if let Some(new_idom) = new_idom
                    && idoms[block] != Some(new_idom)
                {
                    idoms[block] = Some(new_idom);
                    changed = true;
                }
            }
        }

        let mut children = index_vec![Vec::new(); block_count];
        for (block, idom) in idoms.iter_enumerated() {
            if let Some(idom) = *idom
                && idom != block
            {
                children[idom].push(block);
            }
        }
        for children in &mut children {
            children.sort_by_key(|block| block.index());
        }

        Self { idoms, children }
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

    /// Returns the immediate dominator of `block`, if reachable.
    #[must_use]
    pub(crate) fn idom(&self, block: BlockId) -> Option<BlockId> {
        self.idoms.get(block).copied().flatten()
    }

    /// Returns true if `dominator` dominates `block`.
    #[must_use]
    pub(crate) fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        let mut current = block;
        loop {
            if current == dominator {
                return true;
            }
            match self.idom(current) {
                Some(idom) if idom != current => current = idom,
                _ => return false,
            }
        }
    }

    /// Returns dominator-tree children of `block`.
    #[must_use]
    pub(crate) fn children(&self, block: BlockId) -> &[BlockId] {
        self.children.get(block).map_or(&[], Vec::as_slice)
    }

    /// Returns `block`, then its immediate dominators up to the entry.
    #[must_use]
    pub(crate) fn self_and_dominators(&self, block: BlockId) -> Vec<BlockId> {
        let mut out = Vec::new();
        let mut current = Some(block);
        while let Some(block) = current {
            out.push(block);
            current = self.idom(block).filter(|&idom| idom != block);
        }
        out
    }
}
