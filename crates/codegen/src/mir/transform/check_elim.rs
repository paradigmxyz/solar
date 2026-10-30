//! Range-based overflow-check elimination.
//!
//! Checked 0.8.x arithmetic lowers every `add`/`sub`/`mul` with a wrap test
//! that branches to a `Panic(0x11)` block. Most of those tests are dominated
//! by a guard that already proves the operation cannot wrap:
//!
//! - a loop header guard `i < n` proves `i <= 2^256 - 2`, so the `i + 1` increment cannot overflow
//!   and its `lt i+1, i` check is constant false;
//! - `require(b <= a)` proves the following `a - b` cannot underflow, so its `lt a, b` check is
//!   constant false;
//! - a constant bound `x < C` proves `x * K` or `x + K` cannot wrap whenever `(C-1) * K` (resp.
//!   `(C-1) + K`) fits in 256 bits, so the `div`-based mul check or the add check folds;
//! - a product by `2^k` that keeps its shift, tested as `(x << k) >> k == x`, cannot wrap while `x`
//!   stays below `2^(256 - k)`, as the offset `i * 32` of a calldata array element does for a loop
//!   counter.
//!
//! The pass walks the dominator tree. On entry to a block with a unique
//! predecessor ending in a two-way branch, it records what the branch
//! condition implies on that edge: value ranges refined by constants
//! (`x < C` => `x` in `[0, C-1]`, and `x >> k == 0` => `x` in `[0, 2^k - 1]`)
//! and relational predicates between SSA values (`!(a < b)` => `b <= a`).
//! Facts attach to SSA values, which are
//! never redefined, so a fact derived on a dominating edge holds in every
//! dominated block. Branch conditions are then evaluated against the
//! recorded facts with checked 256-bit arithmetic; a condition that is
//! provably constant folds the branch to an unconditional jump, and the dead
//! panic block is cleaned up by the existing CFG passes. A condition that stays
//! undecided drops any `or` operand proven false and `and` operand proven true,
//! as the `x == 0` half of a checked product's `x == 0 || x * y / x == y` when
//! `x` is a counter above zero, and the branch reads what remains. Anything
//! that is not provable is left untouched. Explicit integer casts retain range facts only
//! when their width and sign semantics preserve the bounded values. Semantic checks use the same
//! facts in instruction order: a passing check refines all later execution, and a proven passing
//! check can be removed before expansion. Facts roll back on leaving each dominator subtree, so a
//! check on one conditional path cannot justify removing a check on another. Semantic unsigned
//! `checked_binary` operations become plain ones the same way: a sum or product the operand
//! ranges keep within the type, a difference whose operands are ordered, and a quotient or
//! remainder by a divisor above zero.
//! Opcode result widths also bound environment values, including returndata lengths.
//!
//! The same edge facts propagate equalities, as GVN's `propagateEquality` does: where a unique
//! predecessor proves `x == C` for an immediate `C` (the true edge of `eq x, C`, the false edge of
//! `ne x, C`, or a passed check of one), instructions the edge dominates read `C` in place of `x`.
//! `check-elim` and `integer-cleanup` run it. A guarded length then makes the round-up, copy size,
//! and encoder size arithmetic below it constant for `sccp` and `egraph`, which a range alone
//! would not rewrite. Only constants the target prices as cheap to read qualify, so a word-wide
//! mask or an address never replaces a `DUP`. An instruction takes the constant for every operand
//! proven equal when that makes it evaluate to a cheap immediate, the way `sccp` will; a wider
//! result stays symbolic, since a memory size it grew would move the spill area above it on every
//! path, including those that skip the guard. Otherwise it takes it only for a value that at least
//! two instructions in the proving block's subtree read, a passed check's block counting its reads
//! above the check too: the constant replaces a stack slot and a `DUP` at each read, but at a
//! single read it costs what the value does and only makes sites that were identical, such as
//! inlined copies of one guard body, differ for the tail merging after this pass. Memory addresses
//! and sizes, call arguments, and returned values take constants like any other operand: frame
//! layout keeps spill slots clear of a constant address or size, and the backend protects them from
//! a write it cannot place, so nothing needs to hide them. An outer edge's constant wins over a
//! contradicting inner one, whose edge is dead. Phi operands and terminators keep their values.
//! Removing a redundant mask replaces its result by its input.
//!
//! Before the dominator walk, a bounded forward analysis carries the intersection
//! of relational facts and the union of ranges across predecessor edges. Phi
//! ranges are evaluated in their incoming edge contexts, and so are a phi's
//! relations to values defined before its join: a bound one edge states of its
//! input becomes the phi's own when every edge proves it for its input, as
//! `i = from < n ? from : n - 1` stays below `n`. All states start at
//! unknown; every round is sound even if the iteration budget expires. Facts
//! about definitions executed again in a loop are killed at block entry. This
//! discovers bounded induction ranges without assuming a loop executes or
//! converges, and preserves the existing dominator-scoped reasoning.
//! Only transitive inputs of branch conditions need derived range facts. The
//! analysis follows their SSA operands, including phi inputs, and ignores other
//! computations. A block is revisited only after a predecessor's exit facts
//! change, preserving the original reverse-postorder and eight-round bound.
//! After an edge consumes a single-use predicate, its own range and single-use
//! negations are discarded. Operand ranges and relations remain available;
//! predicates referenced by instructions, phis, or other terminators stay live.
//! Dead blocks are cleared before analysis, so joins use the maintained predecessor lists.
//! Fact copies clone the tables, which costs less than inserting each entry again but keeps the
//! source's spare capacity.
//!
//! Long functions compare a few values, such as constants, in every check, so those values start
//! a candidate relation per check while the scope of a query states few of them. From
//! `MIN_BLOCKS_FOR_ACTIVE_RELATIONS` blocks on, the dominator walk keeps the candidates its
//! scope states listed per value as it adds and rolls back facts: a transitive search leaves a
//! value by those alone, taking them one at a time, and the offset and strict-edge queries read
//! the same lists instead of scanning the scope after every change. A search then costs at most
//! its state budget, however many checks precede it, and visits states in the order the full
//! candidate list would.
//!
//! Loop header phis of the form `p = phi [pre: init], [latch: p - c]` with a
//! constant nonzero step are monotone when the update provably cannot wrap at
//! its definition: `p <= init` then holds on every iteration, and `p >= init`
//! for the additive form. The walk first proves wrap freedom in the update's
//! scope without assuming the invariant, then adds the relation to the header's
//! entry facts and walks again. This establishes `j <= i < length` for a
//! descending inner index initialized from a bounded outer counter.
//!
//! A header phi that its backedges move by other than a constant step can still keep to one side
//! of its value from outside the loop: a binary search's `h` starts at the length and only ever
//! takes `m - 1` for a midpoint `m <= h`. Such bounds, `p <= init` or `init <= p`, are proven
//! together by induction. Every candidate is assumed on entry to its header, and a walk checks
//! each value a backedge carries into a bounded phi in the scope of the block it leaves; a join
//! inside the loop keeps the bound when each of its incoming values does. Candidates with a
//! failing obligation are dropped and the walk repeats, for at most four rounds, until the
//! remaining bounds hold together: each holds on entry from outside its loop, and on every
//! backedge while all of them held at the headers before it. Candidates are proven before the
//! walks that decide folds, so an unproven assumption never reaches a fold. A proven bound joins
//! its header's entry facts and narrows the phi to the range of its initial value there.
//!
//! Constant offsets keep an order: `x + c < y + d` follows from `x < y` when `c <= d` and `y + d`
//! cannot wrap, as a copy's destination index against its length. Every `calldatasize` read
//! equals the first, so a bound read once covers the reads a loop makes again.
//!
//! A midpoint `m = (a + b) / 2`, or `a + (b - a) / 2`, lies between its endpoints once their order
//! is known and the sum cannot wrap: `a <= m <= b` when `a <= b`, and `m < b` when `a < b`. The
//! overflow-safe form's checked sum then cannot fail either. Checked sums and differences carry
//! exact ranges, since they fail rather than wrap, and a division by a positive constant bounds
//! its quotient from above. A range a sum `x + c` with a constant `c` takes on narrows `x` when
//! the sum cannot have wrapped, so the size checks of an allocation, `(length + 1) << 5 >> 5`
//! keeping `length + 1`, bound its length for the arithmetic that follows.
//!
//! When both the start and the step of such a phi are constants, the phi also
//! carries a range bound from the target's trip-count limit: a counter cannot
//! travel further than `step << MAX_TRIP_COUNT_BITS` because every iteration
//! burns gas that fits in 64 bits. That bound is intersected into every range
//! query for the phi, so `i + 2` and `i + 4` never wrap for a counter that
//! starts at zero even when the loop bound is an arbitrary word. The revert
//! path such a check guards is unreachable by any transaction, not merely
//! unlikely, so removing it preserves the checked semantics. A counter that
//! only some iterations advance, `if (c) ++count`, carries the same bound with
//! its largest step when every value its update can take, through phis and
//! selects, is the phi itself or the phi plus a constant; a phi met twice on
//! that walk, as an inner loop's header, gives no bound.
//!
//! Transitive relational queries lazily index candidate edges once per function,
//! when a query needs to combine facts: the conditions of branches and of semantic
//! checks, whose passing states the condition for all later execution, so
//! `assert(pos < length)` bounds the positions a loop derives from `pos`. An edge is
//! followed only when its fact is present in the current scope; the index itself
//! proves nothing. Derived values that never exceed their source (right shifts, masks,
//! remainders, divisions by a nonzero constant) contribute universal `<=` edges
//! that hold in every scope, so `i < length / 2` reaches `i < length`. A remainder is also
//! below its divisor once the divisor is above zero: always for a checked remainder, which
//! fails on zero, or a constant divisor, and otherwise in the scope of its definition when that
//! scope proves the divisor positive, as an index `r % n` drawn under `n > 1` is below `n`. A
//! checked difference fails rather than wrap, so it never exceeds its minuend and stays below
//! it for a nonzero constant subtrahend: a heap's parent `(pos - 1) >> 1` is below `pos`. A plain
//! difference by a nonzero constant stays below its minuend in the scope of its definition when
//! that scope proves the minuend at least the constant, as the parent a sift computes as
//! `pos - 1` once `pos != 0`, after check elimination made its difference plain. A value
//! with a strict path below it in the current scope is at least one, which
//! folds `length == 0` guards and the `length - 1` underflow check that follow
//! `i < length / 2`; differences inherit the strict bound of their minuend. Search follows at most
//! 128 states in stable block and operand order, carrying whether an unsigned ordering path
//! contains a strict edge. Exhausting this bound leaves the check in place; disequality is never
//! treated as transitive.
//!
//! Signed comparisons rotate intervals by the sign bit to use the same ordered
//! bounds. An interval crossing the rotation boundary widens to unknown; signed
//! facts only become unsigned relations when both operands have the same known
//! sign. These bounds use the existing join and dominance scopes.
//!
//! Runtime-only functions also use bounds from zero-extended immutable encodings.
//! These bounds follow the target's actual immediate width, not the result's
//! nominal type. Constructor-reachable functions, including helpers shared with
//! runtime code, are excluded: constructor loads read full staging words. Signed
//! and left-aligned encodings remain unknown. The bounds are immutable facts and
//! are available to both the forward analysis and the dominator walk.
//! The separate `immutable-check-elim` adapter runs after ABI getter inlining,
//! selecting only runtime functions that load bounded immutables. Before using those
//! bounds, it narrows unsigned immutable encodings when every assignment fits, using
//! the shared value-width and caller-argument proofs. Missing assignments and unknown
//! words keep their declared width. Narrow SSA loads extend back to their original
//! types; existing word-carrier loads already satisfy the integer layout contract.
//! This exposes facts hidden behind getter calls during the ordinary earlier check passes.
//!
//! The `late-check-elim` adapter revisits conditions unified by CSE after memory
//! lowering. Gas mode only removes redundant failure edges from blocks on a CFG
//! cycle: repeated checks repay their removal each iteration, while other changes can disrupt
//! shared ABI encoder tails and increase both size and gas. Size mode uses the
//! full cleanup. Cycle membership is only a profitability filter. Gas mode uses
//! dominator-scoped facts, sufficient for conditions unified by CSE, and leaves
//! fixed-point range propagation to the earlier check passes. Size mode retains
//! the full forward analysis. Both use the existing conservative proof logic.
//! Integer cleanup reuses the dominator walk after integer legalization to remove masks
//! whose inputs already fit, without running the forward fixed-point analysis. A word
//! widened back from its own nonzero test, `zext(v != 0)`, counts as a one-bit mask, so a
//! boolean that ABI decoding checked keeps its word instead of normalizing it again. A high
//! mask `and x, MAX << low`, which ranges cannot express, is redundant where some `shl x, k`
//! with `256 - k >= low` has range `{0}`: that `bytesN` guard proves the low bits zero. Only
//! functions with a high mask on a value they also shift enter the pass for this rule, since
//! equality propagation makes the code of other functions worse.
//! It also uses runtime immutable bounds, since legalization can introduce masks after the
//! earlier immutable pass. Constructor-reachable functions retain their full-word semantics.
//! Run late check elimination after the post-memory CSE. Only functions with removed checks receive
//! CFG cleanup, avoiding unrelated late block merges in other functions.

use super::{
    call_cleanup,
    cfg_simplify::simplify_function,
    egraph::{max_bits_with_args, use_counts},
};
use crate::{
    backend::evm::op,
    mir::{
        ArithmeticKind, BitWidthTest, BlockId, Builtin, Callee, CheckedOp, EffectKind, Function,
        FunctionBuilder, FunctionId, ImmutableEncoding, ImmutableId, InstId, InstKind, MirType,
        Module, Terminator, TypeSize, Value, ValueId, ValueLayout,
        analysis::{
            CfgInfo, aborts, cold_functions,
            integers::{integer_bits, integer_mask, integer_max},
        },
        immutable::immutable_push_type_size,
        pass::{
            FunctionAnalyses, MirPass, run_function_pass, run_function_pass_with_cfg,
            run_selected_function_pass,
        },
        utils::{IndexLists, eval, fold_terminator_to_jump, invalidate_unreachable_blocks},
    },
    target::Target,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet, FxIndexMap, FxIndexSet},
};
use std::ops::Deref;

/// Function pass for range-based overflow-check elimination.
pub(crate) struct CheckElim;

impl MirPass for CheckElim {
    fn name(&self) -> &'static str {
        "check-elim"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            let eliminator = CheckEliminator {
                propagate_equalities: Some(analyses.runtime_target(gcx)),
                ..Default::default()
            };
            let cfg = CfgInfo::new(func);
            eliminator.run(func, &cfg, None) != 0
        })
    }
}

/// Removes integer masks and boolean renormalizations proved redundant by branches or
/// runtime immutable widths.
pub(crate) struct IntegerCleanup;

impl MirPass for IntegerCleanup {
    fn name(&self) -> &'static str {
        "integer-cleanup"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let bounds = runtime_immutable_bounds(module, gcx);
        let runtime = (!bounds.is_empty()).then(|| runtime_only_functions(module));
        let cleanup = |func: &mut Function, analyses: &FunctionAnalyses| {
            let immutable_ranges = runtime
                .as_ref()
                .is_some_and(|runtime| runtime.contains(analyses.function()))
                .then_some(&bounds);
            let target = analyses.runtime_target(gcx);
            if immutable_ranges.is_none()
                && !func
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(Terminator::Branch { .. })))
            {
                return false;
            }
            let shifted_high_masks = shifted_high_mask_inputs(func);
            if shifted_high_masks.is_empty()
                && !func.instructions().any(|id| {
                    let kind = &func.inst(id).kind;
                    low_mask_input(func, kind).is_some()
                        || widened_nonzero_test(func, kind).is_some()
                        || compares_with_immediate(func, kind, &target)
                })
            {
                return false;
            }
            let cfg = CfgInfo::new(func);
            let removed = invalidate_unreachable_blocks(func, cfg.reachable());
            let mut eliminator = CheckEliminator {
                collect_masks: true,
                shifted_high_masks,
                propagate_equalities: Some(target),
                immutable_ranges,
                ..Default::default()
            };
            let facts = index_vec![Facts::default(); func.blocks.len()];
            let _ = eliminator.collect_folds(func, &cfg, &facts, &[], &mut Vec::new(), None);
            let propagated = eliminator.apply_equalities(func) != 0;
            if eliminator.redundant_masks.is_empty() {
                return removed != 0 || propagated;
            }
            let mut replacements = FxHashMap::default();
            let mut dead = DenseBitSet::new_empty(func.num_insts());
            for &(id, value) in &eliminator.redundant_masks {
                replacements.insert(func.inst_result_value(id).unwrap(), value);
                dead.insert(id);
            }
            func.replace_uses_canonicalized(&replacements);
            for block in &mut func.blocks {
                block.instructions.retain(|&id| !dead.contains(id));
            }
            true
        };
        // Only the immutable bound case has ever requested the CFG snapshot, which decides what
        // survives an edit in the analysis caches.
        if runtime.is_some() {
            run_function_pass_with_cfg(module, analyses, cleanup)
        } else {
            run_function_pass(module, analyses, cleanup)
        }
    }
}

/// Revisits checks exposed by physical memory lowering and CSE.
pub(crate) struct LateCheckElim;

impl MirPass for LateCheckElim {
    fn name(&self) -> &'static str {
        "late-check-elim"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let cold = cold_functions(module);
        run_function_pass_with_cfg(module, analyses, |func, analyses| {
            let gas = analyses.runtime_target(gcx).optimization().is_gas();
            let selected = gas.then(|| analyses.cfg().cyclic_blocks());
            if selected.is_some_and(DenseBitSet::is_empty) {
                return false;
            }
            // Only folds of selected branches with a reverting arm are kept, and `lower-checks`
            // has already expanded every check, so without such a branch the elimination
            // cannot change anything.
            if let Some(selected) = selected
                && !selected.iter().any(|block| {
                    let Some(Terminator::Branch { then_block, else_block, .. }) =
                        func.blocks[block].terminator
                    else {
                        return false;
                    };
                    leads_to_revert(func, then_block, &cold)
                        || leads_to_revert(func, else_block, &cold)
                })
            {
                return false;
            }
            let changed = CheckEliminator::default().run(
                func,
                analyses.cfg(),
                selected.map(|blocks| (blocks, &cold)),
            ) != 0;
            if changed {
                // branch proven_condition, checked, panic => jump checked
                // Remove unreachable panic blocks and merge the successful continuation.
                let _ = simplify_function(func);
            }
            changed
        })
    }
}

