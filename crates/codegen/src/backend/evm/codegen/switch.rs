//! Target-aware switch lowering selection and emission.
//!
//! Constant switches start as MIR `switch` terminators. This module compares
//! several EVM shapes for the same sorted case values: a source-ordered linear
//! scan, a balanced binary tree, modulo buckets, a bounds-checked dense table,
//! and collision-free bit-slice or affine hashes. Each candidate models both
//! hit and miss paths, including the default cleanup sequence and the later
//! block-layout effects that change label widths.
//!
//! Gas mode limits extra code and ranks candidates by the modeled runtime cost.
//! Size mode uses conservative label widths while selecting a plan and lets the
//! EVM IR layout pass recover safe local-width and fallthrough wins. The emitter
//! keeps the original case order for linear scans and uses sorted values only
//! for shapes whose dispatch arithmetic requires it.

use super::{
    super::{
        assembler::Label,
        ir::{
            assembly::{estimated_indexed_jump_code_size, packs_indexed_jump},
            immediate_materialization_cost,
        },
        op, push_len,
        stack::StackOp,
    },
    EvmCodegen,
};
use crate::mir::{BlockId, Function, Terminator, ValueId};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode, SwitchLowering};
use solar_data_structures::map::FxHashSet;
use std::mem::size_of;

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

const PACKED_TERMINAL_TARGET_MAX_SIZE: usize = 2;
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

/// Placement of the switch's default target after EVM IR layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SwitchDefaultLayout {
    /// The default reaches an outlined body of unknown size.
    #[default]
    Outlined,
    /// The default terminates inline, such as a selector miss that reverts.
    Inline,
    /// The default and every case reach the same deduplicated empty terminal.
    SharedTerminal,
}

/// Inputs that are shared by every switch lowering candidate.
#[derive(Clone, Copy, Debug)]
pub(super) struct SwitchPlanOptions {
    pub(super) optimization: OptimizationMode,
    pub(super) evm_version: EvmVersion,
    pub(super) default: SwitchDefault,
    /// Conservative bound from the assembler's artifact context. Final EVM
    /// IR lowering chooses the exact width for each table.
    pub(super) table_target_width: usize,
    pub(super) max_gas_code_growth: usize,
    pub(super) max_bit_slice_gas_code_growth: usize,
    pub(super) forced: SwitchLowering,
    pub(super) layout: SwitchLayout,
}

