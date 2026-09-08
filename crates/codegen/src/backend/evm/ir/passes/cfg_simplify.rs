//! Simplify machine-level EVM control flow before block layout and assembly.
//!
//! The pass truncates instructions after a terminal opcode, folds branches whose two edges have
//! the same target, redirects label-only jump thunks, removes unreachable blocks, and merges an
//! unconditional predecessor into its sole unaddressed successor. It repeats these steps because
//! each rewrite can expose another. Degenerate branches include both structural
//! [`TerminatorKind::JumpI`] terminators and the physical `PUSH target; JUMPI; jump target` form
//! emitted when edge-specific stack scheduling lowers one branch edge before EVM IR construction.
//!
//! Final cleanup also recognizes labels passed straight to a shared `JUMPI` head as direct
//! jump targets. Deferring this until sharing is complete avoids exposing larger tails whose
//! merger would add jumps back to the paths being shortened.
//! Gas cleanup duplicates a word-return body of at most eight bytes into an empty stub
//! reached by at least two other empty stubs. It amortizes the copy across those paths while
//! preserving every address-taken label. The copy stays after structural sharing so it cannot
//! be merged back into the jump it removes. Function-entry blocks and activation events on
//! the replaced jump are excluded.
//! Other address-taken blocks remain distinct, and block merging requires one reference so changing
//! a predecessor cannot affect another edge. The pass preserves the condition's stack effect with a
//! `POP`; later dead-code elimination may remove the pure condition computation. Replacing the
//! physical form's `PUSH target; JUMPI` with that `POP` changes what runs after the condition, so
//! it only applies where `keep_with_next` allows that boundary to be disturbed.

use super::{
    EvmPass,
    utils::{instruction_size_lower_bound, is_split_point, remap_block_order, retain_blocks},
};
use crate::backend::evm::{
    ir::{Block, BlockId, Metadata, Module, PushValue, Terminator, TerminatorKind},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;

pub(super) struct CfgSimplify {
    thread_shared_jumps: bool,
}

impl CfgSimplify {
    pub(super) const EARLY: Self = Self { thread_shared_jumps: false };
    pub(super) const FINAL: Self = Self { thread_shared_jumps: true };
}

impl EvmPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        simplify_cfg(gcx, module, self.thread_shared_jumps)
    }
}

fn simplify_cfg(gcx: Gcx<'_>, module: &mut Module, thread_shared_jumps: bool) -> bool {
    let mut state = RunState::default();
    state.reserve(module.blocks.len());
    let mut changed = false;
    loop {
        let truncated = truncate_after_terminal(module);
        let degenerate = simplify_degenerate_branches(module);
        let redirected = redirect_jump_thunks(
            module,
            thread_shared_jumps,
            &mut state.thunks,
            &mut state.addressed,
            &mut state.jump_heads,
            &mut state.order,
        );
        let inlined = thread_shared_jumps
            && gcx.sess.opts.optimization.is_gas()
            && inline_shared_return_thunks(gcx, module, &mut state.references);
        let swept = remove_unreachable_blocks(
            module,
            &mut state.reachable,
            &mut state.pending,
            &mut state.order,
        );
        let coalesced =
            coalesce_blocks(module, &mut state.references, &mut state.retained, &mut state.order);
        changed |= truncated || degenerate || redirected || inlined || swept || coalesced;
        if !truncated && !degenerate && !redirected && !inlined && !swept && !coalesced {
            return changed;
        }
    }
}

