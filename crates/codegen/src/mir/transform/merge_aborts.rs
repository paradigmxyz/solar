//! Merging of tests that branch to the same aborting block.
//!
//! Checked arithmetic and ABI decoding guard straight-line code with tests that
//! each branch to an aborting block, such as a `Panic(0x11)` revert, and pay a
//! conditional jump apiece. When a block ends with such a test, its
//! continuation is entered from nowhere else, except from the test's own
//! aborting block when that block calls a function that never returns and only
//! this test enters it, holds only speculatable instructions, and ends with a
//! test into the same aborting block, the first test can wait for the second:
//!
//! ```text
//! b1: ...; jumpi c1, abort, b2         b1: ...; jump b2
//! b2: x = ...; jumpi c2, abort, b3  => b2: x = ...; c = or c1, c2; jumpi c, abort, b3
//! ```
//!
//! Two tests that continue on their true edges combine with `and` instead. A
//! test that aborts on its false edge while the other aborts on its true edge
//! turns around when it compares a word with a literal: `x < k` fails exactly
//! when `x > k - 1`, and that complement replaces it where it was. In loops any other such test
//! turns into its negation, `eq c, false`, as a checked product of two words does, which aborts
//! unless one factor is zero or the quotient gives back the other. In builds that copy loops, a
//! branch on `eq c, false` for a boolean `c` is a test of `c` with its edges swapped and merges
//! as `c`, as a checked product by a word does: it aborts unless the factor's comparison with
//! the hoisted limit, `gt x, limit`, fails, and that comparison joins the additions' checks with
//! no `iszero`. Merges repeat along a chain, so a run of checks, such as the overflow checks of
//! an unrolled loop's copies, branches once.
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
//! reads such as calldata loads: they write nothing and have fixed gas costs.
//! Operations whose gas depends on operands or account warmth stay behind the
//! original test. Merging can still change the gas spent before a
//! revert and the exact out-of-gas threshold. The aborting block must have no
//! phis, so it cannot tell which edge reached it; the values it reads dominate
//! both edges.
//!
//! Aborting blocks merge when they do the same, as the panic calls that an
//! unrolled loop's copies clone do; the merged test enters the second one.
//!
//! Profitability: every merge saves a conditional jump and its pushed label for an `and` or `or`,
//! priced by the target. A zero test of a word that aborts when the word is nonzero, as a
//! non-payable entry's `ne callvalue, 0` or an address check's `ne (shr 160, x), 0` does, joins
//! outside loops as the word itself: `c = ne (or w, zext c2), 0`, which a branch reads for free as
//! it read `w`, so the entry's value check and calldata size check branch once.
//!
//! A signed `intN` parameter's check aborts when `eq x, signextend(b, x)` fails. Outside loops
//! it joins as the word that is nonzero when it fails, `xor x, signextend(b, x)`, which replaces
//! the comparison at the price the target gives the pair. The ABI decoder already tests `bool`,
//! `uintN`, and power-of-two enum parameters as shifted words, which join as above; the e-graph
//! turns the signed check's exclusive or back into `eq x, signextend(b, x)`, so it has no
//! producer-side form. Next to a test that aborts on its false edge, the pair stays comparisons
//! that combine with `and`, as merges go pair by pair; in loops the test turns
//! around as above. Other zero tests of words stay apart: a branch reads the word for free, and a
//! loop's latch also takes its back edge on it, as an increment's overflow check `eq (add i, 1), 0`
//! does, while the merged test would pay an `iszero` for the word, another to turn the latch
//! around, and the shuffles that keep the first condition next to it. A negated test pays its
//! `iszero` only in loops, where the saved jump repeats, in size builds too. A boolean's negation
//! merges as the boolean only in builds that copy loops, where every copy of a body repeats the
//! saving: in a single body the condition stays live across the next check, and the scheduler
//! answered with more shuffles than the saved jump. Runs late, after the final loop passes, check
//! elimination and the CFG cleanup that joins each copy's blocks, so the tests it merges are the
//! ones that remain.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, Callee, EffectKind, Function, FunctionId, Immediate, InstKind, Instruction,
        MirType, Module, Terminator, Value, ValueId,
        analysis::{CfgInfo, aborts, cold_functions},
        pass::{MirPass, run_function_pass},
        utils::{rebuild_predecessors, replace_terminator},
    },
    target::{Cost, Target},
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
        let cold = cold_functions(module);
        run_function_pass(module, analyses, |func, analyses| {
            merge_function(func, &cold, &analyses.target(gcx))
        })
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

/// An unsigned comparison of a word with a literal: `word < literal` when `below`, and
/// `word > literal` otherwise.
#[derive(Clone, Copy)]
struct Compare {
    word: ValueId,
    below: bool,
    literal: U256,
}

