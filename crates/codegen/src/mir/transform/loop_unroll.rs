//! Unrolling of counted loops by two or four, and peeling of loops whose body
//! tests words computed before the loop.
//!
//! A loop `for (i = a; i < n; i += s)` pays its header test, a compare and a
//! conditional jump, once per iteration. This pass gives such a loop a main
//! loop that runs the body `k` times per test while `i + (k - 1) * s < n`, that
//! is while `k` more iterations remain, and keeps the original loop after it
//! for the last iterations. With `k = 2`:
//!
//! ```text
//! main_header:
//!   state = phi [preheader: start], [latch_b: next_b]
//!   ahead = add i, s
//!   jumpi (lt ahead, n), body_a, header
//! body_a: first copy; jump header_b
//! header_b: the header's other instructions for `i + s`; jump body_b
//! body_b: second copy; jump main_header
//! header: state = phi [main_header: state], [latch: next]; the original loop
//! ```
//!
//! A loop that runs while `i <= n` leaves its main loop once `i + s > n`, or,
//! stepping by one, once `i < n` fails, which needs no addition.
//!
//! A loop that runs until its counter reaches the bound, `for (; i != n; i += s)`
//! as assembly loops over pointers are written, knows its remaining count
//! exactly: `n - i` is `s` times the count, so with `s = 2^t * u` for an odd
//! `u`, bit `t` of `n - i` is the count's parity. A counter that counts down,
//! `for (; i != 0; i -= m)` or `i > 0`, steps by `s = 2^256 - m`. One peeled
//! iteration evens the count out, and the original header's test then admits
//! two iterations:
//!
//! ```text
//! check:
//!   state = phi [preheader: start]
//!   jumpi (ne (and (shr t, (sub n, i)), 1), 0), body_a, header
//! body_a: peeled copy; jump header
//! header: state = phi [check: state], [latch_a: next_a], [latch_b: next_b]
//!   jumpi (ne i, n), body, exit
//! body: the original body; jump header_b
//! header_b: the header's other instructions for `i + s`; jump body_b
//! body_b: second copy; jump header
//! ```
//!
//! A body that aborts unless a word computed before the loop holds, such as a
//! division's zero check on a divisor the loop never changes, first runs one
//! iteration in a copy that keeps those tests, and the loop after it jumps past
//! them. When the copy's header declines the first iteration, it enters the
//! original header, which declines it as well, so the original header's exit
//! stays the loop's only one. Without those reads of words from before the
//! loop, the loop may then unroll.
//!
//! A counter that only counts iterations, `for (i = 0; i < n; ++i)` with `i`
//! read by nothing but its test and its increment, counts from `-n` up to zero
//! instead: `r = i - n` modulo `2^256` starts at `0 - n`, steps by one as `i`
//! did, and `i < n` holds exactly when `r != 0`, which a branch reads for free,
//! so the loop no longer needs its bound. A loop that unrolls by four keeps its
//! counter, as its main test already runs once per four iterations.
//!
//! Recognition: as for `loop-split`, a natural loop with a preheader and no
//! inner loop, whose header branches into the body while `i < n`, `i <= n`,
//! `i != n` or `i > 0` and otherwise leaves the loop, where `i` is a header
//! phi, or a pointer phi the header converts to an integer, that the loop's
//! single back edge advances by adding a literal step, or for `!=` and `> 0`
//! also by subtracting one, and `n` is defined outside the loop. The latch may
//! also branch to a block that aborts, such as the increment's overflow check.
//! A `<` or `<=` test also needs a literal start, except for `<=` stepping by
//! one. The header's other instructions must be free of effects, because the
//! original header repeats them for the iteration the main loop declined. The
//! body must be straight-line: every branch inside it continues the loop on one
//! arm and aborts on the other, such as an arithmetic panic, where a block
//! aborts when it reverts or calls a function that never returns. The body may
//! read words computed before the loop only when the backend rebuilds them
//! where they are read, as calldata words at fixed offsets and environment
//! reads: the stack scheduler spills other such words, and their copies
//! measured slower than the original loop.
//!
//! Safety: for `<` and `<=`, `i` starts at a literal and grows by a literal
//! step, so within the target's trip-count bound `i + (k - 1) * s` cannot wrap,
//! and `i + (k - 1) * s < n` implies `i + j * s < n` for every earlier copy
//! `j`, as it does with `<=`. Stepping by one, `i < n` gives `i + 1 <= n`
//! without wrapping from any start. The main loop therefore runs the iterations
//! the original loop runs next, in the same order with the same values and
//! effects, `k` at a time. Each later copy's header test holds by the main
//! header's test, so it enters its body directly. When the main test fails, the
//! original loop receives the exact loop-carried state and finishes. Every loop
//! block is cloned with its instructions and effects unchanged. Edges that
//! leave the loop from its body, such as panics, reach the same blocks from
//! every copy; their phis gain the cloned edges, and a loop qualifies only when
//! no block reachable from such an edge uses a loop-defined value except as a
//! phi input on that edge, so every definition still dominates its uses. The
//! header's exit stays the only way to leave the original loop normally, so
//! values read after the loop keep their definitions. A peeled loop's body runs
//! only after the copy's tests held, and they read the same words; the copy's
//! declined first iteration reaches a header free of effects with the same
//! state, which declines it too.
//!
//! For `!=`, counts are taken modulo `2^256`, as the counter wraps, and a step
//! that subtracts `m` adds `2^256 - m`; `i > 0` is `i != 0`. When `n - i`
//! is `s` times some count `k`, the loop runs exactly `k` more iterations, and
//! bit `t` of `n - i` is `k`'s parity. A set bit means `i != n`, so the peeled
//! iteration is one the original loop runs; an even count remains after it, so
//! whenever the header admits an iteration a second one follows, and the second
//! copy's test holds. When no such `k` exists, `i` never reaches `n`: the
//! original loop never ends, and neither does the unrolled one, every test of
//! which the original loop also passes.
//!
//! Each copy advances the counter as the original latch does, and the main
//! header's addition feeds only its test. Reusing `ahead` as the first copy's
//! counter saves that addition, but leaves `i` live only on the exit edge and
//! `ahead` only on the body edge, which the stack scheduler pays for with swaps
//! and pops in the copies; recomputing keeps every copy's stack shape the
//! original body's.
//!
//! Profitability: gas mode only, and only when the optimizer runs reach the
//! target's threshold for copying loops: unrolling and peeling trade code size
//! for runtime gas, and at fewer runs, as at the default 200, a build still
//! values its size. Counting a counter up to zero copies nothing and runs at any
//! runs. Copies are priced by the target over the deployment's expected
//! executions. Each group of `k` iterations skips `k - 1` header tests
//! and back-edge jumps. For `<` and `<=`, the first group pays for the original
//! loop's last test and every test of the main loop, including the declined
//! one, adds the offset first, except for `<=` stepping by one; for `!=`, which
//! unrolls by two, the parity check costs about one test per entry. The new
//! code holds `k` more copies of the body and one of the header, and the pass
//! takes the factor that saves the most. Literal bounds give the trip count,
//! and other loops are assumed to run the target's estimate for uncounted
//! loops. A peeled copy of the header and the body pays when the conditional
//! jumps that the iterations after the first skip save more. A `<=` loop
//! stepping by one unrolls by four only from a literal start, as its main test
//! then adds the offset. Runs after the late loop passes have fixed the loop's
//! physical shape, and before the final CFG cleanup and dead-code elimination
//! remove the cloned tests the copies no longer read.

