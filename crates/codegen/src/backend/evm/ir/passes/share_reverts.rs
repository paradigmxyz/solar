//! Share adjacent empty revert paths in physically laid-out EVM IR.

use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, Operand, Terminator, TerminatorKind},
    opcode as op,
};
use alloy_primitives::U256;

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    let empty_reverts: Vec<_> =
        module.blocks.indices().map(|block| is_empty_revert(module, block)).collect();
    let Some(shared) = module.blocks.indices().find(|block| empty_reverts[block.index()]) else {
        return false;
    };
    if preserves_shared_revert_low_address(module, shared) {
        return false;
    }
    let mut changed = false;
    let block_ids: Vec<_> = module.blocks.indices().collect();
    for block_id in block_ids {
        let block = &mut module.blocks[block_id];
        let Some(Terminator { kind: TerminatorKind::Jump(revert), .. }) = &block.terminator else {
            continue;
        };
        if !empty_reverts[revert.index()] {
            continue;
        }
        let [.., target, jumpi] = block.instructions.as_mut_slice() else { continue };
        if jumpi.opcode != op::JUMPI || jumpi.is_encoded_push() {
            continue;
        }
        let [Operand::Block(continuation)] = target.operands.as_slice() else { continue };
        if !target.is_encoded_push() {
            continue;
        }
        let continuation = *continuation;
        if revert.index() != block_id.index() + 1 || continuation.index() != revert.index() + 1 {
            continue;
        }
        *target = Instruction::push(Operand::Block(shared));
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
    let references = module
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .flat_map(|inst| &inst.operands)
        .filter(|operand| matches!(operand, Operand::Block(target) if *target == shared))
        .count();
    if references < 2 {
        return false;
    }
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
    let shared_end: usize = module.blocks.iter().take(shared.index() + 1).map(block_size).sum();
    let total: usize = module.blocks.iter().map(block_size).sum();
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
            Some(TerminatorKind::RawOpcode(op::REVERT))
        )
}

fn is_zero_push(inst: &Instruction) -> bool {
    (inst.is_encoded_push()
        && matches!(inst.operands.as_slice(), [Operand::Immediate(value)] if *value == U256::ZERO))
        || (inst.opcode == op::PUSH0 && !inst.is_encoded_push())
}
