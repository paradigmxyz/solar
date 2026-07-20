//! Outline repeated closed computations and large immediate pushes.

use crate::backend::evm::{
    ir::{Block, BlockId, Instruction, Module, Operand, Terminator, TerminatorKind},
    opcode as op,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::hash::{Hash, Hasher};

const MIN_CLOSED_RUN: usize = 4;

pub(super) fn run(module: &mut Module, options: super::PassOptions) -> bool {
    let mut state = RunState::default();
    outline_closed_computations(module, &mut state)
        | outline_repeated_pushes(module, options, &mut state)
}

fn outline_closed_computations(module: &mut Module, state: &mut RunState) -> bool {
    let mut candidates = FxHashMap::<MachineInstSlice<'_>, SmallVec<[Site; 2]>>::default();
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
                    let key = MachineInstSlice(&block.instructions[start..=end]);
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
    groups.sort_unstable_by_key(|(key, sites)| {
        let first = sites[0];
        (std::cmp::Reverse(key.0.len()), first.block.index(), first.start)
    });
    let mut claimed: Vec<_> = module
        .blocks
        .iter()
        .map(|block| DenseBitSet::new_empty(block.instructions.len()))
        .collect();
    let mut chosen = Vec::new();
    for (_, sites) in groups {
        let mut free = SmallVec::<[Site; 2]>::new();
        for site in sites {
            if !claimed[site.block.index()].contains_any(site.start..site.start + site.len) {
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
            claimed[site.block.index()].insert_range(site.start..site.start + site.len);
        }
        chosen.push(ChosenGroup { body, sites: free, height: first.height });
    }
    if chosen.is_empty() {
        return false;
    }

    let mut stubs = Vec::with_capacity(chosen.len());
    for group in &chosen {
        let mut stub = Block::new(state.next_label(module));
        stub.instructions = group.body.clone();
        if group.height == 1 {
            stub.instructions.push(Instruction::opcode(op::SWAP1));
        }
        stub.terminator = Some(Terminator::new(TerminatorKind::RawOpcode(op::JUMP)));
        stubs.push(module.add_block(stub));
    }
    let original_blocks = claimed.len();
    let mut edits = vec![SmallVec::<[_; 1]>::new(); original_blocks];
    for (group, stub) in chosen.into_iter().zip(stubs) {
        for site in group.sites {
            edits[site.block.index()].push((site.start, site.len, stub));
        }
    }
    apply_outline_edits(module, edits, state);
    true
}

fn outline_repeated_pushes(
    module: &mut Module,
    options: super::PassOptions,
    state: &mut RunState,
) -> bool {
    let mut sites = FxHashMap::<U256, SmallVec<[(BlockId, usize); 2]>>::default();
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
    let mut edits = vec![SmallVec::<[_; 1]>::new(); original_blocks];
    for value in values {
        let mut stub = Block::new(state.next_label(module));
        stub.instructions.push(Instruction::push(Operand::Immediate(value)));
        stub.instructions.push(Instruction::opcode(op::SWAP1));
        stub.terminator = Some(Terminator::new(TerminatorKind::RawOpcode(op::JUMP)));
        let stub = module.add_block(stub);
        for &(block, index) in &sites[&value] {
            edits[block.index()].push((index, 1, stub));
        }
    }
    apply_outline_edits(module, edits, state);
    true
}

fn apply_outline_edits(
    module: &mut Module,
    mut edits: Vec<SmallVec<[(usize, usize, BlockId); 1]>>,
    state: &mut RunState,
) {
    for (block, edits) in edits.iter_mut().enumerate() {
        edits.sort_unstable_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        let block = BlockId::from_usize(block);
        for &(start, len, stub) in edits.iter() {
            split_outline_site(module, block, start, len, stub, state);
        }
    }
}

fn split_outline_site(
    module: &mut Module,
    block: BlockId,
    start: usize,
    len: usize,
    stub: BlockId,
    state: &mut RunState,
) {
    let mut continuation = Block::new(state.next_label(module));
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

#[derive(Clone, Copy, Debug)]
struct Site {
    block: BlockId,
    start: usize,
    len: usize,
    height: u16,
}

struct ChosenGroup {
    body: Vec<Instruction>,
    sites: SmallVec<[Site; 2]>,
    height: u16,
}

#[derive(Clone, Copy)]
struct MachineInstSlice<'a>(&'a [Instruction]);

impl PartialEq for MachineInstSlice<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().zip(other.0).all(|(a, b)| {
                a.opcode == b.opcode && a.encoding == b.encoding && a.operands == b.operands
            })
    }
}

impl Eq for MachineInstSlice<'_> {}

impl Hash for MachineInstSlice<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        for inst in self.0 {
            inst.opcode.hash(state);
            inst.encoding.hash(state);
            inst.operands.hash(state);
        }
    }
}

#[derive(Default)]
struct RunState {
    next_label: Option<u32>,
}

impl RunState {
    fn next_label(&mut self, module: &Module) -> u32 {
        let label = *self.next_label.get_or_insert_with(|| {
            module
                .blocks
                .iter()
                .map(|block| block.label)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .expect("EVM IR block label overflow")
        });
        self.next_label = Some(label.checked_add(1).expect("EVM IR block label overflow"));
        label
    }
}
