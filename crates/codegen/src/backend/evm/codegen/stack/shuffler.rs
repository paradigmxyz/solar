//! Stack shuffler for stack layout transitions.
//!
//! This module converts a source stack layout to a target layout using DUP, SWAP, and POP
//! operations. Layouts of up to four words compare nontrivial greedy results with a bounded
//! shortest-action search and take the searched sequence only when it improves one objective
//! without worsening action count, static gas, or encoded size. Larger non-permutation layouts
//! use the verified greedy result, with the bounded search as the correctness fallback when it
//! cannot reach the target. When exact search reaches its state cap it stops enqueueing successors
//! but drains the existing frontier, preserving targets that were discovered before the cap.
//! Physical runs with unique values use a direct deletion-order and permutation solver, which
//! avoids a wider graph search for the SWAP/POP cleanup sequences seen in generated code.
//!
//! ## Algorithm overview
//!
//! The fast path uses a greedy approach with multiplicity tracking:
//!
//! 1. Count how many copies of each value are needed in the target.
//! 2. DUP values that need multiple copies.
//! 3. SWAP values to correct positions.
//! 4. POP excess values.
//!
//! Swaps between equal values are omitted. A transition is returned only when the modeled
//! source reaches the exact target. Equal-multiplicity layouts need no pushes or pops. Unique
//! permutations use the direct cycle solver up to the target's SWAP reach; permutations with
//! duplicates use exact search through eight words (at most 20,160 arrangements with one repeated
//! word). Runs whose source words are known to repeat always take this path. The greedy result
//! wins cost ties. Exact results are cached by complete symbolic layout and EVM version within
//! each worker; generated wrappers and repeated cleanup passes frequently ask the same bounded
//! question.

use super::model::StackModel;
use crate::{
    backend::evm::op::{StackOp, StackOpMetrics},
    mir::ValueId,
};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::{FxHashMap, StdEntry};
use std::{cell::RefCell, collections::VecDeque, ops::ControlFlow};

const MAX_LAYOUT_SEARCH_STATES: usize = 100_000;
const MAX_SHARED_EXACT_SEARCHES: usize = 2_048;
const EXACT_LAYOUT_OPTIMIZATION_LIMIT: usize = 4;
const EXACT_PERMUTATION_LIMIT: usize = 8;
const MAX_ENUMERATED_PHYSICAL_REMOVALS: usize = 7;
const PHYSICAL_RESYNTHESIS_LAYOUT_LIMIT: usize = 236;

type Layout = SmallVec<[ValueId; 16]>;
type VisitedLayouts = FxHashMap<Layout, usize>;

