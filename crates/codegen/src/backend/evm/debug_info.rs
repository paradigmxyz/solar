//! Final EVM instruction locations used by source-level debug formats.

use alloy_primitives::Bytes;
use solar_interface::Span;

/// One instruction in finalized bytecode with its originating source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugInstruction {
    /// Byte offset of the opcode in the artifact.
    pub offset: usize,
    /// Raw EVM opcode byte.
    pub opcode: u8,
    /// Encoded immediate bytes, excluding the opcode.
    pub argument: Bytes,
    /// Source span associated with the instruction, when available.
    pub source_span: Option<Span>,
}