use super::loop_split::{rebuild_predecessors, retarget};
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionId, Immediate, InstId, InstKind, Instruction,
        MirType, Module, OpTraits, Terminator, Value, ValueId,
        analysis::{Loop, LoopAnalyzer, LoopInfo, aborts, cold_functions},
        pass::{MirPass, run_function_pass},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Function pass that unrolls counted loops and peels loops with invariant tests.
pub(crate) struct LoopUnroll;

impl MirPass for LoopUnroll {
    fn name(&self) -> &'static str {
        "loop-unroll"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        let cold = cold_functions(module);
        run_function_pass(module, analyses, |func, _| unroll_function(func, &cold, target))
    }
}

/// How the header decides that another iteration runs.
#[derive(Clone, Copy)]
enum Test {
    /// `i < n`, with a literal start so `i + s` cannot wrap.
    Below,
    /// `i <= n`, with a literal start so `i + s` cannot wrap.
    AtMost,
    /// `i <= n` stepping by one: two more iterations run while `i < n`, from any start.
    UpTo,
    /// `i != n`.
    Reaches,
}

/// How the main loop admits `factor` more iterations at once.
#[derive(Clone, Copy)]
enum MainTest {
    /// `word + offset < bound`.
    AheadBelow(U256),
    /// Leaves once `word + offset > bound`.
    AheadBeyond(U256),
    /// `word < bound`, for `<=` stepping by one.
    Below,
    /// A peeled iteration evens out the count of a `!=` loop, whose own test then admits two.
    Parity,
}

/// A loop to unroll and the facts the unrolling needs.
struct Unroll {
    header: BlockId,
    preheader: BlockId,
    latch: BlockId,
    /// The header's successor inside the loop.
    body: BlockId,
    blocks: DenseBitSet<BlockId>,
    /// How many copies of the body the main loop runs per test.
    factor: u32,
    main_test: MainTest,
    /// The counter as the header tests it: the counting phi, or its integer value.
    word: ValueId,
    /// The loop-invariant bound the header compares the counter against.
    bound: ValueId,
    step: U256,
}

/// The shape both unrolling and peeling need: a natural loop with a preheader and a single
/// latch, no inner loop, a header free of effects that branches into a straight-line body or
/// out of the loop.
struct Shape {
    preheader: BlockId,
    latch: BlockId,
    /// The header's successor inside the loop.
    body: BlockId,
    /// The header's successor outside the loop.
    exit: BlockId,
    /// Whether the header enters the body when its condition holds.
    enters_on_true: bool,
    condition: ValueId,
    loop_insts: DenseBitSet<InstId>,
}

/// A loop `for (i = 0; i < n; ++i)` whose counter feeds only its own test and increment.
struct Reverse {
    preheader: BlockId,
    /// The counter phi.
    counter: ValueId,
    /// `i < n`, which the header branches on.
    condition: ValueId,
    bound: ValueId,
}

/// A loop whose body aborts unless a word computed before the loop holds, such as a division's
/// zero check on a divisor the loop never changes.
struct Peel {
    header: BlockId,
    preheader: BlockId,
    latch: BlockId,
    exit: BlockId,
    blocks: DenseBitSet<BlockId>,
    /// Body blocks ending with such a test, with the block each continues to.
    tests: Vec<(BlockId, BlockId)>,
}

fn unroll_function(func: &mut Function, cold: &DenseBitSet<FunctionId>, target: Target) -> bool {
    let mut changed = false;
    let mut done = FxHashSet::default();
    let mut peeled = FxHashSet::default();
    // A counter that only counts iterations counts from `-n` up to zero instead, so the header
    // tests it against zero, which a branch reads for free, and needs no bound in the loop. A
    // loop that unrolls by four keeps its counter: its main test already runs once per four
    // iterations.
    let loops = LoopAnalyzer::new().analyze_structure(func);
    let reversals: Vec<_> = loops
        .all_loops()
        .filter(|l| plan(func, &loops, l, cold, target).is_none_or(|unroll| unroll.factor != 4))
        .filter_map(|l| plan_reverse(func, l))
        .collect();
    changed |= !reversals.is_empty();
    for reversal in &reversals {
        reverse(func, reversal);
    }
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        // A peeled loop no longer reads its invariant tests and may unroll next.
        if let Some(plan) = loops
            .all_loops()
            .filter(|l| !peeled.contains(&l.header))
            .find_map(|l| plan_peel(func, &loops, l, cold, target))
        {
            peeled.insert(plan.header);
            peel(func, &plan);
            changed = true;
            continue;
        }
        let Some(unroll) = loops
            .all_loops()
            .filter(|l| !done.contains(&l.header))
            .find_map(|l| plan(func, &loops, l, cold, target))
        else {
            break;
        };
        done.insert(unroll.header);
        done.insert(apply(func, &unroll));
        changed = true;
    }
    changed
}

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst) => Some(&func.inst(*inst).kind),
        _ => None,
    }
}

