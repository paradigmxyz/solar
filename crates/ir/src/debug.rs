//! Source debug information that EVM IR operations carry.

use smallvec::SmallVec;
use solar_data_structures::map::FxHashSet;
use solar_interface::{Span, Symbol};

/// Source origins associated with one machine instruction.
///
/// A shared instruction keeps every distinct origin of the code merged into it. More than one
/// origin means the instruction is ambiguous: consumers must not select one of them.
pub type DebugSpans = SmallVec<[Span; 2]>;

/// Finds the spans of a batch that a list of origins lacks.
///
/// A list that grows without bound cannot afford a scan per span. This scans while a batch is
/// small and hashes the list once the batch passes [`Self::SCAN_LIMIT`] spans, so merging many
/// origins into one list stays linear in the origins merged.
#[derive(Default)]
pub struct DistinctSpans {
    inspected: usize,
    seen: Option<FxHashSet<Span>>,
}

impl DistinctSpans {
    /// Number of spans looked up by scanning before the list is hashed.
    pub const SCAN_LIMIT: usize = 16;

    /// Returns whether `span` is missing from the list, which holds `existing`.
    ///
    /// The caller appends every span for which this returns true, so the list always equals
    /// `existing` plus the spans accepted so far.
    pub fn is_new<I: Iterator<Item = Span>>(
        &mut self,
        span: Span,
        existing: impl Fn() -> I,
    ) -> bool {
        if let Some(seen) = &mut self.seen {
            return seen.insert(span);
        }
        if existing().any(|old| old == span) {
            return false;
        }
        self.inspected += 1;
        if self.inspected > Self::SCAN_LIMIT {
            self.seen = Some(existing().chain([span]).collect());
        }
        true
    }
}

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
/// The label must still have exactly one direct incoming reference with `transfer` as its sole
/// source origin, or as an explicit checkpoint identity when it has no source origin. An existing
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
