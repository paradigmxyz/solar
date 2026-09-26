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
//!   `(C-1) + K`) fits in 256 bits, so the `div`-based mul check or the add check folds.
//!
//! The pass walks the dominator tree. On entry to a block with a unique
//! predecessor ending in a two-way branch, it records what the branch
//! condition implies on that edge: value ranges refined by constants
//! (`x < C` => `x` in `[0, C-1]`) and relational predicates between SSA
//! values (`!(a < b)` => `b <= a`). Facts attach to SSA values, which are
//! never redefined, so a fact derived on a dominating edge holds in every
//! dominated block. Branch conditions are then evaluated against the
//! recorded facts with checked 256-bit arithmetic; a condition that is
//! provably constant folds the branch to an unconditional jump, and the dead
//! panic block is cleaned up by the existing CFG passes. Anything that is
//! not provable is left untouched. A width test spelled as a shift, `x >> k`
//! being zero, bounds `x` below `2^k`, and a zero-extended test is zero
//! exactly when the test is false, so failure words that `or` several tests
//! still give each test's fact on the passing edge. Explicit integer casts retain range facts only
//! when their width and sign semantics preserve the bounded values. Semantic checks use the same
//! facts in instruction order: a passing check refines all later execution, and a proven passing
//! check can be removed before expansion. Facts roll back on leaving each dominator subtree, so a
//! check on one conditional path cannot justify removing a check on another.
//!
//! The same facts retire cleanups that change nothing in their scope: an `and`
//! with a low mask `2^k - 1` of a value bounded by the mask, and a `signextend`
//! of a value below its sign bit, give way to their operand. These are mostly
//! the ABI cleanups of a narrowing cast's result behind its own range check.
//! A branch condition that the code it guards uses again, such as a length
//! test that also selects a mask, is replaced by its truth wherever the scope
//! decides it, so `zext(n < 32) - 1` is zero behind `n < 32` and the condition
//! need not stay live into the arm.
//! A block that calls a function which never returns to its caller, because
//! every path reverts, stops, returns from the external call, or tail-calls
//! such a function, contributes no edge: a failing arm that calls a revert
//! helper and falls into the join leaves the passing edge's facts intact.
//!
//! Before the dominator walk, a bounded forward analysis carries the intersection
//! of relational facts and the union of ranges across predecessor edges. Phi
//! ranges are evaluated in their incoming edge contexts. All states start at
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
//! A loop header phi's range leaves out the paths that carry it around the loop
//! unchanged: every visit holds the preheader value or a value an earlier
//! iteration stored, so the union of the stores' edge ranges covers it by
//! induction over the header's visits. Phis between a store and the latch are
//! decomposed into their incoming edges. This bounds a binary search's `low`
//! and `high`, which one path updates while the other carries them.
//!
//! Loop header phis of the form `p = phi [pre: init], [latch: p - c]` with a
//! constant nonzero step are monotone when the update provably cannot wrap at
//! its definition: `p <= init` then holds on every iteration, and `p >= init`
//! for the additive form. The walk first proves wrap freedom in the update's
//! scope without assuming the invariant, then adds the relation to the header's
//! entry facts and walks again. This establishes `j <= i < length` for a
//! descending inner index initialized from a bounded outer counter.
//!
//! A header phi whose updates are not constant steps is monotone when each
//! update is proven, in the scope computing it, to stay on the phi's side of its
//! current value. A binary search's `high = mid - 1` stays below `high` because
//! the midpoint lies between the bounds, which establishes `high <= length`. The
//! midpoint fact itself is recorded where `(x + y) >> 1` is defined: when the
//! scope orders `x <= y` and the sum cannot wrap, it lies between them.
//!
//! When both the start and the step of such a phi are constants, the phi also
//! carries a range bound from the target's trip-count limit: a counter cannot
//! travel further than `step << MAX_TRIP_COUNT_BITS` because every iteration
//! burns gas that fits in 64 bits. That bound is intersected into every range
//! query for the phi, so `i + 2` and `i + 4` never wrap for a counter that
//! starts at zero even when the loop bound is an arbitrary word. The revert
//! path such a check guards is unreachable by any transaction, not merely
//! unlikely, so removing it preserves the checked semantics.
//!
//! Paired zero-based cursors also expose a scaled capacity invariant. When an
//! input cursor advances by a constant `S`, an output cursor advances by at
//! most `K * S`, and a checked allocation reserves `input_length * K`,
//! induction proves every write of at most `K * S` bytes remains within that
//! allocation. The proof requires the same natural-loop header/backedge, an
//! active guard (`input < input_length` for a unit step, `input + S <=
//! input_length` otherwise, found past the header's own checks), an exact
//! capacity product or doubling, and alias analysis showing that the input
//! length and output capacity are stable throughout the loop. Unknown writes,
//! wider output steps, multiple latches, or logical-length mutations retain
//! their checks.
//!
//! Two more universal equalities feed those proofs. A checked `u256` sum equals
//! a wrapping sum of the same operands wherever it is defined, so a loop test
//! on `i + 16` covers a bounds check that adds 16 to `i` again. In a module
//! without inline assembly, rereads of a parameter object's length agree, and
//! a fresh object's rereads equal the length stored at its allocation, when
//! nothing in the function can write that length: stores into other fresh
//! objects cannot reach it, nor can writes that end at or below the zero slot,
//! such as the scratch words a slot hash writes, a write in a block that leaves
//! the function reaches only later reads in that block, and calls count
//! through their memory summaries. The rereads themselves stay in place for the scheduler to
//! price. A comparison of two sums with constant offsets also reads a reread
//! length as the checked sum stored at allocation, which cannot wrap, so
//! `31 + (n & ~31)` stays within a buffer of `(n & 255) + 32` bytes.
//!
//! Transitive relational queries lazily index candidate edges once per function,
//! when a query needs to combine facts. An edge is followed only
//! when its fact is present in the current scope; the index itself proves
//! nothing. Derived values that never exceed their source (right shifts, masks,
//! remainders, divisions by a nonzero constant) contribute universal `<=` edges
//! that hold in every scope, so `i < length / 2` reaches `i < length`. Two
//! masks of one word are ordered the same way when one mask's bits are a
//! subset of the other's, as `x & ~31 <= x & 255` after folding merged the
//! masks that separated a rounded length from the word it came from. So does
//! an if-converted minimum `b + (a < b) * (a - b)`, below both `a` and `b`,
//! which lets `i < min(x.length, y.length)` reach both lengths. A value
//! with a strict path below it in the current scope is at least one, which
//! folds `length == 0` guards and the `length - 1` underflow check that follow
//! `i < length / 2`; differences inherit the strict bound of their minuend. Search follows at most
//! 128 states in stable block and operand order, carrying whether an unsigned ordering path
//! contains a strict edge. Exhausting this bound leaves the check in place; disequality is never
//! treated as transitive.
//!
//! Bitwise `or` and `xor` stay below the next power of two above both
//! operands' bounds, and `or` stays at or above each operand, so lane sums over
//! bytes mixed that way and bit counts built from table lookups stay bounded.
//!
//! Checked scaling by a power of two tests that `(x << k) >> k == x`; the test
//! holds whenever `x`'s range leaves its top `k` bits clear, so an allocation
//! sized from a bounded length drops it. A checked product
//! `or (eq y, 0), (eq (div (mul x, y), y), x)` holds whenever the bounds of `x`
//! and `y` multiply without wrapping; its first disjunct covers a zero `y`. A
//! difference `a - b` taken where the scope orders `b <= a` cannot wrap, so it
//! stays below `a`'s bound even when the operands' ranges overlap. Together they
//! drop the capacity checks of a replacement sized from bounded lengths.
//!
//! Signed comparisons rotate intervals by the sign bit to use the same ordered
//! bounds. An interval crossing the rotation boundary widens to unknown; signed
//! facts only become unsigned relations when both operands have the same known
//! sign. These bounds use the existing join and dominance scopes.
//!
//! A module without inline assembly bounds every memory object's logical length by the
//! allocation limit: each length was written by a checked allocation, an ABI decoder, or a core
//! operation that only shortens an object. Assembly can store any word in a length word, so a
//! module with any function carrying assembly, directly or inlined, keeps lengths unknown.
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
//! words keep their declared width; unsigned layouts retain their i256 SSA carrier.
//! This exposes facts hidden behind getter calls during the ordinary earlier check passes.

//! The `late-check-elim` adapter revisits conditions unified by CSE after memory
//! lowering. Gas mode only removes redundant failure edges from blocks on a CFG
//! cycle: repeated checks repay their removal each iteration, while other changes can disrupt
//! shared ABI encoder tails and increase both size and gas. Size mode uses the
//! full cleanup. Cycle membership is only a profitability filter. Gas mode uses
//! dominator-scoped facts, sufficient for conditions unified by CSE, and leaves
//! fixed-point range propagation to the earlier check passes. Size mode retains
//! the full forward analysis. Both use the existing conservative proof logic.
//! Run it after the post-memory CSE. Only functions with removed checks receive
//! CFG cleanup, avoiding unrelated late block merges in other functions.

use super::{call_cleanup, cfg_simplify::simplify_function, egraph::max_bits_with_args};
use crate::{
    mir::{
        ArithmeticKind, BlockId, Builtin, Callee, CheckedOp, Function, FunctionId, Immediate,
        ImmutableEncoding, ImmutableId, InstId, InstKind, Module, Terminator, TypeSize, Value,
        ValueId, ValueLayout,
        analysis::{
            Access, AddressSpace, AliasAnalysis, CallGraphInfo, CfgInfo, Location,
            MemoryCallSummaries,
        },
        immutable::immutable_push_type_size,
        memory::EvmMemoryLayout,
        pass::{
            MirPass, run_function_pass, run_function_pass_with_cfg, run_selected_function_pass,
        },
        utils::{self as mir_utils, fold_terminator_to_jump},
    },
    target::Target,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::{rc::Rc, sync::Arc};

/// Function pass for range-based overflow-check elimination.
pub(crate) struct CheckElim;

impl MirPass for CheckElim {
    fn name(&self) -> &'static str {
        "check-elim"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let object_lengths = object_length_bound(module);
        let summaries = object_lengths.is_some().then(|| analyses.call_summaries(module));
        let never_returning = Arc::new(never_returning(module));
        run_function_pass(module, analyses, |func, _| {
            let mut eliminator = CheckEliminator::new(None, object_lengths);
            eliminator.call_summaries.clone_from(&summaries);
            eliminator.never_returning = Some(Arc::clone(&never_returning));
            eliminator.run(func) != 0
        })
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
        let reverting = module
            .functions
            .iter_enumerated()
            .filter_map(|(id, func)| {
                leads_to_revert(func, BlockId::ENTRY, &FxHashSet::default()).then_some(id)
            })
            .collect::<FxHashSet<_>>();
        let object_lengths = object_length_bound(module);
        let never_returning = Arc::new(never_returning(module));
        run_function_pass_with_cfg(module, analyses, |func, analyses| {
            let selected =
                gcx.sess.opts.optimization.is_gas().then(|| analyses.cfg().cyclic_blocks());
            if selected.is_some_and(DenseBitSet::is_empty) {
                return false;
            }
            let mut eliminator = CheckEliminator::new(None, object_lengths);
            eliminator.cfg = Some(Rc::clone(analyses.cfg()));
            eliminator.never_returning = Some(Arc::clone(&never_returning));
            let changed =
                eliminator.run_in_blocks(func, selected.map(|blocks| (blocks, &reverting))) != 0;
            if changed {
                // branch proven_condition, checked, panic => jump checked
                // Remove unreachable panic blocks and merge the successful continuation.
                let _ = simplify_function(func);
            }
            changed
        })
    }
}