/// Post-lowering CFG facts used to refine plan ranking.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SwitchLayout {
    /// Every case target is a distinct, single-predecessor block eligible for
    /// coalescing into a perfect-hash guard.
    pub(super) coalesce_case_targets: bool,
    /// The coalesced case targets all jump to one continuation.
    pub(super) shared_case_continuation: bool,
    /// Number of entry cases whose empty function targets are eligible for
    /// terminal deduplication into one shared `STOP`.
    pub(super) terminal_case_count: usize,
    /// Placement of the default target relative to the switch.
    pub(super) default_layout: SwitchDefaultLayout,
    /// Optimistic and conservative sizes of the entry trace before the switch.
    pub(super) trace_size_bounds: Option<(usize, usize)>,
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
            Self::Revert => push_len(evm_version, U256::ZERO) * 2 + 1,
            Self::CleanupJump => 1 + MIN_DEFAULT_JUMP_LEN,
            Self::Fallthrough => 0,
            Self::CleanupFallthrough => 1,
        }
    }

    fn max_code_size(self, evm_version: EvmVersion, target_width: usize) -> usize {
        match self {
            Self::Jump => max_default_jump_len(target_width),
            Self::Revert => push_len(evm_version, U256::ZERO) * 2 + 1,
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
            Self::Fallthrough => 0,
            Self::CleanupFallthrough => POP_GAS,
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
        let binary_size = BinarySizeContext {
            equality_costs: &equality_costs,
            ordered_costs: &ordered_costs,
            evm_version,
            default: explicit_default,
            table_target_width,
            layout,
        };
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
                    let SwitchPlan::Binary { leaf_size } = plan else { unreachable!() };
                    if optimization == OptimizationMode::Size {
                        binary_size.key(cost, linear_cost, leaf_size)
                    } else {
                        cost.gas_key()
                    }
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
                    .min_by_key(|&(cost, _)| plan_cost_key(cost, optimization))
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
            .min_by_key(|&(cost, _)| plan_cost_key(cost, optimization)),
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
    let mut best_size_key = linear_cost.size_key();
    let explicit_default = default.without_fallthrough();
    let (equality_costs, ordered_costs) = case_test_costs(
        values,
        evm_version,
        table_target_width,
        explicit_default.needs_value_cleanup(),
        coalesce_case_targets,
    );
    let binary_size = BinarySizeContext {
        equality_costs: &equality_costs,
        ordered_costs: &ordered_costs,
        evm_version,
        default: explicit_default,
        table_target_width,
        layout,
    };
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
                    let key = binary_size.key(cost, linear_cost, leaf_size);
                    if key < best_size_key {
                        best_size_key = key;
                        true
                    } else {
                        false
                    }
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
                let key = cost.size_key();
                if key < best_size_key {
                    best_size_key = key;
                    true
                } else {
                    false
                }
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
                let key = cost.size_key();
                if key < best_size_key {
                    best_size_key = key;
                    true
                } else {
                    false
                }
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

fn plan_cost_key(cost: LoweringCost, optimization: OptimizationMode) -> (usize, usize, usize) {
    if optimization == OptimizationMode::Size { cost.size_key() } else { cost.gas_key() }
}

#[derive(Clone, Copy)]
struct BinarySizeContext<'a> {
    equality_costs: &'a [TestCost],
    ordered_costs: &'a [TestCost],
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    layout: SwitchLayout,
}

impl BinarySizeContext<'_> {
    fn key(
        self,
        cost: LoweringCost,
        linear_cost: LoweringCost,
        leaf_size: usize,
    ) -> (usize, usize, usize) {
        let leaves = binary_leaf_count(self.equality_costs.len(), leaf_size);
        let locality_credit = self
            .layout
            .trace_size_bounds
            .and_then(|(prefix_min_size, prefix_layout_size)| {
                // Tail merging can replace at most one terminal reference per
                // leaf. The remaining references must still make the shared
                // terminal eligible for block-layout packing.
                if self.layout.terminal_case_count <= leaves || self.table_target_width <= 1 {
                    return None;
                }
                let linear_end = prefix_min_size.saturating_add(linear_cost.code_size);
                let first_block_size = match self.layout.default_layout {
                    SwitchDefaultLayout::Outlined => return None,
                    SwitchDefaultLayout::Inline => binary_first_block_max_size(
                        self.equality_costs,
                        self.ordered_costs,
                        leaf_size,
                        self.default
                            .max_code_size(self.evm_version, self.table_target_width)
                            .max(SwitchDefault::Revert.code_size(self.evm_version) + JUMPDEST_LEN),
                    ),
                    SwitchDefaultLayout::SharedTerminal => binary_first_block_max_size(
                        self.equality_costs,
                        self.ordered_costs,
                        leaf_size,
                        self.default.max_code_size(self.evm_version, self.table_target_width),
                    ),
                };
                let binary_end = prefix_layout_size.saturating_add(first_block_size);
                (linear_end > usize::from(u8::MAX)
                    && binary_end.saturating_add(PACKED_TERMINAL_TARGET_MAX_SIZE)
                        <= usize::from(u8::MAX))
                .then(|| {
                    self.equality_costs
                        .len()
                        .min(self.layout.terminal_case_count)
                        .saturating_add(1)
                        .saturating_mul(self.table_target_width.saturating_sub(1))
                })
            })
            .unwrap_or_default();
        let nonterminal_cases =
            self.equality_costs.len().saturating_sub(self.layout.terminal_case_count);
        // Every nonterminal case can prevent at most one leaf from ending in
        // the common empty terminal. Tail merging replaces each remaining
        // leaf's `EQ; PUSH; JUMPI; JUMP` suffix with a jump to one shared
        // eight-byte tail block.
        let terminal_leaves = leaves.saturating_sub(nonterminal_cases);
        let tail_merge_credit = terminal_leaves.saturating_mul(4).saturating_sub(8);
        let split_width_charge = self.layout.trace_size_bounds.map_or(0, |(_, prefix_size)| {
            let all_cases_terminal = self.layout.terminal_case_count >= self.equality_costs.len();
            binary_split_width_charge(
                self,
                leaf_size,
                prefix_size,
                locality_credit == 0 || !all_cases_terminal,
            )
        });
        (
            cost.code_size
                .saturating_add(split_width_charge)
                .saturating_sub(locality_credit)
                .saturating_sub(tail_merge_credit),
            cost.hit_gas_sum,
            cost.miss_gas,
        )
    }
}

fn binary_leaf_count(len: usize, leaf_size: usize) -> usize {
    if len <= leaf_size {
        1
    } else {
        let mid = len / 2;
        binary_leaf_count(mid, leaf_size) + binary_leaf_count(len - mid, leaf_size)
    }
}

fn binary_split_width_charge(
    context: BinarySizeContext<'_>,
    leaf_size: usize,
    prefix_size: usize,
    conservative_leaf_targets: bool,
) -> usize {
    let BinarySizeContext {
        equality_costs,
        ordered_costs,
        evm_version,
        default,
        table_target_width,
        ..
    } = context;

    struct Simulation<'a> {
        leaf_size: usize,
        evm_version: EvmVersion,
        default: SwitchDefault,
        table_target_width: usize,
        conservative_leaf_targets: bool,
        width_charges: &'a mut [usize],
        next_split: usize,
    }

    impl Simulation<'_> {
        fn run(
            &mut self,
            equality_costs: &[TestCost],
            ordered_costs: &[TestCost],
            offset: &mut usize,
        ) -> bool {
            if equality_costs.len() <= self.leaf_size {
                let default_size = if self.conservative_leaf_targets {
                    self.default.max_code_size(self.evm_version, self.table_target_width)
                } else {
                    self.default.code_size(self.evm_version)
                };
                *offset = equality_costs.iter().fold(
                    offset.saturating_add(default_size),
                    |offset, test| {
                        offset.saturating_add(if self.conservative_leaf_targets {
                            test.max_code_size
                        } else {
                            test.code_size
                        })
                    },
                );
                return false;
            }

            let split = self.next_split;
            self.next_split += 1;
            let mid = equality_costs.len() / 2;
            *offset = offset
                .saturating_add(ordered_costs[mid].code_size)
                .saturating_add(self.width_charges[split]);
            let mut changed = self.run(&equality_costs[mid..], &ordered_costs[mid..], offset);
            let target_width = ((usize::BITS - offset.leading_zeros()) as usize).div_ceil(8).max(1);
            let width_charge = target_width.saturating_sub(1);
            if width_charge > self.width_charges[split] {
                self.width_charges[split] = width_charge;
                changed = true;
            }
            *offset = offset.saturating_add(JUMPDEST_LEN);
            changed | self.run(&equality_costs[..mid], &ordered_costs[..mid], offset)
        }
    }

    let mut width_charges = vec![0; equality_costs.len()];
    loop {
        let mut offset = prefix_size;
        let mut simulation = Simulation {
            leaf_size,
            evm_version,
            default,
            table_target_width,
            conservative_leaf_targets,
            width_charges: &mut width_charges,
            next_split: 0,
        };
        if !simulation.run(equality_costs, ordered_costs, &mut offset) {
            return width_charges.into_iter().sum();
        }
    }
}

