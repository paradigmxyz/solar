//! Stack scheduling optimization to minimize DUP/SWAP operations.
//!
//! This module provides:
//! - Metrics collection for DUP/SWAP operations
//! - Use-frequency analysis for optimal stack ordering
//! - Stack distance optimization to prefer shallow DUPs

use super::model::MAX_STACK_ACCESS;
use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, InstKind, ValueId},
};
use rustc_hash::FxHashMap;
use std::cmp::Reverse;

/// Metrics for DUP/SWAP operations in generated code.
#[derive(Clone, Debug, Default)]
pub struct StackMetrics {
    /// Total number of DUP operations generated.
    pub dup_count: usize,
    /// Total number of SWAP operations generated.
    pub swap_count: usize,
    /// Histogram of DUP depths (DUP1, DUP2, ..., DUP16).
    /// Index 0 = DUP1, index 15 = DUP16.
    pub dup_depth_histogram: [usize; 16],
    /// Histogram of SWAP depths (SWAP1, SWAP2, ..., SWAP16).
    /// Index 0 = SWAP1, index 15 = SWAP16.
    pub swap_depth_histogram: [usize; 16],
    /// Number of spill operations (when stack depth exceeds 16).
    pub spill_count: usize,
    /// Number of reload operations (loading spilled values).
    pub reload_count: usize,
}

impl StackMetrics {
    /// Creates a new empty metrics instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a DUP operation at the given depth (1-16).
    pub fn record_dup(&mut self, depth: u8) {
        debug_assert!(depth >= 1 && depth <= 16, "DUP depth out of range: {}", depth);
        self.dup_count += 1;
        if depth >= 1 && depth <= 16 {
            self.dup_depth_histogram[(depth - 1) as usize] += 1;
        }
    }

    /// Records a SWAP operation at the given depth (1-16).
    pub fn record_swap(&mut self, depth: u8) {
        debug_assert!(depth >= 1 && depth <= 16, "SWAP depth out of range: {}", depth);
        self.swap_count += 1;
        if depth >= 1 && depth <= 16 {
            self.swap_depth_histogram[(depth - 1) as usize] += 1;
        }
    }

    /// Records a spill operation.
    pub fn record_spill(&mut self) {
        self.spill_count += 1;
    }

    /// Records a reload operation.
    pub fn record_reload(&mut self) {
        self.reload_count += 1;
    }

    /// Calculates estimated gas cost for stack operations.
    /// DUP/SWAP each cost 3 gas.
    #[must_use]
    pub fn estimated_gas(&self) -> u64 {
        let dup_swap_gas = (self.dup_count + self.swap_count) as u64 * 3;
        // Spills require PUSH + MSTORE = ~6 gas, reloads require PUSH + MLOAD = ~6 gas
        let spill_gas = (self.spill_count + self.reload_count) as u64 * 6;
        dup_swap_gas + spill_gas
    }

    /// Returns the average DUP depth (lower is better).
    #[must_use]
    pub fn average_dup_depth(&self) -> f64 {
        if self.dup_count == 0 {
            return 0.0;
        }
        let total_depth: usize =
            self.dup_depth_histogram.iter().enumerate().map(|(i, &count)| (i + 1) * count).sum();
        total_depth as f64 / self.dup_count as f64
    }

    /// Returns the count of deep DUPs (DUP9 and deeper).
    /// Deep DUPs indicate poor scheduling.
    #[must_use]
    pub fn deep_dup_count(&self) -> usize {
        self.dup_depth_histogram[8..].iter().sum()
    }

    /// Merges another metrics instance into this one.
    pub fn merge(&mut self, other: &Self) {
        self.dup_count += other.dup_count;
        self.swap_count += other.swap_count;
        for i in 0..16 {
            self.dup_depth_histogram[i] += other.dup_depth_histogram[i];
            self.swap_depth_histogram[i] += other.swap_depth_histogram[i];
        }
        self.spill_count += other.spill_count;
        self.reload_count += other.reload_count;
    }
}

