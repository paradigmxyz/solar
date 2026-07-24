//! Stack shuffler for stack layout transitions.
//!
//! This module converts a source stack layout to a target layout using DUP, SWAP, and POP
//! operations. Layouts of up to four words compare nontrivial greedy results with a bounded exact
//! search and take an exact sequence only when it improves one objective without worsening action
//! count or static gas. Larger layouts use the verified greedy result, with exact search as the
//! correctness fallback when the greedy pass cannot reach the target.
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

use super::model::{MAX_STACK_ACCESS, StackModel, StackOp};
use crate::mir::ValueId;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
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
    /// Multiplicity: how many copies of each value are needed.
    multiplicities: FxHashMap<ValueId, usize>,
}

impl<'a> StackShuffler<'a> {
    /// Creates a new shuffler to transform source to target layout.
    pub(crate) fn new(source: &StackModel, target: &'a [TargetSlot]) -> Self {
        let source_stack: Layout = source.as_slice().iter().copied().collect();

        // Count multiplicities in target
        let mut multiplicities = FxHashMap::default();
        for slot in target {
            let TargetSlot::Value(v) = slot;
            *multiplicities.entry(*v).or_insert(0) += 1;
        }

        Self { source: source_stack, target, ops: Vec::new(), multiplicities }
    }

    /// Performs the shuffle and returns the result.
    pub(crate) fn shuffle(mut self) -> Option<ShuffleResult> {
        let original = self.source.clone();

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
            && greedy.as_ref().is_none_or(|result| result.ops.len() > operation_lower_bound)
        {
            let exact = Self::search_exact(original, self.target, &self.multiplicities);
            return match (greedy, exact) {
                (Some(greedy), Some(exact)) => {
                    let exact_gas = Self::static_gas(&exact.ops);
                    let greedy_gas = Self::static_gas(&greedy.ops);
                    if exact.ops.len() <= greedy.ops.len()
                        && exact_gas <= greedy_gas
                        && (exact.ops.len() < greedy.ops.len() || exact_gas < greedy_gas)
                    {
                        Some(exact)
                    } else {
                        Some(greedy)
                    }
                }
                (None, Some(exact)) => Some(exact),
                (greedy, None) => greedy,
            };
        }

        greedy.or_else(|| Self::search_exact(original, self.target, &self.multiplicities))
    }

    fn search_exact(
        source: Layout,
        target: &[TargetSlot],
        multiplicities: &FxHashMap<ValueId, usize>,
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
                break;
            }

            let max_swap = stack.len().saturating_sub(1).min(MAX_STACK_ACCESS);
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
                    stack.iter().take(MAX_STACK_ACCESS).position(|&slot| slot == Some(value))
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

    fn static_gas(ops: &[StackOp]) -> usize {
        ops.iter().map(|op| if matches!(op, StackOp::Pop) { 2 } else { 3 }).sum()
    }

    fn enqueue(
        queue: &mut VecDeque<Layout>,
        predecessors: &mut Predecessors,
        previous: &Layout,
        next: Layout,
        op: StackOp,
    ) {
        if predecessors.contains_key(&next) {
            return;
        }
        predecessors.insert(next.clone(), Some((previous.clone(), op)));
        queue.push_back(next);
    }

    fn matches_target(source: &[Option<ValueId>], target: &[TargetSlot]) -> bool {
        source.len() == target.len()
            && source.iter().zip(target).all(|(&source, target)| match target {
                TargetSlot::Value(value) => source == Some(*value),
            })
    }

    /// Phase 1: Ensure we have enough copies of each value in source.
    fn ensure_multiplicities(&mut self) {
        // For each value, check if we have enough copies.
        for (&value, &needed) in self.multiplicities.iter() {
            let current_count = self.source.iter().filter(|&&v| v == Some(value)).count();
            if current_count < needed {
                // DUP this value until its target multiplicity is available.
                if let Some(depth) = self.find_value(value)
                    && depth < MAX_STACK_ACCESS
                {
                    for _ in current_count..needed {
                        let dup_n = (self.find_value(value).unwrap_or(0) + 1) as u8;
                        if dup_n <= 16 {
                            self.ops.push(StackOp::Dup(dup_n));
                            self.source.insert(0, Some(value));
                        }
                    }
                }
            }
        }
    }

