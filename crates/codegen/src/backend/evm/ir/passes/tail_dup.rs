//! Copy small loop blocks into the loop blocks that jump to them (`tail-dup`).
//!
//! A loop body that branches joins its paths again before the next iteration: each path ends
//! with a jump to a shared block, often a latch that advances the loop's words and jumps back to
//! the header. Every such transfer costs a pushed label, a `JUMP`, and the target's `JUMPDEST` on
//! each iteration that takes it. This pass copies a small block into each block of the same loop
//! that ends with an explicit jump to it, so the path runs straight on into the copy. A copy of a
//! latch then ends with the latch's jump to the header, which a second round can copy as well:
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
//! # Safety
//!
//! A block's entry stack is the same from every predecessor, so its code runs on the same stack
//! in a copy. Only position-independent code is copied: concrete immediates, data offsets and
//! sizes, block labels, stack operations, conditional jumps to a label pushed right before them,
//! and opcodes that observe neither their position nor the gas left. A label keeps naming the
//! same block, and the copy takes whole blocks, so `keep_with_next` boundaries stay intact.
//! Immutable and library placeholders, which the assembler patches by site, stay where they are,
//! and so do opcodes the target lacks, whose new uses the checks between passes reject.
//!
//! A copy never takes the stack higher than the path it replaces. That path pushed the label of
//! its jump into the block one word over the block's entry stack, ran the block's code, and went
//! on into the next block's. A copy of a block that falls through ends with a jump whose label
//! goes on the stack the block leaves, so such a block qualifies only when the path reaches that
//! height anyway: in the block's own code, or in the next block's before it can branch away, as
//! a callee entered right after its return address does. Otherwise the copy could exceed the
//! 1024-word limit where the path stays within it.
//!
//! # Profitability
//!
//! Only gas builds copy, and only between hot blocks inside a loop. A copy saves the transfer on
//! every iteration through the edge and deposits the copied bytes, less the removed label push
//! and jump, as [`crate::target::Target::duplicates_loop_block`] prices it over the optimizer
//! runs. The paths into a block, its incoming edges and the conditional jumps inside other blocks
//! that target it, share its iterations equally, so a block many paths join pays its copies back
//! more slowly. The original block stays for its other predecessors; CFG cleanup removes it once
//! no jump reaches it.
//!
//! Below the runs that copy loop bodies, the copies into a small module together may add only a
//! share of its code, [`crate::target::Target::loop_join_copy_budget`], and those that save the
//! most gas per added byte go first. A contract built around one small loop, where a couple of
//! copies would grow it by a tenth or more, then keeps its size at the default runs, while the
//! copies of a larger contract's loops, a small share of it, all go.
//!
//! # Place in the pipeline
//!
//! The pass runs after the last structural sharing, so tail merging does not fold the copies
//! back together, and before the final CFG cleanup and loop layout, which remove the blocks the
//! copies replaced and pick the loop's fallthroughs.
//!
//! NOTE: A copy keeps its instructions' source locations, but not the function entry event of the
//! copied block's `JUMPDEST`, which no longer runs on that path; recording it on the caller's last
//! instruction could overwrite that instruction's own transition.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        ir::{Block, BlockId, Instruction, Module, ModuleTarget, TerminatorKind},
        op,
    },
    target::{Cost, Target},
};
use solar_data_structures::index::IndexVec;
use solar_sema::Gcx;
use std::cmp::Ordering;

pub(super) struct TailDup;

impl EvmPass for TailDup {
    fn name(&self) -> &'static str {
        "tail-dup"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        module.optimization(gcx).is_gas()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = module.target(gcx);
        let mut budget = target.loop_join_copy_budget(code_bytes(target, module));
        let mut changed = false;
        // A latch's copy ends with the latch's jump to the header, which the next round copies.
        for _ in 0..MAX_ROUNDS {
            let paths = incoming_paths(module);
            let mut candidates = module
                .blocks
                .indices()
                .filter_map(|caller| {
                    let (callee, copy) = candidate(module, target, &paths, caller)?;
                    let (added, saved) = target.loop_block_copy(copy, paths[callee]);
                    Some((caller, callee, added, saved))
                })
                .collect::<Vec<_>>();
            // The budget takes the copies that save the most gas per added byte first.
            candidates.sort_by(|a, b| by_gas_per_byte((a.2, a.3), (b.2, b.3)).then(a.0.cmp(&b.0)));
            // Every copy of a round takes its block as the round found it, so a block grown by
            // one copy still qualifies for the others.
            let copies = candidates
                .into_iter()
                .filter(|&(_, _, added, _)| {
                    let fits = added <= budget;
                    if fits {
                        budget -= added;
                    }
                    fits
                })
                .map(|(caller, callee, ..)| {
                    let tail = &module.blocks[callee];
                    (caller, tail.instructions.clone(), tail.terminator.clone())
                })
                .collect::<Vec<_>>();
            if copies.is_empty() {
                break;
            }
            for (caller, instructions, terminator) in copies {
                // jump tail -> tail.instructions; tail.terminator
                let block = &mut module.blocks[caller];
                block.instructions.extend(instructions);
                block.terminator = terminator;
            }
            changed = true;
        }
        changed
    }
}

