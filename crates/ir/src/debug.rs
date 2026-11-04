//! Source debug information that EVM IR operations carry.

use smallvec::SmallVec;
use solar_interface::{Span, Symbol};

/// Maximum retained origins of a shared instruction.
///
/// Retaining several origins identifies ambiguity without allowing repeated
/// merging to grow metadata without bound. Consumers select an origin from
/// several only through [`single_origin`].
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

/// A removed transfer's source checkpoint carried by an existing destination label.
/// The label must still have exactly one direct incoming reference with `transfer` as its single
/// origin, or as an explicit checkpoint identity when it has no source origin. An existing
/// function invocation's declaration also identifies an unlocated transfer. This proof is
/// rechecked after transformations; it never keeps a label alive or invents a function event.
/// Any borrowed function entry is owned by this checkpoint and disappears with its proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugEntryCheckpoint {
    /// Source operation executed on entry to the label.
    pub source: Span,
    /// Source or checkpoint identity of the transfer that exclusively reaches this label.
    pub transfer: Span,
    /// Modifier depth of the source operation.
    pub modifier_depth: u32,
    /// Function entry that is valid only while this checkpoint's exclusive-entry proof holds.
    /// Native block entries are independent and must not be stored here.
    pub function_invoke: Option<DebugFunction>,
}

/// Returns the single source origin of `spans`: the first span when they form one nesting chain
/// (every pair is nested), otherwise [`None`].
///
/// Origin lists keep insertion order and merges append, so the first span is the instruction's
/// own location, and a nested origin added to it does not change it. Spans of different source
/// files occupy disjoint position ranges, so they never nest.
pub fn single_origin(spans: &[Span]) -> Option<Span> {
    let nested = spans.iter().all(|&a| spans.iter().all(|&b| a.contains(b) || b.contains(a)));
    spans.first().copied().filter(|_| nested)
}
