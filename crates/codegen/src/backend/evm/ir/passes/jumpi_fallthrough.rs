//! Turn a branch into the block that follows it around, so the block is entered by falling in.
//!
//! Tail merging and layout leave `condition; push next; jumpi; jump other` where `next` is the
//! very next block: the `JUMPI` jumps over the `JUMP` and the next block's `JUMPDEST` receives
//! it. The pass turns the test around, `condition; iszero; push other; jumpi`, and lets control
//! fall into `next`. A condition that already ends in an `ISZERO` drops it instead, and one that
//! ends in `EQ` or `SUB` trades it for the other. This saves
//! the second transfer and, unless another reference keeps it, the `JUMPDEST` of `next`, three
//! bytes for the common `PUSH1` labels and one more for the dropped `JUMPDEST`. It costs an
//! `ISZERO` on both edges, which a jump to `other` repays: the old branch ran `PUSH; JUMPI; PUSH;
//! JUMP` there, and the new one runs `ISZERO; PUSH; JUMPI`.
//!
//! [`Target::falls_through_turned_branch`] prices both shapes with the edge frequencies of the
//! branch. A cold `next` never runs, and a cold `other` never repays. A branch in a loop enters
//! `next` [`Target::UNCOUNTED_LOOP_ITERATIONS`] times for each entry of `other` when `next` is in
//! a loop, which gas builds do not pay the `ISZERO` for; it enters `other` that often when only
//! `other` is in a loop, as the latch of a loop whose exit follows it does. Every other branch
//! splits its executions equally. Size builds take the bytes unless a loop's extra gas outweighs
//! their deposit, which at one optimizer run it never does. The loop odds are a static guess:
//! the header of an inner loop whose exit continues the outer one has both edges in loops and
//! counts as staying in the loop.
//!
//! The pass runs once the layout is final and never moves a block, so every other fallthrough
//! stays. It skips a branch whose boundary before the `PUSH` is not a legal split point, and a
//! jump that declares a call effect.

use super::{
    EvmPass,
    utils::{count_block_references, is_split_point},
};
use crate::{
    backend::evm::{
        ir::{BlockId, BlockTarget, Instruction, Module, PushValue, TerminatorKind},
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
        let mut references = Default::default();
        count_block_references(module, &mut references);
        let mut changed = false;
        for block_id in module.blocks.indices() {
            let Some(next) = module.next_block(block_id) else { continue };
            let block = &module.blocks[block_id];
            if let Some(term) = &block.terminator
                && let TerminatorKind::Jump(other) = term.kind
                && term.metadata.function_invoke().is_none()
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
                        iszero
                            .metadata
                            .copy_source_debug_from(&block.instructions[push + 1].metadata);
                        block.instructions.insert(push, iszero);
                    }
                    TurnedTest::Drops => {
                        block.instructions.remove(push - 1);
                    }
                    TurnedTest::Swaps => {
                        let condition = &mut block.instructions[push - 1];
                        let inverse = if condition.opcode == op::EQ { op::SUB } else { op::EQ };
                        condition.replace_preserving_metadata(Instruction::opcode(inverse));
                    }
                }
                if let Some(term) = &mut block.terminator {
                    term.kind = TerminatorKind::Jump(next);
                }
                references[next] -= 1;
                changed = true;
            }
        }
        changed
    }
}

/// How often a branch of `block` enters `next` and `other` relative to each other: never a cold
/// block, and otherwise equally, except in a loop. A branch in a loop is assumed to stay in it
/// through `next` when `next` is in a loop, [`Target::UNCOUNTED_LOOP_ITERATIONS`] times for each
/// time it enters `other`, so a loop whose edges both stay in loops, as the header of an inner
/// loop whose exit continues the outer one, keeps its jump instead of risking the `ISZERO` on
/// every iteration. It enters `other` that often when only `other` is in a loop.
fn edge_runs(module: &Module, block: BlockId, next: BlockId, other: BlockId) -> (u64, u64) {
    let (next, other) = (&module.blocks[next].metadata, &module.blocks[other].metadata);
    match (next.hotness.is_cold(), other.hotness.is_cold()) {
        (true, _) => (0, 1),
        (_, true) => (1, 0),
        _ if !module.blocks[block].metadata.in_loop => (1, 1),
        _ if next.in_loop => (Target::UNCOUNTED_LOOP_ITERATIONS, 1),
        _ if other.in_loop => (1, Target::UNCOUNTED_LOOP_ITERATIONS),
        _ => (1, 1),
    }
}