#[derive(Clone, Copy)]
struct Predecessor {
    previous: usize,
    op: StackOp,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ExactSearchKey {
    source: Layout,
    target: Layout,
    max_stack_access: usize,
    evm_version: EvmVersion,
}

type ExactSearchCache = FxHashMap<ExactSearchKey, Option<Vec<StackOp>>>;

thread_local! {
    static EXACT_SEARCH_CACHE: RefCell<ExactSearchCache> = RefCell::default();
}

pub(crate) fn lowered_stack_cost(
    ops: &[StackOp],
    evm_version: EvmVersion,
) -> (usize, usize, usize) {
    ops.iter().fold((0, 0, 0), |(instructions, gas, size), op| {
        let metrics = op.metrics(evm_version).expect("valid stack operation");
        (
            instructions + metrics.instruction_count,
            gas + metrics.static_gas,
            size + metrics.assembled_len,
        )
    })
}

/// Resynthesizes a bounded physical stack operation sequence from its symbolic result.
///
/// `equal[i]` names, for the word at depth `i` before the run, the shallowest word known to hold
/// the same value; words past its end are distinct. The result contains `EXCHANGE` only when
/// `exchanges` is set.
pub(crate) fn resynthesize_physical_ops(
    ops: &[StackOp],
    evm_version: EvmVersion,
    exchanges: bool,
    equal: &[u8],
) -> Option<Vec<StackOp>> {
    let mut source_depth = 0usize;
    let mut available = 0usize;
    for &stack_op in ops {
        stack_op.lowering(evm_version)?;
        let required = stack_op.required_depth();
        if available < required {
            source_depth += required - available;
            available = required;
        }
        available = available.checked_add_signed(stack_op.net_growth())?;
    }
    if source_depth.max(available) > PHYSICAL_RESYNTHESIS_LAYOUT_LIMIT {
        return None;
    }

    let source = StackModel::from_top_to_bottom((0..source_depth).map(|depth| {
        ValueId::from_usize(equal.get(depth).map_or(depth, |&same| usize::from(same)))
    }));
    let mut target = source.clone();
    for &stack_op in ops {
        target.apply(stack_op);
    }
    let target = target.as_slice();
    // The permutation solver assumes distinct source words.
    let distinct = equal
        .iter()
        .take(source_depth)
        .enumerate()
        .all(|(depth, &same)| usize::from(same) == depth);
    let permutation = distinct
        .then(|| synthesize_unique_layout(source.as_slice(), target, evm_version))
        .flatten();
    if !evm_version.has_extended_stack_ops() && permutation.is_some() {
        return permutation;
    }
    let mut shuffler = StackShuffler::new(source.as_slice(), target, evm_version, exchanges);
    let words = source_depth.max(target.len());
    let shuffled = if words <= EXACT_LAYOUT_OPTIMIZATION_LIMIT
        || (!distinct && words <= EXACT_PERMUTATION_LIMIT)
    {
        shuffler.shuffle()
    } else {
        shuffler.run_greedy()
    }
    .filter(|ops| ops.iter().all(|op| op.lowering(evm_version).is_some()));
    match (permutation, shuffled) {
        (Some(permutation), Some(shuffled)) => Some(
            if lowered_stack_cost(&permutation, evm_version)
                <= lowered_stack_cost(&shuffled, evm_version)
            {
                permutation
            } else {
                shuffled
            },
        ),
        (ops @ Some(_), None) | (None, ops @ Some(_)) => ops,
        (None, None) => None,
    }
}

fn synthesize_unique_layout(
    source: &[ValueId],
    target: &[ValueId],
    evm_version: EvmVersion,
) -> Option<Vec<StackOp>> {
    if source.len() < target.len() {
        return None;
    }
    if target.is_empty() {
        return Some(vec![StackOp::Pop; source.len()]);
    }

    let source = Layout::from_slice(source);
    let mut target_values = Layout::new();
    for &value in target {
        if target_values.contains(&value) || !source.contains(&value) {
            return None;
        }
        target_values.push(value);
    }

    let mut removed = source
        .iter()
        .copied()
        .filter(|value| !target_values.contains(value))
        .collect::<SmallVec<[ValueId; 16]>>();
    if removed.len() > MAX_ENUMERATED_PHYSICAL_REMOVALS {
        return None;
    }
    removed.sort_unstable();
    let mut search = RemovalSearch {
        evm_version,
        pop: StackOp::Pop.metrics(evm_version)?,
        swap: StackOp::Swap(1).metrics(evm_version)?,
        target: &target_values,
        removed: &removed,
        current: source,
        ops: Vec::new(),
        best: None,
    };
    match search.visit(0, (0, 0, 0)) {
        ControlFlow::Break(ops) => Some(ops),
        ControlFlow::Continue(()) => search.best.map(|(ops, _)| ops),
    }
}

/// A depth-first search over removal orders in lexicographic order of the removed values,
/// keeping the first cheapest complete sequence.
struct RemovalSearch<'a> {
    evm_version: EvmVersion,
    pop: StackOpMetrics,
    swap: StackOpMetrics,
    target: &'a [ValueId],
    /// Values to remove; the ones the current prefix has not removed yet are still in `current`.
    removed: &'a [ValueId],
    current: SmallVec<[ValueId; 16]>,
    ops: Vec<StackOp>,
    best: Option<(Vec<StackOp>, (usize, usize, usize))>,
}

