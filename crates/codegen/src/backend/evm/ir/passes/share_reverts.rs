//! Share adjacent empty revert paths in physically laid-out EVM IR.
//!
//! A branch that falls through to an empty `REVERT` and jumps over it on success can invert the
//! branch: the failure edge jumps to one shared empty revert while success falls through to its
//! continuation. The pass recognizes both `PUSH0; PUSH0; REVERT` and the legacy `PUSH0; DUP1;
//! REVERT` spelling. It preserves the original layout when moving a frequently referenced shared
//! revert could widen its target push and lose more bytes than the removed jump saves.
//!
//! An adjacent cold revert with a payload can use the same rewrite when inversion removes an
//! existing `ISZERO`, exchanges `EQ` and `SUB`, or adjusts a constant comparison without
//! increasing its cost. It keeps its own target and payload; successful execution then falls
//! through without the inversion or a continuation label after block layout.
//!
//! Inverting the branch can need an extra `ISZERO` before the branch target, which runs between
//! the condition and the jump. That boundary must be one `keep_with_next` allows to be disturbed,
//! so a sequence whose intervening gas is observable is left alone.

use super::{
    EvmPass, compact_pushes::immediate_materialization_cost, peephole::invert_comparison,
    utils::is_split_point,
};
use crate::{
    backend::evm::{
        ir::{Block, BlockId, Instruction, Module, ModuleTarget, Terminator, TerminatorKind},
        op,
    },
    target::Target,
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

fn share_reverts(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut empty_reverts = DenseBitSet::new_empty(module.blocks.len());
    let mut cold_reverts = DenseBitSet::new_empty(module.blocks.len());
    for (id, block) in module.blocks.iter_enumerated() {
        if is_empty_revert(block) {
            empty_reverts.insert(id);
        } else if block.metadata.hotness.is_cold() && ends_in_revert(block) {
            cold_reverts.insert(id);
        }
    }
    let shared = empty_reverts.iter().next().filter(|&shared| {
        !preserves_shared_revert_low_address(module.runtime_target(gcx), module, shared)
    });
    let mut changed = false;
    for (block_id, block) in module.blocks.iter_mut_enumerated() {
        let Some(term) = &block.terminator else { continue };
        let TerminatorKind::Jump(revert) = term.kind else { continue };
        if revert.index() != block_id.index() + 1 {
            continue;
        }
        let is_empty = empty_reverts.contains(revert);
        let Some(target_revert) =
            (if is_empty { shared } else { cold_reverts.contains(revert).then_some(revert) })
        else {
            continue;
        };
        let [.., target, jumpi] = block.instructions.as_slice() else { continue };
        let Some(continuation) = target.pushed_block() else { continue };
        if jumpi.as_evm_opcode() != Some(op::JUMPI)
            || !target.is_encoded_push()
            || continuation.index() != revert.index() + 1
        {
            continue;
        }
        // Inverting the branch drops an `ISZERO`, exchanges `EQ` and `SUB`, or inserts an `ISZERO`
        // before the branch target. Dropping and inserting change what runs at that boundary, so
        // both need a boundary `keep_with_next` allows to be disturbed.
        let condition_end = block.instructions.len() - 2;
        let prev = condition_end.checked_sub(1).map(|index| &block.instructions[index]);
        let is_iszero = prev.is_some_and(|inst| inst.opcode == op::ISZERO);
        let inverse = prev.filter(|inst| !inst.keeps_with_next()).and_then(|inst| {
            match inst.as_evm_opcode()? {
                op::EQ => Some(op::SUB),
                op::SUB => Some(op::EQ),
                _ => None,
            }
        });
        let inverted = if !is_empty && !is_iszero && inverse.is_none() {
            let Some((start, bound, opposite)) =
                invert_comparison(&block.instructions[..condition_end], evm_version)
            else {
                continue;
            };
            let previous = block.instructions[start].concrete_immediate().unwrap();
            let before = immediate_materialization_cost(evm_version, previous);
            let after = immediate_materialization_cost(evm_version, bound);
            if after.0 > before.0 || after.1 > before.1 {
                continue;
            }
            Some((start, bound, opposite))
        } else {
            None
        };
        if !is_split_point(&block.instructions, condition_end - usize::from(is_iszero)) {
            continue;
        }
        // <condition>
        // iszero
        // push <shared>
        // jump <continuation>
        let mut push = Instruction::push_block(target_revert);
        push.metadata.copy_source_debug_from(&target.metadata);
        let mut terminator = Terminator::new(TerminatorKind::Jump(continuation));
        terminator.metadata.copy_source_debug_from(&term.metadata);
        block.terminator = Some(terminator);
        block.instructions[condition_end] = push;
        if let Some((start, bound, opposite)) = inverted {
            block.instructions[start].replace_preserving_metadata(Instruction::push_value(bound));
            block.instructions[condition_end - 1]
                .replace_preserving_metadata(Instruction::opcode(opposite));
        } else if is_iszero {
            block.instructions.remove(condition_end - 1);
        } else if let Some(opcode) = inverse {
            block.instructions[condition_end - 1]
                .replace_preserving_metadata(Instruction::opcode(opcode));
        } else {
            let mut iszero = Instruction::opcode(op::ISZERO);
            iszero.metadata.copy_source_debug_from(&block.instructions[condition_end + 1].metadata);
            block.instructions.insert(condition_end, iszero);
        }
        changed = true;
    }
    changed
}

fn preserves_shared_revert_low_address(target: Target, module: &Module, shared: BlockId) -> bool {
    // Inverting the branch can remove the early unconditional jump that lets
    // layout keep a frequently referenced revert below the PUSH1 boundary.
    // Estimate every block with a label, one-byte pushes, and a jump below that boundary.
    let push = target.opcode(op::PUSH1).bytes as usize;
    let block_size = |block: &Block| {
        target.opcode(op::JUMPDEST).bytes as usize
            + block
                .instructions
                .iter()
                .map(|inst| if inst.is_encoded_push() { push } else { 1 })
                .sum::<usize>()
            + block.terminator.as_ref().map_or(0, |term| match term.kind {
                TerminatorKind::Jump(_) => push + target.opcode(op::JUMP).bytes as usize,
                _ => 1,
            })
    };
    let mut references = 0;
    let mut shared_end = 0;
    let mut total = 0;
    for (block_id, block) in module.blocks.iter_enumerated() {
        references +=
            block.instructions.iter().filter(|inst| inst.pushed_block() == Some(shared)).count();
        total += block_size(block);
        if block_id == shared {
            shared_end = total;
        }
    }
    references >= 2 && shared_end <= 0xff && total > 0xff
}

fn is_empty_revert(block: &Block) -> bool {
    let [zero, second] = block.instructions.as_slice() else { return false };
    is_zero_push(zero)
        && (second.as_stack_op() == Some(op::StackOp::Dup(1)) || is_zero_push(second))
        && ends_in_revert(block)
}

fn ends_in_revert(block: &Block) -> bool {
    matches!(block.terminator.as_ref().map(|term| &term.kind), Some(TerminatorKind::Op(op::REVERT)))
}

fn is_zero_push(inst: &Instruction) -> bool {
    inst.concrete_immediate() == Some(U256::ZERO)
}
