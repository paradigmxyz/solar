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
    map::FxHashSet,
    newtype_index,
};

newtype_index! {
    struct ComponentId;
}

/// Control-flow facts for one MIR function.
#[derive(Clone, Debug)]
pub(crate) struct CfgInfo {
    successors: IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    reachable: OnceCell<DenseBitSet<BlockId>>,
    rpo: OnceCell<Vec<BlockId>>,
    cyclic_blocks: OnceCell<DenseBitSet<BlockId>>,
    dominators: OnceCell<DominatorTree>,
    transitive_reachability: OnceCell<TransitiveReachability>,
}

impl CfgInfo {
    /// Snapshots the control-flow graph for `func`.
    #[must_use]
    pub(crate) fn new(func: &Function) -> Self {
        let successors = func
            .blocks
            .iter()
            .map(|block| {
                block.terminator.as_ref().map(|term| term.successors()).unwrap_or_default()
            })
            .collect();
        Self {
            successors,
            reachable: OnceCell::new(),
            rpo: OnceCell::new(),
            cyclic_blocks: OnceCell::new(),
            dominators: OnceCell::new(),
            transitive_reachability: OnceCell::new(),
        }
    }

    /// Returns successor blocks for `block`.
    #[must_use]
    pub(crate) fn successors(&self, block: BlockId) -> &[BlockId] {
        &self.successors[block]
    }

    /// Returns whether `func` has exactly the snapshotted CFG.
    #[must_use]
    pub(crate) fn has_same_edges(&self, func: &Function) -> bool {
        self.successors.len() == func.blocks.len()
            && func.blocks.iter_enumerated().all(|(block, basic_block)| {
                basic_block
                    .terminator
                    .as_ref()
                    .map(|terminator| terminator.successors())
                    .unwrap_or_default()
                    .as_slice()
                    == self.successors[block].as_slice()
            })
    }