impl RemovalSearch<'_> {
    /// Extends the current prefix of `depth` removals, breaking with a sequence nothing can beat.
    fn visit(
        &mut self,
        depth: usize,
        prefix_cost: (usize, usize, usize),
    ) -> ControlFlow<Vec<StackOp>> {
        let count = self.removed.len();
        if depth == count {
            let mut current = self.current.clone();
            let mut ops = self.ops.clone();
            ops.extend(synthesize_unique_permutation(&mut current, self.target));
            if ops.iter().all(|op| op.lowering(self.evm_version).is_some()) {
                let cost = lowered_stack_cost(&ops, self.evm_version);
                // Removing k words requires at least k POPs. Nothing can improve
                // a sequence that meets this bound, regardless of removal order.
                if ops.len() == count {
                    return ControlFlow::Break(ops);
                }
                if self.best.as_ref().is_none_or(|(_, best_cost)| cost < *best_cost) {
                    self.best = Some((ops, cost));
                }
            }
            return ControlFlow::Continue(());
        }
        for &value in self.removed {
            let Some(position) = self.current.iter().position(|&current| current == value) else {
                continue;
            };
            let start = self.ops.len();
            if position != 0 {
                self.ops.push(StackOp::Swap(position as u8));
                self.current.swap(0, position);
            }
            self.ops.push(StackOp::Pop);
            self.current.remove(0);
            // Costs only grow, every remaining removal needs a `POP`, and a `SWAP` must come
            // next unless the top is removed next or the layout is final. No order sharing this
            // prefix can beat `best` then, and a prefix without a lowering makes every such order
            // invalid. Skip them all.
            let cost =
                self.ops[start..].iter().try_fold(prefix_cost, |(instructions, gas, size), op| {
                    let metrics = op.metrics(self.evm_version)?;
                    Some((
                        instructions + metrics.instruction_count,
                        gas + metrics.static_gas,
                        size + metrics.assembled_len,
                    ))
                });
            let pops = count - depth - 1;
            let swaps = usize::from(if pops == 0 {
                self.current.as_slice() != self.target
            } else {
                !self.removed.contains(&self.current[0])
            });
            if let Some(cost) = cost
                && self.best.as_ref().is_none_or(|(_, best_cost)| {
                    let bound = (
                        cost.0
                            + pops * self.pop.instruction_count
                            + swaps * self.swap.instruction_count,
                        cost.1 + pops * self.pop.static_gas + swaps * self.swap.static_gas,
                        cost.2 + pops * self.pop.assembled_len + swaps * self.swap.assembled_len,
                    );
                    bound < *best_cost
                })
            {
                self.visit(depth + 1, cost)?;
            }
            // Restore the prefix before trying the next value.
            self.current.insert(0, value);
            if position != 0 {
                self.current.swap(0, position);
            }
            self.ops.truncate(start);
        }
        ControlFlow::Continue(())
    }
}

fn synthesize_unique_permutation(
    current: &mut SmallVec<[ValueId; 16]>,
    target: &[ValueId],
) -> Vec<StackOp> {
    let mut ops = Vec::new();
    loop {
        let top_target = target.iter().position(|&target| target == current[0]).unwrap();
        if top_target != 0 {
            ops.push(StackOp::Swap(top_target as u8));
            current.swap(0, top_target);
            continue;
        }

        let Some(cycle) =
            current.iter().zip(target).position(|(&current, &target)| current != target)
        else {
            return ops;
        };
        ops.push(StackOp::Swap(cycle as u8));
        current.swap(0, cycle);
    }
}

/// The stack shuffler transforms a source stack layout to a target layout.
struct StackShuffler<'a> {
    /// Current source stack (mutable during shuffling).
    source: Layout,
    /// Target layout we're shuffling to.
    target: &'a [ValueId],
    /// Operations generated so far.
    ops: Vec<StackOp>,
    /// Target used to compare logical operations by their lowered cost.
    evm_version: EvmVersion,
    /// Multiplicity: how many copies of each value are needed.
    multiplicities: FxHashMap<ValueId, usize>,
    /// Whether two non-top words may swap with one `EXCHANGE` instead of three `SWAP`s.
    exchanges: bool,
}

