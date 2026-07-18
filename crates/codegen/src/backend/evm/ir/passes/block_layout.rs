//! EVM basic block trace layout.
//!
//! The IR keeps control-flow edges explicit and leaves physical fallthrough to
//! assembly. This pass follows unconditional jump successors to form linear
//! traces, making those successor blocks adjacent whenever possible. The
//! assembler can then omit jumps whose target is the next emitted block without
//! encoding physical layout assumptions in the IR. Independent hot traces are
//! placed before cold terminal traces so unlikely exit paths do not interrupt
//! hot code.

use super::utils::{
    is_evm_terminal, remap_block_order, visit_terminator_operands, visit_terminator_targets,
};
use crate::backend::evm::ir::{Block, BlockId, Instruction, Module, Operand, TerminatorKind};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;

pub(super) fn run(module: &mut Module, options: super::PassOptions) -> bool {
    let mut predecessor_counts = vec![0usize; module.blocks.len()];
    for block in &module.blocks {
        if let Some(target) = layout_successor(block)
            && target.index() < predecessor_counts.len()
        {
            predecessor_counts[target.index()] += 1;
        }
    }

    let original_order: Vec<_> = module.blocks.indices().collect();
    let mut order = Vec::with_capacity(original_order.len());
    let mut placed = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.entry_block {
        append_layout_trace(module, entry, &mut placed, &mut order);
    }
    for cold in [false, true] {
        for &block in &original_order {
            if predecessor_counts[block.index()] == 0
                && is_cold_terminal_block(&module.blocks[block]) == cold
            {
                append_layout_trace(module, block, &mut placed, &mut order);
            }
        }
    }

    pack_hot_terminal_blocks(module, &mut order, options);
    for cold in [false, true] {
        for &block in &original_order {
            if is_cold_terminal_block(&module.blocks[block]) == cold {
                append_layout_trace(module, block, &mut placed, &mut order);
            }
        }
    }

    if order == original_order {
        return false;
    }
    remap_block_order(module, &order);
    true
}

fn pack_hot_terminal_blocks(
    module: &Module,
    order: &mut Vec<BlockId>,
    options: super::PassOptions,
) {
    let Some(first_terminal) = order.iter().enumerate().position(|(position, &block)| {
        is_physical_terminal_boundary(&module.blocks[block], order.get(position + 1).copied())
    }) else {
        return;
    };
    let insert_at = first_terminal + 1;
    let references = block_reference_counts(module, order);
    let insert_offset: usize = order[..insert_at]
        .iter()
        .enumerate()
        .map(|(index, &block)| {
            estimated_block_size(
                &module.blocks[block],
                order.get(index + 1).copied(),
                references[block.index()] != 0,
                options,
            )
        })
        .sum();
    if insert_offset >= 0xff {
        return;
    }

    struct Candidate {
        block: BlockId,
        position: usize,
        size: usize,
        references: usize,
    }
    let mut candidates = Vec::new();
    for position in insert_at..order.len() {
        let block = order[position];
        if position == 0
            || !is_physical_terminal_boundary(&module.blocks[order[position - 1]], Some(block))
            || !is_terminal_block(&module.blocks[block])
        {
            continue;
        }
        let size = estimated_block_size(
            &module.blocks[block],
            order.get(position + 1).copied(),
            references[block.index()] != 0,
            options,
        );
        let count = references[block.index()];
        if size <= 32 && count >= 2 {
            candidates.push(Candidate { block, position, size, references: count });
        }
    }
    candidates.sort_by(|a, b| {
        (b.references * a.size)
            .cmp(&(a.references * b.size))
            .then(b.references.cmp(&a.references))
            .then(a.position.cmp(&b.position))
    });
    let mut budget = 0xff_usize.saturating_sub(insert_offset);
    let mut picked = Vec::new();
    for candidate in candidates {
        if candidate.size <= budget {
            budget -= candidate.size;
            picked.push(candidate.block);
        }
    }
    if picked.is_empty() {
        return;
    }
    order.retain(|block| !picked.contains(block));
    order.splice(insert_at..insert_at, picked);
}

fn block_reference_counts(module: &Module, order: &[BlockId]) -> Vec<usize> {
    let mut references = vec![0usize; module.blocks.len()];
    for (position, &block_id) in order.iter().enumerate() {
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            for operand in &inst.operands {
                count_block_operand(operand, &mut references);
            }
        }
        if let Some(term) = &block.terminator {
            if !matches!(
                &term.kind,
                TerminatorKind::Jump(target) if order.get(position + 1) == Some(target)
            ) {
                visit_terminator_targets(&term.kind, |target| references[target.index()] += 1);
            }
            visit_terminator_operands(&term.kind, |operand| {
                count_block_operand(operand, &mut references);
            });
        }
    }
    references
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
