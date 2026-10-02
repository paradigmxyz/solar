//! Merging of tests that branch to the same aborting block.
//!
//! Checked arithmetic and ABI decoding guard straight-line code with tests that
//! each branch to an aborting block, such as a `Panic(0x11)` revert, and pay a
//! conditional jump apiece. When a block ends with such a test, its
//! continuation is entered from nowhere else, except from blocks that abort
//! first, holds only speculatable instructions, and ends with a test into the
//! same aborting block, the first test can wait for the second:
//!
//! ```text
//! b1: ...; jumpi c1, abort, b2         b1: ...; jump b2
//! b2: x = ...; jumpi c2, abort, b3  => b2: x = ...; c = or c1, c2; jumpi c, abort, b3
//! ```
//!
//! Two tests that continue on their true edges combine with `and` instead.
//! Merges repeat along a chain, so a run of checks, such as the overflow checks
//! of an unrolled loop's copies, branches once.
//!
//! Safety: when the first test fails, the original code aborts at once, and the
//! merged code first runs the continuation's instructions, then aborts on the
//! combined test. Those instructions are pure word operations or environment
//! reads such as calldata loads: they write nothing and cost bounded gas, so
//! running them first changes nothing but the gas an aborting call spends before
//! it reverts. The aborting block must have no phis, so it cannot tell which
//! edge reached it; the values it reads dominate both edges.
//!
//! Aborting blocks merge when they do the same, as the panic calls that an
//! unrolled loop's copies clone do; the merged test enters the second one.
//!
//! Profitability: every merge saves a conditional jump and its pushed label for
//! an `and` or `or`, priced by the target. Tests that abort on different edges
//! stay apart, as would a zero test: a branch reads either for free, while the
//! combined word would pay an `iszero`, and the stack scheduler answered the
//! extra live condition with spills in measured loops. Runs late, after the
//! final loop passes, check elimination and the CFG cleanup that joins each
//! copy's blocks, so the tests it merges are the ones that remain.

use super::loop_split::rebuild_predecessors;
use crate::{
    backend::evm::op,
    mir::{
        BlockId, Callee, EffectKind, Function, FunctionId, InstKind, Instruction, MirType, Module,
        Terminator, Value, ValueId,
        analysis::{aborts, cold_functions},
        pass::{MirPass, run_function_pass},
    },
    target::Target,
};
use solar_data_structures::bit_set::DenseBitSet;

/// Function pass that merges tests into a shared aborting block.
pub(crate) struct MergeAborts;

impl MirPass for MergeAborts {
    fn name(&self) -> &'static str {
        "merge-aborts"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        let cold = cold_functions(module);
        run_function_pass(module, analyses, |func, _| merge_function(func, &cold, target))
    }
}

/// A test on a block's way out: its condition, the aborting block, the continuation, and
/// whether the condition holding means aborting.
struct Test {
    condition: ValueId,
    abort: BlockId,
    next: BlockId,
    aborts_when_true: bool,
}

fn merge_function(func: &mut Function, cold: &DenseBitSet<FunctionId>, target: Target) -> bool {
    rebuild_predecessors(func);
    let mut changed = false;
    // Each sweep follows every chain of tests forward from its first block; a later sweep picks
    // up chains whose head a merge further down only now completed.
    loop {
        let mut merged = false;
        for index in 0..func.blocks.len() {
            let mut block = BlockId::from_usize(index);
            while let Some((first, second)) = mergeable(func, block, cold, target) {
                let next = first.next;
                merge(func, block, first, second);
                merged = true;
                block = next;
            }
        }
        if !merged {
            return changed;
        }
        changed = true;
    }
}

/// The test a block ends with, when exactly one of its edges enters a shared aborting block.
fn test(func: &Function, block: BlockId, cold: &DenseBitSet<FunctionId>) -> Option<Test> {
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[block].terminator
    else {
        return None;
    };
    // A branch tests a word against zero for free.
    if let Value::Inst(inst) = func.value(condition)
        && let InstKind::Eq(a, b) | InstKind::Ne(a, b) = func.inst(*inst).kind
        && [a, b].into_iter().any(|operand| func.value_u256(operand).is_some_and(|v| v.is_zero()))
    {
        return None;
    }
    let shared = |block: BlockId| {
        aborts(func, block, cold)
            && !func.blocks[block]
                .instructions
                .iter()
                .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
    };
    match (shared(then_block), shared(else_block)) {
        (true, false) => {
            Some(Test { condition, abort: then_block, next: else_block, aborts_when_true: true })
        }
        (false, true) => {
            Some(Test { condition, abort: else_block, next: then_block, aborts_when_true: false })
        }
        _ => None,
    }
}