fn jumps_to(func: &Function, block: BlockId, target: BlockId) -> bool {
    matches!(func.blocks[block].terminator, Some(Terminator::Jump(to)) if to == target)
}

/// Whether a block continues only to `target`, apart from edges into aborting blocks such as
/// an overflow check's panic.
fn continues_to(
    func: &Function,
    block: BlockId,
    target: BlockId,
    cold: &DenseBitSet<FunctionId>,
) -> bool {
    let Some(terminator) = &func.blocks[block].terminator else { return false };
    let successors = terminator.successors();
    successors.contains(&target)
        && successors.iter().all(|&successor| successor == target || aborts(func, successor, cold))
}

/// Whether the backend rebuilds a word where it is read instead of keeping it live: a calldata
/// word at a fixed offset or a stable environment read.
fn rebuilt_at_use(func: &Function, value: ValueId) -> bool {
    let Some(kind) = inst_kind(func, value) else { return true };
    match kind {
        InstKind::Zext(inner) => rebuilt_at_use(func, *inner),
        InstKind::CalldataLoad(offset) => func.value_u256(*offset).is_some(),
        _ => {
            let def = kind.op_def();
            def.traits.contains(OpTraits::REMATERIALIZABLE) && def.effect != EffectKind::Pure
        }
    }
}

fn shape(
    func: &Function,
    loops: &LoopInfo,
    l: &Loop,
    cold: &DenseBitSet<FunctionId>,
) -> Option<Shape> {
    let preheader = l.preheader?;
    let &[latch] = l.back_edges.as_slice() else { return None };
    if latch == l.header
        || !jumps_to(func, preheader, l.header)
        || !continues_to(func, latch, l.header, cold)
    {
        return None;
    }
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[l.header].terminator
    else {
        return None;
    };
    let (body, exit, enters_on_true) =
        match (l.blocks.contains(then_block), l.blocks.contains(else_block)) {
            (true, false) => (then_block, else_block, true),
            (false, true) => (else_block, then_block, false),
            _ => return None,
        };
    if loops.all_loops().any(|other| other.header != l.header && l.blocks.contains(other.header)) {
        return None;
    }
    let mut loop_insts = DenseBitSet::new_empty(func.num_insts());
    for block in l.blocks.iter() {
        for &inst in &func.blocks[block].instructions {
            loop_insts.insert(inst);
        }
    }
    // Only a straight-line body gains: every branch inside it continues the loop on one arm
    // and aborts on the other, such as an arithmetic panic.
    for block in l.blocks.iter() {
        if block == l.header || aborts(func, block, cold) {
            continue;
        }
        let successors = func.blocks[block].terminator.as_ref()?.successors();
        let continuing: Vec<_> =
            successors.iter().filter(|&&successor| !aborts(func, successor, cold)).collect();
        if continuing.len() != 1 || !l.blocks.contains(*continuing[0]) {
            return None;
        }
    }
    // The original header repeats its instructions for the iteration the main loop declined,
    // or for the one a peeled iteration's header declined, so they must give the same results
    // again: a read of remaining gas or memory size could see a new value and branch the other
    // way.
    if func.blocks[l.header].instructions.iter().any(|&inst| {
        let kind = &func.inst(inst).kind;
        !matches!(kind, InstKind::Phi(_))
            && (kind.has_side_effects() || kind.effects().observes_execution())
    }) {
        return None;
    }
    Some(Shape { preheader, latch, body, exit, enters_on_true, condition, loop_insts })
}

/// Whether the blocks the loop's body can leave to, such as panics, read loop values only as
/// the phi inputs of those edges, so the edges of every copy can reach them.
fn exits_read_through_phis(
    func: &Function,
    l: &Loop,
    exit: BlockId,
    loop_insts: &DenseBitSet<InstId>,
) -> bool {
    let defined_in_loop = |value| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(*inst),
        _ => false,
    };
    let mut reachable = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = Vec::new();
    for block in l.blocks.iter() {
        let Some(terminator) = &func.blocks[block].terminator else { return false };
        for successor in terminator.successors() {
            if !l.blocks.contains(successor)
                && !(block == l.header && successor == exit)
                && reachable.insert(successor)
            {
                worklist.push(successor);
            }
        }
    }
    while let Some(block) = worklist.pop() {
        let body = &func.blocks[block];
        for &inst in &body.instructions {
            let kind = &func.inst(inst).kind;
            let reads_loop_value = match kind {
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .any(|&(from, value)| !l.blocks.contains(from) && defined_in_loop(value)),
                _ => kind.operands().into_iter().any(defined_in_loop),
            };
            if reads_loop_value {
                return false;
            }
        }
        let Some(terminator) = &body.terminator else { return false };
        if terminator.operands().into_iter().any(defined_in_loop) {
            return false;
        }
        for successor in terminator.successors() {
            if !l.blocks.contains(successor) && reachable.insert(successor) {
                worklist.push(successor);
            }
        }
    }
    true
}

