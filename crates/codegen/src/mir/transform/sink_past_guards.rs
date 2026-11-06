//! Sinks computations on calldata words past a guard into the block that continues after it.
//!
//! A guard is a conditional branch with one successor that only reverts: a range check, an
//! overflow test, a panic. Code that runs before the guard in source order but is read only after
//! it, such as `Role next = allowed ? A : B;` ahead of a check that an enum read from storage is
//! in range, computes its result on the way to the guard. The value then stays live across the
//! branch, which costs the stack scheduler a `DUP` or `SWAP`, and the revert path computes it for
//! nothing. Value checks that sit at a use, as via-IR places them, create this shape often.
//!
//! **What moves.** A set of instructions in the guard's block moves to the start of the block that
//! continues after the guard, in their original order. Members are pure instructions and reads of
//! calldata at constant offsets: a calldata word cannot change during the call, so reading it after
//! the guard gives the same word. Zero tests stay: `place-reads` places a test by its reader. A
//! member's every reader is another member, or sits in a block the continuing block dominates. The
//! reverting successor then reads none of the results. The continuing block must have the guard's
//! block as its only predecessor, so a loop header is never a target and the set runs once per
//! entry, as before.
//!
//! **Closure.** A set may only have operands that are constants, results of its own members, or
//! calldata words that nothing else in the block reads. Such a set carries no other value across
//! the guard in place of its results, and adds no read: `place-reads` reads the calldata word again
//! where it is used, which replaces the read that was carried. A set that reads any other value,
//! for example `add ptr, 32` on a memory pointer or `add arg, 64` on an argument, does not move.
//! Moving it makes the operand, which lives on past the guard anyway, be reached again after the
//! guard with a `DUP`, which costs what carrying the result cost, and it changes the order the
//! scheduler sees. An earlier version moved every pure instruction whose result was read only
//! after the guard, and it grew the corpus: solady-lib-string by 111 bytes at `-Ogas`, and
//! tempo-zones-portal's runtime gas by 20. Closed sets add no carried value and no read, so they
//! need no price from the target; the pass takes no profitability decision.
//!
//! **Placement.** The pass runs once in the lowered pipeline, after the last pass that creates
//! guards and before `place-reads`, which then places the calldata words the moved computation
//! left behind by their remaining readers.

use super::word_sequence::is_pure;
use crate::mir::{
    BlockId, Function, InstId, InstKind, Module, Terminator, ValueId,
    analysis::CfgInfo,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

pub(crate) struct SinkPastGuards;

impl MirPass for SinkPastGuards {
    fn name(&self) -> &'static str {
        "sink-past-guards"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run(func))
    }
}

