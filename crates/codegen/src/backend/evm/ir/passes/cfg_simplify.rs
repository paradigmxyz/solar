//! Simplify machine-level EVM control flow before block layout and assembly.
//!
//! The pass truncates instructions after a terminal opcode, folds branches whose two edges have
//! the same target or reach halting blocks with identical bodies, directly or through empty
//! jumps, redirects label-only jump thunks, removes unreachable blocks, and merges an
//! unconditional predecessor into its sole unaddressed successor. A folded branch's taken copy
//! lends the surviving block its hotness, loop membership and debug origins, as terminal
//! deduplication does when it shares equal blocks. Bodies containing `PC` are not equivalent
//! across positions and retain their branches. It repeats these steps because
//! each rewrite can expose another. Degenerate branches include both structural
//! [`TerminatorKind::JumpI`] terminators and the physical `PUSH target; JUMPI; jump target` form
//! emitted when edge-specific stack scheduling lowers one branch edge before EVM IR construction.
//! In gas mode, an inverted branch to a uniquely referenced loop body is rotated: its small exit
//! tail moves to a separate block and the body can merge into the header. This removes an `ISZERO`
//! and the body's `JUMPDEST` on each iteration. Only direct backedges and tails of at most eight
//! instructions are considered; no instructions are duplicated and protected boundaries stay
//! intact.
//!
//! Final cleanup exposes acyclic branch triangles as structural conditional terminators so
//! layout can place their taken arm before the join. The `jumpi` keeps its origin and the removed
//! jump and pushed label take theirs with them. It excludes glued instructions. Keeping this after
//! sharing avoids changing which physical instruction sequences earlier passes can merge.
//! Final cleanup also recognizes labels passed straight to a shared `JUMPI` head as direct
//! jump targets. Deferring this until sharing is complete avoids exposing larger tails whose
//! merger would add jumps back to the paths being shortened.
//! Size cleanup turns a constant label passed to a shared one-instruction `JUMPI` body into
//! a direct conditional terminator. This lets layout remove the intermediate jump; glued
//! sequences remain intact.
//! Gas cleanup duplicates a word-return body of at most eight bytes into an empty stub
//! reached by at least two other empty stubs. It amortizes the copy across those paths while
//! preserving every address-taken label. The copy stays after structural sharing so it cannot
//! be merged back into the jump it removes. Debug events move to retained operations where
//! representable; they do not affect rewrite eligibility.
//! Other address-taken blocks remain distinct, and block merging requires one reference so changing
//! a predecessor cannot affect another edge. The pass preserves the condition's stack effect with a
//! `POP`; later dead-code elimination may remove the pure condition computation. Replacing the
//! physical form's `PUSH target; JUMPI` with that `POP` changes what runs after the condition, so
//! it only applies where `keep_with_next` allows that boundary to be disturbed.
//!
//! A block that declares a call effect, the entry of a function or shared body, is absorbed into
//! its predecessor only if the stack verifier can still follow its returns. The successors of the
//! absorbed code take over the effect, unless the code pushes a block's address and calls out or
//! consumes the return address, when the block stays apart; a return through a dynamic jump needs
//! no successor, since the address is in the merged block.
//!
//! Compiler-generated return continuations explicitly declare that their label's numeric identity
//! is unobservable. Empty continuation thunks can therefore be bypassed even through a pushed
//! return address. Ordinary address-taken labels remain opaque. The declaration is emitted by call
//! lowering and machine outlining and round-trips through EVM IR independently of debug output.
//!
//! A bypassed thunk's jump takes its source origin with it, as a removed branch does in LLVM. Its
//! function exit moves to an unconditional transfer that skips it, never to a conditional one,
//! which also executes when that edge is not taken. A jump that ends a function, such as a tail
//! call, hands its origin to that transfer with the exit, so the call keeps its location. Its
//! function entry goes to its destination label, the callee's first instruction, never onto a jump
//! of the callers, and only while the thunk is the label's only entry: another edge into the label,
//! such as a loop's back edge, would run the entry more than once per call. After an unconditional
//! edge the entry stays on the label; after a conditional edge, later passes drop it when the label
//! becomes shared, falls through or acquires opaque address uses.