fn plan(
    func: &Function,
    loops: &LoopInfo,
    l: &Loop,
    cold: &DenseBitSet<FunctionId>,
    target: Target,
) -> Option<Unroll> {
    if !target.copies_loops() {
        return None;
    }
    let Shape { preheader, latch, body, exit, enters_on_true, condition, loop_insts } =
        shape(func, loops, l, cold)?;
    // The stack planner spills a word the body reads from before the loop, and the copies
    // measured slower than the original loop.
    for block in l.blocks.iter() {
        if block == l.header {
            continue;
        }
        let reads_hoisted = |value: ValueId| {
            matches!(func.value(value), Value::Inst(inst) if !loop_insts.contains(*inst))
                && !rebuilt_at_use(func, value)
        };
        if func.blocks[block].instructions.iter().any(|&inst| {
            let kind = &func.inst(inst).kind;
            !matches!(kind, InstKind::Phi(_)) && kind.operands().into_iter().any(reads_hoisted)
        }) || func.blocks[block]
            .terminator
            .as_ref()
            .is_some_and(|terminator| terminator.operands().into_iter().any(reads_hoisted))
        {
            return None;
        }
    }
    let defined_in_loop = |value| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(*inst),
        _ => false,
    };

    // header: counter = phi [preheader: start], [latch: next]
    //         word = counter | ptrtoint counter
    //         jumpi (lt word, bound) | (ne word, bound) | (gt word, 0), body, exit
    //         jumpi (gt word, bound) | (eq word, bound), exit, body
    // latch:  next = add word, step | sub word, step | inttoptr (add|sub word, step)
    let reaches =
        |a, b| if defined_in_loop(a) { (Test::Reaches, a, b) } else { (Test::Reaches, b, a) };
    let is_zero = |value| func.value_u256(value).is_some_and(|value| value.is_zero());
    let (test, word, bound) = match (inst_kind(func, condition)?, enters_on_true) {
        // A word above zero has not reached it.
        (&InstKind::Gt(word, zero), true) | (&InstKind::Lt(zero, word), true) if is_zero(zero) => {
            (Test::Reaches, word, zero)
        }
        (&InstKind::Lt(word, bound), true) | (&InstKind::Gt(bound, word), true) => {
            (Test::Below, word, bound)
        }
        (&InstKind::Gt(word, bound), false) | (&InstKind::Lt(bound, word), false) => {
            (Test::AtMost, word, bound)
        }
        (&InstKind::Ne(a, b), true) | (&InstKind::Eq(a, b), false) => reaches(a, b),
        _ => return None,
    };
    if defined_in_loop(bound) {
        return None;
    }
    let counter = match inst_kind(func, word) {
        Some(&InstKind::PtrToInt(pointer, _)) => pointer,
        _ => word,
    };
    let Value::Inst(phi) = func.value(counter) else { return None };
    if !func.blocks[l.header].instructions.contains(phi) {
        return None;
    }
    let InstKind::Phi(incoming) = &func.inst(*phi).kind else { return None };
    let mut start = None;
    let mut next = None;
    for &(from, value) in incoming {
        if from == preheader {
            start = Some(value);
        } else if from == latch {
            next = Some(value);
        } else {
            return None;
        }
    }
    let (start, next) = (start?, next?);
    let advance = match inst_kind(func, next)? {
        &InstKind::IntToPtr(advance) if counter != word => advance,
        _ if counter == word => next,
        _ => return None,
    };
    let step = match *inst_kind(func, advance)? {
        InstKind::Add(a, b) => match (func.value_u256(a), func.value_u256(b)) {
            (_, Some(step)) if a == word => step,
            (Some(step), _) if b == word => step,
            _ => return None,
        },
        // Counting down by `m` steps by `-m`, modulo 2^256, which only a loop that runs until
        // its counter reaches the bound can do.
        InstKind::Sub(a, b) if a == word && matches!(test, Test::Reaches) => {
            func.value_u256(b)?.wrapping_neg()
        }
        _ => return None,
    };
    if step.is_zero() {
        return None;
    }
    let test = match test {
        Test::AtMost if step == U256::from(1) => Test::UpTo,
        test => test,
    };
    let literal_start = func.value_u256(start);
    let uncounted = U256::from(Target::UNCOUNTED_LOOP_ITERATIONS);
    let trips = match (test, literal_start, func.value_u256(bound)) {
        (Test::Below, Some(start), Some(limit)) => limit.saturating_sub(start).div_ceil(step),
        (Test::AtMost | Test::UpTo, Some(start), Some(limit)) => {
            limit.checked_sub(start).map_or(U256::ZERO, |left| left / step + U256::from(1))
        }
        // A step of `2^256 - m` counts down by `m`.
        (Test::Reaches, Some(start), Some(limit)) => {
            let (distance, magnitude) = if step.bit(255) {
                (start.wrapping_sub(limit), step.wrapping_neg())
            } else {
                (limit.wrapping_sub(start), step)
            };
            if (distance % magnitude).is_zero() { distance / magnitude } else { uncounted }
        }
        _ => uncounted,
    };
    let trips = u64::try_from(trips).unwrap_or(u64::MAX);
    // The counter travels at most `step << MAX_TRIP_COUNT_BITS` from a literal start, so
    // `word + (factor - 1) * step` never wraps.
    let ahead = |factor: u32| {
        let offset = step.checked_mul(U256::from(factor - 1))?;
        let travel = step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?;
        literal_start?.checked_add(travel)?.checked_add(offset)?;
        Some(offset)
    };
    let candidates = [2, 4].into_iter().filter_map(|factor| {
        let main_test = match test {
            Test::Below => MainTest::AheadBelow(ahead(factor)?),
            Test::AtMost => MainTest::AheadBeyond(ahead(factor)?),
            // `i + 1 <= n` follows from `i < n` without wrapping, whatever the start.
            Test::UpTo if factor == 2 => MainTest::Below,
            Test::UpTo => MainTest::AheadBeyond(ahead(factor)?),
            Test::Reaches if factor == 2 => MainTest::Parity,
            Test::Reaches => return None,
        };
        let saving = lifetime_saving(func, l, main_test, factor, trips, step, target);
        (saving > 0).then_some((saving, factor, main_test))
    });
    let (_, factor, main_test) = candidates.max_by_key(|&(saving, ..)| saving)?;

    if !exits_read_through_phis(func, l, exit, &loop_insts) {
        return None;
    }
    Some(Unroll {
        header: l.header,
        preheader,
        latch,
        body,
        blocks: l.blocks.clone(),
        factor,
        main_test,
        word,
        bound,
        step,
    })
}

