//! Target-dependent selection of compact immediate materializations.
//!
//! A literal `PUSHn` is not always the shortest way to construct a 256-bit constant. For each
//! concrete immediate, this pass compares the literal encoding with a fixed set of equivalent
//! recipes: `PUSH0; NOT` for an all-ones word, `NOT` of a shorter inverse, and shift-based forms
//! for masks or values with trailing zero bytes. It then emits the recipe with the fewest encoded
//! bytes, keeping the literal on ties so the pass never increases code size. In size builds
//! and cold blocks, division of the all-ones word and multiplication also synthesize repeated
//! byte patterns. Gas-mode hot blocks retain the existing shift and inverse recipes.
//!
//! Selection accounts for the active EVM version: `PUSH0` and shift opcodes are used only when the
//! target supports them. The exported cost helper uses the common recipes, so other EVM IR
//! passes can compare a prospective rewrite with their bytes and static gas. NOTE: the
//! size-only repeated-pattern recipes are emitted here; the common cost helper conservatively
//! retains the larger estimate for those constants.
//!
//! Recipe emission recursively selects materializations for child pushes, so one pass reaches a
//! fixed point. The default pipeline expands recipes once before structural cleanup because tail
//! merging and outlining profit from the concrete instruction shape. Push reordering and
//! assembly-only lowering use the same recipe API for constants they inspect or introduce later.

