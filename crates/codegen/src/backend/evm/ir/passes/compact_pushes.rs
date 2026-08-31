//! Target-dependent selection of compact immediate materializations.
//!
//! A literal `PUSHn` is not always the shortest way to construct a 256-bit constant. For each
//! concrete immediate, this pass compares the literal encoding with a fixed set of equivalent
//! recipes: `PUSH0; NOT` for an all-ones word, `NOT` of a shorter inverse, and shift-based forms
//! for masks or values with trailing zero bytes. It then emits the recipe with the fewest encoded
//! bytes, keeping the literal on ties so the pass never increases code size.
//!
//! Selection accounts for the active EVM version: `PUSH0` and shift opcodes are used only when the
//! target supports them. The exported cost helper uses the same selector, so other EVM IR passes
//! can compare a prospective rewrite with the bytes and static gas that this pass will emit.
//! Recipes may use more instructions and transient stack space than a literal push; passes that
//! move encoded pushes must therefore preserve their own stack-headroom proof.
//!
//! Recipe emission recursively selects materializations for child pushes, so one pass reaches a
//! fixed point. The default pipeline expands recipes once before structural cleanup because tail
//! merging and outlining profit from the concrete instruction shape. Push reordering and
//! assembly-only lowering use the same recipe API for constants they inspect or introduce later.

use super::{
    EvmPass,
    utils::{StackDepths, relative_stack_high_water},
};
use crate::backend::evm::{
    EVM_WORD_BYTES,
    ir::{BlockId, Instruction, Module},
    op,
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
const EVM_WORD_BITS: usize = EVM_WORD_BYTES * 8;
const MIN_COMPACT_MASK_WIDTH: u8 = 5;
const BASE_GAS: usize = 2;
const VERY_LOW_GAS: usize = 3;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let depths = StackDepths::new(module);
    let mut changed = false;
    let mut scratch = Vec::new();
    for index in 0..module.blocks.len() {
        let block_id = BlockId::from_usize(index);
        let block = &mut module.blocks[block_id];
        if !block.instructions.iter().any(|inst| {
            immediate(inst)
                .is_some_and(|value| !matches!(select(evm_version, value), CompactPush::Literal))
        }) {
            continue;
        }
        scratch.clear();
        std::mem::swap(&mut block.instructions, &mut scratch);
        block.instructions.reserve(scratch.len());
        let high_water = relative_stack_high_water(&scratch);
        let mut relative_depth = 0isize;
        for (index, inst) in scratch.drain(..).enumerate() {
            let Some(value) = immediate(&inst) else {
                update_relative_depth(&inst, &mut relative_depth);
                block.instructions.push(inst);
                continue;
            };
            let materialization = ImmediateMaterialization::new(evm_version, value);
            if matches!(materialization.recipe, CompactPush::Literal) {
                block.instructions.push(inst);
            } else if materialization_fits(
                inst.metadata.compact_headroom,
                depths.as_ref(),
                block_id,
                index,
                relative_depth,
                high_water,
                materialization.stack_peak(),
            ) {
                materialize_selected(&mut block.instructions, materialization);
                changed = true;
            } else {
                block.instructions.push(inst);
            }
            relative_depth += 1;
        }
    }
    changed
}

fn update_relative_depth(inst: &Instruction, depth: &mut isize) {
    if let Some(effect) = inst.effective_stack_effect() {
        *depth += isize::from(effect.outputs) - isize::from(effect.inputs);
    }
}

fn materialization_fits(
    scheduled_headroom: bool,
    depths: Option<&StackDepths>,
    block: BlockId,
    index: usize,
    relative_depth: isize,
    high_water: Option<isize>,
    peak: usize,
) -> bool {
    scheduled_headroom
        || high_water.is_some_and(|high_water| {
            relative_depth.checked_add_unsigned(peak).is_some_and(|depth| depth <= high_water)
        })
        || depths.is_some_and(|depths| depths.has_headroom(block, index, peak))
}

fn immediate(inst: &Instruction) -> Option<U256> {
    inst.concrete_immediate()
}

fn push(value: U256) -> Instruction {
    Instruction::push_value(value)
}

/// One instruction in a selected immediate materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend::evm) enum ImmediateMaterializationOp {
    Push(U256),
    Opcode(u8),
}

/// The shortest selected materialization for one concrete immediate.
#[derive(Clone, Copy)]
pub(in crate::backend::evm) struct ImmediateMaterialization {
    evm_version: EvmVersion,
    value: U256,
    recipe: CompactPush,
}

