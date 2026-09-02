//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, push, and terminator reference together.

use super::compact_pushes::selected_len;
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, PushValue, TerminatorKind},
    op,
};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashSet,
};
use solar_sema::Gcx;

/// The machine-level identity shared by transforms that compare instructions.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct MachineInstKey(u8, u8, Option<PushValue>, Option<op::StackOp>);

impl MachineInstKey {
    pub(super) fn new(inst: &Instruction) -> Self {
        Self(inst.opcode, inst.encoding, inst.value, inst.as_stack_op())
    }
}

/// Allocates unused textual block labels without assuming labels are dense.
pub(super) struct FreshLabels {
    occupied: FxHashSet<u32>,
    next: u32,
}

impl FreshLabels {
    pub(super) fn new(module: &Module) -> Self {
        let occupied = module.blocks.iter().map(|block| block.label).collect::<FxHashSet<_>>();
        let next =
            occupied.iter().copied().max().and_then(|label| label.checked_add(1)).unwrap_or(0);
        Self { occupied, next }
    }

    /// Reserves `count` labels before a transform mutates the module.
    pub(super) fn take(&mut self, count: usize) -> Option<Vec<u32>> {
        (0..count).map(|_| self.next()).collect()
    }

    fn next(&mut self) -> Option<u32> {
        let start = self.next;
        loop {
            let label = self.next;
            self.next = self.next.wrapping_add(1);
            if self.occupied.insert(label) {
                return Some(label);
            }
            if self.next == start {
                return None;
            }
        }
    }
}

/// Returns a conservative lower bound for one instruction's assembled byte length.
pub(super) fn instruction_size_lower_bound(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    if !inst.is_encoded_push() {
        return inst.as_stack_op().map_or(1, |stack_op| {
            stack_op
                .assembled_len(gcx.sess.opts.evm_version)
                .expect("EVM IR passes only run on target-compatible stack operations")
        });
    }
    if let Some(type_size) = inst.immutable_type_size() {
        return usize::from(type_size.bytes()) + 1;
    }
    if inst.deferred_push().is_none()
        && let Some(PushValue::Immediate(value)) = inst.value
    {
        return selected_len(gcx, value);
    }
    // Labels, data offsets, and deferred relocations are address-sensitive. They may resolve to
    // zero, so one byte is the only safe lower bound before assembly.
    1
}

/// Returns whether a terminator ends the current physical fallthrough trace.
pub(super) fn is_terminal_boundary(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::IndexedJump(_))
        || matches!(kind, TerminatorKind::Op(opcode) if op::is_terminal(*opcode))
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
    let mut remap = index_vec![None; module.blocks.len()];
    let mut old_blocks =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect::<IndexVec<BlockId, _>>();
    let mut blocks = IndexVec::with_capacity(order.len());
    for &old_block in order {
        let block =
            old_blocks[old_block].take().expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block] = Some(new_block);
    }
    module.blocks = blocks;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Block(block)) = &mut inst.value {
                *block = remap[*block].expect("referenced block must be retained");
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &IndexVec<BlockId, Option<BlockId>>) {
    kind.visit_targets_mut(|target| {
        *target = remap[*target].expect("terminator target must be retained");
    });
}
