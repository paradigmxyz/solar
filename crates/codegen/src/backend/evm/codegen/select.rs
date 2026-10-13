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
    mir::{EffectKind, Function, InstKind, Op, OpTraits, Value, ValueId},
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

impl generated::Context for RuleContext {}

/// Returns the single-opcode lowering of `op`, when it has one.
pub(crate) fn opcode_lowering(op: &Op) -> Option<OpcodeLowering> {
    generated::constructor_select_opcode(&mut RuleContext, op)
}

impl InstKind {
    /// Returns the EVM opcode that directly implements this instruction.
    pub(crate) fn evm_opcode(&self) -> Option<u8> {
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