use super::{
    EvmPass,
    block_layout::{terminator_kind, triangle_arm},
    terminal_dedup::absorb_equal_terminal,
    utils::{
        FreshLabels, count_block_references, is_split_point, lower_bound, remap_block_order,
        retain_blocks,
    },
};
use crate::backend::evm::{
    DebugFunction,
    ir::{
        Block, BlockId, CallEffect, Instruction, Metadata, Module, ModuleTarget, PushValue,
        StackEffect, Terminator, TerminatorKind, default_terminator_stack_effect,
    },
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
        if self.thread_shared_jumps { "cfg-simplify" } else { "cfg-simplify-early" }
    }

    fn cache_config(&self) -> u64 {
        u64::from(self.thread_shared_jumps)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        simplify_cfg(gcx, module, self.thread_shared_jumps)
    }
}

fn simplify_cfg(gcx: Gcx<'_>, module: &mut Module, thread_shared_jumps: bool) -> bool {
    let mut state = RunState::new(module.blocks.len());
    let mut changed =
        module.optimization.is_gas() && rotate_loop_exits(module, &mut state.references);
    // Later rounds only add `POP`s or move instructions of already truncated blocks.
    changed |= truncate_after_terminal(module);
    loop {
        let direct = simplify_known_jumps(module);
        let degenerate = simplify_degenerate_branches(module);
        let redirected = redirect_jump_thunks(module, thread_shared_jumps, &mut state);
        let inlined = thread_shared_jumps
            && module.optimization.is_gas()
            && inline_shared_return_thunks(gcx, module, &mut state.references);
        let branches =
            thread_shared_jumps && module.optimization.is_size() && expose_shared_branches(module);
        let swept = remove_unreachable_blocks(module, &mut state);
        let coalesced = coalesce_blocks(module, &mut state);
        let round = direct || degenerate || redirected || inlined || branches || swept || coalesced;
        changed |= round;
        if !round {
            if thread_shared_jumps {
                changed |= normalize_triangle_branches(module);
            }
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
                && insts[index + 1].is_encoded_push()
                && let Some(body) = insts[index + 1].pushed_block()
                && body != header
                && insts[index + 2].as_evm_opcode() == Some(op::JUMPI)
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
    count_block_references(module, references);
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
        exit.metadata.repeats = block.metadata.repeats;
        exit.instructions = block.instructions.split_off(index + 3);
        exit.terminator = block.terminator.take();
        // NOTE: The removed `iszero` takes its origins with it; only its function events move.
        let removed = block.instructions.remove(index);
        block.instructions[index].metadata.move_function_events(&removed.metadata);
        block.terminator = Some(Terminator::new(TerminatorKind::Jump(body)));
        let exit = module.blocks.push(exit);
        module.blocks[header].instructions[index].value = Some(PushValue::Block(exit));
    }
    true
}

fn expose_shared_branches(module: &mut Module) -> bool {
    let mut changed = false;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if let Some(jump) = &block.terminator
            && let TerminatorKind::Jump(target) = jump.kind
            && let Some(pushed) = block.instructions.last()
            && let Some(PushValue::Block(then_block)) = pushed.value
            && pushed.is_encoded_push()
            && is_split_point(&block.instructions, block.instructions.len() - 1)
            && !pushed.keeps_with_next()
            && let body = &module.blocks[target]
            && let [jumpi] = body.instructions.as_slice()
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && !jumpi.keeps_with_next()
            && let Some(continuation) = &body.terminator
            && let TerminatorKind::Jump(else_block) = continuation.kind
        {
            // push taken; jump head; head: jumpi; jump other -> jumpi taken, other
            let mut branch = Terminator::new(TerminatorKind::JumpI { then_block, else_block });
            // NOTE: The `jumpi` is the root. The pushed label is its operand and both jumps are
            // removed, so their origins go with them and only function events move.
            branch.metadata.preserve_debug_loc(&jumpi.metadata);
            branch.metadata.move_function_events(&jumpi.metadata);
            branch.metadata.move_function_events(&pushed.metadata);
            branch.metadata.move_function_events(&jump.metadata);
            if branch.metadata.function_invoke().is_none()
                && let Some(function) = body.metadata.function_invoke
            {
                branch.metadata.set_function_invoke(function);
            }
            let block = &mut module.blocks[block_id];
            block.instructions.pop();
            block.terminator = Some(branch);
            changed = true;
        }
    }
    changed
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
            && let Some(TerminatorKind::Jump(target)) = terminator_kind(block)
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
        if store.as_evm_opcode() != Some(op::MSTORE)
            || size.concrete_immediate() != Some(alloy_primitives::U256::from(32))
            || lower_bound(module.runtime_target(gcx), &body.instructions) > 7
            || offset.concrete_immediate().is_none()
            || returned.concrete_immediate() != offset.concrete_immediate()
            || !matches!(terminator_kind(body), Some(TerminatorKind::Op(op::RETURN)))
        {
            continue;
        }
        // thunk: jump return_body -> thunk: mstore(offset, value); return(offset, 32)
        let mut instructions = body.instructions.clone();
        let mut terminator = body.terminator.clone();
        // NOTE: The copy runs on the path of the thunk's jump, which enters the body's function
        // if the body's label does, so the entry of the label moves to the first instruction of
        // the copy, as in `tail_copy`. The removed jump takes its origins with it. Its entry also
        // moves to the first instruction, which runs first, and its exit to the last, which
        // leaves.
        if instructions[0].metadata.function_invoke().is_none()
            && let Some(function) =
                body.metadata.function_invoke.or(jump.metadata.function_invoke())
        {
            instructions[0].metadata.set_function_invoke(function);
        }
        if let Some(terminator) = &mut terminator
            && terminator.metadata.function_exit().is_none()
            && let Some(exit) = jump.metadata.function_exit()
        {
            terminator.metadata.set_function_exit(exit);
        }
        module.blocks[block_id].instructions = instructions;
        module.blocks[block_id].terminator = terminator;
        changed = true;
    }
    changed
}

