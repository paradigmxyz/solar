//! Final EVM instruction locations used by source-level debug formats.

use smallvec::SmallVec;
use solar_interface::{Span, Symbol};

pub use crate::source_info::MAX_DEBUG_SPANS;

/// Source origins associated with one machine instruction.
pub type DebugSpans = SmallVec<[Span; 2]>;

/// Source-language identity of a function activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugFunction {
    /// Function identifier in the source language.
    pub identifier: Symbol,
    /// Source range of the complete declaration.
    pub declaration: Span,
}

/// Function activation transition associated with an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugFunctionExit {
    /// Successful return from the active function.
    Return,
    /// Revert from the active function.
    Revert,
}

/// One instruction in finalized bytecode with its originating source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugInstruction {
    /// Byte offset of the opcode in the artifact.
    pub offset: u32,
    /// Raw EVM opcode byte.
    pub opcode: u8,
    /// Source spans associated with the instruction.
    ///
    /// More than one span means an optimization shared this instruction
    /// between multiple source-level origins.
    pub source_spans: DebugSpans,
    /// Function entered after this instruction executes.
    pub function_invoke: Option<DebugFunction>,
    /// Function activation closed after this instruction executes.
    pub function_exit: Option<DebugFunctionExit>,
    /// Legacy source-map modifier nesting depth for this instruction.
    pub modifier_depth: u32,
}
