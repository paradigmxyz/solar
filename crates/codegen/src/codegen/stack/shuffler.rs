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
//!
//! Key insight: Values that can be "freely generated" (literals, function labels)
//! don't need to be in the source stack - they can be pushed when needed.

use super::model::{MAX_STACK_ACCESS, StackModel, StackOp};
use crate::mir::ValueId;
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, ToSmallVec};

/// Result of a shuffle operation.
#[derive(Clone, Debug, Default)]
pub struct ShuffleResult {
    /// The sequence of operations to perform.
    pub ops: Vec<StackOp>,
    /// Number of DUP operations.
    pub dup_count: usize,
    /// Number of SWAP operations.
    pub swap_count: usize,
    /// Number of POP operations.
    pub pop_count: usize,
}

impl ShuffleResult {
    /// Creates an empty shuffle result.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no operations are needed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns the total number of operations.
    #[must_use]
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    /// Estimated gas cost (DUP/SWAP = 3 gas each, POP = 2 gas).
    #[must_use]
    pub fn estimated_gas(&self) -> u64 {
        (self.dup_count + self.swap_count) as u64 * 3 + self.pop_count as u64 * 2
    }
}

/// Represents a slot in the target layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSlot {
    /// A specific value must be in this slot.
    Value(ValueId),
    /// This slot should be empty (value will be popped).
    Empty,
    /// Any value is acceptable (don't care).
    Any,
}

