//! Target-dependent selection of compact immediate materializations.
//!
//! A literal `PUSHn` is not always the shortest way to construct a 256-bit constant. For each
//! concrete immediate, this pass compares the literal encoding with a fixed set of equivalent
//! recipes: `PUSH0; NOT` for an all-ones word, `NOT` of a shorter inverse, and shift-based forms
//! for masks or values with trailing zero bytes. Size and unoptimized builds emit the recipe with
//! the fewest encoded bytes, keeping the literal on ties so the pass never increases code size. Gas
//! builds rank recipes by the target's lifetime cost: static gas over the expected executions plus
//! the deposit of every encoded byte, again keeping the literal on ties.
//!
//! Like solc's constant optimizer, the lifetime cost charges the expected executions once per
//! distinct value while every copy pays its own deposit. A value repeated across many sites, such
//! as a string literal in a family of functions, therefore keeps its compact recipe, while a
//! unique constant becomes a literal once its runtime gas outweighs the extra bytes: a mid-width
//! mask such as `2**64 - 1` at the default 200 runs, and every recipe once runtime gas dominates
//! the deposit. Only copies in hot blocks are counted. Cold blocks end in a revert, so their
//! constants keep the fewest bytes.
//!
//! A gas build that exceeds EIP-170 is rescued in steps. First, constants take their shortest
//! recipe until they cover the bytes over the limit, those giving up the least static gas per
//! saved byte first; a constant with a copy in a loop is weighed over the iterations assumed for a
//! loop of unknown trip count. If the runtime still does not fit, parametric outlining runs with
//! gas-first constants instead, and only then does every constant take its shortest recipe too.
//!
//! Selection accounts for the active EVM version: `PUSH0` and shift opcodes are used only when the
//! target supports them. The exported cost helpers take the same [`ImmediatePolicy`], so other EVM
//! IR passes compare a prospective rewrite with the bytes and static gas that this pass emits for a
//! single hot copy. The MIR target model and jump-table lowering in the assembler keep the byte
//! policy.
//!
//! Recipe emission recursively selects materializations for child pushes, so one pass reaches a
//! fixed point. The default pipeline expands recipes once before structural cleanup because tail
//! merging and outlining profit from the concrete instruction shape. Push reordering and
//! assembly-only lowering use the same recipe API for constants they inspect or introduce later.

use super::EvmPass;
use crate::{
    backend::evm::{
        ir::{Instruction, Metadata, Module, SizeRescue},
        op::{self, WORD_BYTES},
    },
    target::{Cost, GasTier, Target},
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

pub(super) struct CompactPushes;

impl EvmPass for CompactPushes {
    fn name(&self) -> &'static str {
        "compact-pushes"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        compact_pushes(gcx, module)
    }
}
const EVM_WORD_BITS: usize = WORD_BYTES * 8;
const MIN_COMPACT_MASK_WIDTH: u8 = 5;
const BASE_GAS: usize = GasTier::Base.fixed_gas() as usize;
const VERY_LOW_GAS: usize = GasTier::VeryLow.fixed_gas() as usize;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let target = Target::new(gcx);
    let bytes_policy = ImmediatePolicy::Bytes(target.evm_version());
    let policy = match module.size_rescue {
        SizeRescue::Full => bytes_policy,
        SizeRescue::None | SizeRescue::Constants { .. } | SizeRescue::Outline => {
            ImmediatePolicy::of(target)
        }
    };
    let copies = hot_copies(module, policy);
    // The pass runs again after data packing, so the budget shrinks by what each run saves.
    let rescued = match module.size_rescue {
        SizeRescue::Constants { bytes } => {
            let (rescued, saved) = rescued_constants(module, policy, &copies, bytes);
            module.size_rescue = SizeRescue::Constants { bytes: bytes.saturating_sub(saved) };
            rescued
        }
        SizeRescue::None | SizeRescue::Outline | SizeRescue::Full => FxHashSet::default(),
    };
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        let cold = block.metadata.hotness.is_cold();
        let policy_of = |value: U256| {
            if cold || rescued.contains(&value) {
                bytes_policy
            } else {
                policy.with_copies(copies.get(&value).copied().unwrap_or(1))
            }
        };
        if !block.instructions.iter().any(|inst| {
            inst.concrete_immediate().is_some_and(|value| {
                !matches!(select(policy_of(value), value).1, CompactPush::Literal)
            })
        }) {
            continue;
        }
        scratch.clear();
        std::mem::swap(&mut block.instructions, &mut scratch);
        block.instructions.reserve(scratch.len());
        for inst in scratch.drain(..) {
            let Some(value) = inst.concrete_immediate() else {
                block.instructions.push(inst);
                continue;
            };
            let materialization = ImmediateMaterialization::with_policy(policy_of(value), value);
            if matches!(materialization.recipe, CompactPush::Literal) {
                block.instructions.push(inst);
            } else {
                materialize_selected(&mut block.instructions, materialization, &inst.metadata);
                changed = true;
            }
        }
    }
    changed
}

