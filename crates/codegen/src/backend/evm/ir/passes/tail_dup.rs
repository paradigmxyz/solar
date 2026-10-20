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
//! larger than the jump.
//!
//! # Safety
//!
//! A block's entry stack is the same from every predecessor, so its code runs on the same stack
//! in a copy. Only position-independent code is copied: concrete immediates, data offsets and
//! sizes, block labels, stack operations, conditional jumps to a label pushed right before them,
//! and opcodes that observe neither their position nor the gas left. A label keeps naming the
//! same block, and the copy takes whole blocks, so `keep_with_next` boundaries stay intact.
//! Immutable and library placeholders, which the assembler patches by site, stay where they are.
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
        ir::{Block, BlockId, Instruction, Module, TerminatorKind},
        op,
    },
    target::{Cost, Target},
};
use solar_data_structures::index::IndexVec;
use solar_sema::Gcx;

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
        let mut changed = false;
        // A latch's copy ends with the latch's jump to the header, which the next round copies.
        for _ in 0..MAX_ROUNDS {
            let paths = incoming_paths(module);
            let mut round = false;
            for caller in module.blocks.indices() {
                if let Some(callee) = candidate(module, target, &paths, caller) {
                    let tail = &module.blocks[callee];
                    let instructions = tail.instructions.clone();
                    let terminator = tail.terminator.clone();
                    // jump tail -> tail.instructions; tail.terminator
                    let block = &mut module.blocks[caller];
                    block.instructions.extend(instructions);
                    block.terminator = terminator;
                    round = true;
                }
            }
            changed |= round;
            if !round {
                break;
            }
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

/// The block `caller` jumps to, when copying it into `caller` pays.
fn candidate(
    module: &Module,
    target: Target,
    paths: &IndexVec<BlockId, u32>,
    caller: BlockId,
) -> Option<BlockId> {
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
            // The copy jumps where the tail falls through, which must leave the loop.
            if module.blocks[next].metadata.in_loop {
                return None;
            }
            target.label_push() + target.opcode(op::JUMP)
        }
        TerminatorKind::Jump(_) => target.label_push() + target.opcode(op::JUMP),
        // A terminal tail runs once per call, which `inline-returns` already weighs.
        TerminatorKind::Op(_) => return None,
        TerminatorKind::JumpI { .. } | TerminatorKind::IndexedJump(_) => return None,
    };
    let copy = copy_cost(target, tail)? + ending;
    target.duplicates_loop_block(copy, paths[callee]).then_some(callee)
}

/// Whether `block` holds hot code of a loop.
fn in_hot_loop(block: &Block) -> bool {
    block.metadata.in_loop && !block.metadata.hotness.is_cold()
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
    let definition = op::definition(inst.opcode)?;
    // A conditional jump to the label pushed right before it goes to the same block from a copy.
    let direct_jumpi = inst.opcode == op::JUMPI
        && before.last().is_some_and(|label| label.pushed_block().is_some());
    if !definition.is_position_independent() && !direct_jumpi {
        return None;
    }
    Some(target.opcode(inst.opcode))
}