/// Recognizes short unconditional failure paths: a chain of jumps to a block that aborts,
/// including through outlined revert helpers and other functions that never return.
/// This only selects profitable candidates; range analysis proves the edge unreachable.
fn leads_to_revert(func: &Function, mut block: BlockId, cold: &DenseBitSet<FunctionId>) -> bool {
    // Bound classification work and leave cycles or longer paths unclassified.
    for _ in 0..8 {
        if aborts(func, block, cold) {
            return true;
        }
        let Some(Terminator::Jump(next)) = func.blocks[block].terminator else { return false };
        block = next;
    }
    false
}

/// Applies runtime immutable bounds after getter inlining exposes their loads.
pub(crate) struct ImmutableCheckElim;

impl MirPass for ImmutableCheckElim {
    fn name(&self) -> &'static str {
        "immutable-check-elim"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let narrowed = narrow_immutable_layouts(module);
        let bounds = runtime_immutable_bounds(module, gcx);
        if bounds.is_empty() {
            return narrowed;
        }
        let mut loading = DenseBitSet::new_empty(module.functions.len());
        for id in runtime_only_functions(module).iter() {
            let func = module.function(id);
            if func.instructions().any(|inst| {
                matches!(func.inst(inst).kind,
                    InstKind::LoadImmutable(immutable) if bounds.contains_key(&immutable))
            }) {
                loading.insert(id);
            }
        }
        run_selected_function_pass(module, analyses, &loading, |func, analyses| {
            let eliminator =
                CheckEliminator { immutable_ranges: Some(&bounds), ..Default::default() };
            eliminator.run(func, analyses.cfg(), None) != 0
        }) || narrowed
    }
}

/// Shrinks unsigned encodings only when every assignment preserves all stored bits.
/// Narrow loads extend back to their original result types so existing uses remain well-typed.
fn narrow_immutable_layouts(module: &mut Module) -> bool {
    let can_narrow = |ty| {
        matches!(ty, ValueLayout::Address)
            || matches!(ty, ValueLayout::UInt(size) if size.bits() > 8)
    };
    if !module.iter_immutables().any(|(_, immutable)| can_narrow(immutable.ty)) {
        return false;
    }
    let arguments = call_cleanup::infer_arguments(module);
    let mut widths = FxHashMap::<_, u32>::default();
    for (id, func) in module.functions.iter_enumerated() {
        for inst in func.instructions() {
            if let InstKind::StoreImmutable(immutable, value) = func.inst(inst).kind
                && can_narrow(module.immutable(immutable).ty)
            {
                let bits = max_bits_with_args(func, value, 8, &|index| {
                    call_cleanup::argument_bits(func, id, index, &arguments)
                });
                widths
                    .entry(immutable)
                    .and_modify(|width| *width = (*width).max(bits))
                    .or_insert(bits);
            }
        }
    }
    let mut types = FxHashMap::default();
    for (id, bits) in widths {
        let bits = bits.max(1).div_ceil(8) * 8;
        if let Some(size) = module.immutable(id).ty.type_size()
            && bits < u32::from(size.bits())
        {
            module.immutable_mut(id).ty = ValueLayout::UInt(TypeSize::new_int_bits(bits as u16));
            types.insert(id, module.immutable(id).ty.mir_type());
        }
    }
    if !types.is_empty() {
        for func in &mut module.functions {
            for block in func.blocks.indices() {
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                for inst in instructions {
                    match builder.func().inst(inst).kind {
                        InstKind::StoreImmutable(id, value) if types.contains_key(&id) => {
                            builder.set_debug_context(&builder.func().inst(inst).metadata.clone());
                            let value = builder.cast(value, types[&id]);
                            builder
                                .func_mut()
                                .inst_mut(inst)
                                .replace_kind(InstKind::StoreImmutable(id, value));
                        }
                        InstKind::LoadImmutable(id)
                            if types.contains_key(&id)
                                && builder.func().inst(inst).result_ty != Some(MirType::I256) =>
                        {
                            builder.set_debug_context(&builder.func().inst(inst).metadata.clone());
                            let value = builder.load_immutable(id, types[&id]);
                            builder.func_mut().inst_mut(inst).replace_kind(InstKind::Zext(value));
                        }
                        _ => {}
                    }
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
            }
        }
    }
    !types.is_empty()
}

/// Excludes every constructor-reachable helper, including recursive and tail-call edges.
fn runtime_only_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let mut runtime = module.runtime_reachable_functions();
    runtime.subtract(&module.constructor_reachable_functions());
    runtime
}

fn runtime_immutable_bounds(
    module: &Module,
    gcx: solar_sema::Gcx<'_>,
) -> FxHashMap<ImmutableId, Range> {
    module
        .iter_immutables()
        .filter_map(|(id, immutable)| {
            let encoding @ ImmutableEncoding::Unsigned(_) = immutable.ty.immutable_encoding()?
            else {
                return None;
            };
            let width = immutable_push_type_size(
                encoding,
                module.optimization,
                gcx.sess.opts.evm_version.has_bitwise_shifting(),
            )
            .bits();
            (width < 256).then(|| (id, Range::new(U256::ZERO, U256::MAX >> (256 - width))))
        })
        .collect()
}

/// Maximum recursion depth when evaluating value ranges and conditions.
const MAX_DEPTH: usize = 12;

/// Functions with this many blocks list the relations their scope states during the dominator
/// walk. Smaller ones have too few candidates per value to repay the lists.
const MIN_BLOCKS_FOR_ACTIVE_RELATIONS: usize = 128;

/// An inclusive unsigned 256-bit interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    lo: U256,
    hi: U256,
}

impl Range {
    const FULL: Self = Self { lo: U256::ZERO, hi: U256::MAX };

    const fn new(lo: U256, hi: U256) -> Self {
        Self { lo, hi }
    }

    fn singleton(value: U256) -> Self {
        Self { lo: value, hi: value }
    }

    fn is_singleton(self) -> bool {
        self.lo == self.hi
    }

    /// Intersects two ranges. Returns `None` when the intersection is empty,
    /// which means the current program point is dynamically unreachable.
    fn intersect(self, other: Self) -> Option<Self> {
        let lo = self.lo.max(other.lo);
        let hi = self.hi.min(other.hi);
        (lo <= hi).then_some(Self { lo, hi })
    }

    /// Whether the range is all zero or all nonzero.
    fn truth(self) -> Option<bool> {
        if self.lo > U256::ZERO {
            Some(true)
        } else if self.hi.is_zero() {
            Some(false)
        } else {
            None
        }
    }

    /// The sign bit of every value in the range, when they share it.
    fn sign(self, bits: u32) -> Option<bool> {
        let sign = self.lo.bit((bits - 1) as usize);
        (sign == self.hi.bit((bits - 1) as usize)).then_some(sign)
    }

    /// Whether the values of both ranges share one known sign bit.
    fn same_sign(self, other: Self, bits: u32) -> bool {
        let sign = self.sign(bits);
        sign.is_some() && sign == other.sign(bits)
    }

    /// Rotates unsigned order into signed order, or back, widening a split interval.
    fn flip_sign(self, bits: u32) -> Self {
        if self.sign(bits).is_some() {
            let sign = U256::from(1) << (bits - 1);
            Self::new(self.lo ^ sign, self.hi ^ sign)
        } else {
            Self::new(U256::ZERO, U256::MAX >> (256 - bits))
        }
    }

    fn union(self, other: Self) -> Self {
        Self { lo: self.lo.min(other.lo), hi: self.hi.max(other.hi) }
    }
}

/// Differences indexed under their subtrahend, each entry a minuend and the
/// value holding their difference.
type DifferenceIndex = FxHashMap<ValueId, SmallVec<[(ValueId, ValueId); 2]>>;

/// A relational predicate between two SSA values.
///
/// `Lt(a, b)` means `a < b` and `Le(a, b)` means `a <= b`, both unsigned.
/// `Eq` and `Ne` are stored with operands ordered by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Relation {
    Lt(ValueId, ValueId),
    Le(ValueId, ValueId),
    Eq(ValueId, ValueId),
    Ne(ValueId, ValueId),
}

impl Relation {
    fn operands(self) -> (ValueId, ValueId) {
        match self {
            Self::Lt(a, b) | Self::Le(a, b) | Self::Eq(a, b) | Self::Ne(a, b) => (a, b),
        }
    }

    /// The value a search at `value` reaches through this relation, and whether the path is
    /// strict from there: equality goes both ways, `<=` carries order, `<` makes the path
    /// strict, and disequality leads nowhere.
    fn step(self, value: ValueId, strict: bool, equality_only: bool) -> Option<(ValueId, bool)> {
        match self {
            Self::Eq(a, b) if a == value => Some((b, strict)),
            Self::Eq(a, b) if b == value => Some((a, strict)),
            Self::Le(a, b) if a == value && !equality_only => Some((b, strict)),
            Self::Lt(a, b) if a == value && !equality_only => Some((b, true)),
            _ => None,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Facts {
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
}

/// Candidate relations listed under the values a search leaves from, each once and in the order
/// they were added: under its left operand, and under both for an equality.
#[derive(Default)]
struct RelationIndex {
    lists: FxHashMap<ValueId, SmallVec<[Relation; 2]>>,
    listed: FxHashSet<Relation>,
}

impl RelationIndex {
    fn insert(&mut self, relation: Relation) {
        if !self.listed.insert(relation) {
            return;
        }
        let (a, b) = relation.operands();
        self.lists.entry(a).or_default().push(relation);
        if matches!(relation, Relation::Eq(..)) && a != b {
            self.lists.entry(b).or_default().push(relation);
        }
    }
}

impl Deref for RelationIndex {
    type Target = FxHashMap<ValueId, SmallVec<[Relation; 2]>>;

    fn deref(&self) -> &Self::Target {
        &self.lists
    }
}

/// The relations of the current scope that searches walk, kept in step with `relations` by the
/// dominator walk.
///
/// A constant compared by every check of a long function is one endpoint of a candidate relation
/// per check, but a query's scope states few of them. Listing only those spares a search a
/// membership test of every candidate, and keeps the order the relation index lists them in.
/// The same lists answer the two queries that would otherwise scan the scope again after every
/// change to it: the offsets a bound holds for, and the values above a strict edge, which only
/// grow as relations join the scope and roll back with them.
struct ActiveRelations {
    /// Positions of a relation in the index lists of its left and right operand.
    slots: FxHashMap<Relation, [u32; 2]>,
    /// Positions in each value's index list of its relations in scope, ascending.
    in_scope: FxHashMap<ValueId, SmallVec<[u32; 4]>>,
    /// Constants `c` such that `base + c < bound` is in scope, by `(bound, base)`.
    offset_bounds: FxHashMap<(ValueId, ValueId), SmallVec<[U256; 2]>>,
    /// Values above a strict edge, through the relations in scope.
    nonzero: GrowableBitSet<ValueId>,
    /// The values added to `nonzero`, in order.
    nonzero_trail: Vec<ValueId>,
    /// The length of `nonzero_trail` before each relation added to the scope.
    marks: Vec<usize>,
}

impl ActiveRelations {
    const ABSENT: u32 = u32::MAX;

    /// Creates empty lists over the candidates of `index`.
    fn new(func: &Function, index: &RelationIndex) -> Self {
        let mut slots = FxHashMap::<Relation, [u32; 2]>::default();
        for (&value, relations) in index.iter() {
            for (position, &relation) in relations.iter().enumerate() {
                let slot = slots.entry(relation).or_insert([Self::ABSENT; 2]);
                slot[usize::from(relation.operands().0 != value)] = position as u32;
            }
        }
        Self {
            slots,
            in_scope: FxHashMap::default(),
            offset_bounds: FxHashMap::default(),
            nonzero: GrowableBitSet::with_capacity(func.num_values()),
            nonzero_trail: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Lists `relation` for searches, and for the offset queries when it is strict, unless the
    /// index lacks it or it is already listed.
    fn link(&mut self, func: &Function, relation: Relation) {
        let Some(&slot) = self.slots.get(&relation) else { return };
        let (a, b) = relation.operands();
        let mut added = false;
        for (value, position) in [(a, slot[0]), (b, slot[1])] {
            if position == Self::ABSENT {
                continue;
            }
            let positions = self.in_scope.entry(value).or_default();
            if let Err(at) = positions.binary_search(&position) {
                positions.insert(at, position);
                added = true;
            }
        }
        if added && matches!(relation, Relation::Lt(..)) {
            for (base, amount) in offsets_of(func, a) {
                self.offset_bounds.entry((b, base)).or_default().push(amount);
            }
        }
    }

    /// Takes `relation` out of the lists `link` put it in.
    fn unlink(&mut self, func: &Function, relation: Relation) {
        let Some(&slot) = self.slots.get(&relation) else { return };
        let (a, b) = relation.operands();
        let mut removed = false;
        for (value, position) in [(a, slot[0]), (b, slot[1])] {
            if position != Self::ABSENT
                && let Some(positions) = self.in_scope.get_mut(&value)
                && let Ok(at) = positions.binary_search(&position)
            {
                positions.remove(at);
                removed = true;
            }
        }
        if removed && matches!(relation, Relation::Lt(..)) {
            for (base, amount) in offsets_of(func, a) {
                if let Some(amounts) = self.offset_bounds.get_mut(&(b, base))
                    && let Some(at) = amounts.iter().rposition(|&other| other == amount)
                {
                    amounts.swap_remove(at);
                }
            }
        }
    }

    /// Adds `relation` to the scope.
    fn add(&mut self, func: &Function, index: &RelationIndex, relation: Relation) {
        self.marks.push(self.nonzero_trail.len());
        let linked = self.slots.contains_key(&relation);
        self.link(func, relation);
        let mut pending = SmallVec::<[ValueId; 8]>::new();
        // A strict edge makes its upper endpoint nonzero, and an edge carries nonzero upward.
        if let Relation::Lt(_, bound) = relation {
            self.reach(&mut pending, bound);
        }
        if linked {
            match relation {
                Relation::Lt(a, b) | Relation::Le(a, b) if self.nonzero.contains(a) => {
                    self.reach(&mut pending, b);
                }
                Relation::Eq(a, b) if self.nonzero.contains(a) => self.reach(&mut pending, b),
                Relation::Eq(a, b) if self.nonzero.contains(b) => self.reach(&mut pending, a),
                _ => {}
            }
        }
        while let Some(current) = pending.pop() {
            let Some(positions) = self.in_scope.get(&current) else { continue };
            let list = &index[&current];
            let next = positions
                .iter()
                .filter_map(|&position| list[position as usize].step(current, false, false))
                .collect::<SmallVec<[_; 4]>>();
            for (value, _) in next {
                self.reach(&mut pending, value);
            }
        }
    }

    /// Marks `value` nonzero, and queues it to carry that upward, unless it already is.
    fn reach(&mut self, pending: &mut SmallVec<[ValueId; 8]>, value: ValueId) {
        if self.nonzero.insert(value) {
            self.nonzero_trail.push(value);
            pending.push(value);
        }
    }

    /// Removes the relation most recently added to the scope, which leaves unless `permanent`.
    fn undo(&mut self, func: &Function, relation: Relation, permanent: bool) {
        if let Some(mark) = self.marks.pop() {
            while self.nonzero_trail.len() > mark {
                let value = self.nonzero_trail.pop().expect("checked len");
                self.nonzero.remove(value);
            }
        }
        if !permanent {
            self.unlink(func, relation);
        }
    }
}

/// The bases and constants `value` adds: `x + c` and `c + x` for a constant `c`.
fn offsets_of(func: &Function, value: ValueId) -> SmallVec<[(ValueId, U256); 2]> {
    let Some(&InstKind::Add(first, second)) = inst_kind(func, value) else {
        return SmallVec::new();
    };
    [(first, second), (second, first)]
        .into_iter()
        .filter_map(|(base, constant)| Some((base, func.value_u256(constant)?)))
        .collect()
}

fn ordered(a: ValueId, b: ValueId) -> (ValueId, ValueId) {
    if a.index() <= b.index() { (a, b) } else { (b, a) }
}

/// A loop header phi whose backedge update moves away from its initial value
/// by a constant step, pending a proof that the update cannot wrap.
#[derive(Clone, Copy, Debug)]
struct MonotonePhi {
    /// The loop header holding the phi.
    header: BlockId,
    /// The phi result.
    value: ValueId,
    /// The incoming value from outside the loop.
    initial: ValueId,
    /// The backedge update result, `value - step` or `value + step`.
    next: ValueId,
    /// The constant step operand.
    step: ValueId,
    /// The block defining `next`, whose facts decide wrap freedom.
    home: BlockId,
    /// Whether the update subtracts the step.
    decreasing: bool,
}

impl MonotonePhi {
    /// The invariant that holds once the update is known not to wrap.
    fn relation(&self) -> Relation {
        if self.decreasing {
            Relation::Le(self.value, self.initial)
        } else {
            Relation::Le(self.initial, self.value)
        }
    }
}

/// A loop header phi held on one side of a value defined before its loop, `p <= limit` when
/// `upper` and `limit <= p` otherwise: its value from outside the loop, or a value the loop
/// compares it with. The bound holds on every iteration when the value entering from outside and
/// every value the backedges carry into the phi keep it.
#[derive(Clone, Copy, Debug)]
struct HeaderBound {
    /// The loop header holding the phi.
    header: BlockId,
    /// The phi result.
    value: ValueId,
    /// The value bounding the phi, defined before the loop.
    limit: ValueId,
    /// Whether the phi stays at or below `limit`, rather than at or above it.
    upper: bool,
}

impl HeaderBound {
    /// The invariant the bound states.
    fn relation(&self) -> Relation {
        if self.upper {
            Relation::Le(self.value, self.limit)
        } else {
            Relation::Le(self.limit, self.value)
        }
    }
}

/// A value entering a bounded phi, which must keep the bound in the scope at the end of `block`,
/// where control leaves for the header or for a join on the way to it.
#[derive(Clone, Copy, Debug)]
struct BoundObligation {
    /// The index of the bound among the bounds under proof.
    bound: usize,
    block: BlockId,
    value: ValueId,
}

/// A midpoint `value = (low + high) / 2`, or `low + (high - low) / 2`, which lies between its
/// endpoints once their order is known.
#[derive(Clone, Copy, Debug)]
struct Midpoint {
    value: ValueId,
    low: ValueId,
    high: ValueId,
    /// The sum `low + high` of the first form, which must not wrap. The second form needs only
    /// `low <= high`, which keeps its difference and its sum exact.
    sum: Option<ValueId>,
}

/// Branches to fold to one target, passing checks to remove, and branch conditions to reduce to
/// one operand, as a dominator walk decides them.
type Folds = (Vec<(BlockId, BlockId)>, DenseBitSet<InstId>, Vec<(BlockId, ValueId)>);

/// Operands of one instruction and the constants they equal.
type Replacements = SmallVec<[(ValueId, ValueId); 2]>;

/// A value proven equal to an immediate by the edge into `block`.
struct Equality {
    constant: ValueId,
    block: BlockId,
    /// Whether at least two instructions below `block` read the value, once asked.
    repeated: Option<bool>,
}

/// Range-based overflow-check eliminator.
#[derive(Default)]
struct CheckEliminator<'a> {
    /// Context-independent bounds for runtime-only immutable loads.
    immutable_ranges: Option<&'a FxHashMap<ImmutableId, Range>>,
    collect_masks: bool,
    redundant_masks: Vec<(InstId, ValueId)>,
    /// The constant left shifts of each value a high mask cleans, as `(shift, result)`.
    shifted_high_masks: FxHashMap<ValueId, SmallVec<[(usize, ValueId); 2]>>,
    /// Records constants that dominating branches prove equal to a value, when the target
    /// prices the constant as cheap to read.
    propagate_equalities: Option<Target>,
    /// Values proven equal to a constant in the current scope.
    equalities: FxHashMap<ValueId, Equality>,
    equality_undo: Vec<ValueId>,
    /// The block whose entry or checks the walk is recording facts for.
    scope: Option<BlockId>,
    /// Blocks of the instructions reading each value, built on the first query.
    readers: Option<IndexLists<ValueId, BlockId>>,
    /// Instructions that read an equal value, with the operands that become its constant.
    equality_uses: Vec<(InstId, Replacements)>,
    /// Semantic checked operations proven not to fail, with the plain operation each becomes.
    unfailing: Vec<(InstId, InstKind)>,
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
    /// Possible outgoing facts; each candidate still requires a scoped membership check.
    relation_index: Option<RelationIndex>,
    /// Whether the dominator walk is running, which keeps `active_relations` in step with
    /// `relations`.
    scoped: bool,
    /// The indexed candidates in scope, built on the first search of a walk. Boxed: most
    /// functions never build it, and inline it would widen every access to the fields around it.
    active_relations: Option<Box<ActiveRelations>>,
    /// Proven header phi invariants, indexed alongside the branch-derived candidates.
    monotone_relations: Vec<Relation>,
    /// Relations of join phis that every incoming value satisfies, indexed the same way.
    phi_relations: Vec<Relation>,
    /// Orderings between a derived value and its source that hold wherever the
    /// value exists: shifts, masks, and constant divisions never grow.
    universal_relations: FxHashSet<Relation>,
    /// Relations indexed under their right operand, for lower-bound searches.
    reverse_index: Option<FxHashMap<ValueId, SmallVec<[Relation; 2]>>>,
    /// Differences indexed under their subtrahend: `b` maps to every `(a, a - b)`.
    difference_index: Option<DifferenceIndex>,
    /// Depth of the sum-bound lemma, which asks relational questions of its own.
    sum_depth: usize,
    /// Values above a strict edge, shared by queries in the same fact scope, once
    /// `strict_bounds_valid`.
    strict_lower_bounds: GrowableBitSet<ValueId>,
    strict_bounds_valid: bool,
    /// Scratch for building `strict_lower_bounds`.
    strict_pending: Vec<ValueId>,
    /// Counting header phis with constant start and step, bounded by the
    /// distance any affordable number of iterations can travel.
    trip_bounds: FxHashMap<ValueId, Range>,
    range_undo: Vec<(ValueId, Option<Range>)>,
    relation_undo: Vec<Relation>,
    /// Scratch states for relation searches, reused across queries.
    relation_seen: FxHashSet<(ValueId, bool)>,
    /// Header bounds assumed on entry to their headers: the bounds under proof, or the proven
    /// ones in the final walks.
    header_bounds: Vec<HeaderBound>,
    /// What the bounds under proof require of the values their backedges carry, by block.
    obligations: Vec<BoundObligation>,
    /// The bounds under proof with an obligation the last walk could not prove.
    failed_bounds: FxHashSet<usize>,
    /// Midpoints feeding branches, by the instruction defining them.
    midpoints: FxIndexMap<InstId, Midpoint>,
}

impl<'a> CheckEliminator<'a> {
    /// Runs check elimination on a function, rewriting only the selected blocks when given and
    /// retaining all facts needed to prove their checks. Returns the number of rewrites.
    fn run(
        mut self,
        func: &mut Function,
        cfg: &CfgInfo,
        selected: Option<(&DenseBitSet<BlockId>, &DenseBitSet<FunctionId>)>,
    ) -> usize {
        if !func.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(Terminator::Branch { then_block, else_block, .. }) if then_block != else_block
            ) || block.instructions.iter().any(|&inst| {
                matches!(
                    func.inst(inst).kind,
                    InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), .. }
                ) || checked_operands(&func.inst(inst).kind).is_some()
            })
        }) {
            return 0;
        }
        let removed = invalidate_unreachable_blocks(func, cfg.reachable());
        let relevant = branch_inputs(func, cfg);
        if relevant.is_empty() {
            return removed;
        }
        let definitions = func.inst_block_table();
        self.universal_relations = universal_relations(func, &relevant);
        self.midpoints = midpoints(func, &relevant);

