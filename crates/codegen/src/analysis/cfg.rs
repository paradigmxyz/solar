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
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Control-flow facts for one MIR function.
#[derive(Clone, Debug)]
pub struct CfgInfo {
    entry_block: BlockId,
    successors: Vec<SmallVec<[BlockId; 2]>>,
    reachable: OnceCell<DenseBitSet<BlockId>>,
    rpo: OnceCell<Vec<BlockId>>,
    dominators: OnceCell<DominatorTree>,
    reachability: OnceCell<FxHashMap<BlockId, DenseBitSet<BlockId>>>,
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
            entry_block: func.entry_block,
            successors,
            reachable: OnceCell::new(),
            rpo: OnceCell::new(),
            dominators: OnceCell::new(),
            reachability: OnceCell::new(),
        }
    }

    /// Returns successor blocks for `block`.
    #[must_use]
    pub fn successors(&self, block: BlockId) -> &[BlockId] {
        &self.successors[block.index()]
    }

    /// Returns the blocks reachable from the entry.
    #[must_use]
    pub fn reachable(&self) -> &DenseBitSet<BlockId> {
        self.reachable.get_or_init(|| {
            let mut reachable = DenseBitSet::new_empty(self.successors.len());
            let mut stack = Vec::new();
            stack.push(self.entry_block);
            while let Some(block) = stack.pop() {
                if reachable.insert(block) {
                    stack.extend(self.successors[block.index()].iter().copied());
                }
            }
            reachable
        })
    }

    /// Returns true if `block` is reachable from the entry.
    #[must_use]
    pub fn is_reachable(&self, block: BlockId) -> bool {
        self.reachable().contains(block)
    }

    /// Returns reachable blocks in reverse postorder.
    #[must_use]
    pub fn rpo(&self) -> &[BlockId] {
        self.rpo.get_or_init(|| {
            let mut reachable = DenseBitSet::new_empty(self.successors.len());
            let mut rpo = Vec::with_capacity(self.successors.len());
            let mut stack = vec![(self.entry_block, 0usize)];
            reachable.insert(self.entry_block);
            while let Some((block, next)) = stack.last_mut() {
                if let Some(&succ) = self.successors[block.index()].get(*next) {
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
        self.dominators
            .get_or_init(|| DominatorTree::compute(self.entry_block, &self.successors, self.rpo()))
    }

    /// Returns block-to-block reachability through at least one CFG edge.
    ///
    /// The map is computed lazily because only memory/state-aware passes need
    /// this more expensive transitive query.
    pub fn transitive_reachability(&self) -> &FxHashMap<BlockId, DenseBitSet<BlockId>> {
        self.reachability.get_or_init(|| {
            let mut reachability = FxHashMap::default();
            let mut stack = Vec::new();
            for block_index in 0..self.successors.len() {
                let block_id = BlockId::from_usize(block_index);
                let mut reachable = DenseBitSet::new_empty(self.successors.len());
                stack.clear();
                stack.extend(self.successors[block_id.index()].iter().copied());
                while let Some(block) = stack.pop() {
                    if reachable.insert(block) {
                        stack.extend(self.successors[block.index()].iter().copied());
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
pub struct DominatorTree {
    idoms: Vec<Option<BlockId>>,
    children: Vec<Vec<BlockId>>,
}

impl DominatorTree {
    fn compute(
        entry_block: BlockId,
        successors: &[SmallVec<[BlockId; 2]>],
        rpo: &[BlockId],
    ) -> Self {
        let block_count = successors.len();
        let mut predecessors = vec![Vec::new(); block_count];
        for (block_index, block_successors) in successors.iter().enumerate() {
            let block = BlockId::from_usize(block_index);
            for &successor in block_successors {
                predecessors[successor.index()].push(block);
            }
        }
        let mut rpo_numbers = vec![usize::MAX; block_count];
        for (number, &block) in rpo.iter().enumerate() {
            rpo_numbers[block.index()] = number;
        }

        let mut idoms = vec![None; block_count];
        idoms[entry_block.index()] = Some(entry_block);
        let mut changed = true;
        while changed {
            changed = false;
            for &block in rpo {
                if block == entry_block {
                    continue;
                }
                let mut new_idom: Option<BlockId> = None;
                for &pred in &predecessors[block.index()] {
                    if idoms[pred.index()].is_none() {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => pred,
                        Some(current) => Self::intersect(&idoms, &rpo_numbers, pred, current),
                    });
                }
                if let Some(new_idom) = new_idom
                    && idoms[block.index()] != Some(new_idom)
                {
                    idoms[block.index()] = Some(new_idom);
                    changed = true;
                }
            }
        }

        let mut children = vec![Vec::new(); block_count];
        for (block_index, idom) in idoms.iter().copied().enumerate() {
            let block = BlockId::from_usize(block_index);
            if block == entry_block {
                continue;
            }
            if let Some(idom) = idom {
                children[idom.index()].push(block);
            }
        }
        for children in &mut children {
            children.sort_by_key(|block| block.index());
        }

        Self { idoms, children }
    }

    fn intersect(
        idoms: &[Option<BlockId>],
        rpo_numbers: &[usize],
        a: BlockId,
        b: BlockId,
    ) -> BlockId {
        let (mut a, mut b) = (a, b);
        while a != b {
            while rpo_numbers[a.index()] > rpo_numbers[b.index()] {
                a = idoms[a.index()].expect("processed block has an immediate dominator");
            }
            while rpo_numbers[b.index()] > rpo_numbers[a.index()] {
                b = idoms[b.index()].expect("processed block has an immediate dominator");
            }
        }
        a
    }

    /// Returns the immediate dominator of `block`, if reachable.
    #[must_use]
    pub fn idom(&self, block: BlockId) -> Option<BlockId> {
        self.idoms.get(block.index()).copied().flatten()
    }

    /// Returns true if `dominator` dominates `block`.
    #[must_use]
    pub fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
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
    pub fn children(&self, block: BlockId) -> &[BlockId] {
        self.children.get(block.index()).map_or(&[], Vec::as_slice)
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
