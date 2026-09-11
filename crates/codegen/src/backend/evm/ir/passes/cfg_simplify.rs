//! Simplify machine-level EVM control flow before block layout and assembly.
//!
//! The pass truncates instructions after a terminal opcode, folds branches whose two edges have
//! the same target, redirects label-only jump thunks, removes unreachable blocks, and merges an
//! unconditional predecessor into its sole unaddressed successor. It repeats these steps because
//! each rewrite can expose another. Degenerate branches include both structural
//! [`TerminatorKind::JumpI`] terminators and the physical `PUSH target; JUMPI; jump target` form
//! emitted when edge-specific stack scheduling lowers one branch edge before EVM IR construction.
//! In gas mode, an inverted branch to a uniquely referenced loop body is rotated: its small exit
//! tail moves to a separate block and the body can merge into the header. This removes an `ISZERO`
//! and the body's `JUMPDEST` on each iteration. Only direct backedges and tails of at most eight
//! instructions are considered; no instructions are duplicated and protected boundaries stay
//! intact.
//!
//! Address-taken blocks remain distinct, and block merging requires one reference so changing a
//! predecessor cannot affect another edge. The pass preserves the condition's stack effect with a
//! `POP`; later dead-code elimination may remove the pure condition computation. A pushed label
//! consumed immediately by a dynamic jump becomes an explicit CFG edge, exposing thunks left by
//! return-tail inlining. Replacing the
//! physical form's `PUSH target; JUMPI` with that `POP` changes what runs after the condition, so
//! it only applies where `keep_with_next` allows that boundary to be disturbed.
//!
//! Compiler-generated return continuations explicitly declare that their label's numeric identity
//! is unobservable. Empty continuation thunks can therefore be bypassed even through a pushed
//! return address. Ordinary address-taken labels remain opaque. The declaration is emitted by call
//! lowering and machine outlining and round-trips through EVM IR independently of debug output.

use super::{
    EvmPass,
    utils::{FreshLabels, is_split_point, remap_block_order, retain_blocks},
};
use crate::backend::evm::{
    ir::{Block, BlockId, Metadata, Module, PushValue, Terminator, TerminatorKind},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
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

/// Runs CFG cleanup only when the wrapped transform changes the module.
pub(super) struct Cleanup<T>(pub(super) T);

impl<T: EvmPass> EvmPass for Cleanup<T> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        self.0.is_enabled(gcx, module)
    }

    fn is_required(&self) -> bool {
        self.0.is_required()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let changed = self.0.run_pass(gcx, module);
        if changed {
            // changed CFG; simplify CFG
            let _ = CfgSimplify.run_pass(gcx, module);
        }
        changed
    }
}

fn simplify_cfg(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    state.reserve(module.blocks.len());
    let mut changed =
        gcx.sess.opts.optimization.is_gas() && rotate_loop_exits(module, &mut state.references);
    loop {
        let truncated = truncate_after_terminal(module);
        let direct = simplify_known_jumps(module);
        let degenerate = simplify_degenerate_branches(module);
        let redirected =
            redirect_jump_thunks(module, &mut state.thunks, &mut state.addressed, &mut state.order);
        let swept = remove_unreachable_blocks(
            module,
            &mut state.reachable,
            &mut state.pending,
            &mut state.order,
        );
        let coalesced =
            coalesce_blocks(module, &mut state.references, &mut state.retained, &mut state.order);
        changed |= truncated || direct || degenerate || redirected || swept || coalesced;
        if !truncated && !direct && !degenerate && !redirected && !swept && !coalesced {
            return changed;
        }
    }
}

fn rotate_loop_exits(module: &mut Module, references: &mut IndexVec<BlockId, usize>) -> bool {
    let mut candidates = Vec::new();
    for (header, block) in module.blocks.iter_enumerated() {
        if block.terminator.is_none() {
            continue;
        }
        for index in
            block.instructions.len().saturating_sub(11)..block.instructions.len().saturating_sub(2)
        {
            let insts = &block.instructions;
            if insts[index].as_evm_opcode() == Some(op::ISZERO)
                && insts[index].has_canonical_stack_effect()
                && insts[index + 1].is_encoded_push()
                && insts[index + 1].has_canonical_stack_effect()
                && let Some(body) = insts[index + 1].pushed_block()
                && body != header
                && insts[index + 2].as_evm_opcode() == Some(op::JUMPI)
                && insts[index + 2].has_canonical_stack_effect()
                && (index..=index + 3).all(|i| is_split_point(insts, i))
                && module.blocks[body].instructions.windows(2).any(|pair| {
                    pair[0].pushed_block() == Some(header)
                        && pair[1].as_evm_opcode() == Some(op::JUMPI)
                })
            {
                candidates.push((header, body, index));
            }
        }
    }
    if candidates.is_empty() {
        return false;
    }
    // Most CFG cleanups have no candidate. Count references and reserve labels
    // only after the bounded instruction scan finds a possible rotation.
    references.resize(module.blocks.len(), 0);
    if let Some(entry) = references.first_mut() {
        *entry = 1;
    }
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                references[target] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| references[target] += 1);
        }
    }
    candidates.retain(|&(_, body, _)| references[body] == 1);
    candidates.dedup_by_key(|candidate| candidate.0);
    if candidates.is_empty() {
        return false;
    }
    let Some(labels) = FreshLabels::new(module).take(candidates.len()) else { return false };
    for ((header, body, index), label) in candidates.into_iter().zip(labels) {
        // header: condition; iszero; push body; jumpi; exit_tail
        // -> header: condition; push exit; jumpi; jump body
        //    exit: exit_tail
        let block = &mut module.blocks[header];
        let mut exit = Block::new(label);
        exit.metadata.hotness = block.metadata.hotness;
        exit.instructions = block.instructions.split_off(index + 3);
        exit.terminator = block.terminator.take();
        let removed = block.instructions.remove(index);
        block.instructions[index].metadata.absorb_debug_info(&removed.metadata);
        block.instructions[index].metadata.stack = None;
        block.terminator = Some(Terminator::new(TerminatorKind::Jump(body)));
        let exit = module.blocks.push(exit);
        module.blocks[header].instructions[index].value = Some(PushValue::Block(exit));
    }
    true
}