        const MAX_ACYCLIC_JOIN_INSTRUCTIONS: usize = 128;
        let bounded_acyclic_join = cfg.cyclic_blocks().is_empty()
            && func.instructions().take(MAX_ACYCLIC_JOIN_INSTRUCTIONS + 1).count()
                > MAX_ACYCLIC_JOIN_INSTRUCTIONS;
        let mut facts = if selected.is_some() || bounded_acyclic_join {
            // Bound the additional search in large acyclic functions, where
            // copying fact sets through long chains is quadratic; the ordinary
            // dominator proof still runs. Cyclic functions retain fixed-point
            // propagation for induction ranges.
            index_vec![Facts::default(); func.blocks.len()]
        } else {
            self.join_facts(func, cfg, &definitions, &relevant)
        };
        let phis = loop_phis(func, cfg, &relevant);
        let candidates = monotone_phi_candidates(func, &definitions, &phis);
        for phi in &candidates {
            if let Some(bound) = trip_count_bound(func, phi) {
                self.trip_bounds.insert(phi.value, bound);
            }
        }
        for (value, bound) in counter_bounds(func, &phis) {
            self.trip_bounds.entry(value).or_insert(bound);
        }
        let bounds = header_bound_candidates(func, cfg, &definitions, &relevant, &candidates);
        self.prove_header_bounds(func, cfg, &definitions, bounds, &mut facts, selected);
        let mut proven = Vec::new();
        let (mut folds, mut checks, mut reductions) =
            self.collect_folds(func, cfg, &facts, &candidates, &mut proven, selected);
        if !proven.is_empty() {
            // The invariant is available wherever the phi is: attach it to the
            // header's entry facts and index it for transitive queries.
            for phi in &proven {
                facts[phi.header].relations.insert(phi.relation());
                if let Some(initial) = func.value_u256(phi.initial) {
                    let bound = if phi.decreasing {
                        Range::new(U256::ZERO, initial)
                    } else {
                        Range::new(initial, U256::MAX)
                    };
                    let ranges = &mut facts[phi.header].ranges;
                    let narrowed = ranges
                        .get(&phi.value)
                        .map_or(bound, |range| range.intersect(bound).unwrap_or(*range));
                    ranges.insert(phi.value, narrowed);
                }
                self.monotone_relations.push(phi.relation());
            }
            self.invalidate_indexes();
            (folds, checks, reductions) =
                self.collect_folds(func, cfg, &facts, &[], &mut Vec::new(), selected);
        }
        self.ranges.clear();
        self.relations.clear();
        self.range_undo.clear();
        self.relation_undo.clear();

