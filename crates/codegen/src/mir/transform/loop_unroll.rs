//! Unrolling of counted loops by two or four, complete unrolling of loops that
//! run a literal number of times, and peeling of loops whose body tests words
//! computed before the loop.
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
//!   jumpi (ne (and (shr t, (sub n, i)), 1), 0), body_a, enter
//! body_a: peeled copy; jump enter
//! enter: state_0 = phi [check: state], [latch_a: next_a]; jump header
//! header: state = phi [enter: state_0], [latch_b: next_b]
//!   jumpi (ne i, n), body, exit
//! body: the original body; jump header_b
//! header_b: the header's other instructions for `i + s`; jump body_b
//! body_b: second copy; jump header
//! ```
//!
//! Both ways into the loop meet in `enter`, so the loop keeps a single
//! preheader: the backend carries a loop's words on the stack only through a
//! loop it enters by one edge, and otherwise copies the header's phis through
//! memory in every iteration.
//!
//! A bound built as `start + (m << t)`, as a pointer's end usually is, leaves
//! `m << t` to go on entry, whose bit `t` is bit zero of `m`, so the check
//! tests the low bit of `m` without the subtraction and the shift.
//!
//! A body that aborts unless a word computed before the loop holds, such as a
//! division's zero check on a divisor the loop never changes, first runs one
//! iteration in a copy that keeps those tests, and the loop after it jumps past
//! them. When the copy's header declines the first iteration, it enters the
//! original header, which declines it as well, so the original header's exit
//! stays the loop's only one. That edge and the copy's latch meet in a block
//! before the header, as the parity check's two edges do, so the loop keeps a
//! single preheader and, without those reads of words from before the loop,
//! may then unroll.
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
//! `i != n` or `i > 0` and otherwise leaves the loop, where `i` is a header phi,
//! or a pointer phi the header converts to an integer, that the loop's single
//! back edge advances by adding a literal step, or for `!=` and `> 0` also by
//! subtracting one, and `n` is defined outside the loop. The latch may also
//! branch to a block that aborts, such as the increment's overflow check. A `<`
//! or `<=` test also needs a literal start, except for `<=` stepping by one.
//! The header's other instructions must be free of effects, because the
//! original header repeats them for the iteration the main loop declined. Every
//! branch inside the body continues the loop on one arm and leaves it on the
//! other: to a block that aborts, such as an arithmetic panic, where a block
//! aborts when it reverts or calls a function that never returns, or to a side
//! exit, such as the block a search reaches at its match. The body may read
//! words computed before the loop only when the backend rebuilds them where
//! they are read, as calldata words at fixed offsets and environment reads, or
//! when it only compares against them, as a checked multiplication by a word
//! the loop never changes compares against its hoisted product limit. The
//! stack scheduler spills such words, and copies that compute with them
//! measured slower than the original loop, while copies that only compare
//! against them measured faster.
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
//! phi input on that edge, so every definition still dominates its uses. A side
//! exit entered from a single loop block that reads loop values directly, as a
//! search's match reads the pointer it stopped at, first takes each of them
//! through a phi on that edge, so every copy brings its own. An aborting block
//! gets no such phis: the backend never carries its phis on the stack, but
//! copies them through memory ahead of every branch to it, in every iteration,
//! so a direct read in an aborting block keeps the loop rolled. The header's
//! exit stays the only way to leave the original loop through its test, so
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
//! original body's. When no copy reads the counter between its steps, the
//! copies' steps chain as `add (add i, s), s`, which the pass sums into
//! `add i, 2s`. Addition wraps modulo `2^256`, so the sum is exact, and the
//! group advances the counter once. A copy that reads its step only as the
//! address of one memory or calldata access, as a pointer's copies read their
//! elements, sums too: its other offsets read the group's first pointer as
//! `add p, s + c`, `evm-inst-schedule` moves the one remaining step next to its
//! access, and the copies stop rewriting the pointer's stack slot between them.
//! A step read as a value, such as a counter added to an accumulator, keeps its
//! chain: pinning the base on the stack measured slower there.
//!
//! A loop whose literal start, step and bound give it at most 16 iterations
//! becomes that many copies of its header and body. For `<` and `<=`, the final
//! counter update must not wrap. The copies are chained in order: each
//! copy's header jumps into its body, as its test holds, and takes the previous
//! copy's latch values for its phis. The original header follows the last copy
//! and jumps to the exit, as its test fails there, so the values read after the
//! loop keep their definitions, and the original body becomes unreachable.
//! Side exits gain a phi input from every copy. This runs before any other
//! unrolling, saves every test and back edge, and lets later folding treat the
//! counter as a constant in each copy. It pays when that saving outweighs the
//! body's code once more per iteration but one, less the header's test, and
//! the copies of the body may take at most 256 bytes, so that a contract near
//! the code size limit cannot grow past it.
//!
//! Profitability: gas mode only, and only when the optimizer runs reach the
//! target's threshold for copying loops: unrolling, complete unrolling and
//! peeling trade code size for runtime gas, and at fewer runs, as at the
//! default 200, a build still values its size. Counting a counter up to zero
//! copies nothing and runs at any runs. Copies are priced by the target over
//! the deployment's expected executions. Each group of `k` iterations skips
//! `k - 1` header tests and back-edge jumps. For `<` and `<=`, the first group
//! pays for the original loop's last test and every test of the main loop,
//! including the declined one, adds the offset first, except for `<=` stepping
//! by one; for `!=`, which unrolls by two, the parity check costs about one
//! test per entry. The new code holds `k` more copies of the body and one of
//! the header, and the pass takes the factor that saves the most. Literal
//! bounds give the trip count, and other loops are assumed to run the target's
//! estimate for uncounted loops. A peeled copy of the header and the body pays
//! when the conditional jumps that the iterations after the first skip save
//! more. A `<=` loop stepping by one unrolls by four only from a literal start,
//! as its main test then adds the offset. Runs after the late loop passes have
//! fixed the loop's physical shape, and before the final CFG cleanup and
//! dead-code elimination remove the cloned tests the copies no longer read.

