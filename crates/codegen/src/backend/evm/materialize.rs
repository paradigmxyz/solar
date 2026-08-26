//! EVM value rematerialization recipes.

use super::op;
use crate::mir::{Function, InstKind, Value, ValueId};

/// Returns the opcode for a stable nullary read that is cheaper to re-emit than preserve.
pub(super) const fn rematerializable_nullary_opcode(kind: &InstKind) -> Option<u8> {
    Some(match kind {
        InstKind::CalldataSize => op::CALLDATASIZE,
        InstKind::CodeSize => op::CODESIZE,
        InstKind::Caller => op::CALLER,
        InstKind::CallValue => op::CALLVALUE,
        InstKind::Address => op::ADDRESS,
        InstKind::Origin => op::ORIGIN,
        InstKind::GasPrice => op::GASPRICE,
        InstKind::Coinbase => op::COINBASE,
        InstKind::Timestamp => op::TIMESTAMP,
        InstKind::BlockNumber => op::NUMBER,
        InstKind::PrevRandao => op::PREVRANDAO,
        InstKind::GasLimit => op::GASLIMIT,
        InstKind::ChainId => op::CHAINID,
        InstKind::BaseFee => op::BASEFEE,
        InstKind::BlobBaseFee => op::BLOBBASEFEE,
        _ => return None,
    })
}

/// Returns whether an instruction result can be cheaply rebuilt from stable operands.
pub(super) const fn is_cheap_recomputable_kind(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::Add(_, _)
            | InstKind::Sub(_, _)
            | InstKind::Mul(_, _)
            | InstKind::And(_, _)
            | InstKind::Or(_, _)
            | InstKind::Xor(_, _)
            | InstKind::Shl(_, _)
            | InstKind::Shr(_, _)
            | InstKind::Sar(_, _)
            | InstKind::ConstructorArgsBase
    )
}

/// Returns whether an instruction result is cheap enough to rebuild from its operands.
pub(super) fn is_cheap_recomputable_value(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst_id) = func.value(value) else { return false };
    is_cheap_recomputable_kind(&func.inst(*inst_id).kind)
}

/// Returns whether an instruction result can be rebuilt across basic blocks.
pub(super) const fn is_cross_block_recomputable_kind(kind: &InstKind) -> bool {
    is_cheap_recomputable_kind(kind)
        || matches!(
            kind,
            InstKind::CallValue
                | InstKind::Caller
                | InstKind::Origin
                | InstKind::CalldataSize
                | InstKind::CalldataLoad(_)
                | InstKind::InternalFrameAddr(_)
                | InstKind::Timestamp
                | InstKind::BlockNumber
        )
}