        let unfailing = std::mem::take(&mut self.unfailing);
        if folds.is_empty()
            && checks.is_empty()
            && reductions.is_empty()
            && unfailing.is_empty()
            && self.equality_uses.is_empty()
        {
            return removed;
        }
        // checked_op a, b that cannot fail -> op a, b
        for (id, kind) in &unfailing {
            func.inst_mut(*id).replace_kind(kind.clone());
        }
        let propagated = self.apply_equalities(func);
        // branch proven_condition, keep, discard => jump keep
        for &(block, keep) in &folds {
            // jumpi condition, ..., keep -> jump keep
            fold_terminator_to_jump(func, block, keep);
        }
        if !checks.is_empty() {
            // check a proven passing condition -> nothing
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !checks.contains(id));
            }
        }
        // jumpi (or c, 0) | (and c, 1), then, else -> jumpi c, then, else
        for &(block, reduced) in &reductions {
            if let Some(Terminator::Branch { condition, .. }) = &mut func.blocks[block].terminator {
                *condition = reduced;
            }
        }
        removed + folds.len() + checks.count() + unfailing.len() + reductions.len() + propagated
    }

    /// Walks the dominator tree, recording edge and check facts. Returns branch folds, proven
    /// passing checks to remove, and branch conditions to reduce to one of their operands.
    /// `candidates` whose update is proven wrap-free in its defining block's
    /// scope are appended to `proven`. With `selected`, only folds of selected branches whose
    /// discarded arm reverts are evaluated and returned; evaluation records no facts.
    #[allow(clippy::too_many_arguments)]
    fn collect_folds(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        facts: &IndexVec<BlockId, Facts>,
        candidates: &[MonotonePhi],
        proven: &mut Vec<MonotonePhi>,
        selected: Option<(&DenseBitSet<BlockId>, &DenseBitSet<FunctionId>)>,
    ) -> Folds {
        enum Walk {
            Enter(BlockId),
            Exit { range_mark: usize, relation_mark: usize, equality_mark: usize },
        }

        let mut folds = Vec::new();
        let mut checks = DenseBitSet::new_empty(func.num_insts());
        let mut reductions = Vec::new();
        self.unfailing.clear();
        self.equality_uses.clear();
        self.readers = None;
        let mut stack = Vec::new();
        stack.push(Walk::Enter(BlockId::ENTRY));
        self.scoped = func.blocks.len() >= MIN_BLOCKS_FOR_ACTIVE_RELATIONS;
        self.active_relations = None;
        while let Some(item) = stack.pop() {
            match item {
                Walk::Exit { range_mark, relation_mark, equality_mark } => {
                    while self.equality_undo.len() > equality_mark {
                        let value = self.equality_undo.pop().expect("checked len");
                        self.equalities.remove(&value);
                    }
                    while self.range_undo.len() > range_mark {
                        let (value, old) = self.range_undo.pop().expect("checked len");
                        match old {
                            Some(range) => self.ranges.insert(value, range),
                            None => self.ranges.remove(&value),
                        };
                    }
                    if self.relation_undo.len() > relation_mark {
                        self.strict_bounds_valid = false;
                    }
                    while self.relation_undo.len() > relation_mark {
                        let relation = self.relation_undo.pop().expect("checked len");
                        self.relations.remove(&relation);
                        if let Some(active) = &mut self.active_relations {
                            let permanent = self.universal_relations.contains(&relation);
                            active.undo(func, relation, permanent);
                        }
                    }
                }
                Walk::Enter(block) => {
                    stack.push(Walk::Exit {
                        range_mark: self.range_undo.len(),
                        relation_mark: self.relation_undo.len(),
                        equality_mark: self.equality_undo.len(),
                    });

                    self.scope = Some(block);
                    for (&value, &range) in &facts[block].ranges {
                        self.narrow(value, range);
                    }
                    for &relation in &facts[block].relations {
                        self.add_relation(func, relation);
                    }
                    if let Some((condition, is_true)) = dominating_edge_fact(func, block) {
                        self.assume(func, condition, is_true, MAX_DEPTH);
                    }
                    for index in 0..self.header_bounds.len() {
                        let bound = self.header_bounds[index];
                        if bound.header == block {
                            // p <= limit <= hi(limit), or lo(limit) <= limit <= p
                            let limit = self.range_of(func, bound.limit, MAX_DEPTH);
                            let range = if bound.upper {
                                Range::new(U256::ZERO, limit.hi)
                            } else {
                                Range::new(limit.lo, U256::MAX)
                            };
                            self.narrow(bound.value, range);
                        }
                    }
                    for candidate in candidates.iter().filter(|candidate| candidate.home == block) {
                        if self.update_cannot_wrap(func, candidate) {
                            proven.push(*candidate);
                        }
                    }

                    for &id in &func.blocks[block].instructions {
                        if !self.equalities.is_empty() {
                            self.propagate_equalities_into(func, cfg, id);
                        }
                        // `low + (high - low) / 2` stays at most `high`, so its checked sum
                        // cannot fail once `low <= high`.
                        let mut unfailing_midpoint = None;
                        if let Some(&midpoint) = self.midpoints.get(&id)
                            && self.assume_midpoint(func, midpoint)
                            && midpoint.sum.is_none()
                            && let InstKind::CheckedBinary { op: CheckedOp::Add, lhs, rhs, .. } =
                                func.inst(id).kind
                        {
                            unfailing_midpoint = Some(InstKind::Add(lhs, rhs));
                        }
                        if self.collect_masks
                            && let Some(value) = self.redundant_mask(func, id)
                        {
                            self.redundant_masks.push((id, value));
                        }
                        // x % y < y wherever the definition's scope proves y > 0
                        if let InstKind::Mod(_, divisor) = func.inst(id).kind
                            && let Some(value) = func.inst_result_value(id)
                            && self.range_of(func, divisor, MAX_DEPTH).lo > U256::ZERO
                        {
                            self.add_relation(func, Relation::Lt(value, divisor));
                        }
                        // x - k < x wherever the definition's scope proves x >= k > 0
                        if let InstKind::Sub(x, k) = func.inst(id).kind
                            && let Some(value) = func.inst_result_value(id)
                            && let Some(k) = func.value_u256(k)
                            && !k.is_zero()
                            && self.range_of(func, x, MAX_DEPTH).lo >= k
                        {
                            self.add_relation(func, Relation::Lt(value, x));
                        }
                        let fact = match &func.inst(id).kind {
                            InstKind::ICall {
                                function: Callee::Builtin(Builtin::Check { is_zero, .. }),
                                args,
                            } => Some((args[0], *is_zero)),
                            InstKind::ICall {
                                function: Callee::Builtin(Builtin::Require(_)),
                                args,
                            } => Some((args[0], true)),
                            _ => None,
                        };
                        if let Some((condition, passing)) = fact {
                            if self.eval_truth(func, condition, MAX_DEPTH) == Some(passing) {
                                checks.insert(id);
                            }
                            self.assume(func, condition, passing, MAX_DEPTH);
                        }
                        if !self.collect_masks
                            && selected.is_none()
                            && let Some(kind) = unfailing_midpoint
                                .or_else(|| self.unfailing_kind(func, &func.inst(id).kind))
                        {
                            self.unfailing.push((id, kind));
                        }
                    }
                    self.check_obligations(func, block);

                    if !self.collect_masks
                        && let Some(Terminator::Branch { condition, then_block, else_block }) =
                            func.blocks[block].terminator.as_ref()
                        && then_block != else_block
                        && selected.is_none_or(|(selected, cold)| {
                            selected.contains(block)
                                && (leads_to_revert(func, *then_block, cold)
                                    || leads_to_revert(func, *else_block, cold))
                        })
                    {
                        match self.eval_truth(func, *condition, MAX_DEPTH) {
                            Some(truth)
                                if selected.is_none_or(|(_, cold)| {
                                    let discarded = if truth { *else_block } else { *then_block };
                                    leads_to_revert(func, discarded, cold)
                                }) =>
                            {
                                folds.push((block, if truth { *then_block } else { *else_block }));
                            }
                            None if selected.is_none() => {
                                let mut reduced = *condition;
                                while let Some(operand) = self.reduced_condition(func, reduced) {
                                    reduced = operand;
                                }
                                if reduced != *condition {
                                    reductions.push((block, reduced));
                                }
                            }
                            _ => {}
                        }
                    }

                    for &child in cfg.dominators().children(block) {
                        stack.push(Walk::Enter(child));
                    }
                }
            }
        }
        self.scoped = false;
        self.active_relations = None;
        (folds, checks, reductions)
    }

    /// The operand an `or` or `and` condition reduces to when the other one is decided as the
    /// neutral value of the operation: false for `or`, true for `and`, whose `i1` operands are
    /// then exactly zero and one.
    fn reduced_condition(&mut self, func: &Function, condition: ValueId) -> Option<ValueId> {
        let (a, b, neutral) = match *inst_kind(func, condition)? {
            InstKind::Or(a, b) => (a, b, false),
            InstKind::And(a, b) => (a, b, true),
            _ => return None,
        };
        if self.eval_truth(func, a, MAX_DEPTH) == Some(neutral) {
            Some(b)
        } else if self.eval_truth(func, b, MAX_DEPTH) == Some(neutral) {
            Some(a)
        } else {
            None
        }
    }

    /// The plain operation a semantic unsigned checked operation becomes when the current scope
    /// proves it cannot fail: a sum or product within its type, a difference that does not
    /// underflow, or a quotient or remainder by a nonzero divisor.
    fn unfailing_kind(&mut self, func: &Function, kind: &InstKind) -> Option<InstKind> {
        let (op, bits, lhs, rhs) = checked_operands(kind)?;
        let max = integer_mask(u32::from(bits));
        let (ra, rb) = (self.range_of(func, lhs, MAX_DEPTH), self.range_of(func, rhs, MAX_DEPTH));
        let unfailing = match op {
            CheckedOp::Add => ra.hi.checked_add(rb.hi).is_some_and(|hi| hi <= max),
            CheckedOp::Mul => ra.hi.checked_mul(rb.hi).is_some_and(|hi| hi <= max),
            CheckedOp::Sub => self.eval_lt(func, lhs, rhs, MAX_DEPTH) == Some(false),
            CheckedOp::Div | CheckedOp::Rem => rb.lo > U256::ZERO,
            _ => false,
        };
        unfailing.then(|| match op {
            CheckedOp::Add => InstKind::Add(lhs, rhs),
            CheckedOp::Mul => InstKind::Mul(lhs, rhs),
            CheckedOp::Sub => InstKind::Sub(lhs, rhs),
            CheckedOp::Div => InstKind::Div(lhs, rhs),
            _ => InstKind::Mod(lhs, rhs),
        })
    }

    /// Decides in the current scope whether a monotone phi's update cannot
    /// wrap, without assuming the invariant it would establish.
    fn update_cannot_wrap(&mut self, func: &Function, phi: &MonotonePhi) -> bool {
        if phi.decreasing {
            // `p - c` wraps exactly when `p < c`.
            self.eval_lt(func, phi.value, phi.step, MAX_DEPTH) == Some(false)
        } else {
            // `lt (add p, c), p` is the wrap flag of `p + c`.
            self.eval_lt(func, phi.next, phi.value, MAX_DEPTH) == Some(false)
        }
    }

    /// Proves the header bounds that hold on every iteration and attaches them to their headers'
    /// entry facts. Every candidate is assumed on entry to its header, and a walk checks each
    /// value the backedges carry into a bounded phi in the scope of the block it leaves. A bound
    /// with a failing obligation is dropped, and the walk repeats until the remaining bounds are
    /// inductive together: each holds on entry from outside its loop, and on every backedge while
    /// all of them held at the headers before it.
    fn prove_header_bounds(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        definitions: &IndexVec<InstId, Option<BlockId>>,
        mut bounds: Vec<HeaderBound>,
        facts: &mut IndexVec<BlockId, Facts>,
        selected: Option<(&DenseBitSet<BlockId>, &DenseBitSet<FunctionId>)>,
    ) {
        const MAX_ROUNDS: usize = 4;
        for _ in 0..MAX_ROUNDS {
            if bounds.is_empty() {
                break;
            }
            bound_obligations(func, cfg, definitions, &bounds, &mut self.obligations);
            self.failed_bounds.clear();
            let mut assumed = facts.clone();
            for bound in &bounds {
                assumed[bound.header].relations.insert(bound.relation());
            }
            self.header_bounds = bounds;
            self.invalidate_indexes();
            let _ = self.collect_folds(func, cfg, &assumed, &[], &mut Vec::new(), selected);
            bounds = std::mem::take(&mut self.header_bounds);
            if self.failed_bounds.is_empty() {
                for bound in &bounds {
                    facts[bound.header].relations.insert(bound.relation());
                }
                self.header_bounds = bounds;
                break;
            }
            bounds = bounds
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !self.failed_bounds.contains(index))
                .map(|(_, bound)| bound)
                .collect();
        }
        self.obligations.clear();
        self.failed_bounds.clear();
        self.invalidate_indexes();
    }

    /// Drops the indexes and caches built from the relations a walk assumes.
    fn invalidate_indexes(&mut self) {
        self.relation_index = None;
        self.reverse_index = None;
        self.strict_bounds_valid = false;
    }

    /// Checks the obligations of the bounds under proof that fall on `block`, in the scope at its
    /// end.
    fn check_obligations(&mut self, func: &Function, block: BlockId) {
        let start = self.obligations.partition_point(|obligation| obligation.block < block);
        for index in start..self.obligations.len() {
            let BoundObligation { bound, block: home, value } = self.obligations[index];
            if home != block {
                break;
            }
            if self.failed_bounds.contains(&bound) {
                continue;
            }
            let HeaderBound { limit, upper, .. } = self.header_bounds[bound];
            let (low, high) = if upper { (value, limit) } else { (limit, value) };
            let holds = values_equal(func, low, high)
                || self.eval_lt(func, high, low, MAX_DEPTH) == Some(false)
                || self.eval_lt(func, low, high, MAX_DEPTH) == Some(true);
            if !holds {
                self.failed_bounds.insert(bound);
            }
        }
    }

    /// Records that a midpoint lies between its endpoints where the scope orders them and its
    /// sum cannot wrap: `low <= m <= high` when `low <= high`, and `m < high` when `low < high`.
    /// Returns whether the endpoints were ordered.
    fn assume_midpoint(&mut self, func: &Function, midpoint: Midpoint) -> bool {
        let Midpoint { value, low, high, sum } = midpoint;
        if let Some(sum) = sum
            && !matches!(
                inst_kind(func, sum),
                Some(InstKind::CheckedBinary {
                    op: CheckedOp::Add,
                    arithmetic: ArithmeticKind::Unsigned(_),
                    ..
                })
            )
            && self.eval_lt(func, sum, low, MAX_DEPTH) != Some(false)
        {
            return false;
        }
        // The halved sum is symmetric in its operands; the halved difference is not.
        let orders: &[(ValueId, ValueId)] =
            if sum.is_some() { &[(low, high), (high, low)] } else { &[(low, high)] };
        let mut ordered = false;
        for &(a, b) in orders {
            if self.eval_lt(func, b, a, MAX_DEPTH) != Some(false) {
                continue;
            }
            ordered = true;
            // !(m < a) and !(b < m)
            self.assume_lt(func, value, a, false, MAX_DEPTH);
            self.assume_lt(func, b, value, false, MAX_DEPTH);
            if self.eval_lt(func, a, b, MAX_DEPTH) == Some(true) {
                self.assume_lt(func, value, b, true, MAX_DEPTH);
            }
        }
        ordered
    }

    /// Transfers edge facts from unknown, retaining only definitions available
    /// at the join and evaluating relevant phis in each predecessor's context.
    fn join_facts(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        definitions: &IndexVec<InstId, Option<BlockId>>,
        relevant: &DenseBitSet<ValueId>,
    ) -> IndexVec<BlockId, Facts> {
        const MAX_ROUNDS: usize = 8;
        // Join phi relations tried at one join, the first in relation order.
        const MAX_PHI_CANDIDATES: usize = 8;
        // A predicate consumed only by this branch cannot be queried after the edge.
        // Preserve its operand facts, but do not copy the dead predicate's own range
        // through every later block. Single-use ISZERO chains have the same property.
        let uses = use_counts(func);
        let mut consumed_conditions = FxHashMap::<BlockId, SmallVec<[ValueId; 2]>>::default();
        for &block in cfg.rpo() {
            if let Some(Terminator::Branch { condition, .. }) = func.blocks[block].terminator {
                let mut value = condition;
                while uses[value] == 1 {
                    consumed_conditions.entry(block).or_default().push(value);
                    let Some(inner) =
                        inst_kind(func, value).and_then(|kind| kind.zero_test_operand(func))
                    else {
                        break;
                    };
                    value = inner;
                }
            }
        }
        let mut entries = index_vec![Facts::default(); func.blocks.len()];
        let mut exits = entries.clone();
        let mut cx = Self { immutable_ranges: self.immutable_ranges, ..Self::default() };
        cx.universal_relations.clone_from(&self.universal_relations);
        let mut pending = cfg.reachable().clone();
        let mut visited = DenseBitSet::new_empty(func.blocks.len());
        let mut phis = Vec::new();
        let mut phi_ranges = Vec::new();
        let mut phi_candidates = Vec::<Relation>::new();
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for &block in cfg.rpo() {
                if !pending.remove(block) {
                    continue;
                }
                let mut merged: Option<Facts> = None;
                // A phi of this join stays below a value defined before it when every input
                // does on its edge, as `i = from < n ? from : n - 1` stays below `n`. Bounds an
                // edge states of an input are the candidates; every edge must prove them.
                phis.clear();
                phis.extend(func.blocks[block].instructions.iter().filter_map(|&inst| {
                    let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
                    let value = func.inst_result_value(inst)?;
                    relevant.contains(value).then_some((value, incoming))
                }));
                let joins = func.blocks[block].predecessors.len() > 1;
                phi_candidates.clear();
                if block != BlockId::ENTRY {
                    for &pred in &func.blocks[block].predecessors {
                        cx.enter_edge(func, block, pred, &exits[pred]);
                        // Evaluate all inputs before assigning any phi: loop
                        // phis describe a simultaneous parallel assignment.
                        phi_ranges.clear();
                        for &(value, incoming) in &phis {
                            if let Some(&(_, input)) =
                                incoming.iter().find(|&&(from, _)| from == pred)
                            {
                                phi_ranges.push((
                                    value,
                                    input,
                                    cx.range_of(func, input, MAX_DEPTH),
                                ));
                            }
                        }
                        for value in consumed_conditions.get(&pred).into_iter().flatten() {
                            cx.ranges.remove(value);
                        }
                        let available = |value| match func.value(value) {
                            Value::Inst(inst) => definitions[*inst].is_some_and(|home| {
                                home != block && cfg.dominators().dominates(home, block)
                            }),
                            _ => true,
                        };
                        if joins {
                            for &(phi, input, _) in &phi_ranges {
                                for &relation in &cx.relations {
                                    let candidate = match relation {
                                        Relation::Lt(x, bound) if x == input => {
                                            Relation::Lt(phi, bound)
                                        }
                                        Relation::Le(x, bound) if x == input => {
                                            Relation::Le(phi, bound)
                                        }
                                        _ => continue,
                                    };
                                    let (_, bound) = candidate.operands();
                                    if bound != phi && available(bound) {
                                        phi_candidates.push(candidate);
                                    }
                                }
                            }
                        }
                        cx.ranges.retain(|&value, _| available(value));
                        cx.relations.retain(|relation| {
                            let (a, b) = relation.operands();
                            available(a) && available(b)
                        });
                        for &(value, _, range) in &phi_ranges {
                            if range != Range::FULL {
                                cx.ranges.insert(value, range);
                            }
                        }
                        if let Some(merged) = &mut merged {
                            merged.ranges.retain(|value, range| {
                                if let Some(other) = cx.ranges.get(value) {
                                    *range = range.union(*other);
                                    *range != Range::FULL
                                } else {
                                    false
                                }
                            });
                            merged.relations.retain(|relation| cx.relations.contains(relation));
                        } else {
                            merged = Some(Facts {
                                ranges: std::mem::take(&mut cx.ranges),
                                relations: std::mem::take(&mut cx.relations),
                            });
                        }
                    }
                }
                // The relations of an edge come in hash order. A join with more candidates than it
                // tries keeps the first in relation order, which does not depend on the hasher.
                phi_candidates.sort_unstable();
                phi_candidates.dedup();
                phi_candidates.truncate(MAX_PHI_CANDIDATES);
                if !phi_candidates.is_empty() {
                    for &pred in &func.blocks[block].predecessors {
                        cx.enter_edge(func, block, pred, &exits[pred]);
                        phi_candidates.retain(|&candidate| {
                            let (phi, bound) = candidate.operands();
                            let Some(&(_, input)) = phis
                                .iter()
                                .find(|&&(value, _)| value == phi)
                                .and_then(|(_, incoming)| {
                                    incoming.iter().find(|&&(from, _)| from == pred)
                                })
                            else {
                                return false;
                            };
                            match candidate {
                                Relation::Lt(..) => {
                                    cx.eval_lt(func, input, bound, MAX_DEPTH) == Some(true)
                                }
                                _ => {
                                    input == bound
                                        || cx.eval_lt(func, bound, input, MAX_DEPTH) == Some(false)
                                }
                            }
                        });
                    }
                    if let Some(merged) = &mut merged {
                        for &relation in &phi_candidates {
                            merged.relations.insert(relation);
                            if !cx.phi_relations.contains(&relation) {
                                cx.phi_relations.push(relation);
                                if let Some(index) = &mut cx.relation_index {
                                    index.insert(relation);
                                }
                            }
                        }
                    }
                }
                let entry = merged.unwrap_or_default();
                // The exit facts depend only on the entry facts.
                let entry_changed = entries[block] != entry;
                if !visited.insert(block) && !entry_changed {
                    continue;
                }
                cx.ranges.clone_from(&entry.ranges);
                cx.relations.clone_from(&entry.relations);
                cx.strict_bounds_valid = false;
                cx.range_undo.clear();
                cx.relation_undo.clear();
                for &inst in &func.blocks[block].instructions {
                    if let Some(value) = func.inst_result_value(inst)
                        && relevant.contains(value)
                    {
                        let range = cx.range_of(func, value, MAX_DEPTH);
                        if range != Range::FULL {
                            cx.ranges.insert(value, range);
                        }
                    }
                }
                let exit = Facts {
                    ranges: std::mem::take(&mut cx.ranges),
                    relations: std::mem::take(&mut cx.relations),
                };
                let exit_changed = exits[block] != exit;
                if exit_changed {
                    for &successor in cfg.successors(block) {
                        pending.insert(successor);
                    }
                }
                changed |= entry_changed || exit_changed;
                entries[block] = entry;
                exits[block] = exit;
            }
            if !changed {
                break;
            }
        }
        if cx.phi_relations.is_empty() {
            self.relation_index = cx.relation_index;
            self.reverse_index = cx.reverse_index;
        } else {
            // The indexes are rebuilt with the join relations.
            self.phi_relations = cx.phi_relations;
        }
        self.difference_index = cx.difference_index;
        entries
    }

    fn redundant_mask(&mut self, func: &Function, id: InstId) -> Option<ValueId> {
        let inst = func.inst(id);
        if let Some((value, low)) = high_mask_input(func, &inst.kind) {
            if inst.result_ty != func.value_ty(value)
                || inst.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
            {
                return None;
            }
            let shifts = self.shifted_high_masks.get(&value)?.clone();
            // `shl x, k` is zero iff the low `256 - k` bits of `x` are, which covers the `low`
            // bits the mask clears.
            return shifts
                .into_iter()
                .any(|(shift, result)| {
                    256 - shift >= low && self.range_of(func, result, MAX_DEPTH).hi.is_zero()
                })
                .then_some(value);
        }
        let (value, mask) = low_mask_input(func, &inst.kind)
            .or_else(|| widened_nonzero_test(func, &inst.kind).map(|value| (value, U256::ONE)))?;
        (self.range_of(func, value, MAX_DEPTH).hi <= mask
            && inst.result_ty == func.value_ty(value)
            && inst.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure))
        .then_some(value)
    }

    // === Fact recording ===

    /// Records the consequences of `value` being `truth` on the current
    /// dominator subtree.
    fn assume(&mut self, func: &Function, value: ValueId, truth: bool, depth: usize) {
        // The condition value itself is now known nonzero or zero.
        if truth {
            self.narrow(value, Range::new(U256::from(1), U256::MAX));
        } else {
            self.narrow(value, Range::singleton(U256::ZERO));
        }
        let Some(depth) = depth.checked_sub(1) else { return };
        let Some(kind) = inst_kind(func, value) else { return };
        // A width test bounds its word the same way in either spelling.
        if let Some(test) = BitWidthTest::of_kind(func, kind) {
            self.narrow(
                test.word,
                if truth == test.fits {
                    Range::new(U256::ZERO, test.limit())
                } else {
                    Range::new(test.limit() + U256::ONE, U256::MAX)
                },
            );
        }
        match *kind {
            InstKind::Ne(a, b) => self.assume_eq(func, a, b, !truth, depth),
            InstKind::Lt(a, b) => self.assume_lt(func, a, b, truth, depth),
            InstKind::Gt(a, b) => self.assume_lt(func, b, a, truth, depth),
            InstKind::SLt(a, b) => self.assume_slt(func, a, b, truth, depth),
            InstKind::SGt(a, b) => self.assume_slt(func, b, a, truth, depth),
            InstKind::Eq(a, b) => self.assume_eq(func, a, b, truth, depth),
            // `sub a, b` is nonzero iff `a != b`.
            InstKind::Sub(a, b) | InstKind::Xor(a, b) => self.assume_eq(func, a, b, !truth, depth),
            // `and a, b != 0` implies both operands are nonzero.
            InstKind::And(a, b) if truth => {
                self.assume(func, a, true, depth);
                self.assume(func, b, true, depth);
            }
            // `or a, b == 0` implies both operands are zero.
            InstKind::Or(a, b) if !truth => {
                self.assume(func, a, false, depth);
                self.assume(func, b, false, depth);
            }
            // `x >> k` is zero iff `x < 2^k`, the range test word-sequence prefers.
            InstKind::Shr(shift, x)
                if let Some(shift) = func.value_u256(shift)
                    && shift > U256::ZERO
                    && shift < U256::from(256) =>
            {
                let bound = U256::from(1) << shift.to::<usize>();
                if truth {
                    self.narrow(x, Range::new(bound, U256::MAX));
                } else {
                    self.narrow(x, Range::new(U256::ZERO, bound - U256::from(1)));
                }
            }
            _ => {}
        }
    }

    /// Records the consequences of `(a < b) == truth` (unsigned).
    fn assume_lt(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        if truth {
            self.add_relation(func, Relation::Lt(a, b));
            // a < b <= hi(b)  =>  a <= hi(b) - 1
            let hi_b = self.range_of(func, b, depth).hi;
            if hi_b > U256::ZERO {
                self.narrow(a, Range::new(U256::ZERO, hi_b - U256::from(1)));
            }
            // lo(a) <= a < b  =>  b >= lo(a) + 1
            let lo_a = self.range_of(func, a, depth).lo;
            if lo_a < U256::MAX {
                self.narrow(b, Range::new(lo_a + U256::from(1), U256::MAX));
            }
        } else {
            // !(a < b)  =>  b <= a
            self.add_relation(func, Relation::Le(b, a));
            let lo_b = self.range_of(func, b, depth).lo;
            self.narrow(a, Range::new(lo_b, U256::MAX));
            let hi_a = self.range_of(func, a, depth).hi;
            self.narrow(b, Range::new(U256::ZERO, hi_a));
        }
        self.narrow_summand(func, a, depth);
        self.narrow_summand(func, b, depth);
    }

    /// Refines signed bounds without treating a cross-sign comparison as unsigned.
    fn assume_slt(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        let bits = integer_bits(func, a);
        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.same_sign(rb, bits) {
            self.assume_lt(func, a, b, truth, depth);
            return;
        }
        let ra = ra.flip_sign(bits);
        let rb = rb.flip_sign(bits);
        let (a_limit, b_limit) = if truth {
            (
                Range::new(U256::ZERO, rb.hi.saturating_sub(U256::from(1))),
                Range::new(ra.lo.saturating_add(U256::from(1)), integer_max(func, a)),
            )
        } else {
            (Range::new(rb.lo, integer_max(func, a)), Range::new(U256::ZERO, ra.hi))
        };
        if let Some(range) = ra.intersect(a_limit) {
            self.narrow(a, range.flip_sign(bits));
        }
        if let Some(range) = rb.intersect(b_limit) {
            self.narrow(b, range.flip_sign(bits));
        }
    }

    /// Records the consequences of `(a == b) == truth`.
    fn assume_eq(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        if func.value_u256(b).is_some_and(|v| v.is_zero()) {
            self.assume(func, a, !truth, depth);
        } else if func.value_u256(a).is_some_and(|v| v.is_zero()) {
            self.assume(func, b, !truth, depth);
        }
        let (x, y) = ordered(a, b);
        if truth {
            self.add_relation(func, Relation::Eq(x, y));
            self.record_equality(func, a, b);
            self.record_equality(func, b, a);
            let range = self.range_of(func, a, depth);
            self.narrow(b, range);
            let range = self.range_of(func, b, depth);
            self.narrow(a, range);
            self.narrow_summand(func, a, depth);
            self.narrow_summand(func, b, depth);
        } else {
            self.add_relation(func, Relation::Ne(x, y));
            self.exclude_boundary(func, a, b, depth);
            self.exclude_boundary(func, b, a, depth);
        }
    }

    /// Records that `value` equals `constant` in the current scope, when `constant` is an
    /// immediate. Operands of `eq`, `ne`, `sub`, and `xor` share one type.
    fn record_equality(&mut self, func: &Function, value: ValueId, constant: ValueId) {
        // An outer scope's constant stays: a contradicting inner edge is dead.
        if let Some(target) = &self.propagate_equalities
            && let Some(constant_value) = func.value_u256(constant)
            && target.is_cheap_immediate(constant_value)
            && func.value_u256(value).is_none()
            && let Some(block) = self.scope
            && !self.equalities.contains_key(&value)
        {
            self.equalities.insert(value, Equality { constant, block, repeated: None });
            self.equality_undo.push(value);
        }
    }

    /// Substitutes proven constants for operands of `id`, as the module docs describe.
    fn propagate_equalities_into(&mut self, func: &Function, cfg: &CfgInfo, id: InstId) {
        let kind = &func.inst(id).kind;
        let mut replaced = Replacements::new();
        kind.visit_operands(|operand| {
            if let Some(equality) = self.equalities.get(&operand) {
                replaced.push((operand, equality.constant));
            }
        });
        if replaced.is_empty() || matches!(kind, InstKind::Phi(_)) {
            return;
        }
        let evaluated = eval::eval_typed_inst(func, kind, |operand| {
            let constant =
                replaced.iter().find(|&&(from, _)| from == operand).map_or(operand, |&(_, c)| c);
            func.value_u256(constant).ok_or(())
        })
        .ok()
        .flatten();
        match evaluated {
            // A wider result would make every read larger, and a memory range it sizes would
            // move the spill area above it on every path, including those that skip the guard.
            Some(value)
                if !self
                    .propagate_equalities
                    .as_ref()
                    .is_some_and(|t| t.is_cheap_immediate(value)) =>
            {
                return;
            }
            Some(_) => {}
            None => replaced.retain(|&mut (operand, _)| self.is_repeated(func, cfg, operand)),
        }
        if !replaced.is_empty() {
            self.equality_uses.push((id, replaced));
        }
    }

    /// Whether `value` equals a constant that at least two instructions below its edge read.
    fn is_repeated(&mut self, func: &Function, cfg: &CfgInfo, value: ValueId) -> bool {
        let Some(equality) = self.equalities.get_mut(&value) else { return false };
        let (block, readers) = (equality.block, &mut self.readers);
        *equality.repeated.get_or_insert_with(|| {
            readers
                .get_or_insert_with(|| value_readers(func))
                .get(value)
                .iter()
                .filter(|&&reader| cfg.dominators().dominates(block, reader))
                .nth(1)
                .is_some()
        })
    }

    /// Replaces the recorded operands by their constants. Returns the number of
    /// instructions changed.
    fn apply_equalities(&mut self, func: &mut Function) -> usize {
        let uses = std::mem::take(&mut self.equality_uses);
        for (id, replaced) in &uses {
            // operand -> constant it equals
            func.inst_mut(*id).rewrite_operands(|operand| {
                if let Some(&(_, constant)) = replaced.iter().find(|&&(from, _)| from == *operand) {
                    *operand = constant;
                }
            });
        }
        uses.len()
    }

    /// Narrows `x` to the range its sum `value = x + c` with a constant `c` now has, when the sum
    /// cannot have wrapped: a checked sum fails rather than wrap, and a plain word sum wrapped only
    /// if it is below both `c` and `x`. This carries the bound an allocation's size checks put on
    /// `length + 1` back to the length.
    fn narrow_summand(&mut self, func: &Function, value: ValueId, depth: usize) {
        let (x, c, checked) = match inst_kind(func, value) {
            Some(&InstKind::Add(a, b)) if integer_bits(func, value) == 256 => (a, b, false),
            Some(&InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(_),
                lhs,
                rhs,
            }) => (lhs, rhs, true),
            _ => return,
        };
        let (x, c) = match (func.value_u256(c), func.value_u256(x)) {
            (Some(c), _) => (x, c),
            (None, Some(constant)) => (c, constant),
            (None, None) => return,
        };
        let range = self.range_of(func, value, depth);
        let exact = checked || range.lo >= c || self.has_relation(func, Relation::Le(x, value));
        if exact && range.hi >= c {
            self.narrow(x, Range::new(range.lo.saturating_sub(c), range.hi - c));
        }
    }

    /// Given `a != b` with `b` a known singleton at a boundary of `a`'s
    /// range, shrinks `a`'s range by one.
    fn exclude_boundary(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) {
        let rb = self.range_of(func, b, depth);
        if !rb.is_singleton() {
            return;
        }
        let ra = self.range_of(func, a, depth);
        if ra.is_singleton() {
            return;
        }
        if ra.lo == rb.lo {
            self.narrow(a, Range::new(ra.lo + U256::from(1), ra.hi));
        } else if ra.hi == rb.hi {
            self.narrow(a, Range::new(ra.lo, ra.hi - U256::from(1)));
        }
    }

    /// Intersects the recorded range of `value` with `range`, logging the
    /// previous entry for scope restoration. Contradictions (an empty
    /// intersection means the current edge is dynamically dead) are skipped:
    /// keeping the weaker fact is always sound.
    fn narrow(&mut self, value: ValueId, range: Range) {
        let old = self.ranges.get(&value).copied();
        let Some(new) = old.unwrap_or(Range::FULL).intersect(range) else { return };
        // A missing entry already denotes FULL. Materializing that sentinel
        // makes long check chains copy facts that convey no restriction.
        if new == old.unwrap_or(Range::FULL) {
            return;
        }
        self.range_undo.push((value, old));
        self.ranges.insert(value, new);
    }

    fn add_relation(&mut self, func: &Function, relation: Relation) {
        if self.relations.insert(relation) {
            self.strict_bounds_valid = false;
            self.relation_undo.push(relation);
            if let (Some(active), Some(index)) = (&mut self.active_relations, &self.relation_index)
            {
                active.add(func, index, relation);
            }
        }
    }

    /// Lists the indexed relations in scope for a dominator walk that keeps them. Builds the
    /// relation index first if needed.
    fn ensure_active_relations(&mut self, func: &Function) {
        if self.active_relations.is_some() {
            return;
        }
        self.ensure_relation_index(func);
        let index = self.relation_index.as_ref().expect("relation index was just built");
        let mut active = ActiveRelations::new(func, index);
        for &relation in &self.universal_relations {
            active.link(func, relation);
        }
        // The undo log holds every relation of the scope, in the order the walk added them.
        for &relation in &self.relation_undo {
            active.add(func, index, relation);
        }
        self.active_relations = Some(Box::new(active));
    }

    /// Builds the forward relation index on first use.
    fn ensure_relation_index(&mut self, func: &Function) {
        if self.relation_index.is_some() {
            return;
        }
        let mut index = relation_candidates(func);
        let relations = self.monotone_relations.iter().chain(&self.phi_relations);
        for &relation in relations.chain(&self.universal_relations) {
            index.insert(relation);
        }
        for bound in &self.header_bounds {
            index.insert(bound.relation());
        }
        for &Midpoint { value, low, high, .. } in self.midpoints.values() {
            for (a, b) in [(low, high), (high, low)] {
                index.insert(Relation::Le(a, value));
                index.insert(Relation::Le(value, b));
                index.insert(Relation::Lt(value, b));
            }
        }
        self.relation_index = Some(index);
    }

    fn ensure_reverse_index(&mut self, func: &Function) {
        if self.reverse_index.is_some() {
            return;
        }
        self.ensure_relation_index(func);
        let mut reverse = FxHashMap::<_, SmallVec<[Relation; 2]>>::default();
        for relation in self.relation_index.as_ref().unwrap().values().flatten() {
            let (a, b) = relation.operands();
            for key in if matches!(relation, Relation::Eq(..)) { [a, b] } else { [b, b] } {
                let entry = reverse.entry(key).or_default();
                if !entry.contains(relation) {
                    entry.push(*relation);
                }
            }
        }
        self.reverse_index = Some(reverse);
    }

    fn ensure_difference_index(&mut self, func: &Function) {
        if self.difference_index.is_some() {
            return;
        }
        let mut differences = FxHashMap::<_, SmallVec<[(ValueId, ValueId); 2]>>::default();
        for inst_id in func.instructions() {
            let Some(value) = func.inst_result_value(inst_id) else { continue };
            if let InstKind::Sub(minuend, subtrahend) = func.inst(inst_id).kind {
                differences.entry(subtrahend).or_default().push((minuend, value));
            }
        }
        self.difference_index = Some(differences);
    }

    /// Whether `base + offset` stays below some value the scope relates to
    /// `limit`, which also proves the sum cannot wrap.
    ///
    /// A checked slice bounds its index by a difference: `k < end - start`
    /// with `start <= end` puts `start + k` below `end`, because the scope
    /// computing `end - start` without wrapping makes the difference exact.
    /// The sum stays below `end`, so it cannot wrap either. Passing `None` for
    /// `limit` asks only for wrap freedom.
    fn sum_stays_below(&mut self, func: &Function, sum: ValueId, limit: Option<ValueId>) -> bool {
        const MAX_SUM_DEPTH: usize = 2;
        if self.sum_depth >= MAX_SUM_DEPTH {
            return false;
        }
        let Some(&InstKind::Add(first, second)) = inst_kind(func, sum) else { return false };
        self.ensure_difference_index(func);
        self.sum_depth += 1;
        let found = [(first, second), (second, first)].into_iter().any(|(base, offset)| {
            let candidates = self
                .difference_index
                .as_ref()
                .and_then(|index| index.get(&base))
                .cloned()
                .unwrap_or_default();
            candidates.into_iter().any(|(minuend, difference)| {
                self.has_relation(func, Relation::Lt(offset, difference))
                    && self.has_relation(func, Relation::Le(base, minuend))
                    && limit.is_none_or(|limit| {
                        minuend == limit || self.has_relation(func, Relation::Le(minuend, limit))
                    })
            })
        });
        self.sum_depth -= 1;
        found
    }

    /// Whether `base + amount < bound` holds in the current scope for a
    /// literal `amount` of at least `offset` such that `base + amount`
    /// cannot wrap.
    fn has_larger_offset_bound(
        &mut self,
        func: &Function,
        base: ValueId,
        offset: U256,
        bound: ValueId,
        depth: usize,
    ) -> bool {
        let mut amounts = SmallVec::<[U256; 4]>::new();
        if self.scoped {
            self.ensure_active_relations(func);
            let active = self.active_relations.as_ref().expect("active relations were just built");
            let bounds = active.offset_bounds.get(&(bound, base));
            amounts
                .extend(bounds.into_iter().flatten().copied().filter(|&amount| amount >= offset));
        } else {
            self.ensure_reverse_index(func);
            let reverse = self.reverse_index.as_ref().expect("relation index was just built");
            for &fact in reverse.get(&bound).into_iter().flatten() {
                if let Relation::Lt(index, limit) = fact
                    && limit == bound
                    && (self.relations.contains(&fact) || self.universal_relations.contains(&fact))
                {
                    amounts.extend(offsets_of(func, index).into_iter().filter_map(
                        |(x, amount)| (x == base && amount >= offset).then_some(amount),
                    ));
                }
            }
        }
        if amounts.is_empty() {
            return false;
        }
        let hi = self.range_of(func, base, depth).hi;
        amounts
            .into_iter()
            .any(|amount| hi.checked_add(amount).is_some_and(|hi| hi <= integer_max(func, base)))
    }

    /// Whether some value is provably below `value` in the current scope,
    /// which puts `value` at one or more: every word is at least zero.
    fn has_strict_lower_bound(&mut self, func: &Function, value: ValueId) -> bool {
        if self.scoped {
            // Without relations in scope no value is above a strict edge.
            if self.relation_undo.is_empty() {
                return false;
            }
            self.ensure_active_relations(func);
            return self
                .active_relations
                .as_ref()
                .is_some_and(|active| active.nonzero.contains(value));
        }
        if !self.strict_bounds_valid {
            self.strict_lower_bounds.clear();
            self.strict_pending.clear();
            for &fact in &self.relations {
                if let Relation::Lt(_, bound) = fact
                    && self.strict_lower_bounds.insert(bound)
                {
                    self.strict_pending.push(bound);
                }
            }
            if !self.strict_pending.is_empty() {
                self.ensure_relation_index(func);
                let index = self.relation_index.as_ref().expect("relation index was just built");
                // A strict edge makes its upper endpoint nonzero. Propagate that fact through
                // active orderings once per scope instead of searching backward for every value.
                while let Some(current) = self.strict_pending.pop() {
                    for &fact in index.get(&current).into_iter().flatten() {
                        if !self.relations.contains(&fact)
                            && !self.universal_relations.contains(&fact)
                        {
                            continue;
                        }
                        if let Some((next, _)) = fact.step(current, false, false)
                            && self.strict_lower_bounds.insert(next)
                        {
                            self.strict_pending.push(next);
                        }
                    }
                }
            }
            self.strict_bounds_valid = true;
        }
        self.strict_lower_bounds.contains(value)
    }

    fn has_relation(&mut self, func: &Function, relation: Relation) -> bool {
        if self.relations.contains(&relation) || self.universal_relations.contains(&relation) {
            return true;
        }
        let (start, end, needs_strict, equality_only) = match relation {
            Relation::Lt(a, b) => (a, b, true, false),
            Relation::Le(a, b) => (a, b, false, false),
            Relation::Eq(a, b) => (a, b, false, true),
            Relation::Ne(..) => return false,
        };
        if start == end && !needs_strict {
            return true;
        }
        if self.relations.len() <= 1 && self.universal_relations.is_empty() {
            // A single strict/equal edge also proves a non-strict comparison.
            // Every other single-edge implication was covered by direct lookup.
            return self.relations.iter().any(|fact| match *fact {
                Relation::Lt(a, b) => !equality_only && !needs_strict && a == start && b == end,
                Relation::Eq(a, b) => !needs_strict && ordered(start, end) == (a, b),
                Relation::Le(..) | Relation::Ne(..) => false,
            });
        }
        self.ensure_relation_index(func);
        if self.scoped {
            self.ensure_active_relations(func);
        }
        let index = self.relation_index.as_ref().expect("relation index was just built");
        if !index.contains_key(&start) {
            return false;
        }
        let mut seen = std::mem::take(&mut self.relation_seen);
        seen.clear();
        let found = self.search_relations(start, end, needs_strict, equality_only, &mut seen);
        self.relation_seen = seen;
        if let Some(found) = found {
            return found;
        }
        // A sum bounded by a difference is below that difference's minuend,
        // which the edges above could not express because the difference
        // relates the offset rather than the sum.
        match relation {
            Relation::Lt(sum, limit) => self.sum_stays_below(func, sum, Some(limit)),
            // `a <= a + b` needs only that the sum cannot wrap.
            Relation::Le(base, sum) => {
                matches!(inst_kind(func, sum), Some(&InstKind::Add(x, y)) if x == base || y == base)
                    && self.sum_stays_below(func, sum, None)
            }
            Relation::Eq(..) | Relation::Ne(..) => false,
        }
    }

    /// A bounded implication search: equality is bidirectional, <= carries
    /// order, and one strict edge makes the complete path strict. Disequality
    /// is not transitive. Exhausting the budget only misses an optimization.
    /// Returns `None` when the search space is exhausted without a decision.
    fn search_relations(
        &self,
        start: ValueId,
        end: ValueId,
        needs_strict: bool,
        equality_only: bool,
        seen: &mut FxHashSet<(ValueId, bool)>,
    ) -> Option<bool> {
        const MAX_RELATION_STATES: usize = 128;
        let index = self.relation_index.as_ref().expect("relation index was just built");
        if self.active_relations.is_none() {
            let mut pending = SmallVec::<[_; 8]>::new();
            pending.push((start, false));
            while let Some((value, strict)) = pending.pop() {
                if value == end && (!needs_strict || strict) {
                    return Some(true);
                }
                if !seen.insert((value, strict)) {
                    continue;
                }
                if seen.len() >= MAX_RELATION_STATES {
                    return Some(false);
                }
                for &fact in index.get(&value).into_iter().flatten() {
                    if !self.relations.contains(&fact) && !self.universal_relations.contains(&fact)
                    {
                        continue;
                    }
                    if let Some(next) = fact.step(value, strict, equality_only) {
                        pending.push(next);
                    }
                }
            }
            return None;
        }
        // Depth-first, taking a state's relations last to first and only as the search reaches
        // them, which visits the states in the order of a stack holding every successor. Each
        // frame holds the candidates of its value and the positions of those in scope it has left.
        let active = self.active_relations.as_ref().expect("active relations are built");
        let mut frames = SmallVec::<[_; 8]>::new();
        let mut next = Some((start, false));
        loop {
            if let Some((value, strict)) = next.take() {
                if value == end && (!needs_strict || strict) {
                    return Some(true);
                }
                if seen.insert((value, strict)) {
                    if seen.len() >= MAX_RELATION_STATES {
                        return Some(false);
                    }
                    let candidates = index.get(&value).map_or(&[][..], |list| list.as_slice());
                    let positions = active.in_scope.get(&value).map_or(&[][..], |p| p.as_slice());
                    frames.push((value, strict, candidates, positions));
                }
            }
            let (value, strict, candidates, positions) = frames.last_mut()?;
            let Some((&position, rest)) = positions.split_last() else {
                frames.pop();
                continue;
            };
            *positions = rest;
            next = candidates[position as usize].step(*value, *strict, equality_only);
        }
    }

    // === Evaluation ===

    /// Computes a sound overapproximation of the values `value` can take at
    /// the current program point.
    fn range_of(&mut self, func: &Function, value: ValueId, depth: usize) -> Range {
        if let Some(constant) = func.value_u256(value) {
            return Range::singleton(constant);
        }
        let bits = integer_bits(func, value).min(256);
        let full = Range::new(U256::ZERO, integer_mask(bits));
        let mut range =
            self.ranges.get(&value).and_then(|range| range.intersect(full)).unwrap_or(full);
        if let Some(bound) = self.trip_bounds.get(&value) {
            range = range.intersect(*bound).unwrap_or(range);
        }
        if range.lo.is_zero() && self.has_strict_lower_bound(func, value) {
            range.lo = U256::ONE;
        }
        let Some(depth) = depth.checked_sub(1) else { return range };
        let Some(kind) = inst_kind(func, value) else { return range };
        let derived = match *kind {
            InstKind::Zext(source) => self.range_of(func, source, depth),
            InstKind::Trunc(source, bits) if (1..=256).contains(&bits) => {
                let source = self.range_of(func, source, depth);
                let mask = integer_mask(bits);
                if source.hi <= mask { source } else { Range::new(U256::ZERO, mask) }
            }
            InstKind::Sext(source, from_bits, _) if (1..=256).contains(&from_bits) => {
                let source = self.range_of(func, source, depth);
                // Sign extension preserves nonnegative values; other signs remain unknown.
                if source.hi < (U256::ONE << (from_bits - 1)) { source } else { Range::FULL }
            }
            InstKind::LoadImmutable(id) => self
                .immutable_ranges
                .and_then(|ranges| ranges.get(&id))
                .copied()
                .unwrap_or(Range::FULL),
            InstKind::Add(a, b) => {
                // An unconstrained operand leaves the wrapping sum's interval unconstrained.
                let rb = self.range_of(func, b, depth);
                if rb == full {
                    full
                } else {
                    let ra = self.range_of(func, a, depth);
                    match ra.hi.checked_add(rb.hi).filter(|&hi| hi <= full.hi) {
                        Some(hi) => Range::new(ra.lo.wrapping_add(rb.lo), hi),
                        None => Range::FULL,
                    }
                }
            }
            InstKind::Sub(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                if ra.lo >= rb.hi {
                    Range::new(ra.lo - rb.hi, ra.hi - rb.lo)
                } else if self.has_relation(func, Relation::Le(b, a)) {
                    // A strictly ordered difference is at least one.
                    let hi = ra.hi.saturating_sub(rb.lo);
                    let lo = if hi >= U256::ONE && self.has_relation(func, Relation::Lt(b, a)) {
                        U256::ONE
                    } else {
                        U256::ZERO
                    };
                    Range::new(lo, hi)
                } else {
                    Range::FULL
                }
            }
            InstKind::Mul(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                match ra.hi.checked_mul(rb.hi).filter(|&hi| hi <= full.hi) {
                    Some(hi) => Range::new(ra.lo.wrapping_mul(rb.lo), hi),
                    None => Range::FULL,
                }
            }
            InstKind::Div(a, b) => {
                // EVM division by zero yields zero, so the result never
                // exceeds the dividend.
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                if rb.lo > U256::ZERO {
                    Range::new(ra.lo / rb.hi, ra.hi / rb.lo)
                } else {
                    Range::new(U256::ZERO, ra.hi)
                }
            }
            // A checked sum or difference fails rather than wrap, so it is exact
            // wherever it exists.
            InstKind::CheckedBinary {
                op: op @ (CheckedOp::Add | CheckedOp::Sub),
                arithmetic: ArithmeticKind::Unsigned(bits),
                lhs,
                rhs,
            } => {
                let max = integer_mask(u32::from(bits));
                let ra = self.range_of(func, lhs, depth);
                let rb = self.range_of(func, rhs, depth);
                match op {
                    CheckedOp::Add => match ra.lo.checked_add(rb.lo).filter(|&lo| lo <= max) {
                        Some(lo) => Range::new(lo, ra.hi.saturating_add(rb.hi).min(max)),
                        None => Range::FULL,
                    },
                    _ if ra.hi >= rb.lo => {
                        // A strictly ordered difference is at least one.
                        let hi = ra.hi - rb.lo;
                        let mut lo = ra.lo.saturating_sub(rb.hi);
                        if lo.is_zero()
                            && hi >= U256::ONE
                            && self.has_relation(func, Relation::Lt(rhs, lhs))
                        {
                            lo = U256::ONE;
                        }
                        Range::new(lo, hi)
                    }
                    _ => Range::FULL,
                }
            }
            InstKind::Mod(a, b) => {
                // EVM modulo by zero yields zero; otherwise the result is
                // less than the divisor and never exceeds the dividend.
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                let bound = if rb.hi > U256::ZERO { rb.hi - U256::from(1) } else { U256::ZERO };
                Range::new(U256::ZERO, bound.min(ra.hi))
            }
            InstKind::And(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                Range::new(U256::ZERO, ra.hi.min(rb.hi))
            }
            // Neither sets a bit above both operands' highest bits; `or` also
            // keeps every bit of each, so it is at least either operand.
            InstKind::Or(a, b) | InstKind::Xor(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                let bits = ra.hi.max(rb.hi).bit_len();
                let hi = if bits == 256 { U256::MAX } else { (U256::ONE << bits) - U256::ONE };
                let lo =
                    if matches!(kind, InstKind::Or(..)) { ra.lo.max(rb.lo) } else { U256::ZERO };
                Range::new(lo, hi)
            }
            // EVM shifts take the count first. A constant right shift maps both
            // bounds; a constant left shift keeps them when the top cannot spill.
            InstKind::Shr(shift, value) => {
                let rv = self.range_of(func, value, depth);
                match shift_amount(self, func, shift, depth) {
                    Some(bits) => Range::new(rv.lo >> bits, rv.hi >> bits),
                    None => Range::new(U256::ZERO, rv.hi),
                }
            }
            InstKind::Shl(shift, value) => {
                let rv = self.range_of(func, value, depth);
                match shift_amount(self, func, shift, depth) {
                    Some(shift) if shift < bits as usize && rv.hi <= (full.hi >> shift) => {
                        Range::new(rv.lo << shift, rv.hi << shift)
                    }
                    _ => Range::FULL,
                }
            }
            // `byte` extracts one byte and yields zero past the word.
            InstKind::Byte(..) => Range::new(U256::ZERO, U256::from(255)),
            InstKind::Not(a) => {
                let ra = self.range_of(func, a, depth);
                Range::new(ra.hi ^ full.hi, ra.lo ^ full.hi)
            }
            InstKind::Lt(..)
            | InstKind::Gt(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Eq(..)
            | InstKind::Ne(..) => match self.eval_truth(func, value, depth) {
                Some(true) => Range::singleton(U256::from(1)),
                Some(false) => Range::singleton(U256::ZERO),
                None => Range::new(U256::ZERO, U256::from(1)),
            },
            InstKind::Select(condition, then_value, else_value) => {
                match self.eval_truth(func, condition, depth) {
                    Some(true) => self.range_of(func, then_value, depth),
                    Some(false) => self.range_of(func, else_value, depth),
                    None => self
                        .range_of(func, then_value, depth)
                        .union(self.range_of(func, else_value, depth)),
                }
            }
            _ => kind.evm_opcode().and_then(op::definition).map_or(Range::FULL, |def| {
                Range::new(U256::ZERO, integer_mask(u32::from(def.result_bits)))
            }),
        };
        // Both bounds are sound, so their intersection is too. An empty
        // intersection means this point is dynamically unreachable; keep the
        // recorded fact in that case.
        if let Some(intersection) = range.intersect(derived) {
            range = intersection;
        }
        range
    }

    /// Evaluates the truthiness (`!= 0`) of `value`, if provable.
    fn eval_truth(&mut self, func: &Function, value: ValueId, depth: usize) -> Option<bool> {
        if let Some(constant) = func.value_u256(value) {
            return Some(!constant.is_zero());
        }
        if let Some(truth) = self.ranges.get(&value).and_then(|range| range.truth()) {
            return Some(truth);
        }
        if self.has_strict_lower_bound(func, value) {
            return Some(true);
        }
        let depth = depth.checked_sub(1)?;
        let kind = inst_kind(func, value)?;
        match *kind {
            InstKind::Lt(a, b) => self.eval_lt(func, a, b, depth),
            InstKind::Gt(a, b) => self.eval_lt(func, b, a, depth),
            InstKind::SLt(a, b) => self.eval_slt(func, a, b, depth),
            InstKind::SGt(a, b) => self.eval_slt(func, b, a, depth),
            InstKind::Eq(a, b) => self.eval_eq(func, a, b, depth),
            InstKind::Ne(a, b) => self.eval_eq(func, a, b, depth).map(|truth| !truth),
            InstKind::Sub(a, b) | InstKind::Xor(a, b) => {
                self.eval_eq(func, a, b, depth).map(|eq| !eq)
            }
            InstKind::And(a, b) => {
                let ta = self.eval_truth(func, a, depth);
                let tb = self.eval_truth(func, b, depth);
                if ta == Some(false) || tb == Some(false) {
                    return Some(false);
                }
                // Bitwise AND of two values both known to be exactly one.
                let one = Range::singleton(U256::from(1));
                if self.range_of(func, a, depth) == one && self.range_of(func, b, depth) == one {
                    return Some(true);
                }
                None
            }
            InstKind::Or(a, b) => {
                if self.doubling_check_holds(func, a, b, depth)
                    || self.doubling_check_holds(func, b, a, depth)
                {
                    return Some(true);
                }
                let ta = self.eval_truth(func, a, depth);
                let tb = self.eval_truth(func, b, depth);
                if ta == Some(true) || tb == Some(true) {
                    return Some(true);
                }
                if ta == Some(false) && tb == Some(false) {
                    return Some(false);
                }
                None
            }
            _ => self.range_of(func, value, depth).truth(),
        }
    }

    /// Evaluates signed order using bounds rotated by the sign bit.
    fn eval_slt(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) -> Option<bool> {
        if a == b {
            return Some(false);
        }
        let bits = integer_bits(func, a);
        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.same_sign(rb, bits) {
            return self.eval_lt(func, a, b, depth);
        }
        let ra = ra.flip_sign(bits);
        let rb = rb.flip_sign(bits);
        if ra.hi < rb.lo {
            Some(true)
        } else if ra.lo >= rb.hi {
            Some(false)
        } else {
            None
        }
    }

    /// Evaluates `a < b` (unsigned), if provable.
    fn eval_lt(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) -> Option<bool> {
        if a == b {
            return Some(false);
        }

        // Overflow check for checked add: `lt (add x, y), x` is the wrap
        // flag of `x + y`. Test it before the general relation and range path:
        // expanding the result range would recursively derive the same input
        // bounds and discard it once wrapping remains possible.
        if let Some(&InstKind::Add(x, y)) = inst_kind(func, a)
            && (b == x || b == y)
        {
            let rx = self.range_of(func, x, depth);
            let ry = self.range_of(func, y, depth);
            if rx.hi.checked_add(ry.hi).is_some_and(|hi| hi <= integer_max(func, a)) {
                return Some(false);
            }
            if rx.lo.checked_add(ry.lo).is_none_or(|lo| lo > integer_max(func, a)) {
                return Some(true);
            }
        }

        // Underflow check variant `lt x, (sub x, y)`: equivalent to
        // `lt x, y` for every `y` (with wrapping subtraction).
        if let Some(&InstKind::Sub(x, y)) = inst_kind(func, b)
            && a == x
            && let Some(reduced_depth) = depth.checked_sub(1)
        {
            return self.eval_lt(func, x, y, reduced_depth);
        }

        // Offsets keep an order: `x + c < y + d` follows from `x < y` when `c <= d` and
        // `y + d` cannot wrap, as a copy's destination index `k + offset` against the
        // destination's length `n + offset` while `k < n`.
        if let (Some((x, c)), Some((y, d))) = (constant_offset(func, a), constant_offset(func, b))
            && c <= d
            && self
                .range_of(func, y, depth)
                .hi
                .checked_add(d)
                .is_some_and(|hi| hi <= integer_max(func, b))
            && self.has_relation(func, Relation::Lt(x, y))
        {
            return Some(true);
        }

        // A doubled index stays below a doubled bound its source respects:
        // `2x < 2y` and `2x + 1 < 2y` follow from `x < y` when `2y` cannot wrap,
        // which covers `out[2 * i]` and `out[2 * i + 1]` against `2 * n`.
        if let (Some((x, _)), Some((y, false))) = (doubled(func, a), doubled(func, b))
            && self.range_of(func, y, depth).hi <= integer_max(func, y) >> 1
            && self.has_relation(func, Relation::Lt(x, y))
        {
            return Some(true);
        }

        // A decremented index stays below a bound its source respects:
        // `x - c < b` follows from `x <= b` when `c >= 1` and `x - c` cannot
        // wrap, which covers `a[j - 1]` after `j <= i < a.length`.
        if let Some((x, c, exact)) = difference_operands(func, a)
            && let Some(step) = func.value_u256(c)
            && !step.is_zero()
            && (exact || self.range_of(func, x, depth).lo >= step)
            && self.has_relation(func, Relation::Le(x, b))
        {
            return Some(true);
        }

        // A difference stays below whatever its minuend stays below:
        // `x - y < b` follows from `y <= x` (no wrap) and `x < b`, which
        // covers `a[length - 1 - i]` once `i <= length - 1 < length`.
        if let Some(&InstKind::Sub(x, y)) = inst_kind(func, a)
            && let Some(reduced_depth) = depth.checked_sub(1)
            && (self.has_relation(func, Relation::Le(y, x))
                || self.range_of(func, x, depth).lo >= self.range_of(func, y, depth).hi)
            && self.eval_lt(func, x, b, reduced_depth) == Some(true)
        {
            return Some(true);
        }

        // `x - 1 < b` is false when `b < x` and `x` is nonzero: `b < x` means
        // `b <= x - 1`, which covers `length - 1 < i` after `i < length`.
        if let Some(&InstKind::Sub(x, c)) = inst_kind(func, a)
            && func.value_u256(c) == Some(U256::ONE)
            && self.range_of(func, x, depth).lo >= U256::ONE
            && self.has_relation(func, Relation::Lt(b, x))
        {
            return Some(false);
        }

        // A smaller constant offset stays below whatever a larger one stays
        // below: `x + b < n` follows from `x + a < n` when `b <= a` and
        // `x + a` cannot wrap, which covers the lookahead guards and the
        // original bound of a loop split to run while `i + K < n`.
        let (base, offset) = offsets_of(func, a).first().copied().unwrap_or((a, U256::ZERO));
        if self.has_larger_offset_bound(func, base, offset, b, depth) {
            return Some(true);
        }

        let (x, y) = ordered(a, b);
        if self.has_relation(func, Relation::Lt(a, b)) {
            return Some(true);
        }
        // `a <= b` and `a != b`
        if self.relations.contains(&Relation::Ne(x, y))
            && self.has_relation(func, Relation::Le(a, b))
        {
            return Some(true);
        }
        // `b = s + 1` with `s < a` is at most `a`, and the sum cannot wrap below `a`.
        if let Some(successor) = successor_of(func, b)
            && self.has_relation(func, Relation::Lt(successor, a))
        {
            return Some(false);
        }
        // `b = s + 1` with `a <= s` is above `a` when the sum cannot wrap: a checked sum fails
        // rather than wrap, and a plain one wraps only from the largest word.
        if let Some(successor) = successor_of(func, b)
            && self.has_relation(func, Relation::Le(a, successor))
            && (matches!(inst_kind(func, b), Some(InstKind::CheckedBinary { .. }))
                || self.range_of(func, successor, depth).hi < integer_max(func, successor))
        {
            return Some(true);
        }
        if self.has_relation(func, Relation::Lt(b, a))
            || self.has_relation(func, Relation::Le(b, a))
            || self.has_relation(func, Relation::Eq(x, y))
        {
            return Some(false);
        }

        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.hi < rb.lo {
            return Some(true);
        }
        if ra.lo >= rb.hi {
            return Some(false);
        }

        None
    }

    /// Evaluates `a == b`, if provable.
    fn eval_eq(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) -> Option<bool> {
        if a == b {
            return Some(true);
        }

        // Overflow check for checked mul: `eq (div (mul x, y), y), x` holds
        // iff `x * y` did not wrap, provided the divisor is nonzero. Recognize
        // it before deriving the complete ranges of both expression trees.
        if let Some(truth) = self.eval_muldiv_roundtrip(func, a, b, depth) {
            return Some(truth);
        }
        if let Some(truth) = self.eval_muldiv_roundtrip(func, b, a, depth) {
            return Some(truth);
        }
        // Its doubling form `eq (shr (add x, x), 1), x` holds iff `x + x` did not wrap.
        for (shifted, expected) in [(a, b), (b, a)] {
            if let Some(&InstKind::Shr(count, sum)) = inst_kind(func, shifted)
                && func.value_u256(count) == Some(U256::from(1))
                && doubled(func, sum) == Some((expected, false))
                && self.range_of(func, expected, depth).hi <= integer_max(func, expected) >> 1
            {
                return Some(true);
            }
        }
        // A product by `2^k` tests `eq (shr (shl x, k), k), x`, which holds iff the left
        // shift kept every bit of `x` within its width, as a calldata element offset `i * 32`
        // does for `i < length` once decoding bounded the length.
        for (shifted, expected) in [(a, b), (b, a)] {
            if let Some(&InstKind::Shr(count, inner)) = inst_kind(func, shifted)
                && let Some(&InstKind::Shl(inner_count, value)) = inst_kind(func, inner)
                && value == expected
                && let Some(bits) = func.value_u256(count)
                && func.value_u256(inner_count) == Some(bits)
                && bits < U256::from(256)
                && self.range_of(func, expected, depth).hi
                    <= integer_max(func, expected) >> bits.to::<usize>()
            {
                return Some(true);
            }
        }

        let (x, y) = ordered(a, b);
        if self.has_relation(func, Relation::Eq(x, y)) {
            return Some(true);
        }
        if self.has_relation(func, Relation::Ne(x, y))
            || self.has_relation(func, Relation::Lt(a, b))
            || self.has_relation(func, Relation::Lt(b, a))
        {
            return Some(false);
        }

        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.hi < rb.lo || rb.hi < ra.lo {
            return Some(false);
        }
        if ra.is_singleton() && ra == rb {
            return Some(true);
        }

        None
    }

    /// Recognizes the checked doubling `or (eq x, 0), (eq (div (add x, x), x), 2)`,
    /// which holds whenever `x + x` cannot wrap: a zero `x` satisfies the first
    /// disjunct and any other `x` divides its doubling back to two.
    fn doubling_check_holds(
        &mut self,
        func: &Function,
        zero_test: ValueId,
        roundtrip: ValueId,
        depth: usize,
    ) -> bool {
        let Some(x) = inst_kind(func, zero_test).and_then(|kind| kind.zero_test_operand(func))
        else {
            return false;
        };
        let Some(&InstKind::Eq(lhs, rhs)) = inst_kind(func, roundtrip) else { return false };
        let quotient = if func.value_u256(rhs) == Some(U256::from(2)) {
            lhs
        } else if func.value_u256(lhs) == Some(U256::from(2)) {
            rhs
        } else {
            return false;
        };
        let Some(&InstKind::Div(sum, divisor)) = inst_kind(func, quotient) else { return false };
        if divisor != x || doubled(func, sum) != Some((x, false)) {
            return false;
        }
        self.range_of(func, x, depth).hi <= (integer_max(func, x) >> 1)
    }

    /// Recognizes `div (mul x, y), d == x` with `d == y` and proves it true
    /// when `x * y` cannot wrap and the divisor is provably nonzero.
    fn eval_muldiv_roundtrip(
        &mut self,
        func: &Function,
        div_value: ValueId,
        expected: ValueId,
        depth: usize,
    ) -> Option<bool> {
        let InstKind::Div(mul_value, divisor) = *inst_kind(func, div_value)? else { return None };
        let InstKind::Mul(p, q) = *inst_kind(func, mul_value)? else { return None };
        for (x, y) in [(p, q), (q, p)] {
            if x != expected || !values_equal(func, divisor, y) {
                continue;
            }
            let ry = self.range_of(func, y, depth);
            if ry.lo.is_zero() {
                continue;
            }
            let rx = self.range_of(func, x, depth);
            if rx.hi.checked_mul(ry.hi).is_some_and(|hi| hi <= integer_max(func, mul_value)) {
                return Some(true);
            }
        }
        None
    }

    /// Resets the scoped facts to `exit`, the facts leaving `pred`, and assumes what
    /// `pred`'s branch implies on its edge to `block`.
    fn enter_edge(&mut self, func: &Function, block: BlockId, pred: BlockId, exit: &Facts) {
        self.ranges.clone_from(&exit.ranges);
        self.relations.clone_from(&exit.relations);
        self.strict_bounds_valid = false;
        self.range_undo.clear();
        self.relation_undo.clear();
        if let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[pred].terminator
            && then_block != else_block
        {
            self.assume(func, condition, then_block == block, MAX_DEPTH);
        }
    }
}