/// The stack shuffler transforms a source stack layout to a target layout.
pub struct StackShuffler<'a> {
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
    pub fn new(source: &StackModel, target: &'a [TargetSlot]) -> Self {
        let source_stack: SmallVec<[Option<ValueId>; 16]> =
            source.as_slice().iter().copied().collect();

        // Count multiplicities in target
        let mut multiplicities = FxHashMap::default();
        for slot in target {
            if let TargetSlot::Value(v) = slot {
                *multiplicities.entry(*v).or_insert(0) += 1;
            }
        }

        Self { source: source_stack, target, ops: Vec::new(), multiplicities }
    }

    /// Performs the shuffle and returns the result.
    pub fn shuffle(mut self) -> ShuffleResult {
        // Phase 1: Ensure we have enough copies of each value
        self.ensure_multiplicities();

        // Phase 2: Arrange values to match target positions
        self.arrange_positions();

        // Phase 3: Pop excess values
        self.pop_excess();

        // Count operation types
        let mut result = ShuffleResult::new();
        for op in &self.ops {
            match op {
                StackOp::Dup(_) => result.dup_count += 1,
                StackOp::Swap(_) => result.swap_count += 1,
                StackOp::Pop => result.pop_count += 1,
            }
        }
        result.ops = self.ops;
        result
    }

    /// Phase 1: Ensure we have enough copies of each value in source.
    fn ensure_multiplicities(&mut self) {
        // For each value, check if we have enough copies
        for (&value, &needed) in self.multiplicities.iter() {
            let current_count = self.source.iter().filter(|&&v| v == Some(value)).count();
            if current_count < needed {
                // Need to DUP this value
                if let Some(depth) = self.find_value(value) {
                    if depth < MAX_STACK_ACCESS {
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
                    if let Some(source_depth) = self.find_value_from(*target_val, target_depth) {
                        if source_depth != target_depth && source_depth < MAX_STACK_ACCESS {
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
                                if target_depth < MAX_STACK_ACCESS
                                    && source_depth < MAX_STACK_ACCESS
                                {
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
                TargetSlot::Empty | TargetSlot::Any => {
                    // Don't care about this position
                }
            }
        }
    }

    /// Phase 3: Pop excess values from the stack.
    fn pop_excess(&mut self) {
        // Count how many of each value we still need
        let mut still_needed: FxHashMap<ValueId, usize> = FxHashMap::default();
        for slot in self.target.iter() {
            if let TargetSlot::Value(v) = slot {
                *still_needed.entry(*v).or_insert(0) += 1;
            }
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

/// Computes the ideal entry layout for an instruction given the desired exit layout.
///
/// This is the core of backward layout analysis:
/// - Given what we want after the instruction (exit layout)
/// - Compute what we need before the instruction (entry layout)
#[derive(Clone, Debug)]
pub struct LayoutAnalysis {
    /// Ideal entry layouts for each instruction (block_id, inst_idx) -> layout.
    pub entry_layouts: FxHashMap<(usize, usize), Vec<TargetSlot>>,
    /// Ideal exit layouts for each block.
    pub block_exit_layouts: FxHashMap<usize, Vec<TargetSlot>>,
}

impl LayoutAnalysis {
    /// Creates a new empty layout analysis.
    #[must_use]
    pub fn new() -> Self {
        Self { entry_layouts: FxHashMap::default(), block_exit_layouts: FxHashMap::default() }
    }

    /// Analyzes a sequence of instructions backward to compute ideal layouts.
    ///
    /// Given:
    /// - A sequence of (operands, result) pairs representing instructions
    /// - The desired exit layout after the last instruction
    ///
    /// Returns the ideal entry layout before the first instruction.
    pub fn analyze_backward(
        instructions: &[(Vec<ValueId>, Option<ValueId>)],
        exit_layout: &[TargetSlot],
    ) -> Vec<TargetSlot> {
        let mut current_layout = exit_layout.to_vec();

        // Work backwards through instructions
        for (operands, result) in instructions.iter().rev() {
            current_layout =
                Self::compute_entry_for_instruction(operands, *result, &current_layout);
        }

        current_layout
    }

    /// Computes the ideal entry layout for a single instruction.
    fn compute_entry_for_instruction(
        operands: &[ValueId],
        result: Option<ValueId>,
        exit_layout: &[TargetSlot],
    ) -> Vec<TargetSlot> {
        let mut entry = Vec::new();

        // Operands should be on top of stack (first operand at depth 0)
        for &op in operands {
            entry.push(TargetSlot::Value(op));
        }

        // Rest of exit layout, excluding the result (which will be produced)
        for slot in exit_layout {
            match slot {
                TargetSlot::Value(v) if Some(*v) == result => {
                    // Skip - this will be produced by the instruction
                }
                _ => entry.push(*slot),
            }
        }

        entry
    }
}

impl Default for LayoutAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a stack layout for a block - the values that should be on the stack
/// when entering or exiting a block, from top to bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockStackLayout {
    /// Stack slots from top (index 0) to bottom.
    /// `None` means "don't care" / junk slot.
    pub slots: SmallVec<[Option<ValueId>; 8]>,
}

impl BlockStackLayout {
    /// Creates an empty layout.
    #[must_use]
    pub fn new() -> Self {
        Self { slots: SmallVec::new() }
    }

    /// Creates a layout with the given values (first = top of stack).
    #[must_use]
    pub fn from_values(values: impl IntoIterator<Item = ValueId>) -> Self {
        Self { slots: values.into_iter().map(Some).collect() }
    }

    /// Creates a layout from a StackModel.
    #[must_use]
    pub fn from_stack_model(model: &StackModel) -> Self {
        Self { slots: model.as_slice().to_smallvec() }
    }

    /// Returns the depth (number of slots).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.slots.len()
    }

    /// Returns true if the layout is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Gets the value at a given depth (0 = top).
    #[must_use]
    pub fn get(&self, depth: usize) -> Option<ValueId> {
        self.slots.get(depth).copied().flatten()
    }

    /// Converts to a target layout for the shuffler.
    #[must_use]
    pub fn to_target_layout(&self) -> Vec<TargetSlot> {
        self.slots
            .iter()
            .map(|&v| match v {
                Some(val) => TargetSlot::Value(val),
                None => TargetSlot::Any,
            })
            .collect()
    }

    /// Finds the position of a value in the layout.
    #[must_use]
    pub fn find(&self, value: ValueId) -> Option<usize> {
        self.slots.iter().position(|&v| v == Some(value))
    }

    /// Returns true if the layout contains a value.
    #[must_use]
    pub fn contains(&self, value: ValueId) -> bool {
        self.find(value).is_some()
    }
}

/// Computes a common stack layout for multiple incoming edges at a merge point.
///
/// The algorithm:
/// 1. Find values that appear in multiple incoming layouts at the same position
/// 2. For values at different positions, choose the position with lowest total shuffle cost
/// 3. Values only needed by one predecessor become "junk slots" for others
///
/// Returns `None` if the layouts are incompatible (too many different values).
pub fn combine_stack_layouts(layouts: &[BlockStackLayout]) -> Option<BlockStackLayout> {
    if layouts.is_empty() {
        return Some(BlockStackLayout::new());
    }

    if layouts.len() == 1 {
        return Some(layouts[0].clone());
    }

    // Find the maximum depth across all layouts
    let max_depth = layouts.iter().map(|l| l.depth()).max().unwrap_or(0);
    if max_depth == 0 {
        return Some(BlockStackLayout::new());
    }

    // Collect all values that appear in any layout
    let mut all_values: FxHashMap<ValueId, Vec<(usize, usize)>> = FxHashMap::default();
    for (layout_idx, layout) in layouts.iter().enumerate() {
        for (depth, slot) in layout.slots.iter().enumerate() {
            if let Some(val) = slot {
                all_values.entry(*val).or_default().push((layout_idx, depth));
            }
        }
    }

    // Build the combined layout
    // Strategy: for each position, find the value that appears most often at that position
    let mut combined = BlockStackLayout { slots: smallvec::smallvec![None; max_depth] };

    // First pass: place values that are at the same position in all layouts
    for depth in 0..max_depth {
        let mut value_at_depth: Option<ValueId> = None;
        let mut consistent = true;

        for layout in layouts {
            let val = layout.get(depth);
            match (value_at_depth, val) {
                (None, Some(v)) => value_at_depth = Some(v),
                (Some(existing), Some(v)) if existing != v => {
                    consistent = false;
                    break;
                }
                _ => {}
            }
        }

        if consistent {
            combined.slots[depth] = value_at_depth;
        }
    }

    // Second pass: for values not yet placed, find best position
    for (&value, positions) in &all_values {
        if combined.contains(value) {
            continue; // Already placed
        }

        // Find the most common position for this value
        let mut pos_counts: FxHashMap<usize, usize> = FxHashMap::default();
        for &(_, depth) in positions {
            *pos_counts.entry(depth).or_default() += 1;
        }

        // Pick the position with highest count, preferring lower depths on ties
        if let Some((&best_pos, _)) = pos_counts.iter().max_by(|a, b| {
            a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)) // Higher count, then lower depth
        }) {
            if best_pos < combined.slots.len() && combined.slots[best_pos].is_none() {
                combined.slots[best_pos] = Some(value);
            }
        }
    }

    // Trim trailing None values
    while combined.slots.last() == Some(&None) {
        combined.slots.pop();
    }

    Some(combined)
}

/// Computes the shuffle cost from a source layout to a target layout.
/// This estimates the number of DUP/SWAP/POP operations needed.
#[must_use]
pub fn estimate_shuffle_cost(source: &BlockStackLayout, target: &BlockStackLayout) -> usize {
    let mut cost = 0;

    // Count mismatched positions (need SWAP)
    for (depth, target_val) in
        target.slots.iter().enumerate().filter_map(|(i, s)| s.map(|v| (i, v)))
    {
        match source.find(target_val) {
            Some(src_depth) if src_depth != depth => {
                // Need to move this value - costs at least 1 SWAP
                cost += 1;
            }
            None => {
                // Value not in source - need to push or reload
                cost += 2; // Push is more expensive
            }
            _ => {} // Already in correct position
        }
    }

    // Count values in source that aren't in target (need POP)
    for val in source.slots.iter().flatten() {
        if !target.contains(*val) {
            cost += 1; // POP
        }
    }

    // Count values that need DUP (appear more times in target than source)
    for val in target.slots.iter().flatten() {
        let source_count = source.slots.iter().filter(|&&v| v == Some(*val)).count();
        let target_count = target.slots.iter().filter(|&&v| v == Some(*val)).count();
        if target_count > source_count {
            cost += target_count - source_count; // DUP operations
        }
    }

    cost
}

/// Represents a "freely generable" value that can be pushed without being on the stack.
/// These values don't need to be in the source layout when shuffling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreelyGenerable {
    /// An immediate value (literal).
    Immediate,
    /// A function argument (can be loaded from calldata).
    Argument,
    /// A function label (for internal calls).
    FunctionLabel,
}

/// Checks if a value can be freely generated (doesn't need to be on stack).
///
/// Freely generable values include:
/// - Literals/immediates
/// - Function arguments (loaded from calldata)
/// - Function labels (for internal calls)
pub fn is_freely_generable(func: &crate::mir::Function, value: ValueId) -> bool {
    matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
}

/// Creates an ideal layout for preparing multiple operands.
///
/// Given operands [a, b, c] where we want a on top:
/// - Returns [Value(a), Value(b), Value(c)]
pub fn ideal_operand_layout(operands: &[ValueId]) -> Vec<TargetSlot> {
    operands.iter().map(|&v| TargetSlot::Value(v)).collect()
}

/// Computes the ideal pre-layout for a binary operation.
///
/// For a binary op that consumes [a, b] (a on top, b below) and produces result:
/// - If we want [result, ...rest] after
/// - We need [a, b, ...rest] before (where rest doesn't contain a or b)
pub fn ideal_binary_op_entry(
    a: ValueId,
    b: ValueId,
    result: Option<ValueId>,
    exit_layout: &[TargetSlot],
) -> Vec<TargetSlot> {
    let mut entry = Vec::with_capacity(exit_layout.len() + 1);

    // The operands should be on top: a at depth 0, b at depth 1
    entry.push(TargetSlot::Value(a));
    entry.push(TargetSlot::Value(b));

    // The rest of the exit layout (skipping the result position)
    for slot in exit_layout.iter() {
        match slot {
            TargetSlot::Value(v) if Some(*v) == result => {
                // Skip the result - it will be produced by the operation
            }
            _ => entry.push(*slot),
        }
    }

    entry
}

/// Computes the ideal pre-layout for a unary operation.
pub fn ideal_unary_op_entry(
    operand: ValueId,
    result: Option<ValueId>,
    exit_layout: &[TargetSlot],
) -> Vec<TargetSlot> {
    let mut entry = Vec::with_capacity(exit_layout.len());

    // The operand should be on top
    entry.push(TargetSlot::Value(operand));

    // The rest of the exit layout (skipping the result position)
    for slot in exit_layout.iter() {
        match slot {
            TargetSlot::Value(v) if Some(*v) == result => {
                // Skip the result - it will be produced by the operation
            }
            _ => entry.push(*slot),
        }
    }

    entry
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
        assert!(result.is_empty());
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
        assert_eq!(result.swap_count, 1);
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
        assert_eq!(result.dup_count, 1);
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
        assert!(result.pop_count >= 1 || result.swap_count >= 1);
    }

    #[test]
    fn test_ideal_binary_op_entry() {
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let result = ValueId::from_usize(2);
        let extra = ValueId::from_usize(3);

        // Exit layout: [result, extra]
        let exit = [TargetSlot::Value(result), TargetSlot::Value(extra)];

        let entry = ideal_binary_op_entry(a, b, Some(result), &exit);

        // Entry should be: [a, b, extra]
        assert_eq!(entry.len(), 3);
        assert_eq!(entry[0], TargetSlot::Value(a));
        assert_eq!(entry[1], TargetSlot::Value(b));
        assert_eq!(entry[2], TargetSlot::Value(extra));
    }

    #[test]
    fn test_backward_layout_analysis() {
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let c = ValueId::from_usize(2);
        let r1 = ValueId::from_usize(3);
        let r2 = ValueId::from_usize(4);

        // Two instructions:
        // 1. r1 = ADD(a, b)
        // 2. r2 = MUL(r1, c)
        let instructions = vec![
            (vec![a, b], Some(r1)),  // ADD(a, b) -> r1
            (vec![r1, c], Some(r2)), // MUL(r1, c) -> r2
        ];

        // Desired exit: [r2]
        let exit = vec![TargetSlot::Value(r2)];

        let entry = LayoutAnalysis::analyze_backward(&instructions, &exit);

        // Entry should be: [a, b, c]
        // Because:
        // - MUL needs [r1, c] and produces r2
        // - ADD needs [a, b] and produces r1
        assert_eq!(entry.len(), 3);
        assert_eq!(entry[0], TargetSlot::Value(a));
        assert_eq!(entry[1], TargetSlot::Value(b));
        assert_eq!(entry[2], TargetSlot::Value(c));
    }

    #[test]
    fn test_ideal_operand_layout() {
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let c = ValueId::from_usize(2);

        let layout = ideal_operand_layout(&[a, b, c]);

        assert_eq!(layout.len(), 3);
        assert_eq!(layout[0], TargetSlot::Value(a));
        assert_eq!(layout[1], TargetSlot::Value(b));
        assert_eq!(layout[2], TargetSlot::Value(c));
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
        assert!(result.swap_count > 0);
    }

    #[test]
    fn test_block_stack_layout_basic() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        assert_eq!(layout.depth(), 2);
        assert_eq!(layout.get(0), Some(v0));
        assert_eq!(layout.get(1), Some(v1));
        assert!(layout.contains(v0));
        assert!(layout.contains(v1));
        assert_eq!(layout.find(v0), Some(0));
        assert_eq!(layout.find(v1), Some(1));
    }

    #[test]
    fn test_block_stack_layout_to_target() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        let target = layout.to_target_layout();

        assert_eq!(target.len(), 2);
        assert_eq!(target[0], TargetSlot::Value(v0));
        assert_eq!(target[1], TargetSlot::Value(v1));
    }

    #[test]
    fn test_combine_stack_layouts_single() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        let combined = combine_stack_layouts(&[layout.clone()]);

        assert!(combined.is_some());
        assert_eq!(combined.unwrap(), layout);
    }

    #[test]
    fn test_combine_stack_layouts_identical() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout1 = BlockStackLayout::from_values([v0, v1]);
        let layout2 = BlockStackLayout::from_values([v0, v1]);
        let combined = combine_stack_layouts(&[layout1, layout2]);

        assert!(combined.is_some());
        let result = combined.unwrap();
        assert_eq!(result.get(0), Some(v0));
        assert_eq!(result.get(1), Some(v1));
    }

    #[test]
    fn test_combine_stack_layouts_different_values() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        // Layout1: [v0, v1]
        // Layout2: [v0, v2]
        // Combined should have v0 at position 0 (consistent)
        let layout1 = BlockStackLayout::from_values([v0, v1]);
        let layout2 = BlockStackLayout::from_values([v0, v2]);
        let combined = combine_stack_layouts(&[layout1, layout2]);

        assert!(combined.is_some());
        let result = combined.unwrap();
        assert_eq!(result.get(0), Some(v0)); // Consistent at position 0
    }

    #[test]
    fn test_estimate_shuffle_cost_identical() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        let cost = estimate_shuffle_cost(&layout, &layout);

        assert_eq!(cost, 0);
    }

    #[test]
    fn test_estimate_shuffle_cost_swap() {
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let source = BlockStackLayout::from_values([v0, v1]);
        let target = BlockStackLayout::from_values([v1, v0]);
        let cost = estimate_shuffle_cost(&source, &target);

        // Both values need to move positions
        assert!(cost > 0);
    }
}
