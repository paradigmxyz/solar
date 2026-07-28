//! Target-aware switch lowering selection.

use super::ir::immediate_materialization_cost;
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode, SwitchLowering};

// Ordinary label pushes relax after layout. Use their minimum possible size so
// fixed-width tables are selected for size only when they beat the best case.
const MIN_LABEL_PUSH_LEN: usize = 2;
const JUMPDEST_LEN: usize = 1;
const MIN_DEFAULT_JUMP_LEN: usize = MIN_LABEL_PUSH_LEN + 1;

const VERY_LOW_GAS: usize = 3;
const JUMP_GAS: usize = 8;
const JUMPI_GAS: usize = 10;
const DEFAULT_JUMP_GAS: usize = VERY_LOW_GAS + JUMP_GAS;
const BASE_GAS: usize = 2;
const POP_GAS: usize = BASE_GAS;
const MOD_GAS: usize = 5;
const MUL_GAS: usize = 5;
const JUMPDEST_GAS: usize = 1;

const INDEXED_JUMP_BASE_LEN: usize = 7;
const MIN_BUCKET_CASES: usize = 2;
// Bound table footprint and the number of bucket blocks processed by EVM IR passes.
const MAX_BUCKET_CASES: usize = 64;
const MAX_PERFECT_BIT_TABLE_SIZE: usize = 256;
const MAX_DENSE_RANGE: usize = 4096;
const MAX_BUCKET_CANDIDATES: usize = 33;
// Guarded bit-slice tables trade predictable hit depth for more code. Keep
// their individual gas-mode growth at a round conservative plateau under unknown
// case frequencies.
pub(super) const MAX_BIT_SLICE_GAS_CODE_GROWTH: usize = 80;
/// Bounds cumulative bytecode growth per artifact under the runtime-gas objective.
///
/// Keep this a round policy limit rather than fitting it to a corpus transition.
pub(super) const MAX_GAS_CODE_GROWTH: usize = 192;

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
    /// Dispatch through a collision-free hash table.
    Perfect { hash: PerfectHash },
}

/// Collision-free hash selected for a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PerfectHash {
    /// Extract a collision-free bit slice and verify the key in its table slot.
    BitSlice { shift: usize, mask: usize },
    /// Normalize a strided set bijectively into a compact table range.
    Affine { low: U256, multiplier: U256, rotate: usize, range: usize },
}

/// Selected switch shape and its conservative growth over a linear scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SwitchSelection {
    pub(super) plan: SwitchPlan,
    pub(super) gas_code_growth: usize,
}

/// Inputs that are shared by every switch lowering candidate.
#[derive(Clone, Copy, Debug)]
pub(super) struct SwitchPlanOptions {
    pub(super) optimization: OptimizationMode,
    pub(super) evm_version: EvmVersion,
    pub(super) default: SwitchDefault,
    pub(super) table_target_width: usize,
    pub(super) max_gas_code_growth: usize,
    pub(super) max_bit_slice_gas_code_growth: usize,
    pub(super) forced: SwitchLowering,
    pub(super) layout: SwitchLayout,
}

/// Post-lowering CFG facts used to refine plan ranking.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SwitchLayout {
    /// Every case target is a distinct, single-predecessor block that CFG
    /// simplification can coalesce into a perfect-hash guard.
    pub(super) coalesce_case_targets: bool,
    /// The coalesced case targets all jump to one continuation.
    pub(super) shared_case_continuation: bool,
    /// Every entry case tail-calls an empty function that terminal deduplication
    /// maps to one nearby `STOP`.
    pub(super) shared_terminal_target: bool,
}

/// Emitted switch miss sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SwitchDefault {
    /// Jump to a fallback or default block.
    Jump,
    /// Revert inline with an empty payload.
    Revert,
    /// Pop the switch value, then jump to the default block.
    CleanupJump,
    /// Continue into the default block without emitting an instruction.
    Fallthrough,
    /// Pop the switch value, then continue into the default block.
    CleanupFallthrough,
}

impl SwitchDefault {
    fn code_size(self, evm_version: EvmVersion) -> usize {
        match self {
            Self::Jump => MIN_DEFAULT_JUMP_LEN,
            Self::Revert => push_len(U256::ZERO, evm_version) * 2 + 1,
            Self::CleanupJump => 1 + MIN_DEFAULT_JUMP_LEN,
            Self::Fallthrough => 0,
            Self::CleanupFallthrough => 1,
        }
    }

    fn max_code_size(self, evm_version: EvmVersion, target_width: usize) -> usize {
        match self {
            Self::Jump => max_default_jump_len(target_width),
            Self::Revert => push_len(U256::ZERO, evm_version) * 2 + 1,
            Self::CleanupJump => 1 + max_default_jump_len(target_width),
            Self::Fallthrough => max_default_jump_len(target_width),
            Self::CleanupFallthrough => 1 + max_default_jump_len(target_width),
        }
    }

    fn gas(self, evm_version: EvmVersion) -> usize {
        match self {
            Self::Jump => DEFAULT_JUMP_GAS,
            Self::Revert => {
                if evm_version.has_push0() {
                    BASE_GAS * 2
                } else {
                    VERY_LOW_GAS * 2
                }
            }
            Self::CleanupJump => POP_GAS + DEFAULT_JUMP_GAS,
            Self::Fallthrough => DEFAULT_JUMP_GAS,
            Self::CleanupFallthrough => POP_GAS + DEFAULT_JUMP_GAS,
        }
    }

    const fn needs_value_cleanup(self) -> bool {
        matches!(self, Self::CleanupJump | Self::CleanupFallthrough)
    }

    const fn can_fallthrough(self) -> bool {
        matches!(self, Self::Fallthrough | Self::CleanupFallthrough)
    }

    const fn without_fallthrough(self) -> Self {
        match self {
            Self::Fallthrough => Self::Jump,
            Self::CleanupFallthrough => Self::CleanupJump,
            _ => self,
        }
    }
}

