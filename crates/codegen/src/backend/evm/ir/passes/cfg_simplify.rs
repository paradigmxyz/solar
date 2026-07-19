//! Machine-level EVM control-flow simplification.

use super::utils::retain_blocks;
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
        let coalesced = coalesce_blocks(module);
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
            term.kind.visit_targets_mut(|target| {
                let resolved = resolve(*target);
                changed |= resolved != *target;
                *target = resolved;
            });
            term.kind
                .visit_operands_mut(|operand| redirect_operand(operand, &resolve, &mut changed));
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
            term.kind.visit_targets(|target| pending.push(target));
            term.kind.visit_operands(|operand| {
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

fn coalesce_blocks(module: &mut Module) -> bool {
    let mut references = vec![0usize; module.blocks.len()];
    for block in &module.blocks {
        for inst in &block.instructions {
            for operand in &inst.operands {
                count_operand(operand, &mut references);
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target.index()] += 1);
            term.kind.visit_operands(|operand| {
                count_operand(operand, &mut references);
            });
        }
    }

    let mut removed = DenseBitSet::new_empty(module.blocks.len());
    let mut blocks: Vec<_> = module.blocks.indices().collect();
    blocks.sort_unstable_by_key(|&block| module.blocks[block].label);
    for predecessor in blocks {
        if removed.contains(predecessor) {
            continue;
        }
        while let Some(TerminatorKind::Jump(target)) =
            module.blocks[predecessor].terminator.as_ref().map(|terminator| &terminator.kind)
        {
            let target = *target;
            if target == predecessor
                || Some(target) == module.entry_block
                || references[target.index()] != 1
                || !module.blocks[target].entry_stack.is_empty()
                || removed.contains(target)
            {
                break;
            }

            let mut successor = module.blocks[target].clone();
            module.blocks[predecessor].instructions.append(&mut successor.instructions);
            module.blocks[predecessor].terminator = successor.terminator;
            removed.insert(target);
        }
    }
    if removed.is_empty() {
        return false;
    }
    let order: Vec<_> = module.blocks.indices().filter(|block| !removed.contains(*block)).collect();
    retain_blocks(module, &order);
    true
}

fn count_operand(operand: &Operand, references: &mut [usize]) {
    if let Operand::Block(target) = operand {
        references[target.index()] += 1;
    }
}
