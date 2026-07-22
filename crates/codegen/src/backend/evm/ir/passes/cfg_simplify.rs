//! Machine-level EVM control-flow simplification.

use super::utils::{remap_block_order, retain_blocks};
use crate::backend::evm::{
    ir::{BlockId, Module, PushValue, Terminator, TerminatorKind},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;

pub(super) fn run(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    state.reserve(module.blocks.len());
    let mut changed = false;
    loop {
        let truncated = truncate_after_terminal(module);
        let redirected = redirect_jump_thunks(module, &mut state.thunks, &mut state.order);
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
    thunks: IndexVec<BlockId, Option<BlockId>>,
    reachable: DenseBitSet<BlockId>,
    pending: Vec<BlockId>,
    references: IndexVec<BlockId, usize>,
    retained: DenseBitSet<BlockId>,
    order: Vec<BlockId>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            thunks: IndexVec::new(),
            reachable: DenseBitSet::new_empty(0),
            pending: Vec::new(),
            references: IndexVec::new(),
            retained: DenseBitSet::new_empty(0),
            order: Vec::new(),
        }
    }
}

impl RunState {
    fn reserve(&mut self, blocks: usize) {
        reserve_to(self.thunks.as_mut_vec(), blocks);
        reserve_to(&mut self.pending, blocks);
        reserve_to(self.references.as_mut_vec(), blocks);
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

fn redirect_jump_thunks(
    module: &mut Module,
    thunks: &mut IndexVec<BlockId, Option<BlockId>>,
    order: &mut Vec<BlockId>,
) -> bool {
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
            let Some(next) = thunks[target] else { break };
            if next == start {
                return start;
            }
            target = next;
        }
        target
    };

    let mut changed = false;
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
    let entry = resolve(BlockId::ENTRY);
    if entry != BlockId::ENTRY {
        order.clear();
        order.push(entry);
        order.extend(module.blocks.indices().filter(|&block| block != entry));
        remap_block_order(module, order);
        changed = true;
    }
    changed
}

fn remove_unreachable_blocks(
    module: &mut Module,
    reachable: &mut DenseBitSet<BlockId>,
    pending: &mut Vec<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    if module.blocks.is_empty() {
        return false;
    }
    if reachable.domain_size() != module.blocks.len() {
        *reachable = DenseBitSet::new_empty(module.blocks.len());
    } else {
        reachable.clear();
    }
    pending.clear();
    pending.push(BlockId::ENTRY);
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
    references: &mut IndexVec<BlockId, usize>,
    retained: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    references.clear();
    references.resize(module.blocks.len(), 0);
    // Count the implicit program-entry edge.
    if let Some(entry_references) = references.first_mut() {
        *entry_references = 1;
    }
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(PushValue::Block(target)) = &inst.value {
                references[*target] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target] += 1);
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
            if target == predecessor || references[target] != 1 || !retained.contains(target) {
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
