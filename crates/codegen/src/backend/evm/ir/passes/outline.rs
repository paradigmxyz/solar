//! Outline repeated closed computations and large immediate pushes.

use crate::backend::evm::{
    ir::{Block, BlockId, Instruction, Module, Operand, Terminator, TerminatorKind},
    opcode as op,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

const MIN_CLOSED_RUN: usize = 4;

pub(super) fn run(module: &mut Module, options: super::PassOptions) -> bool {
    outline_closed_computations(module) | outline_repeated_pushes(module, options)
}

fn outline_closed_computations(module: &mut Module) -> bool {
    let mut candidates = FxHashMap::<Vec<MachineInstKey>, Vec<Site>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if !block.entry_stack.is_empty() {
            continue;
        }
        for start in 0..block.instructions.len() {
            let mut height = 0i32;
            for end in start..block.instructions.len() {
                let inst = &block.instructions[end];
                let Some((reads, pops, pushes)) = whitelisted_effect(inst) else { break };
                if height < i32::from(reads) {
                    break;
                }
                height = height - i32::from(pops) + i32::from(pushes);
                let len = end + 1 - start;
                if len >= MIN_CLOSED_RUN && matches!(height, 0 | 1) {
                    let key =
                        block.instructions[start..=end].iter().map(MachineInstKey::new).collect();
                    candidates.entry(key).or_default().push(Site {
                        block: block_id,
                        start,
                        len,
                        height: height as u16,
                    });
                }
            }
        }
    }

    let mut groups: Vec<_> = candidates.into_iter().filter(|(_, sites)| sites.len() >= 2).collect();
    groups.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
    let mut claimed: Vec<_> =
        module.blocks.iter().map(|block| vec![false; block.instructions.len()]).collect();
    let mut chosen = Vec::new();
    for (_, sites) in groups {
        let mut free = Vec::new();
        for site in sites {
            if (site.start..site.start + site.len).all(|at| !claimed[site.block.index()][at]) {
                free.push(site);
            }
        }
        if free.len() < 2 {
            continue;
        }
        let first = free[0];
        let body =
            module.blocks[first.block].instructions[first.start..first.start + first.len].to_vec();
        let run_size = lower_bound(&body);
        let stub_size = 1 + run_size + usize::from(first.height) + 1;
        let site_size = if free.len() >= 4 { 7 } else { 8 };
        if free.len() * run_size < free.len() * site_size + stub_size + 2 {
            continue;
        }
        for site in &free {
            claimed[site.block.index()][site.start..site.start + site.len].fill(true);
        }
        chosen.push(ChosenGroup { body, sites: free, height: first.height });
    }
    if chosen.is_empty() {
        return false;
    }

    let mut stubs = Vec::with_capacity(chosen.len());
    for group in &chosen {
        let mut stub = Block::new(fresh_label(module));
        stub.instructions = group.body.clone();
        if group.height == 1 {
            stub.instructions.push(Instruction::opcode(op::SWAP1));
        }
        stub.terminator = Some(Terminator::new(TerminatorKind::RawOpcode(op::JUMP)));
        stubs.push(module.add_block(stub));
    }
    let original_blocks = claimed.len();
    let mut edits = vec![Vec::new(); original_blocks];
    for (group, stub) in chosen.into_iter().zip(stubs) {
        for site in group.sites {
            edits[site.block.index()].push((site.start, site.len, stub));
        }
    }
    apply_outline_edits(module, edits);
    true
}