/// How much lifetime gas the tests and jumps the main loop skips save over the deposit of its
/// code; positive when unrolling pays.
fn lifetime_saving(
    func: &Function,
    l: &Loop,
    main_test: MainTest,
    factor: u32,
    trips: u64,
    step: U256,
    target: Target,
) -> i128 {
    let compare = match main_test {
        MainTest::AheadBelow(_) => op::LT,
        MainTest::AheadBeyond(_) | MainTest::Below => op::GT,
        MainTest::Parity => op::SUB,
    };
    let tested = target.opcode(compare)
        + target.dup().times(2)
        + target.opcode(op::PUSH2)
        + target.opcode(op::JUMPI);
    let back_edge =
        target.opcode(op::PUSH2) + target.opcode(op::JUMP) + target.opcode(op::JUMPDEST);
    let skip = tested + back_edge;
    let groups = u32::try_from(trips / u64::from(factor)).unwrap_or(u32::MAX);
    let saving = match main_test {
        // Every group of iterations skips a test and a back edge per copy but one, the first
        // group's skips pay for the original loop's last test and back edge, and every test of
        // the main loop, including the declined one, first adds the offset.
        MainTest::AheadBelow(offset) | MainTest::AheadBeyond(offset) => {
            let advance = target.opcode(op::ADD) + target.push(offset);
            let skipped = skip.times(groups.saturating_mul(factor - 1).saturating_sub(1));
            skipped.gas.saturating_sub(advance.times(groups.saturating_add(1)).gas)
        }
        MainTest::Below => skip.times(groups.saturating_sub(1)).gas,
        // Every pair skips a test and a back edge, and entering the loop tests the parity of
        // the remaining count once.
        MainTest::Parity => {
            let mut parity = tested + target.opcode(op::AND) + target.push(U256::from(1));
            let zeros = step.trailing_zeros();
            if zeros != 0 {
                parity += target.opcode(op::SHR) + target.push(U256::from(zeros));
            }
            skip.times(groups).gas.saturating_sub(parity.gas)
        }
    };
    // The new code holds `factor` copies of the body and one of the header.
    let growth: Cost = l
        .blocks
        .iter()
        .map(|block| {
            let copies = if block == l.header { 1 } else { factor };
            target.block_code_estimate(func, block).times(copies)
        })
        .sum();
    target.lifetime_gas(Cost::new(saving, 0)) as i128
        - target.lifetime_gas(Cost::new(0, growth.bytes)) as i128
}

/// Plans to count a loop's iterations from `-n` up to zero when its counter starts at zero,
/// steps by one, and feeds nothing but its test `i < n` and its own increment.
fn plan_reverse(func: &Function, l: &Loop) -> Option<Reverse> {
    let preheader = l.preheader?;
    let &[latch] = l.back_edges.as_slice() else { return None };
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[l.header].terminator
    else {
        return None;
    };
    // The header enters the body while `i < n`.
    if !l.blocks.contains(then_block) || l.blocks.contains(else_block) {
        return None;
    }
    let (counter, bound) = match *inst_kind(func, condition)? {
        InstKind::Lt(counter, bound) | InstKind::Gt(bound, counter) => (counter, bound),
        _ => return None,
    };
    let Value::Inst(phi) = *func.value(counter) else { return None };
    if !func.blocks[l.header].instructions.contains(&phi) {
        return None;
    }
    if let Value::Inst(inst) = func.value(bound)
        && l.blocks.iter().any(|block| func.blocks[block].instructions.contains(inst))
    {
        return None;
    }
    let InstKind::Phi(incoming) = &func.inst(phi).kind else { return None };
    let [(first, first_value), (second, second_value)] = incoming.as_slice() else { return None };
    let (start, next) = match (*first, *second) {
        (from, to) if from == preheader && to == latch => (*first_value, *second_value),
        (from, to) if from == latch && to == preheader => (*second_value, *first_value),
        _ => return None,
    };
    if !func.value_u256(start).is_some_and(|start| start.is_zero()) {
        return None;
    }
    match *inst_kind(func, next)? {
        InstKind::Add(a, b) | InstKind::Add(b, a)
            if a == counter && func.value_u256(b) == Some(U256::from(1)) => {}
        _ => return None,
    }
    // The counter, its increment and its test have no other readers.
    let mut readers = [0usize; 3];
    let mut count = |value: ValueId| {
        for (slot, watched) in [counter, next, condition].into_iter().enumerate() {
            if value == watched {
                readers[slot] += 1;
            }
        }
    };
    for block in &func.blocks {
        for &inst in &block.instructions {
            func.inst(inst).kind.visit_operands(&mut count);
        }
        if let Some(terminator) = &block.terminator {
            terminator.operands().into_iter().for_each(&mut count);
        }
    }
    // counter: the test and the increment; next: the phi; condition: the header's branch.
    (readers == [2, 1, 1]).then_some(Reverse { preheader, counter, condition, bound })
}

