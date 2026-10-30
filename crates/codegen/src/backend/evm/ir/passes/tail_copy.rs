//! Copy a block's code into the blocks that jump to it: `copy-loop-blocks`, `copy-joins`, and
//! `inline-returns`.
//!
//! A transfer to a shared block costs a pushed label, a `JUMP`, and the target's `JUMPDEST`.
//! Each pass replaces a block's explicit jump to another block, its tail, with the tail's
//! instructions and terminator, so the path runs straight on and the transfer is gone. The three
//! passes differ in which tails qualify and in what the copy must repay: `copy-loop-blocks` copies
//! hot loop blocks, including their branches and fallthroughs, before the final layout;
//! `copy-joins` copies straight-line joins that end in a jump, after the final layout; and
//! `inline-returns` copies terminal tails no larger than the jump to them.
//!
//! A tail qualifies for a caller only when the caller ends with an explicit `Jump` to it at a
//! split point, and the tail is neither the entry block, the caller itself, nor the caller's
//! fallthrough, whose transfer may cost fewer bytes. A copy takes whole blocks, so
//! `keep_with_next` boundaries stay intact. The original tail stays for its other predecessors;
//! CFG cleanup removes it once no jump reaches it, and `copy-joins` removes it itself.
//!
//! Only position-independent code is copied, so a copy runs on the same stack as the tail did:
//! a block's entry stack is the same from every predecessor. Immediate pushes, physical stack
//! operations, and opcodes that observe neither their position nor the gas left qualify; the
//! straight-line passes narrow the opcodes further.
//! Immutable and library placeholders, which the assembler patches by site, stay where they are,
//! and so do opcodes the target lacks, whose new uses the checks between passes reject.
//!
//! NOTE: A copy keeps its instructions' source locations and function events, but not the
//! checkpoints of the removed transfer or the function entry event of the tail's `JUMPDEST`,
//! which no longer run on that path. Keeping an executable jump for them would defeat the
//! rewrite; recording one on the caller's last instruction could overwrite that instruction's
//! own transition. Debug origins never affect eligibility.
//!
//! # `copy-loop-blocks`: loop blocks
//!
//! A loop body that branches joins its paths again before the next iteration: each path ends
//! with a jump to a shared block, often a latch that advances the loop's words and jumps back to
//! the header. Every such transfer is paid on each iteration that takes it. This pass copies a
//! small block into each block of the same loop that ends with an explicit jump to it. A copy of
//! a latch then ends with the latch's jump to the header, which a second round can copy as well:
//! the header's test then runs at the end of each path, and only the loop's exit pays a jump.
//!
//! The copy ends as the block does. A jump the block takes explicitly stays a jump of the same
//! cost. A block that falls through into its successor gets an explicit jump at the end of each
//! copy, so it qualifies only when that successor leaves the loop, a path taken once per exit
//! instead of once per iteration. A block that ends the call, such as one that returns, runs
//! once per call, so it stays shared: `inline-returns` copies such tails when they are no
//! larger than the jump. A loop header that returns on the loop's exit is the exception: its
//! conditional jump back into the loop runs it on every iteration, so it is copied like any
//! other loop block, with its return. Whether `inline-returns` copied a shared return into the
//! header first then no longer decides whether the latch jumps back to it, and that depended on
//! where block layout had placed the shared return, which other functions' code moves.
//!
//! Beyond the shared rules, data offsets and sizes, block labels, and conditional jumps to a
//! label pushed right before them are copied: a label keeps naming the same block.
//!
//! A copy never takes the stack higher than the path it replaces. That path pushed the label of
//! its jump into the block one word over the block's entry stack, ran the block's code, and went
//! on into the next block's. A copy of a block that falls through ends with a jump whose label
//! goes on the stack the block leaves, so such a block qualifies only when the path reaches that
//! height anyway: in the block's own code, or in the next block's before it can branch away, as
//! a callee entered right after its return address does. Otherwise the copy could exceed the
//! 1024-word limit where the path stays within it.
//!
//! Only gas builds copy, and only between hot blocks inside a loop. A copy saves the transfer on
//! every iteration through the edge and deposits the copied bytes, less the removed label push
//! and jump, as [`crate::target::Target::duplicates_loop_block`] prices it over the optimizer
//! runs. The paths into a block, its incoming edges and the conditional jumps inside other blocks
//! that target it, share its iterations equally, so a block many paths join pays its copies back
//! more slowly.
//!
//! Below the runs that copy loop bodies, the copies into a small module together may add only a
//! share of its code, [`crate::target::Target::loop_join_copy_budget`], and those that save the
//! most gas per added byte go first. A contract built around one small loop, where a couple of
//! copies would grow it by a tenth or more, then keeps its size at the default runs, while the
//! copies of a larger contract's loops, a small share of it, all go.
//!
//! The pass runs after the last structural sharing, so tail merging does not fold the copies
//! back together, and before the final CFG cleanup and loop layout, which remove the blocks the
//! copies replaced and pick the loop's fallthroughs.
//!
//! # `copy-joins`: joins after layout
//!
//! When both the incoming and outgoing jumps are explicit, copying a straight-line join into its
//! predecessor removes one destination push, jump, and label entry per execution. The outgoing
//! transfer stays intact. Added bytes must cost less to deposit than the removed transfers cost
//! over the configured optimizer runs; size mode does not duplicate joins. This runs after
//! structural sharing and layout: before loop placement these joins can still have fallthrough
//! successors.
//!
//! Beyond the shared rules, only immediate pushes qualify, and of the computations only those
//! that push one word and write neither memory nor storage. Cold callers and tails stay shared.
//! Straight-line loop joins qualify, but no branches are copied: header tests and loop rotation
//! remain with loop layout. Block order and existing label identities stay intact. A tail left
//! without references is removed, and assembly omits labels whose last reference disappears.
//!
//! Copies start beyond the one-byte address range, using an instruction-only lower bound on the
//! prefix size, so they cannot widen the labels and indexed tables packed by block layout.
//!
//! # `inline-returns`: small returns
//!
//! Sharing a one- or two-instruction return tail can cost more than spelling it out. This pass
//! replaces direct jumps with the target's complete terminal body when its encoded size fits the
//! transfer budget. Gas mode budgets a `PUSH1` and `JUMP`; size mode budgets the target's label
//! push and `JUMP`. A destination address is a label, never the `PUSH0` that a zero immediate
//! would use. Across the benchmark corpus this price shrank runtime code by 1,126 bytes, with 244
//! contracts smaller and 3 larger by 21.
//!
//! Beyond the shared rules, only immediate pushes and the computations `copy-joins` copies
//! qualify. The rewrite removes transfer gas independently of execution frequency. It runs after
//! sharing, which can create these tiny tails. The default pipeline cleans up stack identities
//! exposed by the copied return.

