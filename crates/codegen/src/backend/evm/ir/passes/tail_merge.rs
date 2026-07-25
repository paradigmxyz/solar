//! Merge profitable suffixes of machine-level terminal blocks.

use super::{EvmPass, utils::is_evm_terminal};
use crate::backend::evm::ir::{
    Block, BlockId, Hotness, Instruction, Module, PushValue, Terminator, TerminatorKind,
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

fn merge_tails(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    merge_tails_with_state(module, &mut state)
}

pub(super) fn merge_tails_with_state(module: &mut Module, state: &mut RunState) -> bool {
    state.plan_merges(module);
    if state.merges.is_empty() {
        return false;
    }
    let mut next_label = module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .expect("EVM IR block label overflow");
    loop {
        state.apply_merges(module, &mut next_label);
        state.plan_merges(module);
        if state.merges.is_empty() {
            return true;
        }
    }
}

#[derive(Default)]
pub(super) struct RunState {
    roots: FxHashMap<TerminatorKind, usize>,
    nodes: Vec<SuffixNode>,
    merges: Vec<Merge>,
    group_indices: FxHashMap<BlockId, usize>,
    groups: Vec<MergeGroup>,
    commons: Vec<usize>,
    tails: Vec<(usize, BlockId)>,
}

impl RunState {
    fn plan_merges(&mut self, module: &Module) {
        self.roots.clear();
        self.nodes.clear();
        self.merges.clear();
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !is_candidate(block) {
                continue;
            }

            if let Some((representative, common)) = self.find_match(block)
                && common > 0
                && suffix_lower_bound(block, common) > 5
            {
                self.merges.push(Merge { representative, block: block_id, common });
            } else {
                self.insert_representative(block_id, block);
            }
        }
    }

    fn find_match(&self, block: &Block) -> Option<(BlockId, usize)> {
        let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
        let mut node = *self.roots.get(terminator)?;
        let mut matched = None;
        for (common, inst) in block.instructions.iter().rev().enumerate() {
            let Some(&next) = self.nodes[node].children.get(&InstructionKey::new(inst)) else {
                break;
            };
            node = next;
            matched =
                self.nodes[node].representative.map(|representative| (representative, common + 1));
        }
        matched
    }

    fn insert_representative(&mut self, block_id: BlockId, block: &Block) {
        let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
        let mut node = if let Some(&root) = self.roots.get(terminator) {
            root
        } else {
            let root = self.nodes.len();
            self.nodes.push(SuffixNode::default());
            self.roots.insert(terminator.clone(), root);
            root
        };
        for inst in block.instructions.iter().rev() {
            let key = InstructionKey::new(inst);
            node = if let Some(&next) = self.nodes[node].children.get(&key) {
                next
            } else {
                let next = self.nodes.len();
                self.nodes.push(SuffixNode::default());
                self.nodes[node].children.insert(key, next);
                next
            };
            self.nodes[node].representative.get_or_insert(block_id);
        }
    }

    fn apply_merges(&mut self, module: &mut Module, next_label: &mut u32) {
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
                let mut tail = Block::new(*next_label);
                *next_label = next_label.checked_add(1).expect("EVM IR block label overflow");
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
    }
}

fn is_candidate(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_evm_terminal(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
    })
}

#[derive(Default)]
struct SuffixNode {
    children: FxHashMap<InstructionKey, usize>,
    representative: Option<BlockId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct InstructionKey {
    opcode: u8,
    encoding: u8,
    value: Option<PushValue>,
}

impl InstructionKey {
    fn new(inst: &Instruction) -> Self {
        Self { opcode: inst.opcode, encoding: inst.encoding, value: inst.value }
    }
}

fn suffix_lower_bound(block: &Block, common: usize) -> usize {
    let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
    terminator_lower_bound(terminator)
        + block.instructions[block.instructions.len() - common..]
            .iter()
            .map(instruction_lower_bound)
            .sum::<usize>()
}

fn terminator_lower_bound(kind: &TerminatorKind) -> usize {
    if matches!(kind, TerminatorKind::Jump(_)) { 3 } else { 1 }
}

fn instruction_lower_bound(inst: &Instruction) -> usize {
    if inst.is_encoded_push() { 2 } else { 1 }
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