/// Counts the copies of each immediate that a recipe could replace in hot blocks, when `policy`
/// weighs gas against deposit.
fn hot_copies(module: &Module, policy: ImmediatePolicy) -> FxHashMap<U256, u32> {
    let mut copies = FxHashMap::<U256, u32>::default();
    if let ImmediatePolicy::Bytes(_) = policy {
        return copies;
    }
    let evm_version = policy.evm_version();
    for block in module.blocks.iter().filter(|block| !block.metadata.hotness.is_cold()) {
        for value in block.instructions.iter().filter_map(Instruction::concrete_immediate) {
            if push_width(evm_version, value) >= MIN_COMPACT_MASK_WIDTH {
                *copies.entry(value).or_default() += 1;
            }
        }
    }
    copies
}

/// Chooses the hot constants whose shortest recipes together save at least `bytes` bytes over
/// what `policy` selects, those giving up the least static gas per saved byte first, and returns
/// them with the bytes they save. A constant with a copy in a loop pays its extra gas on every
/// iteration a loop without a known trip count is assumed to run.
fn rescued_constants(
    module: &Module,
    policy: ImmediatePolicy,
    copies: &FxHashMap<U256, u32>,
    bytes: usize,
) -> (FxHashSet<U256>, usize) {
    let bytes_policy = ImmediatePolicy::Bytes(policy.evm_version());
    let looped = module
        .blocks
        .iter()
        .filter(|block| block.metadata.in_loop && !block.metadata.hotness.is_cold())
        .flat_map(|block| block.instructions.iter().filter_map(Instruction::concrete_immediate))
        .collect::<FxHashSet<_>>();
    let mut candidates = copies
        .iter()
        .filter_map(|(&value, &count)| {
            let ((kept_len, kept_gas), _) = select(policy.with_copies(count), value);
            let ((short_len, short_gas), _) = select(bytes_policy, value);
            let saved = kept_len.checked_sub(short_len)? * count as usize;
            let executions =
                if looped.contains(&value) { Target::UNCOUNTED_LOOP_EXECUTIONS } else { 1 };
            let gas = short_gas.saturating_sub(kept_gas) as u128 * u128::from(executions);
            (saved != 0).then_some((value, saved, gas))
        })
        .collect::<Vec<_>>();
    // Least gas per saved byte first, then the larger saving, then the value itself.
    candidates.sort_unstable_by(|&(a, a_saved, a_gas), &(b, b_saved, b_gas)| {
        (a_gas * b_saved as u128)
            .cmp(&(b_gas * a_saved as u128))
            .then(b_saved.cmp(&a_saved))
            .then(a.cmp(&b))
    });
    let mut rescued = FxHashSet::default();
    let mut saved = 0;
    for (value, value_saved, _) in candidates {
        if saved >= bytes {
            break;
        }
        rescued.insert(value);
        saved += value_saved;
    }
    (rescued, saved)
}

fn push(value: U256) -> Instruction {
    Instruction::push_value(value)
}

/// One instruction in a selected immediate materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend) enum ImmediateMaterializationOp {
    Push(U256),
    Opcode(u8),
}

/// How a constant's materialization is chosen.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ImmediatePolicy {
    /// Fewest encoded bytes, keeping the literal on ties.
    Bytes(EvmVersion),
    /// Least lifetime cost under the target: static gas over the value's expected executions
    /// plus the deposit of every encoded byte in each of its `copies`.
    Lifetime { target: Target, copies: u32 },
}

impl ImmediatePolicy {
    /// The policy of `target`'s objective for a single copy: lifetime cost when optimizing for
    /// gas, fewest bytes otherwise.
    pub(crate) fn of(target: Target) -> Self {
        if target.optimization().is_gas() {
            Self::Lifetime { target, copies: 1 }
        } else {
            Self::Bytes(target.evm_version())
        }
    }