/// The blocks of the instructions reading each value, phis aside.
fn value_readers(func: &Function) -> IndexLists<ValueId, BlockId> {
    let mut pairs = Vec::new();
    for (block, body) in func.blocks.iter_enumerated() {
        for &id in &body.instructions {
            let kind = &func.inst(id).kind;
            if !matches!(kind, InstKind::Phi(_)) {
                kind.visit_operands(|operand| pairs.push((operand, block)));
            }
        }
    }
    IndexLists::new(func.num_values(), pairs.iter().copied())
}

/// Values whose ranges can affect a branch, closed over all SSA operands.
/// Phi inputs keep loop-carried dependencies in the set. Memory and call
/// operands are included conservatively even when range evaluation stops there.
fn branch_inputs(func: &Function, cfg: &CfgInfo) -> DenseBitSet<ValueId> {
    let mut relevant = DenseBitSet::new_empty(func.num_values());
    let mut pending = Vec::new();
    for &block in cfg.rpo() {
        if let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block].terminator
            && then_block != else_block
        {
            pending.push(condition);
        }
        for &inst in &func.blocks[block].instructions {
            let kind = &func.inst(inst).kind;
            if let InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), args } = kind
            {
                pending.push(args[0]);
            } else if let Some((_, _, lhs, rhs)) = checked_operands(kind) {
                pending.extend([lhs, rhs]);
            }
        }
    }
    while let Some(value) = pending.pop() {
        if relevant.insert(value)
            && let Value::Inst(inst) = func.value(value)
        {
            pending.extend(func.inst(*inst).operands());
        }
    }
    relevant
}

