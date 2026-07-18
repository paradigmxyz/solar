//! Stack shuffler for optimal stack layout transitions.
//!
//! This module implements a greedy shuffler algorithm similar to solc's approach.
//! The shuffler converts a source stack layout to a target layout using minimal
//! DUP, SWAP, and POP operations.
//!
//! ## Algorithm Overview
//!
//! The shuffler uses a greedy approach with multiplicity tracking:
//! 1. Count how many copies of each value are needed in the target
//! 2. DUP values that need multiple copies
//! 3. SWAP values to correct positions
//! 4. POP excess values
use super::model::{MAX_STACK_ACCESS, StackModel, StackOp};
use crate::mir::ValueId;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;

/// Result of a shuffle operation.
#[derive(Clone, Debug, Default)]
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
    source: SmallVec<[Option<ValueId>; 16]>,
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
        let source_stack: SmallVec<[Option<ValueId>; 16]> =
            source.as_slice().iter().copied().collect();

        // Count multiplicities in target
        let mut multiplicities = FxHashMap::default();
        for slot in target {
            let TargetSlot::Value(v) = slot;
            *multiplicities.entry(*v).or_insert(0) += 1;
        }

        Self { source: source_stack, target, ops: Vec::new(), multiplicities }
    }

    /// Performs the shuffle and returns the result.
    pub(crate) fn shuffle(mut self) -> ShuffleResult {
        // Phase 1: Ensure we have enough copies of each value
        self.ensure_multiplicities();

        // Phase 2: Arrange values to match target positions
        self.arrange_positions();

        // Phase 3: Pop excess values
        self.pop_excess();

        ShuffleResult { ops: self.ops }
    }

    /// Phase 1: Ensure we have enough copies of each value in source.
    fn ensure_multiplicities(&mut self) {
        // For each value, check if we have enough copies
        for (&value, &needed) in self.multiplicities.iter() {
            let current_count = self.source.iter().filter(|&&v| v == Some(value)).count();
            if current_count < needed {
                // Need to DUP this value
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
        // Work from top of stack downward
        for target_depth in 0..self.target.len().min(self.source.len()) {
            let target_slot = &self.target[target_depth];

            match target_slot {
                TargetSlot::Value(target_val) => {
                    // Check if correct value is already at this position
                    if self.source.get(target_depth) == Some(&Some(*target_val)) {
                        continue;
                    }

                    // Find where the target value currently is
                    if let Some(source_depth) = self.find_value_from(*target_val, target_depth)
                        && source_depth != target_depth
                        && source_depth < MAX_STACK_ACCESS
                    {
                        // Need to swap
                        if target_depth == 0 {
                            // Simple case: swap to top
                            let swap_n = source_depth as u8;
                            if (1..=16).contains(&swap_n) {
                                self.ops.push(StackOp::Swap(swap_n));
                                self.source.swap(0, source_depth);
                            }
                        } else {
                            // Need to bring target value to position target_depth
                            // First swap current top to target_depth, then bring value to
                            // top, then swap back
                            if target_depth < MAX_STACK_ACCESS && source_depth < MAX_STACK_ACCESS {
                                // Swap top with target_depth position
                                let swap1 = target_depth as u8;
                                if (1..=16).contains(&swap1) {
                                    self.ops.push(StackOp::Swap(swap1));
                                    self.source.swap(0, target_depth);
                                }

                                // Now find where the value we need is
                                if let Some(new_depth) = self.find_value(*target_val)
                                    && new_depth > 0
                                    && new_depth < MAX_STACK_ACCESS
                                {
                                    let swap2 = new_depth as u8;
                                    if (1..=16).contains(&swap2) {
                                        self.ops.push(StackOp::Swap(swap2));
                                        self.source.swap(0, new_depth);
                                    }
                                }

                                // Swap back to put the value at target_depth
                                if (1..=16).contains(&swap1) {
                                    self.ops.push(StackOp::Swap(swap1));
                                    self.source.swap(0, target_depth);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Phase 3: Pop excess values from the stack.
    fn pop_excess(&mut self) {
        // Count how many of each value we still need
        let mut still_needed: FxHashMap<ValueId, usize> = FxHashMap::default();
        for slot in self.target.iter() {
            let TargetSlot::Value(v) = slot;
            *still_needed.entry(*v).or_insert(0) += 1;
        }

        // Pop values from top that are no longer needed
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
                // Unknown value on top - pop it if target is shorter
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

    #[test]
    fn test_shuffle_already_correct() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let source = make_model(&[Some(v0), Some(v1)]);
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle();
        assert!(result.ops.is_empty());
    }

    #[test]
    fn test_shuffle_swap_needed() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        // Source: [v1, v0] (v1 on top)
        let source = make_model(&[Some(v1), Some(v0)]);
        // Target: [v0, v1] (v0 on top)
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle();
        assert!(result.ops.contains(&StackOp::Swap(1)));
    }

    #[test]
    fn test_shuffle_dup_needed() {
        let v0 = ValueId::from_usize(0);

        // Source: [v0]
        let source = make_model(&[Some(v0)]);
        // Target: [v0, v0] (need two copies)
        let target = [TargetSlot::Value(v0), TargetSlot::Value(v0)];

        let result = StackShuffler::new(&source, &target).shuffle();
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Dup(_))));
    }

    #[test]
    fn test_shuffle_pop_excess() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        // Source: [v0, v1] (v0 on top)
        let source = make_model(&[Some(v0), Some(v1)]);
        // Target: [v1] (only need v1)
        let target = [TargetSlot::Value(v1)];

        let result = StackShuffler::new(&source, &target).shuffle();
        // Should swap v1 to top, then pop v0
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Pop | StackOp::Swap(_))));
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

        let result = StackShuffler::new(&source, &target).shuffle();

        // Should use swaps to rearrange
        assert!(result.ops.iter().any(|op| matches!(op, StackOp::Swap(_))));
    }
}