    /// This policy for a value materialized at `copies` sites.
    #[must_use]
    pub(crate) fn with_copies(self, copies: u32) -> Self {
        match self {
            Self::Bytes(_) => self,
            Self::Lifetime { target, .. } => Self::Lifetime { target, copies: copies.max(1) },
        }
    }

    /// The EVM version whose opcodes the recipes may use.
    pub(crate) fn evm_version(self) -> EvmVersion {
        match self {
            Self::Bytes(evm_version) => evm_version,
            Self::Lifetime { target, .. } => target.evm_version(),
        }
    }

    /// The rank a recipe of `len` bytes and `gas` static gas takes; lower is better.
    fn rank(self, len: usize, gas: usize) -> u128 {
        match self {
            Self::Bytes(_) => len as u128,
            Self::Lifetime { target, copies } => {
                let bytes = (len as u32).saturating_mul(copies);
                target.lifetime_gas(Cost::new(gas as u32, bytes))
            }
        }
    }
}

/// The selected materialization for one concrete immediate.
#[derive(Clone, Copy)]
pub(in crate::backend) struct ImmediateMaterialization {
    policy: ImmediatePolicy,
    value: U256,
    recipe: CompactPush,
}

impl ImmediateMaterialization {
    /// Returns the shortest materialization for `value` on `evm_version`.
    pub(in crate::backend) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self::with_policy(ImmediatePolicy::Bytes(evm_version), value)
    }

    /// Returns the materialization `policy` selects for `value`.
    pub(in crate::backend) fn with_policy(policy: ImmediatePolicy, value: U256) -> Self {
        Self { policy, value, recipe: select(policy, value).1 }
    }

    /// Returns the materialization's maximum relative stack height.
    pub(in crate::backend) fn stack_peak(self) -> usize {
        self.metrics().stack_peak
    }

    /// Visits each concrete instruction in execution order.
    pub(in crate::backend) fn for_each(self, mut f: impl FnMut(ImmediateMaterializationOp)) {
        self.for_each_inner(&mut f);
    }

    fn for_each_inner(self, f: &mut impl FnMut(ImmediateMaterializationOp)) {
        let push = ImmediateMaterializationOp::Push;
        let opcode = ImmediateMaterializationOp::Opcode;
        let child = |value| Self::with_policy(self.policy, value);
        match self.recipe {
            CompactPush::Literal => f(push(self.value)),
            CompactPush::FullWord => {
                child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::LowerAllOnesMask { shift } => {
                child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
                child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHR));
            }
            CompactPush::Not => {
                child(!self.value).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::Shl { shift } => {
                child(self.value >> usize::from(shift)).for_each_inner(f);
                child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHL));
            }
        }
    }

    fn metrics(self) -> ImmediateMaterializationMetrics {
        let mut metrics = ImmediateMaterializationMetrics::default();
        let mut depth = 0usize;
        let evm_version = self.policy.evm_version();
        self.for_each(|materialized| match materialized {
            ImmediateMaterializationOp::Push(value) => {
                let (len, gas) = literal_cost(evm_version, value);
                metrics.encoded_len += len;
                metrics.static_gas += gas;
                depth += 1;
                metrics.stack_peak = metrics.stack_peak.max(depth);
            }
            ImmediateMaterializationOp::Opcode(opcode) => {
                let (inputs, outputs) =
                    op::stack_io(opcode).expect("compact immediate recipes use known EVM opcodes");
                depth = depth - usize::from(inputs) + usize::from(outputs);
                metrics.encoded_len += 1;
                metrics.static_gas += match opcode {
                    op::NOT | op::SHL | op::SHR => VERY_LOW_GAS,
                    _ => unreachable!("compact immediate recipes use very-low-gas opcodes"),
                };
            }
        });
        debug_assert_eq!(depth, 1);
        metrics
    }
}

#[derive(Default)]
struct ImmediateMaterializationMetrics {
    encoded_len: usize,
    static_gas: usize,
    stack_peak: usize,
}

pub(super) fn materialize_immediate(
    instructions: &mut Vec<Instruction>,
    policy: ImmediatePolicy,
    value: U256,
) {
    materialize_selected(
        instructions,
        ImmediateMaterialization::with_policy(policy, value),
        &Metadata::default(),
    );
}