use super::{
    EvmPass,
    utils::{count_block_references, instruction_size_lower_bound, is_split_point, retain_blocks},
};
use crate::{
    backend::evm::{
        ir::{Block, BlockId, Instruction, Module, ModuleTarget, Terminator, TerminatorKind},
        op,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;
use std::cmp::Ordering;

pub(super) struct CopyLoopBlocks;

impl EvmPass for CopyLoopBlocks {
    fn name(&self) -> &'static str {
        "copy-loop-blocks"
    }

    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        module.optimization.is_gas()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = module.target(gcx);
        let mut budget = target.loop_join_copy_budget(code_bytes(target, module));
        let mut changed = false;
        for _ in 0..MAX_ROUNDS {
            let paths = incoming_paths(module);
            let mut candidates = module
                .blocks
                .indices()
                .filter_map(|caller| {
                    let (callee, copy) = loop_candidate(module, target, &paths, caller)?;
                    let (added, saved) = target.loop_block_copy(copy, paths[callee]);
                    Some((caller, callee, added, saved))
                })
                .collect::<Vec<_>>();
            // The budget takes the copies that save the most gas per added byte first.
            candidates.sort_by(|a, b| by_gas_per_byte((a.2, a.3), (b.2, b.3)).then(a.0.cmp(&b.0)));
            // Every copy of a round takes its block as the round found it, so a block grown by
            // one copy still qualifies for the others.
            let mut copies = Vec::new();
            for (caller, callee, added, _) in candidates {
                if added <= budget {
                    budget -= added;
                    copies.push((caller, tail_body(module, callee)));
                }
            }
            if copies.is_empty() {
                break;
            }
            for (caller, tail) in copies {
                append_tail(module, caller, tail);
            }
            changed = true;
        }
        changed
    }
}

pub(super) struct CopyJoins;

