//! Share adjacent empty revert paths in physically laid-out EVM IR.

use super::EvmPass;
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

    fn is_required(&self) -> bool {
        false
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
        let [.., target, jumpi] = block.instructions.as_mut_slice() else { continue };
        if jumpi.opcode != op::JUMPI || jumpi.is_encoded_push() {
            continue;
        }
        let continuation = match &target.value {
            Some(PushValue::Block(continuation)) => *continuation,
            _ => continue,
        };
        if !target.is_encoded_push() {
            continue;
        }
        if revert.index() != block_id.index() + 1 || continuation.index() != revert.index() + 1 {
            continue;
        }
        *target = Instruction::push_block(shared);
        block.terminator = Some(Terminator::new(TerminatorKind::Jump(continuation)));
        let condition_end = block.instructions.len() - 2;
        match block.instructions.get(condition_end.wrapping_sub(1)).map(|inst| inst.opcode) {
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
    let [zero, dup] = block.instructions.as_slice() else { return false };
    is_zero_push(zero)
        && dup.opcode == op::DUP1
        && !dup.is_encoded_push()
        && matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::REVERT))
        )
}

fn is_zero_push(inst: &Instruction) -> bool {
    (inst.is_encoded_push()
        && inst.deferred_push().is_none()
        && inst.immutable_push().is_none()
        && matches!(inst.value, Some(PushValue::Immediate(value)) if value == U256::ZERO))
        || (inst.opcode == op::PUSH0 && !inst.is_encoded_push())
}