impl std::fmt::Display for StackMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Stack Operation Metrics:")?;
        writeln!(f, "  DUP count: {}", self.dup_count)?;
        writeln!(f, "  SWAP count: {}", self.swap_count)?;
        writeln!(f, "  Average DUP depth: {:.2}", self.average_dup_depth())?;
        writeln!(f, "  Deep DUPs (9+): {}", self.deep_dup_count())?;
        writeln!(f, "  Spill count: {}", self.spill_count)?;
        writeln!(f, "  Reload count: {}", self.reload_count)?;
        writeln!(f, "  Estimated gas: {}", self.estimated_gas())?;
        Ok(())
    }
}

/// Global metrics collector for tracking stack operations across codegen.
#[derive(Clone, Debug, Default)]
pub struct MetricsCollector {
    /// Accumulated metrics.
    pub metrics: StackMetrics,
    /// Shuffler operations saved (compared to naive approach).
    pub shuffler_ops_saved: usize,
    /// Number of shuffle operations performed.
    pub shuffle_count: usize,
}

impl MetricsCollector {
    /// Creates a new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a DUP operation.
    pub fn record_dup(&mut self, depth: u8) {
        self.metrics.record_dup(depth);
    }

    /// Records a SWAP operation.
    pub fn record_swap(&mut self, depth: u8) {
        self.metrics.record_swap(depth);
    }

    /// Records a shuffle operation and how many ops it saved.
    pub fn record_shuffle(&mut self, ops_saved: usize) {
        self.shuffle_count += 1;
        self.shuffler_ops_saved += ops_saved;
    }

    /// Returns a summary string.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "DUP: {}, SWAP: {}, Shuffles: {}, Ops saved: {}, Gas: {}",
            self.metrics.dup_count,
            self.metrics.swap_count,
            self.shuffle_count,
            self.shuffler_ops_saved,
            self.metrics.estimated_gas()
        )
    }
}

/// Use-frequency analysis for values in a block.
///
/// Tracks how many times each value is used within a block to help
/// with stack scheduling decisions.
#[derive(Clone, Debug)]
pub struct UseFrequency {
    /// Map from value to use count within the current scope.
    uses: FxHashMap<ValueId, u32>,
    /// Map from value to next use distance (instructions until next use).
    next_use: FxHashMap<ValueId, u32>,
}

impl UseFrequency {
    /// Creates a new use frequency tracker.
    #[must_use]
    pub fn new() -> Self {
        Self { uses: FxHashMap::default(), next_use: FxHashMap::default() }
    }

    /// Analyzes use frequency for a block.
    pub fn analyze_block(&mut self, func: &Function, block_id: BlockId) {
        self.uses.clear();
        self.next_use.clear();

        let block = &func.blocks[block_id];

        // Count uses for each value
        for &inst_id in &block.instructions {
            let inst = func.instruction(inst_id);
            let operands = inst.kind.operands();
            for op in operands {
                *self.uses.entry(op).or_insert(0) += 1;
            }
        }

        // Compute next-use distances by scanning backward
        let mut current_distance: FxHashMap<ValueId, u32> = FxHashMap::default();
        for (idx, &inst_id) in block.instructions.iter().enumerate().rev() {
            let inst = func.instruction(inst_id);
            let operands = inst.kind.operands();

            for op in operands {
                // If we haven't seen this value yet (scanning backward), record distance
                if !self.next_use.contains_key(&op) {
                    current_distance.insert(op, idx as u32);
                }
            }

            // Update next_use for values at this position
            for (val, &dist) in &current_distance {
                self.next_use.insert(*val, (block.instructions.len() as u32).saturating_sub(dist));
            }
        }
    }

    /// Returns the use count for a value.
    #[must_use]
    pub fn use_count(&self, val: ValueId) -> u32 {
        self.uses.get(&val).copied().unwrap_or(0)
    }

    /// Returns the next use distance for a value (0 = immediately used).
    #[must_use]
    pub fn next_use_distance(&self, val: ValueId) -> u32 {
        self.next_use.get(&val).copied().unwrap_or(u32::MAX)
    }