impl<'a> StackShuffler<'a> {
    fn new(
        source: &[ValueId],
        target: &'a [ValueId],
        evm_version: EvmVersion,
        exchanges: bool,
    ) -> Self {
        let mut multiplicities = FxHashMap::default();
        for &value in target {
            *multiplicities.entry(value).or_default() += 1;
        }
        Self {
            source: Layout::from_slice(source),
            target,
            ops: Vec::new(),
            evm_version,
            multiplicities,
            exchanges,
        }
    }

    fn max_stack_access(&self) -> usize {
        self.evm_version.reachable_stack_depth()
    }

    /// Performs the shuffle and returns the result.
    fn shuffle(mut self) -> Option<Vec<StackOp>> {
        let original = self.source.clone();
        let max_stack_access = self.max_stack_access();
        let greedy = self.run_greedy();
        let permutation = original.len() == self.target.len()
            && original.len() <= max_stack_access + 1
            && self.multiplicities.iter().all(|(&value, &count)| {
                original.iter().filter(|&&slot| slot == value).count() == count
            });
        let unique = permutation && self.multiplicities.values().all(|&count| count == 1);
        let operation_lower_bound = original
            .len()
            .abs_diff(self.target.len())
            .max(usize::from(original.as_slice() != self.target));
        if (original.len().max(self.target.len()) <= EXACT_LAYOUT_OPTIMIZATION_LIMIT
            || unique
            || (permutation && original.len() <= EXACT_PERMUTATION_LIMIT))
            && greedy.as_ref().is_none_or(|result| {
                lowered_stack_cost(result, self.evm_version).0 > operation_lower_bound
            })
        {
            let exact = if unique {
                synthesize_unique_layout(&original, self.target, self.evm_version)
            } else {
                Self::search_exact(
                    original,
                    self.target,
                    &self.multiplicities,
                    max_stack_access,
                    self.evm_version,
                )
            };
            return match (greedy, exact) {
                (Some(greedy), Some(exact)) => {
                    let (exact_actions, exact_gas, exact_size) =
                        lowered_stack_cost(&exact, self.evm_version);
                    let (greedy_actions, greedy_gas, greedy_size) =
                        lowered_stack_cost(&greedy, self.evm_version);
                    let use_exact = exact_actions <= greedy_actions
                        && exact_gas <= greedy_gas
                        && exact_size <= greedy_size
                        && (exact_actions < greedy_actions
                            || exact_gas < greedy_gas
                            || exact_size < greedy_size);
                    if use_exact { Some(exact) } else { Some(greedy) }
                }
                (None, Some(exact)) => Some(exact),
                (greedy, None) => greedy,
            };
        }

        greedy.or_else(|| {
            Self::search_exact(
                original,
                self.target,
                &self.multiplicities,
                max_stack_access,
                self.evm_version,
            )
        })
    }

    fn run_greedy(&mut self) -> Option<Vec<StackOp>> {
        self.ensure_multiplicities();
        self.arrange_positions();
        self.pop_excess();
        (self.source.as_slice() == self.target).then(|| std::mem::take(&mut self.ops))
    }

    fn search_exact(
        source: Layout,
        target: &[ValueId],
        multiplicities: &FxHashMap<ValueId, usize>,
        max_stack_access: usize,
        evm_version: EvmVersion,
    ) -> Option<Vec<StackOp>> {
        let key = ExactSearchKey {
            source: source.clone(),
            target: Layout::from_slice(target),
            max_stack_access,
            evm_version,
        };
        if let Some(ops) = EXACT_SEARCH_CACHE.with_borrow(|cache| cache.get(&key).cloned()) {
            return ops;
        }

        let result = Self::search_exact_uncached(source, target, multiplicities, max_stack_access);
        EXACT_SEARCH_CACHE.with_borrow_mut(|cache| {
            if cache.len() == MAX_SHARED_EXACT_SEARCHES {
                cache.clear();
            }
            cache.insert(key, result.clone());
        });
        result
    }