/// Plans to run a loop's first iteration in a copy when its body aborts unless a word computed
/// before the loop holds: the iterations after it then skip those tests.
fn plan_peel(
    func: &Function,
    loops: &LoopInfo,
    l: &Loop,
    cold: &DenseBitSet<FunctionId>,
    target: Target,
) -> Option<Peel> {
    if !target.copies_loops() {
        return None;
    }
    let shape = shape(func, loops, l, cold)?;
    let tests: Vec<_> = l
        .blocks
        .iter()
        .filter(|&block| block != l.header)
        .filter_map(|block| {
            let Some(Terminator::Branch { condition, then_block, else_block }) =
                func.blocks[block].terminator
            else {
                return None;
            };
            let invariant = match func.value(condition) {
                Value::Inst(inst) => !shape.loop_insts.contains(*inst),
                Value::Arg(_) => true,
                _ => false,
            };
            if !invariant {
                return None;
            }
            match (aborts(func, then_block, cold), aborts(func, else_block, cold)) {
                (true, false) => Some((block, else_block)),
                (false, true) => Some((block, then_block)),
                _ => None,
            }
        })
        .collect();
    if tests.is_empty() || !exits_read_through_phis(func, l, shape.exit, &shape.loop_insts) {
        return None;
    }
    // Every iteration after the first skips each test's conditional jump, and the copy holds
    // one more header and body.
    let test = target.opcode(op::JUMPI) + target.opcode(op::PUSH2) + target.dup();
    let skipped = test.times(u32::try_from(tests.len()).unwrap_or(u32::MAX));
    let iterations = u32::try_from(Target::UNCOUNTED_LOOP_ITERATIONS - 1).unwrap_or(u32::MAX);
    let saving = skipped.times(iterations).gas;
    let growth: Cost = l.blocks.iter().map(|block| target.block_code_estimate(func, block)).sum();
    (target.lifetime_gas(Cost::new(saving, 0)) > target.lifetime_gas(Cost::new(0, growth.bytes)))
        .then(|| Peel {
            header: l.header,
            preheader: shape.preheader,
            latch: shape.latch,
            exit: shape.exit,
            blocks: l.blocks.clone(),
            tests,
        })
}

/// A copy of the loop: each original block's clone and each original value's clone.
struct Copy {
    blocks: FxHashMap<BlockId, BlockId>,
    values: FxHashMap<ValueId, ValueId>,
}

impl Copy {
    fn value(&self, value: ValueId) -> ValueId {
        self.values.get(&value).copied().unwrap_or(value)
    }
}

/// Clones every loop block with its instructions, keeping edges that leave the loop.
fn clone_loop(func: &mut Function, blocks: &[BlockId]) -> Copy {
    let mut copy = Copy { blocks: FxHashMap::default(), values: FxHashMap::default() };
    for &block in blocks {
        copy.blocks.insert(block, func.alloc_block());
    }
    for &block in blocks {
        let originals = func.blocks[block].instructions.clone();
        let mut instructions = Vec::with_capacity(originals.len());
        for inst in originals {
            let original = func.inst(inst);
            let mut instruction = Instruction::new(original.kind.clone(), original.result_ty);
            instruction.metadata.copy_debug_context(&original.metadata);
            let cloned = if let Some(result) = func.inst_result_value(inst) {
                let (cloned, cloned_result) = func.alloc_value_inst(instruction);
                copy.values.insert(result, cloned_result);
                cloned
            } else {
                func.alloc_inst(instruction)
            };
            instructions.push(cloned);
        }
        func.blocks[copy.blocks[&block]].instructions = instructions;
    }
    for &block in blocks {
        let clone = copy.blocks[&block];
        for inst in func.blocks[clone].instructions.clone() {
            let kind = &mut func.inst_mut(inst).kind;
            if let InstKind::Phi(incoming) = kind {
                for (from, _) in incoming.iter_mut() {
                    if let Some(&clone_from) = copy.blocks.get(from) {
                        *from = clone_from;
                    }
                }
            }
            kind.visit_operands_mut(|value| *value = copy.value(*value));
        }
        let (mut terminator, metadata) = {
            let body = &func.blocks[block];
            (
                body.terminator.clone().expect("loop blocks are terminated"),
                body.terminator_metadata.clone(),
            )
        };
        terminator.visit_operands_mut(|value| *value = copy.value(*value));
        retarget(&mut terminator, |successor| {
            copy.blocks.get(&successor).copied().unwrap_or(successor)
        });
        func.blocks[clone].set_terminator(terminator, metadata);
    }
    copy
}

/// Appends a compiler-generated instruction to a block and returns its result.
fn append(func: &mut Function, block: BlockId, kind: InstKind, ty: MirType) -> ValueId {
    let (inst, value) =
        func.alloc_value_inst(Instruction::new(kind, Some(ty)).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}

/// Ends a block with a branch, keeping its terminator's source context.
fn branch(func: &mut Function, block: BlockId, condition: ValueId, then: BlockId, other: BlockId) {
    let (_, metadata) = func.blocks[block].take_terminator();
    func.blocks[block].set_terminator(
        Terminator::Branch { condition, then_block: then, else_block: other },
        metadata,
    );
}

/// Sends a block's edges to `from` to `to` instead.
fn redirect(func: &mut Function, block: BlockId, from: BlockId, to: BlockId) {
    let (terminator, metadata) = func.blocks[block].take_terminator();
    let mut terminator = terminator.expect("loop blocks are terminated");
    retarget(&mut terminator, |successor| if successor == from { to } else { successor });
    func.blocks[block].set_terminator(terminator, metadata);
}

/// Edits the incoming list of the phi defining `phi`.
fn edit_phi(func: &mut Function, phi: ValueId, edit: impl FnOnce(&mut Vec<(BlockId, ValueId)>)) {
    let Value::Inst(inst) = *func.value(phi) else { return };
    if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
        edit(incoming);
    }
}

