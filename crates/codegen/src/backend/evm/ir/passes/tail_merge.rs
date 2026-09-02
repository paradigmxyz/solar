//! Merge profitable suffixes of machine-level terminal blocks.
//!
//! The pass groups blocks by their terminator and indexes representative tails
//! in reverse. This finds each block's longest shared suffix without comparing
//! it with every earlier block. It then splits profitable suffixes into shared
//! tail blocks until no new merges remain. Each candidate includes the cost of its new jumps and
//! labels, and the pass keeps address-taken or otherwise incompatible entries separate.

use super::{
    EvmPass,
    utils::{FreshLabels, MachineInstKey, instruction_size_lower_bound, is_terminal_boundary},
};
use crate::backend::evm::{
    ir::{Block, BlockId, Hotness, Module, Terminator, TerminatorKind},
    op::{StackOp, push_len},
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

pub(super) struct TailMerge;

impl EvmPass for TailMerge {
    fn name(&self) -> &'static str {
        "tail-merge"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        merge_tails(gcx, module)
    }
}

fn merge_tails(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    state.plan_merges(gcx, module);
    if state.merges.is_empty() {
        return false;
    }
    let mut labels = FreshLabels::new(module);
    let mut changed = false;
    loop {
        if !state.apply_merges(module, &mut labels) {
            return changed;
        }
        changed = true;
        state.plan_merges(gcx, module);
        if state.merges.is_empty() {
            return true;
        }
    }
}

#[derive(Default)]
struct RunState {
    merges: Vec<Merge>,
    group_indices: FxHashMap<BlockId, usize>,
    groups: Vec<MergeGroup>,
    commons: Vec<usize>,
    tails: Vec<(usize, BlockId)>,
    tail_roots: FxHashMap<TerminatorKind, usize>,
    tail_nodes: Vec<TailNode>,
    tail_node_pool: Vec<TailNode>,
}