fn binary_first_block_max_size(
    equality_costs: &[TestCost],
    ordered_costs: &[TestCost],
    leaf_size: usize,
    leaf_default_size: usize,
) -> usize {
    if equality_costs.len() <= leaf_size {
        return equality_costs
            .iter()
            .fold(leaf_default_size, |size, test| size.saturating_add(test.max_code_size));
    }

    let mid = equality_costs.len() / 2;
    ordered_costs[mid].max_code_size.saturating_add(binary_first_block_max_size(
        &equality_costs[mid..],
        &ordered_costs[mid..],
        leaf_size,
        leaf_default_size,
    ))
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

    fn size_key(self) -> (usize, usize, usize) {
        (self.code_size, self.hit_gas_sum, self.miss_gas)
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
    let hash_len = 1 + push_len(evm_version, U256::from(bucket_count)) + 1 + 1;
    let hash_gas = VERY_LOW_GAS * 3 + MOD_GAS;
    let (mut cost, dispatch_gas) = indexed_jump_dispatch_cost(
        values.len(),
        bucket_count,
        (hash_len, hash_gas),
        evm_version,
        table_target_width,
    );

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

fn indexed_jump_dispatch_cost(
    case_count: usize,
    table_len: usize,
    hash: (usize, usize),
    evm_version: EvmVersion,
    table_target_width: usize,
) -> (LoweringCost, usize) {
    let (hash_len, hash_gas) = hash;
    let indexed_jump_gas =
        indexed_jump_gas(table_len, table_target_width, evm_version) + JUMPDEST_GAS;
    let dispatch_gas = hash_gas + indexed_jump_gas;
    let cost = LoweringCost {
        code_size: hash_len
            + estimated_indexed_jump_code_size(
                table_len,
                table_target_width,
                1,
                evm_version,
                table_target_width == 1,
            ),
        max_code_size: hash_len
            + estimated_indexed_jump_code_size(
                table_len,
                table_target_width,
                table_target_width,
                evm_version,
                false,
            ),
        hit_gas_sum: dispatch_gas * case_count,
        miss_gas: dispatch_gas,
    };
    (cost, dispatch_gas)
}

fn bounded_indexed_jump_cost(
    values: &[U256],
    range: usize,
    hash: (usize, usize),
    evm_version: EvmVersion,
    default: SwitchDefault,
    table_target_width: usize,
    shared_case_continuation: bool,
) -> LoweringCost {
    let (hash_len, hash_gas) = hash;
    let bounds_prefix_len = 1 + push_len(evm_version, U256::from(range)) + 1;
    let bounds_len = bounds_prefix_len + MIN_LABEL_PUSH_LEN + 1;
    let max_bounds_len = bounds_prefix_len + max_label_push_len(table_target_width) + 1;
    let bounds_gas = VERY_LOW_GAS * 4 + JUMPI_GAS;
    let indexed_jump_gas = indexed_jump_gas(range, table_target_width, evm_version);
    let continuation_gas = usize::from(shared_case_continuation) * DEFAULT_JUMP_GAS;
    let hit_gas = hash_gas + bounds_gas + JUMPDEST_GAS + indexed_jump_gas + continuation_gas;
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
            + estimated_indexed_jump_code_size(
                range,
                table_target_width,
                1,
                evm_version,
                table_target_width == 1,
            )
            + usize::from(shared_case_continuation) * MIN_DEFAULT_JUMP_LEN,
        max_code_size: hash_len
            + max_bounds_len
            + 1
            + max_default_jump_len(table_target_width)
            + JUMPDEST_LEN
            + estimated_indexed_jump_code_size(
                range,
                table_target_width,
                table_target_width,
                evm_version,
                false,
            ),
        hit_gas_sum: hit_gas * values.len(),
        miss_gas,
    }
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
    Some((
        low,
        range,
        bounded_indexed_jump_cost(
            values,
            range,
            (normalize_len, normalize_gas),
            evm_version,
            default,
            table_target_width,
            shared_case_continuation,
        ),
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
    let table_size = mask + 1;
    let (mut cost, dispatch_gas) = indexed_jump_dispatch_cost(
        equality_costs.len(),
        table_size,
        (hash_len, hash_gas),
        evm_version,
        table_target_width,
    );

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
    bounded_indexed_jump_cost(
        values,
        range,
        (hash_len, hash_gas),
        evm_version,
        default,
        table_target_width,
        false,
    )
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

fn indexed_jump_gas(table_size: usize, target_width: usize, evm_version: EvmVersion) -> usize {
    if packs_indexed_jump(table_size, target_width, evm_version) {
        let scale = target_width * 8;
        VERY_LOW_GAS * 6 + if scale.is_power_of_two() { VERY_LOW_GAS } else { MUL_GAS } + JUMP_GAS
    } else {
        VERY_LOW_GAS * 4 + MUL_GAS + JUMP_GAS * 2 + JUMPDEST_GAS
    }
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

#[derive(Clone, Copy, Debug)]
struct MirSwitchEntry {
    value: U256,
    value_id: ValueId,
    target: BlockId,
}

impl<'gcx> EvmCodegen<'gcx> {
    fn constant_switch_entries(
        &self,
        func: &Function,
        cases: &[(ValueId, BlockId)],
    ) -> Option<(Vec<U256>, Vec<MirSwitchEntry>)> {
        let mut entries = cases
            .iter()
            .map(|&(value_id, target)| {
                let value = func.value(value_id).as_immediate()?.as_u256()?;
                Some(MirSwitchEntry { value, value_id, target })
            })
            .collect::<Option<Vec<_>>>()?;
        let linear_values = entries.iter().map(|entry| entry.value).collect();
        entries.sort_unstable_by_key(|entry| entry.value);
        if entries.windows(2).any(|entries| entries[0].value == entries[1].value) {
            return None;
        }
        Some((linear_values, entries))
    }

    fn switch_layout(
        &self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
    ) -> SwitchLayout {
        let mut targets = FxHashSet::default();
        let coalesce_case_targets = entries.iter().all(|entry| {
            entry.target != default
                && targets.insert(entry.target)
                && func.blocks[entry.target].predecessors.len() == 1
        });
        let continuation = coalesce_case_targets.then(|| {
            entries.first().and_then(|entry| {
                let Terminator::Jump(target) = func.blocks[entry.target].terminator.as_ref()?
                else {
                    return None;
                };
                Some(*target)
            })
        });
        let shared_case_continuation = continuation.flatten().is_some_and(|continuation| {
            entries.iter().all(|entry| {
                matches!(
                    func.blocks[entry.target].terminator,
                    Some(Terminator::Jump(target)) if target == continuation
                )
            })
        });
        let terminal_case_count = if self.emitting_entry {
            entries
                .iter()
                .filter(|entry| {
                    matches!(
                        func.blocks[entry.target].terminator.as_ref(),
                        Some(Terminator::TailCall { function, args })
                            if args.is_empty() && self.empty_stop_functions.contains(*function)
                    )
                })
                .count()
        } else {
            0
        };
        let default_layout = match func.blocks[default].terminator.as_ref() {
            Some(Terminator::Revert { .. }) => SwitchDefaultLayout::Inline,
            Some(Terminator::TailCall { function, args })
                if args.is_empty() && self.empty_stop_functions.contains(*function) =>
            {
                SwitchDefaultLayout::SharedTerminal
            }
            _ => SwitchDefaultLayout::Outlined,
        };
        SwitchLayout {
            coalesce_case_targets,
            shared_case_continuation,
            terminal_case_count,
            default_layout,
            trace_size_bounds: if terminal_case_count != 0 {
                self.asm.current_trace_size_bounds(
                    self.asm.indexed_jump_target_width_bound(),
                    size_of::<u64>(),
                )
            } else {
                None
            },
        }
    }

    fn emit_linear_mir_switch(&mut self, func: &Function, cases: &[(ValueId, BlockId)]) {
        for &(value_id, target) in cases {
            let value = func.value(value_id).as_immediate().and_then(|value| value.as_u256());
            self.emit_mir_switch_eq_jump(func, value_id, value, target);
        }
    }

    fn emit_binary_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        leaf_size: usize,
    ) {
        if entries.len() <= leaf_size {
            for entry in entries {
                self.emit_mir_switch_eq_jump(func, entry.value_id, Some(entry.value), entry.target);
            }
            self.emit_mir_switch_default(default, can_fallthrough);
            return;
        }

        let mid = entries.len() / 2;
        let left_label = self.asm.new_label();
        let entry_stack = self.scheduler.stack.clone();

        // With the pivot on top, GT computes `pivot > selector`.
        self.emit_stack_op(StackOp::Dup(1));
        self.emit_operand(func, entries[mid].value_id);
        self.asm.emit_op(op::GT);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_push_label(left_label);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.instruction_executed(1, None);

        self.emit_binary_mir_switch(func, &entries[mid..], default, false, leaf_size);

        self.asm.define_label(left_label);
        self.scheduler.stack = entry_stack;
        self.emit_binary_mir_switch(func, &entries[..mid], default, can_fallthrough, leaf_size);
    }

    fn emit_bucketed_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        bucket_count: usize,
    ) {
        let mut buckets = vec![Vec::new(); bucket_count];
        for &entry in entries {
            buckets[bucket_index(entry.value, bucket_count)].push(entry);
        }
        let default_label = self.block_labels[&default];
        let empty_label = (!self.emitting_entry && buckets.iter().any(|bucket| bucket.is_empty()))
            .then(|| self.asm.new_label());
        let bucket_labels: Vec<_> = buckets
            .iter()
            .map(|bucket| {
                if bucket.is_empty() {
                    empty_label.unwrap_or(default_label)
                } else {
                    self.asm.new_label()
                }
            })
            .collect();

        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(bucket_count));
        self.scheduler.stack.push_unknown();
        self.emit_stack_op(StackOp::Swap(1));
        self.asm.emit_op(op::MOD);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_indexed_jump(bucket_labels.clone());
        self.scheduler.stack.pop();

        let entry_stack = self.scheduler.stack.clone();
        if let Some(empty_label) = empty_label {
            self.asm.define_label(empty_label);
            self.scheduler.stack = entry_stack.clone();
            self.emit_mir_switch_default(default, false);
        }
        let last_bucket = buckets.iter().rposition(|bucket| !bucket.is_empty()).unwrap();
        for (index, (label, bucket)) in bucket_labels.into_iter().zip(buckets).enumerate() {
            if bucket.is_empty() {
                continue;
            }
            self.asm.define_label(label);
            self.scheduler.stack = entry_stack.clone();
            for entry in bucket {
                self.emit_mir_switch_eq_jump(func, entry.value_id, Some(entry.value), entry.target);
            }
            self.emit_mir_switch_default(default, can_fallthrough && index == last_bucket);
        }
    }

    fn emit_dense_mir_switch(
        &mut self,
        entries: &[MirSwitchEntry],
        default: BlockId,
        low: U256,
        range: usize,
    ) {
        let mut targets = vec![self.block_labels[&default]; range];
        for entry in entries {
            let index = usize::try_from(entry.value - low)
                .expect("dense switch table index must fit usize");
            targets[index] = self.block_labels[&entry.target];
        }

        if !low.is_zero() {
            self.asm.emit_push(low);
            self.scheduler.stack.push_unknown();
            self.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_op(op::SUB);
            self.scheduler.instruction_executed_untracked(2);
        }
        self.emit_bounded_indexed_jump(default, range, targets);
    }

    fn emit_perfect_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        hash: PerfectHash,
    ) {
        match hash {
            PerfectHash::BitSlice { shift, mask } => {
                self.emit_bit_slice_mir_switch(func, entries, default, can_fallthrough, shift, mask)
            }
            PerfectHash::Affine { low, multiplier, rotate, range } => {
                self.emit_affine_mir_switch(entries, default, low, multiplier, rotate, range)
            }
        }
    }

    fn emit_bit_slice_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        shift: usize,
        mask: usize,
    ) {
        let mut slots = vec![None; mask + 1];
        for &entry in entries {
            let slot = &mut slots[bit_slice_index(entry.value, shift, mask)];
            assert!(slot.replace(entry).is_none(), "perfect switch hash must not collide");
        }
        let default_label = self.block_labels[&default];
        let miss_label = (!self.emitting_entry).then(|| self.asm.new_label());
        let slot_labels = slots
            .iter()
            .map(|slot| {
                if slot.is_some() {
                    self.asm.new_label()
                } else {
                    miss_label.unwrap_or(default_label)
                }
            })
            .collect::<Vec<_>>();

        self.emit_stack_op(StackOp::Dup(1));
        if shift != 0 {
            self.asm.emit_push(U256::from(shift));
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::SHR);
            self.scheduler.instruction_executed_untracked(2);
        }
        self.asm.emit_push(U256::from(mask));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::AND);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_indexed_jump(slot_labels.clone());
        self.scheduler.stack.pop();

        let entry_stack = self.scheduler.stack.clone();
        let last_slot = slots.iter().rposition(Option::is_some).unwrap();
        for (index, (label, entry)) in slot_labels.into_iter().zip(slots).enumerate() {
            let Some(entry) = entry else { continue };
            self.asm.define_label(label);
            self.scheduler.stack = entry_stack.clone();
            self.emit_mir_switch_eq_jump_with_miss(
                func,
                entry.value_id,
                Some(entry.value),
                entry.target,
                miss_label,
            );
            if miss_label.is_none() {
                self.emit_mir_switch_default(default, can_fallthrough && index == last_slot);
            }
        }
        if let Some(miss_label) = miss_label {
            self.asm.define_label(miss_label);
            self.scheduler.stack = entry_stack;
            self.emit_mir_switch_default(default, can_fallthrough);
        }
    }

    fn emit_affine_mir_switch(
        &mut self,
        entries: &[MirSwitchEntry],
        default: BlockId,
        low: U256,
        multiplier: U256,
        rotate: usize,
        range: usize,
    ) {
        let mut targets = vec![self.block_labels[&default]; range];
        for entry in entries {
            targets[affine_index(entry.value, low, multiplier, rotate)] =
                self.block_labels[&entry.target];
        }

        if !low.is_zero() {
            self.asm.emit_push(low);
            self.scheduler.stack.push_unknown();
            self.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_op(op::SUB);
            self.scheduler.instruction_executed_untracked(2);
        }
        if multiplier != U256::ONE {
            self.asm.emit_push(multiplier);
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::MUL);
            self.scheduler.instruction_executed_untracked(2);
        }
        if rotate != 0 {
            self.emit_stack_op(StackOp::Dup(1));
            self.asm.emit_push(U256::from(rotate));
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::SHR);
            self.scheduler.instruction_executed_untracked(2);
            self.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_push(U256::from(256 - rotate));
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::SHL);
            self.scheduler.instruction_executed_untracked(2);
            self.asm.emit_op(op::OR);
            self.scheduler.instruction_executed_untracked(2);
        }
        self.emit_bounded_indexed_jump(default, range, targets);
    }

    fn emit_bounded_indexed_jump(&mut self, default: BlockId, range: usize, targets: Vec<Label>) {
        let in_range = self.asm.new_label();
        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(range));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::GT);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_push_label(in_range);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.instruction_executed(1, None);

        let indexed_stack = self.scheduler.stack.clone();
        self.emit_stack_op(StackOp::Pop);
        self.asm.emit_push_label(self.block_labels[&default]);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(in_range);
        self.scheduler.stack = indexed_stack;
        self.asm.emit_indexed_jump(targets);
        self.scheduler.stack.pop();
    }

    fn emit_mir_switch_eq_jump(
        &mut self,
        func: &Function,
        value_id: ValueId,
        value: Option<U256>,
        target: BlockId,
    ) {
        self.emit_mir_switch_eq_jump_with_miss(func, value_id, value, target, None);
    }

    fn emit_mir_switch_eq_jump_with_miss(
        &mut self,
        func: &Function,
        value_id: ValueId,
        value: Option<U256>,
        target: BlockId,
        miss: Option<Label>,
    ) {
        self.emit_stack_op(StackOp::Dup(1));
        if value.is_some_and(|value| value.is_zero())
            && self.gcx.sess.opts.optimization != OptimizationMode::None
        {
            self.asm.emit_op(op::ISZERO);
            self.scheduler.instruction_executed_untracked(1);
        } else {
            self.emit_operand(func, value_id);
            self.asm.emit_op(op::EQ);
            self.scheduler.instruction_executed_untracked(2);
        }
        if self.emitting_entry {
            self.asm.emit_push_label(self.block_labels[&target]);
            self.asm.emit_op(op::JUMPI);
            self.scheduler.instruction_executed(1, None);
        } else {
            self.asm.emit_op(op::ISZERO);
            self.scheduler.instruction_executed_untracked(1);
            let next = miss.unwrap_or_else(|| self.asm.new_label());
            self.asm.emit_push_label(next);
            self.asm.emit_op(op::JUMPI);
            self.scheduler.instruction_executed(1, None);

            let next_stack = self.scheduler.stack.clone();
            self.emit_stack_op(StackOp::Pop);
            self.asm.emit_push_label(self.block_labels[&target]);
            self.asm.emit_op(op::JUMP);

            if miss.is_none() {
                self.asm.define_label(next);
                self.scheduler.stack = next_stack;
            }
        }
    }

    fn emit_mir_switch_default(&mut self, default: BlockId, can_fallthrough: bool) {
        if !self.emitting_entry {
            self.emit_stack_op(StackOp::Pop);
        }
        if !can_fallthrough {
            self.asm.emit_push_label(self.block_labels[&default]);
            self.asm.emit_op(op::JUMP);
        }
    }

    pub(super) fn emit_switch_terminator(
        &mut self,
        func: &Function,
        value: ValueId,
        default: BlockId,
        cases: &[(ValueId, BlockId)],
        fallthrough: Option<BlockId>,
        preserve_stack: bool,
    ) {
        let constant_entries = self.constant_switch_entries(func, cases);
        let plan = constant_entries.as_ref().map_or(
            SwitchSelection { plan: SwitchPlan::Linear, gas_code_growth: 0 },
            |(linear_values, entries)| {
                let values: Vec<_> = entries.iter().map(|entry| entry.value).collect();
                let layout = self.switch_layout(func, entries, default);
                let default = match (self.emitting_entry, fallthrough == Some(default)) {
                    (true, true) => SwitchDefault::Fallthrough,
                    (true, false) => SwitchDefault::Jump,
                    (false, true) => SwitchDefault::CleanupFallthrough,
                    (false, false) => SwitchDefault::CleanupJump,
                };
                select_switch_plan_with_linear_values_and_budget(
                    &values,
                    linear_values,
                    SwitchPlanOptions {
                        optimization: self.gcx.sess.opts.optimization,
                        evm_version: self.gcx.sess.opts.evm_version,
                        default,
                        table_target_width: self.asm.indexed_jump_target_width_bound(),
                        max_gas_code_growth: self.switch_gas_code_growth_remaining,
                        max_bit_slice_gas_code_growth: self
                            .gcx
                            .sess
                            .opts
                            .unstable
                            .switch_max_bit_slice_gas_code_growth
                            .unwrap_or(MAX_BIT_SLICE_GAS_CODE_GROWTH),
                        forced: self.gcx.sess.opts.unstable.switch_lowering,
                        layout,
                    },
                )
            },
        );
        self.switch_gas_code_growth_remaining =
            self.switch_gas_code_growth_remaining.saturating_sub(plan.gas_code_growth);
        let plan = plan.plan;
        let constant_entries = constant_entries.map(|(_, entries)| entries);

        if preserve_stack {
            debug_assert_eq!(self.scheduler.stack.top(), Some(value));
        } else if self.emitting_entry {
            // The entry's just-computed selector stays on the stack
            // through the case chain — no spill, clear, and reload —
            // and is left inert below the taken arm instead of paying
            // a POP. Every successor terminates externally and the
            // entry runs once, so the leftover word cannot accumulate.
            self.emit_value(func, value);
            while self.scheduler.depth() > 1 {
                self.emit_stack_op(StackOp::Swap(1));
                self.emit_stack_op(StackOp::Pop);
            }
        } else {
            let mut operands = Vec::with_capacity(cases.len() + 1);
            operands.push(value);
            operands.extend(cases.iter().map(|(case_val, _)| *case_val));
            self.spill_values_before_stack_clear(func, &operands);

            if self.scheduler.is_stack_only_value(value) {
                // A stack-only scrutinee has no memory home to reload after
                // the drain. Copy it to the top while it is still tracked and
                // pop the rest from beneath, like the entry dispatch path.
                self.emit_value(func, value);
                while self.scheduler.depth() > 1 {
                    self.emit_stack_op(StackOp::Swap(1));
                    self.emit_stack_op(StackOp::Pop);
                }
            } else {
                self.pop_all_stack_values();
                self.emit_value(func, value);
            }
        }

        match (plan, constant_entries) {
            (SwitchPlan::Binary { leaf_size }, Some(entries)) => {
                self.emit_binary_mir_switch(
                    func,
                    &entries,
                    default,
                    fallthrough == Some(default),
                    leaf_size,
                );
            }
            (SwitchPlan::Buckets { bucket_count }, Some(entries)) => {
                self.emit_bucketed_mir_switch(
                    func,
                    &entries,
                    default,
                    fallthrough == Some(default),
                    bucket_count,
                );
            }
            (SwitchPlan::Dense { low, range }, Some(entries)) => {
                self.emit_dense_mir_switch(&entries, default, low, range);
            }
            (SwitchPlan::Perfect { hash }, Some(entries)) => {
                self.emit_perfect_mir_switch(
                    func,
                    &entries,
                    default,
                    fallthrough == Some(default),
                    hash,
                );
            }
            _ => {
                self.emit_linear_mir_switch(func, cases);
                self.emit_mir_switch_default(default, fallthrough == Some(default));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::keccak256;

    fn values(n: usize) -> Vec<U256> {
        (0..n).map(U256::from).collect()
    }

    #[test]
    fn selects_packed_dense_for_small_dense_switches() {
        assert!(matches!(
            select_switch_plan(
                &values(4),
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::Jump,
                2,
            ),
            SwitchPlan::Dense { .. }
        ));
    }

    #[test]
    fn keeps_size_plans_conservative_before_layout() {
        let values = [0, 3, 5, 7, 8, 18, 19].into_iter().map(U256::from).collect::<Vec<_>>();
        assert_eq!(
            select_switch_plan(
                &values,
                OptimizationMode::Size,
                EvmVersion::Cancun,
                SwitchDefault::CleanupJump,
                2,
            ),
            SwitchPlan::Linear
        );
    }

    #[test]
    fn models_locally_packed_indexed_jump_sizes() {
        assert_eq!(estimated_indexed_jump_code_size(20, 1, 1, EvmVersion::Cancun, true), 30);
        assert_eq!(estimated_indexed_jump_code_size(33, 1, 1, EvmVersion::Cancun, true), 57);
        assert_eq!(estimated_indexed_jump_code_size(20, 2, 2, EvmVersion::Cancun, false), 108);
    }

    #[test]
    fn models_each_binary_split_label_width() {
        let tests = [TestCost { code_size: 1, max_code_size: 1, hit_gas: 0, miss_gas: 0 }; 2];
        let context = BinarySizeContext {
            equality_costs: &tests,
            ordered_costs: &tests,
            evm_version: EvmVersion::Cancun,
            default: SwitchDefault::Revert,
            table_target_width: 2,
            layout: SwitchLayout::default(),
        };
        let charge = |prefix_size| binary_split_width_charge(context, 1, prefix_size, false);

        assert_eq!(charge(250), 0);
        assert_eq!(charge(251), 1);
        assert_eq!(charge(65_530), 2);
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
    fn models_shared_terminal_label_widths() {
        let select = |len| {
            let mut values = (0..len)
                .map(|value| {
                    let hash = keccak256(format!("f{value}()"));
                    U256::from(u32::from_be_bytes(hash[..4].try_into().unwrap()) | 0x8000_0000)
                })
                .collect::<Vec<_>>();
            values.sort_unstable();
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
                    layout: SwitchLayout {
                        terminal_case_count: len,
                        default_layout: SwitchDefaultLayout::Inline,
                        trace_size_bounds: Some((14, 15)),
                        ..SwitchLayout::default()
                    },
                },
            )
            .plan
        };
        assert_eq!(select(16), SwitchPlan::Linear);
        assert_eq!(select(24), SwitchPlan::Binary { leaf_size: 12 });
        assert!(matches!(select(32), SwitchPlan::Binary { .. }));
        assert_eq!(select(41), SwitchPlan::Binary { leaf_size: 20 });
        assert_eq!(select(77), SwitchPlan::Binary { leaf_size: 19 });
        assert_eq!(select(79), SwitchPlan::Binary { leaf_size: 19 });
        assert_eq!(select(80), SwitchPlan::Binary { leaf_size: 10 });
    }

    #[test]
    fn models_partial_terminal_targets() {
        let select = |len| {
            let mut values = (0..len)
                .map(|value| {
                    let hash = keccak256(format!("f{value}()"));
                    U256::from(u32::from_be_bytes(hash[..4].try_into().unwrap()))
                })
                .collect::<Vec<_>>();
            values.sort_unstable();
            select_switch_plan_with_linear_values_and_budget(
                &values,
                &values,
                SwitchPlanOptions {
                    optimization: OptimizationMode::Size,
                    evm_version: EvmVersion::Osaka,
                    default: SwitchDefault::Jump,
                    table_target_width: 2,
                    max_gas_code_growth: MAX_GAS_CODE_GROWTH,
                    max_bit_slice_gas_code_growth: MAX_BIT_SLICE_GAS_CODE_GROWTH,
                    forced: SwitchLowering::Auto,
                    layout: SwitchLayout {
                        terminal_case_count: len - 1,
                        default_layout: SwitchDefaultLayout::Inline,
                        trace_size_bounds: Some((13, 22)),
                        ..SwitchLayout::default()
                    },
                },
            )
            .plan
        };

        assert_eq!(select(40), SwitchPlan::Binary { leaf_size: 10 });
        assert_eq!(select(220), SwitchPlan::Binary { leaf_size: 27 });
    }

    #[test]
    fn rejects_locality_credit_across_outlined_default() {
        let mut values = (0..39)
            .map(|value| {
                let hash = keccak256(format!("f{value}()"));
                U256::from(u32::from_be_bytes(hash[..4].try_into().unwrap()))
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        let selection = select_switch_plan_with_linear_values_and_budget(
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
                layout: SwitchLayout {
                    terminal_case_count: values.len(),
                    default_layout: SwitchDefaultLayout::Outlined,
                    trace_size_bounds: Some((14, 15)),
                    ..SwitchLayout::default()
                },
            },
        );
        assert_eq!(selection.plan, SwitchPlan::Linear);
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
    fn finds_cross_limb_bit_slice_after_pruning() {
        let values = vec![
            U256::ZERO,
            U256::ONE << 63,
            U256::ONE << 64,
            (U256::ONE << 64) | (U256::ONE << 63) | U256::ONE,
        ];
        let candidates = perfect_hash_candidates(
            &values,
            EvmVersion::Cancun,
            SwitchDefault::CleanupJump,
            2,
            false,
        );
        assert!(candidates.iter().any(|&(_, plan)| {
            plan == SwitchPlan::Perfect { hash: PerfectHash::BitSlice { shift: 63, mask: 3 } }
        }));

        let selection = select_switch_plan_with_linear_values_and_budget(
            &values,
            &values,
            SwitchPlanOptions {
                optimization: OptimizationMode::Gas,
                evm_version: EvmVersion::Cancun,
                default: SwitchDefault::CleanupJump,
                table_target_width: 2,
                max_gas_code_growth: usize::MAX,
                max_bit_slice_gas_code_growth: usize::MAX,
                forced: SwitchLowering::Perfect,
                layout: SwitchLayout::default(),
            },
        );
        assert_eq!(
            selection.plan,
            SwitchPlan::Perfect { hash: PerfectHash::BitSlice { shift: 63, mask: 3 } }
        );
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
        assert_eq!(forced.gas_code_growth, 63);
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
    fn selects_coalesced_packed_bit_slice_within_independent_cap() {
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
        let uncoalesced = select(SwitchLayout::default());
        assert!(matches!(uncoalesced.plan, SwitchPlan::Buckets { .. }), "{uncoalesced:?}");
        let selection = select(SwitchLayout {
            coalesce_case_targets: true,
            shared_case_continuation: true,
            ..SwitchLayout::default()
        });
        assert!(
            matches!(selection.plan, SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }),
            "{selection:?}",
        );
        assert!(selection.gas_code_growth <= MAX_BIT_SLICE_GAS_CODE_GROWTH);
    }

    #[test]
    fn keeps_packed_bit_slice_within_cap_for_shared_continuation() {
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
        };
        let unshared = select(false);
        assert!(
            matches!(unshared.plan, SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }),
            "{unshared:?}"
        );
        assert!(unshared.gas_code_growth <= MAX_BIT_SLICE_GAS_CODE_GROWTH);
        let shared = select(true);
        assert!(matches!(shared.plan, SwitchPlan::Perfect { hash: PerfectHash::BitSlice { .. } }));
        assert!(shared.gas_code_growth <= MAX_BIT_SLICE_GAS_CODE_GROWTH);
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
    fn selects_profitable_packed_affine_table() {
        let values = (0..5).map(|value| U256::from(value * 7919)).collect::<Vec<_>>();
        assert!(matches!(
            select_switch_plan(
                &values,
                OptimizationMode::Gas,
                EvmVersion::Cancun,
                SwitchDefault::Jump,
                2,
            ),
            SwitchPlan::Perfect { hash: PerfectHash::Affine { .. } }
        ));
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
        assert_eq!(explicit.miss_gas, fallthrough.miss_gas + DEFAULT_JUMP_GAS);
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
        assert_eq!(estimated_indexed_jump_code_size(0, 1, 1, EvmVersion::Cancun, false), 7);
        assert_eq!(estimated_indexed_jump_code_size(0, 2, 2, EvmVersion::Cancun, false), 8);
        assert_eq!(estimated_indexed_jump_code_size(0, 3, 3, EvmVersion::Cancun, false), 9);
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