fn run(func: &mut Function) -> bool {
    let guards = guards(func);
    if guards.is_empty() {
        return false;
    }
    // The readers of each value: an instruction, or `None` for a phi, which reads its input on
    // the edge from the predecessor in the block, and for a terminator.
    let mut uses = FxHashMap::<ValueId, SmallVec<[(BlockId, Option<InstId>); 2]>>::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            match &func.inst(inst).kind {
                InstKind::Phi(incoming) => {
                    for &(pred, operand) in incoming {
                        uses.entry(operand).or_default().push((pred, None));
                    }
                }
                kind => kind.visit_operands(|operand| {
                    uses.entry(operand).or_default().push((block, Some(inst)));
                }),
            }
        }
        if let Some(term) = &data.terminator {
            term.visit_operands(|operand| uses.entry(operand).or_default().push((block, None)));
        }
    }
    let cfg = CfgInfo::new(func);
    let mut moved = false;
    for (block, next) in guards {
        // The instructions that could follow the guard: each is movable, and read only after
        // the guard or by another of them. Dropping one that is not closed drops the
        // instructions that feed it, and the readers of one dropped for its operands.
        let mut producers = FxHashMap::<ValueId, InstId>::default();
        let mut members = FxHashSet::<InstId>::default();
        for &inst in &func.blocks[block].instructions {
            if let Some(result) = func.inst_result_value(inst)
                && movable(func, inst)
            {
                producers.insert(result, inst);
                members.insert(inst);
            }
        }
        let mut pending = members.iter().copied().collect::<Vec<_>>();
        while let Some(inst) = pending.pop() {
            if !members.contains(&inst) {
                continue;
            }
            let result = func.inst_result_value(inst).expect("movable instructions have results");
            let readers = uses.get(&result).map_or(&[][..], SmallVec::as_slice);
            let read_after = !readers.is_empty()
                && readers.iter().all(|&(reader, by)| {
                    by.is_some_and(|by| members.contains(&by))
                        || cfg.dominators().dominates(next, reader)
                });
            let mut operands = SmallVec::<[ValueId; 3]>::new();
            func.inst(inst).kind.visit_operands(|operand| operands.push(operand));
            let closed = operands.iter().all(|&operand| {
                // A calldata word read in an earlier block is read again after the guard only
                // when nothing in this block still reads it, so that no read is added.
                let read_again = func.is_constant_calldata_load(operand)
                    && uses.get(&operand).is_none_or(|readers| {
                        readers.iter().all(|&(reader, by)| {
                            reader != block || by.is_some_and(|by| members.contains(&by))
                        })
                    });
                func.value_u256(operand).is_some()
                    || is_member_result(&producers, &members, operand)
                    || read_again
            });
            if read_after && closed {
                continue;
            }
            members.remove(&inst);
            // The instructions that feed this one, read its result, or share its operands may no
            // longer qualify.
            let mut touched = operands.clone();
            touched.push(result);
            let neighbors = operands
                .iter()
                .filter_map(|operand| producers.get(operand).copied())
                .chain(
                    touched
                        .iter()
                        .flat_map(|value| uses.get(value).into_iter().flatten())
                        .filter_map(|&(_, by)| by),
                )
                .collect::<Vec<_>>();
            pending.extend(neighbors.into_iter().filter(|neighbor| members.contains(neighbor)));
        }
        let sunk = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|inst| members.contains(inst))
            .collect::<Vec<_>>();
        // A computation of constants alone is not worth a move, and a calldata read alone is
        // placed by `place-reads`.
        if !sunk.iter().any(|&inst| is_pure(func.inst(inst))) {
            continue;
        }
        for &inst in &sunk {
            let mut operands = SmallVec::<[ValueId; 3]>::new();
            func.inst(inst).kind.visit_operands(|operand| operands.push(operand));
            for operand in operands {
                for reader in uses.get_mut(&operand).into_iter().flatten() {
                    if *reader == (block, Some(inst)) {
                        reader.0 = next;
                    }
                }
            }
        }
        let (moving, staying) =
            func.blocks[block].instructions.iter().partition(|inst| members.contains(inst));
        func.blocks[block].instructions = staying;
        let start = func.blocks[next]
            .instructions
            .iter()
            .position(|&inst| !matches!(func.inst(inst).kind, InstKind::Phi(_)))
            .unwrap_or(func.blocks[next].instructions.len());
        func.blocks[next].instructions.splice(start..start, moving);
        moved = true;
    }
    moved
}

/// Whether `operand` is the result of an instruction that is still a member.
fn is_member_result(
    producers: &FxHashMap<ValueId, InstId>,
    members: &FxHashSet<InstId>,
    operand: ValueId,
) -> bool {
    producers.get(&operand).is_some_and(|producer| members.contains(producer))
}

/// Whether an instruction can run after a guard it ran before: a pure instruction, or a read of
/// calldata at a constant offset. A zero test stays, since `place-reads` places it by its reader.
fn movable(func: &Function, inst: InstId) -> bool {
    let instruction = func.inst(inst);
    let Some(result) = func.inst_result_value(inst) else { return false };
    let zero_test = matches!(instruction.kind, InstKind::Eq(a, b) | InstKind::Ne(a, b)
        if [a, b].iter().any(|&operand| func.value_u256(operand).is_some_and(|word| word.is_zero())));
    !matches!(instruction.kind, InstKind::Phi(_))
        && !zero_test
        && !instruction.kind.effects().observes_execution()
        && !instruction.kind.effects().control.any()
        && (is_pure(instruction) || func.is_constant_calldata_load(result))
}

/// Each block that ends in a guard, with the block that continues after it: a branch with one
/// successor that only reverts, and another that this block alone enters.
fn guards(func: &Function) -> Vec<(BlockId, BlockId)> {
    let reverts =
        |block: BlockId| func.blocks[block].terminator.as_ref().is_some_and(Terminator::reverts);
    func.blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            let Some(&Terminator::Branch { then_block, else_block, .. }) = data.terminator.as_ref()
            else {
                return None;
            };
            let next = match (reverts(then_block), reverts(else_block)) {
                (true, false) => else_block,
                (false, true) => then_block,
                _ => return None,
            };
            (next != block && func.blocks[next].predecessors.as_slice() == [block])
                .then_some((block, next))
        })
        .collect()
}
