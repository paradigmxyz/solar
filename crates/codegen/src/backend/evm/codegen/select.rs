//! Single-opcode selection for MIR operations.
//!
//! Most MIR operations lower to exactly one EVM opcode whose stack contract
//! follows the operation's operand list. The local rules in `isle/select.isle`
//! select that opcode and its scheduling shape. The target cost model uses
//! the same selector. Operations requiring attributes, control flow, or
//! multiple opcodes remain in the Rust emitter; version legalization and
//! physical stack scheduling are separate from selection.

use crate::{
    backend::evm::op::*,
    mir::{InstKind, Op},
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::ValueId;
    use std::fmt::Write as _;

    #[test]
    fn opcode_selection_matches_schema() {
        let mut output = String::new();
        for &name in InstKind::MNEMONICS {
            if let Some((arity, build)) = InstKind::operand_only(name) {
                let operands = (0..arity).map(ValueId::from_usize).collect::<Vec<_>>();
                let kind = build(&operands);
                if let Some(lowering) = opcode_lowering(&kind.op()) {
                    let definition = definition(lowering.opcode()).unwrap();
                    let (pops, pushes) = definition.stack_io.unwrap();
                    assert_eq!(usize::from(pops), arity, "{name}");
                    assert_eq!(pushes != 0, kind.op_def().result.produces_value(), "{name}");
                    writeln!(
                        output,
                        "{name}: {lowering:?}, {} ({pops} -> {pushes})",
                        definition.mnemonic
                    )
                    .unwrap();
                }
            }
        }
        snapbox::assert_data_eq!(output, snapbox::file!["select.snap"]);
    }
}
