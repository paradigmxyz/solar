//! EVM basic block trace layout.
//!
//! The IR keeps control-flow edges explicit and leaves physical fallthrough to
//! assembly. This pass follows unconditional jump successors to form linear
//! traces, making those successor blocks adjacent whenever possible. The
//! final lowering can then omit jumps whose target is the next emitted block
//! without encoding physical layout assumptions in the IR. Independent hot
//! traces are placed before cold terminal traces so unlikely exit paths do not
//! interrupt hot code.

use super::utils::{is_evm_terminal, remap_block_order};
use crate::backend::evm::ir::{Block, BlockId, Instruction, Module, Operand, TerminatorKind};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;

pub(super) fn run(module: &mut Module, options: super::PassOptions) -> bool {
    if module.blocks.len() <= 1 {
        return false;
    }
    let mut state = RunState::default();
    state.reset(module.blocks.len());
    for block in &module.blocks {
        if let Some(target) = layout_successor(block)
            && target.index() < state.predecessor_counts.len()
        {
            state.predecessor_counts[target.index()] += 1;
        }
    }

    if let Some(entry) = module.entry_block {
        append_layout_trace(module, entry, &mut state.placed, &mut state.order);
    }
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if state.predecessor_counts[block.index()] == 0
                && is_cold_terminal_block(&module.blocks[block]) == cold
            {
                append_layout_trace(module, block, &mut state.placed, &mut state.order);
            }
        }
    }

    pack_hot_terminal_blocks(module, &mut state, options);
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if is_cold_terminal_block(&module.blocks[block]) == cold {
                append_layout_trace(module, block, &mut state.placed, &mut state.order);
            }
        }
    }

    if state.order.iter().copied().eq(module.blocks.indices()) {
        return false;
    }
    remap_block_order(module, &state.order);
    true
}

struct RunState {
    predecessor_counts: Vec<usize>,
    order: Vec<BlockId>,
    placed: DenseBitSet<BlockId>,
    references: Vec<usize>,
    candidates: Vec<Candidate>,
    picked: DenseBitSet<BlockId>,
    picked_order: Vec<BlockId>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            predecessor_counts: Vec::new(),
            order: Vec::new(),
            placed: DenseBitSet::new_empty(0),
            references: Vec::new(),
            candidates: Vec::new(),
            picked: DenseBitSet::new_empty(0),
            picked_order: Vec::new(),
        }
    }
}

impl RunState {
    fn reset(&mut self, blocks: usize) {
        self.predecessor_counts.clear();
        self.predecessor_counts.resize(blocks, 0);
        self.order.clear();
        if self.order.capacity() < blocks {
            self.order.reserve(blocks);
        }
        if self.placed.domain_size() == blocks {
            self.placed.clear();
            self.picked.clear();
        } else {
            self.placed = DenseBitSet::new_empty(blocks);
            self.picked = DenseBitSet::new_empty(blocks);
        }
        self.references.clear();
        self.references.resize(blocks, 0);
        self.candidates.clear();
        self.picked_order.clear();
    }
}

struct Candidate {
    block: BlockId,
    position: usize,
    size: usize,
    references: usize,
}

fn pack_hot_terminal_blocks(module: &Module, state: &mut RunState, options: super::PassOptions) {
    let Some(first_terminal) = state.order.iter().enumerate().position(|(position, &block)| {
        is_physical_terminal_boundary(&module.blocks[block], state.order.get(position + 1).copied())
    }) else {
        return;
    };
    let insert_at = first_terminal + 1;
    block_reference_counts(module, &state.order, &mut state.references);
    let insert_offset: usize = state.order[..insert_at]
        .iter()
        .enumerate()
        .map(|(index, &block)| {
            estimated_block_size(
                &module.blocks[block],
                state.order.get(index + 1).copied(),
                state.references[block.index()] != 0,
                options,
            )
        })
        .sum();
    if insert_offset >= 0xff {
        return;
    }

    for position in insert_at..state.order.len() {
        let block = state.order[position];
        if position == 0
            || !is_physical_terminal_boundary(
                &module.blocks[state.order[position - 1]],
                Some(block),
            )
            || !is_terminal_block(&module.blocks[block])
        {
            continue;
        }
        let size = estimated_block_size(
            &module.blocks[block],
            state.order.get(position + 1).copied(),
            state.references[block.index()] != 0,
            options,
        );
        let count = state.references[block.index()];
        if size <= 32 && count >= 2 {
            state.candidates.push(Candidate { block, position, size, references: count });
        }
    }
    state.candidates.sort_by(|a, b| {
        (b.references * a.size)
            .cmp(&(a.references * b.size))
            .then(b.references.cmp(&a.references))
            .then(a.position.cmp(&b.position))
    });
    let mut budget = 0xff_usize.saturating_sub(insert_offset);
    for candidate in &state.candidates {
        if candidate.size <= budget {
            budget -= candidate.size;
            state.picked.insert(candidate.block);
            state.picked_order.push(candidate.block);
        }
    }
    if state.picked_order.is_empty() {
        return;
    }
    state.order.retain(|block| !state.picked.contains(*block));
    state.order.splice(insert_at..insert_at, state.picked_order.drain(..));
}