/// Every replacement carries the source debug information of the push it materializes.
fn materialize_selected(
    instructions: &mut Vec<Instruction>,
    materialization: ImmediateMaterialization,
    source: &Metadata,
) {
    materialization.for_each(|op| {
        let mut replacement = match op {
            ImmediateMaterializationOp::Push(value) => push(value),
            ImmediateMaterializationOp::Opcode(opcode) => Instruction::opcode(opcode),
        };
        replacement.metadata.copy_source_debug_from(source);
        instructions.push(replacement);
    });
}

/// Returns the byte length, static gas, and recipe `policy` selects for `value`.
fn select(policy: ImmediatePolicy, value: U256) -> ((usize, usize), CompactPush) {
    let evm_version = policy.evm_version();
    let width = push_width(evm_version, value);
    let literal = literal_cost(evm_version, value);
    // NOT recipes require a full-width input. A shifted nonzero literal needs
    // at least two PUSH1s and SHL (five bytes, nine gas), so PUSH4 and shorter
    // already win or tie every recipe under either policy. Keep the literal on
    // ties, as the full search does.
    if width < MIN_COMPACT_MASK_WIDTH {
        return (literal, CompactPush::Literal);
    }
    let mut best = (literal, CompactPush::Literal);
    let mut consider = |(len, gas): (usize, usize), compact| {
        if policy.rank(len, gas) < policy.rank(best.0.0, best.0.1) {
            best = ((len, gas), compact);
        }
    };
    let zero = literal_cost(evm_version, U256::ZERO);

    if value == U256::MAX {
        consider((zero.0 + 1, zero.1 + VERY_LOW_GAS), CompactPush::FullWord);
    }

    if evm_version.has_bitwise_shifting() {
        let bytes = value.to_be_bytes::<WORD_BYTES>();
        let start = WORD_BYTES - width as usize;
        if bytes[start..].iter().all(|&byte| byte == 0xff) {
            let shift = EVM_WORD_BITS - usize::from(width) * 8;
            let shift_cost = literal_cost(evm_version, U256::from(shift));
            consider(
                (zero.0 + 1 + shift_cost.0 + 1, zero.1 + VERY_LOW_GAS * 2 + shift_cost.1),
                CompactPush::LowerAllOnesMask { shift: shift as u8 },
            );
        }
    }

    if width as usize == WORD_BYTES {
        let inverted = !value;
        if push_width(evm_version, inverted) < width {
            let (inverse, _) = select(policy, inverted);
            consider((inverse.0 + 1, inverse.1 + VERY_LOW_GAS), CompactPush::Not);
        }
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if evm_version.has_bitwise_shifting()
        && trailing_zero_bytes > 0
        && trailing_zero_bytes < WORD_BYTES
    {
        let shift = trailing_zero_bytes * 8;
        let (shifted, _) = select(policy, value >> shift);
        let (amount, _) = select(policy, U256::from(shift));
        consider(
            (shifted.0 + amount.0 + 1, shifted.1 + amount.1 + VERY_LOW_GAS),
            CompactPush::Shl { shift: shift as u8 },
        );
    }

    best
}

pub(super) fn selected_len(gcx: Gcx<'_>, value: U256) -> usize {
    select(ImmediatePolicy::of(Target::new(gcx)), value).0.0
}

pub(in crate::backend) fn immediate_materialization_len(
    evm_version: EvmVersion,
    value: U256,
) -> usize {
    select(ImmediatePolicy::Bytes(evm_version), value).0.0
}

/// Returns the byte length and gas cost of the shortest immediate materialization.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    policy_materialization_cost(ImmediatePolicy::Bytes(evm_version), value)
}

/// Returns the byte length and gas cost of the materialization `policy` selects.
pub(crate) fn policy_materialization_cost(policy: ImmediatePolicy, value: U256) -> (usize, usize) {
    let metrics = ImmediateMaterialization::with_policy(policy, value).metrics();
    (metrics.encoded_len, metrics.static_gas)
}

/// Returns the byte length and gas cost of one literal `PUSHn` of `value`.
pub(crate) fn literal_cost(evm_version: EvmVersion, value: U256) -> (usize, usize) {
    (
        fixed_push_len(evm_version, push_width(evm_version, value)),
        if value.is_zero() && evm_version.has_push0() { BASE_GAS } else { VERY_LOW_GAS },
    )
}