fn inline_shared_return_thunks(
    gcx: Gcx<'_>,
    module: &mut Module,
    incoming: &mut IndexVec<BlockId, usize>,
) -> bool {
    incoming.clear();
    incoming.resize(module.blocks.len(), 0);
    for block in &module.blocks {
        if block.instructions.is_empty()
            && let Some(TerminatorKind::Jump(target)) =
                block.terminator.as_ref().map(|term| &term.kind)
        {
            incoming[*target] += 1;
        }
    }
    let mut changed = false;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if incoming[block_id] < 2
            || !block.instructions.is_empty()
            || block.metadata.hotness.is_cold()
        {
            continue;
        }
        let Some(jump) = &block.terminator else { continue };
        let TerminatorKind::Jump(target) = jump.kind else { continue };
        let body = &module.blocks[target];
        let [offset, store, size, returned] = body.instructions.as_slice() else { continue };
        if body.metadata.function_invoke.is_some()
            || jump.metadata.function_invoke().is_some()
            || jump.metadata.function_exit().is_some()
            || !body.instructions.iter().all(|inst| inst.has_canonical_stack_effect())
            || store.as_evm_opcode() != Some(op::MSTORE)
            || size.concrete_immediate() != Some(alloy_primitives::U256::from(32))
            || body
                .instructions
                .iter()
                .map(|inst| instruction_size_lower_bound(gcx, inst))
                .sum::<usize>()
                > 7
            || offset.concrete_immediate().is_none()
            || returned.concrete_immediate() != offset.concrete_immediate()
            || !matches!(
                body.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::Op(op::RETURN))
            )
        {
            continue;
        }
        // thunk: jump return_body -> thunk: mstore(offset, value); return(offset, 32)
        let mut instructions = body.instructions.clone();
        instructions[0].metadata.absorb_debug_info(&jump.metadata);
        let terminator = body.terminator.clone();
        module.blocks[block_id].instructions = instructions;
        module.blocks[block_id].terminator = terminator;
        changed = true;
    }
    changed
}

struct RunState {
    thunks: FxHashMap<BlockId, BlockId>,
    addressed: DenseBitSet<BlockId>,
    jump_heads: DenseBitSet<BlockId>,
    reachable: DenseBitSet<BlockId>,
    pending: Vec<BlockId>,
    references: IndexVec<BlockId, usize>,
    retained: DenseBitSet<BlockId>,
    order: Vec<BlockId>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            thunks: FxHashMap::default(),
            addressed: DenseBitSet::new_empty(0),
            jump_heads: DenseBitSet::new_empty(0),
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
        let Some((at, opcode, metadata)) =
            block.instructions.iter().enumerate().find_map(|(at, inst)| {
                (!inst.is_encoded_push() && op::is_terminal(inst.opcode))
                    .then(|| (at, inst.opcode, inst.metadata.clone()))
            })
        else {
            continue;
        };
        block.instructions.truncate(at);
        let mut terminator = Terminator::new(TerminatorKind::Op(opcode));
        terminator.metadata = metadata;
        block.terminator = Some(terminator);
        changed = true;
    }
    changed
}

fn simplify_degenerate_branches(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        if let Some(Terminator {
            kind: TerminatorKind::JumpI { then_block, else_block },
            metadata,
            ..
        }) = block.terminator.as_ref()
            && then_block == else_block
        {
            let target = *then_block;
            let metadata = metadata.clone();
            let mut pop = crate::backend::evm::ir::Instruction::stack_op(op::StackOp::Pop);
            pop.metadata.copy_source_debug_from(&metadata);
            block.instructions.push(pop);
            let mut terminator = Terminator::new(TerminatorKind::Jump(target));
            terminator.metadata.copy_debug_info_from(&metadata);
            block.terminator = Some(terminator);
            changed = true;
            continue;
        }

        if let Some(TerminatorKind::Jump(target)) = block.terminator.as_ref().map(|term| &term.kind)
            && let [.., pushed, jumpi] = block.instructions.as_slice()
            && pushed.has_canonical_stack_effect()
            && pushed.is_encoded_push()
            && pushed.value == Some(PushValue::Block(*target))
            && jumpi.has_canonical_stack_effect()
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && is_split_point(&block.instructions, block.instructions.len() - 2)
        {
            let metadata = jumpi.metadata.clone();
            block.instructions.truncate(block.instructions.len() - 2);
            let mut pop = crate::backend::evm::ir::Instruction::stack_op(op::StackOp::Pop);
            pop.metadata.copy_source_debug_from(&metadata);
            block.instructions.push(pop);
            changed = true;
        }
    }
    changed
}

