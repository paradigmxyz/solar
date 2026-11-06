//! Sinks computations on calldata words past a guard into the block that continues after it.
//!
//! A guard is a conditional branch with one successor that only reverts: a range check, an
//! overflow test, a panic. Code that runs before the guard in source order, but whose result is
//! only read after it, computes that result on the way to the guard, so the value stays live
//! across the branch and the stack scheduler spends a `DUP` or `SWAP` on it. This pass moves such
//! a computation to the start of the continuing block, where its readers are, so nothing carries
//! it across the guard and the revert path no longer computes it.
//!
//! The computation is a set of pure instructions and calldata reads at constant offsets: a
//! calldata word cannot change during the call, so reading it after the guard gives the same
//! word. The set is closed: its operands are constants, its own results, and calldata words that
//! nothing else in the block reads, which `place-reads` reads again where they are used, so
//! moving it carries no other value across the guard in their place and adds no read. A computation
//! on a value that lives on across the guard, such as a memory pointer plus a constant, does not
//! move: reaching the operand after the guard costs what carrying the result costs, and the
//! schedule that results is not smaller. Every reader of a result outside the set sits in a block
//! the continuing block dominates. The reverting block reads none of the results then, and the
//! continuing block has the guard's block as its only predecessor, so the computation runs once per
//! entry, as it did before.
//!
//! The pass runs before `place-reads`, after the last pass that creates guards.

use super::word_sequence::is_pure;
use crate::mir::{
    BlockId, Function, InstId, InstKind, Module, Terminator, ValueId,
    analysis::CfgInfo,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
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
        let mut sunk = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|&inst| movable(func, inst))
            .collect::<Vec<_>>();
        loop {
            let members = sunk.clone();
            sunk.retain(|&inst| {
                let result =
                    func.inst_result_value(inst).expect("movable instructions have results");
                let readers = uses.get(&result).map_or(&[][..], SmallVec::as_slice);
                let read_after = !readers.is_empty()
                    && readers.iter().all(|&(reader, by)| {
                        by.is_some_and(|by| members.contains(&by))
                            || cfg.dominators().dominates(next, reader)
                    });
                let mut closed = true;
                func.inst(inst).kind.visit_operands(|operand| {
                    // A calldata word read in an earlier block is read again after the guard
                    // only when nothing in this block still reads it, so that no read is added.
                    let read_again = func.is_constant_calldata_load(operand)
                        && uses.get(&operand).is_none_or(|readers| {
                            readers.iter().all(|&(reader, by)| {
                                reader != block || by.is_some_and(|by| members.contains(&by))
                            })
                        });
                    closed &= func.value_u256(operand).is_some()
                        || read_again
                        || members
                            .iter()
                            .any(|&other| func.inst_result_value(other) == Some(operand));
                });
                read_after && closed
            });
            if sunk.len() == members.len() {
                break;
            }
        }
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
            func.blocks[block].instructions.iter().partition(|inst| sunk.contains(inst));
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

/// Whether an instruction can run after a guard it ran before: a pure instruction, or a read of
/// calldata at a constant offset.
fn movable(func: &Function, inst: InstId) -> bool {
    let instruction = func.inst(inst);
    let Some(result) = func.inst_result_value(inst) else { return false };
    !matches!(instruction.kind, InstKind::Phi(_))
        && !instruction.kind.effects().observes_execution()
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
