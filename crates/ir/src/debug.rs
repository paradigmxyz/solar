//! Source debug information that EVM IR operations carry.

use smallvec::SmallVec;
use solar_interface::{Span, Symbol};

/// Maximum retained origins of a shared instruction.
///
/// Retaining several origins identifies ambiguity without allowing repeated
/// merging to grow metadata without bound. Consumers must not select an
/// arbitrary origin when more than one is present.
pub const MAX_DEBUG_SPANS: usize = 8;

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