    /// Phase 2: Arrange values to match target positions using SWAPs.
    fn arrange_positions(&mut self) {
        // Work from top of stack downward.
        for target_depth in 0..self.target.len().min(self.source.len()) {
            let target_slot = &self.target[target_depth];

            match target_slot {
                TargetSlot::Value(target_val) => {
                    // Check if the correct value is already at this position.
                    if self.source.get(target_depth) == Some(&Some(*target_val)) {
                        continue;
                    }

                    // Find where the target value currently is.
                    if let Some(source_depth) = self.find_value_from(*target_val, target_depth)
                        && source_depth != target_depth
                        && source_depth <= MAX_STACK_ACCESS
                    {
                        // Move the selected value into place.
                        if target_depth == 0 {
                            // The top position needs one swap.
                            let swap_n = source_depth as u8;
                            if (1..=16).contains(&swap_n) {
                                self.swap(source_depth);
                            }
                        } else {
                            // First swap the current top to `target_depth`, then bring the selected
                            // value to the top and swap it back.
                            if target_depth <= MAX_STACK_ACCESS && source_depth <= MAX_STACK_ACCESS
                            {
                                // Swap top with target_depth position
                                let swap1 = target_depth as u8;
                                if (1..=16).contains(&swap1) {
                                    self.swap(target_depth);
                                }

                                // Bring the occurrence selected before the first swap to the top.
                                // Re-searching here could pick an already-fixed duplicate above
                                // `target_depth` and disturb the prefix.
                                self.swap(source_depth);

                                // Swap back to put the value at target_depth
                                if (1..=16).contains(&swap1) {
                                    self.swap(target_depth);
                                }
                            }
                        }
                    }
                }
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
        // Count how many of each value we still need.
        let mut still_needed: FxHashMap<ValueId, usize> = FxHashMap::default();
        for slot in self.target.iter() {
            let TargetSlot::Value(v) = slot;
            *still_needed.entry(*v).or_insert(0) += 1;
        }

        // Pop values from the top that are no longer needed.
        while !self.source.is_empty() {
            if let Some(Some(top_val)) = self.source.first() {
                let needed = still_needed.get(top_val).copied().unwrap_or(0);
                let current = self.source.iter().filter(|&&v| v == Some(*top_val)).count();
                if current > needed {
                    self.ops.push(StackOp::Pop);
                    self.source.remove(0);
                } else {
                    break;
                }
            } else if self.source.first() == Some(&None) {
                // Pop an anonymous top value if the target is shorter.
                if self.source.len() > self.target.len() {
                    self.ops.push(StackOp::Pop);
                    self.source.remove(0);
                } else {
                    break;
                }
            } else {
                break;
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

    fn make_model(values: &[Option<ValueId>]) -> StackModel {
        let mut model = StackModel::new();
        // Push in reverse order so first element ends up on top
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
            match op {
                StackOp::Dup(depth) => actual.dup(depth),
                StackOp::Swap(depth) => actual.swap(depth),
                StackOp::Pop => {
                    actual.pop();
                }
            }
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

        // Source: [v1, v0] (v1 on top)
        let source = make_model(&[Some(v1), Some(v0)]);
        // Target: [v0, v1] (v0 on top)
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        assert!(result.ops.contains(&StackOp::Swap(1)));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_dup_needed() {
        let v0 = ValueId::from_usize(0);

        // Source: [v0]
        let source = make_model(&[Some(v0)]);
        // Target: [v0, v0] (need two copies)
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v0)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Dup(_))));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_pop_excess() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        // Source: [v0, v1] (v0 on top)
        let source = make_model(&[Some(v0), Some(v1)]);
        // Target: [v1] (only need v1)
        let target = [TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();
        // Should swap v1 to top, then pop v0
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Pop | StackOp::Swap(_))));
        assert_reaches(&source, &target, &result);
    }

    #[test]
    fn test_shuffle_complex_rearrangement() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        // Source: [v0, v1, v2] (v0 on top)
        let source = make_model(&[Some(v0), Some(v1), Some(v2)]);
        // Target: [v2, v0, v1] (v2 on top)
        let target = [TargetSlot::Value(v2), TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle().unwrap();

        // Should use swaps to rearrange
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Swap(_))));
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
                let exact =
                    StackShuffler::search_exact(shuffler.source, &target, &shuffler.multiplicities)
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