    fn search_exact_uncached(
        source: Layout,
        target: &[ValueId],
        multiplicities: &FxHashMap<ValueId, usize>,
        max_stack_access: usize,
    ) -> Option<Vec<StackOp>> {
        let mut queue = VecDeque::new();
        let mut visited = FxHashMap::default();
        let mut predecessors = Vec::<Option<Predecessor>>::new();
        predecessors.push(None);
        visited.insert(source.clone(), 0);
        queue.push_back((0, source));

        while let Some((state, stack)) = queue.pop_front() {
            if stack.as_slice() == target {
                let mut ops = Vec::new();
                let mut current = state;
                while let Some(predecessor) = predecessors[current] {
                    ops.push(predecessor.op);
                    current = predecessor.previous;
                }
                ops.reverse();
                return Some(ops);
            }
            if visited.len() >= MAX_LAYOUT_SEARCH_STATES {
                continue;
            }
            let max_swap = stack.len().saturating_sub(1).min(max_stack_access);
            for depth in 1..=max_swap {
                if stack[0] == stack[depth] {
                    continue;
                }
                let mut next = Layout::clone(&stack);
                next.swap(0, depth);
                Self::enqueue(
                    &mut queue,
                    &mut visited,
                    &mut predecessors,
                    state,
                    next,
                    StackOp::Swap(depth as u8),
                );
            }

            if stack.len() > target.len() {
                let mut next = Layout::clone(&stack);
                next.remove(0);
                Self::enqueue(
                    &mut queue,
                    &mut visited,
                    &mut predecessors,
                    state,
                    next,
                    StackOp::Pop,
                );
            }

            for (&value, &required) in multiplicities {
                let current = stack.iter().filter(|&&slot| slot == value).count();
                if current >= required {
                    continue;
                }
                let Some(depth) =
                    stack.iter().take(max_stack_access).position(|&slot| slot == value)
                else {
                    continue;
                };
                let mut next = Layout::clone(&stack);
                next.insert(0, value);
                Self::enqueue(
                    &mut queue,
                    &mut visited,
                    &mut predecessors,
                    state,
                    next,
                    StackOp::Dup((depth + 1) as u8),
                );
            }
        }

        None
    }

    fn enqueue(
        queue: &mut VecDeque<(usize, Layout)>,
        visited: &mut VisitedLayouts,
        predecessors: &mut Vec<Option<Predecessor>>,
        previous: usize,
        next: Layout,
        op: StackOp,
    ) {
        if visited.len() >= MAX_LAYOUT_SEARCH_STATES {
            return;
        }
        if let StdEntry::Vacant(entry) = visited.entry(next) {
            let state = predecessors.len();
            let next = entry.key().clone();
            entry.insert(state);
            predecessors.push(Some(Predecessor { previous, op }));
            queue.push_back((state, next));
        }
    }

    /// Phase 1: Ensure we have enough copies of each value in source.
    fn ensure_multiplicities(&mut self) {
        let mut source_counts = FxHashMap::<_, usize>::default();
        for &value in &self.source {
            *source_counts.entry(value).or_default() += 1;
        }

        for (&value, &needed) in &self.multiplicities {
            let current = source_counts.get(&value).copied().unwrap_or(0);
            let missing = needed.saturating_sub(current);
            if missing == 0 {
                continue;
            }
            let Some(depth) =
                self.find_value(value).filter(|&depth| depth < self.max_stack_access())
            else {
                continue;
            };

            self.ops.push(StackOp::Dup((depth + 1) as u8));
            self.source.insert(0, value);
            for _ in 1..missing {
                self.ops.push(StackOp::Dup(1));
                self.source.insert(0, value);
            }
        }
    }

    /// Phase 2: Arrange values to match target positions using SWAPs.
    fn arrange_positions(&mut self) {
        // Work from top of stack downward.
        for target_depth in 0..self.target.len().min(self.source.len()) {
            let target_value = self.target[target_depth];
            if self.source.get(target_depth) == Some(&target_value) {
                continue;
            }
            let Some(source_depth) = self.find_value_from(target_value, target_depth) else {
                continue;
            };
            let max_stack_access = self.max_stack_access();
            if source_depth == target_depth || source_depth > max_stack_access {
                continue;
            }
            if target_depth == 0 {
                self.swap(source_depth);
                continue;
            }
            if target_depth > max_stack_access {
                continue;
            }

            let target_depth = target_depth as u8;
            let source_depth = source_depth as u8;
            if self.exchanges
                && let Some(exchange) =
                    StackOp::from_swaps(target_depth, source_depth, target_depth)
            {
                self.ops.push(exchange);
                self.source.swap(usize::from(target_depth), usize::from(source_depth));
            } else {
                // Bring the selected value through the top when `EXCHANGE` is disabled or cannot
                // encode these two depths.
                self.swap(usize::from(target_depth));
                self.swap(usize::from(source_depth));
                self.swap(usize::from(target_depth));
            }
        }
    }

