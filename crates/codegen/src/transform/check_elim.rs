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

use crate::{
    analysis::CfgInfo,
    mir::{
        BlockId, Function, InstKind, Module, Terminator, Value, ValueId,
        utils::repair_reachability_phis,
    },
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Function pass for range-based overflow-check elimination.
pub(crate) struct CheckElim;

impl MirPass for CheckElim {
    fn name(&self) -> &'static str {
        "check-elim"
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        run_function_pass(module, |func| CheckEliminator::new().run(func) != 0)
    }
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

fn ordered(a: ValueId, b: ValueId) -> (ValueId, ValueId) {
    if a.index() <= b.index() { (a, b) } else { (b, a) }
}

/// Range-based overflow-check eliminator.
#[derive(Default)]
struct CheckEliminator {
    /// Statistics from the last run.
    stats: CheckElimStats,
    ranges: FxHashMap<ValueId, Range>,
    relations: FxHashSet<Relation>,
    range_undo: Vec<(ValueId, Option<Range>)>,
    relation_undo: Vec<Relation>,
}

impl CheckEliminator {
    /// Creates a new check eliminator.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Runs check elimination on a function. Returns the number of folded
    /// branches.
    fn run(&mut self, func: &mut Function) -> usize {
        self.stats = CheckElimStats::default();
        let cfg = CfgInfo::new(func);

        // Predecessors recomputed from reachable terminators: facts must only
        // come from edges that can actually execute.
        let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); func.blocks.len()];
        for &block in cfg.rpo() {
            for &succ in cfg.successors(block) {
                preds[succ.index()].push(block);
            }
        }

        let folds = self.collect_folds(func, &cfg, &preds);
        self.ranges.clear();
        self.relations.clear();
        self.range_undo.clear();
        self.relation_undo.clear();

        if folds.is_empty() {
            return 0;
        }
        for &(block, keep) in &folds {
            func.blocks[block].terminator = Some(Terminator::Jump(keep));
        }
        repair_reachability_phis(func);
        self.stats.branches_folded = folds.len();
        folds.len()
    }

    /// Walks the dominator tree, recording edge facts and evaluating branch
    /// conditions. Returns `(block, kept_target)` folds to apply.
    fn collect_folds(
        &mut self,
        func: &Function,
        cfg: &CfgInfo,
        preds: &[Vec<BlockId>],
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
        if Some(new) == old {
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

    fn has_relation(&self, relation: Relation) -> bool {
        self.relations.contains(&relation)
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
        let (x, y) = ordered(a, b);
        if self.has_relation(Relation::Lt(a, b)) {
            return Some(true);
        }
        if self.has_relation(Relation::Lt(b, a))
            || self.has_relation(Relation::Le(b, a))
            || self.has_relation(Relation::Eq(x, y))
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

        // Overflow check for checked add: `lt (add x, y), x` is the wrap
        // flag of `x + y`. If the maximum bounds cannot wrap the check is
        // false; if even the minimum bounds wrap it is true.
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

        None
    }

    /// Evaluates `a == b`, if provable.
    fn eval_eq(&mut self, func: &Function, a: ValueId, b: ValueId, depth: usize) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        let (x, y) = ordered(a, b);
        if self.has_relation(Relation::Eq(x, y)) {
            return Some(true);
        }
        if self.has_relation(Relation::Ne(x, y))
            || self.has_relation(Relation::Lt(a, b))
            || self.has_relation(Relation::Lt(b, a))
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

        // Overflow check for checked mul: `eq (div (mul x, y), y), x` holds
        // iff `x * y` did not wrap, provided the divisor is nonzero.
        if let Some(truth) = self.eval_muldiv_roundtrip(func, a, b, depth) {
            return Some(truth);
        }
        if let Some(truth) = self.eval_muldiv_roundtrip(func, b, a, depth) {
            return Some(truth);
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

/// Returns the fact implied on the unique dominating edge into `block`:
/// the branch condition of its sole predecessor and whether it is true.
fn dominating_edge_fact(
    func: &Function,
    preds: &[Vec<BlockId>],
    block: BlockId,
) -> Option<(ValueId, bool)> {
    let preds = &preds[block.index()];
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
        Value::Inst(inst_id) => Some(&func.instructions[*inst_id].kind),
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
