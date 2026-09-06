//! Final instruction locations and optional source provenance for debug output.
//!
//! These records describe emitted code without influencing instruction selection,
//! scheduling, layout, or assembly. Several source spans represent a shared origin;
//! consumers must retain that ambiguity instead of selecting an arbitrary span.
//! Function events require lowering provenance and cannot be inferred from a bare
//! jump or terminal opcode.

use super::op;
use solar_config::EvmVersion;
use solar_interface::{Span, Symbol};

/// Source declaration associated with a function invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugFunction {
    /// Source-level name, or the anonymous symbol for an unnamed function.
    pub identifier: Symbol,
    /// Location of the source function declaration.
    pub declaration: Span,
}

/// A source function's terminal control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFunctionExit {
    /// Successful return from the function.
    Return,
    /// Reverting exit from the function.
    Revert,
}

/// Location and optional provenance of one final physical instruction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebugInstruction {
    /// Byte offset within the emitted code object.
    pub offset: usize,
    /// Number of present bytes, including any immediate operand.
    pub length: u8,
    /// The actual opcode byte, including unknown opcode values.
    pub opcode: u8,
    /// Known source origins; an empty collection means unknown or intentionally dropped.
    pub source_spans: Vec<Span>,
    /// Retained modifier nesting, or zero when unavailable.
    pub modifier_depth: u32,
    /// A source invocation associated with this instruction, when known.
    pub function_invoke: Option<DebugFunction>,
    /// A source function exit associated with this instruction, when known.
    pub function_exit: Option<DebugFunctionExit>,
}

/// Decodes instruction boundaries in a code-only byte sequence.
///
/// A truncated PUSH consumes only the bytes that are actually present. Unknown
/// opcode values remain explicit records rather than being assigned another opcode.
///
/// NOTE: Raw bytes do not retain source spans, modifier nesting, function events,
/// or code/data boundaries. Those fields remain unknown here; assembly provenance
/// must supply them for source-aware output. Callers must exclude embedded data.
/// Extended stack immediates follow the selected fork and preserve separate
/// instruction boundaries when the immediate encoding is invalid.
pub(crate) fn collect(bytecode: &[u8], version: EvmVersion) -> Vec<DebugInstruction> {
    let mut instructions = Vec::new();
    let mut offset = 0;
    while let Some(&opcode) = bytecode.get(offset) {
        let immediate = if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            usize::from(opcode - op::PUSH0)
        } else if version.has_extended_stack_ops() {
            let byte = bytecode.get(offset + 1).copied().unwrap_or_default();
            usize::from(match opcode {
                op::DUPN | op::SWAPN => op::decode_depth(byte).is_some(),
                op::EXCHANGE => op::decode_exchange(byte).is_some(),
                _ => false,
            })
        } else {
            0
        };
        let length = 1 + immediate.min(bytecode.len() - offset - 1);
        instructions.push(DebugInstruction {
            offset,
            length: length as u8,
            opcode,
            ..Default::default()
        });
        offset += length;
    }
    instructions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_present_push_bytes_without_inventing_origins() {
        let instructions =
            collect(&[op::PUSH1, op::JUMPDEST, op::PUSH0, op::PUSH32, 0xff], EvmVersion::Osaka);
        assert_eq!(
            instructions
                .iter()
                .map(|inst| (inst.offset, inst.length, inst.opcode))
                .collect::<Vec<_>>(),
            [(0, 2, op::PUSH1), (2, 1, op::PUSH0), (3, 2, op::PUSH32)],
        );
        assert!(instructions.iter().all(|inst| inst.source_spans.is_empty()
            && inst.function_invoke.is_none()
            && inst.function_exit.is_none()));
        assert!(collect(&[], EvmVersion::Osaka).is_empty());
        assert_eq!(collect(&[0x0c], EvmVersion::Osaka)[0].opcode, 0x0c);
    }

    #[test]
    fn extended_immediates_use_fork_and_validity() {
        let bytes = [op::DUPN, 0x5b, op::DUPN, 0x80, op::EXCHANGE, 0x80];
        let offsets =
            |version| collect(&bytes, version).iter().map(|inst| inst.offset).collect::<Vec<_>>();
        assert_eq!(offsets(EvmVersion::Osaka), [0, 1, 2, 3, 4, 5]);
        assert_eq!(offsets(EvmVersion::Amsterdam), [0, 1, 2, 4]);
        assert_eq!(collect(&[op::DUPN], EvmVersion::Amsterdam)[0].length, 1);
    }
}