/// The way one of two tests that abort on different edges turns around, so both abort when their
/// conditions hold: into the complement when it compares a word with a literal, or else into its
/// negation.
enum Flip {
    Complement(Compare),
    Negate,
}

/// A test on a block's way out: its condition, the aborting block, the continuation, and
/// whether the condition holding means aborting.
struct Test {
    condition: ValueId,
    abort: BlockId,
    next: BlockId,
    aborts_when_true: bool,
    /// The word that is nonzero when the test aborts, when it merges as a word rather than as a
    /// boolean. Such a test counts as aborting when true, as the word is nonzero; `condition`
    /// then goes unused unless the test turns back into a comparison.
    word: Option<Word>,
}

impl Test {
    /// Whether the test aborts on its false edge and merges as a boolean.
    fn is_plain_false(&self) -> bool {
        self.word.is_none() && !self.aborts_when_true
    }

    /// Turns a range word back into the comparison it came from, which aborts on its false edge.
    fn restore_comparison(&mut self) {
        self.word = None;
        self.aborts_when_true = false;
    }
}

/// A word that is nonzero exactly when a test aborts, so the test merges as the word.
#[derive(Clone, Copy)]
enum Word {
    /// The test's condition, which already is the word.
    Itself,
    /// `xor a, b`, for a test that `a == b` fails.
    Xor(ValueId, ValueId),
}

impl Word {
    /// The cost of the instruction that makes the word, and of the comparison it replaces.
    fn price(self, target: &Target) -> (Cost, Cost) {
        match self {
            Self::Itself => (Cost::ZERO, Cost::ZERO),
            Self::Xor(..) => (target.opcode(op::XOR), target.opcode(op::EQ)),
        }
    }

    /// Whether the word stands for a comparison, rather than being the tested word itself.
    fn is_range(self) -> bool {
        !matches!(self, Self::Itself)
    }
}