/// Whether two aborting blocks do the same, such as the panic calls of an unrolled loop's
/// copies: the same instructions, and the same terminator unless a call that never returns
/// makes it unreachable.
fn same_abort(func: &Function, a: BlockId, b: BlockId, cold: &DenseBitSet<FunctionId>) -> bool {
    if a == b {
        return true;
    }
    let (a, b) = (&func.blocks[a], &func.blocks[b]);
    a.instructions.len() == b.instructions.len()
        && a.instructions
            .iter()
            .zip(&b.instructions)
            .all(|(&x, &y)| func.inst(x).kind == func.inst(y).kind)
        && (a.terminator == b.terminator
            || a.instructions.iter().any(|&inst| {
                matches!(
                    func.inst(inst).kind,
                    InstKind::ICall { function: Callee::Function(function), .. }
                        if cold.contains(function)
                )
            }))
}

/// Whether running an instruction before an abort is decided changes nothing but gas.
fn speculatable(kind: &InstKind) -> bool {
    !kind.has_side_effects()
        && match kind.op_def().effect {
            EffectKind::Pure => !matches!(kind, InstKind::Phi(_)),
            EffectKind::EnvironmentRead => true,
            _ => false,
        }
}

/// The two tests a block and its continuation end with, when the first can wait.
fn mergeable(
    func: &Function,
    block: BlockId,
    cold: &DenseBitSet<FunctionId>,
    target: Target,
) -> Option<(Test, Test)> {
    let first = test(func, block, cold)?;
    // Other edges into the continuation leave blocks that abort before taking them.
    let continuation = &func.blocks[first.next];
    if first.next == block
        || !continuation.predecessors.iter().all(|&from| from == block || aborts(func, from, cold))
    {
        return None;
    }
    let second = test(func, first.next, cold)?;
    if !same_abort(func, first.abort, second.abort, cold)
        || second.aborts_when_true != first.aborts_when_true
        || !continuation.instructions.iter().all(|&inst| speculatable(&func.inst(inst).kind))
    {
        return None;
    }
    let added = target.opcode(op::OR);
    let saved = target.opcode(op::PUSH2) + target.opcode(op::JUMPI);
    target
        .improves(
            i128::from(saved.gas) - i128::from(added.gas),
            i128::from(saved.bytes) - i128::from(added.bytes),
        )
        .then_some((first, second))
}

/// Appends a compiler-generated instruction to a block and returns its result.
fn append(func: &mut Function, block: BlockId, kind: InstKind) -> ValueId {
    let (inst, value) =
        func.alloc_value_inst(Instruction::new(kind, Some(MirType::I1)).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}

fn merge(func: &mut Function, predecessor: BlockId, first: Test, second: Test) {
    let block = first.next;
    let combined = if second.aborts_when_true {
        // b2: c = or c1, c2
        //     jumpi c, abort, b3
        append(func, block, InstKind::Or(first.condition, second.condition))
    } else {
        // b2: c = and c1, c2
        //     jumpi c, b3, abort
        append(func, block, InstKind::And(first.condition, second.condition))
    };
    let (terminator, metadata) = func.blocks[block].take_terminator();
    let Some(Terminator::Branch { then_block, else_block, .. }) = terminator else {
        unreachable!("the continuation ends with its test")
    };
    func.blocks[block].set_terminator(
        Terminator::Branch { condition: combined, then_block, else_block },
        metadata,
    );

    // b1: ...; jump b2
    let (_, metadata) = func.blocks[predecessor].take_terminator();
    func.blocks[predecessor].set_terminator(Terminator::Jump(block), metadata);
    func.blocks[first.abort].predecessors.retain(|from| *from != predecessor);
}