impl RunState {
    fn plan_merges(&mut self, gcx: Gcx<'_>, module: &Module) {
        self.merges.clear();
        self.tail_roots.clear();
        self.tail_node_pool.append(&mut self.tail_nodes);
        self.tail_node_pool.iter_mut().for_each(TailNode::clear);
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !is_candidate(block) {
                continue;
            }

            let matched = self.longest_common_tail(block);

            if let Some((representative, common)) = matched
                && common > 0
                && suffix_size(gcx, module, block_id, common) > 5
            {
                self.merges.push(Merge { representative, block: block_id, common });
            } else {
                self.insert_tail(block_id, block);
            }
        }
    }

    fn longest_common_tail(&self, block: &Block) -> Option<(BlockId, usize)> {
        let terminator = &block.terminator.as_ref()?.kind;
        let mut node = *self.tail_roots.get(terminator)?;
        let mut matched = None;
        for (common, inst) in block.instructions.iter().rev().enumerate() {
            let Some(&child) = self.tail_nodes[node].children.get(&MachineInstKey::new(inst))
            else {
                break;
            };
            node = child;
            if let Some(representative) = self.tail_nodes[node].representative {
                matched = Some((representative, common + 1));
            }
        }
        matched
    }

    fn insert_tail(&mut self, block_id: BlockId, block: &Block) {
        let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
        let mut node = self.tail_root(terminator);
        self.tail_nodes[node].representative.get_or_insert(block_id);
        for inst in block.instructions.iter().rev() {
            node = self.tail_child(node, MachineInstKey::new(inst));
            self.tail_nodes[node].representative.get_or_insert(block_id);
        }
    }

    fn tail_root(&mut self, terminator: &TerminatorKind) -> usize {
        if let Some(&root) = self.tail_roots.get(terminator) {
            return root;
        }
        let root = self.new_tail_node();
        self.tail_roots.insert(terminator.clone(), root);
        root
    }

    fn tail_child(&mut self, node: usize, key: MachineInstKey) -> usize {
        if let Some(&child) = self.tail_nodes[node].children.get(&key) {
            return child;
        }
        let child = self.new_tail_node();
        self.tail_nodes[node].children.insert(key, child);
        child
    }

    fn new_tail_node(&mut self) -> usize {
        let node = self.tail_nodes.len();
        self.tail_nodes.push(self.tail_node_pool.pop().unwrap_or_default());
        node
    }

    fn apply_merges(&mut self, module: &mut Module, labels: &mut FreshLabels) -> bool {
        self.group_indices.clear();
        let mut group_count = 0;
        for &merge in &self.merges {
            let index = if let Some(&index) = self.group_indices.get(&merge.representative) {
                index
            } else {
                let index = group_count;
                group_count += 1;
                if let Some(group) = self.groups.get_mut(index) {
                    group.representative = merge.representative;
                    group.sites.clear();
                } else {
                    self.groups.push(MergeGroup {
                        representative: merge.representative,
                        sites: Vec::new(),
                    });
                }
                self.group_indices.insert(merge.representative, index);
                index
            };
            self.groups[index].sites.push((merge.block, merge.common));
        }

        let Self { groups, commons, tails, .. } = self;
        let mut label_count = 0;
        for group in groups.iter().take(group_count) {
            commons.clear();
            commons.extend(group.sites.iter().map(|&(_, common)| common));
            commons.sort_unstable();
            commons.dedup();
            label_count += commons.len();
        }
        let Some(labels) = labels.take(label_count) else { return false };
        let mut labels = labels.into_iter();
        for group in groups.iter().take(group_count) {
            let representative = &module.blocks[group.representative];
            let instructions = representative.instructions.clone();
            let terminator = representative.terminator.clone();
            let metadata = representative.metadata;
            let max_hot_common = group
                .sites
                .iter()
                .filter(|&&(block, _)| !module.blocks[block].metadata.hotness.is_cold())
                .map(|&(_, common)| common)
                .max();
            commons.clear();
            commons.extend(group.sites.iter().map(|&(_, common)| common));
            commons.sort_unstable();
            commons.dedup();

            tails.clear();
            let mut previous_common = 0;
            let mut previous_tail = None;
            for &common in commons.iter() {
                let mut tail = Block::new(labels.next().expect("reserved one label per tail"));
                tail.metadata = metadata;
                if !metadata.hotness.is_cold()
                    || max_hot_common.is_some_and(|hot_common| common <= hot_common)
                {
                    tail.metadata.hotness = Hotness::Hot;
                }
                tail.instructions = instructions
                    [instructions.len() - common..instructions.len() - previous_common]
                    .to_vec();
                tail.terminator = previous_tail.map_or_else(
                    || terminator.clone(),
                    |target| Some(Terminator::new(TerminatorKind::Jump(target))),
                );
                let tail = module.add_block(tail);
                tails.push((common, tail));
                previous_common = common;
                previous_tail = Some(tail);
            }

            let &(max_common, max_tail) = tails.last().expect("merge group must have a tail");
            module.blocks[group.representative]
                .instructions
                .truncate(instructions.len() - max_common);
            module.blocks[group.representative].terminator =
                Some(Terminator::new(TerminatorKind::Jump(max_tail)));
            for &(block, common) in &group.sites {
                let tail = tails
                    .binary_search_by_key(&common, |&(known, _)| known)
                    .map(|index| tails[index].1)
                    .expect("tail must exist for every merge site");
                let len = module.blocks[block].instructions.len();
                module.blocks[block].instructions.truncate(len - common);
                module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(tail)));
            }
        }
        debug_assert!(labels.next().is_none());
        true
    }
}

fn is_candidate(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
    })
}

fn suffix_size(gcx: Gcx<'_>, module: &Module, block_id: BlockId, common: usize) -> usize {
    let block = &module.blocks[block_id];
    let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
    terminator_lower_bound(gcx, module, block_id, terminator)
        + block.instructions[block.instructions.len() - common..]
            .iter()
            .map(|inst| match inst.as_stack_op() {
                Some(StackOp::Exchange(_, ..=16)) => 3,
                _ => instruction_size_lower_bound(gcx, inst),
            })
            .sum::<usize>()
}

fn terminator_lower_bound(
    gcx: Gcx<'_>,
    module: &Module,
    block_id: BlockId,
    kind: &TerminatorKind,
) -> usize {
    let TerminatorKind::Jump(target) = kind else { return 1 };
    let next = block_id
        .index()
        .checked_add(1)
        .filter(|&index| index < module.blocks.len())
        .map(BlockId::from_usize);
    if Some(*target) == next {
        0
    } else {
        push_len(gcx.sess.opts.evm_version, alloy_primitives::U256::ZERO) + 1
    }
}

#[derive(Clone, Copy)]
struct Merge {
    representative: BlockId,
    block: BlockId,
    common: usize,
}

struct MergeGroup {
    representative: BlockId,
    sites: Vec<(BlockId, usize)>,
}

#[derive(Default)]
struct TailNode {
    children: FxHashMap<MachineInstKey, usize>,
    representative: Option<BlockId>,
}

impl TailNode {
    fn clear(&mut self) {
        self.children.clear();
        self.representative = None;
    }
}
