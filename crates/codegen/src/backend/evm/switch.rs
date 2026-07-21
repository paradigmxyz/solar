//! Target-aware switch lowering selection.

use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

// Ordinary label pushes relax after layout. Use their minimum possible size so
// fixed-width tables are selected for size only when they beat the best case.
const LABEL_PUSH_LEN: usize = 2;
const JUMPDEST_LEN: usize = 1;
const DEFAULT_JUMP_LEN: usize = LABEL_PUSH_LEN + 1;

const VERY_LOW_GAS: usize = 3;
const JUMP_GAS: usize = 8;
const JUMPI_GAS: usize = 10;
const DEFAULT_JUMP_GAS: usize = VERY_LOW_GAS + JUMP_GAS;
const MOD_GAS: usize = 5;
const MUL_GAS: usize = 5;
const JUMPDEST_GAS: usize = 1;

const INDEXED_JUMP_STUB_LEN: usize = 6;
const INDEXED_JUMP_BASE_LEN: usize = 7;
const INDEXED_JUMP_GUARD_LEN: usize = 1;
const MIN_BUCKET_CASES: usize = 8;
const MAX_DENSE_RANGE: usize = 4096;
const MAX_BUCKET_CANDIDATES: usize = 33;

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
) -> SwitchPlan {
    debug_assert!(values.windows(2).all(|values| values[0] < values[1]));
    if values.len() <= 1 || optimization == OptimizationMode::None {
        return SwitchPlan::Linear;
    }

    let mut best = (lowering_cost(values, values.len(), evm_version), SwitchPlan::Linear);
    if optimization == OptimizationMode::Gas {
        for leaf_size in binary_leaf_sizes(values.len()) {
            let cost = lowering_cost(values, leaf_size, evm_version);
            if cost.gas_key() < best.0.gas_key() {
                best = (cost, SwitchPlan::Binary { leaf_size });
            }
        }
        if values.len() >= MIN_BUCKET_CASES {
            for bucket_count in bucket_count_candidates(values.len()) {
                let cost = bucket_lowering_cost(values, bucket_count, evm_version);
                if cost.gas_key() < best.0.gas_key() {
                    best = (cost, SwitchPlan::Buckets { bucket_count });
                }
            }
        }
    }
    if let Some((low, range, cost)) = dense_lowering_cost(values, evm_version) {
        let better = match optimization {
            OptimizationMode::Gas => cost.gas_key() < best.0.gas_key(),
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
    hit_gas_sum: usize,
    miss_gas: usize,
}

impl LoweringCost {
    fn gas_key(self) -> (usize, usize, usize) {
        (self.hit_gas_sum, self.miss_gas, self.code_size)
    }

    fn size_key(self) -> (usize, usize, usize) {
        (self.code_size, self.hit_gas_sum, self.miss_gas)
    }
}

fn lowering_cost(values: &[U256], leaf_size: usize, evm_version: EvmVersion) -> LoweringCost {
    if values.len() <= leaf_size {
        let mut cost = LoweringCost { code_size: DEFAULT_JUMP_LEN, ..Default::default() };
        let mut path_gas = 0;
        for &value in values {
            let test = equality_test_cost(value, evm_version);
            cost.code_size += test.code_size;
            path_gas += test.gas;
            cost.hit_gas_sum += path_gas;
        }
        cost.miss_gas = path_gas + DEFAULT_JUMP_GAS;
        return cost;
    }

    let mid = values.len() / 2;
    let split = ordered_test_cost(values[mid], evm_version);
    let left = lowering_cost(&values[..mid], leaf_size, evm_version);
    let right = lowering_cost(&values[mid..], leaf_size, evm_version);
    LoweringCost {
        code_size: split.code_size + JUMPDEST_LEN + left.code_size + right.code_size,
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
            + INDEXED_JUMP_GUARD_LEN
            + bucket_count * INDEXED_JUMP_STUB_LEN,
        hit_gas_sum: dispatch_gas * values.len(),
        miss_gas: dispatch_gas,
    };

    let mut bucket_path_gas = vec![0; bucket_count];
    for &value in values {
        let index = bucket_index(value, bucket_count);
        let test = equality_test_cost(value, evm_version);
        if bucket_path_gas[index] == 0 {
            cost.code_size += JUMPDEST_LEN + DEFAULT_JUMP_LEN;
        }
        cost.code_size += test.code_size;
        bucket_path_gas[index] += test.gas;
        cost.hit_gas_sum += bucket_path_gas[index];
        cost.miss_gas = cost.miss_gas.max(dispatch_gas + bucket_path_gas[index] + DEFAULT_JUMP_GAS);
    }
    cost
}

fn dense_lowering_cost(
    values: &[U256],
    evm_version: EvmVersion,
) -> Option<(U256, usize, LoweringCost)> {
    let low = *values.first()?;
    let high = *values.last()?;
    let range = usize::try_from(high - low).ok()?.checked_add(1)?;
    if range > MAX_DENSE_RANGE {
        return None;
    }

    let normalize_len = usize::from(!low.is_zero()) * (push_len(low, evm_version) + 2);
    let normalize_gas = usize::from(!low.is_zero()) * VERY_LOW_GAS * 3;
    let bounds_len = 1 + push_len(U256::from(range), evm_version) + 1 + 1 + LABEL_PUSH_LEN + 1;
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
                + DEFAULT_JUMP_LEN
                + JUMPDEST_LEN
                + INDEXED_JUMP_BASE_LEN
                + INDEXED_JUMP_GUARD_LEN
                + range * INDEXED_JUMP_STUB_LEN,
            hit_gas_sum: hit_gas * values.len(),
            miss_gas,
        },
    ))
}

