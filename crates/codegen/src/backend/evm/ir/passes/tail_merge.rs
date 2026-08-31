//! Merge profitable suffixes of machine-level terminal blocks.

use super::{
    EvmPass,
    utils::{
        FreshLabels, MachineInstKey, StackDepths, instruction_size_lower_bound,
        is_terminal_boundary, relative_stack_depths,
    },
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
    representatives: Vec<BlockId>,
    merges: Vec<Merge>,
    group_indices: FxHashMap<BlockId, usize>,
    groups: Vec<MergeGroup>,
    commons: Vec<usize>,
    tails: Vec<(usize, BlockId)>,
}

impl RunState {
    fn plan_merges(&mut self, gcx: Gcx<'_>, module: &Module) {
        let depths = StackDepths::new(module);
        self.representatives.clear();
        self.merges.clear();
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !is_candidate(block) {
                continue;
            }

            let mut matched = None;
            for &representative in &self.representatives {
                let representative_block = &module.blocks[representative];
                if block.terminator.as_ref().map(|term| &term.kind)
                    != representative_block.terminator.as_ref().map(|term| &term.kind)
                {
                    continue;
                }
                let common = common_suffix(block, representative_block);
                if common > matched.map_or(0, |(_, common)| common) {
                    matched = Some((representative, common));
                }
            }

            if let Some((representative, common)) = matched
                && common > 0
                && suffix_size(gcx, module, block_id, common) > 5
                && split_has_jump_headroom(depths.as_ref(), module, representative, common)
                && split_has_jump_headroom(depths.as_ref(), module, block_id, common)
            {
                self.merges.push(Merge { representative, block: block_id, common });
            } else {
                self.representatives.push(block_id);
            }
        }
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

fn split_has_jump_headroom(
    depths: Option<&StackDepths>,
    module: &Module,
    block_id: BlockId,
    common: usize,
) -> bool {
    let block = &module.blocks[block_id];
    let split = block.instructions.len() - common;
    let local_headroom = relative_stack_depths(&block.instructions).is_some_and(|depths| {
        depths.get(split).is_some_and(|depth| depths.iter().any(|peak| peak > depth))
    });
    local_headroom || depths.is_some_and(|depths| depths.has_headroom(block_id, split, 1))
}

fn is_candidate(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
    })
}

fn common_suffix(a: &Block, b: &Block) -> usize {
    a.instructions
        .iter()
        .rev()
        .zip(b.instructions.iter().rev())
        .take_while(|(a, b)| MachineInstKey::new(a) == MachineInstKey::new(b))
        .count()
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