/// An empty block that only jumps on, with the function events its removal moves.
struct Thunk {
    next: BlockId,
    /// The function the block enters.
    entered: Option<DebugFunction>,
    /// The function exit of the block's jump, which moves to the incoming edges.
    exit: Metadata,
}

/// Scratch storage shared by the rounds of one run.
struct RunState {
    thunks: FxHashMap<BlockId, Thunk>,
    addressed: DenseBitSet<BlockId>,
    jump_heads: DenseBitSet<BlockId>,
    forwarded_continuations: DenseBitSet<BlockId>,
    reachable: DenseBitSet<BlockId>,
    retained: DenseBitSet<BlockId>,
    pending: Vec<BlockId>,
    references: IndexVec<BlockId, usize>,
    order: Vec<BlockId>,
}

impl RunState {
    fn new(blocks: usize) -> Self {
        Self {
            thunks: FxHashMap::default(),
            addressed: DenseBitSet::new_empty(0),
            jump_heads: DenseBitSet::new_empty(0),
            forwarded_continuations: DenseBitSet::new_empty(0),
            reachable: DenseBitSet::new_empty(0),
            retained: DenseBitSet::new_empty(0),
            pending: Vec::with_capacity(blocks),
            references: IndexVec::with_capacity(blocks),
            order: Vec::with_capacity(blocks),
        }
    }
}

fn normalize_triangle_branches(module: &mut Module) -> bool {
    let mut changed = false;
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        if let Some(terminator) = &block.terminator
            && let TerminatorKind::Jump(else_block) = terminator.kind
            && let [.., pushed, jumpi] = block.instructions.as_slice()
            && let Some(PushValue::Block(then_block)) = pushed.value
            && !block.metadata.in_loop
            && pushed.is_encoded_push()
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && is_split_point(&block.instructions, block.instructions.len() - 2)
            && !pushed.keeps_with_next()
            && !jumpi.keeps_with_next()
            && triangle_arm(module, then_block, else_block)
        {
            // push arm; jumpi; jump join -> jumpi arm, join
            let mut branch = Terminator::new(TerminatorKind::JumpI { then_block, else_block });
            // NOTE: The `jumpi` is the root. The pushed label is its operand and the jump is
            // removed, so their origins go with them and only the label's events move.
            branch.metadata.preserve_debug_loc(&jumpi.metadata);
            branch.metadata.move_function_events(&jumpi.metadata);
            branch.metadata.move_function_events(&pushed.metadata);
            let block = &mut module.blocks[block_id];
            block.instructions.truncate(block.instructions.len() - 2);
            block.terminator = Some(branch);
            changed = true;
        }
    }
    changed
}