    fn swap(&mut self, depth: usize) {
        if self.source[0] != self.source[depth] {
            self.ops.push(StackOp::Swap(depth as u8));
            self.source.swap(0, depth);
        }
    }

    /// Phase 3: Pop excess values from the stack.
    fn pop_excess(&mut self) {
        let mut source_counts = FxHashMap::<_, usize>::default();
        for &value in &self.source {
            *source_counts.entry(value).or_default() += 1;
        }

        let mut pop_count = 0;
        for value in &self.source {
            let current = source_counts.get_mut(value).expect("counted source value");
            if *current <= self.multiplicities.get(value).copied().unwrap_or(0) {
                break;
            }
            *current -= 1;
            pop_count += 1;
        }

        if self.source[pop_count..] == *self.target {
            self.ops.extend(std::iter::repeat_n(StackOp::Pop, pop_count));
            self.source.drain(..pop_count);
            return;
        }

        while self.source.len() > self.target.len() {
            let depth = self.target.len();
            if depth == 0 {
                self.ops.push(StackOp::Pop);
                self.source.remove(0);
                continue;
            }
            if depth > self.max_stack_access() {
                break;
            }

            // The arrangement phase fixes the target prefix, so remove the first excess word
            // without discarding that prefix.
            self.swap(depth);
            self.ops.push(StackOp::Pop);
            self.source.remove(0);
            for restore_depth in 1..depth {
                self.swap(restore_depth);
            }
        }
    }

    /// Find the depth of a value in source stack.
    fn find_value(&self, value: ValueId) -> Option<usize> {
        self.source.iter().position(|&v| v == value)
    }

    /// Find a value starting from a minimum depth.
    fn find_value_from(&self, value: ValueId, min_depth: usize) -> Option<usize> {
        self.source.iter().enumerate().skip(min_depth).find(|(_, v)| **v == value).map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequences(values: &[ValueId], len: usize) -> Vec<Vec<ValueId>> {
        if len == 0 {
            return vec![Vec::new()];
        }
        let shorter = sequences(values, len - 1);
        let mut result = Vec::with_capacity(shorter.len() * values.len());
        for prefix in shorter {
            for &value in values {
                let mut sequence = prefix.clone();
                sequence.push(value);
                result.push(sequence);
            }
        }
        result
    }

    #[test]
    fn exhaustive_small_reachable_layouts_are_optimal() {
        let values = [ValueId::from_usize(0), ValueId::from_usize(1), ValueId::from_usize(2)];
        let sources: Vec<_> = (1..=4).flat_map(|len| sequences(&values, len)).collect();
        let targets: Vec<_> = (0..=4).flat_map(|len| sequences(&values, len)).collect();

        for source_values in &sources {
            let source = StackModel::from_top_to_bottom(source_values.iter().copied());
            for target in &targets {
                if target.iter().any(|value| !source_values.contains(value)) {
                    continue;
                }
                let shuffler = StackShuffler::new(source_values, target, EvmVersion::Osaka, true);
                let exact = StackShuffler::search_exact(
                    shuffler.source.clone(),
                    target,
                    &shuffler.multiplicities,
                    16,
                    shuffler.evm_version,
                )
                .unwrap();
                let result = shuffler
                    .shuffle()
                    .unwrap_or_else(|| panic!("failed to shuffle {source_values:?} to {target:?}"));
                assert!(
                    result.len() <= exact.len(),
                    "non-minimal shuffle from {source_values:?} to {target:?}: \
                     greedy={result:?}, exact={exact:?}"
                );
                let mut actual = source.clone();
                for &op in &result {
                    actual.apply(op);
                }
                assert_eq!(actual.as_slice(), target.as_slice());
            }
        }
    }
}
