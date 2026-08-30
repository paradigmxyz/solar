//! Stack shuffler for stack layout transitions.
//!
//! This module converts a source stack layout to a target layout using DUP, SWAP, and POP
//! operations. Layouts of up to four words compare nontrivial greedy results with a bounded
//! shortest-action search and take the searched sequence only when it improves one objective
//! without worsening action count, static gas, or encoded size. Larger layouts use the verified
//! greedy result, with the bounded search as the correctness fallback when the greedy pass cannot
//! reach the target. When exact search reaches its state cap it stops enqueueing successors but
//! drains the existing frontier, preserving targets that were discovered before the cap.
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
//! Swaps between equal tracked values are omitted. A transition is returned only
//! when the modeled source reaches the exact target.

use super::model::StackModel;
use crate::{backend::evm::op::StackOp, mir::ValueId};
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::{FxHashMap, StdEntry};
use std::collections::VecDeque;

const MAX_LAYOUT_SEARCH_STATES: usize = 100_000;
const EXACT_LAYOUT_OPTIMIZATION_LIMIT: usize = 4;

type Layout = SmallVec<[Option<ValueId>; 16]>;
type Predecessors = FxHashMap<Layout, Option<(Layout, StackOp)>>;

/// Result of a shuffle operation.
#[derive(Clone, Debug)]
pub(crate) struct ShuffleResult {
    /// The sequence of operations to perform.
    pub ops: Vec<StackOp>,
}

/// Represents a slot in the target layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetSlot {
    /// A specific value must be in this slot.
    Value(ValueId),
}

/// The stack shuffler transforms a source stack layout to a target layout.
pub(crate) struct StackShuffler<'a> {
    /// Current source stack (mutable during shuffling).
    source: Layout,
    /// Target layout we're shuffling to.
    target: &'a [TargetSlot],
    /// Operations generated so far.
    ops: Vec<StackOp>,
    /// Target used to compare logical operations by their lowered cost.
    evm_version: EvmVersion,
    /// Multiplicity: how many copies of each value are needed.
    multiplicities: FxHashMap<ValueId, usize>,
}

impl<'a> StackShuffler<'a> {
    /// Creates a new shuffler to transform source to target layout.
    #[cfg(test)]
    pub(crate) fn new(source: &StackModel, target: &'a [TargetSlot]) -> Self {
        Self::for_evm_version(source, target, EvmVersion::Osaka)
    }

    /// Creates a shuffler for an EVM version.
    pub(crate) fn for_evm_version(
        source: &StackModel,
        target: &'a [TargetSlot],
        evm_version: EvmVersion,
    ) -> Self {
        let source_stack: Layout = source.as_slice().iter().copied().collect();

        // Count multiplicities in the target.
        let mut multiplicities = FxHashMap::default();
        for slot in target {
            let TargetSlot::Value(v) = slot;
            *multiplicities.entry(*v).or_default() += 1;
        }

        Self { source: source_stack, target, ops: Vec::new(), evm_version, multiplicities }
    }

    fn max_stack_access(&self) -> usize {
        self.evm_version.reachable_stack_depth()
    }

