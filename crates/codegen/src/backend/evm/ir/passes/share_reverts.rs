//! Share adjacent empty revert paths in physically laid-out EVM IR.
//!
//! A branch that falls through to an empty `REVERT` and jumps over it on success can invert the
//! branch: the failure edge jumps to one shared empty revert while success falls through to its
//! continuation. The pass recognizes both `PUSH0; PUSH0; REVERT` and the legacy `PUSH0; DUP1;
//! REVERT` spelling. It preserves the original layout when moving a frequently referenced shared
//! revert could widen its target push and lose more bytes than the removed jump saves.
//!
//! Inverting the branch can need an extra `ISZERO` before the branch target, which runs between
//! the condition and the jump. That boundary must be one `keep_with_next` allows to be disturbed,
//! so a sequence whose intervening gas is observable is left alone.

use super::{EvmPass, utils::is_split_point};
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, PushValue, Terminator, TerminatorKind},
    op,
};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;
use solar_sema::Gcx;

pub(super) struct ShareReverts;

impl EvmPass for ShareReverts {
    fn name(&self) -> &'static str {
        "share-reverts"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        share_reverts(gcx, module)
    }
}

fn share_reverts(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut empty_reverts = DenseBitSet::new_empty(module.blocks.len());
    for block in module.blocks.indices().filter(|&block| is_empty_revert(module, block)) {
        empty_reverts.insert(block);
    }
    let Some(shared) = empty_reverts.iter().next() else {
        return false;
    };
    if preserves_shared_revert_low_address(module, shared) {
        return false;
    }
    let mut changed = false;
    for (index, block) in module.blocks.iter_mut().enumerate() {
        let block_id = BlockId::from_usize(index);
        let Some(revert) = block.terminator.as_ref().and_then(|term| match term.kind {
            TerminatorKind::Jump(target) => Some(target),
            _ => None,
        }) else {
            continue;
        };
        if !empty_reverts.contains(revert) {
            continue;
        }
        let [.., target, jumpi] = block.instructions.as_slice() else { continue };
        let Some(PushValue::Block(continuation)) = target.value else { continue };
        if jumpi.opcode != op::JUMPI
            || jumpi.is_encoded_push()
            || !target.is_encoded_push()
            || revert.index() != block_id.index() + 1
            || continuation.index() != revert.index() + 1
        {
            continue;
        }
        // Inverting the branch drops an `ISZERO`, retargets an `EQ`, or inserts an `ISZERO`
        // before the branch target. Dropping and inserting change what runs at that boundary, so
        // both need a boundary `keep_with_next` allows to be disturbed.
        let condition_end = block.instructions.len() - 2;
        let condition =
            block.instructions.get(condition_end.wrapping_sub(1)).map(|inst| inst.opcode);
        let boundary =
            if condition == Some(op::ISZERO) { condition_end - 1 } else { condition_end };
        if condition != Some(op::EQ) && !is_split_point(&block.instructions, boundary) {
            continue;
        }
        // <condition>
        // iszero
        // push <shared>
        // jump <continuation>
        block.instructions[condition_end] = Instruction::push_block(shared);
        block.terminator = Some(Terminator::new(TerminatorKind::Jump(continuation)));
        match condition {
            Some(op::ISZERO) => {
                block.instructions.remove(condition_end - 1);
            }
            Some(op::EQ) => block.instructions[condition_end - 1].opcode = op::SUB,
            _ => block.instructions.insert(condition_end, Instruction::opcode(op::ISZERO)),
        }
        changed = true;
    }
    changed
}

fn preserves_shared_revert_low_address(module: &Module, shared: BlockId) -> bool {
    // Inverting the branch can remove the early unconditional jump that lets
    // layout keep a frequently referenced revert below the PUSH1 boundary.
    let block_size = |block: &crate::backend::evm::ir::Block| {
        1 + block
            .instructions
            .iter()
            .map(|inst| if inst.is_encoded_push() { 2 } else { 1 })
            .sum::<usize>()
            + block
                .terminator
                .as_ref()
                .map_or(0, |term| if matches!(term.kind, TerminatorKind::Jump(_)) { 3 } else { 1 })
    };
    let mut references = 0;
    let mut shared_end = 0;
    let mut total = 0;
    for (block_id, block) in module.blocks.iter_enumerated() {
        references += block
            .instructions
            .iter()
            .filter(|inst| matches!(inst.value, Some(PushValue::Block(target)) if target == shared))
            .count();
        total += block_size(block);
        if block_id == shared {
            shared_end = total;
        }
    }
    if references < 2 {
        return false;
    }
    shared_end <= 0xff && total > 0xff
}

fn is_empty_revert(module: &Module, block: BlockId) -> bool {
    let block = &module.blocks[block];
    let [zero, second] = block.instructions.as_slice() else { return false };
    is_zero_push(zero)
        && (second.as_stack_op() == Some(op::StackOp::Dup(1)) || is_zero_push(second))
        && matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::REVERT))
        )
}

fn is_zero_push(inst: &Instruction) -> bool {
    inst.is_encoded_push()
        && inst.deferred_push().is_none()
        && inst.immutable_push().is_none()
        && matches!(inst.value, Some(PushValue::Immediate(value)) if value == U256::ZERO)
}