/// Functions that never return to their caller: no path reaches an internal
/// `return`, and every tail call goes to another such function. Calling one
/// ends the frame by reverting, stopping, returning from the external call or
/// looping, so the caller's code after the call never runs.
fn never_returning(module: &Module) -> FxHashSet<FunctionId> {
    let mut returning = DenseBitSet::new_empty(module.functions.len());
    loop {
        let mut changed = false;
        for (id, func) in module.functions.iter_enumerated() {
            if returning.contains(id) {
                continue;
            }
            let returns = func.blocks.iter().any(|block| match &block.terminator {
                Some(Terminator::Return { .. }) => true,
                Some(Terminator::TailCall { function, .. }) => returning.contains(*function),
                _ => false,
            });
            if returns {
                returning.insert(id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    module.functions.indices().filter(|&id| !returning.contains(id)).collect()
}

/// Recognizes short unconditional failure paths, including outlined revert helpers.
/// This only selects profitable candidates; range analysis proves the edge unreachable.
fn leads_to_revert(func: &Function, mut block: BlockId, reverting: &FxHashSet<FunctionId>) -> bool {
    // Bound classification work and leave cycles or longer paths unclassified.
    for _ in 0..8 {
        match func.blocks[block].terminator {
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata) => return true,
            Some(Terminator::TailCall { function, .. }) => return reverting.contains(&function),
            Some(Terminator::Jump(next)) => block = next,
            _ => return false,
        }
    }
    false
}

/// Applies runtime immutable bounds after getter inlining exposes their loads.
/// The range every memory object's logical length stays in, when the module guarantees one.
///
/// Without inline assembly, each length was written by a checked allocation, an ABI decoder,
/// or a core operation that only shortens an object, so the object and its header fit below
/// the allocation limit. Assembly can store any word in a length, so any function carrying it
/// leaves lengths unknown. Removed functions no longer run, and inlining keeps the bit on the
/// callers that received their code.
fn object_length_bound(module: &Module) -> Option<Range> {
    (!module.functions.iter().any(|func| func.attributes.inline_assembly))
        .then(|| Range::new(U256::ZERO, U256::from(EvmMemoryLayout::MAX_ALLOCATION_END)))
}

pub(crate) struct ImmutableCheckElim;

impl MirPass for ImmutableCheckElim {
    fn name(&self) -> &'static str {
        "immutable-check-elim"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let narrowed = narrow_immutable_layouts(module);
        let bounds = module
            .iter_immutables()
            .filter_map(|(id, immutable)| {
                let encoding @ ImmutableEncoding::Unsigned(_) =
                    immutable.ty.immutable_encoding()?
                else {
                    return None;
                };
                let width = immutable_push_type_size(
                    encoding,
                    gcx.sess.opts.optimization,
                    gcx.sess.opts.evm_version.has_bitwise_shifting(),
                )
                .bits();
                (width < 256).then(|| (id, Range::new(U256::ZERO, U256::MAX >> (256 - width))))
            })
            .collect::<FxHashMap<_, _>>();
        if bounds.is_empty() {
            return narrowed;
        }
        let mut runtime_only = runtime_only_functions(module);
        for id in runtime_only.iter().collect::<Vec<_>>() {
            if !module.function(id).instructions().any(|inst| {
                matches!(module.function(id).inst(inst).kind,
                    InstKind::LoadImmutable(immutable) if bounds.contains_key(&immutable))
            }) {
                runtime_only.remove(id);
            }
        }
        let object_lengths = object_length_bound(module);
        run_selected_function_pass(module, analyses, &runtime_only, |func, analyses| {
            let mut eliminator = CheckEliminator::new(Some(&bounds), object_lengths);
            eliminator.cfg = Some(Rc::clone(analyses.cfg()));
            eliminator.run(func) != 0
        }) || narrowed
    }
}

/// Shrinks unsigned encodings only when every assignment preserves all stored bits.
fn narrow_immutable_layouts(module: &mut Module) -> bool {
    let can_narrow = |ty| matches!(ty, ValueLayout::UInt(size) if size.bits() > 8);
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
    let mut changed = false;
    for (id, bits) in widths {
        let bits = bits.max(1).div_ceil(8) * 8;
        if let ValueLayout::UInt(size) = module.immutable(id).ty
            && bits < u32::from(size.bits())
        {
            module.immutable_mut(id).ty = ValueLayout::UInt(TypeSize::new_int_bits(bits as u16));
            changed = true;
        }
    }
    changed
}

/// Excludes every constructor-reachable helper, including recursive and tail-call edges.
pub(super) fn runtime_only_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let graph = CallGraphInfo::new(module);
    let roots = |constructor| {
        module.functions.iter_enumerated().filter_map(move |(id, func)| {
            let selected = if constructor {
                func.attributes.is_constructor
            } else {
                func.selector.is_some()
                    || func.attributes.is_receive
                    || func.attributes.is_fallback
                    || module.dispatch_entry() == Some(id)
            };
            selected.then_some(id)
        })
    };
    let mut runtime = graph.reachable_callees_from(roots(false));
    for root in roots(false) {
        runtime.insert(root);
    }
    let mut constructor = graph.reachable_callees_from(roots(true));
    for root in roots(true) {
        constructor.insert(root);
    }
    runtime.subtract(&constructor);
    runtime
}

/// Maximum recursion depth when evaluating value ranges and conditions.
const MAX_DEPTH: usize = 12;

/// Statistics from check elimination.
#[derive(Debug, Default, Clone)]
struct CheckElimStats {
    /// Number of branches folded to unconditional jumps.
    branches_folded: usize,
    checks_removed: usize,
    /// Number of masks and sign extensions proven to leave their operand unchanged.
    cleanups_removed: usize,
    /// Number of branch-condition uses replaced by the truth their scope proves.
    conditions_decided: usize,
}

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

    /// Rotates unsigned order into signed order, or back, widening a split interval.
    fn flip_sign(self) -> Self {
        if self.lo.bit(255) == self.hi.bit(255) {
            let sign = U256::from(1) << 255;
            Self::new(self.lo ^ sign, self.hi ^ sign)
        } else {
            Self::FULL
        }
    }

    fn union(self, other: Self) -> Self {
        Self { lo: self.lo.min(other.lo), hi: self.hi.max(other.hi) }
    }
}

/// What one dominator walk proves: branches to fold into jumps to the kept
/// target, passing checks to remove, cleanups whose result is their operand,
/// and uses of a branch condition whose truth the use's scope decides.
struct WalkProofs {
    folds: Vec<(BlockId, BlockId)>,
    checks: DenseBitSet<InstId>,
    cleanups: Vec<(ValueId, ValueId)>,
    decided: Vec<(InstId, ValueId, bool)>,
}

/// Differences indexed under their subtrahend, each entry a minuend and the
/// value holding their difference.
type DifferenceIndex = FxHashMap<ValueId, SmallVec<[(ValueId, ValueId); 2]>>;

/// A relational predicate between two SSA values.
///
/// `Lt(a, b)` means `a < b` and `Le(a, b)` means `a <= b`, both unsigned.
/// `Eq` and `Ne` are stored with operands ordered by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Facts {
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
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
    /// The predecessor carrying `initial` into the header.
    preheader: BlockId,
    /// The predecessor carrying `next` back into the header.
    latch: BlockId,
    /// The backedge update result, `value - step` or `value + step`.
    next: ValueId,
    /// The constant step operand.
    step: ValueId,
    /// The block defining `next`, whose facts decide wrap freedom.
    home: BlockId,
    /// Whether the update subtracts the step.
    decreasing: bool,
}

/// A loop-carried output cursor that advances by at most `max_step` while a
/// sibling input cursor advances by a constant `step`.
///
/// Both cursors start at zero and share the same header and backedge. Combined
/// with the loop guard, which puts `index + step <= length` in the body, and an
/// exact `capacity = length * scale` where `max_step <= scale * step`,
/// induction gives `cursor <= index * scale`. This is the checked
/// string-builder shape: every iteration can safely write at most the capacity
/// reserved for the input items it consumes.
#[derive(Clone, Debug)]
struct ScaledCursor {
    cursor: ValueId,
    index: ValueId,
    length: ValueId,
    /// The input cursor's constant step.
    step: U256,
    /// The guard fact active in the loop body: `index < length` for a unit
    /// step, or `index + step <= length`.
    guard: Relation,
    /// Whether the guard's sum wraps instead of checking, so that the proof
    /// must bound the index first.
    wrapping_sum: bool,
    preheader: BlockId,
    loop_blocks: DenseBitSet<BlockId>,
    max_step: U256,
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

/// A loop header phi whose updates never move it away from its initial value,
/// pending a proof of each update in the scope computing it.
///
/// `p = phi [pre: init], [latch: next]`, where `next` reaches the latch through
/// phis whose inputs are `p` itself or an update instruction. When every update
/// `u` satisfies `u <= p` (or `p <= u`) where it is computed, `p <= init` (or
/// `init <= p`) holds on every iteration by induction: each header visit holds
/// `init`, the previous visit's value, or an update no further from `init` than
/// that value. Unlike [`MonotonePhi`], an update need not be a constant step:
/// a binary search's `high = mid - 1` stays below `high` because `mid <= high`.
#[derive(Clone, Debug)]
struct BoundedPhi {
    /// The loop header holding the phi.
    header: BlockId,
    /// The phi result.
    value: ValueId,
    /// The incoming value from outside the loop.
    initial: ValueId,
    /// Whether every update must stay at or below the phi, rather than at or above it.
    decreasing: bool,
    /// Each update with the block defining it, whose facts decide the update's bound.
    updates: SmallVec<[(ValueId, BlockId); 2]>,
}

impl BoundedPhi {
    /// The invariant that holds once every update is known to stay bounded.
    fn relation(&self) -> Relation {
        if self.decreasing {
            Relation::Le(self.value, self.initial)
        } else {
            Relation::Le(self.initial, self.value)
        }
    }
}

/// Range-based overflow-check eliminator.
#[derive(Default)]
struct CheckEliminator<'a> {
    /// Context-independent bounds for runtime-only immutable loads.
    immutable_ranges: Option<&'a FxHashMap<ImmutableId, Range>>,
    /// Bound on every memory object's logical length, when the module guarantees one.
    object_lengths: Option<Range>,
    /// Module call summaries, which let calls that write no parameter keep its
    /// length stable.
    call_summaries: Option<Arc<MemoryCallSummaries>>,
    /// Shared CFG snapshot taken at entry, matching the previous fresh build.
    cfg: Option<Rc<CfgInfo>>,
    /// Functions that never return to their caller; a block that calls one
    /// never reaches its terminator.
    never_returning: Option<Arc<FxHashSet<FunctionId>>>,
    /// Statistics from the last run.
    stats: CheckElimStats,
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
    /// Possible outgoing facts; each candidate still requires a scoped membership check.
    relation_index: Option<FxHashMap<ValueId, SmallVec<[Relation; 2]>>>,
    /// Proven header phi invariants, indexed alongside the branch-derived candidates.
    monotone_relations: Vec<Relation>,
    /// Orderings between a derived value and its source that hold wherever the
    /// value exists: shifts, masks, and constant divisions never grow.
    universal_relations: FxHashSet<Relation>,
    /// The value each stable object-length read agrees with: the length stored
    /// right after a fresh object's allocation, or a parameter's first read.
    length_anchors: FxHashMap<ValueId, ValueId>,
    /// Relations indexed under their right operand, for lower-bound searches.
    reverse_index: Option<FxHashMap<ValueId, SmallVec<[Relation; 2]>>>,
    /// Differences indexed under their subtrahend: `b` maps to every `(a, a - b)`.
    difference_index: Option<DifferenceIndex>,
    /// Depth of the sum-bound lemma, which asks relational questions of its own.
    sum_depth: usize,
    /// Values above a strict edge, shared by queries in the same fact scope.
    strict_lower_bounds: Option<GrowableBitSet<ValueId>>,
    /// Counting header phis with constant start and step, bounded by the
    /// distance any affordable number of iterations can travel.
    trip_bounds: FxHashMap<ValueId, Range>,
    /// Structurally proved bounded output cursors in natural loops.
    scaled_cursors: Vec<ScaledCursor>,
    range_undo: Vec<(ValueId, Option<Range>)>,
    relation_undo: Vec<Relation>,
}

impl<'a> CheckEliminator<'a> {
    /// Creates a new check eliminator.
    #[must_use]
    fn new(
        immutable_ranges: Option<&'a FxHashMap<ImmutableId, Range>>,
        object_lengths: Option<Range>,
    ) -> Self {
        Self { immutable_ranges, object_lengths, ..Self::default() }
    }

    /// Runs check elimination on a function. Returns the number of folded
    /// branches.
    fn run(&mut self, func: &mut Function) -> usize {
        self.run_in_blocks(func, None)
    }