fn merge_function(func: &mut Function, cold: &DenseBitSet<FunctionId>, target: &Target) -> bool {
    rebuild_predecessors(func);
    let mut changed = false;
    let mut wraps = FxHashMap::default();
    // Merges remove only edges into aborting blocks, which lie on no cycle.
    let cfg = CfgInfo::new(func);
    let cyclic = cfg.cyclic_blocks();
    // Each sweep follows every chain of tests forward from its first block; a later sweep picks
    // up chains whose head a merge further down only now completed.
    loop {
        let mut merged = false;
        for mut block in func.blocks.indices() {
            while let Some((first, second, flip)) = mergeable(func, block, cold, cyclic, target) {
                let next = first.next;
                merge(func, block, first, second, flip, &mut wraps);
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
/// Builds that copy loops read a branch on a boolean's negation as a branch on the boolean.
fn test(
    func: &Function,
    block: BlockId,
    cold: &DenseBitSet<FunctionId>,
    cyclic: &DenseBitSet<BlockId>,
    target: &Target,
) -> Option<Test> {
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[block].terminator
    else {
        return None;
    };
    // A branch on `eq c, false` tests the boolean `c` with its edges swapped, so `c` merges as it
    // is, as a checked product's `gt x, limit` does.
    let (condition, then_block, else_block) = match negated_boolean(func, condition) {
        Some(inner) if target.copies_loops() => (inner, else_block, then_block),
        _ => (condition, then_block, else_block),
    };
    let shared = |block: BlockId| aborts(func, block, cold) && !func.block_has_phi(block);
    let (abort, next, aborts_when_true) = match (shared(then_block), shared(else_block)) {
        (true, false) => (then_block, else_block, true),
        (false, true) => (else_block, then_block, false),
        _ => return None,
    };
    // A branch tests a word against zero for free, so a zero test stays apart unless it aborts
    // when the word is nonzero: `or` then joins the word itself, and the merged branch tests the
    // combined word for free too.
    if let Some((word, nonzero)) = zero_test(func, condition) {
        if func.value_ty(word) != Some(MirType::I256) || nonzero != aborts_when_true {
            return None;
        }
        return Some(Test {
            condition: word,
            abort,
            next,
            aborts_when_true: true,
            word: Some(Word::Itself),
        });
    }
    // A range check that aborts when it fails leaves a nonzero word: the same join as above.
    // Loops keep such a test whole, to turn around into its complement.
    if !aborts_when_true
        && !cyclic.contains(block)
        && let Some(word) = range_word(func, condition)
    {
        return Some(Test { condition, abort, next, aborts_when_true: true, word: Some(word) });
    }
    Some(Test { condition, abort, next, aborts_when_true, word: None })
}

/// The word that is nonzero exactly when `condition` fails, for a signed ABI range check:
/// `x == signextend(b, x)` fails when their `xor` is nonzero.
fn range_word(func: &Function, condition: ValueId) -> Option<Word> {
    let &InstKind::Eq(a, b) = func.value_inst_kind(condition)? else { return None };
    let extends = |extended: ValueId, word: ValueId| {
        matches!(
            func.value_inst_kind(extended),
            Some(&InstKind::SignExtend(_, source)) if source == word
        )
    };
    ((extends(a, b) || extends(b, a))
        && func.value_u256(a).is_none()
        && func.value_ty(a) == Some(MirType::I256))
    .then_some(Word::Xor(a, b))
}

/// The word a condition compares with zero, and whether the condition holds when it is nonzero.
fn zero_test(func: &Function, condition: ValueId) -> Option<(ValueId, bool)> {
    let kind = func.value_inst_kind(condition)?;
    kind.nonzero_test_operand(func)
        .map(|word| (word, true))
        .or_else(|| kind.zero_test_operand(func).map(|word| (word, false)))
}

/// The boolean that `condition` negates, when it is `eq c, false` for an `i1` word `c`.
fn negated_boolean(func: &Function, condition: ValueId) -> Option<ValueId> {
    let inner = func.value_inst_kind(condition)?.zero_test_operand(func)?;
    (func.value_ty(inner) == Some(MirType::I1)).then_some(inner)
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

/// Whether an instruction may run before deciding an abort.
fn speculatable(kind: &InstKind, target: &Target) -> bool {
    !kind.has_side_effects()
        && !target.op_has_variable_gas(&kind.op())
        && match kind.effect_kind() {
            EffectKind::Pure => !matches!(kind, InstKind::Phi(_)),
            EffectKind::EnvironmentRead => true,
            _ => false,
        }
}

/// The two tests a block and its continuation end with, when the first can wait, and the test to
/// complement when they abort on different edges.
fn mergeable(
    func: &Function,
    block: BlockId,
    cold: &DenseBitSet<FunctionId>,
    cyclic: &DenseBitSet<BlockId>,
    target: &Target,
) -> Option<(Test, Test, Option<Flip>)> {
    let mut first = test(func, block, cold, cyclic, target)?;
    // Another edge into the continuation may leave only the first test's own aborting block,
    // when that block calls a function that never returns and this test alone enters it. The
    // merge removes that entrance, so the first test's condition still dominates the merged
    // test; any other entrance would reach it without that condition defined.
    let continuation = &func.blocks[first.next];
    if first.next == block || !continuation.predecessors.iter().all(|&from| {
        from == block
            || (from == first.abort
                && matches!(func.blocks[from].predecessors.as_slice(), [only] if *only == block))
    }) {
        return None;
    }
    let mut second = test(func, first.next, cold, cyclic, target)?;
    // A range word joins only a test that aborts when it holds. Next to one that aborts on its
    // false edge, both stay comparisons and combine with `and`.
    if first.word.is_some_and(Word::is_range) && second.is_plain_false() {
        first.restore_comparison();
    } else if second.word.is_some_and(Word::is_range) && first.is_plain_false() {
        second.restore_comparison();
    }
    if !same_abort(func, first.abort, second.abort, cold)
        || !continuation
            .instructions
            .iter()
            .all(|&inst| speculatable(&func.inst(inst).kind, target))
    {
        return None;
    }
    // A word joins only another test that aborts when it holds, with no flip, and only outside
    // loops: a latch's word test also takes the back edge, which the merge would turn around.
    if (first.word.is_some() || second.word.is_some())
        && (cyclic.contains(block) || !second.aborts_when_true)
    {
        return None;
    }
    // A comparison with a literal turns around for free, unlike a test that would pay an
    // `iszero`. Only loops repeat the saving; a flipped test that runs once measured no gain and
    // moved the stack scheduler to rebuild calldata words where it had kept them, so an ABI
    // word's range check joins as a word instead. A negation pays the `iszero`
    // in loops of every build: in size builds it measured 4,461 runtime bytes smaller over the
    // benchmark corpus, and the profitability check below prices the `iszero`.
    let flip = match (first.aborts_when_true, second.aborts_when_true) {
        (true, true) | (false, false) => None,
        _ if !cyclic.contains(block) => return None,
        (false, true) => {
            Some(complement(func, first.condition).map_or(Flip::Negate, Flip::Complement))
        }
        (true, false) => {
            Some(complement(func, second.condition).map_or(Flip::Negate, Flip::Complement))
        }
    };
    let mut added = if matches!(flip, Some(Flip::Negate)) {
        target.opcode(op::OR) + target.opcode(op::ISZERO)
    } else {
        target.opcode(op::OR)
    };
    let mut saved = target.label_push() + target.opcode(op::JUMPI);
    // A word made from a comparison's operands also replaces that comparison.
    for (word, replaced) in [first.word, second.word].into_iter().flatten().map(|w| w.price(target))
    {
        added += word;
        saved += replaced;
    }
    target
        .improves(
            i64::from(target.runtime_gas(saved)) - i64::from(target.runtime_gas(added)),
            i64::from(saved.bytes) - i64::from(added.bytes),
        )
        .then_some((first, second, flip))
}

/// The comparison that holds exactly when an unsigned comparison of a word with a literal fails:
/// `x < k` fails when `x > k - 1`, and `x > k` when `x < k + 1`.
fn complement(func: &Function, condition: ValueId) -> Option<Compare> {
    let literal = |value| func.value_u256(value);
    Some(match *func.value_inst_kind(condition)? {
        InstKind::Lt(word, k) | InstKind::Gt(k, word) if literal(word).is_none() => {
            Compare { word, below: false, literal: literal(k)?.checked_sub(U256::ONE)? }
        }
        InstKind::Gt(word, k) | InstKind::Lt(k, word) if literal(word).is_none() => {
            Compare { word, below: true, literal: literal(k)?.checked_add(U256::ONE)? }
        }
        _ => return None,
    })
}

/// Appends a compiler-generated instruction to a block and returns its result.
fn append(func: &mut Function, block: BlockId, kind: InstKind) -> ValueId {
    append_typed(func, block, kind, MirType::I1)
}

/// Appends a compiler-generated instruction with a result of type `ty` to a block.
fn append_typed(func: &mut Function, block: BlockId, kind: InstKind, ty: MirType) -> ValueId {
    let (inst, value) =
        func.alloc_value_inst(Instruction::new(kind, Some(ty)).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}

fn merge(
    func: &mut Function,
    predecessor: BlockId,
    mut first: Test,
    mut second: Test,
    flip: Option<Flip>,
    wraps: &mut FxHashMap<ValueId, Wrap>,
) {
    let block = first.next;
    // A complement replaces its test where the test was, as the original condition was:
    // b1: c1' = gt x, k - 1 | lt x, k + 1
    // b2: c2' = gt y, l - 1 | lt y, l + 1
    // Otherwise the test turns into its negation:
    // b1: c1' = eq c1, false
    // b2: c2' = eq c2, false
    if let Some(flip) = flip {
        let (test, at) =
            if first.aborts_when_true { (&mut second, block) } else { (&mut first, predecessor) };
        test.condition = match flip {
            Flip::Complement(compare) => append_compare(func, at, compare),
            Flip::Negate => {
                let no = func.alloc_value(Value::Immediate(Immediate::I1(false)));
                append(func, at, InstKind::Eq(test.condition, no))
            }
        };
        test.aborts_when_true = true;
    }
    let combined = if first.word.is_some() || second.word.is_some() {
        // w1 = zext i1 c1 to i256 | xor a, b (for a signed range check c1 of a and b)
        //      (unless c1 is already a word)
        // w2 = the same for c2
        // w = or w1, w2
        // c = ne w, 0
        // jumpi c, abort, b3
        let mut as_word = |test: &Test| match test.word {
            Some(Word::Itself) => test.condition,
            Some(Word::Xor(a, b)) => append_typed(func, block, InstKind::Xor(a, b), MirType::I256),
            None => append_typed(func, block, InstKind::Zext(test.condition), MirType::I256),
        };
        let (a, b) = (as_word(&first), as_word(&second));
        let word = append_typed(func, block, InstKind::Or(a, b), MirType::I256);
        let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        append(func, block, InstKind::Ne(word, zero))
    } else if second.aborts_when_true
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
    let (then_block, else_block) = if second.aborts_when_true {
        (second.abort, second.next)
    } else {
        (second.next, second.abort)
    };
    func.blocks[block].terminator =
        Some(Terminator::Branch { condition: combined, then_block, else_block });

    // b1: ...; jump b2
    replace_terminator(func, predecessor, Terminator::Jump(block));
}

/// Appends a comparison of a word with a literal and returns its result.
fn append_compare(func: &mut Function, block: BlockId, compare: Compare) -> ValueId {
    let literal = func.alloc_value(Value::Immediate(Immediate::I256(compare.literal)));
    let kind = if compare.below {
        InstKind::Lt(compare.word, literal)
    } else {
        InstKind::Gt(compare.word, literal)
    };
    append(func, block, kind)
}

/// The word `sum` adds to `base`, when it is `add base, addend`.
fn addend(func: &Function, sum: ValueId, base: ValueId) -> Option<ValueId> {
    match *func.value_inst_kind(sum)? {
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
            let &InstKind::Lt(sum, base) = func.value_inst_kind(first)? else { return None };
            Wrap { sum, base, bound: upper_bound(func, addend(func, sum, base)?, 8)? }
        }
    };
    let &InstKind::Lt(next, sum) = func.value_inst_kind(second)? else { return None };
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
    match *func.value_inst_kind(value)? {
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
        let &InstKind::Add(a, b) = func.value_inst_kind(next)? else { return None };
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