    /// Performs the shuffle and returns the result.
    pub(crate) fn shuffle(mut self) -> Option<ShuffleResult> {
        let original = self.source.clone();
        let max_stack_access = self.max_stack_access();

        // Phase 1: Ensure we have enough copies of each value.
        self.ensure_multiplicities();

        // Phase 2: Arrange values to match target positions.
        self.arrange_positions();

        // Phase 3: Pop excess values.
        self.pop_excess();

        let greedy = Self::matches_target(&self.source, self.target)
            .then_some(ShuffleResult { ops: self.ops });
        let operation_lower_bound = original
            .len()
            .abs_diff(self.target.len())
            .max(usize::from(!Self::matches_target(&original, self.target)));
        if original.len().max(self.target.len()) <= EXACT_LAYOUT_OPTIMIZATION_LIMIT
            && greedy.as_ref().is_none_or(|result| {
                Self::lowered_cost(&result.ops, self.evm_version).0 > operation_lower_bound
            })
        {
            let exact =
                Self::search_exact(original, self.target, &self.multiplicities, max_stack_access);
            return match (greedy, exact) {
                (Some(greedy), Some(exact)) => {
                    let (exact_actions, exact_gas, exact_size) =
                        Self::lowered_cost(&exact.ops, self.evm_version);
                    let (greedy_actions, greedy_gas, greedy_size) =
                        Self::lowered_cost(&greedy.ops, self.evm_version);
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
            Self::search_exact(original, self.target, &self.multiplicities, max_stack_access)
        })
    }

    fn search_exact(
        source: Layout,
        target: &[TargetSlot],
        multiplicities: &FxHashMap<ValueId, usize>,
        max_stack_access: usize,
    ) -> Option<ShuffleResult> {
        let mut queue = VecDeque::new();
        let mut predecessors = FxHashMap::default();
        predecessors.insert(source.clone(), None);
        queue.push_back(source);

        while let Some(stack) = queue.pop_front() {
            if Self::matches_target(&stack, target) {
                let mut ops = Vec::new();
                let mut current = stack;
                while let Some((previous, op)) = predecessors[&current].clone() {
                    ops.push(op);
                    current = previous;
                }
                ops.reverse();
                return Some(ShuffleResult { ops });
            }
            if predecessors.len() >= MAX_LAYOUT_SEARCH_STATES {
                continue;
            }
            let max_swap = stack.len().saturating_sub(1).min(max_stack_access);
            for depth in 1..=max_swap {
                if stack[0] == stack[depth] {
                    continue;
                }
                let mut next = stack.clone();
                next.swap(0, depth);
                Self::enqueue(
                    &mut queue,
                    &mut predecessors,
                    &stack,
                    next,
                    StackOp::Swap(depth as u8),
                );
            }

            if stack.len() > target.len() {
                let mut next = stack.clone();
                next.remove(0);
                Self::enqueue(&mut queue, &mut predecessors, &stack, next, StackOp::Pop);
            }

            for (&value, &required) in multiplicities {
                let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
                if current >= required {
                    continue;
                }
                let Some(depth) =
                    stack.iter().take(max_stack_access).position(|&slot| slot == Some(value))
                else {
                    continue;
                };
                let mut next = stack.clone();
                next.insert(0, Some(value));
                Self::enqueue(
                    &mut queue,
                    &mut predecessors,
                    &stack,
                    next,
                    StackOp::Dup((depth + 1) as u8),
                );
            }
        }

        None
    }

    fn lowered_cost(ops: &[StackOp], evm_version: EvmVersion) -> (usize, usize, usize) {
        ops.iter().fold((0, 0, 0), |(instructions, gas, size), op| {
            let metrics = op.metrics(evm_version).expect("valid shuffle operation");
            (
                instructions + metrics.instruction_count,
                gas + metrics.static_gas,
                size + metrics.assembled_len,
            )
        })
    }

    fn enqueue(
        queue: &mut VecDeque<Layout>,
        predecessors: &mut Predecessors,
        previous: &Layout,
        next: Layout,
        op: StackOp,
    ) {
        if predecessors.len() >= MAX_LAYOUT_SEARCH_STATES {
            return;
        }
        if let StdEntry::Vacant(entry) = predecessors.entry(next) {
            let next = entry.key().clone();
            entry.insert(Some((previous.clone(), op)));
            queue.push_back(next);
        }
    }

    fn matches_target(source: &[Option<ValueId>], target: &[TargetSlot]) -> bool {
        source.len() == target.len()
            && source.iter().zip(target).all(|(&source, target)| match target {
                TargetSlot::Value(value) => source == Some(*value),
            })
    }

    /// Phase 1: Ensure we have enough copies of each value in source.
    fn ensure_multiplicities(&mut self) {
        let mut source_counts = FxHashMap::<_, usize>::default();
        for value in self.source.iter().flatten() {
            *source_counts.entry(*value).or_default() += 1;
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
            self.source.insert(0, Some(value));
            for _ in 1..missing {
                self.ops.push(StackOp::Dup(1));
                self.source.insert(0, Some(value));
            }
        }
    }

    /// Phase 2: Arrange values to match target positions using SWAPs.
    fn arrange_positions(&mut self) {
        // Work from top of stack downward.
        for target_depth in 0..self.target.len().min(self.source.len()) {
            let TargetSlot::Value(target_value) = self.target[target_depth];
            if self.source.get(target_depth) == Some(&Some(target_value)) {
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
            if let Some(exchange) = StackOp::from_swaps(target_depth, source_depth, target_depth) {
                self.ops.push(exchange);
                self.source.swap(usize::from(target_depth), usize::from(source_depth));
            } else {
                // Bring the selected value through the top when `EXCHANGE` cannot encode these
                // two depths.
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
        for value in self.source.iter().flatten() {
            *source_counts.entry(*value).or_default() += 1;
        }

        let mut pop_count = 0;
        for slot in &self.source {
            let can_pop = if let Some(value) = slot {
                let current = source_counts.get_mut(value).expect("counted source value");
                let needed = self.multiplicities.get(value).copied().unwrap_or(0);
                if *current > needed {
                    *current -= 1;
                    true
                } else {
                    false
                }
            } else {
                self.source.len() - pop_count > self.target.len()
            };
            if !can_pop {
                break;
            }
            pop_count += 1;
        }

        if Self::matches_target(&self.source[pop_count..], self.target) {
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
        self.source.iter().position(|&v| v == Some(value))
    }

    /// Find a value starting from a minimum depth.
    fn find_value_from(&self, value: ValueId, min_depth: usize) -> Option<usize> {
        self.source
            .iter()
            .enumerate()
            .skip(min_depth)
            .find(|(_, v)| **v == Some(value))
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::stack::model::MAX_STACK_ACCESS;

    fn make_model(values: &[Option<ValueId>]) -> StackModel {
        let mut model = StackModel::new();
        // Push in reverse order so the first element ends up on top.
        for &v in values.iter().rev() {
            if let Some(val) = v {
                model.push(val);
            } else {
                model.push_unknown();
            }
        }
        model
    }

    fn assert_reaches(source: &StackModel, target: &[TargetSlot], result: &ShuffleResult) {
        let mut actual = source.clone();
        for &op in &result.ops {
            actual.apply(op);
        }
        assert!(StackShuffler::matches_target(actual.as_slice(), target));
    }

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
    fn test_shuffle_already_correct() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let source = make_model(&[Some(v0), Some(v1)]);
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        assert!(result.ops.is_empty());
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_swap_needed() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        // Source: [v1, v0] (v1 on top).
        let source = make_model(&[Some(v1), Some(v0)]);
        // Target: [v0, v1] (v0 on top).
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        assert!(result.ops.contains(&StackOp::Swap(1)));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_dup_needed() {
        let v0 = ValueId::from_usize(0);

        // Source: [v0].
        let source = make_model(&[Some(v0)]);
        // Target: [v0, v0] (needs two copies).
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v0)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Dup(_))));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_pop_excess() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        // Source: [v0, v1] (v0 on top).
        let source = make_model(&[Some(v0), Some(v1)]);
        // Target: [v1] (only needs v1).
        let target = [TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        // Should swap v1 to the top, then pop v0.
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Pop | StackOp::Swap(_))));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_pops_duplicate_suffix() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);
        let source = make_model(&[Some(v0), Some(v1), Some(v2), Some(v0), Some(v1), Some(v2)]);
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1), TargetSlot::Value(v2)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_pops_large_duplicate_suffix() {
        let values = [23usize, 53, 17, 47, 11, 41, 5, 35, 53, 17, 41, 11, 35, 5, 47, 23];
        let source = make_model(
            &values.iter().copied().map(ValueId::from_usize).map(Some).collect::<Vec<_>>(),
        );
        let target = [23usize, 53, 17, 47, 11, 41, 5, 35]
            .into_iter()
            .map(ValueId::from_usize)
            .map(TargetSlot::Value)
            .collect::<Vec<_>>();

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_complex_rearrangement() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        // Source: [v0, v1, v2] (v0 on top).
        let source = make_model(&[Some(v0), Some(v1), Some(v2)]);
        // Target: [v2, v0, v1] (v2 on top).
        let target = [TargetSlot::Value(v2), TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        // Should use swaps to rearrange.
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Swap(_))));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_amsterdam_shuffle_prefers_smaller_encoding() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);
        let source = make_model(&[Some(v0), Some(v1), Some(v2)]);
        let target = [TargetSlot::Value(v1), TargetSlot::Value(v2), TargetSlot::Value(v0)];

        let result = StackShuffler::for_evm_version(&source, &target, EvmVersion::Amsterdam)
            .shuffle()
            .unwrap();

        assert_eq!(result.ops, [StackOp::Swap(2), StackOp::Swap(1)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_preserves_fixed_duplicate_prefix() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let source = make_model(&[Some(v0), Some(v1), Some(v0)]);
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_eq!(result.ops, [StackOp::Swap(1), StackOp::Swap(2)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_legacy_shuffle_uses_exchange() {
        let values: Vec<_> = (0..4).map(ValueId::from_usize).collect();
        let source = make_model(&values.iter().copied().map(Some).collect::<Vec<_>>());
        let target = [
            TargetSlot::Value(values[0]),
            TargetSlot::Value(values[2]),
            TargetSlot::Value(values[1]),
            TargetSlot::Value(values[3]),
        ];

        let result =
            StackShuffler::for_evm_version(&source, &target, EvmVersion::Osaka).shuffle().unwrap();

        assert_eq!(result.ops, [StackOp::Exchange(1, 2)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_uses_extended_exchange() {
        let values: Vec<_> = (0..18).map(ValueId::from_usize).collect();
        let source = make_model(&values.iter().copied().map(Some).collect::<Vec<_>>());
        let mut target_values = values;
        target_values.swap(1, 17);
        let target: Vec<_> = target_values.into_iter().map(TargetSlot::Value).collect();

        let result = StackShuffler::for_evm_version(&source, &target, EvmVersion::Amsterdam)
            .shuffle()
            .unwrap();

        assert_eq!(result.ops, [StackOp::Exchange(1, 17)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_uses_extended_swap() {
        let values: Vec<_> = (0..18).map(ValueId::from_usize).collect();
        let source = make_model(&values.iter().copied().map(Some).collect::<Vec<_>>());
        let mut target_values = values;
        target_values.swap(0, 17);
        let target: Vec<_> = target_values.into_iter().map(TargetSlot::Value).collect();

        let result = StackShuffler::for_evm_version(&source, &target, EvmVersion::Amsterdam)
            .shuffle()
            .unwrap();

        assert_eq!(result.ops, [StackOp::Swap(17)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_optimizes_duplicate_placement() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let source = make_model(&[Some(v0), Some(v1)]);
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1), TargetSlot::Value(v0)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_eq!(result.ops, [StackOp::Swap(1), StackOp::Dup(2)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_removes_anonymous_word_below_target() {
        let v0 = ValueId::from_usize(0);
        let source = make_model(&[Some(v0), None]);
        let target = [TargetSlot::Value(v0)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_eq!(result.ops, [StackOp::Swap(1), StackOp::Pop]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_uses_swap16() {
        let values: Vec<_> = (0..=MAX_STACK_ACCESS).map(ValueId::from_usize).collect();
        let source = make_model(&values.iter().copied().map(Some).collect::<Vec<_>>());
        let mut target_values = values;
        target_values.swap(0, MAX_STACK_ACCESS);
        let target: Vec<_> = target_values.into_iter().map(TargetSlot::Value).collect();

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        assert_eq!(result.ops, [StackOp::Swap(MAX_STACK_ACCESS as u8)]);
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_missing_value_fails_without_partial_result() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let source = make_model(&[Some(v0)]);
        let target = [TargetSlot::Value(v1)];

        assert!(StackShuffler::new(&source, &target).shuffle().is_none());
    }

    #[test]
    fn exact_search_drains_frontier_after_state_limit() {
        let values = (0..8).map(ValueId::from_usize).collect::<Vec<_>>();
        let source = values.iter().copied().map(Some).collect::<Layout>();
        let target = values[..5].iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
        let multiplicities = target.iter().fold(FxHashMap::default(), |mut counts, target| {
            let TargetSlot::Value(value) = target;
            *counts.entry(*value).or_default() += 1;
            counts
        });

        let result =
            StackShuffler::search_exact(source, &target, &multiplicities, MAX_STACK_ACCESS)
                .unwrap();

        assert_eq!(
            result.ops,
            [
                StackOp::Swap(3),
                StackOp::Swap(6),
                StackOp::Pop,
                StackOp::Swap(3),
                StackOp::Swap(6),
                StackOp::Pop,
                StackOp::Swap(3),
                StackOp::Pop,
            ]
        );
    }

    #[test]
    fn exhaustive_small_reachable_layouts_are_optimal() {
        let values = [ValueId::from_usize(0), ValueId::from_usize(1), ValueId::from_usize(2)];
        let sources: Vec<_> = (1..=4).flat_map(|len| sequences(&values, len)).collect();
        let targets: Vec<_> = (0..=4).flat_map(|len| sequences(&values, len)).collect();

        for source_values in &sources {
            let source = make_model(
                &source_values.iter().copied().map(Some).collect::<Vec<Option<ValueId>>>(),
            );
            for target_values in &targets {
                if target_values.iter().any(|value| !source_values.contains(value)) {
                    continue;
                }
                let target: Vec<_> = target_values.iter().copied().map(TargetSlot::Value).collect();
                let result = StackShuffler::new(&source, &target).shuffle().unwrap_or_else(|| {
                    panic!("failed to shuffle {source_values:?} to {target_values:?}")
                });
                let shuffler = StackShuffler::new(&source, &target);
                let exact = StackShuffler::search_exact(
                    shuffler.source,
                    &target,
                    &shuffler.multiplicities,
                    MAX_STACK_ACCESS,
                )
                .unwrap();
                assert!(
                    result.ops.len() <= exact.ops.len(),
                    "non-minimal shuffle from {source_values:?} to {target_values:?}: \
                     greedy={:?}, exact={:?}",
                    result.ops,
                    exact.ops
                );
                assert_reaches(&source, &target, &result);
            }
        }
    }
}