/// Orderings that hold wherever their operands do: a right shift, a division
/// by a nonzero constant, a mask, or a remainder never exceeds the value it
/// derives from. Only values feeding branches are indexed.
fn universal_relations(func: &Function, relevant: &DenseBitSet<ValueId>) -> FxHashSet<Relation> {
    let mut relations = FxHashSet::default();
    // Every `calldatasize` read gives the call's one size, so each equals the first; the IR keeps
    // them apart, which keeps decoding's live ranges short.
    let mut calldata_size = None;
    for inst_id in func.instructions() {
        let Some(value) = func.inst_result_value(inst_id) else { continue };
        if !relevant.contains(value) {
            continue;
        }
        match func.inst(inst_id).kind {
            InstKind::CalldataSize => match calldata_size {
                Some(first) => {
                    let (a, b) = ordered(first, value);
                    relations.insert(Relation::Eq(a, b));
                }
                None => calldata_size = Some(value),
            },
            InstKind::Shr(_, x) | InstKind::Trunc(x, _) => {
                relations.insert(Relation::Le(value, x));
            }
            InstKind::Mod(x, divisor) => {
                relations.insert(Relation::Le(value, x));
                if func.value_u256(divisor).is_some_and(|c| !c.is_zero()) {
                    relations.insert(Relation::Lt(value, divisor));
                }
            }
            InstKind::Zext(x) => {
                let (a, b) = ordered(value, x);
                relations.insert(Relation::Eq(a, b));
            }
            InstKind::Div(x, divisor) if func.value_u256(divisor).is_some_and(|c| !c.is_zero()) => {
                relations.insert(Relation::Le(value, x));
            }
            // A checked remainder fails on a zero divisor, so it is below its divisor wherever it
            // exists, and so is a remainder by a nonzero constant.
            InstKind::CheckedBinary {
                op: CheckedOp::Rem,
                arithmetic: ArithmeticKind::Unsigned(_),
                lhs: x,
                rhs: divisor,
            } => {
                relations.insert(Relation::Le(value, x));
                relations.insert(Relation::Lt(value, divisor));
            }
            // A checked difference fails rather than wrap, so it never exceeds its minuend, and a
            // nonzero constant subtrahend keeps it below.
            InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(_),
                lhs: x,
                rhs: y,
            } => {
                if func.value_u256(y).is_some_and(|c| !c.is_zero()) {
                    relations.insert(Relation::Lt(value, x));
                } else {
                    relations.insert(Relation::Le(value, x));
                }
            }
            InstKind::And(x, y) => {
                relations.insert(Relation::Le(value, x));
                relations.insert(Relation::Le(value, y));
            }
            // If conversion rewrites `if (x > limit) x = limit` to
            // `x + (x > limit) * (limit - x)`, which is `limit` when the test
            // holds and `x` otherwise, so it never exceeds `limit`.
            InstKind::Add(x, adjustment) => {
                for (x, adjustment) in [(x, adjustment), (adjustment, x)] {
                    if let Some(limit) = clamp_limit(func, x, adjustment) {
                        relations.insert(Relation::Le(value, limit));
                    }
                }
            }
            _ => {}
        }
    }
    relations
}

