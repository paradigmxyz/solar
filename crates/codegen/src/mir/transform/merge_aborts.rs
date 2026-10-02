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
//! Two tests that an addition wrapped, `s1 < s0` for `s1 = s0 + a1` and then
//! `s2 < s1` for `s2 = s1 + a2`, combine into `s2 < s0` instead when `a1 + a2`
//! cannot reach `2^256`, as for loop counters the target's trip-count limit
//! bounds: the sum then wraps at most once, and exactly when it falls below
//! `s0`. The overflow checks of an unrolled accumulation of counters become a
//! single comparison.
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
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

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

/// A test that adding words to `base` wrapped, `lt sum, base`, when the words add up to at most
/// `bound`.
#[derive(Clone, Copy)]
struct Wrap {
    sum: ValueId,
    base: ValueId,
    bound: U256,
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
    let mut wraps = FxHashMap::default();
    // Each sweep follows every chain of tests forward from its first block; a later sweep picks
    // up chains whose head a merge further down only now completed.
    loop {
        let mut merged = false;
        for index in 0..func.blocks.len() {
            let mut block = BlockId::from_usize(index);
            while let Some((first, second)) = mergeable(func, block, cold, target) {
                let next = first.next;
                merge(func, block, first, second, &mut wraps);
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

fn merge(
    func: &mut Function,
    predecessor: BlockId,
    first: Test,
    second: Test,
    wraps: &mut FxHashMap<ValueId, Wrap>,
) {
    let block = first.next;
    let combined = if second.aborts_when_true
        && let Some(wrap) = fused_wrap(func, first.condition, second.condition, wraps)
    {
        // s1 = add s0, a1; c1 = lt s1, s0
        // s2 = add s1, a2; c2 = lt s2, s1
        // b2: c = lt s2, s0
        //     jumpi c, abort, b3
        let combined = append(func, block, InstKind::Lt(wrap.sum, wrap.base));
        wraps.insert(combined, wrap);
        combined
    } else if second.aborts_when_true {
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

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst) => Some(&func.inst(*inst).kind),
        _ => None,
    }
}

/// The word `sum` adds to `base`, when it is `add base, addend`.
fn addend(func: &Function, sum: ValueId, base: ValueId) -> Option<ValueId> {
    match *inst_kind(func, sum)? {
        InstKind::Add(a, b) if a == base => Some(b),
        InstKind::Add(a, b) if b == base => Some(a),
        _ => None,
    }
}

/// The wrap test that stands for both a wrap test of `s1 = s0 + a1` and the following one of
/// `s2 = s1 + a2`, when `a1 + a2` cannot reach `2^256`: then the sum wraps at most once, exactly
/// when `s2 < s0`, as either test reports.
fn fused_wrap(
    func: &Function,
    first: ValueId,
    second: ValueId,
    wraps: &FxHashMap<ValueId, Wrap>,
) -> Option<Wrap> {
    let wrap = match wraps.get(&first) {
        Some(&wrap) => wrap,
        None => {
            let &InstKind::Lt(sum, base) = inst_kind(func, first)? else { return None };
            Wrap { sum, base, bound: upper_bound(func, addend(func, sum, base)?, 8)? }
        }
    };
    let &InstKind::Lt(next, sum) = inst_kind(func, second)? else { return None };
    if sum != wrap.sum {
        return None;
    }
    let bound = wrap.bound.checked_add(upper_bound(func, addend(func, next, sum)?, 8)?)?;
    Some(Wrap { sum: next, base: wrap.base, bound })
}

/// The largest value a word can take: literals, sums of bounded words, comparisons, masks,
/// right shifts, and loop counters that start at a literal and add a literal step, which the
/// target's trip-count limit bounds.
fn upper_bound(func: &Function, value: ValueId, depth: usize) -> Option<U256> {
    if let Some(constant) = func.value_u256(value) {
        return Some(constant);
    }
    let depth = depth.checked_sub(1)?;
    match *inst_kind(func, value)? {
        InstKind::Add(a, b) => {
            upper_bound(func, a, depth)?.checked_add(upper_bound(func, b, depth)?)
        }
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::Ne(..) => Some(U256::from(1)),
        InstKind::Zext(source, ..) => upper_bound(func, source, depth),
        InstKind::And(a, b) => match (upper_bound(func, a, depth), upper_bound(func, b, depth)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (bound, None) | (None, bound) => bound,
        },
        InstKind::Shr(shift, source) => {
            let shift = usize::try_from(func.value_u256(shift)?).ok()?;
            Some(upper_bound(func, source, depth)?.checked_shr(shift).unwrap_or(U256::ZERO))
        }
        InstKind::Phi(ref incoming) => {
            // counter = phi [entry: start], [latch: counter + c1 + ... + ck]
            let &[(_, first), (_, second)] = incoming.as_slice() else { return None };
            let (start, next) = match (func.value_u256(first), func.value_u256(second)) {
                (Some(start), None) => (start, second),
                (None, Some(start)) => (start, first),
                _ => return None,
            };
            let step = counter_step(func, value, next)?;
            start.checked_add(step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?)
        }
        _ => None,
    }
}

/// The literal a loop's update adds to its counter phi per iteration, through a chain of
/// additions of literals.
fn counter_step(func: &Function, phi: ValueId, mut next: ValueId) -> Option<U256> {
    const MAX_ADDS: usize = 16;
    let mut step = U256::ZERO;
    for _ in 0..MAX_ADDS {
        if next == phi {
            return (!step.is_zero()).then_some(step);
        }
        let &InstKind::Add(a, b) = inst_kind(func, next)? else { return None };
        let (rest, literal) = match (func.value_u256(a), func.value_u256(b)) {
            (None, Some(literal)) => (a, literal),
            (Some(literal), None) => (b, literal),
            _ => return None,
        };
        step = step.checked_add(literal)?;
        next = rest;
    }
    None
}