    /// Restricts the rewritten blocks while retaining all facts needed to prove their checks.
    fn run_in_blocks(
        &mut self,
        func: &mut Function,
        selected: Option<(&DenseBitSet<BlockId>, &FxHashSet<FunctionId>)>,
    ) -> usize {
        self.stats = CheckElimStats::default();
        self.strict_lower_bounds = None;
        self.relation_index = None;
        self.reverse_index = None;
        self.difference_index = None;
        self.monotone_relations.clear();
        self.universal_relations.clear();
        self.length_anchors.clear();
        self.trip_bounds.clear();
        self.scaled_cursors.clear();
        if !func.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(Terminator::Branch { then_block, else_block, .. }) if then_block != else_block
            ) || block.instructions.iter().any(|&inst| {
                matches!(
                    func.inst(inst).kind,
                    InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), .. }
                )
            })
        }) {
            return 0;
        }
        let cfg = self.cfg.as_ref().map_or_else(|| Rc::new(CfgInfo::new(func)), Rc::clone);
        let relevant = branch_inputs(func, &cfg);
        if relevant.is_empty() {
            return 0;
        }
        self.universal_relations = universal_relations(func, &relevant);
        self.universal_relations.extend(checked_sum_twins(func));
        if self.object_lengths.is_some() {
            for (read, anchor) in stable_object_lengths(func, self.call_summaries.clone()) {
                self.length_anchors.insert(read, anchor);
                let (a, b) = ordered(anchor, read);
                self.universal_relations.insert(Relation::Eq(a, b));
            }
        }

        // Predecessors recomputed from reachable terminators: facts must only
        // come from edges that can actually execute. A block that calls a
        // function that never returns ends the frame before its terminator.
        let mut preds = index_vec![Vec::new(); func.blocks.len()];
        for &block in cfg.rpo() {
            if self.calls_never_returning(func, block) {
                continue;
            }
            for &succ in cfg.successors(block) {
                preds[succ].push(block);
            }
        }

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
            self.join_facts(func, &cfg, &preds, &relevant)
        };
        let candidates = monotone_phi_candidates(func, &cfg, &preds, &relevant);
        let bounded = bounded_phi_candidates(func, &cfg, &preds, &relevant, &candidates);
        self.scaled_cursors = scaled_cursor_candidates(func, &cfg, &preds, &candidates);
        for phi in &candidates {
            if let Some(bound) = trip_count_bound(func, phi) {
                self.trip_bounds.insert(phi.value, bound);
            }
        }
        let mut proven = Vec::new();
        let mut bounded_proofs = vec![0; bounded.len()];
        let mut proofs = self.collect_folds(
            func,
            &cfg,
            &preds,
            &facts,
            (&candidates, &mut proven),
            (&bounded, &mut bounded_proofs),
        );
        let invariants = proven
            .iter()
            .map(|phi| (phi.header, phi.value, phi.initial, phi.decreasing, phi.relation()))
            .chain(
                bounded
                    .iter()
                    .zip(&bounded_proofs)
                    .filter(|&(phi, &proofs)| proofs == phi.updates.len())
                    .map(|(phi, _)| {
                        (phi.header, phi.value, phi.initial, phi.decreasing, phi.relation())
                    }),
            )
            .collect::<Vec<_>>();
        if !invariants.is_empty() {
            // The invariant is available wherever the phi is: attach it to the
            // header's entry facts and index it for transitive queries.
            for (header, value, initial, decreasing, relation) in invariants {
                facts[header].relations.insert(relation);
                if let Some(initial) = const_of(func, initial) {
                    let bound = if decreasing {
                        Range::new(U256::ZERO, initial)
                    } else {
                        Range::new(initial, U256::MAX)
                    };
                    let ranges = &mut facts[header].ranges;
                    let narrowed = ranges
                        .get(&value)
                        .map_or(bound, |range| range.intersect(bound).unwrap_or(*range));
                    ranges.insert(value, narrowed);
                }
                self.monotone_relations.push(relation);
            }
            self.relation_index = None;
            self.reverse_index = None;
            self.strict_lower_bounds = None;
            proofs = self.collect_folds(
                func,
                &cfg,
                &preds,
                &facts,
                (&[], &mut Vec::new()),
                (&[], &mut []),
            );
        }
        let WalkProofs { mut folds, checks, cleanups, decided } = proofs;
        if let Some((selected, reverting)) = selected {
            folds.retain(|&(block, keep)| {
                if selected.contains(block)
                    && let Some(Terminator::Branch { then_block, else_block, .. }) =
                        func.blocks[block].terminator
                {
                    let discarded = if keep == then_block { else_block } else { then_block };
                    leads_to_revert(func, discarded, reverting)
                } else {
                    false
                }
            });
        }
        self.ranges.clear();
        self.relations.clear();
        self.range_undo.clear();
        self.relation_undo.clear();

        let decided = if selected.is_some() { Vec::new() } else { decided };
        if folds.is_empty() && checks.is_empty() && cleanups.is_empty() && decided.is_empty() {
            return 0;
        }
        // v = op c, ... where the use's scope decides c => v = op true|false, ...
        for &(inst, condition, truth) in &decided {
            let constant = func.alloc_value(Value::Immediate(Immediate::I1(truth)));
            let replacements = FxHashMap::from_iter([(condition, constant)]);
            mir_utils::replace_inst_uses(func.inst_mut(inst), &replacements);
        }
        if !cleanups.is_empty() {
            // v = and x, 2^k - 1 (x <= 2^k - 1) => x
            // v = signextend b, x (x below the sign bit) => x
            let replacements = cleanups.iter().copied().collect::<FxHashMap<_, _>>();
            func.replace_uses_canonicalized(&replacements);
        }
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
        self.stats.branches_folded = folds.len();
        self.stats.checks_removed = checks.count();
        self.stats.cleanups_removed = cleanups.len();
        self.stats.conditions_decided = decided.len();
        self.stats.branches_folded
            + self.stats.checks_removed
            + self.stats.cleanups_removed
            + self.stats.conditions_decided
    }

    /// Walks the dominator tree, recording edge and check facts. Returns branch folds and
    /// proven passing checks to remove.
    /// Monotone candidates whose update is proven wrap-free in its defining block's
    /// scope are appended to their `proven` list. Each bounded candidate's count
    /// grows by one for every update proven to stay bounded in its own scope.
    fn collect_folds(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        preds: &IndexVec<BlockId, Vec<BlockId>>,
        facts: &IndexVec<BlockId, Facts>,
        (candidates, proven): (&[MonotonePhi], &mut Vec<MonotonePhi>),
        (bounded, bounded_proofs): (&[BoundedPhi], &mut [usize]),
    ) -> WalkProofs {
        enum Walk {
            Enter(BlockId),
            Exit { range_mark: usize, relation_mark: usize },
        }

        let mut folds = Vec::new();
        let mut checks = DenseBitSet::new_empty(func.num_insts());
        let mut cleanups = Vec::new();
        let mut decided = Vec::new();
        // A branch condition reused by the code it guards, such as a length
        // test that also selects a mask, is decided wherever the edge it
        // took dominates the use.
        let conditions = func
            .blocks
            .iter()
            .filter_map(|block| match block.terminator {
                Some(Terminator::Branch { condition, then_block, else_block })
                    if then_block != else_block
                        && !matches!(func.value(condition), Value::Immediate(_)) =>
                {
                    Some(condition)
                }
                _ => None,
            })
            .collect::<FxHashSet<_>>();
        let mut stack = vec![Walk::Enter(BlockId::ENTRY)];
        while let Some(item) = stack.pop() {
            match item {
                Walk::Exit { range_mark, relation_mark } => {
                    while self.range_undo.len() > range_mark {
                        let (value, old) = self.range_undo.pop().expect("checked len");
                        match old {
                            Some(range) => self.ranges.insert(value, range),
                            None => self.ranges.remove(&value),
                        };
                    }
                    if self.relation_undo.len() > relation_mark {
                        self.strict_lower_bounds = None;
                    }
                    while self.relation_undo.len() > relation_mark {
                        let relation = self.relation_undo.pop().expect("checked len");
                        self.relations.remove(&relation);
                    }
                }
                Walk::Enter(block) => {
                    stack.push(Walk::Exit {
                        range_mark: self.range_undo.len(),
                        relation_mark: self.relation_undo.len(),
                    });

                    for (&value, &range) in &facts[block].ranges {
                        self.narrow(value, range);
                    }
                    for &relation in &facts[block].relations {
                        self.add_relation(relation);
                    }
                    if let Some((condition, is_true)) = dominating_edge_fact(func, preds, block) {
                        self.assume(func, condition, is_true, MAX_DEPTH);
                    }
                    for candidate in candidates.iter().filter(|candidate| candidate.home == block) {
                        if self.update_cannot_wrap(func, candidate) {
                            proven.push(*candidate);
                        }
                    }

                    for &id in &func.blocks[block].instructions {
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
                        if let Some(average) = func.inst_result_value(id) {
                            self.assume_average(func, average);
                        }
                        if let Some(result) = func.inst_result_value(id)
                            && let Some(operand) = self.redundant_cleanup(func, id)
                        {
                            cleanups.push((result, operand));
                        }
                        if fact.is_none() && !matches!(func.inst(id).kind, InstKind::Phi(_)) {
                            for operand in func.inst(id).kind.operands() {
                                if conditions.contains(&operand)
                                    && !decided
                                        .iter()
                                        .any(|&(inst, value, _)| inst == id && value == operand)
                                    && let Some(truth) = self.eval_truth(func, operand, MAX_DEPTH)
                                {
                                    decided.push((id, operand, truth));
                                }
                            }
                        }
                    }

                    // Every path from an update to the latch leaves this block, so the
                    // facts at its end hold wherever the update reaches the phi.
                    for (phi, proofs) in bounded.iter().zip(bounded_proofs.iter_mut()) {
                        for &(update, home) in &phi.updates {
                            if home == block && self.update_stays_bounded(func, phi, update) {
                                *proofs += 1;
                            }
                        }
                    }

                    if let Some(Terminator::Branch { condition, then_block, else_block }) =
                        func.blocks[block].terminator.as_ref()
                        && then_block != else_block
                        && let Some(truth) = self.eval_truth(func, *condition, MAX_DEPTH)
                    {
                        folds.push((block, if truth { *then_block } else { *else_block }));
                    }

                    for &child in cfg.dominators().children(block) {
                        stack.push(Walk::Enter(child));
                    }
                }
            }
        }
        WalkProofs { folds, checks, cleanups, decided }
    }

    /// Whether `block` calls a function that never returns to its caller.
    fn calls_never_returning(&self, func: &Function, block: BlockId) -> bool {
        let Some(never_returning) = &self.never_returning else { return false };
        func.blocks[block].instructions.iter().any(|&inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::ICall { function: Callee::Function(callee), .. }
                    if never_returning.contains(&callee)
            )
        })
    }

    /// Returns the operand of a cleanup the scope already proves idle: an `and`
    /// with a low mask `2^k - 1` of a value at most the mask, or a
    /// `signextend` of a value whose sign bit and every bit above it are
    /// clear.
    fn redundant_cleanup(&mut self, func: &Function, inst: InstId) -> Option<ValueId> {
        match func.inst(inst).kind {
            InstKind::And(a, b) => {
                let (value, mask) = match (const_of(func, a), const_of(func, b)) {
                    (None, Some(mask)) => (a, mask),
                    (Some(mask), None) => (b, mask),
                    _ => return None,
                };
                let low_mask =
                    mask.checked_add(U256::from(1)).is_some_and(|next| next & mask == U256::ZERO);
                (low_mask && self.range_of(func, value, MAX_DEPTH).hi <= mask).then_some(value)
            }
            InstKind::SignExtend(byte, value) => {
                let byte = const_of(func, byte).filter(|byte| *byte < U256::from(31))?;
                let sign_bit = U256::from(1) << (8 * byte.to::<usize>() + 7);
                (self.range_of(func, value, MAX_DEPTH).hi < sign_bit).then_some(value)
            }
            _ => None,
        }
    }

    /// Decides in the current scope whether a bounded phi's update stays on the
    /// phi's side of its current value, without assuming the invariant it would
    /// establish.
    fn update_stays_bounded(&mut self, func: &Function, phi: &BoundedPhi, update: ValueId) -> bool {
        let (low, high) = if phi.decreasing { (update, phi.value) } else { (phi.value, update) };
        self.eval_lt(func, high, low, MAX_DEPTH) == Some(false)
            || self.eval_lt(func, low, high, MAX_DEPTH) == Some(true)
    }

    /// Records that a halved sum lies between its addends.
    ///
    /// `(x + y) >> 1` is at least `x` and at most `y` when `x <= y` and the sum
    /// does not wrap: `2x <= x + y <= 2y`, and halving keeps the order. A binary
    /// search's midpoint then stays within its bounds.
    fn assume_average(&mut self, func: &Function, average: ValueId) {
        let Some((sum, x, y)) = halved_sum(func, average) else { return };
        if self.eval_lt(func, sum, x, MAX_DEPTH) != Some(false) {
            return;
        }
        for (low, high) in [(x, y), (y, x)] {
            if self.has_relation(func, Relation::Le(low, high)) {
                self.add_relation(Relation::Le(low, average));
                self.add_relation(Relation::Le(average, high));
                return;
            }
        }
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

    /// Transfers edge facts from unknown, retaining only definitions available
    /// at the join and evaluating relevant phis in each predecessor's context.
    fn join_facts(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        preds: &IndexVec<BlockId, Vec<BlockId>>,
        relevant: &DenseBitSet<ValueId>,
    ) -> IndexVec<BlockId, Facts> {
        const MAX_ROUNDS: usize = 8;
        let definitions = func.inst_blocks();
        // A predicate consumed only by this branch cannot be queried after the edge.
        // Preserve its operand facts, but do not copy the dead predicate's own range
        // through every later block. Single-use ISZERO chains have the same property.
        let mut uses = index_vec![0usize; func.num_values()];
        for block in &func.blocks {
            for &inst in &block.instructions {
                for value in func.inst(inst).operands() {
                    uses[value] += 1;
                }
            }
            if let Some(term) = &block.terminator {
                term.for_each_operand(|value| uses[value] += 1);
            }
        }
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
        // Loop headers: blocks entered by an edge from a block they dominate.
        let mut headers = DenseBitSet::new_empty(func.blocks.len());
        for &block in cfg.rpo() {
            if preds[block].iter().any(|&pred| cfg.dominators().dominates(block, pred)) {
                headers.insert(block);
            }
        }
        // The latest range of each phi input on its incoming edge.
        let mut edge_ranges = FxHashMap::<(ValueId, BlockId), Range>::default();
        let mut cx = Self::new(self.immutable_ranges, self.object_lengths);
        cx.universal_relations.clone_from(&self.universal_relations);
        cx.length_anchors.clone_from(&self.length_anchors);
        let mut pending = cfg.reachable().clone();
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for &block in cfg.rpo() {
                if !pending.remove(block) {
                    continue;
                }
                let mut merged: Option<Facts> = None;
                if block != BlockId::ENTRY {
                    for &pred in &preds[block] {
                        cx.ranges.clone_from(&exits[pred].ranges);
                        cx.relations.clone_from(&exits[pred].relations);
                        cx.strict_lower_bounds = None;
                        cx.range_undo.clear();
                        cx.relation_undo.clear();
                        if let Some(Terminator::Branch { condition, then_block, else_block }) =
                            func.blocks[pred].terminator
                            && then_block != else_block
                        {
                            cx.assume(func, condition, then_block == block, MAX_DEPTH);
                        }
                        // Evaluate all inputs before assigning any phi: loop
                        // phis describe a simultaneous parallel assignment.
                        let phi_ranges: Vec<_> = func.blocks[block]
                            .instructions
                            .iter()
                            .filter_map(|&inst| {
                                let InstKind::Phi(incoming) = &func.inst(inst).kind else {
                                    return None;
                                };
                                let value = func.inst_result_value(inst)?;
                                if !relevant.contains(value) {
                                    return None;
                                }
                                let &(_, input) =
                                    incoming.iter().find(|&&(from, _)| from == pred)?;
                                Some((value, cx.range_of(func, input, MAX_DEPTH)))
                            })
                            .collect();
                        for &(value, range) in &phi_ranges {
                            edge_ranges.insert((value, pred), range);
                        }
                        for value in consumed_conditions.get(&pred).into_iter().flatten() {
                            cx.ranges.remove(value);
                        }
                        let available = |value| match func.value(value) {
                            Value::Inst(inst) => definitions.get(inst).is_some_and(|&home| {
                                home != block && cfg.dominators().dominates(home, block)
                            }),
                            _ => true,
                        };
                        cx.ranges.retain(|&value, _| available(value));
                        cx.relations.retain(|relation| {
                            let (a, b) = relation.operands();
                            available(a) && available(b)
                        });
                        for (value, range) in phi_ranges {
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
                if headers.contains(block)
                    && let Some(merged) = &mut merged
                {
                    for &inst in &func.blocks[block].instructions {
                        let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
                        let Some(value) = func.inst_result_value(inst) else { continue };
                        if !relevant.contains(value) {
                            continue;
                        }
                        let carried =
                            incoming.iter().try_fold(None::<Range>, |acc, &(pred, input)| {
                                let range =
                                    carried_range(func, value, value, pred, input, &edge_ranges, 4);
                                let acc = match (acc, range) {
                                    (Some(acc), Some(range)) => Some(acc.union(range)),
                                    (acc, range) => acc.or(range),
                                };
                                (acc != Some(Range::FULL)).then_some(acc)
                            });
                        if let Some(Some(range)) = carried {
                            let narrowed = merged
                                .ranges
                                .get(&value)
                                .map_or(range, |known| known.intersect(range).unwrap_or(*known));
                            merged.ranges.insert(value, narrowed);
                        }
                    }
                }
                let entry = merged.unwrap_or_default();
                cx.ranges.clone_from(&entry.ranges);
                cx.relations.clone_from(&entry.relations);
                cx.strict_lower_bounds = None;
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
                if exits[block] != exit {
                    for &successor in cfg.successors(block) {
                        pending.insert(successor);
                    }
                }
                changed |= entries[block] != entry || exits[block] != exit;
                entries[block] = entry;
                exits[block] = exit;
            }
            if !changed {
                break;
            }
        }
        self.relation_index = cx.relation_index;
        self.reverse_index = cx.reverse_index;
        self.difference_index = cx.difference_index;
        entries
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
            // `shr k, x` is nonzero exactly when `x >= 2^k`.
            InstKind::Shr(shift, x) => {
                if let Some(bits) = shift_amount(self, func, shift, depth) {
                    if bits == 0 {
                        self.assume(func, x, truth, depth);
                    } else {
                        let limit = U256::MAX >> (256 - bits);
                        if truth {
                            self.narrow(x, Range::new(limit + U256::from(1), U256::MAX));
                        } else {
                            self.narrow(x, Range::new(U256::ZERO, limit));
                        }
                    }
                }
            }
            // A zero extension is zero exactly when its source is.
            InstKind::Zext(source) => self.assume(func, source, truth, depth),
            _ => {}
        }
    }

    /// Records the consequences of `(a < b) == truth` (unsigned).
    fn assume_lt(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        if truth {
            self.add_relation(Relation::Lt(a, b));
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
            self.add_relation(Relation::Le(b, a));
            let lo_b = self.range_of(func, b, depth).lo;
            self.narrow(a, Range::new(lo_b, U256::MAX));
            let hi_a = self.range_of(func, a, depth).hi;
            self.narrow(b, Range::new(U256::ZERO, hi_a));
        }
    }

    /// Refines signed bounds without treating a cross-sign comparison as unsigned.
    fn assume_slt(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.lo.bit(255) == ra.hi.bit(255)
            && rb.lo.bit(255) == rb.hi.bit(255)
            && ra.lo.bit(255) == rb.lo.bit(255)
        {
            self.assume_lt(func, a, b, truth, depth);
            return;
        }
        let ra = ra.flip_sign();
        let rb = rb.flip_sign();
        let (a_limit, b_limit) = if truth {
            (
                Range::new(U256::ZERO, rb.hi.saturating_sub(U256::from(1))),
                Range::new(ra.lo.saturating_add(U256::from(1)), U256::MAX),
            )
        } else {
            (Range::new(rb.lo, U256::MAX), Range::new(U256::ZERO, ra.hi))
        };
        if let Some(range) = ra.intersect(a_limit) {
            self.narrow(a, range.flip_sign());
        }
        if let Some(range) = rb.intersect(b_limit) {
            self.narrow(b, range.flip_sign());
        }
    }

    /// Records the consequences of `(a == b) == truth`.
    fn assume_eq(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
        if const_of(func, b).is_some_and(|v| v.is_zero()) {
            self.assume(func, a, !truth, depth);
        } else if const_of(func, a).is_some_and(|v| v.is_zero()) {
            self.assume(func, b, !truth, depth);
        }
        let (x, y) = ordered(a, b);
        if truth {
            self.add_relation(Relation::Eq(x, y));
            let range = self.range_of(func, a, depth);
            self.narrow(b, range);
            let range = self.range_of(func, b, depth);
            self.narrow(a, range);
        } else {
            self.add_relation(Relation::Ne(x, y));
            self.exclude_boundary(func, a, b, depth);
            self.exclude_boundary(func, b, a, depth);
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

    fn add_relation(&mut self, relation: Relation) {
        if self.relations.insert(relation) {
            self.strict_lower_bounds = None;
            self.relation_undo.push(relation);
        }
    }

    /// Builds the forward relation index on first use.
    fn ensure_relation_index(&mut self, func: &Function) {
        if self.relation_index.is_some() {
            return;
        }
        let mut index = relation_candidates(func);
        for &relation in &self.monotone_relations {
            index_relation(&mut index, relation);
        }
        for &relation in &self.universal_relations {
            index_relation(&mut index, relation);
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
                let entry = differences.entry(subtrahend).or_default();
                if !entry.contains(&(minuend, value)) {
                    entry.push((minuend, value));
                }
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
        self.ensure_reverse_index(func);
        let reverse = self.reverse_index.as_ref().expect("relation index was just built");
        let mut amounts = SmallVec::<[U256; 4]>::new();
        for &fact in reverse.get(&bound).into_iter().flatten() {
            let Relation::Lt(index, limit) = fact else { continue };
            if limit != bound
                || !(self.relations.contains(&fact) || self.universal_relations.contains(&fact))
            {
                continue;
            }
            let amount = match inst_kind(func, index) {
                Some(&InstKind::Add(x, c)) if x == base => const_of(func, c),
                Some(&InstKind::Add(c, x)) if x == base => const_of(func, c),
                _ => None,
            };
            if let Some(amount) = amount
                && amount >= offset
            {
                amounts.push(amount);
            }
        }
        if amounts.is_empty() {
            return false;
        }
        let hi = self.range_of(func, base, depth).hi;
        amounts.into_iter().any(|amount| hi.checked_add(amount).is_some())
    }

    /// Decides `x + c1 < y + c2` from a fact `x + d < y` or `x + d <= y`:
    /// `Some(true)` when the sum is strictly below, `Some(false)` when it is
    /// only at most equal. Needs `d <= c1 <= d + c2`, so `x + c1` is
    /// `(x + d) + (c1 - d)` with `c1 - d <= c2`, and `y + c2` not wrapping,
    /// which its own passing check, its range or a checked sum establishes;
    /// then neither side wraps and the order carries over. A side without an
    /// offset has an offset of zero. A limit equal in scope to a sum, such as
    /// a reread length equal to the checked sum stored at allocation, is also
    /// tried as that sum.
    fn shifted_below(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        depth: usize,
    ) -> Option<bool> {
        self.ensure_reverse_index(func);
        let reverse = self.reverse_index.as_ref().expect("relation index was just built");
        let mut limits = SmallVec::<[_; 2]>::new();
        limits.push(shifted_operand(func, b));
        for &fact in reverse.get(&b).into_iter().flatten() {
            if let Relation::Eq(p, q) = fact
                && (self.relations.contains(&fact) || self.universal_relations.contains(&fact))
            {
                let sum = if p == b { q } else { p };
                let limit = shifted_operand(func, sum);
                if !limit.1.is_zero() && !limits.contains(&limit) {
                    limits.push(limit);
                }
            }
        }
        let mut result = None;
        for (y, c2, exact) in limits {
            match self.shifted_below_limit(func, a, b, (y, c2, exact), depth) {
                Some(true) => return Some(true),
                Some(false) => result = Some(false),
                None => {}
            }
        }
        result
    }

    /// [`Self::shifted_below`] against one form `y + c2` of the limit `b`,
    /// `exact` when that sum is checked and so cannot wrap.
    fn shifted_below_limit(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        (y, c2, exact): (ValueId, U256, bool),
        depth: usize,
    ) -> Option<bool> {
        let (x, c1, _) = shifted_operand(func, a);
        if x == y || (c1.is_zero() && c2.is_zero()) {
            return None;
        }
        let reverse = self.reverse_index.as_ref().expect("relation index was just built");
        let mut best = None::<(U256, bool)>;
        for &fact in reverse.get(&y).into_iter().flatten() {
            let (index, strict) = match fact {
                Relation::Lt(index, limit) if limit == y => (index, true),
                Relation::Le(index, limit) if limit == y => (index, false),
                _ => continue,
            };
            if !self.relations.contains(&fact) && !self.universal_relations.contains(&fact) {
                continue;
            }
            let (base, d, _) = shifted_operand(func, index);
            if base != x || d > c1 {
                continue;
            }
            if best.is_none_or(|(known, _)| d > known || (d == known && strict)) {
                best = Some((d, strict));
            }
        }
        let (d, strict) = best?;
        let reach = d.checked_add(c2)?;
        if c1 > reach {
            return None;
        }
        let sum_sound = c2.is_zero()
            || exact
            || self.has_relation(func, Relation::Le(y, b))
            || self.range_of(func, y, depth).hi.checked_add(c2).is_some();
        if !sum_sound {
            return None;
        }
        Some(strict || c1 < reach)
    }

    /// Whether a bounded loop cursor plus `width` fits in `capacity`.
    ///
    /// This discharges checked fixed-width writes in builders that reserve
    /// `scale * input_length` bytes and consume a constant number of input
    /// items per iteration. The proof is structural and local to a natural
    /// loop: both cursors start at zero, the input cursor steps by a constant,
    /// every output update is bounded by `scale` times that step, and no loop
    /// block changes the output object's logical length. A checked
    /// multiplication, or its still-dominating round-trip check after
    /// lowering, proves that the capacity product is exact.
    fn scaled_cursor_fits(
        &mut self,
        func: &Function,
        cursor: ValueId,
        width: U256,
        capacity: Option<ValueId>,
        depth: usize,
    ) -> bool {
        let candidates = self
            .scaled_cursors
            .iter()
            .filter(|candidate| candidate.cursor == cursor && width <= candidate.max_step)
            .cloned()
            .collect::<Vec<_>>();
        for candidate in candidates {
            if !self.has_relation(func, candidate.guard)
                || (candidate.wrapping_sum
                    && self
                        .range_of(func, candidate.index, depth)
                        .hi
                        .checked_add(candidate.step)
                        .is_none())
            {
                continue;
            }
            let requested_object = capacity.and_then(|value| match inst_kind(func, value) {
                Some(&InstKind::MemoryObjectLen(object, kind)) => Some((object, kind)),
                _ => None,
            });
            for &inst_id in &func.blocks[candidate.preheader].instructions {
                let InstKind::SetMemoryObjectLen(object, length, kind) = func.inst(inst_id).kind
                else {
                    continue;
                };
                if requested_object.is_some_and(|requested| requested != (object, kind)) {
                    continue;
                }
                if capacity.is_some_and(|capacity| {
                    requested_object.is_none() && !values_equal(func, capacity, length)
                }) {
                    continue;
                }
                if candidate.loop_blocks.iter().any(|block| {
                    func.blocks[block].instructions.iter().any(|&inst| {
                        matches!(func.inst(inst).kind,
                            InstKind::SetMemoryObjectLen(loop_object, _, loop_kind)
                                if loop_object == object && loop_kind == kind)
                    })
                }) {
                    continue;
                }
                let Some(scale) = self.exact_scale(func, length, &candidate, depth) else {
                    continue;
                };
                let Some(reach) = scale.checked_mul(candidate.step) else { continue };
                if reach >= candidate.max_step && width <= reach {
                    return true;
                }
            }
        }
        false
    }

    /// Returns `scale` for an exact `length * scale` product.
    fn exact_scale(
        &mut self,
        func: &Function,
        product: ValueId,
        candidate: &ScaledCursor,
        depth: usize,
    ) -> Option<U256> {
        // A doubling `length + length` is the product by two the egraph leaves.
        if let Some((source, false)) = doubled(func, product) {
            return (same_stable_length(func, source, candidate.length, candidate)
                && self.range_of(func, source, depth).hi.leading_zeros() >= 1)
                .then(|| U256::from(2));
        }
        let (lhs, rhs, checked) = match inst_kind(func, product)? {
            InstKind::CheckedBinary {
                op: CheckedOp::Mul,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs,
                rhs,
            } => (*lhs, *rhs, true),
            InstKind::Mul(lhs, rhs) => (*lhs, *rhs, false),
            _ => return None,
        };
        let (source, scale) = if same_stable_length(func, lhs, candidate.length, candidate) {
            (lhs, const_of(func, rhs)?)
        } else if same_stable_length(func, rhs, candidate.length, candidate) {
            (rhs, const_of(func, lhs)?)
        } else {
            return None;
        };
        if scale.is_zero() {
            return None;
        }
        if checked || self.range_of(func, source, depth).hi.checked_mul(scale).is_some() {
            return Some(scale);
        }
        for inst_id in func.instructions() {
            let InstKind::Div(dividend, divisor) = func.inst(inst_id).kind else { continue };
            if dividend != product || const_of(func, divisor) != Some(scale) {
                continue;
            }
            let Some(quotient) = func.inst_result_value(inst_id) else { continue };
            let (a, b) = ordered(quotient, source);
            if self.has_relation(func, Relation::Eq(a, b)) {
                return Some(scale);
            }
        }
        None
    }

    /// Whether some value is provably below `value` in the current scope,
    /// which puts `value` at one or more: every word is at least zero.
    fn has_strict_lower_bound(&mut self, func: &Function, value: ValueId) -> bool {
        if self.strict_lower_bounds.is_none() {
            let mut nonzero = GrowableBitSet::with_capacity(func.num_values());
            let mut pending = Vec::new();
            for &fact in &self.relations {
                if let Relation::Lt(_, bound) = fact
                    && nonzero.insert(bound)
                {
                    pending.push(bound);
                }
            }
            if pending.is_empty() {
                self.strict_lower_bounds = Some(nonzero);
                return false;
            }
            self.ensure_relation_index(func);
            let index = self.relation_index.as_ref().expect("relation index was just built");
            // A strict edge makes its upper endpoint nonzero. Propagate that fact through
            // active orderings once per scope instead of searching backward for every value.
            while let Some(current) = pending.pop() {
                for &fact in index.get(&current).into_iter().flatten() {
                    if !self.relations.contains(&fact) && !self.universal_relations.contains(&fact)
                    {
                        continue;
                    }
                    let next = match fact {
                        Relation::Lt(a, b) | Relation::Le(a, b) if a == current => b,
                        Relation::Eq(a, b) => {
                            if a == current {
                                b
                            } else {
                                a
                            }
                        }
                        _ => continue,
                    };
                    if nonzero.insert(next) {
                        pending.push(next);
                    }
                }
            }
            self.strict_lower_bounds = Some(nonzero);
        }
        self.strict_lower_bounds.as_ref().unwrap().contains(value)
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
        let index = self.relation_index.as_ref().expect("relation index was just built");
        if !index.contains_key(&start) {
            return false;
        }
        // A bounded implication search: equality is bidirectional, <= carries
        // order, and one strict edge makes the complete path strict. Disequality
        // is not transitive. Exhausting the budget only misses an optimization.
        const MAX_RELATION_STATES: usize = 128;
        let mut pending = SmallVec::<[_; 8]>::new();
        pending.push((start, false));
        let mut seen = FxHashSet::default();
        while let Some((value, strict)) = pending.pop() {
            if value == end && (!needs_strict || strict) {
                return true;
            }
            if !seen.insert((value, strict)) {
                continue;
            }
            if seen.len() >= MAX_RELATION_STATES {
                return false;
            }
            for &fact in index.get(&value).into_iter().flatten() {
                if !self.relations.contains(&fact) && !self.universal_relations.contains(&fact) {
                    continue;
                }
                let next = match fact {
                    Relation::Eq(a, b) if a == value => Some((b, strict)),
                    Relation::Eq(a, b) if b == value => Some((a, strict)),
                    Relation::Le(a, b) if a == value && !equality_only => Some((b, strict)),
                    Relation::Lt(a, b) if a == value && !equality_only => Some((b, true)),
                    _ => None,
                };
                if let Some(next) = next {
                    pending.push(next);
                }
            }
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

    // === Evaluation ===

    /// Computes a sound overapproximation of the values `value` can take at
    /// the current program point.
    fn range_of(&mut self, func: &Function, value: ValueId, depth: usize) -> Range {
        if let Some(constant) = const_of(func, value) {
            return Range::singleton(constant);
        }
        let mut range = self.ranges.get(&value).copied().unwrap_or(Range::FULL);
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
                let mask = U256::MAX >> (256 - bits);
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
            InstKind::MemoryObjectLen(..) => {
                let lengths = self.object_lengths.unwrap_or(Range::FULL);
                // A stable read is the length its object was given, under that
                // value's bounds in this scope.
                match self.length_anchors.get(&value).copied() {
                    Some(anchor) => {
                        let anchored = self.range_of(func, anchor, depth);
                        lengths.intersect(anchored).unwrap_or(lengths)
                    }
                    None => lengths,
                }
            }
            InstKind::Add(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                match ra.hi.checked_add(rb.hi) {
                    Some(hi) => Range::new(ra.lo.wrapping_add(rb.lo), hi),
                    None => Range::FULL,
                }
            }
            InstKind::Sub(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                if ra.lo >= rb.hi {
                    Range::new(ra.lo - rb.hi, ra.hi - rb.lo)
                } else if ra.hi >= rb.lo && self.has_relation(func, Relation::Le(b, a)) {
                    // A known `b <= a` rules out wrapping where the ranges overlap.
                    Range::new(U256::ZERO, ra.hi - rb.lo)
                } else {
                    Range::FULL
                }
            }
            InstKind::Mul(a, b) => {
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                match ra.hi.checked_mul(rb.hi) {
                    Some(hi) => Range::new(ra.lo.wrapping_mul(rb.lo), hi),
                    None => Range::FULL,
                }
            }
            InstKind::Div(a, b) => {
                // EVM division by zero yields zero, so the result never
                // exceeds the dividend.
                let ra = self.range_of(func, a, depth);
                let rb = self.range_of(func, b, depth);
                let lo = if rb.lo > U256::ZERO { ra.lo / rb.hi } else { U256::ZERO };
                Range::new(lo, ra.hi)
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
                    Some(bits) if rv.hi.leading_zeros() >= bits => {
                        Range::new(rv.lo << bits, rv.hi << bits)
                    }
                    _ => Range::FULL,
                }
            }
            // `byte` extracts one byte and yields zero past the word.
            InstKind::Byte(..) => Range::new(U256::ZERO, U256::from(255)),
            InstKind::Not(a) => {
                let ra = self.range_of(func, a, depth);
                Range::new(!ra.hi, !ra.lo)
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
            _ => Range::FULL,
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
        if let Some(constant) = const_of(func, value) {
            return Some(!constant.is_zero());
        }
        if let Some(range) = self.ranges.get(&value) {
            if range.lo > U256::ZERO {
                return Some(true);
            }
            if range.hi.is_zero() {
                return Some(false);
            }
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
                    || self.product_check_holds(func, a, b, depth)
                    || self.product_check_holds(func, b, a, depth)
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
            _ => {
                let range = self.range_of(func, value, depth);
                if range.lo > U256::ZERO {
                    return Some(true);
                }
                if range.hi.is_zero() {
                    return Some(false);
                }
                None
            }
        }
    }

    /// Evaluates signed order using bounds rotated by the sign bit.
    fn eval_slt(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) -> Option<bool> {
        if a == b {
            return Some(false);
        }
        let ra = self.range_of(func, a, depth);
        let rb = self.range_of(func, b, depth);
        if ra.lo.bit(255) == ra.hi.bit(255)
            && rb.lo.bit(255) == rb.hi.bit(255)
            && ra.lo.bit(255) == rb.lo.bit(255)
        {
            return self.eval_lt(func, a, b, depth);
        }
        let ra = ra.flip_sign();
        let rb = rb.flip_sign();
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
            if rx.hi.checked_add(ry.hi).is_some() {
                return Some(false);
            }
            if rx.lo.checked_add(ry.lo).is_none() {
                return Some(true);
            }
        }

        // A variable-rate output cursor in a bounded builder stays within its
        // reserved capacity. This proves all three forms emitted by checked
        // indexing: the addition does not wrap, the end offset is not beyond
        // the capacity, and the current cursor is strictly inside it while
        // the input loop guard is true.
        if let Some((cursor, width)) = add_with_bounded_width(func, a) {
            if b == cursor && self.scaled_cursor_fits(func, cursor, width, None, depth) {
                return Some(false);
            }
            if self.scaled_cursor_fits(func, cursor, width, Some(b), depth) {
                return Some(true);
            }
        }
        if let Some((cursor, width)) = add_with_bounded_width(func, b)
            && self.scaled_cursor_fits(func, cursor, width, Some(a), depth)
        {
            return Some(false);
        }
        if self.scaled_cursor_fits(func, a, U256::ZERO, Some(b), depth) {
            return Some(true);
        }

        // Underflow check variant `lt x, (sub x, y)`: equivalent to
        // `lt x, y` for every `y` (with wrapping subtraction).
        if let Some(&InstKind::Sub(x, y)) = inst_kind(func, b)
            && a == x
            && let Some(reduced_depth) = depth.checked_sub(1)
        {
            return self.eval_lt(func, x, y, reduced_depth);
        }

        // A doubled index stays below a doubled bound its source respects:
        // `2x < 2y` and `2x + 1 < 2y` follow from `x < y` when `2y` cannot wrap,
        // which covers `out[2 * i]` and `out[2 * i + 1]` against `2 * n`.
        if let (Some((x, _)), Some((y, false))) = (doubled(func, a), doubled(func, b))
            && self.range_of(func, y, depth).hi.leading_zeros() >= 1
            && self.has_relation(func, Relation::Lt(x, y))
        {
            return Some(true);
        }

        // A decremented index stays below a bound its source respects:
        // `x - c < b` follows from `x <= b` when `c >= 1` and `x - c` cannot
        // wrap, which covers `a[j - 1]` after `j <= i < a.length`.
        if let Some(&InstKind::Sub(x, c)) = inst_kind(func, a)
            && let Some(step) = const_of(func, c)
            && !step.is_zero()
            && self.range_of(func, x, depth).lo >= step
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
            && const_of(func, c) == Some(U256::ONE)
            && self.range_of(func, x, depth).lo >= U256::ONE
            && self.has_relation(func, Relation::Lt(b, x))
        {
            return Some(false);
        }

        // A smaller constant offset stays below whatever a larger one stays
        // below: `x + b < n` follows from `x + a < n` when `b <= a` and
        // `x + a` cannot wrap, which covers the lookahead guards and the
        // original bound of a loop split to run while `i + K < n`.
        let (base, offset) = match inst_kind(func, a) {
            Some(&InstKind::Add(x, c)) if const_of(func, c).is_some() => (x, const_of(func, c)),
            Some(&InstKind::Add(c, x)) if const_of(func, c).is_some() => (x, const_of(func, c)),
            _ => (a, Some(U256::ZERO)),
        };
        if let Some(offset) = offset
            && self.has_larger_offset_bound(func, base, offset, b, depth)
        {
            return Some(true);
        }

        // Both sides shifted by constants: `x + c1 < y + c2` follows from
        // `x + d < y`, and `y + c2 < x + c1` is false after `x + d <= y`, when
        // `d <= c1 <= d + c2` and `y + c2` cannot wrap, which covers a word
        // written at `j` into 29 bytes of slack, `j + 32 <= length + 29`,
        // after the guard `j + 3 <= length` and the allocation's own check.
        if self.shifted_below(func, a, b, depth) == Some(true) {
            return Some(true);
        }
        if self.shifted_below(func, b, a, depth).is_some() {
            return Some(false);
        }

        let (x, y) = ordered(a, b);
        if self.has_relation(func, Relation::Lt(a, b)) {
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
        // A zero test negates the truth of an `or` or `and`, which their
        // operands may prove where bit ranges cannot, as for a checked
        // product's disjunction. Comparisons already reach their truth
        // through their ranges.
        for (tested, zero) in [(a, b), (b, a)] {
            if const_of(func, zero) == Some(U256::ZERO)
                && matches!(inst_kind(func, tested), Some(InstKind::Or(..) | InstKind::And(..)))
                && let Some(truth) = self.eval_truth(func, tested, depth)
            {
                return Some(!truth);
            }
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
        // A scaling shift's check `eq (shr k, (shl k, x)), x` holds iff no set bit
        // of `x` shifts out, which `x < 2^(256 - k)` rules out.
        for (shifted, expected) in [(a, b), (b, a)] {
            if let Some(&InstKind::Shr(count, product)) = inst_kind(func, shifted)
                && let Some(&InstKind::Shl(inner, source)) = inst_kind(func, product)
                && source == expected
                && let Some(bits) = const_of(func, count)
                && const_of(func, inner) == Some(bits)
                && bits < U256::from(256)
                && U256::from(self.range_of(func, expected, depth).hi.leading_zeros()) >= bits
            {
                return Some(true);
            }
        }
        // Its doubling form `eq (shr 1, (add x, x)), x` holds iff `x + x` did not wrap.
        for (shifted, expected) in [(a, b), (b, a)] {
            if let Some(&InstKind::Shr(count, sum)) = inst_kind(func, shifted)
                && const_of(func, count) == Some(U256::from(1))
                && doubled(func, sum) == Some((expected, false))
                && self.range_of(func, expected, depth).hi.leading_zeros() >= 1
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
        let (quotient, two) = if const_of(func, rhs) == Some(U256::from(2)) {
            (lhs, rhs)
        } else if const_of(func, lhs) == Some(U256::from(2)) {
            (rhs, lhs)
        } else {
            return false;
        };
        debug_assert!(const_of(func, two).is_some());
        let Some(&InstKind::Div(sum, divisor)) = inst_kind(func, quotient) else { return false };
        if divisor != x || doubled(func, sum) != Some((x, false)) {
            return false;
        }
        self.range_of(func, x, depth).hi.leading_zeros() >= 1
    }

    /// Recognizes the checked product `or (eq y, 0), (eq (div (mul x, y), y), x)`,
    /// which holds whenever `x * y` cannot wrap: a zero `y` satisfies the first
    /// disjunct and any other `y` divides the exact product back to `x`.
    fn product_check_holds(
        &mut self,
        func: &Function,
        zero_test: ValueId,
        roundtrip: ValueId,
        depth: usize,
    ) -> bool {
        let Some(y) = inst_kind(func, zero_test).and_then(|kind| kind.zero_test_operand(func))
        else {
            return false;
        };
        let Some(&InstKind::Eq(lhs, rhs)) = inst_kind(func, roundtrip) else { return false };
        for (quotient, expected) in [(lhs, rhs), (rhs, lhs)] {
            let Some(&InstKind::Div(product, divisor)) = inst_kind(func, quotient) else {
                continue;
            };
            let Some(&InstKind::Mul(p, q)) = inst_kind(func, product) else { continue };
            if !values_equal(func, divisor, y) {
                continue;
            }
            for (x, factor) in [(p, q), (q, p)] {
                if x == expected && values_equal(func, factor, y) {
                    let rx = self.range_of(func, x, depth);
                    let ry = self.range_of(func, y, depth);
                    if rx.hi.checked_mul(ry.hi).is_some() {
                        return true;
                    }
                }
            }
        }
        false
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
            if rx.hi.checked_mul(ry.hi).is_some() {
                return Some(true);
            }
        }
        None
    }
}

/// The range of `input`, the value `owner` receives from `pred`, as far as it can reach the
/// loop header phi `header_phi`, or `None` when the input is the header phi itself.
///
/// A header phi holds either its preheader value or a value an earlier iteration stored in
/// it. A path that carries the phi around the loop unchanged stores what the phi already held,
/// so by induction over the header's visits the union of the other inputs covers every value
/// it takes; the carried paths contribute nothing to that union. Phis between the update and
/// the latch are decomposed into their own incoming edges, each with the range recorded for
/// that edge, up to `depth` levels. Every range is a fact about one edge that holds in each
/// execution, so the union is sound whether or not the forward analysis has converged.
fn carried_range(
    func: &Function,
    header_phi: ValueId,
    owner: ValueId,
    pred: BlockId,
    input: ValueId,
    edge_ranges: &FxHashMap<(ValueId, BlockId), Range>,
    depth: usize,
) -> Option<Range> {
    if input == header_phi {
        return None;
    }
    if let Some(depth) = depth.checked_sub(1)
        && let Value::Inst(inst) = func.value(input)
        && let InstKind::Phi(incoming) = &func.inst(*inst).kind
    {
        let mut carried = None::<Range>;
        for &(inner_pred, inner) in incoming {
            let Some(range) =
                carried_range(func, header_phi, input, inner_pred, inner, edge_ranges, depth)
            else {
                continue;
            };
            let union = carried.map_or(range, |known| known.union(range));
            if union == Range::FULL {
                return Some(Range::FULL);
            }
            carried = Some(union);
        }
        return carried;
    }
    Some(edge_ranges.get(&(owner, pred)).copied().unwrap_or(Range::FULL))
}

/// Values whose ranges can affect a branch, closed over all SSA operands.
/// Phi inputs keep loop-carried dependencies in the set. Memory and call
/// operands are included conservatively even when range evaluation stops there.
fn branch_inputs(func: &Function, cfg: &CfgInfo) -> DenseBitSet<ValueId> {
    let mut relevant = DenseBitSet::new_empty(func.num_values());
    let mut pending = cfg
        .rpo()
        .iter()
        .filter_map(|&block| match func.blocks[block].terminator {
            Some(Terminator::Branch { condition, then_block, else_block })
                if then_block != else_block =>
            {
                Some(condition)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for &block in cfg.rpo() {
        for &inst in &func.blocks[block].instructions {
            if let InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), args } =
                &func.inst(inst).kind
            {
                pending.push(args[0]);
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

/// Follows the boolean operations that `assume` can inspect, in stable block
/// and operand order. Other computations cannot introduce a relational fact.
/// Conditions outside the current scope are harmless: every candidate still
/// requires membership in the current fact set. No IR changes during analysis.
/// Orderings that hold wherever their operands do: a right shift, a division
/// by a nonzero constant, a mask, or a remainder never exceeds the value it
/// derives from. Only values feeding branches are indexed.
fn universal_relations(func: &Function, relevant: &DenseBitSet<ValueId>) -> FxHashSet<Relation> {
    let mut relations = FxHashSet::default();
    let mut masks = FxHashMap::<ValueId, SmallVec<[(ValueId, U256); 2]>>::default();
    for inst_id in func.instructions() {
        let Some(value) = func.inst_result_value(inst_id) else { continue };
        if !relevant.contains(value) {
            continue;
        }
        match func.inst(inst_id).kind {
            InstKind::Shr(_, x) | InstKind::Mod(x, _) => {
                relations.insert(Relation::Le(value, x));
            }
            InstKind::Div(x, divisor) if const_of(func, divisor).is_some_and(|c| !c.is_zero()) => {
                relations.insert(Relation::Le(value, x));
            }
            InstKind::And(x, y) => {
                relations.insert(Relation::Le(value, x));
                relations.insert(Relation::Le(value, y));
                for (x, mask) in [(x, y), (y, x)] {
                    if let Some(mask) = const_of(func, mask) {
                        masks.entry(x).or_default().push((value, mask));
                    }
                }
            }
            // If conversion rewrites `if (x > limit) x = limit`, and so the
            // minimum of two values, to `x + (x > limit) * (limit - x)`, which
            // is `limit` when the test holds and `x` otherwise, so it never
            // exceeds either.
            InstKind::Add(x, adjustment) => {
                for (x, adjustment) in [(x, adjustment), (adjustment, x)] {
                    if let Some(limit) = clamp_limit(func, x, adjustment) {
                        relations.insert(Relation::Le(value, limit));
                        relations.insert(Relation::Le(value, x));
                    }
                }
            }
            _ => {}
        }
    }
    // Two masks of one word: the one whose bits the other's include never
    // exceeds it, as `x & ~31 <= x & 0xff` once folding has merged the masks
    // that separated the rounded value from the word it rounds.
    for masked in masks.values() {
        for &(small, small_mask) in masked {
            for &(large, large_mask) in masked {
                if small != large && small_mask & !large_mask == U256::ZERO {
                    relations.insert(Relation::Le(small, large));
                }
            }
        }
    }
    relations
}

/// Equates each checked `u256` sum with a wrapping sum of the same operands.
///
/// The checked sum is defined only where it did not wrap, and there both hold
/// the same word, so a guard on one bounds the other: a loop test on
/// `i + 16` then covers a bounds check that adds `16` to `i` again.
fn checked_sum_twins(func: &Function) -> Vec<Relation> {
    // Constants may be distinct values with equal words.
    let key = |value: ValueId| match const_of(func, value) {
        Some(constant) => (true, constant),
        None => (false, U256::from(value.index())),
    };
    let mut sums = FxHashMap::<_, (SmallVec<[ValueId; 2]>, SmallVec<[ValueId; 2]>)>::default();
    for inst_id in func.instructions() {
        let Some(value) = func.inst_result_value(inst_id) else { continue };
        let (a, b, checked) = match func.inst(inst_id).kind {
            InstKind::Add(a, b) => (a, b, false),
            InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs,
                rhs,
            } => (lhs, rhs, true),
            _ => continue,
        };
        let (a, b) = (key(a), key(b));
        let entry = sums.entry(if a <= b { (a, b) } else { (b, a) }).or_default();
        if checked { entry.1.push(value) } else { entry.0.push(value) }
    }
    let mut relations = Vec::new();
    for (wrapping, checked) in sums.values() {
        for &checked in checked {
            for &wrapping in wrapping {
                let (a, b) = ordered(checked, wrapping);
                relations.push(Relation::Eq(a, b));
            }
        }
    }
    relations
}

/// Pairs each reread of an object's length that nothing in the function can
/// change with the value it agrees with: a parameter object's first read, or
/// the length set at allocation for a fresh object whose length is set once.
///
/// Callers run this only for modules without inline assembly. There a
/// parameter object lies below the free-memory pointer at entry, while the
/// function's own allocations start at or above it, so writes into other
/// fresh objects cannot reach its length word, and a fresh object's length
/// changes only through its own length stores. A write in a block that leaves
/// the function, such as a panic's encoding or a final truncation, reaches
/// only the reads after it in that block. Any other write that may reach the
/// word, including every call with memory effects, keeps the reads apart. The
/// loads stay where they are: only the checks learn that they agree.
fn stable_object_lengths(
    func: &Function,
    summaries: Option<Arc<MemoryCallSummaries>>,
) -> Vec<(ValueId, ValueId)> {
    let mut reads = FxHashMap::<_, SmallVec<[(InstId, ValueId); 2]>>::default();
    for inst_id in func.instructions() {
        if let InstKind::MemoryObjectLen(object, kind) = func.inst(inst_id).kind
            && let Some(value) = func.inst_result_value(inst_id)
        {
            reads.entry((object, kind)).or_default().push((inst_id, value));
        }
    }
    // A fresh object's anchor is the length stored right after its allocation,
    // before any read in that block; a parameter's is its first read.
    let definitions = func.inst_blocks();
    let mut anchors = FxHashMap::<_, (Option<InstId>, ValueId)>::default();
    for (&(object, kind), group) in &reads {
        match func.value(object) {
            Value::Arg(_) if group.len() > 1 => {
                anchors.insert((object, kind), (None, group[0].1));
            }
            &Value::Inst(alloc) if matches!(func.inst(alloc).kind, InstKind::Alloc { .. }) => {
                let Some(&block) = definitions.get(&alloc) else { continue };
                let instructions = &func.blocks[block].instructions;
                let Some(position) = instructions.iter().position(|&inst| inst == alloc) else {
                    continue;
                };
                for &inst in &instructions[position + 1..] {
                    if group.iter().any(|&(read, _)| read == inst) {
                        break;
                    }
                    if let InstKind::SetMemoryObjectLen(set_object, length, set_kind) =
                        func.inst(inst).kind
                        && set_object == object
                        && set_kind == kind
                    {
                        anchors.insert((object, kind), (Some(inst), length));
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    if anchors.is_empty() {
        return Vec::new();
    }
    let aa = match summaries {
        Some(summaries) => AliasAnalysis::with_call_summaries(func, summaries),
        None => AliasAnalysis::new(func),
    };
    let mut relations = Vec::new();
    for (&(object, kind), &(anchor_set, anchor)) in &anchors {
        let group = &reads[&(object, kind)];
        let Some(location) = aa.memory_object_length_location(func, group[0].0, object, kind)
        else {
            continue;
        };
        let location = Location::Memory(location);
        let stable = func.blocks.iter().all(|block| {
            let exits = block.terminator.as_ref().is_some_and(|term| term.successors().is_empty());
            block.instructions.iter().enumerate().all(|(position, &inst)| {
                let inst_kind = &func.inst(inst).kind;
                let sets_object = matches!(*inst_kind,
                    InstKind::SetMemoryObjectLen(set_object, ..) if set_object == object);
                Some(inst) == anchor_set
                    || !aa.instruction_mod_ref(func, inst).may_write(&aa, location)
                    || writes_below_objects(&aa, func, inst)
                    || (writes_only_fresh_object(func, inst_kind) && !sets_object)
                    || (exits
                        && block.instructions[position + 1..]
                            .iter()
                            .all(|later| group.iter().all(|&(read, _)| read != *later)))
            })
        });
        if stable {
            relations.extend(
                group
                    .iter()
                    .filter(|&&(_, value)| value != anchor)
                    .map(|&(_, value)| (value, anchor)),
            );
        }
    }
    relations
}

/// Whether every memory write of `inst` ends at or below the zero slot, as
/// the scratch words a slot hash writes do.
///
/// Callers run this only for modules without inline assembly. There every
/// memory object's length word lies at or above the zero slot: an empty
/// object is the zero slot itself, and every other object lies above the
/// free-memory pointer's initial value. Alias analysis cannot use that in
/// general, because assembly can make a pointer to any address.
fn writes_below_objects(aa: &AliasAnalysis, func: &Function, inst: InstId) -> bool {
    aa.instruction_mod_ref(func, inst).writes().iter().all(|&access| match access {
        Access::Location(Location::Memory(location)) => location
            .address
            .as_absolute()
            .zip(location.size.as_const())
            .and_then(|(start, size)| start.checked_add(size))
            .is_some_and(|end| end <= EvmMemoryLayout::ZERO_SLOT),
        Access::Location(_) => true,
        Access::Any(space) => space != AddressSpace::Memory,
    })
}

/// The upper limit of a clamp written as `x + (x > limit) * (limit - x)`.
///
/// The product is zero when the test fails, leaving `x`, and `limit - x` when
/// it holds, leaving exactly `limit` under wrapping addition, which is then
/// below `x`. Either way the sum is at most both `x` and `limit`. The test is
/// an `i1` widened to a word, and may be spelled `limit < x`.
fn clamp_limit(func: &Function, x: ValueId, adjustment: ValueId) -> Option<ValueId> {
    let &InstKind::Mul(first, second) = inst_kind(func, adjustment)? else { return None };
    for (condition, difference) in [(first, second), (second, first)] {
        let Some(&InstKind::Zext(condition)) = inst_kind(func, condition) else { continue };
        let (tested, limit) = match inst_kind(func, condition) {
            Some(&InstKind::Gt(tested, limit) | &InstKind::Lt(limit, tested)) => (tested, limit),
            _ => continue,
        };
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

fn relation_candidates(func: &Function) -> FxHashMap<ValueId, SmallVec<[Relation; 2]>> {
    let mut index = FxHashMap::<_, SmallVec<[Relation; 2]>>::default();
    let mut seen = FxHashSet::default();
    let mut add = |relation: Relation| {
        if seen.insert(relation) {
            index_relation(&mut index, relation);
        }
    };
    // A halved sum can lie between its addends in either order.
    for inst in func.instructions() {
        if let Some(average) = func.inst_result_value(inst)
            && let Some((_, x, y)) = halved_sum(func, average)
        {
            for bound in [x, y] {
                add(Relation::Le(bound, average));
                add(Relation::Le(average, bound));
            }
        }
    }
    let mut visited = DenseBitSet::new_empty(func.num_values());
    let mut pending = Vec::new();
    for block in &func.blocks {
        if let Some(Terminator::Branch { condition, then_block, else_block }) = block.terminator
            && then_block != else_block
        {
            pending.push(condition);
        }
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            match inst_kind(func, value) {
                Some(&InstKind::Lt(a, b)) | Some(&InstKind::Gt(b, a)) => {
                    add(Relation::Lt(a, b));
                    add(Relation::Le(b, a));
                }
                Some(
                    &InstKind::Eq(a, b)
                    | &InstKind::Ne(a, b)
                    | &InstKind::Sub(a, b)
                    | &InstKind::Xor(a, b),
                ) => {
                    let (x, y) = ordered(a, b);
                    add(Relation::Eq(x, y));
                }

                Some(&InstKind::And(a, b) | &InstKind::Or(a, b)) => {
                    pending.extend([b, a]);
                }
                _ => {}
            }
        }
    }
    index
}

/// Finds paired zero-based loop cursors where `index` advances by a constant
/// step and `cursor` advances by a path-dependent amount with a finite maximum.
///
/// The header guards the body with `index < length` for a unit step, or with
/// `index + step > length` exiting the loop, where the sum is the checked or
/// wrapping sum the backedge also stores. A wrapping sum qualifies because the
/// guard fact names that exact value: a wrapped sum would still sit below
/// `length`, so the proof requires the index's range to rule wrapping out
/// before trusting it.
fn scaled_cursor_candidates(
    func: &Function,
    cfg: &CfgInfo,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    monotone: &[MonotonePhi],
) -> Vec<ScaledCursor> {
    let mut candidates = Vec::new();
    for index_phi in monotone {
        let Some(step) = const_of(func, index_phi.step) else { continue };
        if index_phi.decreasing || const_of(func, index_phi.initial) != Some(U256::ZERO) {
            continue;
        }
        let loop_blocks =
            natural_loop_blocks(index_phi.header, index_phi.latch, preds, func.blocks.len());
        if !loop_blocks.contains(index_phi.header)
            || !loop_blocks.contains(index_phi.latch)
            || !cfg.dominators().dominates(index_phi.header, index_phi.latch)
        {
            continue;
        }
        let index = index_phi.value;
        let Some((length, guard, wrapping_sum)) =
            loop_guard(func, index_phi.header, &loop_blocks, index, step)
        else {
            continue;
        };
        for &inst_id in &func.blocks[index_phi.header].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
            let Some(cursor) = func.inst_result_value(inst_id) else { continue };
            if cursor == index {
                continue;
            }
            let mut initial = None;
            let mut next = None;
            let mut valid = true;
            for &(block, value) in incoming {
                if block == index_phi.preheader {
                    if initial.replace(value).is_some() {
                        valid = false;
                    }
                } else if block == index_phi.latch {
                    if next.replace(value).is_some() {
                        valid = false;
                    }
                } else {
                    valid = false;
                }
            }
            let (Some(initial), Some(next)) = (initial, next) else { continue };
            if !valid || const_of(func, initial) != Some(U256::ZERO) {
                continue;
            }
            let Some(max_step) = bounded_increment(func, next, cursor, 12) else { continue };
            if max_step.is_zero() {
                continue;
            }
            candidates.push(ScaledCursor {
                cursor,
                index,
                length,
                step,
                guard,
                wrapping_sum,
                preheader: index_phi.preheader,
                loop_blocks: loop_blocks.clone(),
                max_step,
            });
        }
    }
    candidates
        .sort_unstable_by_key(|candidate| (candidate.cursor.index(), candidate.index.index()));
    candidates.dedup_by_key(|candidate| (candidate.cursor, candidate.index));
    candidates
}

/// Finds the test guarding a loop body: the first branch from the header, past
/// checks whose failing side leaves the function, that compares the index or
/// its stepped sum with a length. Returns the length, the fact the body sees,
/// and whether that fact names a wrapping sum.
fn loop_guard(
    func: &Function,
    header: BlockId,
    loop_blocks: &DenseBitSet<BlockId>,
    index: ValueId,
    step: U256,
) -> Option<(ValueId, Relation, bool)> {
    let exits = |block: BlockId| {
        func.blocks[block].terminator.as_ref().is_some_and(|term| term.successors().is_empty())
    };
    let mut block = header;
    for _ in 0..8 {
        match *func.blocks[block].terminator.as_ref()? {
            Terminator::Branch { condition, then_block, else_block } => {
                match *inst_kind(func, condition)? {
                    InstKind::Lt(tested, length) if tested == index && step == U256::ONE => {
                        return Some((length, Relation::Lt(index, length), false));
                    }
                    InstKind::Gt(sum, length) | InstKind::Lt(length, sum)
                        if let Some(wrapping) = step_sum(func, sum, index, step) =>
                    {
                        return Some((length, Relation::Le(sum, length), wrapping));
                    }
                    _ => {}
                }
                block = if exits(then_block) {
                    else_block
                } else if exits(else_block) {
                    then_block
                } else {
                    return None;
                };
            }
            Terminator::Jump(next) => block = next,
            _ => return None,
        }
        if block == header || !loop_blocks.contains(block) {
            return None;
        }
    }
    None
}

/// Whether `sum` adds the constant `step` to `index`, and if so whether the
/// sum wraps (`Some(true)`) or is a checked `u256` sum (`Some(false)`).
fn step_sum(func: &Function, sum: ValueId, index: ValueId, step: U256) -> Option<bool> {
    let (a, b, wrapping) = match *inst_kind(func, sum)? {
        InstKind::Add(a, b) => (a, b, true),
        InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        } => (lhs, rhs, false),
        _ => return None,
    };
    let matches =
        |base: ValueId, offset: ValueId| base == index && const_of(func, offset) == Some(step);
    (matches(a, b) || matches(b, a)).then_some(wrapping)
}

fn natural_loop_blocks(
    header: BlockId,
    latch: BlockId,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    block_count: usize,
) -> DenseBitSet<BlockId> {
    let mut blocks = DenseBitSet::new_empty(block_count);
    blocks.insert(header);
    let mut pending = vec![latch];
    while let Some(block) = pending.pop() {
        if !blocks.insert(block) {
            continue;
        }
        for &pred in &preds[block] {
            if pred != header {
                pending.push(pred);
            }
        }
    }
    blocks
}

/// Maximum nonnegative increment represented by `next` relative to `base`.
fn bounded_increment(func: &Function, next: ValueId, base: ValueId, depth: usize) -> Option<U256> {
    if next == base {
        return Some(U256::ZERO);
    }
    let depth = depth.checked_sub(1)?;
    match inst_kind(func, next)? {
        InstKind::Add(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs: a,
            rhs: b,
        } => {
            if *a == base {
                bounded_value_max(func, *b, depth)
            } else if *b == base {
                bounded_value_max(func, *a, depth)
            } else {
                None
            }
        }
        InstKind::Phi(incoming) => incoming
            .iter()
            .map(|&(_, value)| bounded_increment(func, value, base, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        InstKind::Select(_, then_value, else_value) => [*then_value, *else_value]
            .into_iter()
            .map(|value| bounded_increment(func, value, base, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        _ => None,
    }
}

/// Maximum value of a constant/phi/select expression.
fn bounded_value_max(func: &Function, value: ValueId, depth: usize) -> Option<U256> {
    if let Some(constant) = const_of(func, value) {
        return Some(constant);
    }
    let depth = depth.checked_sub(1)?;
    match inst_kind(func, value)? {
        InstKind::Phi(incoming) => incoming
            .iter()
            .map(|&(_, value)| bounded_value_max(func, value, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        InstKind::Select(_, then_value, else_value) => [*then_value, *else_value]
            .into_iter()
            .map(|value| bounded_value_max(func, value, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        InstKind::Zext(source) => bounded_value_max(func, *source, depth),
        InstKind::Trunc(source, bits) if (1..=256).contains(bits) => {
            let mask = U256::MAX >> (256 - bits);
            bounded_value_max(func, *source, depth).map(|value| value.min(mask))
        }
        InstKind::And(a, b) => match (const_of(func, *a), const_of(func, *b)) {
            (Some(mask), None) => bounded_value_max(func, *b, depth).map(|value| value.min(mask)),
            (None, Some(mask)) => bounded_value_max(func, *a, depth).map(|value| value.min(mask)),
            _ => None,
        },
        InstKind::ExtractValue { aggregate, index, .. } => {
            bounded_aggregate_field_max(func, *aggregate, *index, depth)
        }
        _ => None,
    }
}

fn bounded_aggregate_field_max(
    func: &Function,
    aggregate: ValueId,
    field: u32,
    depth: usize,
) -> Option<U256> {
    let depth = depth.checked_sub(1)?;
    match inst_kind(func, aggregate)? {
        InstKind::InsertValue { aggregate, index, value, .. } => {
            if *index == field {
                bounded_value_max(func, *value, depth)
            } else {
                bounded_aggregate_field_max(func, *aggregate, field, depth)
            }
        }
        InstKind::Phi(incoming) => incoming
            .iter()
            .map(|&(_, value)| bounded_aggregate_field_max(func, value, field, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        InstKind::Select(_, then_value, else_value) => [*then_value, *else_value]
            .into_iter()
            .map(|value| bounded_aggregate_field_max(func, value, field, depth))
            .try_fold(U256::ZERO, |max, value| value.map(|value| max.max(value))),
        _ => None,
    }
}

/// Splits `base + width` when `width` is a bounded constant expression.
fn add_with_bounded_width(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
    let (a, b) = match inst_kind(func, value)? {
        InstKind::Add(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs: a,
            rhs: b,
        } => (*a, *b),
        _ => return None,
    };
    if let Some(width) = bounded_value_max(func, b, 12) {
        Some((a, width))
    } else {
        bounded_value_max(func, a, 12).map(|width| (b, width))
    }
}

/// Indexes `relation` under its left operand, and under both operands for an
/// equality, so transitive searches can leave from either side.
fn index_relation(index: &mut FxHashMap<ValueId, SmallVec<[Relation; 2]>>, relation: Relation) {
    let (a, b) = relation.operands();
    let entry = index.entry(a).or_default();
    if !entry.contains(&relation) {
        entry.push(relation);
    }
    if matches!(relation, Relation::Eq(..)) && a != b {
        let entry = index.entry(b).or_default();
        if !entry.contains(&relation) {
            entry.push(relation);
        }
    }
}

/// Finds header phis `p = phi [pre: init], [latch: p - c]` or `[latch: p + c]`
/// with a constant nonzero step whose header dominates the latch but not the
/// outside predecessor. Only phis that can influence a branch are considered.
fn monotone_phi_candidates(
    func: &Function,
    cfg: &CfgInfo,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    relevant: &DenseBitSet<ValueId>,
) -> Vec<MonotonePhi> {
    let mut candidates = Vec::new();
    let cyclic = cfg.cyclic_blocks();
    if cyclic.is_empty() {
        return candidates;
    }
    let definitions = func.inst_blocks();
    let dominators = cfg.dominators();
    for header in cyclic.iter() {
        for &inst in &func.blocks[header].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
            let Some(value) = func.inst_result_value(inst) else { continue };
            if !relevant.contains(value) {
                continue;
            }
            let [(first_block, first), (second_block, second)] = incoming.as_slice() else {
                continue;
            };
            let (pre, initial, latch, next) = match (
                dominators.dominates(header, *first_block),
                dominators.dominates(header, *second_block),
            ) {
                (false, true) => (*first_block, *first, *second_block, *second),
                (true, false) => (*second_block, *second, *first_block, *first),
                _ => continue,
            };
            if !preds[header].contains(&pre) || !preds[header].contains(&latch) {
                continue;
            }
            let Value::Inst(next_inst) = func.value(next) else { continue };
            let (step, decreasing) = match func.inst(*next_inst).kind {
                InstKind::Sub(base, step) if base == value => (step, true),
                InstKind::Add(base, step) if base == value => (step, false),
                InstKind::Add(step, base) if base == value => (step, false),
                _ => continue,
            };
            if !const_of(func, step).is_some_and(|step| !step.is_zero()) {
                continue;
            }
            let Some(&home) = definitions.get(next_inst) else { continue };
            candidates.push(MonotonePhi {
                header,
                value,
                initial,
                preheader: pre,
                latch,
                next,
                step,
                home,
                decreasing,
            });
        }
    }
    candidates
}

/// Splits `(x + y) >> 1` or `(x + y) / 2` into the sum and its addends.
fn halved_sum(func: &Function, value: ValueId) -> Option<(ValueId, ValueId, ValueId)> {
    let sum = match *inst_kind(func, value)? {
        InstKind::Shr(shift, sum) if const_of(func, shift) == Some(U256::ONE) => sum,
        InstKind::Div(sum, divisor) if const_of(func, divisor) == Some(U256::from(2)) => sum,
        _ => return None,
    };
    let InstKind::Add(x, y) = *inst_kind(func, sum)? else { return None };
    Some((sum, x, y))
}

/// Finds header phis whose latch value is the phi itself or an update, through
/// at most a few phis, and that are not constant-step monotone candidates.
/// Each is proposed in both directions; only one can have every update proven.
fn bounded_phi_candidates(
    func: &Function,
    cfg: &CfgInfo,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    relevant: &DenseBitSet<ValueId>,
    monotone: &[MonotonePhi],
) -> Vec<BoundedPhi> {
    let mut candidates = Vec::new();
    let cyclic = cfg.cyclic_blocks();
    if cyclic.is_empty() {
        return candidates;
    }
    let definitions = func.inst_blocks();
    let dominators = cfg.dominators();
    for header in cyclic.iter() {
        for &inst in &func.blocks[header].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
            let Some(value) = func.inst_result_value(inst) else { continue };
            if !relevant.contains(value) || monotone.iter().any(|phi| phi.value == value) {
                continue;
            }
            let mut initial = None;
            let mut updates = SmallVec::new();
            let mut valid = true;
            for &(pred, input) in incoming {
                if !preds[header].contains(&pred) {
                    continue;
                }
                if !dominators.dominates(header, pred) {
                    valid &= initial.replace(input).is_none();
                } else {
                    valid &= collect_updates(func, &definitions, value, input, &mut updates, 4);
                }
            }
            let Some(initial) = initial else { continue };
            if !valid || updates.is_empty() {
                continue;
            }
            for decreasing in [true, false] {
                candidates.push(BoundedPhi {
                    header,
                    value,
                    initial,
                    decreasing,
                    updates: updates.clone(),
                });
            }
        }
    }
    candidates
}

/// Collects the instructions a latch input can carry into `phi`, looking through
/// at most `depth` phis. Fails on any other input, such as an argument.
fn collect_updates(
    func: &Function,
    definitions: &FxHashMap<InstId, BlockId>,
    phi: ValueId,
    input: ValueId,
    updates: &mut SmallVec<[(ValueId, BlockId); 2]>,
    depth: usize,
) -> bool {
    if input == phi {
        return true;
    }
    let Value::Inst(inst) = *func.value(input) else { return false };
    if let InstKind::Phi(incoming) = &func.inst(inst).kind
        && let Some(depth) = depth.checked_sub(1)
    {
        return incoming
            .iter()
            .all(|&(_, inner)| collect_updates(func, definitions, phi, inner, updates, depth));
    }
    let Some(&home) = definitions.get(&inst) else { return false };
    if !updates.contains(&(input, home)) {
        updates.push((input, home));
    }
    true
}

/// Returns the fact implied on the unique dominating edge into `block`:
/// the branch condition of its sole predecessor and whether it is true.
fn dominating_edge_fact(
    func: &Function,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    block: BlockId,
) -> Option<(ValueId, bool)> {
    let preds = &preds[block];
    let (&first, rest) = preds.split_first()?;
    if rest.iter().any(|&pred| pred != first) {
        return None;
    }
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
            let (inner, one) = if const_of(func, b) == Some(U256::from(1)) {
                (a, b)
            } else if const_of(func, a) == Some(U256::from(1)) {
                (b, a)
            } else {
                return None;
            };
            debug_assert!(const_of(func, one).is_some());
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
    let initial = const_of(func, phi.initial)?;
    let step = const_of(func, phi.step)?;
    let travel = step.checked_shl(Target::MAX_TRIP_COUNT_BITS)?;
    if phi.decreasing {
        Some(Range::new(initial.checked_sub(travel)?, initial))
    } else {
        Some(Range::new(initial, initial.checked_add(travel)?))
    }
}

/// Splits `x + c` with a literal `c` into `(x, c)`; any other value has an
/// offset of zero. The flag is set for a checked `u256` sum, which is defined
/// only where it does not wrap.
fn shifted_operand(func: &Function, value: ValueId) -> (ValueId, U256, bool) {
    let (x, c, exact) = match inst_kind(func, value) {
        Some(&InstKind::Add(x, c)) => (x, c, false),
        Some(&InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        }) => (lhs, rhs, true),
        _ => return (value, U256::ZERO, false),
    };
    match (const_of(func, x), const_of(func, c)) {
        (_, Some(offset)) => (x, offset, exact),
        (Some(offset), None) => (c, offset, exact),
        (None, None) => (value, U256::ZERO, false),
    }
}

fn const_of(func: &Function, value: ValueId) -> Option<U256> {
    match func.value(value) {
        Value::Immediate(imm) => imm.as_u256(),
        _ => None,
    }
}

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    }
}

/// Returns true if both values are the same SSA value or the same constant.
fn values_equal(func: &Function, a: ValueId, b: ValueId) -> bool {
    if a == b {
        return true;
    }
    match (const_of(func, a), const_of(func, b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Whether two SSA values read the same memory-object length and no instruction
/// that executes from allocation through the loop may change that length. This
/// lets a loop-header reload stand in for the load used to size a checked
/// allocation without relying on CSE.
fn same_stable_length(func: &Function, a: ValueId, b: ValueId, candidate: &ScaledCursor) -> bool {
    if a == b {
        return true;
    }
    let (
        Some(&InstKind::MemoryObjectLen(a_object, a_kind)),
        Some(&InstKind::MemoryObjectLen(b_object, b_kind)),
    ) = (inst_kind(func, a), inst_kind(func, b))
    else {
        return false;
    };
    if a_object != b_object || a_kind != b_kind {
        return false;
    }
    let (Value::Inst(a_inst), Value::Inst(_)) = (func.value(a), func.value(b)) else {
        return false;
    };
    let aa = AliasAnalysis::new(func);
    let Some(location) = aa.memory_object_length_location(func, *a_inst, a_object, a_kind) else {
        return false;
    };
    let location = Location::Memory(location);
    std::iter::once(candidate.preheader)
        .chain(candidate.loop_blocks.iter())
        .flat_map(|block| func.blocks[block].instructions.iter().copied())
        .all(|inst| {
            let effects = aa.instruction_mod_ref(func, inst);
            !effects.may_write(&aa, location)
                || writes_only_fresh_object(func, &func.inst(inst).kind)
        })
}

/// Whether a write is confined to an object produced by this function's
/// allocator. A fresh allocation and its header/payload cannot overlap an
/// already-live input object under the MIR allocation contract.
fn writes_only_fresh_object(func: &Function, kind: &InstKind) -> bool {
    let object = match *kind {
        InstKind::Alloc { .. } => return true,
        InstKind::SetMemoryObjectLen(object, ..)
        | InstKind::MemoryObjectStoreField { object, .. }
        | InstKind::MemoryObjectStoreElement { object, .. }
        | InstKind::MemoryObjectStoreByte { object, .. }
        | InstKind::MemoryObjectStoreWord { object, .. }
        | InstKind::MemoryObjectCopyFromSlice { object, .. }
        | InstKind::MemoryObjectCopyFromSliceAt { object, .. } => object,
        InstKind::MemoryObjectCopy { destination, .. } => destination,
        _ => return false,
    };
    matches!(inst_kind(func, object), Some(InstKind::Alloc { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_intersection_and_union() {
        let a = Range::new(U256::from(0), U256::from(10));
        let b = Range::new(U256::from(5), U256::from(20));
        assert_eq!(a.intersect(b), Some(Range::new(U256::from(5), U256::from(10))));
        assert_eq!(a.union(b), Range::new(U256::from(0), U256::from(20)));

        let disjoint = Range::new(U256::from(11), U256::from(12));
        assert_eq!(a.intersect(disjoint), None);
    }
}