/// The upper limit of a clamp written as `x + (x > limit) * (limit - x)`.
///
/// The product is zero when the test fails, leaving `x`, and `limit - x` when
/// it holds, leaving exactly `limit` under wrapping addition. Either way the
/// sum is at most `limit`.
fn clamp_limit(func: &Function, x: ValueId, adjustment: ValueId) -> Option<ValueId> {
    let &InstKind::Mul(first, second) = inst_kind(func, adjustment)? else { return None };
    for (condition, difference) in [(first, second), (second, first)] {
        let Some(&InstKind::Gt(tested, limit)) = inst_kind(func, condition) else { continue };
        if tested != x {
            continue;
        }
        if let Some(&InstKind::Sub(minuend, subtrahend)) = inst_kind(func, difference)
            && minuend == limit
            && subtrahend == x
        {
            return Some(limit);
        }
    }
    None
}

/// Follows the boolean operations that `assume` can inspect, in stable block
/// and operand order. Other computations cannot introduce a relational fact.
/// Conditions outside the current scope are harmless: every candidate still
/// requires membership in the current fact set. No IR changes during analysis.
fn relation_candidates(func: &Function) -> RelationIndex {
    let mut index = RelationIndex::default();
    let mut visited = DenseBitSet::new_empty(func.num_values());
    let mut pending = Vec::new();
    for block in &func.blocks {
        if let Some(Terminator::Branch { condition, then_block, else_block }) = block.terminator
            && then_block != else_block
        {
            pending.push(condition);
        }
        // A passing check states its condition for all later execution, as a branch does for
        // the blocks it dominates.
        for &inst in &block.instructions {
            if let InstKind::ICall {
                function: Callee::Builtin(Builtin::Check { .. } | Builtin::Require(_)),
                args,
            } = &func.inst(inst).kind
                && let Some(&condition) = args.first()
            {
                pending.push(condition);
            }
        }
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            match inst_kind(func, value) {
                Some(&InstKind::Lt(a, b)) | Some(&InstKind::Gt(b, a)) => {
                    index.insert(Relation::Lt(a, b));
                    index.insert(Relation::Le(b, a));
                }
                Some(
                    &InstKind::Eq(a, b)
                    | &InstKind::Ne(a, b)
                    | &InstKind::Sub(a, b)
                    | &InstKind::Xor(a, b),
                ) => {
                    let (x, y) = ordered(a, b);
                    index.insert(Relation::Eq(x, y));
                }

                Some(&InstKind::And(a, b) | &InstKind::Or(a, b)) => {
                    pending.extend([b, a]);
                }
                _ => {}
            }
        }
        // A remainder's bound by its divisor, which the walk states where the divisor is
        // positive, and a difference's by its minuend, where the minuend is at least the
        // constant subtrahend.
        for &inst in &block.instructions {
            if let InstKind::Mod(_, divisor) = func.inst(inst).kind
                && let Some(value) = func.inst_result_value(inst)
            {
                index.insert(Relation::Lt(value, divisor));
            }
            if let InstKind::Sub(x, k) = func.inst(inst).kind
                && let Some(value) = func.inst_result_value(inst)
                && func.value_u256(k).is_some_and(|k| !k.is_zero())
            {
                index.insert(Relation::Lt(value, x));
            }
        }
    }
    index
}

/// The phis of loop headers that can influence a branch, as `(header, value, incoming)`.
fn header_phis<'a>(
    func: &'a Function,
    cfg: &'a CfgInfo,
    relevant: &'a DenseBitSet<ValueId>,
) -> impl Iterator<Item = (BlockId, ValueId, &'a [(BlockId, ValueId)])> + 'a {
    cfg.cyclic_blocks().iter().flat_map(move |header| {
        func.blocks[header].instructions.iter().filter_map(move |&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let value = func.inst_result_value(inst)?;
            relevant.contains(value).then_some((header, value, incoming.as_slice()))
        })
    })
}

/// A loop header phi `p = phi [pre: initial], [latch: next]` with one input from outside its
/// loop and one from a latch its header dominates.
#[derive(Clone, Copy)]
struct LoopPhi {
    header: BlockId,
    value: ValueId,
    initial: ValueId,
    next: ValueId,
}

/// The loop phis among the header phis that can influence a branch.
fn loop_phis(func: &Function, cfg: &CfgInfo, relevant: &DenseBitSet<ValueId>) -> Vec<LoopPhi> {
    let dominators = cfg.dominators();
    header_phis(func, cfg, relevant)
        .filter_map(|(header, value, incoming)| {
            let &[(first_block, first), (second_block, second)] = incoming else { return None };
            let (pre, initial, latch, next) = match (
                dominators.dominates(header, first_block),
                dominators.dominates(header, second_block),
            ) {
                (false, true) => (first_block, first, second_block, second),
                (true, false) => (second_block, second, first_block, first),
                _ => return None,
            };
            let predecessors = &func.blocks[header].predecessors;
            (predecessors.contains(&pre) && predecessors.contains(&latch)).then_some(LoopPhi {
                header,
                value,
                initial,
                next,
            })
        })
        .collect()
}

/// Finds loop phis `p = phi [pre: init], [latch: p - c]` or `[latch: p + c]` with a constant
/// nonzero step.
fn monotone_phi_candidates(
    func: &Function,
    definitions: &IndexVec<InstId, Option<BlockId>>,
    phis: &[LoopPhi],
) -> Vec<MonotonePhi> {
    phis.iter()
        .filter_map(|&LoopPhi { header, value, initial, next }| {
            let Value::Inst(next_inst) = func.value(next) else { return None };
            let (step, decreasing) = match func.inst(*next_inst).kind {
                InstKind::Sub(base, step) if base == value => (step, true),
                InstKind::Add(base, step) if base == value => (step, false),
                InstKind::Add(step, base) if base == value => (step, false),
                _ => return None,
            };
            if func.value_u256(step)?.is_zero() {
                return None;
            }
            let home = definitions[*next_inst]?;
            Some(MonotonePhi { header, value, initial, next, step, home, decreasing })
        })
        .collect()
}

/// Finds header phis with a single incoming value from outside their loop that a backedge may
/// change other than by a constant step, as candidate bounds on both sides of that value, and on
/// the side the loop tests of every value defined before the loop that it compares the phi with:
/// `p < n`, `p == n`, and `p != n` suggest `p <= n`; `n < p` suggests `n <= p`. Monotone phis have
/// their own proof, and a zero start bounds nothing useful.
fn header_bound_candidates(
    func: &Function,
    cfg: &CfgInfo,
    definitions: &IndexVec<InstId, Option<BlockId>>,
    relevant: &DenseBitSet<ValueId>,
    monotone: &[MonotonePhi],
) -> Vec<HeaderBound> {
    const MAX_HEADER_BOUNDS: usize = 12;
    let mut bounds = Vec::new();
    let dominators = cfg.dominators();
    let cyclic = cfg.cyclic_blocks();
    // Whether `value` is defined before the loop of `header`, so it is fixed while the loop runs.
    let before = |value: ValueId, header: BlockId| match func.value(value) {
        Value::Inst(inst) => definitions[*inst]
            .is_some_and(|block| block != header && dominators.dominates(block, header)),
        _ => true,
    };
    let mut found = SmallVec::<[(ValueId, bool); 4]>::new();
    for (header, value, incoming) in header_phis(func, cfg, relevant) {
        if monotone.iter().any(|phi| phi.value == value) {
            continue;
        }
        let mut outside =
            incoming.iter().filter(|&&(block, _)| !dominators.dominates(header, block));
        let (Some(&(_, initial)), None) = (outside.next(), outside.next()) else { continue };
        if incoming.iter().all(|&(_, incoming)| incoming == value || incoming == initial) {
            continue;
        }
        found.clear();
        if !func.value_u256(initial).is_some_and(|initial| initial.is_zero()) {
            found.extend([(initial, true), (initial, false)]);
        }
        for block in cyclic.iter().filter(|&block| dominators.dominates(header, block)) {
            for &compare in &func.blocks[block].instructions {
                let (limit, upper) = match func.inst(compare).kind {
                    InstKind::Lt(p, n) | InstKind::Gt(n, p) if p == value => (n, true),
                    InstKind::Lt(n, p) | InstKind::Gt(p, n) if p == value => (n, false),
                    InstKind::Eq(a, b) | InstKind::Ne(a, b) if a == value => (b, true),
                    InstKind::Eq(a, b) | InstKind::Ne(a, b) if b == value => (a, true),
                    _ => continue,
                };
                if before(limit, header) && !found.contains(&(limit, upper)) {
                    found.push((limit, upper));
                }
            }
        }
        for &(limit, upper) in &found {
            if bounds.len() == MAX_HEADER_BOUNDS {
                return bounds;
            }
            bounds.push(HeaderBound { header, value, limit, upper });
        }
    }
    bounds
}