/// Selects the cheapest supported switch shape for the optimization objective.
#[cfg(test)]
pub(super) fn select_switch_plan(
    values: &[U256],
    optimization: OptimizationMode,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> SwitchPlan {
    select_switch_plan_with_budget(
        values,
        optimization,
        evm_version,
        default,
        table_target_width,
        MAX_GAS_CODE_GROWTH,
    )
    .plan
}

/// Selects a switch shape within the remaining artifact-wide gas-mode growth budget.
#[cfg(test)]
pub(super) fn select_switch_plan_with_budget(
    values: &[U256],
    optimization: OptimizationMode,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    max_gas_code_growth: usize,
) -> SwitchSelection {
    select_switch_plan_with_linear_values_and_budget(
        values,
        values,
        SwitchPlanOptions {
            optimization,
            evm_version,
            default,
            table_target_width,
            max_gas_code_growth,
            max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
            forced: SwitchLowering::Auto,
            layout: SwitchLayout::default(),
        },
    )
}

/// Selects a switch shape in emitted order within the remaining artifact-wide growth budget.
pub(super) fn select_switch_plan_with_linear_values_and_budget(
    values: &[U256],
    linear_values: &[U256],
    options: SwitchPlanOptions,
) -> SwitchSelection {
    let SwitchPlanOptions {
        optimization,
        evm_version,
        default,
        table_target_width,
        max_gas_code_growth,
        max_bit_slice_gas_code_growth,
        forced,
        layout,
    } = options;
    debug_assert!(values.windows(2).all(|values| values[0] < values[1]));
    debug_assert_eq!(values.len(), linear_values.len());
    if values.len() <= 1
        || (optimization == OptimizationMode::None && forced == SwitchLowering::Auto)
    {
        return SwitchSelection { plan: SwitchPlan::Linear, gas_code_growth: 0 };
    }

    let coalesce_case_targets =
        layout.coalesce_case_targets && optimization != OptimizationMode::None;
    let (linear_equality_costs, linear_ordered_costs) = case_test_costs(
        linear_values,
        evm_version,
        table_target_width,
        default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    let linear_cost = lowering_cost_with_tests(
        linear_values,
        &linear_equality_costs,
        &linear_ordered_costs,
        values.len(),
        evm_version,
        default,
        table_target_width,
    );
    if forced == SwitchLowering::Linear {
        return SwitchSelection { plan: SwitchPlan::Linear, gas_code_growth: 0 };
    }

    if forced != SwitchLowering::Auto {
        let explicit_default = default.without_fallthrough();
        let (equality_costs, ordered_costs) = case_test_costs(
            values,
            evm_version,
            table_target_width,
            explicit_default.needs_value_cleanup(),
            coalesce_case_targets,
        );
        let candidate = match forced {
            SwitchLowering::Binary => binary_leaf_sizes(values.len())
                .into_iter()
                .map(|leaf_size| {
                    (
                        lowering_cost_with_tests(
                            values,
                            &equality_costs,
                            &ordered_costs,
                            leaf_size,
                            evm_version,
                            explicit_default,
                            table_target_width,
                        ),
                        SwitchPlan::Binary { leaf_size },
                    )
                })
                .min_by_key(|&(cost, plan)| {
                    plan_cost_key(
                        cost,
                        plan,
                        optimization,
                        values.len(),
                        table_target_width,
                        layout,
                    )
                }),
            SwitchLowering::Buckets if (2..=MAX_BUCKET_CASES).contains(&values.len()) => {
                bucket_count_candidates(values.len())
                    .into_iter()
                    .map(|bucket_count| {
                        (
                            bucket_lowering_cost_with_tests(
                                values,
                                &equality_costs,
                                bucket_count,
                                evm_version,
                                explicit_default,
                                table_target_width,
                            ),
                            SwitchPlan::Buckets { bucket_count },
                        )
                    })
                    .min_by_key(|&(cost, plan)| {
                        plan_cost_key(
                            cost,
                            plan,
                            optimization,
                            values.len(),
                            table_target_width,
                            layout,
                        )
                    })
            }
            SwitchLowering::Dense => dense_lowering_cost(
                values,
                evm_version,
                default,
                table_target_width,
                layout.shared_case_continuation,
            )
            .map(|(low, range, cost)| (cost, SwitchPlan::Dense { low, range })),
            SwitchLowering::Perfect => perfect_hash_candidates_with_tests(
                values,
                &equality_costs,
                evm_version,
                default,
                table_target_width,
            )
            .into_iter()
            .min_by_key(|&(cost, plan)| {
                plan_cost_key(cost, plan, optimization, values.len(), table_target_width, layout)
            }),
            _ => None,
        };
        let Some((cost, plan)) = candidate else {
            return SwitchSelection { plan: SwitchPlan::Linear, gas_code_growth: 0 };
        };
        let gas_code_growth = if optimization == OptimizationMode::Gas {
            cost.max_code_size.saturating_sub(linear_cost.code_size)
        } else {
            0
        };
        return SwitchSelection { plan, gas_code_growth };
    }

    let max_gas_code_size = linear_cost.code_size.saturating_add(max_gas_code_growth);
    let mut best = (linear_cost, SwitchPlan::Linear);
    let explicit_default = default.without_fallthrough();
    let (equality_costs, ordered_costs) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        explicit_default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    if matches!(optimization, OptimizationMode::Gas | OptimizationMode::Size) {
        for leaf_size in binary_leaf_sizes(values.len()) {
            let cost = lowering_cost_with_tests(
                values,
                &equality_costs,
                &ordered_costs,
                leaf_size,
                evm_version,
                explicit_default,
                table_target_width,
            );
            let better = match optimization {
                OptimizationMode::Gas => cost.is_better_for_gas_than(best.0, max_gas_code_size),
                OptimizationMode::Size => {
                    size_key_for_plan(
                        cost,
                        SwitchPlan::Binary { leaf_size },
                        values.len(),
                        table_target_width,
                        layout,
                    ) < size_key_for_plan(best.0, best.1, values.len(), table_target_width, layout)
                }
                _ => false,
            };
            if better {
                best = (cost, SwitchPlan::Binary { leaf_size });
            }
        }
    }
    if optimization == OptimizationMode::Gas
        && (MIN_BUCKET_CASES..=MAX_BUCKET_CASES).contains(&values.len())
    {
        for bucket_count in bucket_count_candidates(values.len()) {
            let cost = bucket_lowering_cost_with_tests(
                values,
                &equality_costs,
                bucket_count,
                evm_version,
                explicit_default,
                table_target_width,
            );
            if cost.is_better_for_gas_than(best.0, max_gas_code_size) {
                best = (cost, SwitchPlan::Buckets { bucket_count });
            }
        }
    }
    if let Some((low, range, cost)) = dense_lowering_cost(
        values,
        evm_version,
        default,
        table_target_width,
        layout.shared_case_continuation,
    ) {
        let better = match optimization {
            OptimizationMode::Gas => cost.is_better_for_gas_than(best.0, max_gas_code_size),
            OptimizationMode::Size => {
                size_key_for_plan(
                    cost,
                    SwitchPlan::Dense { low, range },
                    values.len(),
                    table_target_width,
                    layout,
                ) < size_key_for_plan(best.0, best.1, values.len(), table_target_width, layout)
            }
            _ => false,
        };
        if better {
            best = (cost, SwitchPlan::Dense { low, range });
        }
    }
    for (cost, plan) in perfect_hash_candidates_with_tests(
        values,
        &equality_costs,
        evm_version,
        default,
        table_target_width,
    ) {
        let better = match optimization {
            OptimizationMode::Gas => {
                let max_code_size =
                    if matches!(plan, SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }) {
                        linear_cost
                            .code_size
                            .saturating_add(max_gas_code_growth.min(max_bit_slice_gas_code_growth))
                    } else {
                        max_gas_code_size
                    };
                cost.is_better_for_gas_than(best.0, max_code_size)
            }
            OptimizationMode::Size => {
                size_key_for_plan(cost, plan, values.len(), table_target_width, layout)
                    < size_key_for_plan(best.0, best.1, values.len(), table_target_width, layout)
            }
            _ => false,
        };
        if better {
            best = (cost, plan);
        }
    }
    let gas_code_growth = if optimization == OptimizationMode::Gas && best.1 != SwitchPlan::Linear {
        best.0.max_code_size.saturating_sub(linear_cost.code_size)
    } else {
        0
    };
    SwitchSelection { plan: best.1, gas_code_growth }
}

fn plan_cost_key(
    cost: LoweringCost,
    plan: SwitchPlan,
    optimization: OptimizationMode,
    len: usize,
    table_target_width: usize,
    layout: SwitchLayout,
) -> (usize, usize, usize) {
    if optimization == OptimizationMode::Size {
        size_key_for_plan(cost, plan, len, table_target_width, layout)
    } else {
        cost.gas_key()
    }
}

fn size_key_for_plan(
    cost: LoweringCost,
    plan: SwitchPlan,
    len: usize,
    table_target_width: usize,
    layout: SwitchLayout,
) -> (usize, usize, usize) {
    let locality_credit = usize::from(
        layout.shared_terminal_target
            && table_target_width > 1
            && matches!(plan, SwitchPlan::Binary { .. }),
    ) * len;
    (cost.code_size.saturating_sub(locality_credit), cost.hit_gas_sum, cost.miss_gas)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LoweringCost {
    /// Optimistic size with minimally encoded labels and available fallthroughs.
    code_size: usize,
    /// Conservative size with widened labels and fallthroughs restored as jumps.
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
}

#[cfg(test)]
fn lowering_cost(
    values: &[U256],
    leaf_size: usize,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    coalesce_case_targets: bool,
) -> LoweringCost {
    let (equality_costs, ordered_costs) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    lowering_cost_with_tests(
        values,
        &equality_costs,
        &ordered_costs,
        leaf_size,
        evm_version,
        default,
        table_target_width,
    )
}

fn lowering_cost_with_tests(
    values: &[U256],
    equality_costs: &[TestCost],
    ordered_costs: &[TestCost],
    leaf_size: usize,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> LoweringCost {
    debug_assert_eq!(values.len(), equality_costs.len());
    debug_assert_eq!(values.len(), ordered_costs.len());
    if values.len() <= leaf_size {
        let mut cost = LoweringCost {
            code_size: default.code_size(evm_version),
            max_code_size: default.max_code_size(evm_version, table_target_width),
            ..Default::default()
        };
        let mut path_gas = 0;
        for &test in equality_costs {
            cost.code_size += test.code_size;
            cost.max_code_size += test.max_code_size;
            cost.hit_gas_sum += path_gas + test.hit_gas;
            path_gas += test.miss_gas;
        }
        cost.miss_gas = path_gas + default.gas(evm_version);
        return cost;
    }

    debug_assert!(!default.can_fallthrough());
    let mid = values.len() / 2;
    let split = ordered_costs[mid];
    let left = lowering_cost_with_tests(
        &values[..mid],
        &equality_costs[..mid],
        &ordered_costs[..mid],
        leaf_size,
        evm_version,
        default,
        table_target_width,
    );
    let right = lowering_cost_with_tests(
        &values[mid..],
        &equality_costs[mid..],
        &ordered_costs[mid..],
        leaf_size,
        evm_version,
        default,
        table_target_width,
    );
    LoweringCost {
        code_size: split.code_size + JUMPDEST_LEN + left.code_size + right.code_size,
        max_code_size: split.max_code_size
            + JUMPDEST_LEN
            + left.max_code_size
            + right.max_code_size,
        hit_gas_sum: split.hit_gas * values.len()
            + left.hit_gas_sum
            + mid * JUMPDEST_GAS
            + right.hit_gas_sum,
        miss_gas: split.miss_gas + (left.miss_gas + JUMPDEST_GAS).max(right.miss_gas),
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

#[cfg(test)]
fn bucket_lowering_cost(
    values: &[U256],
    bucket_count: usize,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    coalesce_case_targets: bool,
) -> LoweringCost {
    let (equality_costs, _) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    bucket_lowering_cost_with_tests(
        values,
        &equality_costs,
        bucket_count,
        evm_version,
        default,
        table_target_width,
    )
}

fn bucket_lowering_cost_with_tests(
    values: &[U256],
    equality_costs: &[TestCost],
    bucket_count: usize,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> LoweringCost {
    debug_assert!(!default.can_fallthrough());
    debug_assert_eq!(values.len(), equality_costs.len());
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
    for (&value, &test) in values.iter().zip(equality_costs) {
        let index = bucket_index(value, bucket_count);
        if bucket_path_gas[index] == 0 {
            cost.code_size += JUMPDEST_LEN + default.code_size(evm_version);
            cost.max_code_size +=
                JUMPDEST_LEN + default.max_code_size(evm_version, table_target_width);
        }
        cost.code_size += test.code_size;
        cost.max_code_size += test.max_code_size;
        cost.hit_gas_sum += bucket_path_gas[index] + test.hit_gas;
        bucket_path_gas[index] += test.miss_gas;
        cost.miss_gas =
            cost.miss_gas.max(dispatch_gas + bucket_path_gas[index] + default.gas(evm_version));
    }
    if default.needs_value_cleanup() && bucket_path_gas.contains(&0) {
        // One shared JUMPDEST, POP, and default jump for ordinary MIR switches.
        let default = default.without_fallthrough();
        cost.code_size += JUMPDEST_LEN + default.code_size(evm_version);
        cost.max_code_size += JUMPDEST_LEN + default.max_code_size(evm_version, table_target_width);
        cost.miss_gas = cost.miss_gas.max(dispatch_gas + default.gas(evm_version));
    }
    cost
}

fn dense_lowering_cost(
    values: &[U256],
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    shared_case_continuation: bool,
) -> Option<(U256, usize, LoweringCost)> {
    let low = *values.first()?;
    let high = *values.last()?;
    let range = usize::try_from(high - low).ok()?.checked_add(1)?;
    if range > MAX_DENSE_RANGE {
        return None;
    }

    let (low_len, low_gas) = immediate_materialization_cost(evm_version, low);
    let normalize_len = usize::from(!low.is_zero()) * (low_len + 2);
    let normalize_gas = usize::from(!low.is_zero()) * (low_gas + VERY_LOW_GAS * 2);
    let bounds_prefix_len = 1 + push_len(U256::from(range), evm_version) + 1;
    let bounds_len = bounds_prefix_len + MIN_LABEL_PUSH_LEN + 1;
    let max_bounds_len = bounds_prefix_len + max_label_push_len(table_target_width) + 1;
    let bounds_gas = VERY_LOW_GAS * 4 + JUMPI_GAS;
    let indexed_jump_gas = VERY_LOW_GAS
        + MUL_GAS
        + VERY_LOW_GAS
        + VERY_LOW_GAS
        + JUMP_GAS
        + JUMPDEST_GAS
        + VERY_LOW_GAS
        + JUMP_GAS;
    let continuation_gas = usize::from(shared_case_continuation) * DEFAULT_JUMP_GAS;
    let hit_gas = normalize_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas + continuation_gas;
    let default_body_gas =
        if default == SwitchDefault::Revert { JUMPDEST_GAS + default.gas(evm_version) } else { 0 };
    let out_of_range_miss_gas =
        normalize_gas + bounds_gas + POP_GAS + DEFAULT_JUMP_GAS + default_body_gas;
    let hole_miss_gas = (range > values.len())
        .then_some(normalize_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas + default_body_gas);
    let miss_gas =
        hole_miss_gas.map_or(out_of_range_miss_gas, |gas| out_of_range_miss_gas.max(gas));
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
                + range * indexed_jump_stub_len(table_target_width)
                + usize::from(shared_case_continuation) * MIN_DEFAULT_JUMP_LEN,
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

#[cfg(test)]
fn perfect_hash_candidates(
    values: &[U256],
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    coalesce_case_targets: bool,
) -> Vec<(LoweringCost, SwitchPlan)> {
    let (equality_costs, _) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    perfect_hash_candidates_with_tests(
        values,
        &equality_costs,
        evm_version,
        default,
        table_target_width,
    )
}

fn perfect_hash_candidates_with_tests(
    values: &[U256],
    equality_costs: &[TestCost],
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> Vec<(LoweringCost, SwitchPlan)> {
    debug_assert_eq!(values.len(), equality_costs.len());
    let mut candidates = Vec::with_capacity(4);
    if let Some(hash @ PerfectHash::Affine { .. }) = affine_hash(values, evm_version) {
        candidates.push((
            affine_lowering_cost(values, hash, evm_version, default, table_target_width),
            SwitchPlan::Perfect { hash },
        ));
    }

    if (MIN_BUCKET_CASES..=MAX_BUCKET_CASES).contains(&values.len()) {
        let differing_bits =
            values[1..].iter().fold(U256::ZERO, |bits, &value| bits | (value ^ values[0]));
        let highest_differing_bit = differing_bits.bit_len().saturating_sub(1);
        let minimum_bits = values.len().next_power_of_two().trailing_zeros() as usize;
        for bits in minimum_bits..=minimum_bits + 2 {
            let table_size = 1usize << bits;
            if table_size > MAX_PERFECT_BIT_TABLE_SIZE {
                break;
            }
            let mask = table_size - 1;
            let last_shift = if evm_version.has_bitwise_shifting() {
                (256 - bits).min(highest_differing_bit)
            } else {
                0
            };
            for shift in 0..=last_shift {
                let mut occupied = [0u64; MAX_PERFECT_BIT_TABLE_SIZE / 64];
                let collision = values.iter().any(|&value| {
                    let index = bit_slice_index(value, shift, mask);
                    let bit = 1u64 << (index % 64);
                    let word = &mut occupied[index / 64];
                    let collision = *word & bit != 0;
                    *word |= bit;
                    collision
                });
                if !collision {
                    let hash = PerfectHash::BitSlice { shift, mask };
                    candidates.push((
                        bit_slice_lowering_cost_with_tests(
                            equality_costs,
                            hash,
                            evm_version,
                            default.without_fallthrough(),
                            table_target_width,
                        ),
                        SwitchPlan::Perfect { hash },
                    ));
                    // Every nonzero shift has the same materialization cost, so
                    // no later shift at this table width can rank higher.
                    break;
                }
            }
        }
    }
    candidates
}

#[cfg(test)]
fn bit_slice_lowering_cost(
    values: &[U256],
    hash: PerfectHash,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    coalesce_case_targets: bool,
) -> LoweringCost {
    let (equality_costs, _) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    bit_slice_lowering_cost_with_tests(
        &equality_costs,
        hash,
        evm_version,
        default,
        table_target_width,
    )
}

fn bit_slice_lowering_cost_with_tests(
    equality_costs: &[TestCost],
    hash: PerfectHash,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> LoweringCost {
    let PerfectHash::BitSlice { shift, mask } = hash else { unreachable!() };
    debug_assert!(!default.can_fallthrough());
    let (shift_len, shift_gas) = immediate_materialization_cost(evm_version, U256::from(shift));
    let (mask_len, mask_gas) = immediate_materialization_cost(evm_version, U256::from(mask));
    let hash_len = 1 + usize::from(shift != 0) * (shift_len + 1) + mask_len + 1;
    let hash_gas = VERY_LOW_GAS
        + usize::from(shift != 0) * (shift_gas + VERY_LOW_GAS)
        + mask_gas
        + VERY_LOW_GAS;
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
    let table_size = mask + 1;
    let mut cost = LoweringCost {
        code_size: hash_len
            + INDEXED_JUMP_BASE_LEN
            + table_size * indexed_jump_stub_len(table_target_width),
        max_code_size: hash_len
            + max_indexed_jump_base_len(table_target_width)
            + table_size * indexed_jump_stub_len(table_target_width),
        hit_gas_sum: dispatch_gas * equality_costs.len(),
        miss_gas: dispatch_gas,
    };

    let shared_miss = default.needs_value_cleanup();
    for &test in equality_costs {
        cost.code_size += JUMPDEST_LEN + test.code_size - usize::from(shared_miss) * JUMPDEST_LEN;
        cost.max_code_size +=
            JUMPDEST_LEN + test.max_code_size - usize::from(shared_miss) * JUMPDEST_LEN;
        if !shared_miss {
            cost.code_size += default.code_size(evm_version);
            cost.max_code_size += default.max_code_size(evm_version, table_target_width);
        }
        cost.hit_gas_sum += test.hit_gas;
        cost.miss_gas = cost.miss_gas.max(dispatch_gas + test.miss_gas + default.gas(evm_version));
    }
    if shared_miss {
        cost.code_size += JUMPDEST_LEN + default.code_size(evm_version);
        cost.max_code_size += JUMPDEST_LEN + default.max_code_size(evm_version, table_target_width);
        cost.miss_gas = cost.miss_gas.max(dispatch_gas + default.gas(evm_version));
    }
    cost
}

fn affine_hash(values: &[U256], evm_version: EvmVersion) -> Option<PerfectHash> {
    if !(MIN_BUCKET_CASES..=MAX_DENSE_RANGE).contains(&values.len()) {
        return None;
    }
    let low = values[0];
    let mut stride = values[1] - low;
    for &value in &values[2..] {
        stride = gcd(stride, value - low);
        if stride == U256::ONE {
            return None;
        }
    }
    if stride == U256::ONE {
        return None;
    }

    let range = usize::try_from((values[values.len() - 1] - low) / stride).ok()?.checked_add(1)?;
    if range > MAX_DENSE_RANGE {
        return None;
    }
    let rotate = stride.trailing_zeros();
    if rotate != 0 && !evm_version.has_bitwise_shifting() {
        return None;
    }
    let odd_stride = stride >> rotate;
    let multiplier = wrapping_inverse_odd(odd_stride);
    debug_assert_eq!(odd_stride.wrapping_mul(multiplier), U256::ONE);
    Some(PerfectHash::Affine { low, multiplier, rotate, range })
}

fn affine_lowering_cost(
    values: &[U256],
    hash: PerfectHash,
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
) -> LoweringCost {
    let PerfectHash::Affine { low, multiplier, rotate, range } = hash else { unreachable!() };
    let (low_len, low_gas) = immediate_materialization_cost(evm_version, low);
    let normalize_len = usize::from(!low.is_zero()) * (low_len + 2);
    let normalize_gas = usize::from(!low.is_zero()) * (low_gas + VERY_LOW_GAS * 2);
    let (multiplier_len, multiplier_gas) = immediate_materialization_cost(evm_version, multiplier);
    let multiply_len = usize::from(multiplier != U256::ONE) * (multiplier_len + 1);
    let multiply_gas = usize::from(multiplier != U256::ONE) * (multiplier_gas + MUL_GAS);
    let (rotate_len, rotate_gas) = rotate_cost(rotate, evm_version);
    let hash_len = normalize_len + multiply_len + rotate_len;
    let hash_gas = normalize_gas + multiply_gas + rotate_gas;
    let bounds_prefix_len = 1 + push_len(U256::from(range), evm_version) + 1;
    let bounds_len = bounds_prefix_len + MIN_LABEL_PUSH_LEN + 1;
    let max_bounds_len = bounds_prefix_len + max_label_push_len(table_target_width) + 1;
    let bounds_gas = VERY_LOW_GAS * 4 + JUMPI_GAS;
    let indexed_jump_gas = VERY_LOW_GAS
        + MUL_GAS
        + VERY_LOW_GAS
        + VERY_LOW_GAS
        + JUMP_GAS
        + JUMPDEST_GAS
        + VERY_LOW_GAS
        + JUMP_GAS;
    let hit_gas = hash_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas;
    let default_body_gas =
        if default == SwitchDefault::Revert { JUMPDEST_GAS + default.gas(evm_version) } else { 0 };
    let out_of_range_miss_gas =
        hash_gas + bounds_gas + POP_GAS + DEFAULT_JUMP_GAS + default_body_gas;
    let hole_miss_gas = (range > values.len())
        .then_some(hash_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas + default_body_gas);
    let miss_gas =
        hole_miss_gas.map_or(out_of_range_miss_gas, |gas| out_of_range_miss_gas.max(gas));
    LoweringCost {
        code_size: hash_len
            + bounds_len
            + 1
            + MIN_DEFAULT_JUMP_LEN
            + JUMPDEST_LEN
            + INDEXED_JUMP_BASE_LEN
            + range * indexed_jump_stub_len(table_target_width),
        max_code_size: hash_len
            + max_bounds_len
            + 1
            + max_default_jump_len(table_target_width)
            + JUMPDEST_LEN
            + max_indexed_jump_base_len(table_target_width)
            + range * indexed_jump_stub_len(table_target_width),
        hit_gas_sum: hit_gas * values.len(),
        miss_gas,
    }
}

fn gcd(mut left: U256, mut right: U256) -> U256 {
    while !right.is_zero() {
        (left, right) = (right, left % right);
    }
    left
}

fn rotate_cost(rotate: usize, evm_version: EvmVersion) -> (usize, usize) {
    if rotate == 0 {
        return (0, 0);
    }
    let (right_len, right_gas) = immediate_materialization_cost(evm_version, U256::from(rotate));
    let (left_len, left_gas) =
        immediate_materialization_cost(evm_version, U256::from(256 - rotate));
    (5 + right_len + left_len, VERY_LOW_GAS * 5 + right_gas + left_gas)
}

fn wrapping_inverse_odd(value: U256) -> U256 {
    debug_assert!(value.bit(0));
    let mut inverse = U256::ONE;
    for _ in 0..8 {
        inverse = inverse.wrapping_mul(U256::from(2).wrapping_sub(value.wrapping_mul(inverse)));
    }
    inverse
}

pub(super) fn bit_slice_index(value: U256, shift: usize, mask: usize) -> usize {
    debug_assert!(shift < 256);
    debug_assert!(mask < MAX_PERFECT_BIT_TABLE_SIZE);
    let limbs = value.as_limbs();
    let limb = shift / 64;
    let offset = shift % 64;
    let mut bits = limbs[limb] >> offset;
    if offset != 0 && limb + 1 < limbs.len() {
        bits |= limbs[limb + 1] << (64 - offset);
    }
    bits as usize & mask
}

pub(super) fn affine_index(value: U256, low: U256, multiplier: U256, rotate: usize) -> usize {
    let normalized = (value - low).wrapping_mul(multiplier);
    let hashed = if rotate == 0 {
        normalized
    } else {
        (normalized >> rotate) | (normalized << (256 - rotate))
    };
    usize::try_from(hashed).expect("affine switch hash must fit usize")
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
    hit_gas: usize,
    miss_gas: usize,
}

fn case_test_costs(
    values: &[U256],
    evm_version: EvmVersion,
    table_target_width: usize,
    cleanup_on_hit: bool,
    coalesce_case_targets: bool,
) -> (Vec<TestCost>, Vec<TestCost>) {
    let ordered = values
        .iter()
        .map(|&value| ordered_test_cost(value, evm_version, table_target_width))
        .collect::<Vec<_>>();
    let equality = values
        .iter()
        .zip(&ordered)
        .map(|(&value, &ordered)| {
            let mut cost =
                equality_test_cost_with_ordered(value, ordered, table_target_width, cleanup_on_hit);
            if coalesce_case_targets && cleanup_on_hit {
                refine_coalesced_equality_test(&mut cost, value, table_target_width);
            }
            cost
        })
        .collect();
    (equality, ordered)
}

fn equality_test_cost_with_ordered(
    value: U256,
    ordered: TestCost,
    table_target_width: usize,
    cleanup_on_hit: bool,
) -> TestCost {
    let mut cost = if value.is_zero() {
        // DUP1, ISZERO, PUSH<label>, JUMPI.
        TestCost {
            code_size: 1 + 1 + MIN_LABEL_PUSH_LEN + 1,
            max_code_size: 1 + 1 + max_label_push_len(table_target_width) + 1,
            hit_gas: VERY_LOW_GAS + VERY_LOW_GAS + VERY_LOW_GAS + JUMPI_GAS,
            miss_gas: VERY_LOW_GAS + VERY_LOW_GAS + VERY_LOW_GAS + JUMPI_GAS,
        }
    } else {
        ordered
    };
    if cleanup_on_hit {
        // Invert the comparison and branch over POP, PUSH<label>, JUMP, JUMPDEST.
        cost.code_size += 1 + 1 + MIN_LABEL_PUSH_LEN + 1 + JUMPDEST_LEN;
        cost.max_code_size += 1 + 1 + max_label_push_len(table_target_width) + 1 + JUMPDEST_LEN;
        cost.hit_gas += VERY_LOW_GAS + POP_GAS + VERY_LOW_GAS + JUMP_GAS;
        cost.miss_gas += VERY_LOW_GAS + JUMPDEST_GAS;
    }
    cost
}

fn refine_coalesced_equality_test(cost: &mut TestCost, value: U256, table_target_width: usize) {
    // Peephole folds `EQ; ISZERO` to `SUB`, then CFG simplification coalesces
    // the single-predecessor case target into the equality guard.
    let folded_comparison = usize::from(!value.is_zero());
    cost.code_size = cost.code_size.saturating_sub(MIN_DEFAULT_JUMP_LEN + folded_comparison);
    cost.max_code_size = cost
        .max_code_size
        .saturating_sub(max_default_jump_len(table_target_width) + folded_comparison);
    cost.hit_gas = cost.hit_gas.saturating_sub(DEFAULT_JUMP_GAS + folded_comparison * VERY_LOW_GAS);
    cost.miss_gas = cost.miss_gas.saturating_sub(folded_comparison * VERY_LOW_GAS);
}

fn ordered_test_cost(value: U256, evm_version: EvmVersion, table_target_width: usize) -> TestCost {
    // DUP1, PUSH<value>, EQ/GT, PUSH<label>, JUMPI.
    let (value_len, value_gas) = immediate_materialization_cost(evm_version, value);
    let prefix_len = 1 + value_len + 1;
    TestCost {
        code_size: prefix_len + MIN_LABEL_PUSH_LEN + 1,
        max_code_size: prefix_len + max_label_push_len(table_target_width) + 1,
        hit_gas: VERY_LOW_GAS * 3 + value_gas + JUMPI_GAS,
        miss_gas: VERY_LOW_GAS * 3 + value_gas + JUMPI_GAS,
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
    fn leaves_small_entry_switches_linear() {
        assert_eq!(
            select_switch_plan(
                &values(4),
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::Jump,
                2,
            ),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn forces_benchmark_switch_lowerings() {
        let select = |values: &[U256], forced| {
            select_switch_plan_with_linear_values_and_budget(
                values,
                values,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Gas,
                    evm_version: EvmVersion::Cancun,
                    default: SwitchDefault::CleanupJump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                    forced,
                    layout: SwitchLayout::default(),
                },
            )
            .plan
        };

        assert!(matches!(select(&values(4), SwitchLowering::Binary), SwitchPlan::Binary { .. }));
        assert!(matches!(select(&values(4), SwitchLowering::Buckets), SwitchPlan::Buckets { .. }));
        assert!(matches!(select(&values(24), SwitchLowering::Dense), SwitchPlan::Dense { .. }));
        assert!(matches!(select(&values(24), SwitchLowering::Perfect), SwitchPlan::Perfect { .. }));
        assert_eq!(select(&values(32), SwitchLowering::Linear), SwitchPlan::Linear);
    }

    #[test]
    fn accounts_for_shared_terminal_label_locality() {
        let mut values = [
            0xc43b1a78u64,
            0x10c772cc,
            0xebbd40a9,
            0xd758c88e,
            0x3d492ec4,
            0xcba58af7,
            0x8a67ee70,
            0x4e580bc4,
            0x6d738a50,
            0x905f7d67,
            0x8dc714ba,
            0x40bcff2a,
            0x6d4975a2,
            0x920f5c73,
            0x5fb43592,
            0x24a75cfd,
            0x3b88f6c2,
            0xaa66aa63,
            0x98e9a73d,
            0xebd25c8f,
            0xe6e0ae36,
            0x1eb6457a,
            0x965a68f5,
            0xdca2fb5a,
            0xcbf99d38,
            0x67e648b5,
            0x54eaadab,
            0x7238232f,
            0x1f49dbe7,
            0x87d912cb,
            0xf02a00c9,
            0x41052a0d,
        ]
        .map(U256::from);
        values.sort_unstable();
        let select = |shared_terminal_target| {
            select_switch_plan_with_linear_values_and_budget(
                &values,
                &values,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Size,
                    evm_version: EvmVersion::Cancun,
                    default: SwitchDefault::Jump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                    forced: SwitchLowering::Auto,
                    layout: SwitchLayout { shared_terminal_target, ..SwitchLayout::default() },
                },
            )
            .plan
        };
        assert_eq!(select(false), SwitchPlan::Linear);
        assert!(matches!(select(true), SwitchPlan::Binary { .. }));
    }

    #[test]
    fn finds_collision_free_bit_slices() {
        let values =
            (0..16).map(|value| U256::from(value * value * 256 + value)).collect::<Vec<_>>();
        let candidates = perfect_hash_candidates(
            &values,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        assert!(candidates.iter().any(|&(_, plan)| matches!(
            plan,
            SwitchPlan::Perfect { hash: PerfectHash::BitSlice { shift: 0, mask: 15 } }
        )));
    }

    #[test]
    fn extracts_bit_slices_across_limbs() {
        let values = [
            U256::ZERO,
            U256::MAX,
            U256::from_limbs([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210, 1, 1 << 63]),
        ];
        for value in values {
            for shift in 0..256 {
                for mask in [1, 3, 7, 15, 31, 63, 127, 255] {
                    let expected = usize::try_from((value >> shift) & U256::from(mask)).unwrap();
                    assert_eq!(bit_slice_index(value, shift, mask), expected);
                }
            }
        }
    }

    #[test]
    fn shares_internal_bit_slice_misses() {
        let values =
            (0..16).map(|value| U256::from(value * value * 256 + value)).collect::<Vec<_>>();
        let hash = PerfectHash::BitSlice { shift: 0, mask: 15 };
        let cleanup = bit_slice_lowering_cost(
            &values,
            hash,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let entry = bit_slice_lowering_cost(
            &values,
            hash,
            EvmVersion::Cancun,
            SwitchDefault::Jump,
            2,
            false,
        );
        assert_eq!(cleanup.code_size, entry.code_size + values.len() * 2 + 5);
        assert_eq!(cleanup.max_code_size, entry.max_code_size + values.len() * 2 + 6);
        assert_eq!(cleanup.hit_gas_sum, entry.hit_gas_sum + values.len() * 16);
        assert_eq!(cleanup.miss_gas, entry.miss_gas + 6);
    }

    #[test]
    fn caps_bit_slice_growth_independently() {
        let values =
            (0..32).map(|value| U256::from(value * value * 256 + value)).collect::<Vec<_>>();
        assert!(
            perfect_hash_candidates(
                &values,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                false,
            )
            .iter()
            .any(|(_, plan)| matches!(
                plan,
                SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }
            ))
        );
        assert!(!matches!(
            select_switch_plan_with_budget(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                usize::MAX,
            )
            .plan,
            SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }
        ));

        let fixture =
            [0xcbf99d38u64, 0x87d912cb, 0x920f5c73, 0x41052a0d, 0x7238232f, 0x905f7d67, 0x3b88f6c2]
                .map(U256::from);
        let mut sorted = fixture;
        sorted.sort_unstable();
        let forced = select_switch_plan_with_linear_values_and_budget(
            &sorted,
            &fixture,
            SwitchPlanOptions {
                optimization: OptimizationMode::Gas,
                evm_version: EvmVersion::Cancun,
                default: SwitchDefault::CleanupJump,
                table_target_width: 2,
                max_gas_code_growth: usize::MAX,
                max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                forced: SwitchLowering::Perfect,
                layout: SwitchLayout::default(),
            },
        );
        assert_eq!(forced.gas_code_growth, 108);
        assert!(matches!(
            select_switch_plan_with_linear_values_and_budget(
                &sorted,
                &fixture,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Gas,
                    evm_version: EvmVersion::Cancun,
                    default: SwitchDefault::CleanupJump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: forced.gas_code_growth,
                    forced: SwitchLowering::Auto,
                    layout: SwitchLayout::default(),
                },
            )
            .plan,
            SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }
        ));
    }

    #[test]
    fn keeps_coalesced_bit_slice_within_independent_cap() {
        let linear_values = (0..16)
            .map(|index| U256::from((index + 1) * (index + 1) * 65536 + index))
            .collect::<Vec<_>>();
        let mut values = linear_values.clone();
        values.sort_unstable();
        let select = |layout| {
            select_switch_plan_with_linear_values_and_budget(
                &values,
                &linear_values,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Gas,
                    evm_version: EvmVersion::Cancun,
                    default: SwitchDefault::CleanupJump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                    forced: SwitchLowering::Auto,
                    layout,
                },
            )
        };
        assert!(matches!(select(SwitchLayout::default()).plan, SwitchPlan::Buckets { .. }));
        let selection = select(SwitchLayout {
            coalesce_case_targets: true,
            shared_case_continuation: true,
            ..SwitchLayout::default()
        });
        assert!(matches!(selection.plan, SwitchPlan::Buckets { .. }));
    }

    #[test]
    fn does_not_bypass_bit_slice_cap_for_shared_continuation() {
        let values = (0..6).map(|index| U256::from(20_000 + index * 6)).collect::<Vec<_>>();
        let select = |shared_case_continuation| {
            select_switch_plan_with_linear_values_and_budget(
                &values,
                &values,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Gas,
                    evm_version: EvmVersion::Cancun,
                    default: SwitchDefault::CleanupJump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                    forced: SwitchLowering::Auto,
                    layout: SwitchLayout {
                        coalesce_case_targets: true,
                        shared_case_continuation,
                        ..SwitchLayout::default()
                    },
                },
            )
            .plan
        };
        assert!(matches!(select(false), SwitchPlan::Dense { .. }));
        assert!(!matches!(
            select(true),
            SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }
        ));
    }

    #[test]
    fn maps_arithmetic_progressions_bijectively() {
        for stride in [3, 6, 257, 1024] {
            let low = U256::from(1000);
            let values = (0..32).map(|index| low + U256::from(index * stride)).collect::<Vec<_>>();
            let Some(PerfectHash::Affine { low, multiplier, rotate, range }) =
                affine_hash(&values, EvmVersion::Cancun)
            else {
                panic!("expected affine hash")
            };
            assert_eq!(range, values.len());
            for (index, &value) in values.iter().enumerate() {
                assert_eq!(affine_index(value, low, multiplier, rotate), index);
            }
        }
    }

    #[test]
    fn maps_strided_holes_bijectively() {
        let low = U256::from(1000);
        let indices = [0, 1, 3, 8, 13, 31];
        let values =
            indices.map(|index| low + U256::from(index * 6)).into_iter().collect::<Vec<_>>();
        let Some(PerfectHash::Affine { low, multiplier, rotate, range }) =
            affine_hash(&values, EvmVersion::Cancun)
        else {
            panic!("expected affine hash")
        };
        assert_eq!(range, 32);
        for (&index, &value) in indices.iter().zip(&values) {
            assert_eq!(affine_index(value, low, multiplier, rotate), index);
        }
    }

    #[test]
    fn gates_affine_rotations_by_evm_version() {
        let even = (0..8).map(|index| U256::from(index * 6)).collect::<Vec<_>>();
        assert!(affine_hash(&even, EvmVersion::Byzantium).is_none());

        let odd = (0..8).map(|index| U256::from(index * 3)).collect::<Vec<_>>();
        assert!(matches!(
            affine_hash(&odd, EvmVersion::Byzantium),
            Some(PerfectHash::Affine { rotate: 0, .. })
        ));
    }

    #[test]
    fn costs_affine_holes_as_range_sized_tables() {
        let low = U256::from(1000);
        let full = (0..32).map(|index| low + U256::from(index * 6)).collect::<Vec<_>>();
        let holes = [0, 1, 3, 8, 13, 31]
            .map(|index| low + U256::from(index * 6))
            .into_iter()
            .collect::<Vec<_>>();
        let full_hash = affine_hash(&full, EvmVersion::Cancun).unwrap();
        let hole_hash = affine_hash(&holes, EvmVersion::Cancun).unwrap();
        assert_eq!(full_hash, hole_hash);

        let full_cost = affine_lowering_cost(
            &full,
            full_hash,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
        );
        let hole_cost = affine_lowering_cost(
            &holes,
            hole_hash,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
        );
        assert_eq!(hole_cost.code_size, full_cost.code_size);
        assert_eq!(hole_cost.max_code_size, full_cost.max_code_size);
        assert_eq!(hole_cost.hit_gas_sum * full.len(), full_cost.hit_gas_sum * holes.len());
        assert!(hole_cost.miss_gas > full_cost.miss_gas);
    }

    #[test]
    fn computes_full_width_odd_inverses() {
        for value in [U256::from(3), U256::from(257), U256::MAX] {
            let inverse = wrapping_inverse_odd(value);
            assert_eq!(value.wrapping_mul(inverse), U256::ONE);
        }
    }

    #[test]
    fn selects_profitable_binary_leaf_size() {
        let values = (0..5).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::Jump,
                2,
            ),
            SwitchPlan::Binary { leaf_size: 3 }
        );
    }

    #[test]
    fn accounts_for_taken_binary_split_labels() {
        let values = (0..7).map(|value| U256::from(1 + value * 7919)).collect::<Vec<_>>();
        let binary = select_switch_plan_with_linear_values_and_budget(
            &values,
            &values,
            SwitchPlanOptions {
                optimization: OptimizationMode::Gas,
                evm_version: EvmVersion::Cancun,
                default: SwitchDefault::CleanupJump,
                table_target_width: 2,
                max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                forced: SwitchLowering::Binary,
                layout: SwitchLayout::default(),
            },
        );
        assert_eq!(binary.plan, SwitchPlan::Binary { leaf_size: 3 });
        assert!(matches!(
            select_switch_plan_with_budget(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                usize::MAX,
            )
            .plan,
            SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }
        ));
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
    fn accounts_for_linear_value_cleanup() {
        let values = values(4);
        let with_cleanup = lowering_cost(
            &values,
            values.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let without_cleanup =
            lowering_cost(&values, values.len(), EvmVersion::Cancun, SwitchDefault::Jump, 2, false);
        assert_eq!(with_cleanup.code_size, without_cleanup.code_size + values.len() * 6 + 1);
        assert_eq!(
            with_cleanup.max_code_size,
            without_cleanup.max_code_size + values.len() * 7 + 1
        );
        assert_eq!(with_cleanup.hit_gas_sum, without_cleanup.hit_gas_sum + 88);
        assert_eq!(with_cleanup.miss_gas, without_cleanup.miss_gas + 18);
    }

    #[test]
    fn accounts_for_coalesced_equality_leaves() {
        let values = [U256::from(1), U256::from(2)];
        let cost = |coalesce_case_targets| {
            lowering_cost(
                &values,
                values.len(),
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                coalesce_case_targets,
            )
        };
        let separate = cost(false);
        let coalesced = cost(true);
        assert_eq!(separate.code_size, coalesced.code_size + 8);
        assert_eq!(separate.max_code_size, coalesced.max_code_size + 10);
        assert_eq!(separate.hit_gas_sum, coalesced.hit_gas_sum + 31);
        assert_eq!(separate.miss_gas, coalesced.miss_gas + 6);
    }

    #[test]
    fn costs_linear_values_in_emitted_order() {
        let sorted = values(3);
        let source_order = [U256::from(1), U256::from(2), U256::ZERO];
        let sorted = lowering_cost(
            &sorted,
            sorted.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let source_order = lowering_cost(
            &source_order,
            source_order.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        assert_eq!(source_order.code_size, sorted.code_size);
        assert_eq!(source_order.hit_gas_sum, sorted.hit_gas_sum + 6);
    }

    #[test]
    fn accounts_for_linear_default_fallthrough() {
        let values = values(8);
        let explicit = lowering_cost(
            &values,
            values.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let fallthrough = lowering_cost(
            &values,
            values.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupFallthrough,
            2,
            false,
        );
        assert_eq!(explicit.code_size, fallthrough.code_size + MIN_DEFAULT_JUMP_LEN);
        assert_eq!(explicit.max_code_size, fallthrough.max_code_size);
        assert_eq!(explicit.miss_gas, fallthrough.miss_gas);
    }

    #[test]
    fn accounts_for_bucket_value_cleanup() {
        let values = [U256::ZERO, U256::from(2)];
        let with_cleanup = bucket_lowering_cost(
            &values,
            4,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let without_cleanup =
            bucket_lowering_cost(&values, 4, EvmVersion::Cancun, SwitchDefault::Jump, 2, false);
        assert_eq!(
            with_cleanup.code_size,
            without_cleanup.code_size + 12 + 2 + JUMPDEST_LEN + 1 + MIN_DEFAULT_JUMP_LEN
        );
        assert_eq!(with_cleanup.hit_gas_sum, without_cleanup.hit_gas_sum + 32);
    }

    #[test]
    fn accounts_for_packed_table_target_widths() {
        let values = (0..32).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        let packed = bucket_lowering_cost(
            &values,
            32,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let wide = bucket_lowering_cost(
            &values,
            32,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            3,
            false,
        );

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
    fn accounts_for_pre_shanghai_selector_reverts() {
        assert_eq!(SwitchDefault::Revert.code_size(EvmVersion::Berlin), 5);
        assert_eq!(SwitchDefault::Revert.max_code_size(EvmVersion::Berlin, 2), 5);
        assert_eq!(SwitchDefault::Revert.gas(EvmVersion::Berlin), 6);
        assert_eq!(SwitchDefault::Revert.code_size(EvmVersion::Cancun), 3);
        assert_eq!(SwitchDefault::Revert.gas(EvmVersion::Cancun), 4);
        assert_eq!(SwitchDefault::Jump.max_code_size(EvmVersion::Berlin, 2), 4);
    }

    #[test]
    fn preserves_linear_shape_without_optimization() {
        let sparse = (0..64).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(
                &sparse,
                OptimizationMode::None,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Linear
        );
        assert!(matches!(
            select_switch_plan(
                &sparse,
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }
        ));
    }

    #[test]
    fn selects_affine_hash_for_arithmetic_progressions() {
        let values = (0..32).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan_with_budget(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                usize::MAX,
            ),
            SwitchSelection { plan: SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }, .. }
        ));
    }

    #[test]
    fn selects_affine_hash_beyond_bit_table_limit() {
        let values = (0..100).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan_with_budget(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                usize::MAX,
            ),
            SwitchSelection { plan: SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }, .. }
        ));
    }

    #[test]
    fn selects_dense_table_for_compact_ranges() {
        let values = values(24);
        assert!(matches!(
            select_switch_plan(
                &values,
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Dense { low: U256::ZERO, range: 24 }
        ));
    }

    #[test]
    fn selects_dense_table_with_holes() {
        let values = (0..24).filter(|&value| value != 12).map(U256::from).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(
                &values,
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Dense { low: U256::ZERO, range: 24 }
        );
    }

    #[test]
    fn rejects_excessive_gas_optimized_table_growth() {
        let values = (0..65).map(|value| U256::from(value * 63)).collect::<Vec<_>>();
        let selection = select_switch_plan_with_budget(
            &values,
            OptimizationMode::Gas,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            MAX_GAS_CODE_GROWTH,
        );
        assert!(selection.gas_code_growth <= MAX_GAS_CODE_GROWTH);
    }

    #[test]
    fn accounts_for_bucket_cleanup_in_growth_limit() {
        let values = (0..51).map(|value| U256::from(value * 257)).collect::<Vec<_>>();
        let linear = lowering_cost(
            &values,
            values.len(),
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        let buckets = bucket_lowering_cost(
            &values,
            51,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        assert!(buckets.max_code_size > linear.code_size + MAX_GAS_CODE_GROWTH);
        assert_ne!(
            select_switch_plan(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Buckets { bucket_count: 51 }
        );
    }

    #[test]
    fn selects_dense_table_for_small_internal_switches() {
        assert_eq!(
            select_switch_plan(
                &values(8),
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Dense { low: U256::ZERO, range: 8 }
        );
    }

    #[test]
    fn accounts_for_dense_hole_misses() {
        let dense = [U256::ZERO, U256::from(2)];
        let full = values(3);
        let dense =
            dense_lowering_cost(&dense, EvmVersion::Cancun, SwitchDefault::CleanupJump, 2, false)
                .unwrap()
                .2;
        let full =
            dense_lowering_cost(&full, EvmVersion::Cancun, SwitchDefault::CleanupJump, 2, false)
                .unwrap()
                .2;
        assert!(dense.miss_gas > full.miss_gas);
    }

    #[test]
    fn accounts_for_dense_shared_revert_entry() {
        let values = values(24);
        let jump = dense_lowering_cost(&values, EvmVersion::Cancun, SwitchDefault::Jump, 2, false)
            .unwrap()
            .2;
        let revert =
            dense_lowering_cost(&values, EvmVersion::Cancun, SwitchDefault::Revert, 2, false)
                .unwrap()
                .2;
        assert_eq!(
            revert.miss_gas,
            jump.miss_gas + JUMPDEST_GAS + SwitchDefault::Revert.gas(EvmVersion::Cancun)
        );
    }

    #[test]
    fn accounts_for_compact_max_adjacent_constants() {
        let low = U256::MAX - U256::from(64 * 6);
        let values = (0..65).map(|index| low + U256::from(index * 6)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan(
                &values,
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }
        ));

        let plan = select_switch_plan(
            &values,
            OptimizationMode::Gas,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
        );
        assert!(!matches!(plan, SwitchPlan::Dense { .. }));
    }

    #[test]
    fn bounds_cumulative_gas_mode_growth() {
        let budget = 512;
        let values = (0..48).map(|value| U256::from(value * 4)).collect::<Vec<_>>();
        let mut remaining = budget;
        let mut growth = 0;
        for _ in 0..21 {
            let selection = select_switch_plan_with_budget(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
                remaining,
            );
            growth += selection.gas_code_growth;
            remaining = remaining.saturating_sub(selection.gas_code_growth);
        }
        assert!(growth <= budget);
        assert!(growth > 0);
    }
}