fn block_reference_counts(module: &Module, order: &[BlockId], references: &mut [usize]) {
    for (position, &block_id) in order.iter().enumerate() {
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            for operand in &inst.operands {
                count_block_operand(operand, references);
            }
        }
        if let Some(term) = &block.terminator {
            if !matches!(
                &term.kind,
                TerminatorKind::Jump(target) if order.get(position + 1) == Some(target)
            ) {
                term.kind.visit_targets(|target| references[target.index()] += 1);
            }
            term.kind.visit_operands(|operand| {
                count_block_operand(operand, references);
            });
        }
    }
}

fn count_block_operand(operand: &Operand, references: &mut [usize]) {
    if let Operand::Block(block) = operand {
        references[block.index()] += 1;
    }
}

fn estimated_block_size(
    block: &Block,
    next: Option<BlockId>,
    addressed: bool,
    options: super::PassOptions,
) -> usize {
    usize::from(addressed)
        + block
            .instructions
            .iter()
            .map(|inst| estimated_instruction_size(inst, options))
            .sum::<usize>()
        + block
            .terminator
            .as_ref()
            .map_or(0, |term| estimated_terminator_size(&term.kind, next, options))
}

fn estimated_instruction_size(inst: &Instruction, options: super::PassOptions) -> usize {
    if inst.is_immutable_push() {
        33
    } else if inst.is_deferred_push() {
        3
    } else if inst.is_encoded_push() {
        match inst.operands.as_slice() {
            [Operand::Immediate(value)] => push_len(*value, options),
            [Operand::Block(_)] => 3,
            _ => 1,
        }
    } else {
        1
    }
}

fn estimated_terminator_size(
    kind: &TerminatorKind,
    next: Option<BlockId>,
    options: super::PassOptions,
) -> usize {
    match kind {
        TerminatorKind::Jump(target) => usize::from(Some(*target) != next) * 4,
        TerminatorKind::Stop => usize::from(next.is_some()),
        TerminatorKind::Invalid | TerminatorKind::RawOpcode(_) => 1,
        TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
            operand_push_size(offset, options) + operand_push_size(size, options) + 1
        }
        TerminatorKind::SelfDestruct { recipient } => operand_push_size(recipient, options) + 1,
        TerminatorKind::Branch { .. } | TerminatorKind::Switch { .. } => 0,
    }
}

fn operand_push_size(operand: &Operand, options: super::PassOptions) -> usize {
    match operand {
        Operand::Immediate(value) => push_len(*value, options),
        Operand::Block(_) => 3,
        Operand::Value(_) => 0,
    }
}

fn push_len(value: U256, options: super::PassOptions) -> usize {
    let width = value.byte_len();
    if width == 0 && !options.evm_version.has_push0() { 2 } else { width + 1 }
}

fn is_terminal_block(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| is_evm_terminal(&term.kind))
}

fn is_physical_terminal_boundary(block: &Block, next: Option<BlockId>) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_evm_terminal(&term.kind)
            || matches!(term.kind, TerminatorKind::Jump(target) if Some(target) != next)
    })
}

fn append_layout_trace(
    module: &Module,
    mut block: BlockId,
    placed: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    while block.index() < module.blocks.len() && placed.insert(block) {
        order.push(block);
        let Some(target) = layout_successor(&module.blocks[block]) else { return };
        block = target;
    }
}

fn layout_successor(block: &Block) -> Option<BlockId> {
    match &block.terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Some(*target),
        _ => None,
    }
}

fn is_cold_terminal_block(block: &Block) -> bool {
    block.metadata.hotness.is_cold()
        && block.terminator.as_ref().is_some_and(|term| is_evm_terminal(&term.kind))
}
