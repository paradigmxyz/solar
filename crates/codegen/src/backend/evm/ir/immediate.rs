//! Compact literal construction for physical EVM IR.
//!
//! Candidates use a literal PUSH, its bitwise complement, a shifted short word,
//! or a shifted complement. These bounded constructions cover word masks and
//! aligned constants without recursive search. We minimize encoded bytes, then
//! static gas, and never choose an unavailable shift instruction. Assembly still
//! encodes the chosen literal widths without performing these rewrites.

use super::{InstKind, Instruction};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_config::EvmVersion;

pub(super) fn materialize(version: EvmVersion, value: U256) -> Vec<Instruction> {
    materialize_bounded(version, value, 2)
}

/// Builds a literal without using more temporary words than the caller proved available.
pub(super) fn materialize_bounded(
    version: EvmVersion,
    value: U256,
    max_extra: usize,
) -> Vec<Instruction> {
    let mut best = vec![InstKind::Push(value).into()];
    let mut consider = |candidate: Vec<Instruction>| {
        if cost(version, &candidate) < cost(version, &best) {
            best = candidate;
        }
    };
    // push ~value
    // not
    consider(vec![InstKind::Push(!value).into(), InstKind::Op(op::NOT).into()]);
    if max_extra >= 2 && version.has_bitwise_shifting() && !value.is_zero() {
        let trailing = value.trailing_zeros();
        if trailing > 0 {
            // push value >> trailing
            // push trailing
            // shl
            consider(vec![
                InstKind::Push(value >> trailing).into(),
                InstKind::Push(U256::from(trailing)).into(),
                InstKind::Op(op::SHL).into(),
            ]);
        }
        let leading = value.leading_zeros();
        if leading > 0 {
            // push ~(value << leading | low_mask)
            // not
            // push leading
            // shr
            let low_mask = (U256::ONE << leading) - U256::ONE;
            consider(vec![
                InstKind::Push(!((value << leading) | low_mask)).into(),
                InstKind::Op(op::NOT).into(),
                InstKind::Push(U256::from(leading)).into(),
                InstKind::Op(op::SHR).into(),
            ]);
        }
    }
    best
}

pub(super) fn cost(version: EvmVersion, instructions: &[Instruction]) -> (usize, usize) {
    instructions.iter().fold((0, 0), |(bytes, gas), inst| {
        let (size, price) = match inst.kind {
            InstKind::Push(value) => (
                op::push_len(version, value),
                if value.is_zero() && version.has_push0() { 2 } else { 3 },
            ),
            InstKind::Dup(depth) | InstKind::Swap(depth) => (if depth <= 16 { 1 } else { 2 }, 3),
            InstKind::Exchange(..) => {
                if version.has_extended_stack_ops() {
                    (2, 3)
                } else {
                    (3, 9)
                }
            }
            InstKind::Op(op::POP) => (1, 2),
            _ => (1, 3),
        };
        (bytes + size, gas + price)
    })
}
