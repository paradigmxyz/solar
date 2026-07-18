//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, operand, and terminator reference together.

use crate::backend::evm::{
    ir::{Block, BlockId, Module, Operand, TerminatorKind},
    opcode as op,
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
    remap_blocks(module, order);
}

pub(super) fn retain_blocks(module: &mut Module, order: &[BlockId]) {
    debug_assert!(order.len() <= module.blocks.len());
    remap_blocks(module, order);
}

fn remap_blocks(module: &mut Module, order: &[BlockId]) {
    let mut remap = vec![None; module.blocks.len()];
    let mut old_blocks: Vec<Option<Block>> =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect();
    let mut blocks = IndexVec::with_capacity(order.len());
    for &old_block in order {
        let block = old_blocks[old_block.index()]
            .take()
            .expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block.index()] = Some(new_block);
    }
    module.blocks = blocks;
    module.entry_block =
        module.entry_block.map(|block| remap[block.index()].expect("entry block must be retained"));
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

fn remap_operand_blocks(operand: &mut Operand, remap: &[Option<BlockId>]) {
    if let Operand::Block(block) = operand {
        *block = remap[block.index()].expect("referenced block must be retained");
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &[Option<BlockId>]) {
    kind.visit_targets_mut(|target| {
        *target = remap[target.index()].expect("terminator target must be retained");
    });
    kind.visit_operands_mut(|operand| remap_operand_blocks(operand, remap));
}
