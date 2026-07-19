//! Machine-level EVM control-flow simplification.

use super::utils::retain_blocks;
use crate::backend::evm::{
    ir::{BlockId, Module, Operand, Terminator, TerminatorKind},
    opcode as op,
};
use solar_data_structures::bit_set::DenseBitSet;

pub(super) fn run(
    module: &mut Module,
    _options: super::PassOptions,
    pass_state: &mut super::PassState,
) -> bool {
    let state = &mut pass_state.cfg_simplify;
    state.reserve(module.blocks.len());
    let mut changed = false;
    loop {
        let truncated = truncate_after_terminal(module);
        let redirected = redirect_jump_thunks(module, &mut state.thunks);
        let swept = remove_unreachable_blocks(
            module,
            &mut state.reachable,
            &mut state.pending,
            &mut state.order,
        );
        let coalesced =
            coalesce_blocks(module, &mut state.references, &mut state.retained, &mut state.order);
        changed |= truncated || redirected || swept || coalesced;
        if !truncated && !redirected && !swept && !coalesced {
            return changed;
        }
    }
}

pub(super) struct RunState {
    thunks: Vec<Option<BlockId>>,
    reachable: DenseBitSet<BlockId>,
    pending: Vec<BlockId>,
    references: Vec<usize>,
    retained: DenseBitSet<BlockId>,
    order: Vec<BlockId>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            thunks: Vec::new(),
            reachable: DenseBitSet::new_empty(0),
            pending: Vec::new(),
            references: Vec::new(),
            retained: DenseBitSet::new_empty(0),
            order: Vec::new(),
        }
    }
}

impl RunState {
    fn reserve(&mut self, blocks: usize) {
        reserve_to(&mut self.thunks, blocks);
        reserve_to(&mut self.pending, blocks);
        reserve_to(&mut self.references, blocks);
        reserve_to(&mut self.order, blocks);
    }
}

fn reserve_to<T>(values: &mut Vec<T>, capacity: usize) {
    if values.capacity() < capacity {
        values.reserve(capacity - values.len());
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

fn redirect_jump_thunks(module: &mut Module, thunks: &mut Vec<Option<BlockId>>) -> bool {
    thunks.clear();
    thunks.extend(module.blocks.iter().map(|block| {
        if block.instructions.is_empty() && block.entry_stack.is_empty() {
            match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Jump(target)) => Some(*target),
                _ => None,
            }
        } else {
            None
        }
    }));
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

fn remove_unreachable_blocks(
    module: &mut Module,
    reachable: &mut DenseBitSet<BlockId>,
    pending: &mut Vec<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    let Some(entry) = module.entry_block else { return false };
    if reachable.domain_size() != module.blocks.len() {
        *reachable = DenseBitSet::new_empty(module.blocks.len());
    } else {
        reachable.clear();
    }
    pending.clear();
    pending.push(entry);
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
    order.clear();
    order.extend(reachable.iter());
    retain_blocks(module, order);
    true
}

fn coalesce_blocks(
    module: &mut Module,
    references: &mut Vec<usize>,
    retained: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    references.clear();
    references.resize(module.blocks.len(), 0);
    for block in &module.blocks {
        for inst in &block.instructions {
            for operand in &inst.operands {
                count_operand(operand, references);
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target.index()] += 1);
            term.kind.visit_operands(|operand| {
                count_operand(operand, references);
            });
        }
    }

    if retained.domain_size() != module.blocks.len() {
        *retained = DenseBitSet::new_filled(module.blocks.len());
    } else {
        retained.insert_all();
    }
    for predecessor in module.blocks.indices() {
        if !retained.contains(predecessor) {
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
                || !retained.contains(target)
            {
                break;
            }

            let mut instructions = std::mem::take(&mut module.blocks[target].instructions);
            let terminator = module.blocks[target].terminator.take();
            module.blocks[predecessor].instructions.append(&mut instructions);
            module.blocks[predecessor].terminator = terminator;
            retained.remove(target);
        }
    }
    if retained.count() == module.blocks.len() {
        return false;
    }
    order.clear();
    order.extend(retained.iter());
    retain_blocks(module, order);
    true
}

fn count_operand(operand: &Operand, references: &mut [usize]) {
    if let Operand::Block(target) = operand {
        references[target.index()] += 1;
    }
}
