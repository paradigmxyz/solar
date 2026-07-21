//! Target-aware switch lowering selection.

use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

const LABEL_PUSH_LEN: usize = 3;
const JUMPDEST_LEN: usize = 1;
const DEFAULT_JUMP_LEN: usize = LABEL_PUSH_LEN + 1;

const VERY_LOW_GAS: usize = 3;
const JUMP_GAS: usize = 8;
const JUMPI_GAS: usize = 10;
const DEFAULT_JUMP_GAS: usize = VERY_LOW_GAS + JUMP_GAS;

/// Selected control-flow shape for a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SwitchPlan {
    /// Test every case in source order.
    Linear,
    /// Recursively split sorted cases, using linear leaves of at most this size.
    Binary { leaf_size: usize },
}

/// Selects the cheapest supported switch shape for the optimization objective.
pub(super) fn select_switch_plan(
    values: &[U256],
    optimization: OptimizationMode,
    evm_version: EvmVersion,
) -> SwitchPlan {
    if values.len() <= 1 || optimization != OptimizationMode::Gas {
        return SwitchPlan::Linear;
    }

    let mut best = (lowering_cost(values, values.len(), evm_version), SwitchPlan::Linear);
    for leaf_size in 1..values.len() {
        let cost = lowering_cost(values, leaf_size, evm_version);
        if cost.gas_key() < best.0.gas_key() {
            best = (cost, SwitchPlan::Binary { leaf_size });
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
        hit_gas_sum: split.gas * values.len() + left.hit_gas_sum + right.hit_gas_sum,
        miss_gas: split.gas + left.miss_gas.max(right.miss_gas),
    }
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
        assert_eq!(
            select_switch_plan(&values(5), OptimizationMode::Gas, EvmVersion::Cancun),
            SwitchPlan::Binary { leaf_size: 3 }
        );
    }

    #[test]
    fn preserves_linear_shape_outside_gas_mode() {
        for optimization in [OptimizationMode::None, OptimizationMode::Size] {
            assert_eq!(
                select_switch_plan(&values(64), optimization, EvmVersion::Cancun),
                SwitchPlan::Linear
            );
        }
    }
}
