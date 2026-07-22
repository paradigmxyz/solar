//! Machine-level EVM control-flow simplification.

use super::{EvmPass, utils::retain_blocks};
use crate::backend::evm::{
    ir::{BlockId, Module, PushValue, Terminator, TerminatorKind},
    op,
};
use solar_data_structures::bit_set::DenseBitSet;
use solar_sema::Gcx;

pub(super) struct CfgSimplify;

impl EvmPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        simplify_cfg(gcx, module)
    }
}

fn simplify_cfg(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
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

struct RunState {
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
        block.terminator = Some(Terminator::new(TerminatorKind::Op(opcode)));
        changed = true;
    }
    changed
}

fn redirect_jump_thunks(module: &mut Module, thunks: &mut Vec<Option<BlockId>>) -> bool {
    thunks.clear();
    thunks.extend(module.blocks.iter().map(|block| {
        if block.instructions.is_empty() {
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
            if let Some(PushValue::Block(block)) = &mut inst.value {
                let resolved = resolve(*block);
                changed |= resolved != *block;
                *block = resolved;
            }
        }
        if let Some(term) = &mut block.terminator {
            term.kind.visit_targets_mut(|target| {
                let resolved = resolve(*target);
                changed |= resolved != *target;
                *target = resolved;
            });
        }
    }
    changed
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
            if let Some(PushValue::Block(target)) = &inst.value {
                pending.push(*target);
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| pending.push(target));
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
            if let Some(PushValue::Block(target)) = &inst.value {
                references[target.index()] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target.index()] += 1);
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
