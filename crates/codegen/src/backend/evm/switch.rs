//! Target-aware switch lowering selection.

use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

// Ordinary label pushes relax after layout. Use their minimum possible size so
// fixed-width tables are selected for size only when they beat the best case.
const MIN_LABEL_PUSH_LEN: usize = 2;
const JUMPDEST_LEN: usize = 1;
const MIN_DEFAULT_JUMP_LEN: usize = MIN_LABEL_PUSH_LEN + 1;

const VERY_LOW_GAS: usize = 3;
const JUMP_GAS: usize = 8;
const JUMPI_GAS: usize = 10;
const DEFAULT_JUMP_GAS: usize = VERY_LOW_GAS + JUMP_GAS;
const POP_GAS: usize = 2;
const MOD_GAS: usize = 5;
const MUL_GAS: usize = 5;
const JUMPDEST_GAS: usize = 1;

const INDEXED_JUMP_BASE_LEN: usize = 7;
const MIN_BUCKET_CASES: usize = 8;
// Bound table footprint and the number of bucket blocks processed by EVM IR passes.
const MAX_BUCKET_CASES: usize = 64;
const MAX_DENSE_RANGE: usize = 4096;
const MAX_BUCKET_CANDIDATES: usize = 33;
// Bound per-switch bytecode growth under the runtime-gas objective.
const MAX_GAS_CODE_GROWTH: usize = 512;

/// Selected control-flow shape for a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SwitchPlan {
    /// Test every case in source order.
    Linear,
    /// Recursively split sorted cases, using linear leaves of at most this size.
    Binary { leaf_size: usize },
    /// Dispatch by `value % bucket_count`, then linearly scan one bucket.
    Buckets { bucket_count: usize },
    /// Bounds-check `value - low` and dispatch through a dense target table.
    Dense { low: U256, range: usize },
}

