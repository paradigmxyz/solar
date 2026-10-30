//! Turn a branch into the block that follows it around, so the block is entered by falling in.
//!
//! Tail merging and layout leave `condition; push next; jumpi; jump other` where `next` is the
//! very next block: the `JUMPI` jumps over the `JUMP` and the next block's `JUMPDEST` receives
//! it. The pass turns the test around, `condition; iszero; push other; jumpi`, and lets control
//! fall into `next`. A condition that already ends in an `ISZERO` drops it instead, and one that
//! ends in `EQ` or `SUB` trades it for the other. This saves the second `PUSH; JUMP` and, unless
//! another reference keeps it, the `JUMPDEST` of `next`. It costs an `ISZERO` on both edges,
//! which a jump to `other` repays.
//!
//! [`Target::falls_through_turned_branch`] prices both shapes with the odds of the two edges
//! from [`edge_runs`].
//!
//! The shape arises where a layout places the target of a branch's `PUSH; JUMPI` next by chance.
//! Block layout builds traces along terminator edges only, so after tail merging splits a
//! branch's fallthrough into a shared tail, the loop body or branch target it leaves behind lands
//! next in index order, with a `JUMP` over it. Block layout, loop layout and late structural
//! cleanup, the passes that fix a layout, call [`turn_branches`] when they finish, so later
//! passes see the turned branch; the `jumpi-fallthrough` pass runs it alone, for tests. It never
//! moves a block, so every other fallthrough stays, and it skips a branch whose boundary before
//! the `PUSH` is not a legal split point. Nothing turns at `-O none`.

use super::{
    EvmPass,
    utils::{count_block_references, is_split_point},
};
use crate::{
    backend::evm::{
        ir::{BlockId, BlockTarget, Instruction, Module, PushValue, Terminator, TerminatorKind},
        op,
    },
    target::{Target, TurnedTest},
};
use solar_sema::Gcx;

pub(super) struct JumpiFallthrough;

impl EvmPass for JumpiFallthrough {
    fn name(&self) -> &'static str {
        "jumpi-fallthrough"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        turn_branches(gcx, module)
    }
}

/// Turns every profitable branch into the block that follows it around.
pub(super) fn turn_branches(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if module.optimization.is_none() {
        return false;
    }
    let mut references = Default::default();
    count_block_references(module, &mut references);
    let mut changed = false;
    for block_id in module.blocks.indices() {
        let Some(next) = module.next_block(block_id) else { continue };
        let block = &module.blocks[block_id];
        if let Some(term) = &block.terminator
            && let TerminatorKind::Jump(other) = term.kind
            && other != next
            && let [.., pushed, jumpi] = block.instructions.as_slice()
            && pushed.is_encoded_push()
            && pushed.pushed_block() == Some(next)
            && jumpi.as_evm_opcode() == Some(op::JUMPI)
            && !pushed.keeps_with_next()
            && !jumpi.keeps_with_next()
        {
            let push = block.instructions.len() - 2;
            let condition = push.checked_sub(1).map(|at| &block.instructions[at]);
            let test = match condition.and_then(Instruction::as_evm_opcode) {
                Some(op::ISZERO) => TurnedTest::Drops,
                Some(op::EQ | op::SUB) => TurnedTest::Swaps,
                _ => TurnedTest::Adds,
            };
            let boundary = push - usize::from(test == TurnedTest::Drops);
            if !is_split_point(&block.instructions, boundary) {
                continue;
            }
            let (next_runs, other_runs) = edge_runs(module, block_id, next, other);
            let target = block.target(gcx, module.optimization, module.code_follows);
            if !target.falls_through_turned_branch(
                next_runs,
                other_runs,
                test,
                references[next] > 1,
            ) {
                continue;
            }
            // <condition>
            // iszero
            // push other
            // jumpi
            // (falls into next)
            let block = &mut module.blocks[block_id];
            block.instructions[push].value = Some(PushValue::Block(other));
            match test {
                TurnedTest::Adds => {
                    let mut iszero = Instruction::opcode(op::ISZERO);
                    iszero.metadata.copy_source_debug_from(&block.instructions[push + 1].metadata);
                    block.instructions.insert(push, iszero);
                }
                TurnedTest::Drops => {
                    let removed = block.instructions.remove(push - 1);
                    block.instructions[push - 1].metadata.absorb_debug_info(&removed.metadata);
                }
                TurnedTest::Swaps => {
                    let condition = &mut block.instructions[push - 1];
                    let inverse = if condition.opcode == op::EQ { op::SUB } else { op::EQ };
                    condition.replace_preserving_metadata(Instruction::opcode(inverse));
                }
            }
            // NOTE: The fallthrough emits no instruction, and the old jump's function events
            // belong to its edge into `other`, which is now the `JUMPI`'s. Keep only its source
            // spans rather than put those events on the edge into `next`.
            let mut fallthrough = Terminator::new(TerminatorKind::Jump(next));
            if let Some(term) = &block.terminator {
                fallthrough.metadata.copy_source_debug_from(&term.metadata);
            }
            block.terminator = Some(fallthrough);
            changed = true;
        }
    }
    changed
}

/// How often a branch of `block` enters `next` and `other` relative to each other: never a cold
/// block, and otherwise equally, except in a loop. A branch enters `next`
/// [`Target::UNCOUNTED_LOOP_ITERATIONS`] times for each time it enters `other` when `next` is in
/// a loop, and the reverse when only `other` is and the branch is too, as at the latch of a loop
/// whose exit follows it. When both edges are in loops, as at the header of an inner loop whose
/// exit continues the outer one, this guesses that the branch stays in `next`, so gas builds keep
/// the jump. The branch's own flag is not trusted for that guess: internal-call continuations and
/// trampolines are never marked as loop blocks.
fn edge_runs(module: &Module, block: BlockId, next: BlockId, other: BlockId) -> (u64, u64) {
    let in_loop = module.blocks[block].metadata.in_loop;
    let (next, other) = (&module.blocks[next].metadata, &module.blocks[other].metadata);
    match (next.hotness.is_cold(), other.hotness.is_cold()) {
        (true, _) => (0, 1),
        (_, true) => (1, 0),
        _ if next.in_loop => (Target::UNCOUNTED_LOOP_ITERATIONS, 1),
        _ if in_loop && other.in_loop => (1, Target::UNCOUNTED_LOOP_ITERATIONS),
        _ => (1, 1),
    }
}