    /// Returns whether every edge in `func` existed in the snapshot.
    #[must_use]
    pub(crate) fn contains_edges_of(&self, func: &Function) -> bool {
        if self.successors.len() != func.blocks.len() {
            return false;
        }
        let mut old_set = FxHashSet::default();
        for (block, basic_block) in func.blocks.iter_enumerated() {
            let current = basic_block
                .terminator
                .as_ref()
                .map(|terminator| terminator.successors())
                .unwrap_or_default();
            let old = &self.successors[block];
            if old.len() <= 4 {
                if current.iter().any(|successor| !old.contains(successor)) {
                    return false;
                }
            } else {
                old_set.clear();
                old_set.extend(old.iter().copied());
                if current.iter().any(|successor| !old_set.contains(successor)) {
                    return false;
                }
            }
        }
        true
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

    /// Returns blocks that belong to a control-flow cycle.
    #[must_use]
    pub(crate) fn cyclic_blocks(&self) -> &DenseBitSet<BlockId> {
        self.cyclic_blocks.get_or_init(|| {
            let block_count = self.successors.len();
            let mut predecessors = index_vec![Vec::new(); block_count];
            for (block, successors) in self.successors.iter_enumerated() {
                for &successor in successors {
                    predecessors[successor].push(block);
                }
            }

            let mut visited = DenseBitSet::new_empty(block_count);
            let mut postorder = Vec::with_capacity(block_count);
            let mut stack = Vec::new();
            for root in self.successors.indices() {
                if !visited.insert(root) {
                    continue;
                }
                stack.push((root, 0));
                while let Some((block, next)) = stack.last_mut() {
                    if let Some(&successor) = self.successors[*block].get(*next) {
                        *next += 1;
                        if visited.insert(successor) {
                            stack.push((successor, 0));
                        }
                    } else {
                        postorder.push(*block);
                        stack.pop();
                    }
                }
            }

            let mut assigned = DenseBitSet::new_empty(block_count);
            let mut cyclic = DenseBitSet::new_empty(block_count);
            let mut component = Vec::new();
            for root in postorder.into_iter().rev() {
                if !assigned.insert(root) {
                    continue;
                }
                component.clear();
                component.push(root);
                stack.push((root, 0));
                while let Some((block, next)) = stack.last_mut() {
                    if let Some(&predecessor) = predecessors[*block].get(*next) {
                        *next += 1;
                        if assigned.insert(predecessor) {
                            component.push(predecessor);
                            stack.push((predecessor, 0));
                        }
                    } else {
                        stack.pop();
                    }
                }
                if component.len() > 1 || self.successors[root].contains(&root) {
                    for &block in &component {
                        cyclic.insert(block);
                    }
                }
            }
            cyclic
        })
    }

    /// Returns immediate-dominator information.
    #[must_use]
    pub(crate) fn dominators(&self) -> &DominatorTree {
        self.dominators.get_or_init(|| DominatorTree::compute(&self.successors))
    }

    /// Returns block-to-block reachability through at least one CFG edge.
    ///
    /// The result is computed lazily because only memory/state-aware passes need
    /// this more expensive transitive query.
    pub(crate) fn transitive_reachability(&self) -> &TransitiveReachability {
        self.transitive_reachability
            .get_or_init(|| TransitiveReachability::compute(&self.successors))
    }
}

/// Block reachability compressed through the CFG's strongly connected components.
#[derive(Clone, Debug)]
pub(crate) struct TransitiveReachability {
    components: IndexVec<BlockId, ComponentId>,
    reachable: IndexVec<ComponentId, DenseBitSet<ComponentId>>,
    cyclic: DenseBitSet<ComponentId>,
}

impl TransitiveReachability {
    fn compute(successors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>) -> Self {
        let block_count = successors.len();
        let mut predecessors = index_vec![Vec::new(); block_count];
        for (block, block_successors) in successors.iter_enumerated() {
            for &successor in block_successors {
                predecessors[successor].push(block);
            }
        }

        let mut visited = DenseBitSet::new_empty(block_count);
        let mut postorder = Vec::with_capacity(block_count);
        let mut stack = Vec::new();
        for root in successors.indices() {
            if !visited.insert(root) {
                continue;
            }
            stack.push((root, 0));
            while let Some((block, next)) = stack.last_mut() {
                if let Some(&successor) = successors[*block].get(*next) {
                    *next += 1;
                    if visited.insert(successor) {
                        stack.push((successor, 0));
                    }
                } else {
                    postorder.push(*block);
                    stack.pop();
                }
            }
        }

        let mut components = index_vec![ComponentId::MAX; block_count];
        let mut component_ids = IndexVec::<ComponentId, ()>::new();
        let mut pending = Vec::new();
        for root in postorder.into_iter().rev() {
            if components[root] != ComponentId::MAX {
                continue;
            }
            let component = component_ids.push(());
            components[root] = component;
            pending.push(root);
            while let Some(block) = pending.pop() {
                for &predecessor in &predecessors[block] {
                    if components[predecessor] == ComponentId::MAX {
                        components[predecessor] = component;
                        pending.push(predecessor);
                    }
                }
            }
        }

        let component_count = component_ids.len();
        let mut component_successors = index_vec![Vec::new(); component_count];
        let mut cyclic = DenseBitSet::new_empty(component_count);
        for (block, block_successors) in successors.iter_enumerated() {
            let component = components[block];
            for &successor in block_successors {
                let successor_component = components[successor];
                if successor_component == component {
                    cyclic.insert(component);
                } else {
                    component_successors[component].push(successor_component);
                }
            }
        }
        for successors in &mut component_successors {
            successors.sort_unstable();
            successors.dedup();
        }

        let mut incoming = index_vec![0usize; component_count];
        for successors in &component_successors {
            for &successor in successors {
                incoming[successor] += 1;
            }
        }
        let mut component_pending = incoming
            .iter_enumerated()
            .filter_map(|(component, &count)| (count == 0).then_some(component))
            .collect::<Vec<_>>();
        let mut topological = Vec::with_capacity(component_count);
        while let Some(component) = component_pending.pop() {
            topological.push(component);
            for &successor in &component_successors[component] {
                incoming[successor] -= 1;
                if incoming[successor] == 0 {
                    component_pending.push(successor);
                }
            }
        }
        debug_assert_eq!(topological.len(), component_count);

        let mut reachable = index_vec![DenseBitSet::new_empty(component_count); component_count];
        for &component in topological.iter().rev() {
            let mut component_reachable = DenseBitSet::new_empty(component_count);
            for &successor in &component_successors[component] {
                component_reachable.insert(successor);
                component_reachable.union(&reachable[successor]);
            }
            reachable[component] = component_reachable;
        }

        Self { components, reachable, cyclic }
    }