/// Selects the cheapest supported switch shape for the optimization objective.
pub(super) fn select_switch_plan(
    values: &[U256],
    optimization: OptimizationMode,
    evm_version: EvmVersion,
    needs_value_cleanup: bool,
    table_target_width: usize,
) -> SwitchPlan {
    debug_assert!(values.windows(2).all(|values| values[0] < values[1]));
    if values.len() <= 1 || optimization == OptimizationMode::None {
        return SwitchPlan::Linear;
    }

    let linear_cost = lowering_cost(values, values.len(), evm_version, table_target_width);
    let max_gas_code_size = linear_cost.code_size.saturating_add(MAX_GAS_CODE_GROWTH);
    let mut best = (linear_cost, SwitchPlan::Linear);
    if optimization == OptimizationMode::Gas {
        for leaf_size in binary_leaf_sizes(values.len()) {
            let cost = lowering_cost(values, leaf_size, evm_version, table_target_width);
            if cost.is_better_for_gas_than(best.0, max_gas_code_size) {
                best = (cost, SwitchPlan::Binary { leaf_size });
            }
        }
        if (MIN_BUCKET_CASES..=MAX_BUCKET_CASES).contains(&values.len()) {
            for bucket_count in bucket_count_candidates(values.len()) {
                let cost = bucket_lowering_cost(
                    values,
                    bucket_count,
                    evm_version,
                    needs_value_cleanup,
                    table_target_width,
                );
                if cost.is_better_for_gas_than(best.0, max_gas_code_size) {
                    best = (cost, SwitchPlan::Buckets { bucket_count });
                }
            }
        }
    }
    if let Some((low, range, cost)) = dense_lowering_cost(values, evm_version, table_target_width) {
        let better = match optimization {
            OptimizationMode::Gas => cost.is_better_for_gas_than(best.0, max_gas_code_size),
            OptimizationMode::Size => cost.size_key() < best.0.size_key(),
            _ => false,
        };
        if better {
            best = (cost, SwitchPlan::Dense { low, range });
        }
    }
    best.1
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LoweringCost {
    code_size: usize,
    max_code_size: usize,
    hit_gas_sum: usize,
    miss_gas: usize,
}

impl LoweringCost {
    fn is_better_for_gas_than(self, other: Self, max_code_size: usize) -> bool {
        self.max_code_size <= max_code_size && self.gas_key() < other.gas_key()
    }

    fn gas_key(self) -> (usize, usize, usize) {
        (self.hit_gas_sum, self.miss_gas, self.code_size)
    }

    fn size_key(self) -> (usize, usize, usize) {
        (self.code_size, self.hit_gas_sum, self.miss_gas)
    }
}

fn lowering_cost(
    values: &[U256],
    leaf_size: usize,
    evm_version: EvmVersion,
    table_target_width: usize,
) -> LoweringCost {
    if values.len() <= leaf_size {
        let mut cost = LoweringCost {
            code_size: MIN_DEFAULT_JUMP_LEN,
            max_code_size: max_default_jump_len(table_target_width),
            ..Default::default()
        };
        let mut path_gas = 0;
        for &value in values {
            let test = equality_test_cost(value, evm_version, table_target_width);
            cost.code_size += test.code_size;
            cost.max_code_size += test.max_code_size;
            path_gas += test.gas;
            cost.hit_gas_sum += path_gas;
        }
        cost.miss_gas = path_gas + DEFAULT_JUMP_GAS;
        return cost;
    }

    let mid = values.len() / 2;
    let split = ordered_test_cost(values[mid], evm_version, table_target_width);
    let left = lowering_cost(&values[..mid], leaf_size, evm_version, table_target_width);
    let right = lowering_cost(&values[mid..], leaf_size, evm_version, table_target_width);
    LoweringCost {
        code_size: split.code_size + JUMPDEST_LEN + left.code_size + right.code_size,
        max_code_size: split.max_code_size
            + JUMPDEST_LEN
            + left.max_code_size
            + right.max_code_size,
        hit_gas_sum: split.gas * values.len()
            + left.hit_gas_sum
            + mid * JUMPDEST_GAS
            + right.hit_gas_sum,
        miss_gas: split.gas + (left.miss_gas + JUMPDEST_GAS).max(right.miss_gas),
    }
}

fn binary_leaf_sizes(len: usize) -> Vec<usize> {
    let mut pending = vec![len];
    let mut sizes = Vec::new();
    while let Some(len) = pending.pop() {
        if len <= 1 {
            continue;
        }
        let mid = len / 2;
        for child in [mid, len - mid] {
            if child > 0 && child < len && !sizes.contains(&child) {
                sizes.push(child);
                pending.push(child);
            }
        }
    }
    sizes.sort_unstable();
    sizes
}

fn bucket_count_candidates(len: usize) -> Vec<usize> {
    let first = (len.saturating_mul(3) / 4).max(2);
    let last = len.saturating_mul(5) / 4;
    let count = last - first + 1;
    if count <= MAX_BUCKET_CANDIDATES {
        return (first..=last).collect();
    }

    let span = last - first;
    let denominator = MAX_BUCKET_CANDIDATES - 1;
    let mut candidates = (0..MAX_BUCKET_CANDIDATES)
        .map(|index| first + span.saturating_mul(index) / denominator)
        .collect::<Vec<_>>();
    candidates.push(len);
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn bucket_lowering_cost(
    values: &[U256],
    bucket_count: usize,
    evm_version: EvmVersion,
    needs_value_cleanup: bool,
    table_target_width: usize,
) -> LoweringCost {
    let hash_len = 1 + push_len(U256::from(bucket_count), evm_version) + 1 + 1;
    let hash_gas = VERY_LOW_GAS * 3 + MOD_GAS;
    let indexed_jump_gas = VERY_LOW_GAS
        + MUL_GAS
        + VERY_LOW_GAS
        + VERY_LOW_GAS
        + JUMP_GAS
        + JUMPDEST_GAS
        + VERY_LOW_GAS
        + JUMP_GAS
        + JUMPDEST_GAS;
    let dispatch_gas = hash_gas + indexed_jump_gas;
    let mut cost = LoweringCost {
        code_size: hash_len
            + INDEXED_JUMP_BASE_LEN
            + bucket_count * indexed_jump_stub_len(table_target_width),
        max_code_size: hash_len
            + max_indexed_jump_base_len(table_target_width)
            + bucket_count * indexed_jump_stub_len(table_target_width),
        hit_gas_sum: dispatch_gas * values.len(),
        miss_gas: dispatch_gas,
    };

    let mut bucket_path_gas = vec![0; bucket_count];
    for &value in values {
        let index = bucket_index(value, bucket_count);
        let test = equality_test_cost(value, evm_version, table_target_width);
        if bucket_path_gas[index] == 0 {
            let cleanup_len = usize::from(needs_value_cleanup);
            cost.code_size += JUMPDEST_LEN + cleanup_len + MIN_DEFAULT_JUMP_LEN;
            cost.max_code_size +=
                JUMPDEST_LEN + cleanup_len + max_default_jump_len(table_target_width);
        }
        cost.code_size += test.code_size;
        cost.max_code_size += test.max_code_size;
        bucket_path_gas[index] += test.gas;
        cost.hit_gas_sum += bucket_path_gas[index];
        cost.miss_gas = cost.miss_gas.max(
            dispatch_gas
                + bucket_path_gas[index]
                + usize::from(needs_value_cleanup) * POP_GAS
                + DEFAULT_JUMP_GAS,
        );
    }
    if needs_value_cleanup && bucket_path_gas.contains(&0) {
        // One shared JUMPDEST, POP, and default jump for ordinary MIR switches.
        cost.code_size += JUMPDEST_LEN + 1 + MIN_DEFAULT_JUMP_LEN;
        cost.max_code_size += JUMPDEST_LEN + 1 + max_default_jump_len(table_target_width);
        cost.miss_gas = cost.miss_gas.max(dispatch_gas + POP_GAS + DEFAULT_JUMP_GAS);
    }
    cost
}

fn dense_lowering_cost(
    values: &[U256],
    evm_version: EvmVersion,
    table_target_width: usize,
) -> Option<(U256, usize, LoweringCost)> {
    let low = *values.first()?;
    let high = *values.last()?;
    let range = usize::try_from(high - low).ok()?.checked_add(1)?;
    if range > MAX_DENSE_RANGE {
        return None;
    }

    let normalize_len = usize::from(!low.is_zero()) * (push_len(low, evm_version) + 2);
    let normalize_gas = usize::from(!low.is_zero()) * VERY_LOW_GAS * 3;
    let bounds_prefix_len = 1 + push_len(U256::from(range), evm_version) + 1 + 1;
    let bounds_len = bounds_prefix_len + MIN_LABEL_PUSH_LEN + 1;
    let max_bounds_len = bounds_prefix_len + max_label_push_len(table_target_width) + 1;
    let bounds_gas = VERY_LOW_GAS * 5 + JUMPI_GAS;
    let indexed_jump_gas = VERY_LOW_GAS
        + MUL_GAS
        + VERY_LOW_GAS
        + VERY_LOW_GAS
        + JUMP_GAS
        + JUMPDEST_GAS
        + VERY_LOW_GAS
        + JUMP_GAS;
    let hit_gas = normalize_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas;
    let miss_gas = normalize_gas + bounds_gas + 2 + DEFAULT_JUMP_GAS;
    Some((
        low,
        range,
        LoweringCost {
            code_size: normalize_len
                + bounds_len
                + 1
                + MIN_DEFAULT_JUMP_LEN
                + JUMPDEST_LEN
                + INDEXED_JUMP_BASE_LEN
                + range * indexed_jump_stub_len(table_target_width),
            max_code_size: normalize_len
                + max_bounds_len
                + 1
                + max_default_jump_len(table_target_width)
                + JUMPDEST_LEN
                + max_indexed_jump_base_len(table_target_width)
                + range * indexed_jump_stub_len(table_target_width),
            hit_gas_sum: hit_gas * values.len(),
            miss_gas,
        },
    ))
}

const fn indexed_jump_stub_len(target_width: usize) -> usize {
    // JUMPDEST, PUSH<n> target, JUMP.
    target_width + 3
}

const fn max_indexed_jump_base_len(table_target_width: usize) -> usize {
    // PUSH1 stub length, MUL, PUSH<n> table, ADD, JUMP.
    5 + max_label_push_len(table_target_width)
}

const fn max_label_push_len(target_width: usize) -> usize {
    target_width + 1
}

const fn max_default_jump_len(target_width: usize) -> usize {
    max_label_push_len(target_width) + 1
}

pub(super) fn bucket_index(value: U256, bucket_count: usize) -> usize {
    let limbs = value.as_limbs();
    if limbs[1..].iter().all(|&limb| limb == 0) {
        return (limbs[0] % bucket_count as u64) as usize;
    }

    let modulus = bucket_count as u128;
    limbs.iter().rev().fold(0, |remainder, &limb| ((remainder << 64) | limb as u128) % modulus)
        as usize
}

#[derive(Clone, Copy)]
struct TestCost {
    code_size: usize,
    max_code_size: usize,
    gas: usize,
}

fn equality_test_cost(value: U256, evm_version: EvmVersion, table_target_width: usize) -> TestCost {
    if value.is_zero() {
        // DUP1, ISZERO, PUSH<label>, JUMPI.
        TestCost {
            code_size: 1 + 1 + MIN_LABEL_PUSH_LEN + 1,
            max_code_size: 1 + 1 + max_label_push_len(table_target_width) + 1,
            gas: VERY_LOW_GAS + VERY_LOW_GAS + VERY_LOW_GAS + JUMPI_GAS,
        }
    } else {
        ordered_test_cost(value, evm_version, table_target_width)
    }
}

fn ordered_test_cost(value: U256, evm_version: EvmVersion, table_target_width: usize) -> TestCost {
    // DUP1, PUSH<value>, EQ/GT, PUSH<label>, JUMPI.
    let prefix_len = 1 + push_len(value, evm_version) + 1;
    TestCost {
        code_size: prefix_len + MIN_LABEL_PUSH_LEN + 1,
        max_code_size: prefix_len + max_label_push_len(table_target_width) + 1,
        gas: VERY_LOW_GAS * 4 + JUMPI_GAS,
    }
}

fn push_len(value: U256, evm_version: EvmVersion) -> usize {
    if value.is_zero() && evm_version.has_push0() { 1 } else { 1 + value.byte_len().max(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(n: usize) -> Vec<U256> {
        (0..n).map(U256::from).collect()
    }

    #[test]
    fn leaves_small_switches_linear() {
        assert_eq!(
            select_switch_plan(&values(4), OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn selects_profitable_binary_leaf_size() {
        let values = (0..5).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Binary { leaf_size: 3 }
        );
    }

    #[test]
    fn accounts_for_taken_binary_split_labels() {
        let values = (0..7).map(|value| U256::from(1 + value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Binary { leaf_size: 4 }
        );
    }

    #[test]
    fn bounds_bucket_search_for_large_switches() {
        let candidates = bucket_count_candidates(10_000);
        assert!(candidates.len() <= MAX_BUCKET_CANDIDATES + 1);
        assert!(candidates.contains(&10_000));
        assert!(bucket_count_candidates(97).contains(&97));
    }

    #[test]
    fn computes_bucket_indices_without_wide_division() {
        for value in [U256::ZERO, U256::from(u64::MAX), U256::MAX] {
            for bucket_count in [1, 7, 32, 127, usize::MAX] {
                assert_eq!(
                    bucket_index(value, bucket_count),
                    usize::try_from(value % U256::from(bucket_count)).unwrap()
                );
            }
        }
    }

    #[test]
    fn charges_empty_bucket_cleanup_only_when_needed() {
        let values = [U256::ZERO, U256::from(2)];
        let with_cleanup = bucket_lowering_cost(&values, 4, EvmVersion::Cancun, true, 2);
        let without_cleanup = bucket_lowering_cost(&values, 4, EvmVersion::Cancun, false, 2);
        assert_eq!(
            with_cleanup.code_size,
            without_cleanup.code_size + 2 + JUMPDEST_LEN + 1 + MIN_DEFAULT_JUMP_LEN
        );
        assert_eq!(with_cleanup.hit_gas_sum, without_cleanup.hit_gas_sum);
    }

    #[test]
    fn accounts_for_packed_table_target_widths() {
        let values = (0..32).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        let packed = bucket_lowering_cost(&values, 32, EvmVersion::Cancun, true, 2);
        let wide = bucket_lowering_cost(&values, 32, EvmVersion::Cancun, true, 3);

        assert_eq!(wide.code_size, packed.code_size + 32);
        assert_eq!(wide.hit_gas_sum, packed.hit_gas_sum);
        assert_eq!(wide.miss_gas, packed.miss_gas);
    }

    #[test]
    fn accounts_for_indexed_jump_table_label_width() {
        assert_eq!(max_indexed_jump_base_len(1), INDEXED_JUMP_BASE_LEN);
        assert_eq!(max_indexed_jump_base_len(2), INDEXED_JUMP_BASE_LEN + 1);
        assert_eq!(max_indexed_jump_base_len(3), INDEXED_JUMP_BASE_LEN + 2);
    }

    #[test]
    fn preserves_linear_shape_outside_gas_mode() {
        let sparse = (0..64).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&sparse, OptimizationMode::None, EvmVersion::Cancun, true, 2),
            SwitchPlan::Linear
        );
        assert_eq!(
            select_switch_plan(&sparse, OptimizationMode::Size, EvmVersion::Cancun, true, 2),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn selects_buckets_for_large_sparse_switches() {
        let values = (0..32).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Buckets { .. }
        ));
    }

    #[test]
    fn bounds_bucket_table_fanout() {
        let values = (0..100).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Binary { .. }
        ));
    }

    #[test]
    fn selects_dense_table_for_compact_ranges() {
        let values = values(24);
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Size, EvmVersion::Cancun, true, 2),
            SwitchPlan::Dense { low: U256::ZERO, range: 24 }
        );
    }

    #[test]
    fn rejects_excessive_gas_optimized_table_growth() {
        let values = (0..65).map(|value| U256::from(value * 63)).collect::<Vec<_>>();
        let plan = select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2);
        let cost = match plan {
            SwitchPlan::Linear => lowering_cost(&values, values.len(), EvmVersion::Cancun, 2),
            SwitchPlan::Binary { leaf_size } => {
                lowering_cost(&values, leaf_size, EvmVersion::Cancun, 2)
            }
            SwitchPlan::Buckets { bucket_count } => {
                bucket_lowering_cost(&values, bucket_count, EvmVersion::Cancun, true, 2)
            }
            SwitchPlan::Dense { .. } => {
                dense_lowering_cost(&values, EvmVersion::Cancun, 2).unwrap().2
            }
        };
        let linear = lowering_cost(&values, values.len(), EvmVersion::Cancun, 2);
        assert!(cost.max_code_size <= linear.code_size + MAX_GAS_CODE_GROWTH);
    }

    #[test]
    fn accounts_for_bucket_cleanup_in_growth_limit() {
        let values = (0..51).map(|value| U256::from(value * 257)).collect::<Vec<_>>();
        let linear = lowering_cost(&values, values.len(), EvmVersion::Cancun, 2);
        let buckets = bucket_lowering_cost(&values, 51, EvmVersion::Cancun, true, 2);
        assert!(buckets.max_code_size > linear.code_size + MAX_GAS_CODE_GROWTH);
        assert_ne!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun, true, 2),
            SwitchPlan::Buckets { bucket_count: 51 }
        );
    }

    #[test]
    fn rejects_larger_dense_table_for_small_compact_ranges() {
        assert_eq!(
            select_switch_plan(&values(8), OptimizationMode::Size, EvmVersion::Cancun, true, 2),
            SwitchPlan::Linear
        );
    }
}