/// The rounds of copying: a latch, then the header its copies jump to.
const MAX_ROUNDS: usize = 2;

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
fn candidate(
    module: &Module,
    target: Target,
    paths: &IndexVec<BlockId, u32>,
    caller: BlockId,
) -> Option<(BlockId, Cost)> {
    let block = &module.blocks[caller];
    let Some(TerminatorKind::Jump(callee)) = block.terminator.as_ref().map(|term| &term.kind)
    else {
        return None;
    };
    let callee = *callee;
    if callee == BlockId::ENTRY
        || callee == caller
        || Some(callee) == module.next_block(caller)
        || !in_hot_loop(block)
        || !is_split_point(&block.instructions, block.instructions.len())
    {
        return None;
    }
    let tail = &module.blocks[callee];
    if !in_hot_loop(tail) {
        return None;
    }
    // The copy's own transfer must not cost more on each iteration than the tail's does.
    let ending = match tail.terminator.as_ref()?.kind {
        TerminatorKind::Jump(next) if Some(next) == module.next_block(callee) => {
            // The copy jumps where the tail falls through, which must leave the loop, and must
            // push that jump's label no higher than the path through the tail already reaches.
            let next = &module.blocks[next];
            if next.metadata.in_loop || !label_fits_path(tail, next) {
                return None;
            }
            target.label_push() + target.opcode(op::JUMP)
        }
        TerminatorKind::Jump(_) => target.label_push() + target.opcode(op::JUMP),
        // A tail that tests the loop runs on every iteration, whatever it does on exit.
        TerminatorKind::Op(opcode) if continues_loop(module, tail) => target.opcode(opcode),
        // A terminal tail runs once per call, which `inline-returns` already weighs.
        TerminatorKind::Op(_) => return None,
        TerminatorKind::JumpI { .. } | TerminatorKind::IndexedJump(_) => return None,
    };
    let copy = copy_cost(target, tail)? + ending;
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

/// The cost of copying the instructions of `block`, or `None` when one of them is tied to its
/// position.
fn copy_cost(target: Target, block: &Block) -> Option<Cost> {
    let mut cost = Cost::ZERO;
    for (index, inst) in block.instructions.iter().enumerate() {
        cost += instruction_cost(target, &block.instructions[..index], inst)?;
    }
    Some(cost)
}

/// The cost of one copied instruction after `before`, its predecessors in the block.
fn instruction_cost(target: Target, before: &[Instruction], inst: &Instruction) -> Option<Cost> {
    if inst.is_encoded_push() {
        if inst.immutable_type_size().is_some() || inst.pushed_library().is_some() {
            return None;
        }
        if let Some(value) = inst.concrete_immediate() {
            return Some(target.push(value));
        }
        if inst.pushed_block().is_some()
            || inst.pushed_data().is_some()
            || inst.pushed_data_size().is_some()
        {
            return Some(target.label_push());
        }
        return None;
    }
    if let Some(stack_op) = inst.as_stack_op() {
        return target.stack_op(stack_op);
    }
    if !op::is_available(inst.opcode, target.evm_version()) {
        return None;
    }
    let definition = op::definition(inst.opcode)?;
    // A conditional jump to the label pushed right before it goes to the same block from a copy.
    let direct_jumpi = inst.opcode == op::JUMPI
        && before.last().is_some_and(|label| label.pushed_block().is_some());
    if !definition.is_position_independent() && !direct_jumpi {
        return None;
    }
    Some(target.opcode(inst.opcode))
}

/// The module's code bytes, of which its copies may add a share.
fn code_bytes(target: Target, module: &Module) -> u64 {
    let landing = u64::from(target.opcode(op::JUMPDEST).bytes);
    module
        .blocks
        .iter()
        .map(|block| {
            let body =
                block.instructions.iter().map(|inst| u64::from(instruction_bytes(target, inst)));
            let end = block.terminator.as_ref().map_or(0, |term| match &term.kind {
                TerminatorKind::Jump(_) => (target.label_push() + target.opcode(op::JUMP)).bytes,
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
