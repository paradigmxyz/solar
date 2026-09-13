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
//! not provable is left untouched.
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
//!
//! Transitive relational queries lazily index candidate edges once per function,
//! when a query needs to combine facts. An edge is followed only
//! when its fact is present in the current scope; the index itself proves
//! nothing. Search follows at most 128 states in stable block and operand order, carrying
//! whether an unsigned ordering path contains a strict edge. Exhausting this
//! bound leaves the check in place; disequality is never treated as transitive.
//!
//! Runtime-only functions also use bounds from zero-extended immutable encodings.
//! These bounds follow the target's actual immediate width, not the result's
//! nominal type. Constructor-reachable functions, including helpers shared with
//! runtime code, are excluded: constructor loads read full staging words. Signed
//! and left-aligned encodings remain unknown. The bounds are immutable facts and
//! are available to both the forward analysis and the dominator walk.
//! The separate `immutable-check-elim` adapter runs after ABI getter inlining,
//! selecting only runtime functions that load bounded immutables. This exposes
//! facts hidden behind getter calls during the ordinary earlier check passes.

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

use super::cfg_simplify::simplify_function;
use crate::mir::{
    BlockId, Function, FunctionId, ImmutableEncoding, ImmutableId, InstKind, Module, Terminator,
    Value, ValueId,
    analysis::{CallGraphInfo, CfgInfo},
    immutable::immutable_push_type_size,
    pass::{MirPass, run_function_pass, run_function_pass_with_cfg, run_selected_function_pass},
    utils::repair_reachability_phis,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::rc::Rc;

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
        run_function_pass(module, analyses, |func, _| {
            let mut eliminator = CheckEliminator::new(None);
            let changed = eliminator.run(func) != 0;
            let repaired = repair_reachability_phis(func);
            changed || repaired
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
        run_function_pass_with_cfg(module, analyses, |func, analyses| {
            let selected =
                gcx.sess.opts.optimization.is_gas().then(|| analyses.cfg().cyclic_blocks());
            if selected.is_some_and(DenseBitSet::is_empty) {
                return false;
            }
            let mut eliminator = CheckEliminator::new(None);
            eliminator.cfg = Some(Rc::clone(analyses.cfg()));
            let changed =
                eliminator.run_in_blocks(func, selected.map(|blocks| (blocks, &reverting))) != 0;
            if changed {
                // branch proven_condition, checked, panic => jump checked
                // Remove unreachable panic blocks and merge the successful continuation.
                let _ = repair_reachability_phis(func);
                let _ = simplify_function(func);
            }
            changed
        })
    }
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
            return false;
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
        run_selected_function_pass(module, analyses, &runtime_only, |func, analyses| {
            let mut eliminator = CheckEliminator::new(Some(&bounds));
            eliminator.cfg = Some(Rc::clone(analyses.cfg()));
            let changed = eliminator.run(func) != 0;
            let repaired = repair_reachability_phis(func);
            changed || repaired
        })
    }
}

/// Excludes every constructor-reachable helper, including recursive and tail-call edges.
fn runtime_only_functions(module: &Module) -> DenseBitSet<FunctionId> {
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

    fn union(self, other: Self) -> Self {
        Self { lo: self.lo.min(other.lo), hi: self.hi.max(other.hi) }
    }
}

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

/// Range-based overflow-check eliminator.
#[derive(Default)]
struct CheckEliminator<'a> {
    /// Context-independent bounds for runtime-only immutable loads.
    immutable_ranges: Option<&'a FxHashMap<ImmutableId, Range>>,
    /// Shared CFG snapshot taken at entry, matching the previous fresh build.
    cfg: Option<Rc<CfgInfo>>,
    /// Statistics from the last run.
    stats: CheckElimStats,
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
    /// Possible outgoing facts; each candidate still requires a scoped membership check.
    relation_index: Option<FxHashMap<ValueId, SmallVec<[Relation; 2]>>>,
    range_undo: Vec<(ValueId, Option<Range>)>,
    relation_undo: Vec<Relation>,
}

