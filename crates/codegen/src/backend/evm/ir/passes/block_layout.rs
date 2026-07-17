//! EVM basic block trace layout.
//!
//! The IR keeps control-flow edges explicit and leaves physical fallthrough to
//! assembly. This pass follows unconditional jump successors to form linear
//! traces, making those successor blocks adjacent whenever possible. The
//! assembler can then omit jumps whose target is the next emitted block without
//! encoding physical layout assumptions in the IR.

use super::utils::remap_block_order;
use crate::backend::evm::ir::{Block, BlockId, Module, TerminatorKind};
use solar_data_structures::bit_set::DenseBitSet;

pub(super) fn run(module: &mut Module) -> bool {
    let mut predecessor_counts = vec![0usize; module.blocks.len()];
    for block in &module.blocks {
        if let Some(target) = layout_successor(block)
            && target.index() < predecessor_counts.len()
        {
            predecessor_counts[target.index()] += 1;
        }
    }

    let mut order = Vec::with_capacity(module.blocks.len());
    let mut placed = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.entry_block {
        append_layout_trace(module, entry, &mut placed, &mut order);
    }
    for block in module.blocks.indices() {
        if predecessor_counts[block.index()] == 0 {
            append_layout_trace(module, block, &mut placed, &mut order);
        }
    }
    for block in module.blocks.indices() {
        append_layout_trace(module, block, &mut placed, &mut order);
    }

    if order.iter().copied().eq(module.blocks.indices()) {
        return false;
    }
    remap_block_order(module, &order);
    true
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