impl EvmPass for CopyJoins {
    fn name(&self) -> &'static str {
        "copy-joins"
    }

    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        module.optimization.is_gas()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = module.runtime_target(gcx);
        // Bound the removed transfer by its smallest encoding: even a non-entry label can
        // resolve to zero when the entry block falls through without emitting instructions.
        let transfer = target.push(U256::ZERO) + target.opcode(op::JUMP);
        let saved = Cost { bytes: 0, ..transfer + target.opcode(op::JUMPDEST) };
        let added = (target.label_push() + target.opcode(op::JUMP)).bytes - transfer.bytes;
        let mut prefix_bytes = 0;
        let mut references = IndexVec::new();
        count_block_references(module, &mut references);
        let mut duplicated = DenseBitSet::new_empty(module.blocks.len());
        let mut changed = false;
        for caller in module.blocks.indices() {
            if prefix_bytes <= usize::from(u8::MAX) {
                prefix_bytes += (module.blocks[caller].instructions.iter())
                    .map(|inst| instruction_size_lower_bound(target, inst))
                    .sum::<usize>();
                continue;
            }
            let Some(callee) = jump_callee(module, caller) else { continue };
            let tail = &module.blocks[callee];
            let Some(&TerminatorKind::Jump(successor)) = tail.terminator.as_ref().map(|t| &t.kind)
            else {
                continue;
            };
            if module.blocks[caller].metadata.hotness.is_cold()
                || tail.metadata.hotness.is_cold()
                || successor == callee
                || successor == caller
                || Some(successor) == module.next_block(callee)
            {
                continue;
            }
            let repaid = |copy: Cost| {
                target.lifetime_gas(Cost::new(0, added + copy.bytes)) < target.lifetime_gas(saved)
            };
            if copy_cost(target, tail, false, repaid).is_none() {
                continue;
            }
            references[callee] -= 1;
            references[successor] += 1;
            duplicated.insert(callee);
            let tail = tail_body(module, callee);
            append_tail(module, caller, tail);
            changed = true;
        }
        if !changed {
            return false;
        }
        // Remove duplicated tails left without references.
        let order = module
            .blocks
            .indices()
            .filter(|&block| !(duplicated.contains(block) && references[block] == 0))
            .collect::<Vec<_>>();
        if order.len() < module.blocks.len() {
            retain_blocks(module, &order);
        }
        true
    }
}

pub(super) struct InlineReturns;

impl EvmPass for InlineReturns {
    fn name(&self) -> &'static str {
        "inline-returns"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = module.runtime_target(gcx);
        let address = if module.optimization.is_gas() {
            target.opcode(op::PUSH1)
        } else {
            target.label_push()
        };
        let budget = (address + target.opcode(op::JUMP)).bytes;
        let mut changed = false;
        for caller in module.blocks.indices() {
            let Some(callee) = jump_callee(module, caller) else { continue };
            let tail = &module.blocks[callee];
            let Some(&TerminatorKind::Op(opcode)) = tail.terminator.as_ref().map(|t| &t.kind)
            else {
                continue;
            };
            let ending = target.opcode(opcode).bytes;
            if copy_cost(target, tail, false, |copy| copy.bytes + ending <= budget).is_none() {
                continue;
            }
            let tail = tail_body(module, callee);
            append_tail(module, caller, tail);
            changed = true;
        }
        changed
    }
}

/// The rounds of copying: a latch, then the header its copies jump to.
const MAX_ROUNDS: usize = 2;

/// The block `caller` jumps to explicitly, when a copy of it may take the jump's place.
fn jump_callee(module: &Module, caller: BlockId) -> Option<BlockId> {
    let block = &module.blocks[caller];
    let &TerminatorKind::Jump(callee) = &block.terminator.as_ref()?.kind else { return None };
    (callee != BlockId::ENTRY
        && callee != caller
        && Some(callee) != module.next_block(caller)
        && is_split_point(&block.instructions, block.instructions.len()))
    .then_some(callee)
}

/// The instructions and terminator a copy of the block `callee` appends to its caller.
fn tail_body(module: &Module, callee: BlockId) -> (Vec<Instruction>, Option<Terminator>) {
    let tail = &module.blocks[callee];
    (tail.instructions.clone(), tail.terminator.clone())
}

/// Replaces the jump that ends `caller` with a tail.
fn append_tail(
    module: &mut Module,
    caller: BlockId,
    (instructions, terminator): (Vec<Instruction>, Option<Terminator>),
) {
    // jump tail -> tail.instructions; tail.terminator
    let block = &mut module.blocks[caller];
    block.instructions.extend(instructions);
    block.terminator = terminator;
}

