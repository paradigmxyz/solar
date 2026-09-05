//! Physical switch selection with bounded, shared table growth.
//!
//! Every strategy consumes one selector above an opaque stack prefix. Linear
//! chains test in source order; binary trees split sorted unsigned keys. Dense
//! tables subtract their minimum and range-check the normalized index. Modulo
//! buckets and collision-free bit slices retain the original selector and check
//! equality at leaves, so every non-case value reaches the default even when its
//! hash collides. Tables contain only physical block identities.
//!
//! Automatic selection keeps small switches linear, uses dense tables for short
//! ranges, and otherwise considers bounded bit slices before binary search. A
//! planner owns the gas-mode growth allowances for both artifacts of a contract;
//! runtime lowering spends them before constructor lowering. No table may exceed
//! 256 entries. Encoding and label-width decisions remain in primitive assembly.

use super::{
    ir::{self, BlockId, InstKind as I, TerminatorKind as T},
    op,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode, SwitchLowering};
use solar_data_structures::map::FxHashMap;

pub(crate) struct Planner {
    mode: SwitchLowering,
    optimization: OptimizationMode,
    version: EvmVersion,
    growth: usize,
    slice_growth: usize,
}

impl Planner {
    pub(crate) fn new(
        mode: SwitchLowering,
        optimization: OptimizationMode,
        version: EvmVersion,
        growth: Option<usize>,
        slice_growth: Option<usize>,
    ) -> Self {
        Self {
            mode,
            optimization,
            version,
            growth: growth.unwrap_or(usize::MAX),
            slice_growth: slice_growth.unwrap_or(usize::MAX),
        }
    }

    /// Returns a physical entry consuming the selector and preserving its prefix.
    pub(crate) fn lower(
        &mut self,
        module: &mut ir::Module,
        cases: &[(U256, BlockId)],
        default: BlockId,
    ) -> BlockId {
        if cases.len() < 2 || self.mode == SwitchLowering::Linear {
            return linear(module, cases, default);
        }
        let mut sorted = cases.to_vec();
        sorted.sort_unstable_by_key(|case| case.0);
        let range = sorted.last().unwrap().0 - sorted[0].0;
        let dense_count = usize::try_from(range)
            .ok()
            .and_then(|range| range.checked_add(1))
            .filter(|&count| count <= 256);
        let forced = self.mode != SwitchLowering::Auto;
        if (self.mode == SwitchLowering::Dense
            || (!forced
                && cases.len() >= 5
                && dense_count.is_some_and(|count| count <= cases.len() * 2)))
            && let Some(count) = dense_count
            && self.reserve(count.saturating_sub(cases.len()) * 3 + 12, false, forced)
        {
            return dense(module, &sorted, default, count);
        }
        if self.mode == SwitchLowering::Buckets {
            let count = if self.optimization.is_size() {
                cases.len()
            } else if self.optimization.is_gas() {
                cases.len() + 1
            } else {
                cases.len().div_ceil(4) * 5
            }
            .clamp(1, 256);
            let mut buckets = vec![Vec::new(); count];
            for &(key, target) in cases {
                buckets[(key % U256::from(count)).to::<usize>()].push((key, target));
            }
            let targets = buckets.iter().map(|bucket| linear(module, bucket, default)).collect();
            // push <bucket count>; dup2; mod; indexed_jump <checked buckets>
            return block(
                module,
                vec![I::Push(U256::from(count)), I::Dup(2), I::Op(op::MOD)],
                T::IndexedJump(targets),
            );
        }
        if (self.mode == SwitchLowering::Perfect
            || (!forced && self.optimization.is_gas() && cases.len() >= 5))
            && self.version.has_bitwise_shifting()
        {
            let minimum = cases.len().next_power_of_two();
            for count in [minimum, minimum.saturating_mul(2)] {
                if count > 256 {
                    break;
                }
                for shift in 0usize..256 {
                    let mut occupied = vec![None; count];
                    let mut collision = false;
                    for &(key, target) in cases {
                        let index = ((key >> shift) & U256::from(count - 1)).to::<usize>();
                        if occupied[index].replace((key, target)).is_some() {
                            collision = true;
                            break;
                        }
                    }
                    if collision {
                        continue;
                    }
                    if !self.reserve(count * 3 + 12, true, forced) {
                        break;
                    }
                    let targets = occupied
                        .into_iter()
                        .map(|case| match case {
                            Some(case) => linear(module, &[case], default),
                            None => linear(module, &[], default),
                        })
                        .collect();
                    // dup1; push <shift>; shr; push <mask>; and
                    // indexed_jump <equality-checked leaves>
                    return block(
                        module,
                        vec![
                            I::Dup(1),
                            I::Push(U256::from(shift)),
                            I::Op(op::SHR),
                            I::Push(U256::from(count - 1)),
                            I::Op(op::AND),
                        ],
                        T::IndexedJump(targets),
                    );
                }
            }
        }
        if !forced && cases.len() <= 4 {
            return linear(module, cases, default);
        }
        binary(module, &sorted, default)
    }