fn fixed_push_len(evm_version: EvmVersion, width: u8) -> usize {
    if width == 0 { zero_push_len(evm_version) } else { 1 + width as usize }
}

fn zero_push_len(evm_version: EvmVersion) -> usize {
    if evm_version.has_push0() { 1 } else { 2 }
}

fn push_width(evm_version: EvmVersion, value: U256) -> u8 {
    if value.is_zero() && !evm_version.has_push0() { 1 } else { value.byte_len() as u8 }
}

#[derive(Clone, Copy)]
enum CompactPush {
    Literal,
    FullWord,
    LowerAllOnesMask { shift: u8 },
    Not,
    Shl { shift: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_selected_immediate_materializations() {
        assert_eq!(immediate_materialization_cost(EvmVersion::Cancun, U256::MAX), (2, 5));
        assert_eq!(immediate_materialization_cost(EvmVersion::Berlin, U256::MAX), (3, 6));
        assert_eq!(
            immediate_materialization_cost(EvmVersion::Cancun, U256::MAX - U256::from(384)),
            (4, 6)
        );
        assert_eq!(immediate_materialization_cost(EvmVersion::Cancun, U256::ONE << 128), (5, 9));
        assert_eq!(
            immediate_materialization_cost(EvmVersion::Cancun, (U256::ONE << 40) - U256::ONE),
            (5, 11)
        );

        let nested = !(U256::ONE << 128usize);
        assert_eq!(immediate_materialization_cost(EvmVersion::Cancun, nested), (6, 12));
        assert_eq!(ImmediateMaterialization::new(EvmVersion::Cancun, nested).stack_peak(), 2);
        let mut ops = Vec::new();
        ImmediateMaterialization::new(EvmVersion::Cancun, nested).for_each(|op| ops.push(op));
        assert_eq!(
            ops,
            [
                ImmediateMaterializationOp::Push(U256::ONE),
                ImmediateMaterializationOp::Push(U256::from(128)),
                ImmediateMaterializationOp::Opcode(op::SHL),
                ImmediateMaterializationOp::Opcode(op::NOT),
            ]
        );
    }

    #[test]
    fn lifetime_policy_weighs_gas_against_deposit() {
        use solar_config::OptimizationMode;

        let lifetime = |runs| {
            ImmediatePolicy::of(Target::with(EvmVersion::Cancun, OptimizationMode::Gas, runs))
        };
        let size = ImmediatePolicy::of(Target::with(EvmVersion::Cancun, OptimizationMode::Size, 1));
        let u64_max = U256::from(u64::MAX);
        let u128_max = U256::from(u128::MAX);
        // At 200 runs a PUSH8 costs 12 lifetime units per run against 16 for
        // `PUSH0 NOT PUSH1 SHR`, while PUSH16 costs 20 against the same 16.
        assert_eq!(policy_materialization_cost(lifetime(200), u64_max), (9, 3));
        assert_eq!(policy_materialization_cost(lifetime(200), u128_max), (5, 11));
        assert_eq!(policy_materialization_cost(lifetime(200), U256::MAX), (2, 5));
        // Each copy pays its deposit while the executions are counted once: a third
        // copy of the mask pays for the recipe.
        assert_eq!(policy_materialization_cost(lifetime(200).with_copies(2), u64_max), (9, 3));
        assert_eq!(policy_materialization_cost(lifetime(200).with_copies(3), u64_max), (5, 11));
        // A unique left-aligned string word is a literal at 1,000 runs, a shared one is not.
        let abc = U256::from(0x61_62_63) << 232;
        assert_eq!(policy_materialization_cost(lifetime(1_000), abc), (33, 3));
        assert_eq!(policy_materialization_cost(lifetime(1_000).with_copies(2), abc), (7, 9));
        // Once runtime gas dominates the deposit every recipe yields to its literal.
        assert_eq!(policy_materialization_cost(lifetime(1_000_000), u128_max), (17, 3));
        assert_eq!(policy_materialization_cost(lifetime(1_000_000), U256::MAX), (33, 3));
        // Size and unoptimized builds keep the fewest bytes.
        assert_eq!(policy_materialization_cost(size, u64_max), (5, 11));
        assert_eq!(
            policy_materialization_cost(size, u64_max),
            immediate_materialization_cost(EvmVersion::Cancun, u64_max)
        );
    }
}
