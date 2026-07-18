//! Machine-level EVM control-flow simplification.

use super::utils::{
    retain_blocks, visit_terminator_operands, visit_terminator_targets,
    visit_terminator_targets_mut,
};
use crate::backend::evm::{
    ir::{BlockId, Module, Operand, Terminator, TerminatorKind},
    opcode as op,
};
use solar_data_structures::bit_set::DenseBitSet;

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    let mut changed = false;
    loop {
        let truncated = truncate_after_terminal(module);
        let redirected = redirect_jump_thunks(module);
        let swept = remove_unreachable_blocks(module);
        let coalesced = coalesce_one_block(module);
        changed |= truncated || redirected || swept || coalesced;
        if !truncated && !redirected && !swept && !coalesced {
            return changed;
        }
    }
}

fn truncate_after_terminal(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        let Some((at, opcode)) = block.instructions.iter().enumerate().find_map(|(at, inst)| {
            (!inst.is_encoded_push() && op::is_terminal(inst.opcode)).then_some((at, inst.opcode))
        }) else {
            continue;
        };
        block.instructions.truncate(at);
        block.terminator = Some(Terminator::new(TerminatorKind::RawOpcode(opcode)));
        changed = true;
    }
    changed
}

fn redirect_jump_thunks(module: &mut Module) -> bool {
    let thunks: Vec<_> = module
        .blocks
        .iter()
        .map(|block| {
            if block.instructions.is_empty() && block.entry_stack.is_empty() {
                match block.terminator.as_ref().map(|term| &term.kind) {
                    Some(TerminatorKind::Jump(target)) => Some(*target),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    if thunks.iter().all(Option::is_none) {
        return false;
    }

    let resolve = |start: BlockId| {
        let mut target = start;
        for _ in 0..thunks.len() {
            let Some(next) = thunks[target.index()] else { break };
            if next == start {
                return start;
            }
            target = next;
        }
        target
    };

    let mut changed = false;
    if let Some(entry) = &mut module.entry_block {
        let target = resolve(*entry);
        changed |= target != *entry;
        *entry = target;
    }
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            for operand in &mut inst.operands {
                redirect_operand(operand, &resolve, &mut changed);
            }
        }
        if let Some(term) = &mut block.terminator {
            visit_terminator_targets_mut(&mut term.kind, |target| {
                let resolved = resolve(*target);
                changed |= resolved != *target;
                *target = resolved;
            });
            redirect_terminator_operands(&mut term.kind, &resolve, &mut changed);
        }
    }
    changed
}

fn redirect_operand(
    operand: &mut Operand,
    resolve: &impl Fn(BlockId) -> BlockId,
    changed: &mut bool,
) {
    if let Operand::Block(block) = operand {
        let target = resolve(*block);
        *changed |= target != *block;
        *block = target;
    }
}

fn redirect_terminator_operands(
    kind: &mut TerminatorKind,
    resolve: &impl Fn(BlockId) -> BlockId,
    changed: &mut bool,
) {
    match kind {
        TerminatorKind::Branch { condition, .. } => redirect_operand(condition, resolve, changed),
        TerminatorKind::Switch { value, cases, .. } => {
            redirect_operand(value, resolve, changed);
            for (case, _) in cases {
                redirect_operand(case, resolve, changed);
            }
        }
        TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
            redirect_operand(offset, resolve, changed);
            redirect_operand(size, resolve, changed);
        }
        TerminatorKind::SelfDestruct { recipient } => {
            redirect_operand(recipient, resolve, changed);
        }
        TerminatorKind::Jump(_)
        | TerminatorKind::Stop
        | TerminatorKind::Invalid
        | TerminatorKind::RawOpcode(_) => {}
    }
}

fn remove_unreachable_blocks(module: &mut Module) -> bool {
    let Some(entry) = module.entry_block else { return false };
    let mut reachable = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = vec![entry];
    while let Some(block_id) = pending.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            for operand in &inst.operands {
                if let Operand::Block(target) = operand {
                    pending.push(*target);
                }
            }
        }
        if let Some(term) = &block.terminator {
            visit_terminator_targets(&term.kind, |target| pending.push(target));
            visit_terminator_operands(&term.kind, |operand| {
                if let Operand::Block(target) = operand {
                    pending.push(*target);
                }
            });
        }
    }
    if reachable.count() == module.blocks.len() {
        return false;
    }
    let order: Vec<_> = reachable.iter().collect();
    retain_blocks(module, &order);
    true
}

fn coalesce_one_block(module: &mut Module) -> bool {
    let mut references = vec![0usize; module.blocks.len()];
    for block in &module.blocks {
        for inst in &block.instructions {
            for operand in &inst.operands {
                count_operand(operand, &mut references);
            }
        }
        if let Some(term) = &block.terminator {
            visit_terminator_targets(&term.kind, |target| references[target.index()] += 1);
            visit_terminator_operands(&term.kind, |operand| {
                count_operand(operand, &mut references);
            });
        }
    }

    let entry = module.entry_block;
    let candidate = module.blocks.indices().find_map(|predecessor| {
        let TerminatorKind::Jump(target) = &module.blocks[predecessor].terminator.as_ref()?.kind
        else {
            return None;
        };
        (*target != predecessor
            && Some(*target) != entry
            && references[target.index()] == 1
            && module.blocks[*target].entry_stack.is_empty())
        .then_some((predecessor, *target))
    });
    let Some((predecessor, target)) = candidate else { return false };

    let mut successor = module.blocks[target].clone();
    module.blocks[predecessor].instructions.append(&mut successor.instructions);
    module.blocks[predecessor].terminator = successor.terminator;
    let order: Vec<_> = module.blocks.indices().filter(|block| *block != target).collect();
    retain_blocks(module, &order);
    true
}

fn count_operand(operand: &Operand, references: &mut [usize]) {
    if let Operand::Block(target) = operand {
        references[target.index()] += 1;
    }
}