/// The cost of copying the instructions of `block`, or `None` when one of them cannot be copied
/// or `fits` rejects the running cost. Costs only grow, so a `fits` that stays false once false
/// is checked before each instruction and a tail that cannot fit stops the scan early. A
/// `loop_block` copy also takes labels, data offsets and sizes, and conditional jumps.
fn copy_cost(
    target: Target,
    block: &Block,
    loop_block: bool,
    fits: impl Fn(Cost) -> bool,
) -> Option<Cost> {
    let mut cost = Cost::ZERO;
    let mut after_label = false;
    for inst in &block.instructions {
        if !fits(cost) {
            return None;
        }
        cost += instruction_cost(target, inst, loop_block, after_label)?;
        after_label = inst.pushed_block().is_some();
    }
    fits(cost).then_some(cost)
}

/// The cost of one copied instruction, which follows a pushed block label if `after_label`.
fn instruction_cost(
    target: Target,
    inst: &Instruction,
    loop_block: bool,
    after_label: bool,
) -> Option<Cost> {
    // Immutable and library pushes have no concrete immediate and no address, so they stay.
    if inst.is_encoded_push() {
        if let Some(value) = inst.concrete_immediate() {
            return Some(target.push(value));
        }
        let address = inst.pushed_block().is_some()
            || inst.pushed_data().is_some()
            || inst.pushed_data_size().is_some();
        return (loop_block && address).then(|| target.label_push());
    }
    if let Some(stack_op) = inst.as_stack_op() {
        return target.stack_op(stack_op);
    }
    let definition = op::definition(inst.opcode)?;
    // A conditional jump to the label pushed right before it goes to the same block from a copy.
    let copyable = if loop_block {
        definition.is_position_independent() || (inst.opcode == op::JUMPI && after_label)
    } else {
        op::is_unaffected_by_preceding_push(inst.opcode)
    };
    (copyable && definition.is_available(target.evm_version())).then(|| target.opcode(inst.opcode))
}

/// The number of paths into each block: edges of terminators, and conditional jumps inside
/// blocks to the label pushed right before them.
fn incoming_paths(module: &Module) -> IndexVec<BlockId, u32> {
    let mut paths = IndexVec::from_vec(vec![0; module.blocks.len()]);
    for block in &module.blocks {
        for pair in block.instructions.windows(2) {
            if pair[1].as_evm_opcode() == Some(op::JUMPI)
                && let Some(to) = pair[0].pushed_block()
            {
                paths[to] += 1;
            }
        }
        if let Some(terminator) = &block.terminator {
            terminator.kind.visit_targets(|to| paths[to] += 1);
        }
    }
    paths
}

/// The block `caller` jumps to and the cost of its copy, when copying it into `caller` pays.
fn loop_candidate(
    module: &Module,
    target: Target,
    paths: &IndexVec<BlockId, u32>,
    caller: BlockId,
) -> Option<(BlockId, Cost)> {
    let callee = jump_callee(module, caller)?;
    let tail = &module.blocks[callee];
    if !in_hot_loop(&module.blocks[caller]) || !in_hot_loop(tail) {
        return None;
    }
    // The copy's own transfer must not cost more on each iteration than the tail's does.
    let jump = target.label_push() + target.opcode(op::JUMP);
    let ending = match tail.terminator.as_ref()?.kind {
        // The copy jumps where the tail falls through, which must leave the loop, and must push
        // that jump's label no higher than the path through the tail already reaches.
        TerminatorKind::Jump(next) if Some(next) == module.next_block(callee) => {
            let next = &module.blocks[next];
            if next.metadata.in_loop || !label_fits_path(tail, next) {
                return None;
            }
            jump
        }
        TerminatorKind::Jump(_) => jump,
        // A tail that tests the loop runs on every iteration, whatever it does on exit.
        TerminatorKind::Op(opcode) if continues_loop(module, tail) => target.opcode(opcode),
        // A terminal tail runs once per call, which `inline-returns` already weighs.
        TerminatorKind::Op(_) => return None,
        TerminatorKind::JumpI { .. } | TerminatorKind::IndexedJump(_) => return None,
    };
    let copy = copy_cost(target, tail, true, |_| true)? + ending;
    target.duplicates_loop_block(copy, paths[callee]).then_some((callee, copy))
}

/// Whether `block` holds hot code of a loop.
fn in_hot_loop(block: &Block) -> bool {
    block.metadata.in_loop && !block.metadata.hotness.is_cold()
}