fn simplify_known_jumps(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        if let Some(term) = &mut block.terminator
            && term.kind == TerminatorKind::Op(op::JUMP)
            && let Some(last) = block.instructions.last()
            && last.is_encoded_push()
            && last.has_canonical_stack_effect()
            && !last.keeps_with_next()
            && let Some(target) = last.pushed_block()
            && is_split_point(&block.instructions, block.instructions.len() - 1)
        {
            // push target; jump -> jump target
            term.metadata.absorb_debug_info(&last.metadata);
            term.metadata.stack = None;
            term.kind = TerminatorKind::Jump(target);
            block.instructions.pop();
            changed = true;
        }
    }
    changed
}

struct RunState {
    thunks: FxHashMap<BlockId, BlockId>,
    addressed: DenseBitSet<BlockId>,
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
                (!inst.is_encoded_push() && op::is_terminal(inst.opcode)).then_some((
                    at,
                    inst.opcode,
                    inst.metadata.clone(),
                ))
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
    thunks: &mut FxHashMap<BlockId, BlockId>,
    addressed: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) -> bool {
    // A thunk is an empty block that only jumps on. Redirect direct branches and explicitly
    // unobservable return addresses; ordinary pushed labels may be compared numerically.
    thunks.clear();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if block.instructions.is_empty()
            && let Some(terminator) = &block.terminator
            && let TerminatorKind::Jump(target) = &terminator.kind
        {
            thunks.insert(block_id, *target);
        }
    }
    if thunks.is_empty() {
        return false;
    }

    addressed.clear_to(module.blocks.len());
    for block in &module.blocks {
        for (at, inst) in block.instructions.iter().enumerate() {
            if let Some(PushValue::Block(target)) = &inst.value
                && !is_direct_jump_label(block, at)
                && !module.blocks[*target].metadata.is_continuation
            {
                addressed.insert(*target);
            }
        }
    }
    thunks.retain(|block, _| !addressed.contains(*block));
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
    let mut forwarded_continuations = DenseBitSet::new_empty(module.blocks.len());
    for block in &mut module.blocks {
        for at in 0..block.instructions.len() {
            if let Some(PushValue::Block(target)) = block.instructions[at].value
                && (is_direct_jump_label(block, at) || thunks.contains_key(&target))
            {
                if is_direct_jump_label(block, at)
                    && let Some(metadata) = thunk_metadata.get(&target)
                {
                    block.instructions[at].metadata.absorb_debug_info(metadata);
                }
                // NOTE: A return-address push executes before the callee, while the bypassed
                // continuation runs after it. Its debug events cannot move onto that push;
                // those zero-instruction checkpoints are intentionally dropped with the thunk.
                let resolved = resolve(target);
                if !is_direct_jump_label(block, at) && !addressed.contains(resolved) {
                    // The forwarded return address remains unobservable. Retain that fact on
                    // its destination unless the destination already had an opaque address use.
                    forwarded_continuations.insert(resolved);
                }
                changed |= resolved != target;
                // push continuation; ...; continuation: jump target -> push target; ...
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
    // target: <unobservable control continuation>
    for target in forwarded_continuations.iter() {
        changed |= !module.blocks[target].metadata.is_continuation;
        module.blocks[target].metadata.is_continuation = true;
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

pub(super) fn is_direct_jump_label(block: &Block, at: usize) -> bool {
    block.instructions.get(at + 1).is_some_and(|inst| matches!(inst.opcode, op::JUMP | op::JUMPI))
        || (at + 1 == block.instructions.len()
            && block
                .terminator
                .as_ref()
                .is_some_and(|term| matches!(term.kind, TerminatorKind::Op(op::JUMP | op::JUMPI))))
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
            module.blocks[predecessor].metadata.in_loop |= module.blocks[target].metadata.in_loop;
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