    /// Returns whether `to` is reachable from `from` through at least one CFG edge.
    #[must_use]
    pub(crate) fn can_reach(&self, from: BlockId, to: BlockId) -> bool {
        let from = self.components[from];
        let to = self.components[to];
        if from == to { self.cyclic.contains(from) } else { self.reachable[from].contains(to) }
    }
}

/// Immediate-dominator tree for one MIR function.
#[derive(Clone, Debug)]
pub(crate) struct DominatorTree {
    idoms: IndexVec<BlockId, Option<BlockId>>,
    children: IndexVec<BlockId, Vec<BlockId>>,
    preorder: IndexVec<BlockId, usize>,
    subtree_end: IndexVec<BlockId, usize>,
}

impl DominatorTree {
    fn compute(successors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>) -> Self {
        let block_count = successors.len();
        let mut predecessors = index_vec![Vec::new(); block_count];
        for (block, block_successors) in successors.iter_enumerated() {
            for &successor in block_successors {
                predecessors[successor].push(block);
            }
        }

        // Build the DFS spanning tree used by Lengauer-Tarjan.
        let mut blocks = Vec::with_capacity(block_count);
        let mut block_preorders = index_vec![usize::MAX; block_count];
        let mut parents = Vec::with_capacity(block_count);
        let mut stack = vec![(BlockId::ENTRY, 0usize)];
        blocks.push(BlockId::ENTRY);
        block_preorders[BlockId::ENTRY] = 0;
        parents.push(0);
        while let Some((block, next_successor)) = stack.last_mut() {
            if let Some(&successor) = successors[*block].get(*next_successor) {
                *next_successor += 1;
                if block_preorders[successor] == usize::MAX {
                    let preorder = blocks.len();
                    blocks.push(successor);
                    block_preorders[successor] = preorder;
                    parents.push(block_preorders[*block]);
                    stack.push((successor, 0));
                }
            } else {
                stack.pop();
            }
        }

        // Compute semidominators and partial immediate dominators with the
        // link-eval forest, then resolve the remaining relative dominators.
        let reachable = blocks.len();
        let mut immediate_dominators = vec![0usize; reachable];
        let mut semidominators = (0..reachable).collect::<Vec<_>>();
        let mut labels = semidominators.clone();
        let mut ancestors = vec![usize::MAX; reachable];
        let mut buckets = vec![Vec::new(); reachable];
        let mut eval_stack = Vec::new();
        for block_preorder in (1..reachable).rev() {
            for &predecessor in &predecessors[blocks[block_preorder]] {
                let predecessor = block_preorders[predecessor];
                if predecessor == usize::MAX {
                    continue;
                }
                let evaluated = Self::eval(
                    predecessor,
                    &mut ancestors,
                    &semidominators,
                    &mut labels,
                    &mut eval_stack,
                );
                semidominators[block_preorder] =
                    semidominators[block_preorder].min(semidominators[evaluated]);
            }

            buckets[semidominators[block_preorder]].push(block_preorder);
            let parent = parents[block_preorder];
            ancestors[block_preorder] = parent;
            for pending in std::mem::take(&mut buckets[parent]) {
                let evaluated = Self::eval(
                    pending,
                    &mut ancestors,
                    &semidominators,
                    &mut labels,
                    &mut eval_stack,
                );
                immediate_dominators[pending] =
                    if semidominators[evaluated] < semidominators[pending] {
                        evaluated
                    } else {
                        parent
                    };
            }
        }
        for block_preorder in 1..reachable {
            if immediate_dominators[block_preorder] != semidominators[block_preorder] {
                immediate_dominators[block_preorder] =
                    immediate_dominators[immediate_dominators[block_preorder]];
            }
        }

        let mut idoms = index_vec![None; block_count];
        for (block_preorder, &block) in blocks.iter().enumerate() {
            idoms[block] = Some(blocks[immediate_dominators[block_preorder]]);
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

        let mut preorder = index_vec![usize::MAX; block_count];
        let mut subtree_end = index_vec![usize::MAX; block_count];
        let mut next_preorder = 0;
        let mut stack = vec![(BlockId::ENTRY, 0usize)];
        preorder[BlockId::ENTRY] = next_preorder;
        next_preorder += 1;
        while let Some((block, next_child)) = stack.last_mut() {
            if let Some(&child) = children[*block].get(*next_child) {
                *next_child += 1;
                preorder[child] = next_preorder;
                next_preorder += 1;
                stack.push((child, 0));
            } else {
                subtree_end[*block] = next_preorder;
                stack.pop();
            }
        }

        Self { idoms, children, preorder, subtree_end }
    }

    /// Returns the label with the lowest semidominator on `node`'s compressed
    /// ancestor path.
    fn eval(
        node: usize,
        ancestors: &mut [usize],
        semidominators: &[usize],
        labels: &mut [usize],
        stack: &mut Vec<usize>,
    ) -> usize {
        stack.clear();
        let mut current = node;
        while ancestors[current] != usize::MAX && ancestors[ancestors[current]] != usize::MAX {
            stack.push(current);
            current = ancestors[current];
        }
        while let Some(current) = stack.pop() {
            let ancestor = ancestors[current];
            if semidominators[labels[ancestor]] < semidominators[labels[current]] {
                labels[current] = labels[ancestor];
            }
            ancestors[current] = ancestors[ancestor];
        }
        labels[node]
    }

    /// Returns the immediate dominator of `block`, if reachable.
    #[must_use]
    pub(crate) fn idom(&self, block: BlockId) -> Option<BlockId> {
        self.idoms.get(block).copied().flatten()
    }

    /// Returns true if `dominator` dominates `block`.
    #[must_use]
    pub(crate) fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        if dominator == block {
            return true;
        }
        let start = self.preorder[dominator];
        start != usize::MAX
            && start <= self.preorder[block]
            && self.preorder[block] < self.subtree_end[dominator]
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
