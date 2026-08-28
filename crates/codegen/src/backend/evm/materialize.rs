//! EVM value rematerialization recipes.

use super::op;
use crate::mir::{Function, InstKind, Value, ValueId};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};

/// Returns whether a MIR value is a calling-convention-backed rematerializable leaf.
pub(super) const fn is_rematerializable_leaf(value: &Value) -> bool {
    matches!(value, Value::Immediate(_) | Value::Arg(_))
}

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
        InstKind::SlotNum => op::SLOTNUM,
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
        || rematerializable_nullary_opcode(kind).is_some()
        || matches!(kind, InstKind::CalldataLoad(_) | InstKind::InternalFrameAddr(_))
}

/// Computes values that can be rebuilt across blocks from leaves available under the active
/// calling convention.
pub(super) fn cross_block_values(
    func: &Function,
    leaf_is_available: impl Fn(ValueId) -> bool,
) -> DenseBitSet<ValueId> {
    let mut users = index_vec![SmallVec::<[ValueId; 2]>::new(); func.num_values()];
    let mut remaining = index_vec![usize::MAX; func.num_values()];

    let mut recomputable = DenseBitSet::new_empty(func.num_values());
    let mut worklist = Vec::new();
    for value in func.live_values() {
        if is_rematerializable_leaf(func.value(value))
            && leaf_is_available(value)
            && recomputable.insert(value)
        {
            worklist.push(value);
        }
    }
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        if !is_cross_block_recomputable_kind(&func.inst(inst_id).kind) {
            continue;
        }
        let operands = func.inst(inst_id).kind.operands();
        remaining[result] = operands.len();
        if operands.is_empty() && recomputable.insert(result) {
            worklist.push(result);
        }
        for operand in operands {
            users[operand].push(result);
        }
    }

    while let Some(value) = worklist.pop() {
        for &user in &users[value] {
            remaining[user] -= 1;
            if remaining[user] == 0 && recomputable.insert(user) {
                worklist.push(user);
            }
        }
    }
    recomputable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{BlockId, Immediate, ImmutableId, Instruction, MirType, Value};
    use alloy_primitives::U256;
    use solar_interface::Ident;

    #[test]
    fn rematerializes_only_stable_nullary_reads() {
        assert_eq!(
            rematerializable_nullary_opcode(&InstKind::CalldataSize),
            Some(op::CALLDATASIZE)
        );
        assert_eq!(rematerializable_nullary_opcode(&InstKind::SlotNum), Some(op::SLOTNUM));
        assert_eq!(rematerializable_nullary_opcode(&InstKind::BlockNumber), Some(op::NUMBER));
        assert_eq!(rematerializable_nullary_opcode(&InstKind::ReturnDataSize), None);
    }

    #[test]
    fn cross_block_recomputation_requires_stable_leaves() {
        let mut function = Function::new(Ident::DUMMY);
        let argument = function.alloc_param(MirType::uint256());
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (safe_inst, safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(argument, immediate),
            Some(MirType::uint256()),
        ));
        let (nested_safe_inst, nested_safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Mul(safe, argument),
            Some(MirType::uint256()),
        ));
        let (calldata_inst, calldata) = function.alloc_value_inst(Instruction::new(
            InstKind::CalldataLoad(safe),
            Some(MirType::uint256()),
        ));
        let (calldata_safe_inst, calldata_safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(calldata, immediate),
            Some(MirType::uint256()),
        ));
        let (context_inst, context) = function
            .alloc_value_inst(Instruction::new(InstKind::CallValue, Some(MirType::uint256())));
        let (immutable_inst, immutable) = function.alloc_value_inst(Instruction::new(
            InstKind::LoadImmutable(ImmutableId::from_usize(0)),
            Some(MirType::uint256()),
        ));
        let (mutable_inst, mutable) = function.alloc_value_inst(Instruction::new(
            InstKind::SLoad(immediate),
            Some(MirType::uint256()),
        ));
        let (unsafe_inst, unsafe_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(mutable, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[BlockId::ENTRY].instructions.extend([
            safe_inst,
            nested_safe_inst,
            calldata_inst,
            calldata_safe_inst,
            context_inst,
            immutable_inst,
            mutable_inst,
            unsafe_inst,
        ]);
        let recomputable = cross_block_values(&function, |_| true);
        let without_argument = cross_block_values(&function, |value| value != argument);

        assert!(recomputable.contains(safe));
        assert!(recomputable.contains(nested_safe));
        assert!(recomputable.contains(calldata));
        assert!(recomputable.contains(calldata_safe));
        assert!(recomputable.contains(context));
        assert!(!recomputable.contains(immutable));
        assert!(!recomputable.contains(mutable));
        assert!(!recomputable.contains(unsafe_value));
        assert!(!without_argument.contains(safe));
        assert!(!without_argument.contains(nested_safe));
        assert!(!without_argument.contains(calldata));
        assert!(!without_argument.contains(calldata_safe));
        assert!(without_argument.contains(context));
    }
}