/// Unrolls the loop and returns the main loop's header.
fn apply(func: &mut Function, unroll: &Unroll) -> BlockId {
    let blocks: Vec<BlockId> = unroll.blocks.iter().collect();
    // Edges that leave the loop from its body, before any terminator changes.
    let side_exits: Vec<_> = blocks
        .iter()
        .filter(|&&block| block != unroll.header)
        .flat_map(|&block| {
            let terminator = func.blocks[block].terminator.as_ref().expect("terminated");
            terminator
                .successors()
                .into_iter()
                .filter(|&successor| !unroll.blocks.contains(successor))
                .map(move |successor| (block, successor))
        })
        .collect();
    let copies: Vec<Copy> = (0..unroll.factor).map(|_| clone_loop(func, &blocks)).collect();
    let header_phis: Vec<_> = func.blocks[unroll.header]
        .instructions
        .iter()
        .filter_map(|&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let latch_value = incoming
                .iter()
                .find_map(|&(from, value)| (from == unroll.latch).then_some(value))?;
            Some((func.inst_result_value(inst)?, latch_value))
        })
        .collect();
    let first = &copies[0];
    let first_latch = first.blocks[&unroll.latch];
    let last = &copies[copies.len() - 1];
    let last_latch = last.blocks[&unroll.latch];

    let mut replacements = FxHashMap::default();
    let (entry, main_header) = if let MainTest::Parity = unroll.main_test {
        // check: state' = phi [preheader: start]
        //        left = sub bound, word' | word' when bound = 0
        //        jumpi (ne (and (shr zeros(step), left), 1), 0), body', header
        let check = first.blocks[&unroll.header];
        let word = first.value(unroll.word);
        // Against a zero bound, `0 - i` and `i` agree at bit `t` whenever `i` is a multiple of
        // `2^t`, and otherwise neither loop ends.
        let mut left = if func.value_u256(unroll.bound).is_some_and(|bound| bound.is_zero()) {
            word
        } else {
            append(func, check, InstKind::Sub(unroll.bound, word), MirType::I256)
        };
        let zeros = unroll.step.trailing_zeros();
        if zeros != 0 {
            let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(zeros))));
            left = append(func, check, InstKind::Shr(shift, left), MirType::I256);
        }
        // Integer conversions are already lowered here, so mask the low bit.
        let one = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(1))));
        let bit = append(func, check, InstKind::And(left, one), MirType::I256);
        let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let odd = append(func, check, InstKind::Ne(bit, zero), MirType::I1);
        branch(func, check, odd, first.blocks[&unroll.body], unroll.header);

        // latch': ... header
        // latch: ... header''
        // latch'': ... header
        let last_header = last.blocks[&unroll.header];
        redirect(func, first_latch, check, unroll.header);
        redirect(func, unroll.latch, unroll.header, last_header);
        redirect(func, last_latch, last_header, unroll.header);

        // check: state' = phi [preheader: start]
        // header: state = phi [check: state'], [latch': next'], [latch'': next'']
        for &(phi, latch_value) in &header_phis {
            replacements.insert(last.value(phi), latch_value);
            edit_phi(func, first.value(phi), |incoming| {
                incoming.retain(|&(from, _)| from != first_latch);
            });
            edit_phi(func, phi, |incoming| {
                for (from, value) in incoming.iter_mut() {
                    if *from == unroll.preheader {
                        *from = check;
                        *value = first.value(phi);
                    } else if *from == unroll.latch {
                        *from = last_latch;
                        *value = last.value(latch_value);
                    }
                }
                incoming.push((first_latch, first.value(latch_value)));
            });
        }
        (check, unroll.header)
    } else {
        let main_header = first.blocks[&unroll.header];
        let word = first.value(unroll.word);
        let first_body = first.blocks[&unroll.body];
        match unroll.main_test {
            // main_header: ahead = add word_1, offset
            //              jumpi (lt ahead, bound), body_1, header
            MainTest::AheadBelow(offset) => {
                let offset = func.alloc_value(Value::Immediate(Immediate::I256(offset)));
                let ahead = append(func, main_header, InstKind::Add(word, offset), MirType::I256);
                let below =
                    append(func, main_header, InstKind::Lt(ahead, unroll.bound), MirType::I1);
                branch(func, main_header, below, first_body, unroll.header);
            }
            // main_header: ahead = add word_1, offset
            //              jumpi (gt ahead, bound), header, body_1
            MainTest::AheadBeyond(offset) => {
                let offset = func.alloc_value(Value::Immediate(Immediate::I256(offset)));
                let ahead = append(func, main_header, InstKind::Add(word, offset), MirType::I256);
                let beyond =
                    append(func, main_header, InstKind::Gt(ahead, unroll.bound), MirType::I1);
                branch(func, main_header, beyond, unroll.header, first_body);
            }
            // main_header: jumpi (lt word_1, bound), body_1, header
            _ => {
                let below =
                    append(func, main_header, InstKind::Lt(word, unroll.bound), MirType::I1);
                branch(func, main_header, below, first_body, unroll.header);
            }
        }

        // latch_j: ... header_(j+1)
        // latch_factor: ... main_header
        for (index, copy) in copies.iter().enumerate() {
            let next =
                copies.get(index + 1).map_or(main_header, |next| next.blocks[&unroll.header]);
            redirect(func, copy.blocks[&unroll.latch], copy.blocks[&unroll.header], next);
        }

        // main_header: state_1 = phi [preheader: start], [latch_factor: next_factor]
        // header: state = phi [main_header: state_1], [latch: next]
        for &(phi, latch_value) in &header_phis {
            for pair in copies.windows(2) {
                replacements.insert(pair[1].value(phi), pair[0].value(latch_value));
            }
            edit_phi(func, first.value(phi), |incoming| {
                for (from, value) in incoming.iter_mut() {
                    if *from == first_latch {
                        *from = last_latch;
                        *value = last.value(latch_value);
                    }
                }
            });
            edit_phi(func, phi, |incoming| {
                for (from, value) in incoming.iter_mut() {
                    if *from == unroll.preheader {
                        *from = main_header;
                        *value = first.value(phi);
                    }
                }
            });
        }
        (main_header, main_header)
    };

    // header_j: ...; jump body_j
    let copy_phis: FxHashSet<_> = copies[1..]
        .iter()
        .flat_map(|copy| header_phis.iter().map(|&(phi, _)| copy.value(phi)))
        .collect();
    for copy in &copies[1..] {
        let copy_header = copy.blocks[&unroll.header];
        let kept: Vec<_> = func.blocks[copy_header]
            .instructions
            .iter()
            .copied()
            .filter(|&inst| {
                func.inst_result_value(inst).is_none_or(|result| !copy_phis.contains(&result))
            })
            .collect();
        func.blocks[copy_header].instructions = kept;
        let (_, metadata) = func.blocks[copy_header].take_terminator();
        func.blocks[copy_header]
            .set_terminator(Terminator::Jump(copy.blocks[&unroll.body]), metadata);
    }

    // preheader: ... jump entry
    let (terminator, metadata) = func.blocks[unroll.preheader].take_terminator();
    let mut terminator = terminator.expect("the preheader is terminated");
    retarget(
        &mut terminator,
        |successor| {
            if successor == unroll.header { entry } else { successor }
        },
    );
    func.blocks[unroll.preheader].set_terminator(terminator, metadata);

    // exit: v = phi [block: x], ..., [block': x'], [block'': x'']
    for (block, successor) in side_exits {
        for inst in func.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = incoming
                    .iter()
                    .filter(|&&(from, _)| from == block)
                    .flat_map(|&(_, value)| {
                        copies.iter().map(move |copy| (copy.blocks[&block], copy.value(value)))
                    })
                    .collect();
                incoming.extend(cloned);
            }
        }
    }

    // A copied header's phi can map to another copied header's phi when the latch passes one
    // header phi to another, as a swap does (`a = phi [latch: b]`, `b = phi [latch: a]`): with
    // four copies, `a_2` maps to `b_1`, which maps to `a_0`. Follow each chain to the value that
    // survives.
    func.replace_uses_canonicalized(&replacements);
    rebuild_predecessors(func);
    main_header
}

