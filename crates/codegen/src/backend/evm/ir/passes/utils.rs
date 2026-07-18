//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, operand, and terminator reference together.

use crate::backend::evm::{
    assembler::op,
    ir::{Block, BlockId, Module, Operand, TerminatorKind},
};
use solar_data_structures::index::IndexVec;

pub(super) fn is_evm_terminal(kind: &TerminatorKind) -> bool {
    matches!(
        kind,
        TerminatorKind::Return { .. }
            | TerminatorKind::Revert { .. }
            | TerminatorKind::Stop
            | TerminatorKind::Invalid
            | TerminatorKind::SelfDestruct { .. }
    ) || matches!(kind, TerminatorKind::RawOpcode(opcode) if op::is_terminal(*opcode))
}

pub(in crate::backend::evm::ir) fn remap_block_order(module: &mut Module, order: &[BlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    let mut remap = vec![BlockId::from_usize(0); module.blocks.len()];
    let mut old_blocks: Vec<Option<Block>> =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect();
    let mut blocks = IndexVec::with_capacity(old_blocks.len());
    for &old_block in order {
        let block = old_blocks[old_block.index()]
            .take()
            .expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block.index()] = new_block;
    }
    debug_assert!(old_blocks.into_iter().all(|block| block.is_none()));
    module.blocks = blocks;
    module.entry_block = module.entry_block.map(|block| remap[block.index()]);
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            for operand in &mut inst.operands {
                remap_operand_blocks(operand, &remap);
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_operand_blocks(operand: &mut Operand, remap: &[BlockId]) {
    if let Operand::Block(block) = operand {
        *block = remap[block.index()];
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &[BlockId]) {
    visit_terminator_targets_mut(kind, |target| *target = remap[target.index()]);
}

fn visit_terminator_targets_mut(kind: &mut TerminatorKind, mut visit: impl FnMut(&mut BlockId)) {
    match kind {
        TerminatorKind::Jump(target) => visit(target),
        TerminatorKind::Branch { then_block, else_block, .. } => {
            visit(then_block);
            visit(else_block);
        }
        TerminatorKind::Switch { default, cases, .. } => {
            visit(default);
            for (_, target) in cases {
                visit(target);
            }
        }
        TerminatorKind::Return { .. }
        | TerminatorKind::Revert { .. }
        | TerminatorKind::Stop
        | TerminatorKind::Invalid
        | TerminatorKind::SelfDestruct { .. }
        | TerminatorKind::RawOpcode(_) => {}
    }
}