use super::EvmPass;
use crate::{
    backend::evm::{
        ir::{Instruction, Metadata, Module},
        op::{self, WORD_BYTES},
    },
    target::GasTier,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
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
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        let repeated = gcx.sess.opts.optimization.is_size() || block.metadata.hotness.is_cold();
        if !block.instructions.iter().any(|inst| {
            inst.concrete_immediate().is_some_and(|value| {
                !matches!(select_recipe(evm_version, value, repeated).1, CompactPush::Literal)
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
            let materialization =
                ImmediateMaterialization::with_repeated(evm_version, value, repeated);
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

fn push(value: U256) -> Instruction {
    Instruction::push_value(value)
}

/// One instruction in a selected immediate materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend) enum ImmediateMaterializationOp {
    Push(U256),
    Opcode(u8),
}

/// The shortest selected materialization for one concrete immediate.
#[derive(Clone, Copy)]
pub(in crate::backend) struct ImmediateMaterialization {
    evm_version: EvmVersion,
    value: U256,
    recipe: CompactPush,
    repeated: bool,
}

impl ImmediateMaterialization {
    /// Returns the materialization selected for `value` on `evm_version`.
    pub(in crate::backend) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self::with_repeated(evm_version, value, false)
    }

    fn with_repeated(evm_version: EvmVersion, value: U256, repeated: bool) -> Self {
        Self { evm_version, value, repeated, recipe: select_recipe(evm_version, value, repeated).1 }
    }

    fn child(self, value: U256) -> Self {
        Self::with_repeated(self.evm_version, value, self.repeated)
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
        match self.recipe {
            CompactPush::Literal => f(push(self.value)),
            CompactPush::FullWord => {
                self.child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::LowerAllOnesMask { shift } => {
                self.child(U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
                self.child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHR));
            }
            CompactPush::Not => {
                self.child(!self.value).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::Shl { shift } => {
                self.child(self.value >> usize::from(shift)).for_each_inner(f);
                self.child(U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHL));
            }
            // divisor; MAX; DIV
            CompactPush::AllOnesDiv { divisor } => {
                self.child(divisor).for_each_inner(f);
                self.child(U256::MAX).for_each_inner(f);
                f(opcode(op::DIV));
            }
            // factor; divisor; MAX; DIV; MUL
            CompactPush::Repeat { factor, divisor } => {
                self.child(factor).for_each_inner(f);
                self.child(divisor).for_each_inner(f);
                self.child(U256::MAX).for_each_inner(f);
                f(opcode(op::DIV));
                f(opcode(op::MUL));
            }
        }
    }

    fn metrics(self) -> ImmediateMaterializationMetrics {
        // A literal is one push, which the visitor below would measure the same way.
        if matches!(self.recipe, CompactPush::Literal) {
            let (encoded_len, static_gas) = literal_cost(self.evm_version, self.value);
            return ImmediateMaterializationMetrics { encoded_len, static_gas, stack_peak: 1 };
        }
        let mut metrics = ImmediateMaterializationMetrics::default();
        let mut depth = 0usize;
        self.for_each(|materialized| match materialized {
            ImmediateMaterializationOp::Push(value) => {
                let (len, gas) = literal_cost(self.evm_version, value);
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
                    op::DIV | op::MUL => GasTier::Low.fixed_gas() as usize,
                    _ => unreachable!("unexpected compact immediate opcode"),
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
    evm_version: EvmVersion,
    value: U256,
) {
    materialize_selected(
        instructions,
        ImmediateMaterialization::new(evm_version, value),
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

pub(super) fn selected_len(gcx: Gcx<'_>, value: U256) -> usize {
    immediate_materialization_len(gcx.sess.opts.evm_version, value)
}

pub(in crate::backend) fn immediate_materialization_len(
    evm_version: EvmVersion,
    value: U256,
) -> usize {
    select_with_len(evm_version, value).0
}

fn select_with_len(evm_version: EvmVersion, value: U256) -> (usize, CompactPush) {
    select_recipe(evm_version, value, false)
}

fn select_recipe(evm_version: EvmVersion, value: U256, repeated: bool) -> (usize, CompactPush) {
    let width = push_width(evm_version, value);
    let normal_len = fixed_push_len(evm_version, width);
    // NOT recipes require a full-width input. A shifted nonzero literal needs
    // at least two PUSH1s and SHL (five bytes), so PUSH4 and shorter already win
    // or tie every recipe. Keep the literal on ties, as the full search does.
    if width < MIN_COMPACT_MASK_WIDTH {
        return (normal_len, CompactPush::Literal);
    }
    let mut best = (normal_len, CompactPush::Literal);
    let mut consider = |len, compact| {
        if len < best.0 {
            best = (len, compact);
        }
    };

    if value == U256::MAX {
        consider(zero_push_len(evm_version) + 1, CompactPush::FullWord);
    }

    if evm_version.has_bitwise_shifting() && width >= MIN_COMPACT_MASK_WIDTH {
        let bytes = value.to_be_bytes::<WORD_BYTES>();
        let start = WORD_BYTES - width as usize;
        if bytes[start..].iter().all(|&byte| byte == 0xff) {
            let shift = EVM_WORD_BITS - usize::from(width) * 8;
            consider(
                zero_push_len(evm_version) + 4,
                CompactPush::LowerAllOnesMask { shift: shift as u8 },
            );
        }
    }

    if width as usize == WORD_BYTES {
        let inverted = !value;
        if push_width(evm_version, inverted) < width {
            consider(select_recipe(evm_version, inverted, repeated).0 + 1, CompactPush::Not);
        }
    }

    // Restrict divisions and multiplications to size-first code. Every child is narrower,
    // so recursive selection terminates, and strict byte savings are required.
    let child_len = |word| select_recipe(evm_version, word, repeated).0;
    let narrower = |word| push_width(evm_version, word) < width;
    if repeated && value < U256::MAX {
        let full = child_len(U256::MAX);
        let divisor = U256::MAX / value;
        if (U256::MAX % value).is_zero() && narrower(divisor) {
            consider(child_len(divisor) + full + 1, CompactPush::AllOnesDiv { divisor });
        }
        for period in [1usize, 2, 4, 8, 16] {
            let divisor = (U256::ONE << (period * 8)) - U256::ONE;
            let ones = U256::MAX / divisor;
            if !(value % ones).is_zero() {
                continue;
            }
            let factor = value / ones;
            if factor > U256::ONE && narrower(factor) && narrower(divisor) {
                consider(
                    child_len(factor) + child_len(divisor) + full + 2,
                    CompactPush::Repeat { factor, divisor },
                );
            }
        }
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if evm_version.has_bitwise_shifting()
        && trailing_zero_bytes > 0
        && trailing_zero_bytes < WORD_BYTES
    {
        let shift = trailing_zero_bytes * 8;
        let shifted = value >> shift;
        consider(
            select_recipe(evm_version, shifted, repeated).0
                + select_recipe(evm_version, U256::from(shift), repeated).0
                + 1,
            CompactPush::Shl { shift: shift as u8 },
        );
    }

    best
}

/// Returns the byte length and gas cost of the selected immediate materialization.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    let metrics = ImmediateMaterialization::new(evm_version, value).metrics();
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
    AllOnesDiv { divisor: U256 },
    Repeat { factor: U256, divisor: U256 },
    Literal,
    FullWord,
    LowerAllOnesMask { shift: u8 },
    Not,
    Shl { shift: u8 },
}