/// Runs the loop's first iteration in a copy, after which the loop's own invariant tests hold.
fn peel(func: &mut Function, peel: &Peel) {
    let blocks: Vec<BlockId> = peel.blocks.iter().collect();
    // Edges that leave the loop from its body, before any terminator changes.
    let side_exits: Vec<_> = blocks
        .iter()
        .filter(|&&block| block != peel.header)
        .flat_map(|&block| {
            let terminator = func.blocks[block].terminator.as_ref().expect("terminated");
            terminator
                .successors()
                .into_iter()
                .filter(|&successor| !peel.blocks.contains(successor))
                .map(move |successor| (block, successor))
        })
        .collect();
    let copy = clone_loop(func, &blocks);
    let header_phis: Vec<_> = func.blocks[peel.header]
        .instructions
        .iter()
        .filter_map(|&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let latch_value =
                incoming.iter().find_map(|&(from, value)| (from == peel.latch).then_some(value))?;
            Some((func.inst_result_value(inst)?, latch_value))
        })
        .collect();
    let first_header = copy.blocks[&peel.header];
    let first_latch = copy.blocks[&peel.latch];

    // preheader: ... jump header'
    // header': state' = phi [preheader: start]
    //          jumpi test', body', header
    // latch': ... jump header
    redirect(func, peel.preheader, peel.header, first_header);
    redirect(func, first_header, peel.exit, peel.header);
    redirect(func, first_latch, first_header, peel.header);

    // header: state = phi [header': state'], [latch': next'], [latch: next]
    // When the copy's header declines the first iteration, the original one declines it too and
    // leaves the loop, so its exit stays the only one.
    for &(phi, latch_value) in &header_phis {
        edit_phi(func, copy.value(phi), |incoming| {
            incoming.retain(|&(from, _)| from != first_latch);
        });
        edit_phi(func, phi, |incoming| {
            for (from, value) in incoming.iter_mut() {
                if *from == peel.preheader {
                    *from = first_header;
                    *value = copy.value(phi);
                }
            }
            incoming.push((first_latch, copy.value(latch_value)));
        });
    }

    // exit: v = phi [block: x], [block': x']
    for (block, successor) in side_exits {
        for inst in func.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = incoming
                    .iter()
                    .filter(|&&(from, _)| from == block)
                    .map(|&(_, value)| (copy.blocks[&block], copy.value(value)))
                    .collect();
                incoming.extend(cloned);
            }
        }
    }

    // The loop runs only after the copy's tests held, and they test the same words.
    // block: jumpi condition, keep, abort => jump keep
    for &(block, keep) in &peel.tests {
        let (_, metadata) = func.blocks[block].take_terminator();
        func.blocks[block].set_terminator(Terminator::Jump(keep), metadata);
    }
    rebuild_predecessors(func);
}

/// Counts a loop's iterations from `-n` up to zero: `r = i - n` modulo `2^256` starts at `-n`,
/// steps by one as `i` did, and `i < n` is `r != 0`.
fn reverse(func: &mut Function, reversal: &Reverse) {
    // preheader: r0 = sub 0, n
    // header: r = phi [preheader: r0], [latch: r + 1]
    //         jumpi (ne r, 0), body, exit
    let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
    let (inst, start) = func.alloc_value_inst(
        Instruction::new(InstKind::Sub(zero, reversal.bound), Some(MirType::I256))
            .with_debug_info_dropped(),
    );
    func.blocks[reversal.preheader].instructions.push(inst);
    edit_phi(func, reversal.counter, |incoming| {
        for (from, value) in incoming.iter_mut() {
            if *from == reversal.preheader {
                *value = start;
            }
        }
    });
    let Value::Inst(inst) = *func.value(reversal.condition) else { return };
    func.inst_mut(inst).kind = InstKind::Ne(reversal.counter, zero);
}