    /// Returns values sorted by use frequency (most frequently used first).
    #[must_use]
    pub fn values_by_frequency(&self) -> Vec<ValueId> {
        let mut values: Vec<_> = self.uses.iter().map(|(&v, &c)| (v, c)).collect();
        values.sort_by_key(|&(_, count)| Reverse(count));
        values.into_iter().map(|(v, _)| v).collect()
    }
}

impl Default for UseFrequency {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents the order in which binary operands should be emitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperandOrder {
    /// Emit operand A first (goes deeper on stack), then B.
    /// This is the default for instructions like ADD where A is first operand.
    AFirst,
    /// Emit operand B first (goes deeper on stack), then A.
    /// Preferred when B is used more frequently and should stay accessible.
    BFirst,
}

/// Determines the optimal operand order for a binary operation.
///
/// The goal is to minimize DUP depth for values that will be used again.
/// If one operand is used more frequently, it should be on top of the stack
/// after the operation (if the operation preserves it).
pub fn optimal_operand_order(
    a: ValueId,
    b: ValueId,
    current_stack: &[Option<ValueId>],
    use_frequency: &UseFrequency,
    liveness: &Liveness,
    block: BlockId,
    inst_idx: usize,
) -> OperandOrder {
    // If both operands are the same, order doesn't matter
    if a == b {
        return OperandOrder::AFirst;
    }

    // Check if either operand is already on stack
    let a_depth = current_stack.iter().position(|&v| v == Some(a));
    let b_depth = current_stack.iter().position(|&v| v == Some(b));

    // Prefer ordering that minimizes total DUP depth
    match (a_depth, b_depth) {
        (Some(ad), Some(bd)) => {
            // Both on stack - prefer order that keeps frequently-used value accessible
            let a_freq = use_frequency.use_count(a);
            let b_freq = use_frequency.use_count(b);

            // If one is much more frequently used, prefer that one on top
            if a_freq > b_freq + 2 {
                OperandOrder::BFirst // A ends up on top after B is pushed first
            } else if b_freq > a_freq + 2 {
                OperandOrder::AFirst // B ends up on top after A is pushed first
            } else {
                // Similar frequency - prefer the one at shallower depth to minimize DUP
                if ad < bd { OperandOrder::BFirst } else { OperandOrder::AFirst }
            }
        }
        (Some(_), None) => {
            // A is on stack, B is not - emit B first, then DUP A
            // This is efficient because A doesn't need to be pushed
            OperandOrder::BFirst
        }
        (None, Some(_)) => {
            // B is on stack, A is not - emit A first, then DUP B
            OperandOrder::AFirst
        }
        (None, None) => {
            // Neither on stack - check liveness for the best order
            let a_is_live = !liveness.is_dead_after(a, block, inst_idx);
            let b_is_live = !liveness.is_dead_after(b, block, inst_idx);

            match (a_is_live, b_is_live) {
                (true, false) => {
                    // Only A is live after - emit A first so it stays deeper on stack
                    // Then emit B on top for the operation
                    // After operation, we need A for later use
                    OperandOrder::AFirst
                }
                (false, true) => {
                    // Only B is live after - emit B first
                    OperandOrder::BFirst
                }
                _ => {
                    // Both live or both dead - use frequency as tiebreaker
                    let a_freq = use_frequency.use_count(a);
                    let b_freq = use_frequency.use_count(b);
                    if a_freq > b_freq { OperandOrder::AFirst } else { OperandOrder::BFirst }
                }
            }
        }
    }
}

/// Checks if an instruction is commutative (operand order doesn't affect result).
/// For commutative operations, we can swap operands to minimize stack operations.
#[must_use]
pub const fn is_commutative(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::Add(_, _)
            | InstKind::Mul(_, _)
            | InstKind::And(_, _)
            | InstKind::Or(_, _)
            | InstKind::Xor(_, _)
            | InstKind::Eq(_, _)
    )
}