    fn reserve(&mut self, bytes: usize, slice: bool, forced: bool) -> bool {
        if forced || !self.optimization.is_gas() {
            return true;
        }
        if bytes > self.growth || (slice && bytes > self.slice_growth) {
            return false;
        }
        self.growth -= bytes;
        if slice {
            self.slice_growth -= bytes;
        }
        true
    }
}

fn block(module: &mut ir::Module, insts: Vec<I>, terminator: T) -> BlockId {
    // <physical instructions>; <terminator>
    module.blocks.push(ir::Block {
        insts: insts.into_iter().map(Into::into).collect(),
        terminator: terminator.into(),
        ..Default::default()
    })
}

fn linear(module: &mut ir::Module, cases: &[(U256, BlockId)], default: BlockId) -> BlockId {
    let Some((&(last, target), preceding)) = cases.split_last() else {
        // pop <selector>; jump <default>
        return block(module, vec![I::Op(op::POP)], T::Jump(default));
    };
    // push <last case>; sub
    // jumpi <default>, <matching edge>
    let mut next = block(module, vec![I::Push(last), I::Op(op::SUB)], T::JumpI(default, target));
    let mut exits = FxHashMap::default();
    for &(key, target) in preceding.iter().rev() {
        let target = *exits.entry(target).or_insert_with(|| {
            // pop <selector>; jump <matching edge>
            block(module, vec![I::Op(op::POP)], T::Jump(target))
        });
        // dup1; push <case>; sub
        // jumpi <next comparison>, <matching edge>
        next = block(module, vec![I::Dup(1), I::Push(key), I::Op(op::SUB)], T::JumpI(next, target));
    }
    next
}

fn binary(module: &mut ir::Module, cases: &[(U256, BlockId)], default: BlockId) -> BlockId {
    if cases.len() <= 3 {
        return linear(module, cases, default);
    }
    let middle = cases.len() / 2;
    let left = binary(module, &cases[..middle], default);
    let right = binary(module, &cases[middle..], default);
    // dup1; push <pivot>; gt
    // jumpi <keys below pivot>, <keys at or above pivot>
    block(module, vec![I::Dup(1), I::Push(cases[middle].0), I::Op(op::GT)], T::JumpI(left, right))
}

fn dense(
    module: &mut ir::Module,
    cases: &[(U256, BlockId)],
    default: BlockId,
    count: usize,
) -> BlockId {
    let minimum = cases[0].0;
    let mut targets = vec![default; count];
    for &(key, target) in cases {
        targets[(key - minimum).to::<usize>()] = target;
    }
    // indexed_jump <normalized dense table>
    let table = block(module, vec![], T::IndexedJump(targets));
    // pop <out-of-range index>; jump <default>
    let fallback = block(module, vec![I::Op(op::POP)], T::Jump(default));
    // push <minimum>; swap1; sub
    // dup1; push <count>; gt
    // jumpi <in-range table>, <default>
    block(
        module,
        vec![
            I::Push(minimum),
            I::Swap(1),
            I::Op(op::SUB),
            I::Dup(1),
            I::Push(U256::from(count)),
            I::Op(op::GT),
        ],
        T::JumpI(table, fallback),
    )
}