pub(super) fn bucket_index(value: U256, bucket_count: usize) -> usize {
    usize::try_from(value % U256::from(bucket_count)).expect("bucket index must fit usize")
}

#[derive(Clone, Copy)]
struct TestCost {
    code_size: usize,
    gas: usize,
}

fn equality_test_cost(value: U256, evm_version: EvmVersion) -> TestCost {
    if value.is_zero() {
        // DUP1, ISZERO, PUSH<label>, JUMPI.
        TestCost {
            code_size: 1 + 1 + LABEL_PUSH_LEN + 1,
            gas: VERY_LOW_GAS + VERY_LOW_GAS + VERY_LOW_GAS + JUMPI_GAS,
        }
    } else {
        ordered_test_cost(value, evm_version)
    }
}

fn ordered_test_cost(value: U256, evm_version: EvmVersion) -> TestCost {
    // DUP1, PUSH<value>, EQ/GT, PUSH<label>, JUMPI.
    TestCost {
        code_size: 1 + push_len(value, evm_version) + 1 + LABEL_PUSH_LEN + 1,
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
            select_switch_plan(&values(4), OptimizationMode::Gas, EvmVersion::Cancun),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn selects_profitable_binary_leaf_size() {
        let values = (0..5).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun),
            SwitchPlan::Binary { leaf_size: 3 }
        );
    }

    #[test]
    fn accounts_for_taken_binary_split_labels() {
        let values = (0..7).map(|value| U256::from(1 + value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun),
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
    fn preserves_linear_shape_outside_gas_mode() {
        let sparse = (0..64).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(&sparse, OptimizationMode::None, EvmVersion::Cancun),
            SwitchPlan::Linear
        );
        assert_eq!(
            select_switch_plan(&sparse, OptimizationMode::Size, EvmVersion::Cancun),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn selects_buckets_for_large_sparse_switches() {
        let values = (0..32).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan(&values, OptimizationMode::Gas, EvmVersion::Cancun),
            SwitchPlan::Buckets { .. }
        ));
    }

    #[test]
    fn selects_dense_table_for_compact_ranges() {
        let values = values(24);
        assert_eq!(
            select_switch_plan(&values, OptimizationMode::Size, EvmVersion::Cancun),
            SwitchPlan::Dense { low: U256::ZERO, range: 24 }
        );
    }

    #[test]
    fn rejects_larger_dense_table_for_small_compact_ranges() {
        assert_eq!(
            select_switch_plan(&values(8), OptimizationMode::Size, EvmVersion::Cancun),
            SwitchPlan::Linear
        );
    }
}