/// The values entering each bounded phi, with the blocks whose scope must prove they keep the
/// bound: the value from outside the loop unless it is the limit, and the values the backedges
/// carry. A phi joining paths inside the loop keeps the bound when each of its incoming values
/// does at the end of its predecessor; the bounded phi itself keeps it by assumption.
///
/// A join that several paths reach expands once at each depth, and each obligation is kept once,
/// so nested joins add their incoming values rather than multiplying them. The obligations are
/// sorted by block, for [`CheckEliminator::check_obligations`] to find the ones of a block.
fn bound_obligations(
    func: &Function,
    cfg: &CfgInfo,
    definitions: &IndexVec<InstId, Option<BlockId>>,
    bounds: &[HeaderBound],
    obligations: &mut Vec<BoundObligation>,
) {
    const MAX_JOIN_DEPTH: usize = 4;
    let dominators = cfg.dominators();
    obligations.clear();
    let mut kept = FxIndexSet::default();
    let mut expanded = FxHashSet::default();
    let mut pending = Vec::new();
    for (bound, &HeaderBound { header, value: phi, limit, .. }) in bounds.iter().enumerate() {
        let Some(InstKind::Phi(incoming)) = inst_kind(func, phi) else { continue };
        kept.clear();
        expanded.clear();
        for &(block, value) in incoming {
            if !dominators.dominates(header, block) && value != limit {
                kept.insert((block, value));
            }
        }
        pending.clear();
        pending.extend(
            incoming
                .iter()
                .filter(|&&(block, _)| dominators.dominates(header, block))
                .map(|&(block, value)| (block, value, 0)),
        );
        while let Some((block, value, depth)) = pending.pop() {
            if value == phi {
                continue;
            }
            if depth < MAX_JOIN_DEPTH
                && let Value::Inst(inst) = func.value(value)
                && let InstKind::Phi(joined) = &func.inst(*inst).kind
                && let Some(join) = definitions[*inst]
                && join != header
                && dominators.dominates(header, join)
            {
                if expanded.insert((*inst, depth)) {
                    pending.extend(joined.iter().map(|&(block, value)| (block, value, depth + 1)));
                }
                continue;
            }
            kept.insert((block, value));
        }
        obligations.extend(kept.drain(..).map(|(block, value)| BoundObligation {
            bound,
            block,
            value,
        }));
    }
    obligations.sort_by_key(|obligation| obligation.block);
}

/// Finds the midpoints feeding branches: `(a + b) / 2` and `(a + b) >> 1`, and the ordered
/// `a + (b - a) / 2` and `a + ((b - a) >> 1)`.
fn midpoints(func: &Function, relevant: &DenseBitSet<ValueId>) -> FxIndexMap<InstId, Midpoint> {
    let halved = |value: ValueId| match *inst_kind(func, value)? {
        InstKind::Div(x, two) if func.value_u256(two) == Some(U256::from(2)) => Some(x),
        InstKind::Shr(one, x) if func.value_u256(one) == Some(U256::ONE) => Some(x),
        _ => None,
    };
    let sum = |value: ValueId| match *inst_kind(func, value)? {
        InstKind::Add(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(_),
            lhs: a,
            rhs: b,
        } => Some((a, b)),
        _ => None,
    };
    let mut found = FxIndexMap::default();
    for inst in func.instructions() {
        let Some(value) = func.inst_result_value(inst) else { continue };
        if !relevant.contains(value) {
            continue;
        }
        if let Some(total) = halved(value)
            && let Some((low, high)) = sum(total)
        {
            found.insert(inst, Midpoint { value, low, high, sum: Some(total) });
        } else if let Some((x, y)) = sum(value) {
            for (low, half) in [(x, y), (y, x)] {
                if let Some(span) = halved(half)
                    && let Some((high, base, _)) = difference_operands(func, span)
                    && base == low
                {
                    found.insert(inst, Midpoint { value, low, high, sum: None });
                    break;
                }
            }
        }
    }
    found
}

/// Returns the fact implied on the unique dominating edge into `block`:
/// the branch condition of its sole predecessor and whether it is true.
fn dominating_edge_fact(func: &Function, block: BlockId) -> Option<(ValueId, bool)> {
    let &[first] = func.blocks[block].predecessors.as_slice() else { return None };
    let Terminator::Branch { condition, then_block, else_block } =
        func.blocks[first].terminator.as_ref()?
    else {
        return None;
    };
    // A branch with both arms on `block` implies nothing.
    if then_block == else_block {
        return None;
    }
    if *then_block == block {
        Some((*condition, true))
    } else if *else_block == block {
        Some((*condition, false))
    } else {
        None
    }
}

/// The constant shift count of a shift operand below the word width, if known.
fn shift_amount(
    eliminator: &mut CheckEliminator<'_>,
    func: &Function,
    shift: ValueId,
    depth: usize,
) -> Option<usize> {
    let range = eliminator.range_of(func, shift, depth);
    (range.is_singleton() && range.lo < U256::from(256)).then(|| range.lo.to::<usize>())
}

/// Recognizes `x + x` and `x + x + 1`, the shapes a checked multiplication by
/// two takes after canonicalization, as `(x, adds_one)`.
fn doubled(func: &Function, value: ValueId) -> Option<(ValueId, bool)> {
    match *inst_kind(func, value)? {
        InstKind::Add(a, b) if a == b => Some((a, false)),
        InstKind::Add(a, b) => {
            let inner = if func.value_u256(b) == Some(U256::from(1)) {
                a
            } else if func.value_u256(a) == Some(U256::from(1)) {
                b
            } else {
                return None;
            };
            match *inst_kind(func, inner)? {
                InstKind::Add(x, y) if x == y => Some((x, true)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The values a counting header phi can take within the target's trip bound:
/// `initial + k * step` (or `initial - k * step`) for at most
/// `2^MAX_TRIP_COUNT_BITS` latch executions, when both constants are known
/// and the far end fits in a word.
fn trip_count_bound(func: &Function, phi: &MonotonePhi) -> Option<Range> {
    let initial = func.value_u256(phi.initial)?;
    let step = func.value_u256(phi.step)?;
    if phi.decreasing {
        let travel = step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?;
        Some(Range::new(initial.checked_sub(travel)?, initial))
    } else {
        ascending_trip_bound(func, phi.value, initial, step)
    }
}

/// The values a counter of `value`'s type from `initial` can take within the target's trip
/// bound, advancing by at most `step` per iteration, when the far end does not wrap.
fn ascending_trip_bound(
    func: &Function,
    value: ValueId,
    initial: U256,
    step: U256,
) -> Option<Range> {
    let travel = step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?;
    let far = initial.checked_add(travel).filter(|&far| far <= integer_max(func, value))?;
    Some(Range::new(initial, far))
}

/// The operand and the constant of `and x, C`.
fn masked_input(func: &Function, kind: &InstKind) -> Option<(ValueId, U256)> {
    let InstKind::And(a, b) = *kind else { return None };
    func.value_u256(b).map(|mask| (a, mask)).or_else(|| func.value_u256(a).map(|mask| (b, mask)))
}

/// The value `x` of `and x, MAX << low`, which clears the low `low` bits, and `low`.
fn high_mask_input(func: &Function, kind: &InstKind) -> Option<(ValueId, usize)> {
    let (value, mask) = masked_input(func, kind)?;
    let low = mask.trailing_zeros();
    (low > 0 && low < 256 && mask == U256::MAX << low).then_some((value, low))
}

/// The constant left shifts `shl x, k`, as `(k, result)`, that prove some high mask on `x`
/// redundant when they are zero: `k < 256` and `256 - k` at least the mask's `low`.
fn shifted_high_mask_inputs(
    func: &Function,
) -> FxHashMap<ValueId, SmallVec<[(usize, ValueId); 2]>> {
    let mut lows = FxHashMap::<_, usize>::default();
    for id in func.instructions() {
        if let Some((value, low)) = high_mask_input(func, &func.inst(id).kind) {
            let min = lows.entry(value).or_insert(low);
            *min = (*min).min(low);
        }
    }
    let mut shifts = FxHashMap::<_, SmallVec<_>>::default();
    if lows.is_empty() {
        return shifts;
    }
    for id in func.instructions() {
        if let InstKind::Shl(shift, value) = func.inst(id).kind
            && let Some(&low) = lows.get(&value)
            && let Some(shift) = func.value_u64(shift)
            && shift < 256
            && 256 - shift as usize >= low
            && let Some(result) = func.inst_result_value(id)
        {
            shifts.entry(value).or_default().push((shift as usize, result));
        }
    }
    shifts
}

fn low_mask_input(func: &Function, kind: &InstKind) -> Option<(ValueId, U256)> {
    let (value, mask) = masked_input(func, kind)?;
    (mask.wrapping_add(U256::ONE) & mask == U256::ZERO).then_some((value, mask))
}

/// An `eq` or `ne` of a value with an immediate that `target` prices as cheap to read, whose
/// edges prove the value that constant.
fn compares_with_immediate(func: &Function, kind: &InstKind, target: &Target) -> bool {
    let (InstKind::Eq(a, b) | InstKind::Ne(a, b)) = *kind else { return false };
    func.value_u256(a).xor(func.value_u256(b)).is_some_and(|value| target.is_cheap_immediate(value))
}

/// The word `v` of `zext(v != 0)`, which is `v` itself whenever `v` is 0 or 1, as a
/// boolean that ABI decoding already checked is.
fn widened_nonzero_test(func: &Function, kind: &InstKind) -> Option<ValueId> {
    let InstKind::Zext(test) = *kind else { return None };
    inst_kind(func, test)?.nonzero_test_operand(func)
}

/// The values a header phi `p = phi [pre: initial], [latch: next]` can take when every value
/// `next` can be, through phis and selects inside the loop, is `p` itself or `p + c` for a
/// constant `c`, as a counter that only some iterations advance is: within the target's trip
/// bound, `p` moves at most `2^MAX_TRIP_COUNT_BITS` times the largest step from a constant
/// start, when the far end fits in `p`'s type.
fn counter_bounds<'a>(
    func: &'a Function,
    phis: &'a [LoopPhi],
) -> impl Iterator<Item = (ValueId, Range)> + 'a {
    phis.iter().filter_map(|&LoopPhi { value, initial, next, .. }| {
        let initial = func.value_u256(initial)?;
        let step = largest_step(func, value, next)?;
        Some((value, ascending_trip_bound(func, value, initial, step)?))
    })
}

/// The largest constant `next` adds to the header phi `phi`, when every value `next` can be,
/// through phis and selects, is `phi` itself or `phi + c` for a constant `c`, and some adds. A
/// phi seen twice, as an inner loop's header, fails the walk.
fn largest_step(func: &Function, phi: ValueId, next: ValueId) -> Option<U256> {
    const MAX_VALUES: usize = 32;
    let mut largest = U256::ZERO;
    let mut pending = vec![next];
    let mut seen = FxHashSet::default();
    while let Some(value) = pending.pop() {
        if value == phi {
            continue;
        }
        if !seen.insert(value) || seen.len() > MAX_VALUES {
            return None;
        }
        match *inst_kind(func, value)? {
            InstKind::Add(a, b) if a == phi => largest = largest.max(func.value_u256(b)?),
            InstKind::Add(a, b) if b == phi => largest = largest.max(func.value_u256(a)?),
            InstKind::Phi(ref incoming) => pending.extend(incoming.iter().map(|&(_, value)| value)),
            InstKind::Select(_, a, b) => pending.extend([a, b]),
            _ => return None,
        }
    }
    (!largest.is_zero()).then_some(largest)
}

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    }
}

/// The value and constant of a sum `value + constant` with a nonzero constant.
fn constant_offset(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
    let InstKind::Add(x, y) = *inst_kind(func, value)? else { return None };
    match (func.value_u256(x), func.value_u256(y)) {
        (None, Some(c)) if !c.is_zero() => Some((x, c)),
        (Some(c), None) if !c.is_zero() => Some((y, c)),
        _ => None,
    }
}

/// Returns true if both values are the same SSA value or the same constant.
fn values_equal(func: &Function, a: ValueId, b: ValueId) -> bool {
    if a == b {
        return true;
    }
    match (func.value_u256(a), func.value_u256(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The operation, width, and operands of a semantic unsigned checked operation that
/// `check-elim` can prove does not fail.
fn checked_operands(kind: &InstKind) -> Option<(CheckedOp, u16, ValueId, ValueId)> {
    let InstKind::CheckedBinary { op, arithmetic: ArithmeticKind::Unsigned(bits), lhs, rhs } =
        *kind
    else {
        return None;
    };
    matches!(op, CheckedOp::Add | CheckedOp::Sub | CheckedOp::Mul | CheckedOp::Div | CheckedOp::Rem)
        .then_some((op, bits, lhs, rhs))
}

/// The value `value` is one more than, as `s + 1`, plain or checked.
fn successor_of(func: &Function, value: ValueId) -> Option<ValueId> {
    let (a, b) = match *inst_kind(func, value)? {
        InstKind::Add(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(_),
            lhs: a,
            rhs: b,
        } => (a, b),
        _ => return None,
    };
    if func.value_u256(b) == Some(U256::ONE) {
        Some(a)
    } else if func.value_u256(a) == Some(U256::ONE) {
        Some(b)
    } else {
        None
    }
}

/// The minuend and subtrahend of a subtraction, and whether it is exact: a checked one fails
/// rather than wrap.
fn difference_operands(func: &Function, value: ValueId) -> Option<(ValueId, ValueId, bool)> {
    match *inst_kind(func, value)? {
        InstKind::Sub(x, y) => Some((x, y, false)),
        InstKind::CheckedBinary {
            op: CheckedOp::Sub,
            arithmetic: ArithmeticKind::Unsigned(_),
            lhs,
            rhs,
        } => Some((lhs, rhs, true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::parser::parse_module;
    use solar_interface::{ColorChoice, Session};
    use std::fmt::Write;

    /// A loop whose backedge value reaches the header through `levels` joins of `fan` paths each,
    /// every path into a join carrying the value from the join before it.
    fn fanned_joins(fan: usize, levels: usize) -> String {
        let mut text = String::from(
            "@module Joins\n\nfn @f(arg0: i256, arg1: i256) -> i256 {\n  bb0:\n    v0 = add arg0, 1\n    jump bb1\n",
        );
        let latch = 2 + levels * (fan + 1);
        let exit = latch + 1;
        writeln!(text, "  bb1:\n    v1 = phi [bb0: 0], [bb{latch}: v{}]", 3 + levels).unwrap();
        writeln!(
            text,
            "    v2 = lt v1, v0\n    jumpi v2, bb2, bb{exit}\n  bb2:\n    v3 = add v1, 1"
        )
        .unwrap();
        for level in 1..=levels {
            // switch arg1 -> fan blocks -> join
            let first = 3 + (level - 1) * (fan + 1);
            let join = first + fan;
            let cases: Vec<_> =
                (1..fan).map(|case| format!("{case} => bb{}", first + case)).collect();
            writeln!(text, "    switch arg1, default bb{first}, [{}]", cases.join(", ")).unwrap();
            for block in first..join {
                writeln!(text, "  bb{block}:\n    jump bb{join}").unwrap();
            }
            let incoming: Vec<_> =
                (first..join).map(|block| format!("[bb{block}: v{}]", 2 + level)).collect();
            writeln!(text, "  bb{join}:\n    v{} = phi {}", 3 + level, incoming.join(", "))
                .unwrap();
        }
        writeln!(text, "    jump bb1\n  bb{exit}:\n    ret v1\n}}").unwrap();
        text
    }

    #[test]
    fn join_obligations_count_each_once() {
        let fan = 32;
        let text = fanned_joins(fan, 4);
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let module = parse_module(&sess, &text).expect("the joins parse");
            let func = module.functions.iter().next().expect("one function");
            let cfg = CfgInfo::new(func);
            // The header holds the only two-way phi; the limit is the entry's first value.
            let (header, phi) = func
                .blocks
                .iter_enumerated()
                .find_map(|(block, data)| {
                    let &inst = data.instructions.first()?;
                    matches!(&func.inst(inst).kind, InstKind::Phi(incoming) if incoming.len() == 2)
                        .then(|| (block, func.inst_result_value(inst).unwrap()))
                })
                .expect("a header phi");
            let limit =
                func.inst_result_value(func.blocks[BlockId::ENTRY].instructions[0]).unwrap();
            let bound = HeaderBound { header, value: phi, limit, upper: true };
            let mut obligations = Vec::new();
            bound_obligations(func, &cfg, &func.inst_block_table(), &[bound], &mut obligations);
            // The entry's zero, and the body's value at the end of each path into the first join,
            // however many paths reach that join.
            assert_eq!(obligations.len(), fan + 1);
        });
    }
}