use super::loop_split::retarget;
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionId, Immediate, InstId, InstKind, Instruction,
        MirType, Module, OpTraits, Terminator, Value, ValueId,
        analysis::{Loop, LoopAnalyzer, LoopInfo, aborts, cold_functions},
        pass::{MirPass, run_function_pass},
        utils::rebuild_predecessors,
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
    /// Side exits that read loop values directly, with the values a phi on their edge
    /// carries instead before the loop is copied.
    exit_phis: Vec<(BlockId, Vec<ValueId>)>,
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

/// A loop that runs a literal number of times, few enough to repeat its body that many times in
/// place of the loop.
struct Full {
    header: BlockId,
    preheader: BlockId,
    latch: BlockId,
    body: BlockId,
    exit: BlockId,
    blocks: DenseBitSet<BlockId>,
    trips: u32,
}

/// The most iterations a loop may run to be unrolled completely, whatever its pricing.
const MAX_COMPLETE_TRIPS: u64 = 16;

/// The most bytes the copies of a completely unrolled loop's body may take, whatever its
/// pricing, so that a contract near the code size limit cannot grow past it.
const MAX_COMPLETE_BYTES: u32 = 256;

fn unroll_function(func: &mut Function, cold: &DenseBitSet<FunctionId>, target: Target) -> bool {
    let mut changed = false;
    let mut done = FxHashSet::default();
    let mut peeled = FxHashSet::default();
    // A loop that runs a literal number of times becomes that many copies of its body, with
    // no test or jump left between them.
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(full) = loops.all_loops().find_map(|l| plan_full(func, &loops, l, cold, target))
        else {
            break;
        };
        unroll_fully(func, &full);
        changed = true;
    }
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
    // and leaves it on the other, to an abort such as an arithmetic panic or to a side exit
    // such as a search's match.
    for block in l.blocks.iter() {
        if block == l.header || aborts(func, block, cold) {
            continue;
        }
        let successors = func.blocks[block].terminator.as_ref()?.successors();
        let continuing: Vec<_> = successors
            .iter()
            .filter(|&&successor| l.blocks.contains(successor) && !aborts(func, successor, cold))
            .collect();
        if continuing.len() != 1 {
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
    cold: &DenseBitSet<FunctionId>,
) -> bool {
    exit_reads(func, l, exit, loop_insts, cold).is_some_and(|reads| reads.is_empty())
}

/// The loop values that side exits entered only from the loop read directly, by exit, which a
/// phi on the exit edge can carry instead; `None` when a block further from the loop reads a
/// loop value other than as the phi input of an edge from the loop.
fn exit_reads(
    func: &Function,
    l: &Loop,
    exit: BlockId,
    loop_insts: &DenseBitSet<InstId>,
    cold: &DenseBitSet<FunctionId>,
) -> Option<Vec<(BlockId, Vec<ValueId>)>> {
    let defined_in_loop = |value| match func.value(value) {
        Value::Inst(inst) => loop_insts.contains(*inst),
        _ => false,
    };
    let mut reads = Vec::new();
    let mut reachable = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = Vec::new();
    for block in l.blocks.iter() {
        let terminator = func.blocks[block].terminator.as_ref()?;
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
        let terminator = body.terminator.as_ref()?;
        // A side exit entered only from one loop block can take its reads through phis. The
        // backend never carries the phis of an aborting block on the stack: it copies them
        // through memory ahead of the branch, so such a block keeps the loop rolled when it
        // reads a loop value directly.
        let fixable = matches!(body.predecessors.as_slice(), &[pred] if l.blocks.contains(pred))
            && !aborts(func, block, cold);
        let mut direct = Vec::new();
        let mut read = |value: ValueId| {
            if !defined_in_loop(value) {
                return true;
            }
            if fixable && func.value_ty(value).is_some() {
                if !direct.contains(&value) {
                    direct.push(value);
                }
                return true;
            }
            false
        };
        for &inst in &body.instructions {
            let kind = &func.inst(inst).kind;
            let reads_through_phis = match kind {
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .all(|&(from, value)| l.blocks.contains(from) || !defined_in_loop(value)),
                _ => kind.operands().into_iter().all(&mut read),
            };
            if !reads_through_phis {
                return None;
            }
        }
        if !terminator.operands().into_iter().all(&mut read) {
            return None;
        }
        if !direct.is_empty() {
            reads.push((block, direct));
        }
        for successor in terminator.successors() {
            if !l.blocks.contains(successor) && reachable.insert(successor) {
                worklist.push(successor);
            }
        }
    }
    Some(reads)
}

/// Gives each side exit a phi for every loop value it reads directly, so that every copy of
/// the loop can bring its own value along its own edge.
fn add_exit_phis(func: &mut Function, exit_phis: &[(BlockId, Vec<ValueId>)]) {
    for (block, values) in exit_phis {
        let &[pred] = func.blocks[*block].predecessors.as_slice() else { continue };
        let mut replacements = FxHashMap::default();
        let mut phis = Vec::with_capacity(values.len());
        for &value in values {
            // block: value' = phi [pred: value]
            let (phi, result) = func.alloc_value_inst(
                Instruction::new(InstKind::Phi(vec![(pred, value)]), func.value_ty(value))
                    .with_debug_info_dropped(),
            );
            phis.push(phi);
            replacements.insert(value, result);
        }
        let mut instructions = phis;
        for inst in std::mem::take(&mut func.blocks[*block].instructions) {
            let kind = &mut func.inst_mut(inst).kind;
            if !matches!(kind, InstKind::Phi(_)) {
                kind.visit_operands_mut(|operand| {
                    *operand = replacements.get(operand).copied().unwrap_or(*operand);
                });
            }
            instructions.push(inst);
        }
        func.blocks[*block].instructions = instructions;
        if let Some(terminator) = func.blocks[*block].terminator.as_mut() {
            terminator.visit_operands_mut(|operand| {
                *operand = replacements.get(operand).copied().unwrap_or(*operand);
            });
        }
    }
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
    let shape = shape(func, loops, l, cold)?;
    let Shape { preheader, latch, body, exit, ref loop_insts, .. } = shape;
    // The stack planner spills a word the body reads from before the loop, and the copies
    // measured slower than the original loop, unless the body only compares against it, as the
    // header compares the counter with its bound: a hoisted product limit is such a word.
    let reads_hoisted = |value: ValueId| {
        matches!(func.value(value), Value::Inst(inst) if !loop_insts.contains(*inst))
            && !rebuilt_at_use(func, value)
    };
    for block in l.blocks.iter() {
        if block == l.header {
            continue;
        }
        if func.blocks[block].instructions.iter().any(|&inst| {
            let kind = &func.inst(inst).kind;
            !matches!(
                kind,
                InstKind::Phi(_)
                    | InstKind::Lt(..)
                    | InstKind::Gt(..)
                    | InstKind::SLt(..)
                    | InstKind::SGt(..)
            ) && kind.operands().into_iter().any(reads_hoisted)
        }) || func.blocks[block]
            .terminator
            .as_ref()
            .is_some_and(|terminator| terminator.operands().into_iter().any(reads_hoisted))
        {
            return None;
        }
    }
    let Counter { test, word, bound, start, step, trips } = counter(func, l, &shape)?;
    let literal_start = func.value_u256(start);
    let trips = trips.unwrap_or(Target::UNCOUNTED_LOOP_ITERATIONS);
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

    let exit_phis = exit_reads(func, l, exit, loop_insts, cold)?;
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
        exit_phis,
    })
}