/// Whether `block` jumps conditionally to hot code of its loop, as a header does that returns on
/// the loop's exit.
fn continues_loop(module: &Module, block: &Block) -> bool {
    block.instructions.windows(2).any(|pair| {
        pair[1].as_evm_opcode() == Some(op::JUMPI)
            && pair[0].pushed_block().is_some_and(|to| in_hot_loop(&module.blocks[to]))
    })
}

/// The module's code bytes, of which its copies may add a share.
fn code_bytes(target: Target, module: &Module) -> u64 {
    let landing = u64::from(target.opcode(op::JUMPDEST).bytes);
    let jump = target.label_push() + target.opcode(op::JUMP);
    module
        .blocks
        .iter()
        .map(|block| {
            let body =
                block.instructions.iter().map(|inst| u64::from(instruction_bytes(target, inst)));
            let end = block.terminator.as_ref().map_or(0, |term| match &term.kind {
                TerminatorKind::Jump(_) => jump.bytes,
                TerminatorKind::JumpI { .. } => {
                    (target.label_push() + target.opcode(op::JUMPI)).bytes
                }
                TerminatorKind::IndexedJump(_) => target.opcode(op::JUMP).bytes,
                TerminatorKind::Op(opcode) => target.opcode(*opcode).bytes,
            });
            landing + body.sum::<u64>() + u64::from(end)
        })
        .sum()
}

/// The encoded bytes of one instruction, a pushed label or placeholder as a label push.
fn instruction_bytes(target: Target, inst: &Instruction) -> u32 {
    if inst.is_encoded_push() {
        return inst
            .concrete_immediate()
            .map_or(target.label_push().bytes, |value| target.push(value).bytes);
    }
    inst.as_stack_op()
        .and_then(|stack_op| target.stack_op(stack_op))
        .map_or_else(|| target.opcode(inst.opcode).bytes, |cost| cost.bytes)
}

/// Whether the label a copy of `block` pushes to jump to `next`, where the block falls through,
/// stays within the highest stack of the path the copy replaces: the label of the jump into the
/// block, one word over its entry stack, the block's own peak, or the first word the code of
/// `next` pushes over the stack the block leaves, before `next` can branch away.
fn label_fits_path(block: &Block, next: &Block) -> bool {
    let (mut height, mut peak) = (0i64, 1i64);
    for inst in &block.instructions {
        height += stack_growth(inst);
        peak = peak.max(height);
    }
    if height < peak {
        return true;
    }
    let mut rise = 0i64;
    for inst in &next.instructions {
        rise += stack_growth(inst);
        if rise > 0 {
            return true;
        }
        if inst.as_evm_opcode() == Some(op::JUMPI) {
            return false;
        }
    }
    false
}

/// The words `inst` adds to the stack, negative when it removes some.
fn stack_growth(inst: &Instruction) -> i64 {
    let effect = inst.stack_effect();
    i64::from(effect.outputs) - i64::from(effect.inputs)
}

/// Orders copies that add `added` bytes and save `saved` gas by the gas they save per added byte,
/// highest first. Copies that add no bytes come first: compared by cross-multiplication, one that
/// also saves no gas would tie with every other copy, which orders nothing consistently.
fn by_gas_per_byte((added_a, saved_a): (u64, u64), (added_b, saved_b): (u64, u64)) -> Ordering {
    (added_a != 0).cmp(&(added_b != 0)).then_with(|| {
        (u128::from(saved_b) * u128::from(added_a))
            .cmp(&(u128::from(saved_a) * u128::from(added_b)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gas_per_byte_orders_totally() {
        // (added, saved), with copies that add no bytes, save no gas, or both.
        let copies = [(10, 10), (0, 0), (4, 8), (0, 12), (1, 120), (3, 0), (2, 4), (6, 0)];
        for a in copies {
            for b in copies {
                assert_eq!(by_gas_per_byte(a, b), by_gas_per_byte(b, a).reverse(), "{a:?} {b:?}");
                for c in copies {
                    if by_gas_per_byte(a, b).is_le() && by_gas_per_byte(b, c).is_le() {
                        assert!(by_gas_per_byte(a, c).is_le(), "{a:?} <= {b:?} <= {c:?}");
                    }
                }
            }
        }
        let mut sorted = copies;
        sorted.sort_by(|&a, &b| by_gas_per_byte(a, b));
        assert_eq!(sorted, [(0, 0), (0, 12), (1, 120), (4, 8), (2, 4), (10, 10), (3, 0), (6, 0)]);
    }
}
