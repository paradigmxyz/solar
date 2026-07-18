//! Merge profitable suffixes of machine-level terminal blocks.

use super::utils::is_evm_terminal;
use crate::backend::evm::ir::{Block, Instruction, Module, Terminator, TerminatorKind};

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    let mut changed = false;
    while let Some((left, right, common)) = best_merge(module) {
        let left_len = module.blocks[left].instructions.len();
        let mut tail = Block::new(fresh_label(module));
        tail.metadata = module.blocks[left].metadata;
        tail.instructions = module.blocks[left].instructions[left_len - common..].to_vec();
        tail.terminator = module.blocks[left].terminator.clone();
        let tail = module.add_block(tail);
        for block in [left, right] {
            let len = module.blocks[block].instructions.len();
            module.blocks[block].instructions.truncate(len - common);
            module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(tail)));
        }
        changed = true;
    }
    changed
}

fn best_merge(
    module: &Module,
) -> Option<(crate::backend::evm::ir::BlockId, crate::backend::evm::ir::BlockId, usize)> {
    let terminals: Vec<_> = module
        .blocks
        .iter_enumerated()
        .filter_map(|(id, block)| {
            (block.entry_stack.is_empty()
                && block.instructions.iter().all(|inst| inst.result.is_none())
                && block.terminator.as_ref().is_some_and(|term| {
                    is_evm_terminal(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
                }))
            .then_some(id)
        })
        .collect();
    let mut best = None;
    for (at, &left) in terminals.iter().enumerate() {
        for &right in &terminals[at + 1..] {
            if module.blocks[left].terminator.as_ref().map(|term| &term.kind)
                != module.blocks[right].terminator.as_ref().map(|term| &term.kind)
            {
                continue;
            }
            let lhs = &module.blocks[left].instructions;
            let rhs = &module.blocks[right].instructions;
            let mut common = 0;
            while common < lhs.len().min(rhs.len())
                && same_machine_instruction(
                    &lhs[lhs.len() - 1 - common],
                    &rhs[rhs.len() - 1 - common],
                )
            {
                common += 1;
            }
            let terminator_size = terminator_lower_bound(
                &module.blocks[left].terminator.as_ref().expect("terminal block").kind,
            );
            if common != 0
                && lower_bound(&lhs[lhs.len() - common..]) + terminator_size > 5
                && best.is_none_or(|(_, _, known)| common > known)
            {
                best = Some((left, right, common));
            }
        }
    }
    best
}

fn terminator_lower_bound(kind: &TerminatorKind) -> usize {
    if matches!(kind, TerminatorKind::Jump(_)) { 3 } else { 1 }
}

fn same_machine_instruction(left: &Instruction, right: &Instruction) -> bool {
    left.result.is_none()
        && right.result.is_none()
        && left.opcode == right.opcode
        && left.encoding == right.encoding
        && left.operands == right.operands
}

fn lower_bound(instructions: &[Instruction]) -> usize {
    instructions.iter().map(|inst| if inst.is_encoded_push() { 2 } else { 1 }).sum()
}

fn fresh_label(module: &Module) -> u32 {
    module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .expect("EVM IR block label overflow")
}