fn outline_repeated_pushes(module: &mut Module, options: super::PassOptions) -> bool {
    let mut sites = FxHashMap::<U256, Vec<(BlockId, usize)>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for (index, inst) in block.instructions.iter().enumerate() {
            if inst.is_encoded_push()
                && !inst.is_deferred_push()
                && !inst.is_immutable_push()
                && let [Operand::Immediate(value)] = inst.operands.as_slice()
            {
                sites.entry(*value).or_default().push((block_id, index));
            }
        }
    }

    const SITE_BYTES: usize = 8;
    const MIN_SAVING: usize = 8;
    let mut values: Vec<_> = sites
        .iter()
        .filter_map(|(&value, occurrences)| {
            let push_len = push_len(value, options);
            let inline = occurrences.len() * push_len;
            let outlined = occurrences.len() * SITE_BYTES + push_len + 3;
            (occurrences.len() >= 2 && inline >= outlined + MIN_SAVING).then_some(value)
        })
        .collect();
    if values.is_empty() {
        return false;
    }
    values.sort_unstable();

    let original_blocks = module.blocks.len();
    let mut edits = vec![Vec::new(); original_blocks];
    for value in values {
        let mut stub = Block::new(fresh_label(module));
        stub.instructions.push(Instruction::push(Operand::Immediate(value)));
        stub.instructions.push(Instruction::opcode(op::SWAP1));
        stub.terminator = Some(Terminator::new(TerminatorKind::RawOpcode(op::JUMP)));
        let stub = module.add_block(stub);
        for &(block, index) in &sites[&value] {
            edits[block.index()].push((index, 1, stub));
        }
    }
    apply_outline_edits(module, edits);
    true
}

fn apply_outline_edits(module: &mut Module, mut edits: Vec<Vec<(usize, usize, BlockId)>>) {
    for (block, edits) in edits.iter_mut().enumerate() {
        edits.sort_unstable_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        let block = BlockId::from_usize(block);
        for &(start, len, stub) in edits.iter() {
            split_outline_site(module, block, start, len, stub);
        }
    }
}

fn split_outline_site(
    module: &mut Module,
    block: BlockId,
    start: usize,
    len: usize,
    stub: BlockId,
) {
    let mut continuation = Block::new(fresh_label(module));
    continuation.metadata = module.blocks[block].metadata;
    continuation.instructions = module.blocks[block].instructions.split_off(start + len);
    module.blocks[block].instructions.truncate(start);
    continuation.terminator = module.blocks[block].terminator.take();
    let continuation = module.add_block(continuation);
    module.blocks[block].instructions.push(Instruction::push(Operand::Block(continuation)));
    module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(stub)));
}

fn whitelisted_effect(inst: &Instruction) -> Option<(u16, u16, u16)> {
    if inst.result.is_some() {
        return None;
    }
    if inst.is_encoded_push() {
        return Some((0, 0, 1));
    }
    Some(match inst.opcode {
        op::CALLDATASIZE | op::PUSH0 | op::RETURNDATASIZE | op::MSIZE | op::CALLVALUE => (0, 0, 1),
        op::ISZERO | op::NOT | op::CALLDATALOAD | op::MLOAD => (1, 1, 1),
        op::ADD
        | op::SUB
        | op::MUL
        | op::AND
        | op::OR
        | op::XOR
        | op::SHL
        | op::SHR
        | op::LT
        | op::GT
        | op::SLT
        | op::SGT
        | op::EQ
        | op::DIV => (2, 2, 1),
        op::MSTORE => (2, 2, 0),
        op::POP => (1, 1, 0),
        dup if (op::DUP1..=op::DUP16).contains(&dup) => (u16::from(dup - op::DUP1) + 1, 0, 1),
        swap if (op::SWAP1..=op::SWAP16).contains(&swap) => (u16::from(swap - op::SWAP1) + 2, 0, 0),
        _ => return None,
    })
}

fn lower_bound(instructions: &[Instruction]) -> usize {
    instructions.iter().map(|inst| if inst.is_encoded_push() { 2 } else { 1 }).sum()
}

fn push_len(value: U256, options: super::PassOptions) -> usize {
    let width = value.byte_len();
    if width == 0 && !options.evm_version.has_push0() { 2 } else { width + 1 }
}

fn fresh_label(module: &Module) -> u32 {
    module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .expect("EVM IR block label overflow")
}

#[derive(Clone, Copy, Debug)]
struct Site {
    block: BlockId,
    start: usize,
    len: usize,
    height: u16,
}

struct ChosenGroup {
    body: Vec<Instruction>,
    sites: Vec<Site>,
    height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