fn redirect_jump_thunks(
    module: &mut Module,
    thread_shared_jumps: bool,
    thunks: &mut FxHashMap<BlockId, BlockId>,
    addressed: &mut DenseBitSet<BlockId>,
    jump_heads: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    // Redirect direct edges through empty blocks. Keep labels used as values distinct, and move
    // each removed thunk's debug event to its incoming edges.
    jump_heads.clear_to(module.blocks.len());
    if thread_shared_jumps {
        for (block_id, block) in module.blocks.iter_enumerated() {
            if block.instructions.first().is_some_and(|inst| {
                inst.has_canonical_stack_effect() && inst.as_evm_opcode() == Some(op::JUMPI)
            }) {
                jump_heads.insert(block_id);
            }
        }
    }
    addressed.clear_to(module.blocks.len());
    for block in &module.blocks {
        for (at, inst) in block.instructions.iter().enumerate() {
            if let Some(PushValue::Block(target)) = &inst.value
                && !is_direct_jump_label(block, at, jump_heads)
            {
                addressed.insert(*target);
            }
        }
    }

    thunks.clear();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if !addressed.contains(block_id)
            && block.instructions.is_empty()
            && let Some(terminator) = &block.terminator
            && let TerminatorKind::Jump(target) = &terminator.kind
        {
            thunks.insert(block_id, *target);
        }
    }
    if thunks.is_empty() {
        return false;
    }

    let block_count = module.blocks.len();
    let resolve = |start: BlockId| {
        let mut target = start;
        for _ in 0..block_count {
            let Some(&next) = thunks.get(&target) else { break };
            if next == start {
                return start;
            }
            target = next;
        }
        target
    };

    let thunk_metadata = thunks
        .keys()
        .map(|&block_id| {
            let block = &module.blocks[block_id];
            let mut metadata = Metadata::default();
            if let Some(function) = block.metadata.function_invoke {
                metadata.set_function_invoke(function);
            }
            if let Some(terminator) = &block.terminator {
                metadata.absorb_debug_info(&terminator.metadata);
            }
            (block_id, metadata)
        })
        .collect::<FxHashMap<_, _>>();

    let mut changed = false;
    for block in &mut module.blocks {
        for at in 0..block.instructions.len() {
            if is_direct_jump_label(block, at, jump_heads)
                && let Some(PushValue::Block(target)) = block.instructions[at].value
            {
                if let Some(metadata) = thunk_metadata.get(&target) {
                    block.instructions[at].metadata.absorb_debug_info(metadata);
                }
                let resolved = resolve(target);
                changed |= resolved != target;
                block.instructions[at].value = Some(PushValue::Block(resolved));
            }
        }
        if let Some(term) = &mut block.terminator {
            term.kind.visit_targets_mut(|target| {
                if let Some(metadata) = thunk_metadata.get(target) {
                    term.metadata.absorb_debug_info(metadata);
                }
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

// PUSH target; jump head; head: JUMPI -> a direct use of target
fn is_direct_jump_label(block: &Block, at: usize, jump_heads: &DenseBitSet<BlockId>) -> bool {
    block.instructions.get(at + 1).is_some_and(|inst| matches!(inst.opcode, op::JUMP | op::JUMPI))
        || (at + 1 == block.instructions.len()
            && block.terminator.as_ref().is_some_and(|term| match term.kind {
                TerminatorKind::Op(op::JUMP | op::JUMPI) => true,
                TerminatorKind::Jump(target) => jump_heads.contains(target),
                _ => false,
            }))
}

#[must_use]
fn remove_unreachable_blocks(
    module: &mut Module,
    reachable: &mut DenseBitSet<BlockId>,
    pending: &mut Vec<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    if module.blocks.is_empty() {
        return false;
    }
    reachable.clear_to(module.blocks.len());
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
            let mut terminator = module.blocks[target].terminator.take();
            if let Some(function) = module.blocks[target].metadata.function_invoke {
                if let Some(instruction) = instructions.first_mut() {
                    instruction.metadata.set_function_invoke(function);
                } else if let Some(terminator) = &mut terminator {
                    terminator.metadata.set_function_invoke(function);
                }
            }
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
