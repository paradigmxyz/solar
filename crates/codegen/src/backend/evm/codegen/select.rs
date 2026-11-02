//! Single-opcode selection for MIR operations.
//!
//! Most MIR operations lower to exactly one EVM opcode whose stack contract
//! follows the operation's operand list. The local rules in `isle/mir-to-evm/select.isle`
//! select that opcode and its scheduling shape. The target cost model uses
//! the same selector. Operations requiring attributes, control flow, or
//! multiple opcodes remain in the Rust emitter; version legalization and
//! physical stack scheduling are separate from selection.

use crate::{
    backend::evm::op::*,
    mir::{EffectKind, Function, InstKind, LogTopics, Op, OpTraits, PtrLocation, Value, ValueId},
};

/// Stack shape of a MIR operation that lowers to one EVM opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpcodeLowering {
    /// No operands; the opcode pushes the result.
    Nullary { opcode: u8 },
    /// One operand; the opcode pushes the result.
    Unary { opcode: u8 },
    /// Two operands in operand order; the opcode pushes the result.
    Binary { opcode: u8 },
    /// An address and a value; the opcode pushes nothing.
    Store { opcode: u8 },
    /// Every operand pushed last to first; the opcode pushes the result.
    Nary { opcode: u8 },
    /// Every operand pushed last to first; the opcode copies into memory.
    MemoryCopy { opcode: u8 },
    /// Every operand pushed last to first; the opcode emits a log.
    Log { opcode: u8 },
}

impl OpcodeLowering {
    /// The selected opcode.
    pub(crate) fn opcode(self) -> u8 {
        match self {
            Self::Nullary { opcode }
            | Self::Unary { opcode }
            | Self::Binary { opcode }
            | Self::Store { opcode }
            | Self::Nary { opcode }
            | Self::MemoryCopy { opcode }
            | Self::Log { opcode } => opcode,
        }
    }
}

#[allow(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    rust_2018_idioms,
    unnameable_types,
    unreachable_code,
    unreachable_pub,
    unused_imports,
    unused_mut,
    unused_variables
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/select.isle.rs"));
}

struct RuleContext;

impl generated::Context for RuleContext {
    fn log_opcode(&mut self, topics: LogTopics) -> u8 {
        LOG0 + topics.len() as u8
    }

    fn load_opcode(&mut self, location: PtrLocation) -> Option<u8> {
        match location {
            PtrLocation::Memory => Some(MLOAD),
            PtrLocation::Calldata => Some(CALLDATALOAD),
            PtrLocation::Returndata => None,
            PtrLocation::Storage => Some(SLOAD),
            PtrLocation::Transient => Some(TLOAD),
        }
    }

    fn store_opcode(&mut self, location: PtrLocation) -> Option<u8> {
        match location {
            PtrLocation::Memory => Some(MSTORE),
            PtrLocation::Storage => Some(SSTORE),
            PtrLocation::Transient => Some(TSTORE),
            PtrLocation::Calldata | PtrLocation::Returndata => None,
        }
    }

    fn copy_opcode(&mut self, location: PtrLocation) -> Option<u8> {
        match location {
            PtrLocation::Memory => Some(MCOPY),
            PtrLocation::Calldata => Some(CALLDATACOPY),
            PtrLocation::Returndata => Some(RETURNDATACOPY),
            PtrLocation::Storage | PtrLocation::Transient => None,
        }
    }
}

/// Returns the single-opcode lowering of `op`, when it has one.
pub(crate) fn opcode_lowering(op: &Op) -> Option<OpcodeLowering> {
    generated::constructor_select_opcode(&mut RuleContext, op)
}

/// The EVM opcode that implements an instruction.
pub(crate) trait InstKindOpcode {
    /// Returns the EVM opcode that directly implements this instruction.
    fn evm_opcode(&self) -> Option<u8>;
}

impl InstKindOpcode for InstKind {
    fn evm_opcode(&self) -> Option<u8> {
        opcode_lowering(&self.op()).map(OpcodeLowering::opcode)
    }
}

/// Returns the opcode of a stable nullary read, possibly zero-extended, that is cheaper to read
/// again than to keep on the stack.
pub(crate) fn rematerializable_nullary_value(func: &Function, value: ValueId) -> Option<u8> {
    let Value::Inst(inst) = *func.value(value) else { return None };
    let kind = &func.inst(inst).kind;
    if let InstKind::Zext(inner) = *kind {
        return rematerializable_nullary_value(func, inner);
    }
    // The rematerializable nullary reads are environment reads; pure rematerializable
    // operations are arithmetic, which never lowers to a nullary opcode.
    let def = kind.op_def();
    if def.traits.contains(OpTraits::REMATERIALIZABLE)
        && def.effect != EffectKind::Pure
        && let Some(OpcodeLowering::Nullary { opcode }) = opcode_lowering(&kind.op())
    {
        Some(opcode)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::evm::op, mir::Semantics};

    #[test]
    fn selection_matches_declared_opcodes() {
        for kind in crate::mir::semantics::opcode_samples() {
            let Some(Semantics::Opcode(name, _)) = kind.semantics() else { unreachable!() };
            // Calls lower outside instruction selection.
            let selected = kind.evm_opcode();
            assert!(
                selected.is_none_or(|selected| Some(selected) == op::from_mnemonic(name)),
                "{kind:?}"
            );
        }
    }
}
