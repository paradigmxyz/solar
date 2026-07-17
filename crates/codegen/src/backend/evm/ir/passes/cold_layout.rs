//! Cold terminal block placement.
//!
//! Cold blocks that terminate EVM execution do not need to occupy positions in
//! the hot block sequence. This pass moves them to the end of the module when
//! the preceding block has an explicit control-flow barrier, preserving the
//! existing control flow while leaving more useful layout opportunities for hot
//! code.

use super::utils::{is_evm_terminal, remap_block_order};
use crate::backend::evm::ir::{Block, BlockId, Module, TerminatorKind};

pub(super) fn run(module: &mut Module) -> bool {
    let mut kept = Vec::with_capacity(module.blocks.len());
    let mut moved = Vec::new();

    for (block_id, block) in module.blocks.iter_enumerated() {
        if is_movable_cold_terminal_block(module, block_id, block) {
            moved.push(block_id);
        } else {
            kept.push(block_id);
        }
    }

    if moved.is_empty() {
        return false;
    }

    kept.extend(moved);
    remap_block_order(module, &kept);
    true
}

fn is_movable_cold_terminal_block(module: &Module, block_id: BlockId, block: &Block) -> bool {
    if module.entry_block == Some(block_id) || block_id.index() == 0 {
        return false;
    }
    let Some(term) = &block.terminator else {
        return false;
    };
    if !block.metadata.hotness.is_cold() || !is_evm_terminal(&term.kind) {
        return false;
    }
    let previous = BlockId::from_usize(block_id.index() - 1);
    module.blocks[previous].terminator.as_ref().is_some_and(|term| is_layout_barrier(&term.kind))
}

fn is_layout_barrier(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::Jump(_)) || is_evm_terminal(kind)
}