impl<'a> CheckEliminator<'a> {
    /// Creates a new check eliminator.
    #[must_use]
    fn new(immutable_ranges: Option<&'a FxHashMap<ImmutableId, Range>>) -> Self {
        Self { immutable_ranges, ..Self::default() }
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
        self.relation_index = None;
        if !func.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(Terminator::Branch { then_block, else_block, .. }) if then_block != else_block
            )
        }) {
            return 0;
        }
        let cfg = self.cfg.as_ref().map_or_else(|| Rc::new(CfgInfo::new(func)), Rc::clone);
        let relevant = branch_inputs(func, &cfg);
        if relevant.is_empty() {
            return 0;
        }

        // Predecessors recomputed from reachable terminators: facts must only
        // come from edges that can actually execute.
        let mut preds = index_vec![Vec::new(); func.blocks.len()];
        for &block in cfg.rpo() {
            for &succ in cfg.successors(block) {
                preds[succ].push(block);
            }
        }

        const MAX_ACYCLIC_JOIN_INSTRUCTIONS: usize = 128;
        let bounded_acyclic_join = cfg.cyclic_blocks().is_empty()
            && func.instructions().take(MAX_ACYCLIC_JOIN_INSTRUCTIONS + 1).count()
                > MAX_ACYCLIC_JOIN_INSTRUCTIONS;
        let facts = if selected.is_some() || bounded_acyclic_join {
            // Bound the additional search in large acyclic functions, where
            // copying fact sets through long chains is quadratic; the ordinary
            // dominator proof still runs. Cyclic functions retain fixed-point
            // propagation for induction ranges.
            index_vec![Facts::default(); func.blocks.len()]
        } else {
            self.join_facts(func, &cfg, &preds, &relevant)
        };
        let mut folds = self.collect_folds(func, &cfg, &preds, &facts);
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

        if folds.is_empty() {
            return 0;
        }
        // branch proven_condition, keep, discard => jump keep
        for &(block, keep) in &folds {
            func.blocks[block].terminator = Some(Terminator::Jump(keep));
        }
        self.stats.branches_folded = folds.len();
        folds.len()
    }

    /// Walks the dominator tree, recording edge facts and evaluating branch
    /// conditions. Returns `(block, kept_target)` folds to apply.
    fn collect_folds(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        preds: &IndexVec<BlockId, Vec<BlockId>>,
        facts: &IndexVec<BlockId, Facts>,
    ) -> Vec<(BlockId, BlockId)> {
        enum Walk {
            Enter(BlockId),
            Exit { range_mark: usize, relation_mark: usize },
        }

        let mut folds = Vec::new();
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
        folds
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
                    let Some(InstKind::IsZero(inner)) = inst_kind(func, value) else { break };
                    value = *inner;
                }
            }
        }
        let mut entries = index_vec![Facts::default(); func.blocks.len()];
        let mut exits = entries.clone();
        let mut cx = Self::new(self.immutable_ranges);
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
                let entry = merged.unwrap_or_default();
                cx.ranges.clone_from(&entry.ranges);
                cx.relations.clone_from(&entry.relations);
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
            InstKind::IsZero(a) => self.assume(func, a, !truth, depth),
            InstKind::Lt(a, b) => self.assume_lt(func, a, b, truth, depth),
            InstKind::Gt(a, b) => self.assume_lt(func, b, a, truth, depth),
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

    /// Records the consequences of `(a == b) == truth`.
    fn assume_eq(&mut self, func: &Function, a: ValueId, b: ValueId, truth: bool, depth: usize) {
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
            self.relation_undo.push(relation);
        }
    }

    fn has_relation(&mut self, func: &Function, relation: Relation) -> bool {
        if self.relations.contains(&relation) {
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
        if self.relations.len() <= 1 {
            // A single strict/equal edge also proves a non-strict comparison.
            // Every other single-edge implication was covered by direct lookup.
            return self.relations.iter().any(|fact| match *fact {
                Relation::Lt(a, b) => !equality_only && !needs_strict && a == start && b == end,
                Relation::Eq(a, b) => !needs_strict && ordered(start, end) == (a, b),
                Relation::Le(..) | Relation::Ne(..) => false,
            });
        }
        let index = self.relation_index.get_or_insert_with(|| relation_candidates(func));
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
                if !self.relations.contains(&fact) {
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
        false
    }

    // === Evaluation ===

    /// Computes a sound overapproximation of the values `value` can take at
    /// the current program point.
    fn range_of(&mut self, func: &Function, value: ValueId, depth: usize) -> Range {
        if let Some(constant) = const_of(func, value) {
            return Range::singleton(constant);
        }
        let mut range = self.ranges.get(&value).copied().unwrap_or(Range::FULL);
        let Some(depth) = depth.checked_sub(1) else { return range };
        let Some(kind) = inst_kind(func, value) else { return range };
        let derived = match *kind {
            InstKind::LoadImmutable(id) => self
                .immutable_ranges
                .and_then(|ranges| ranges.get(&id))
                .copied()
                .unwrap_or(Range::FULL),
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
                if ra.lo >= rb.hi { Range::new(ra.lo - rb.hi, ra.hi - rb.lo) } else { Range::FULL }
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
            InstKind::Lt(..)
            | InstKind::Gt(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Eq(..)
            | InstKind::IsZero(..) => match self.eval_truth(func, value, depth) {
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
        let depth = depth.checked_sub(1)?;
        let kind = inst_kind(func, value)?;
        match *kind {
            InstKind::Lt(a, b) => self.eval_lt(func, a, b, depth),
            InstKind::Gt(a, b) => self.eval_lt(func, b, a, depth),
            InstKind::Eq(a, b) => self.eval_eq(func, a, b, depth),
            InstKind::IsZero(a) => self.eval_truth(func, a, depth).map(|truth| !truth),
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

        // Underflow check variant `lt x, (sub x, y)`: equivalent to
        // `lt x, y` for every `y` (with wrapping subtraction).
        if let Some(&InstKind::Sub(x, y)) = inst_kind(func, b)
            && a == x
            && let Some(reduced_depth) = depth.checked_sub(1)
        {
            return self.eval_lt(func, x, y, reduced_depth);
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

        // Overflow check for checked mul: `eq (div (mul x, y), y), x` holds
        // iff `x * y` did not wrap, provided the divisor is nonzero. Recognize
        // it before deriving the complete ranges of both expression trees.
        if let Some(truth) = self.eval_muldiv_roundtrip(func, a, b, depth) {
            return Some(truth);
        }
        if let Some(truth) = self.eval_muldiv_roundtrip(func, b, a, depth) {
            return Some(truth);
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
fn relation_candidates(func: &Function) -> FxHashMap<ValueId, SmallVec<[Relation; 2]>> {
    let mut index = FxHashMap::<_, SmallVec<[Relation; 2]>>::default();
    let mut seen = FxHashSet::default();
    let mut add = |relation: Relation| {
        if seen.insert(relation) {
            let (a, b) = relation.operands();
            index.entry(a).or_default().push(relation);
            if matches!(relation, Relation::Eq(..)) && a != b {
                index.entry(b).or_default().push(relation);
            }
        }
    };
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
                Some(&InstKind::Eq(a, b) | &InstKind::Sub(a, b) | &InstKind::Xor(a, b)) => {
                    let (x, y) = ordered(a, b);
                    add(Relation::Eq(x, y));
                }
                Some(&InstKind::IsZero(a)) => pending.push(a),
                Some(&InstKind::And(a, b) | &InstKind::Or(a, b)) => {
                    pending.extend([b, a]);
                }
                _ => {}
            }
        }
    }
    index
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