fn truncate_after_terminal(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        if let Some(at) = block
            .instructions
            .iter()
            .position(|inst| !inst.is_encoded_push() && op::is_terminal(inst.opcode))
        {
            let terminal = &mut block.instructions[at];
            let mut terminator = Terminator::new(TerminatorKind::Op(terminal.opcode));
            terminator.metadata = std::mem::take(&mut terminal.metadata);
            block.instructions.truncate(at);
            block.terminator = Some(terminator);
            changed = true;
        }
    }
    changed
}

fn simplify_degenerate_branches(module: &mut Module) -> bool {
    let mut changed = false;
    for block_id in module.blocks.indices() {
        // A branch whose edges reach equal halting blocks runs the same code either way: fold it
        // into the else edge, whose code the taken copy's paths then share.
        if let Some(Terminator { kind: TerminatorKind::JumpI { then_block, else_block }, .. }) =
            module.blocks[block_id].terminator
            && absorb_equal_terminal(module, then_block, else_block)
            && let Some(terminator) = module.blocks[block_id].terminator.as_mut()
        {
            terminator.kind = TerminatorKind::JumpI { then_block: else_block, else_block };
        }
        if let Some(TerminatorKind::Jump(target)) = terminator_kind(&module.blocks[block_id])
            && let [.., pushed, jumpi] = module.blocks[block_id].instructions.as_slice()
            && let Some(PushValue::Block(pushed_block)) = pushed.value
            && pushed.is_encoded_push()
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && let target = *target
            && absorb_equal_terminal(module, pushed_block, target)
        {
            // push copy; jumpi; jump target -> push target; jumpi; jump target
            let at = module.blocks[block_id].instructions.len() - 2;
            module.blocks[block_id].instructions[at].value = Some(PushValue::Block(target));
        }

        let block = &mut module.blocks[block_id];
        if let Some(Terminator {
            kind: TerminatorKind::JumpI { then_block, else_block },
            metadata,
            ..
        }) = block.terminator.as_ref()
            && then_block == else_block
        {
            let mut pop = Instruction::stack_op(op::StackOp::Pop);
            pop.metadata.preserve_debug_loc(metadata);
            let mut terminator = Terminator::new(TerminatorKind::Jump(*then_block));
            terminator.metadata.preserve_debug_loc(metadata);
            terminator.metadata.move_function_events(metadata);
            block.instructions.push(pop);
            block.terminator = Some(terminator);
            changed = true;
            continue;
        }

        if let Some(TerminatorKind::Jump(target)) = terminator_kind(block)
            && let [.., pushed, jumpi] = block.instructions.as_slice()
            && pushed.is_encoded_push()
            && pushed.value == Some(PushValue::Block(*target))
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && is_split_point(&block.instructions, block.instructions.len() - 2)
        {
            let mut pop = Instruction::stack_op(op::StackOp::Pop);
            pop.metadata.preserve_debug_loc(&jumpi.metadata);
            pop.metadata.move_function_events(&pushed.metadata);
            pop.metadata.move_function_events(&jumpi.metadata);
            block.instructions.truncate(block.instructions.len() - 2);
            block.instructions.push(pop);
            changed = true;
        }
    }
    changed
}

/// Moves the function exit of a bypassed thunk's jump onto an unconditional transfer, with the
/// origin of that jump, as the transfer now ends the function in its place: a tail call keeps the
/// location of the call it makes, not that of the branch that led to it.
fn take_thunk_exit(metadata: &mut Metadata, exit: &Metadata) {
    if metadata.function_exit().is_none() && exit.function_exit().is_some() {
        metadata.preserve_debug_loc(exit);
    }
    metadata.move_function_events(exit);
}

