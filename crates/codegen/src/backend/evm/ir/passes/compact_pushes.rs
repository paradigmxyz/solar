//! Target-dependent selection of compact immediate materializations.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, PushValue},
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

const EVM_WORD_BYTES: usize = 32;
const EVM_WORD_BITS: usize = EVM_WORD_BYTES * 8;
const MIN_COMPACT_MASK_WIDTH: u8 = 5;
const BASE_GAS: usize = 2;
const VERY_LOW_GAS: usize = 3;

fn compact_pushes(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        if !block.instructions.iter().any(|inst| {
            immediate(inst)
                .is_some_and(|value| !matches!(select(evm_version, value), CompactPush::Literal))
        }) {
            continue;
        }
        scratch.clear();
        std::mem::swap(&mut block.instructions, &mut scratch);
        block.instructions.reserve(scratch.len());
        for inst in scratch.drain(..) {
            let Some(value) = immediate(&inst) else {
                block.instructions.push(inst);
                continue;
            };
            match select(evm_version, value) {
                CompactPush::Literal => block.instructions.push(inst),
                CompactPush::FullWord => {
                    block.instructions.push(push(U256::ZERO));
                    block.instructions.push(Instruction::opcode(op::NOT));
                    changed = true;
                }
                CompactPush::LowerAllOnesMask { shift } => {
                    block.instructions.push(push(U256::ZERO));
                    block.instructions.push(Instruction::opcode(op::NOT));
                    block.instructions.push(push(U256::from(shift)));
                    block.instructions.push(Instruction::opcode(op::SHR));
                    changed = true;
                }
                CompactPush::Not => {
                    block.instructions.push(push(!value));
                    block.instructions.push(Instruction::opcode(op::NOT));
                    changed = true;
                }
                CompactPush::Shl { shift } => {
                    block.instructions.push(push(value >> usize::from(shift)));
                    block.instructions.push(push(U256::from(shift)));
                    block.instructions.push(Instruction::opcode(op::SHL));
                    changed = true;
                }
            }
        }
    }
    changed
}

fn immediate(inst: &Instruction) -> Option<U256> {
    if !inst.is_encoded_push() || inst.deferred_push().is_some() || inst.immutable_push().is_some()
    {
        return None;
    }
    match &inst.value {
        Some(PushValue::Immediate(value)) => Some(*value),
        _ => None,
    }
}

fn push(value: U256) -> Instruction {
    Instruction::push_value(value)
}

fn select(evm_version: EvmVersion, value: U256) -> CompactPush {
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

    if width >= MIN_COMPACT_MASK_WIDTH {
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
        consider(
            fixed_push_len(evm_version, push_width(evm_version, inverted)) + 1,
            CompactPush::Not,
        );
    }

    let trailing_zero_bytes = value.trailing_zeros() / 8;
    if trailing_zero_bytes > 0 && trailing_zero_bytes < EVM_WORD_BYTES {
        let shift = trailing_zero_bytes * 8;
        let shifted = value >> shift;
        consider(
            fixed_push_len(evm_version, push_width(evm_version, shifted)) + 3,
            CompactPush::Shl { shift: shift as u8 },
        );
    }

    best.1
}

/// Returns the byte length and gas cost of the selected immediate materialization.
pub(in crate::backend::evm) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    match select(evm_version, value) {
        CompactPush::Literal => literal_cost(evm_version, value),
        CompactPush::FullWord => {
            let (zero_len, zero_gas) = literal_cost(evm_version, U256::ZERO);
            (zero_len + 1, zero_gas + VERY_LOW_GAS)
        }
        CompactPush::LowerAllOnesMask { shift } => {
            let (zero_len, zero_gas) = literal_cost(evm_version, U256::ZERO);
            let (shift_len, shift_gas) = literal_cost(evm_version, U256::from(shift));
            (zero_len + 1 + shift_len + 1, zero_gas + VERY_LOW_GAS + shift_gas + VERY_LOW_GAS)
        }
        CompactPush::Not => {
            let (inverted_len, inverted_gas) = literal_cost(evm_version, !value);
            (inverted_len + 1, inverted_gas + VERY_LOW_GAS)
        }
        CompactPush::Shl { shift } => {
            let (value_len, value_gas) = literal_cost(evm_version, value >> usize::from(shift));
            let (shift_len, shift_gas) = literal_cost(evm_version, U256::from(shift));
            (value_len + shift_len + 1, value_gas + shift_gas + VERY_LOW_GAS)
        }
    }
}

fn literal_cost(evm_version: EvmVersion, value: U256) -> (usize, usize) {
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
    }
}
