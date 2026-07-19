//! Merge profitable suffixes of machine-level terminal blocks.

use super::utils::is_evm_terminal;
use crate::backend::evm::ir::{
    Block, BlockId, Instruction, Module, Operand, Terminator, TerminatorKind,
};
use solar_data_structures::map::FxHashMap;

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    let mut changed = false;
    let mut next_label = module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .expect("EVM IR block label overflow");
    loop {
        let merges = plan_merges(module);
        if merges.is_empty() {
            break;
        }
        apply_merges(module, merges, &mut next_label);
        changed = true;
    }
    changed
}

fn plan_merges(module: &Module) -> Vec<Merge> {
    let mut roots = FxHashMap::<TerminatorKind, usize>::default();
    let mut nodes = Vec::<SuffixNode>::new();
    let mut merges = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if !block.entry_stack.is_empty()
            || block.instructions.iter().any(|inst| inst.result.is_some())
        {
            continue;
        }
        let Some(kind) = block
            .terminator
            .as_ref()
            .map(|term| &term.kind)
            .filter(|kind| is_evm_terminal(kind) || matches!(kind, TerminatorKind::Jump(_)))
        else {
            continue;
        };
        let node = if let Some(&node) = roots.get(kind) {
            node
        } else {
            let node = nodes.len();
            nodes.push(SuffixNode::default());
            roots.insert(kind.clone(), node);
            node
        };
        let mut node = node;
        let mut size = terminator_lower_bound(kind);
        let mut path = Vec::with_capacity(block.instructions.len());
        let mut matched = None;
        for (depth, inst) in block.instructions.iter().rev().enumerate() {
            size += instruction_lower_bound(inst);
            let key = MachineInstKey::new(inst);
            node = if let Some(&child) = nodes[node].children.get(&key) {
                child
            } else {
                let child = nodes.len();
                nodes.push(SuffixNode::default());
                nodes[node].children.insert(key, child);
                child
            };
            path.push(node);
            if let Some(representative) = nodes[node].representative {
                matched = Some((representative, depth + 1, size));
            }
        }
        if let Some((representative, common, size)) = matched
            && size > 5
        {
            merges.push(Merge { representative, block: block_id, common });
        } else {
            for node in path {
                nodes[node].representative.get_or_insert(block_id);
            }
        }
    }
    merges
}

fn apply_merges(module: &mut Module, merges: Vec<Merge>, next_label: &mut u32) {
    let mut group_indices = FxHashMap::<BlockId, usize>::default();
    let mut groups = Vec::<MergeGroup>::new();
    for merge in merges {
        let index = if let Some(&index) = group_indices.get(&merge.representative) {
            index
        } else {
            let index = groups.len();
            groups.push(MergeGroup { representative: merge.representative, sites: Vec::new() });
            group_indices.insert(merge.representative, index);
            index
        };
        groups[index].sites.push((merge.block, merge.common));
    }

    for group in groups {
        let representative = &module.blocks[group.representative];
        let instructions = representative.instructions.clone();
        let terminator = representative.terminator.clone();
        let metadata = representative.metadata;
        let mut commons: Vec<_> = group.sites.iter().map(|&(_, common)| common).collect();
        commons.sort_unstable();
        commons.dedup();

        let mut tails = Vec::with_capacity(commons.len());
        let mut previous_common = 0;
        let mut previous_tail = None;
        for &common in &commons {
            let mut tail = Block::new(*next_label);
            *next_label = next_label.checked_add(1).expect("EVM IR block label overflow");
            tail.metadata = metadata;
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
        module.blocks[group.representative].instructions.truncate(instructions.len() - max_common);
        module.blocks[group.representative].terminator =
            Some(Terminator::new(TerminatorKind::Jump(max_tail)));
        for (block, common) in group.sites {
            let tail = tails
                .iter()
                .find_map(|&(known, tail)| (known == common).then_some(tail))
                .expect("tail must exist for every merge site");
            let len = module.blocks[block].instructions.len();
            module.blocks[block].instructions.truncate(len - common);
            module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(tail)));
        }
    }
}

fn terminator_lower_bound(kind: &TerminatorKind) -> usize {
    if matches!(kind, TerminatorKind::Jump(_)) { 3 } else { 1 }
}

fn instruction_lower_bound(inst: &Instruction) -> usize {
    if inst.is_encoded_push() { 2 } else { 1 }
}

#[derive(Default)]
struct SuffixNode {
    children: FxHashMap<MachineInstKey, usize>,
    representative: Option<BlockId>,
}

#[derive(PartialEq, Eq, Hash)]
struct MachineInstKey {
    opcode: u8,
    encoding: u8,
    operands: Vec<Operand>,
}

impl MachineInstKey {
    fn new(inst: &Instruction) -> Self {
        Self { opcode: inst.opcode, encoding: inst.encoding, operands: inst.operands.clone() }
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