/// A loop's counter as its header tests it.
struct Counter {
    test: Test,
    /// The counter as the header tests it: the counting phi, or its integer value.
    word: ValueId,
    /// The loop-invariant bound the header compares the counter against.
    bound: ValueId,
    /// The counter's value on entry.
    start: ValueId,
    step: U256,
    /// How many iterations the loop runs, when its start and bound are literals.
    trips: Option<u64>,
}

/// Recognizes the counter that a loop of `shape` steps and its header tests.
fn counter(func: &Function, l: &Loop, shape: &Shape) -> Option<Counter> {
    let Shape { preheader, latch, enters_on_true, condition, ref loop_insts, .. } = *shape;
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
    let trips = match (test, func.value_u256(start), func.value_u256(bound)) {
        (Test::Below | Test::AtMost | Test::UpTo, Some(start), Some(limit)) => {
            let trips = if matches!(test, Test::Below) {
                Some(limit.saturating_sub(start).div_ceil(step))
            } else {
                limit
                    .checked_sub(start)
                    .map_or(Some(U256::ZERO), |left| (left / step).checked_add(U256::from(1)))
            };
            // The final update must reach the failing test without wrapping below the bound.
            trips.filter(|&trips| {
                step.checked_mul(trips).and_then(|travel| start.checked_add(travel)).is_some()
            })
        }
        // A step of `2^256 - m` counts down by `m`.
        (Test::Reaches, Some(start), Some(limit)) => {
            let (distance, magnitude) = if step.bit(255) {
                (start.wrapping_sub(limit), step.wrapping_neg())
            } else {
                (limit.wrapping_sub(start), step)
            };
            (distance % magnitude).is_zero().then(|| distance / magnitude)
        }
        _ => None,
    };
    let trips = trips.map(|trips| u64::try_from(trips).unwrap_or(u64::MAX));
    Some(Counter { test, word, bound, start, step, trips })
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
    if tests.is_empty() || !exits_read_through_phis(func, l, shape.exit, &shape.loop_insts, cold) {
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

/// Plans to unroll a loop completely when it runs a literal number of times and removing every
/// test and back edge saves more lifetime gas than the copies' deposit costs.
fn plan_full(
    func: &Function,
    loops: &LoopInfo,
    l: &Loop,
    cold: &DenseBitSet<FunctionId>,
    target: Target,
) -> Option<Full> {
    if !target.copies_loops() {
        return None;
    }
    let shape = shape(func, loops, l, cold)?;
    let counter = counter(func, l, &shape)?;
    let trips = counter.trips.filter(|trips| (1..=MAX_COMPLETE_TRIPS).contains(trips))?;
    if !exits_read_through_phis(func, l, shape.exit, &shape.loop_insts, cold) {
        return None;
    }
    // Every iteration and the final test skip the header's test, and every iteration skips its
    // back edge; the copies hold the body once more for every iteration but one, and the
    // header's test goes away.
    let compare = match counter.test {
        Test::Below => op::LT,
        Test::AtMost | Test::UpTo => op::GT,
        Test::Reaches => op::SUB,
    };
    let tested = target.opcode(compare)
        + target.dup().times(2)
        + target.opcode(op::PUSH2)
        + target.opcode(op::JUMPI);
    let back_edge =
        target.opcode(op::PUSH2) + target.opcode(op::JUMP) + target.opcode(op::JUMPDEST);
    let trips = u32::try_from(trips).ok()?;
    let saving = (tested + back_edge).times(trips) + tested;
    let body: Cost = l
        .blocks
        .iter()
        .filter(|&block| block != l.header)
        .map(|block| target.block_code_estimate(func, block))
        .sum();
    if body.times(trips).bytes > MAX_COMPLETE_BYTES {
        return None;
    }
    let header = target.block_code_estimate(func, l.header);
    let growth = body.times(trips - 1).bytes.saturating_sub(header.bytes);
    (target.lifetime_gas(Cost::new(saving.gas, 0)) > target.lifetime_gas(Cost::new(0, growth)))
        .then(|| Full {
            header: l.header,
            preheader: shape.preheader,
            latch: shape.latch,
            body: shape.body,
            exit: shape.exit,
            blocks: l.blocks.clone(),
            trips,
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

/// The value a header phi takes on entry from `preheader`.
fn entry_value(func: &Function, phi: ValueId, preheader: BlockId) -> Option<ValueId> {
    let InstKind::Phi(incoming) = inst_kind(func, phi)? else { return None };
    incoming.iter().find_map(|&(from, value)| (from == preheader).then_some(value))
}

/// The operand of `sum = add a, b` other than `start`.
fn addend(func: &Function, sum: ValueId, start: ValueId) -> Option<ValueId> {
    match *inst_kind(func, sum)? {
        InstKind::Add(a, b) if a == start => Some(b),
        InstKind::Add(a, b) if b == start => Some(a),
        _ => None,
    }
}

/// `m` when `value` is `shl bits, m`.
fn shifted_left(func: &Function, value: ValueId, bits: usize) -> Option<ValueId> {
    let &InstKind::Shl(count, shifted) = inst_kind(func, value)? else { return None };
    (func.value_u256(count) == Some(U256::from(bits))).then_some(shifted)
}

/// Returns the operand of an addition other than its literal, and the literal.
fn literal_step(func: &Function, lhs: ValueId, rhs: ValueId) -> Option<(ValueId, U256)> {
    match (func.value_u256(lhs), func.value_u256(rhs)) {
        (None, Some(step)) => Some((lhs, step)),
        (Some(step), None) => Some((rhs, step)),
        _ => None,
    }
}

/// Sums each literal step in `blocks` with the literal step before it when nothing else reads the
/// earlier sum, or one access only as its address, as the copies' counter steps chain when no
/// body reads the counter, and a pointer's when each copy reads it at constant offsets.
fn fold_steps(func: &mut Function, blocks: &[BlockId]) {
    let mut uses = super::egraph::use_counts(func);
    // The copies' header tests no branch reads any more still read the counter; drop them
    // first, as dead-code elimination would.
    let mut dropped = true;
    while dropped {
        dropped = false;
        for &block in blocks {
            let mut kept = Vec::with_capacity(func.blocks[block].instructions.len());
            for inst in func.blocks[block].instructions.clone() {
                if func.inst_result_value(inst).is_some_and(|result| uses[result] == 0)
                    && func.inst(inst).kind.effect_kind() == EffectKind::Pure
                {
                    func.inst(inst).kind.visit_operands(|operand| uses[operand] -= 1);
                    dropped = true;
                } else {
                    kept.push(inst);
                }
            }
            func.blocks[block].instructions = kept;
        }
    }
    // The literal sums each step's result feeds, `add i_1, c` for `i_1 = add i, s`.
    let literal_sum = |func: &Function, inst: InstId| {
        let InstKind::Add(lhs, rhs) = func.inst(inst).kind else { return None };
        (func.inst(inst).result_ty == Some(MirType::I256)).then_some(())?;
        literal_step(func, lhs, rhs)
    };
    // The steps each memory or calldata access reads as its address.
    let is_step = |func: &Function, value: ValueId| matches!(*func.value(value), Value::Inst(inst) if literal_sum(func, inst).is_some());
    let mut sums = FxHashMap::<ValueId, u32>::default();
    let mut addresses = FxHashMap::<ValueId, u32>::default();
    for &block in blocks {
        for &inst in &func.blocks[block].instructions {
            if let Some((inner, _)) = literal_sum(func, inst)
                && is_step(func, inner)
            {
                *sums.entry(inner).or_default() += 1;
            } else if func.inst(inst).kind.effect_kind() != EffectKind::Pure
                && let Some(&address) = func.inst(inst).kind.operands().first()
                && is_step(func, address)
            {
                *addresses.entry(address).or_default() += 1;
            }
        }
    }
    for &block in blocks {
        for inst in func.blocks[block].instructions.clone() {
            // A sum reads the earlier step's base instead when nothing else reads the step, or
            // only one access as its address: the step then lives only up to that access, which
            // `evm-inst-schedule` moves next to it, and the copies stop rewriting the pointer's
            // slot between them. A step read as a value, such as a counter added to an
            // accumulator, keeps its chain: pinning the base on the stack measured slower there.
            if let Some((inner, outer)) = literal_sum(func, inst)
                && let others = uses[inner] - sums.get(&inner).copied().unwrap_or_default()
                && (others == 0 || (others == 1 && addresses.get(&inner) == Some(&1)))
                && let Value::Inst(inner_inst) = *func.value(inner)
                && let Some((base, step)) = literal_sum(func, inner_inst)
            {
                // inner = add base, step; next = add inner, outer
                // next = add base, step + outer
                let sum = Value::Immediate(Immediate::I256(step.wrapping_add(outer)));
                let sum = func.alloc_value(sum);
                func.inst_mut(inst).kind = InstKind::Add(base, sum);
                uses[inner] -= 1;
                uses[base] += 1;
                if let Some(count) = sums.get_mut(&inner) {
                    *count -= 1;
                }
            }
        }
    }
}

/// Unrolls the loop and returns the main loop's header.
fn apply(func: &mut Function, unroll: &Unroll) -> BlockId {
    add_exit_phis(func, &unroll.exit_phis);
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
        //        left = sub bound, word' | word' when bound = 0 | d when bound = add start, d
        //        jumpi (ne (and (shr zeros(step), left), 1), 0), body', header
        //        jumpi (ne (and m, 1), 0), body', header   when left = shl zeros(step), m
        let check = first.blocks[&unroll.header];
        let word = first.value(unroll.word);
        // Against a zero bound, `0 - i` and `i` agree at bit `t` whenever `i` is a multiple of
        // `2^t`, and otherwise neither loop ends. A bound built as `start + d`, as a pointer's
        // end is, leaves `d` to go on entry.
        let mut left = if func.value_u256(unroll.bound).is_some_and(|bound| bound.is_zero()) {
            word
        } else if let Some(distance) = entry_value(func, unroll.word, unroll.preheader)
            .and_then(|start| addend(func, unroll.bound, start))
        {
            distance
        } else {
            append(func, check, InstKind::Sub(unroll.bound, word), MirType::I256)
        };
        let zeros = unroll.step.trailing_zeros();
        if zeros != 0 {
            // Bit `t` of `m << t` is bit zero of `m`.
            left = match shifted_left(func, left, zeros) {
                Some(count) => count,
                None => {
                    let shift =
                        func.alloc_value(Value::Immediate(Immediate::I256(U256::from(zeros))));
                    append(func, check, InstKind::Shr(shift, left), MirType::I256)
                }
            };
        }
        // Integer conversions are already lowered here, so mask the low bit.
        let one = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(1))));
        let bit = append(func, check, InstKind::And(left, one), MirType::I256);
        let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let odd = append(func, check, InstKind::Ne(bit, zero), MirType::I1);
        // Both ways into the loop meet in a block of their own, so the loop keeps a single
        // preheader, which the stack planner needs to keep its words on the stack.
        let enter = func.alloc_block();
        func.blocks[enter].set_generated_terminator(Terminator::Jump(unroll.header));
        branch(func, check, odd, first.blocks[&unroll.body], enter);

        // latch': ... enter
        // latch: ... header''
        // latch'': ... header
        let last_header = last.blocks[&unroll.header];
        redirect(func, first_latch, check, enter);
        redirect(func, unroll.latch, unroll.header, last_header);
        redirect(func, last_latch, last_header, unroll.header);

        // check: state' = phi [preheader: start]
        // enter: state_0 = phi [check: state'], [latch': next']
        // header: state = phi [enter: state_0], [latch'': next'']
        for &(phi, latch_value) in &header_phis {
            replacements.insert(last.value(phi), latch_value);
            edit_phi(func, first.value(phi), |incoming| {
                incoming.retain(|&(from, _)| from != first_latch);
            });
            let (merge, merged) = func.alloc_value_inst(
                Instruction::new(
                    InstKind::Phi(vec![
                        (check, first.value(phi)),
                        (first_latch, first.value(latch_value)),
                    ]),
                    func.value_ty(phi),
                )
                .with_debug_info_dropped(),
            );
            func.blocks[enter].instructions.push(merge);
            edit_phi(func, phi, |incoming| {
                for (from, value) in incoming.iter_mut() {
                    if *from == unroll.preheader {
                        *from = enter;
                        *value = merged;
                    } else if *from == unroll.latch {
                        *from = last_latch;
                        *value = last.value(latch_value);
                    }
                }
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

    // i_2 = add (add i, s), s -> i_2 = add i, 2s, when no copy reads i_1
    let cloned: Vec<_> =
        copies.iter().flat_map(|copy| blocks.iter().map(|block| copy.blocks[block])).collect();
    fold_steps(func, &cloned);
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

    // Both ways from the copy into the loop meet in a block of their own, so the loop keeps a
    // single preheader, which the stack planner needs to keep its words on the stack and which
    // lets the loop unroll next.
    let enter = func.alloc_block();
    func.blocks[enter].set_generated_terminator(Terminator::Jump(peel.header));

    // preheader: ... jump header'
    // header': state' = phi [preheader: start]
    //          jumpi test', body', enter
    // latch': ... jump enter
    redirect(func, peel.preheader, peel.header, first_header);
    redirect(func, first_header, peel.exit, enter);
    redirect(func, first_latch, first_header, enter);

    // enter: state_0 = phi [header': state'], [latch': next']
    // header: state = phi [enter: state_0], [latch: next]
    // When the copy's header declines the first iteration, the original one declines it too and
    // leaves the loop, so its exit stays the only one.
    for &(phi, latch_value) in &header_phis {
        edit_phi(func, copy.value(phi), |incoming| {
            incoming.retain(|&(from, _)| from != first_latch);
        });
        let (merge, merged) = func.alloc_value_inst(
            Instruction::new(
                InstKind::Phi(vec![
                    (first_header, copy.value(phi)),
                    (first_latch, copy.value(latch_value)),
                ]),
                func.value_ty(phi),
            )
            .with_debug_info_dropped(),
        );
        func.blocks[enter].instructions.push(merge);
        edit_phi(func, phi, |incoming| {
            for (from, value) in incoming.iter_mut() {
                if *from == peel.preheader {
                    *from = enter;
                    *value = merged;
                }
            }
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

/// Replaces a loop that runs `trips` times with that many copies of its header and body.
fn unroll_fully(func: &mut Function, full: &Full) {
    let blocks: Vec<BlockId> = full.blocks.iter().collect();
    // Edges that leave the loop from its body, before any terminator changes.
    let side_exits: Vec<_> = blocks
        .iter()
        .filter(|&&block| block != full.header)
        .flat_map(|&block| {
            let terminator = func.blocks[block].terminator.as_ref().expect("terminated");
            terminator
                .successors()
                .into_iter()
                .filter(|&successor| !full.blocks.contains(successor))
                .map(move |successor| (block, successor))
        })
        .collect();
    let header_phis: Vec<_> = func.blocks[full.header]
        .instructions
        .iter()
        .filter_map(|&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let latch_value =
                incoming.iter().find_map(|&(from, value)| (from == full.latch).then_some(value))?;
            Some((func.inst_result_value(inst)?, latch_value))
        })
        .collect();
    let copies: Vec<Copy> = (0..full.trips).map(|_| clone_loop(func, &blocks)).collect();

    // preheader: ... jump header_1
    redirect(func, full.preheader, full.header, copies[0].blocks[&full.header]);
    for (index, copy) in copies.iter().enumerate() {
        let header = copy.blocks[&full.header];
        let latch = copy.blocks[&full.latch];
        let next = copies.get(index + 1).map_or(full.header, |next| next.blocks[&full.header]);
        // header_k: state_k = phi [preheader: start] | [latch_(k-1): next_(k-1)]
        //           jump body_k
        // latch_k: ... jump header_(k+1)
        for &(phi, latch_value) in &header_phis {
            let previous = index.checked_sub(1).map(|previous| &copies[previous]);
            edit_phi(func, copy.value(phi), |incoming| match previous {
                Some(previous) => {
                    *incoming = vec![(previous.blocks[&full.latch], previous.value(latch_value))];
                }
                None => incoming.retain(|&(from, _)| from == full.preheader),
            });
        }
        let (_, metadata) = func.blocks[header].take_terminator();
        func.blocks[header].set_terminator(Terminator::Jump(copy.blocks[&full.body]), metadata);
        redirect(func, latch, header, next);
    }

    // header: state = phi [latch_n: next_n], [latch: next]
    //         jump exit
    // After the last copy the header's test fails, so the original body becomes unreachable and
    // its latch's phi inputs only keep the phis consistent until cleanup removes it.
    let last = &copies[copies.len() - 1];
    for &(phi, latch_value) in &header_phis {
        edit_phi(func, phi, |incoming| {
            for (from, value) in incoming.iter_mut() {
                if *from == full.preheader {
                    *from = last.blocks[&full.latch];
                    *value = last.value(latch_value);
                }
            }
        });
    }
    let (_, metadata) = func.blocks[full.header].take_terminator();
    func.blocks[full.header].set_terminator(Terminator::Jump(full.exit), metadata);

    // exit: v = phi [block: x], [block_1: x_1], ..., [block_n: x_n]
    for (block, successor) in side_exits {
        for inst in func.blocks[successor].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                let cloned: Vec<_> = copies
                    .iter()
                    .flat_map(|copy| {
                        incoming
                            .iter()
                            .filter(|&&(from, _)| from == block)
                            .map(|&(_, value)| (copy.blocks[&block], copy.value(value)))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                incoming.extend(cloned);
            }
        }
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