fn redirect_jump_thunks(
    module: &mut Module,
    thread_shared_jumps: bool,
    state: &mut RunState,
) -> bool {
    let RunState {
        thunks, addressed, jump_heads, forwarded_continuations, order, references, ..
    } = state;
    // Redirect direct edges through empty blocks. Keep labels used as values distinct, and move
    // each removed thunk's debug event to its incoming edges. A thunk is an empty block that only
    // jumps on: redirect direct branches and explicitly unobservable return addresses; ordinary
    // pushed labels may be compared numerically.
    thunks.clear();
    jump_heads.clear_to(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        match block.instructions.first() {
            Some(inst) => {
                if thread_shared_jumps && inst.as_evm_opcode() == Some(op::JUMPI) {
                    jump_heads.insert(block_id);
                }
            }
            None => {
                if let Some(terminator) = &block.terminator
                    && let TerminatorKind::Jump(next) = terminator.kind
                {
                    thunks
                        .insert(block_id, Thunk { next, entered: None, exit: Metadata::default() });
                }
            }
        }
    }
    let jump_heads = thread_shared_jumps.then_some(&*jump_heads);
    if thunks.is_empty() {
        return false;
    }

    addressed.clear_to(module.blocks.len());
    mark_addressed_blocks(module, jump_heads, addressed);
    thunks.retain(|&block_id, thunk| {
        let block = &module.blocks[block_id];
        if addressed.contains(block_id) {
            return false;
        }
        thunk.entered = block.metadata.function_invoke;
        if let Some(terminator) = &block.terminator {
            thunk.entered = thunk.entered.or(terminator.metadata.function_invoke());
            if let Some(exit) = terminator.metadata.function_exit() {
                thunk.exit.set_function_exit(exit);
                thunk.exit.preserve_debug_loc(&terminator.metadata);
            }
        }
        true
    });
    if thunks.is_empty() {
        return false;
    }

    let block_count = module.blocks.len();
    let resolve = |start: BlockId| {
        let mut target = start;
        for _ in 0..block_count {
            let Some(thunk) = thunks.get(&target) else { break };
            if thunk.next == start {
                return start;
            }
            target = thunk.next;
        }
        target
    };

    let mut changed = false;
    let mut lent = Vec::new();
    let mut entered = Vec::new();
    if thunks.values().any(|thunk| thunk.entered.is_some()) {
        count_block_references(module, references);
    }
    forwarded_continuations.clear_to(module.blocks.len());
    let track_debug_info = module.debug_info_is_tracked();
    for (block_id, block) in module.blocks.iter_mut_enumerated() {
        // A thunk's own jump is part of the chain that its incoming edge enters.
        let account_entries = track_debug_info && !thunks.contains_key(&block_id);
        for at in 0..block.instructions.len() {
            if let Some(PushValue::Block(target)) = block.instructions[at].value
                && (is_jump_label(block, at, jump_heads) || thunks.contains_key(&target))
            {
                let direct = is_direct_jump_label(block, at);
                if direct && let Some(thunk) = thunks.get(&target) {
                    let conditional =
                        block.instructions.get(at + 1).is_some_and(|inst| inst.opcode == op::JUMPI);
                    if account_entries
                        && let Some(entry) = thunk_chain_invoke(target, thunks, references)
                    {
                        if conditional { &mut lent } else { &mut entered }.push(entry);
                    }
                    // A bypassed thunk's exit moves only onto an unconditional transfer: it
                    // executes on the thunk's path, a conditional one also on other paths. The
                    // thunk's jump takes its origins with it, as a removed branch does in LLVM's
                    // `MergeBlockIntoPredecessor`
                    // (llvm/lib/Transforms/Utils/BasicBlockUtils.cpp#L346-L371 at llvm-project
                    // e98b16a2710b), unless it ends the function: `MergeBlockIntoPredecessor` keeps
                    // the successor's terminator, so the transfer that takes the exit takes the
                    // jump's origin.
                    // NOTE: The thunk's entry never moves onto an incoming jump: the callee starts
                    // at its first instruction, which the chain's destination label holds, as the
                    // ethdebug format places `invoke` on the callee's entry `JUMPDEST`. Without an
                    // exclusive destination label the entry is unknown: another edge into the
                    // label, such as a loop's back edge, would run it more than once per call.
                    if !conditional {
                        take_thunk_exit(&mut block.instructions[at].metadata, &thunk.exit);
                    }
                }
                // NOTE: A return-address push executes before the callee, while the bypassed
                // continuation runs after it. Its debug events cannot move onto that push, so
                // they are dropped with the thunk.
                let resolved = resolve(target);
                if !direct && !addressed.contains(resolved) {
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
            let conditional = !matches!(term.kind, TerminatorKind::Jump(_));
            term.kind.visit_targets_mut(|target| {
                if let Some(thunk) = thunks.get(target) {
                    if account_entries
                        && let Some(entry) = thunk_chain_invoke(*target, thunks, references)
                    {
                        if conditional { &mut lent } else { &mut entered }.push(entry);
                    }
                    // A bypassed thunk's exit moves only onto an unconditional transfer; see the
                    // NOTE above.
                    if !conditional {
                        take_thunk_exit(&mut term.metadata, &thunk.exit);
                    }
                }
                let resolved = resolve(*target);
                changed |= resolved != *target;
                *target = resolved;
            });
        }
    }
    for (target, function) in entered {
        let metadata = &mut module.blocks[target].metadata;
        if metadata.function_invoke.is_none() && metadata.lent_invoke.is_none() {
            metadata.function_invoke = Some(function);
        }
    }
    for (target, function) in lent {
        let metadata = &mut module.blocks[target].metadata;
        if metadata.function_invoke.is_none()
            && metadata.lent_invoke.is_none_or(|lent| lent == function)
        {
            metadata.lent_invoke = Some(function);
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

/// Recognizes a label consumed directly by the following jump.
pub(super) fn is_direct_jump_label(block: &Block, at: usize) -> bool {
    is_jump_label(block, at, None)
}

/// Recognizes a label consumed directly by the following jump or, when `jump_heads` is given, by
/// a jump to a block starting with `JUMPI`.
fn is_jump_label(block: &Block, at: usize, jump_heads: Option<&DenseBitSet<BlockId>>) -> bool {
    block.instructions.get(at + 1).is_some_and(|inst| matches!(inst.opcode, op::JUMP | op::JUMPI))
        || (at + 1 == block.instructions.len()
            && block.terminator.as_ref().is_some_and(|term| match term.kind {
                TerminatorKind::Op(op::JUMP) => true,
                // PUSH target; jump head; head: JUMPI -> a direct use of target
                TerminatorKind::Jump(target) => {
                    jump_heads.is_some_and(|heads| heads.contains(target))
                }
                _ => false,
            }))
}

/// Adds to `addressed` the blocks whose label is pushed as an observable value: not consumed by
/// a jump as `is_jump_label` decides, and not a return continuation.
pub(super) fn mark_addressed_blocks(
    module: &Module,
    jump_heads: Option<&DenseBitSet<BlockId>>,
    addressed: &mut DenseBitSet<BlockId>,
) {
    for block in &module.blocks {
        for (at, inst) in block.instructions.iter().enumerate() {
            if let Some(target) = inst.pushed_block()
                && !is_jump_label(block, at, jump_heads)
                && !module.blocks[target].metadata.is_continuation
            {
                addressed.insert(target);
            }
        }
    }
}

/// Removes blocks unreachable from the entry. Afterwards, `references` holds every remaining
/// block's count of pushed labels and terminator edges targeting it, plus the implicit
/// program-entry edge.
#[must_use]
fn remove_unreachable_blocks(module: &mut Module, state: &mut RunState) -> bool {
    let RunState { reachable, pending, references, order, .. } = state;
    references.clear();
    references.resize(module.blocks.len(), 0);
    if module.blocks.is_empty() {
        return false;
    }
    // Count the implicit program-entry edge.
    references[BlockId::ENTRY] = 1;
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
                references[*target] += 1;
                pending.push(*target);
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| {
                references[target] += 1;
                pending.push(target);
            });
        }
    }
    if reachable.count() == module.blocks.len() {
        return false;
    }
    order.clear();
    order.extend(reachable.iter());
    retain_blocks(module, order);
    // Only reachable blocks were scanned, so the counts already exclude the removed blocks.
    for (block, &old) in order.iter().enumerate() {
        references[BlockId::from_usize(block)] = references[old];
    }
    references.truncate(order.len());
    true
}

/// Merges each block into its only predecessor, given every block's `references` count.
fn coalesce_blocks(module: &mut Module, state: &mut RunState) -> bool {
    let RunState { references, retained, order, .. } = state;
    retained.clear_to(module.blocks.len());
    retained.insert_all();
    for predecessor in module.blocks.indices() {
        if !retained.contains(predecessor) {
            continue;
        }
        while let Some(TerminatorKind::Jump(target)) = terminator_kind(&module.blocks[predecessor])
        {
            let target = *target;
            if target == predecessor || references[target] != 1 || !retained.contains(target) {
                break;
            }

            let absorption = absorption(&module.blocks[target]);
            if matches!(absorption, Absorption::Blocked) {
                break;
            }

            // predecessor: prefix; jump target
            // target [loop]: suffix => predecessor [loop]: prefix; suffix
            let mut instructions = std::mem::take(&mut module.blocks[target].instructions);
            let mut terminator = module.blocks[target].terminator.take();
            // NOTE: The callee starts at the first instruction of its merged code, which no jump
            // enters now. An event that the instruction already has marks another entry of this
            // code, which stays.
            if let Some(function) = module.blocks[target].metadata.function_invoke {
                if let Some(instruction) = instructions.first_mut() {
                    if instruction.metadata.function_invoke().is_none() {
                        instruction.metadata.set_function_invoke(function);
                    }
                } else if let Some(terminator) = &mut terminator
                    && terminator.metadata.function_invoke().is_none()
                {
                    terminator.metadata.set_function_invoke(function);
                }
            }
            // The effect describes the successor's code, the slot it returns through and its net
            // stack change, so every other entry into the successor sees the same one, and an
            // effect the successor already declares is kept.
            if let Absorption::Passes(effect) = absorption
                && let Some(terminator) = &terminator
            {
                terminator.kind.visit_targets(|next| {
                    module.blocks[next].metadata.call_effect.get_or_insert(effect);
                });
            }
            module.comments.absorb(module.blocks[predecessor].label, module.blocks[target].label);
            module.blocks[predecessor].metadata.in_loop |= module.blocks[target].metadata.in_loop;
            module.blocks[predecessor].metadata.repeats |= module.blocks[target].metadata.repeats;
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

/// What absorbing a block into its predecessor does to the block's call effect.
enum Absorption {
    /// The block keeps its effect: its code ends the call, returns through a jump to the address
    /// its predecessor pushed, which the verifier finds in the merged block, or has no effect.
    Free,
    /// The successors take the effect over.
    Passes(CallEffect),
    /// The code after the block no longer shows where it returns, because it calls out or
    /// consumes or copies its return address, so the block stays apart.
    Blocked,
}

fn absorption(block: &Block) -> Absorption {
    let Some(effect) = block.metadata.call_effect else { return Absorption::Free };
    let instructions = &block.instructions;
    let Some(Terminator { kind, .. }) = &block.terminator else { return Absorption::Blocked };
    match kind {
        TerminatorKind::Op(opcode) if *opcode != op::JUMP => return Absorption::Free,
        TerminatorKind::Op(_) => {
            let returns_here = effect
                .after(instructions, StackEffect::new(0, 0))
                .is_some_and(|effect| effect.ret == 0);
            return if returns_here { Absorption::Free } else { Absorption::Blocked };
        }
        _ => {}
    }
    let pushes_address = instructions.iter().enumerate().any(|(at, inst)| {
        inst.pushed_block().is_some()
            && !instructions.get(at + 1).is_some_and(|next| next.opcode == op::JUMPI)
    });
    let after = default_terminator_stack_effect(kind)
        .filter(|_| !pushes_address)
        .and_then(|last| effect.after(instructions, last));
    after.map_or(Absorption::Blocked, Absorption::Passes)
}

fn simplify_known_jumps(module: &mut Module) -> bool {
    let mut changed = false;
    for block in &mut module.blocks {
        if let Some(term) = &mut block.terminator
            && term.kind == TerminatorKind::Op(op::JUMP)
            && let Some(last) = block.instructions.last()
            && last.is_encoded_push()
            && !last.keeps_with_next()
            && let Some(target) = last.pushed_block()
            && is_split_point(&block.instructions, block.instructions.len() - 1)
        {
            // push target; jump -> jump target
            // NOTE: The jump is the root and the pushed label its operand, so only the label's
            // function events move.
            term.metadata.move_function_events(&last.metadata);
            term.kind = TerminatorKind::Jump(target);
            block.instructions.pop();
            changed = true;
        }
    }
    changed
}

/// The function entry that an edge through a thunk chain carries to its destination, with that
/// destination, when the chain ends at an existing label that the edge exclusively reaches and
/// every thunk of the chain that enters a function agrees on it.
fn thunk_chain_invoke(
    start: BlockId,
    thunks: &FxHashMap<BlockId, Thunk>,
    references: &IndexVec<BlockId, usize>,
) -> Option<(BlockId, DebugFunction)> {
    let mut function = None;
    let mut target = start;
    for _ in 0..thunks.len() {
        let Some(thunk) = thunks.get(&target) else { break };
        if references.get(thunk.next) != Some(&1) {
            return None;
        }
        if let Some(entered) = thunk.entered {
            if function.is_some_and(|other| other != entered) {
                return None;
            }
            function = Some(entered);
        }
        target = thunk.next;
    }
    (!thunks.contains_key(&target)).then_some((target, function?))
}
