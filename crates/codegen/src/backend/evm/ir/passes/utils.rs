//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, push, and terminator reference together.

use crate::backend::evm::{
    ir::{BlockId, Module, TerminatorKind},
    op,
};

pub(super) fn is_evm_terminal(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::Op(opcode) if op::is_terminal(*opcode))
}

pub(in crate::backend::evm::ir) fn remap_block_order(module: &mut Module, order: &[BlockId]) {
    debug_assert_eq!(order.len(), module.block_count());
    module.retain_blocks(order);
}

pub(super) fn retain_blocks(module: &mut Module, order: &[BlockId]) {
    debug_assert!(order.len() <= module.block_count());
    module.retain_blocks(order);
}
