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
/// small and hashes the list once the batch passes [`Self::SCAN_LIMIT`] lookups, duplicates
/// included, so merging many origins into one list stays linear in the origins merged.
#[derive(Default)]
pub struct DistinctSpans {
    inspected: usize,
    seen: Option<FxHashSet<Span>>,
}

impl DistinctSpans {
    /// Number of lookups answered by scanning before the list is hashed.
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
        if self.seen.is_none() {
            self.inspected += 1;
            if self.inspected > Self::SCAN_LIMIT {
                self.seen = Some(existing().collect());
            }
        }
        if let Some(seen) = &mut self.seen {
            return seen.insert(span);
        }
        !existing().any(|old| old == span)
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

/// Why an instruction or terminator has the source origins it has, or none.
///
/// This copies LLVM's `DebugLoc::Kind` (`llvm/include/llvm/IR/DebugLoc.h` L51-L73). Only
/// [`Self::Normal`] without any origin is a gap: LLVM's `not-generate` check reports exactly those
/// (`Debugify.cpp` `checkInstructionCoverage`, `hasLoc`: `Loc || Kind != DebugLocKind::Normal`).
/// A kind never changes how code is generated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DebugLocKind {
    /// The origins say everything: a source location, or a missing one that nobody explained.
    #[default]
    Normal,
    /// The origins were intentionally removed under the rules for dropping a location (LLVM
    /// `Instruction::dropLocation`).
    Dropped,
    /// The code belongs to no source construct (LLVM `DebugLoc::getCompilerGenerated`).
    CompilerGenerated,
    /// The location is unknown or ambiguous in a way that cannot be represented yet (LLVM
    /// `DebugLoc::getUnknown`). The audit also sets it on a gap it reported, so that later passes
    /// are not blamed for it.
    Unknown,
    /// Code that is not expected to be emitted (LLVM `DebugLoc::getTemporary`).
    Temporary,
}

impl DebugLocKind {
    /// Returns the keyword that spells this kind inside `!metadata(...)`, or `None` for
    /// [`Self::Normal`].
    #[must_use]
    pub const fn keyword(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Dropped => Some("dropped"),
            Self::CompilerGenerated => Some("compiler_generated"),
            Self::Unknown => Some("unknown"),
            Self::Temporary => Some("temporary"),
        }
    }

    /// Returns the kind of the location left by merging two locations that both have no origin.
    /// Equal kinds stay, and a gap on either side stays visible, as with LLVM's
    /// `getMergedLocation`. Two different explanations leave `Unknown`, LLVM's "ambiguous in a way
    /// that cannot be represented". The result never depends on whether a gap was already
    /// reported, since `Normal` and `Unknown` are not printed: the audit must not change output.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Normal, _) | (_, Self::Normal) => Self::Normal,
            (a, b) if a as u8 == b as u8 => a,
            _ => Self::Unknown,
        }
    }
}