impl ImmediateMaterialization {
    /// Returns the materialization selected for `value` on `evm_version`.
    pub(in crate::backend::evm) fn new(evm_version: EvmVersion, value: U256) -> Self {
        Self { evm_version, value, recipe: select(evm_version, value) }
    }

    /// Returns the materialization's maximum relative stack height.
    pub(in crate::backend::evm) fn stack_peak(self) -> usize {
        self.metrics().stack_peak
    }

    /// Visits each concrete instruction in execution order.
    pub(in crate::backend::evm) fn for_each(self, mut f: impl FnMut(ImmediateMaterializationOp)) {
        self.for_each_inner(&mut f);
    }

    fn for_each_inner(self, f: &mut impl FnMut(ImmediateMaterializationOp)) {
        let push = ImmediateMaterializationOp::Push;
        let opcode = ImmediateMaterializationOp::Opcode;
        match self.recipe {
            CompactPush::Literal => f(push(self.value)),
            CompactPush::FullWord => {
                Self::new(self.evm_version, U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::LowerAllOnesMask { shift } => {
                Self::new(self.evm_version, U256::ZERO).for_each_inner(f);
                f(opcode(op::NOT));
                Self::new(self.evm_version, U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHR));
            }
            CompactPush::Not => {
                Self::new(self.evm_version, !self.value).for_each_inner(f);
                f(opcode(op::NOT));
            }
            CompactPush::Shl { shift } => {
                Self::new(self.evm_version, self.value >> usize::from(shift)).for_each_inner(f);
                Self::new(self.evm_version, U256::from(shift)).for_each_inner(f);
                f(opcode(op::SHL));
            }
        }
    }

    fn metrics(self) -> ImmediateMaterializationMetrics {
        let mut metrics = ImmediateMaterializationMetrics::default();
        let mut depth = 0usize;
        self.for_each(|materialized| match materialized {
            ImmediateMaterializationOp::Push(value) => {
                let (len, gas) = literal_materialization_cost(self.evm_version, value);
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
    evm_version: EvmVersion,
    value: U256,
) {
    materialize_selected(instructions, ImmediateMaterialization::new(evm_version, value));
}

fn materialize_selected(
    instructions: &mut Vec<Instruction>,
    materialization: ImmediateMaterialization,
) {
    materialization.for_each(|op| match op {
        ImmediateMaterializationOp::Push(value) => instructions.push(push(value)),
        ImmediateMaterializationOp::Opcode(opcode) => {
            instructions.push(Instruction::opcode(opcode));
        }
    });
}

fn select(evm_version: EvmVersion, value: U256) -> CompactPush {
    select_with_len(evm_version, value).1
}

pub(super) fn selected_len(gcx: Gcx<'_>, value: U256) -> usize {
    immediate_materialization_len(gcx.sess.opts.evm_version, value)
}

pub(in crate::backend::evm) fn immediate_materialization_len(
    evm_version: EvmVersion,
    value: U256,
) -> usize {
    select_with_len(evm_version, value).0
}

fn select_with_len(evm_version: EvmVersion, value: U256) -> (usize, CompactPush) {
    let width = push_width(evm_version, value);
    let normal_len = fixed_push_len(evm_version, width);
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
        let bytes = value.to_be_bytes::<EVM_WORD_BYTES>();
        let start = EVM_WORD_BYTES - width as usize;
        if bytes[start..].iter().all(|&byte| byte == 0xff) {
            let shift = EVM_WORD_BITS - usize::from(width) * 8;
            consider(
                zero_push_len(evm_version) + 4,
                CompactPush::LowerAllOnesMask { shift: shift as u8 },
            );
        }
    }

    if width as usize == EVM_WORD_BYTES {
        let inverted = !value;
        if push_width(evm_version, inverted) < width {
            consider(select_with_len(evm_version, inverted).0 + 1, CompactPush::Not);
        }
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if evm_version.has_bitwise_shifting()
        && trailing_zero_bytes > 0
        && trailing_zero_bytes < EVM_WORD_BYTES
    {
        let shift = trailing_zero_bytes * 8;
        let shifted = value >> shift;
        consider(
            select_with_len(evm_version, shifted).0
                + select_with_len(evm_version, U256::from(shift)).0
                + 1,
            CompactPush::Shl { shift: shift as u8 },
        );
    }

    best
}

/// Returns the byte length and gas cost of the selected immediate materialization.
pub(in crate::backend::evm) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    let metrics = ImmediateMaterialization::new(evm_version, value).metrics();
    (metrics.encoded_len, metrics.static_gas)
}

fn literal_materialization_cost(evm_version: EvmVersion, value: U256) -> (usize, usize) {
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
}