/// Analysis result for a block's instruction ordering.
#[derive(Clone, Debug)]
pub struct SchedulingHint {
    /// Preferred operand orders for binary operations.
    /// Key is instruction index within block.
    pub operand_orders: FxHashMap<usize, OperandOrder>,
    /// Values that should be prioritized for stack placement.
    pub hot_values: Vec<ValueId>,
}

impl SchedulingHint {
    /// Creates empty scheduling hints.
    #[must_use]
    pub fn new() -> Self {
        Self { operand_orders: FxHashMap::default(), hot_values: Vec::new() }
    }

    /// Analyzes a block and generates scheduling hints.
    pub fn analyze(func: &Function, block_id: BlockId, liveness: &Liveness) -> Self {
        let mut hints = Self::new();
        let mut use_freq = UseFrequency::new();
        use_freq.analyze_block(func, block_id);

        let block = &func.blocks[block_id];

        // Collect hot values (used 3+ times)
        hints.hot_values = use_freq
            .values_by_frequency()
            .into_iter()
            .filter(|v| use_freq.use_count(*v) >= 3)
            .take(MAX_STACK_ACCESS) // Only track up to 16 hot values
            .collect();

        // Analyze binary operations for optimal operand order
        for (idx, &inst_id) in block.instructions.iter().enumerate() {
            let inst = func.instruction(inst_id);

            // Only optimize commutative operations
            if !is_commutative(&inst.kind) {
                continue;
            }

            let operands = inst.kind.operands();
            if operands.len() == 2 {
                let a = operands[0];
                let b = operands[1];

                // Simulate empty stack for now - real optimization would track actual stack
                let order = optimal_operand_order(a, b, &[], &use_freq, liveness, block_id, idx);

                if order != OperandOrder::AFirst {
                    hints.operand_orders.insert(idx, order);
                }
            }
        }

        hints
    }

    /// Gets the operand order for an instruction, defaulting to AFirst.
    #[must_use]
    pub fn get_operand_order(&self, inst_idx: usize) -> OperandOrder {
        self.operand_orders.get(&inst_idx).copied().unwrap_or(OperandOrder::AFirst)
    }
}

impl Default for SchedulingHint {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_metrics() {
        let mut metrics = StackMetrics::new();

        metrics.record_dup(1);
        metrics.record_dup(1);
        metrics.record_dup(3);
        metrics.record_swap(1);

        assert_eq!(metrics.dup_count, 3);
        assert_eq!(metrics.swap_count, 1);
        assert_eq!(metrics.dup_depth_histogram[0], 2); // DUP1
        assert_eq!(metrics.dup_depth_histogram[2], 1); // DUP3
        assert!((metrics.average_dup_depth() - 1.67).abs() < 0.1);
    }

    #[test]
    fn test_deep_dup_detection() {
        let mut metrics = StackMetrics::new();

        // Shallow DUPs
        for _ in 0..5 {
            metrics.record_dup(1);
        }

        // Deep DUPs (DUP9+)
        metrics.record_dup(9);
        metrics.record_dup(12);
        metrics.record_dup(16);

        assert_eq!(metrics.deep_dup_count(), 3);
    }

    #[test]
    fn test_is_commutative() {
        assert!(is_commutative(&InstKind::Add(ValueId::from_usize(0), ValueId::from_usize(1))));
        assert!(is_commutative(&InstKind::Mul(ValueId::from_usize(0), ValueId::from_usize(1))));
        assert!(!is_commutative(&InstKind::Sub(ValueId::from_usize(0), ValueId::from_usize(1))));
        assert!(!is_commutative(&InstKind::Div(ValueId::from_usize(0), ValueId::from_usize(1))));
    }

    #[test]
    fn test_estimated_gas() {
        let mut metrics = StackMetrics::new();

        metrics.record_dup(1);
        metrics.record_dup(2);
        metrics.record_swap(1);

        // 3 stack ops * 3 gas = 9 gas
        assert_eq!(metrics.estimated_gas(), 9);

        metrics.record_spill();
        metrics.record_reload();

        // 9 + 2 * 6 = 21 gas
        assert_eq!(metrics.estimated_gas(), 21);
    }
}
