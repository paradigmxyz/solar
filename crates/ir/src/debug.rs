//! Source debug information that MIR and EVM IR instructions carry.

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

/// The source location of one instruction: its origins, its modifier depth, and why it has no
/// origin when it has none. MIR and EVM IR instructions embed it, so each rule below is written
/// once.
///
/// The three rules copy LLVM's `setDebugLoc`, `DILocation::getMergedLocation` and
/// `Instruction::dropLocation`, and solc's `AssemblyItem(op, x.debugData())`.
///
/// The modifier depth says where the code sits in the nesting of modifiers, not which source it
/// came from. Like solc, which stamps it on every item whether or not the item has a location
/// (`libevmasm/Assembly.cpp` L112-L114 at v0.8.38), it stays true when the origins are gone. So
/// [`Self::drop`] keeps it, and [`Self::merge`] keeps it when every input has the same depth,
/// spanless inputs included, and sets 0 when they differ.
///
/// Origins use a compact layout, because MIR instructions are numerous: the first one is inline
/// and the rest are boxed, so an instruction with at most one origin does not allocate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DebugLoc {
    /// First origin, or [`Span::DUMMY`] if none.
    span: Span,
    /// Origins after the first, allocated only after a merge.
    additional_spans: Option<Box<AdditionalSpans>>,
    /// Legacy source-map modifier nesting depth.
    modifier_depth: u32,
    /// Why there is no origin, when there is none.
    kind: DebugLocKind,
}

impl DebugLoc {
    /// A location without origins, at modifier depth 0.
    pub const EMPTY: Self = Self {
        span: Span::DUMMY,
        additional_spans: None,
        modifier_depth: 0,
        kind: DebugLocKind::Normal,
    };

    /// Returns the first origin.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        (!self.span.is_dummy()).then_some(self.span)
    }

    /// Returns the only origin, or `None` if there are none or several.
    #[must_use]
    pub fn only_span(&self) -> Option<Span> {
        if self.additional_spans.is_some() { None } else { self.span() }
    }

    /// Returns every origin, in merge order.
    pub fn spans(&self) -> impl Iterator<Item = Span> + Clone + '_ {
        self.span()
            .into_iter()
            .chain(self.additional_spans.iter().flat_map(|spans| spans.spans.iter().copied()))
    }

    /// Returns whether there is at least one origin.
    #[must_use]
    pub fn has_spans(&self) -> bool {
        !self.span.is_dummy()
    }

    /// Replaces the origins by `span` and resets the kind to [`DebugLocKind::Normal`].
    pub fn set_span(&mut self, span: Option<Span>) {
        self.span = span.filter(|span| !span.is_dummy()).unwrap_or(Span::DUMMY);
        self.additional_spans = None;
        self.kind = DebugLocKind::Normal;
    }

    /// Replaces the origins by `spans`, without duplicates, and resets the kind to
    /// [`DebugLocKind::Normal`].
    pub fn set_spans(&mut self, spans: impl IntoIterator<Item = Span>) {
        self.set_span(None);
        self.insert_spans(spans);
    }

    /// Returns the modifier depth.
    #[must_use]
    pub const fn modifier_depth(&self) -> u32 {
        self.modifier_depth
    }

    /// Sets the modifier depth.
    pub fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
    }

    /// Returns why there is no origin, when there is none.
    #[must_use]
    pub const fn kind(&self) -> DebugLocKind {
        self.kind
    }

    /// Sets why there is no origin, without touching the origins. Passes use [`Self::drop`].
    pub fn set_kind(&mut self, kind: DebugLocKind) {
        self.kind = kind;
    }

    /// Returns whether there is neither an origin nor a reason for lacking one, which is what
    /// LLVM's `not-generate` check reports.
    #[must_use]
    pub fn is_gap(&self) -> bool {
        self.kind == DebugLocKind::Normal && !self.has_spans()
    }

    /// Rule R1, preserve: copies the whole location of the one instruction this one stands for,
    /// like LLVM's `setDebugLoc(I->getDebugLoc())` and solc's `AssemblyItem(op, x.debugData())`.
    pub fn preserve(&mut self, from: &Self) {
        self.clone_from(from);
    }

    /// Rule R2, merge: adds the origins of the equivalent instructions `others` that this one now
    /// stands for, like LLVM's `DILocation::getMergedLocation`, but keeping every origin instead
    /// of collapsing to line 0.
    ///
    /// The depth stays when it is the same on every input, this one included, and becomes 0
    /// otherwise. Without any origin the kind becomes [`DebugLocKind::merge`] of the kinds, and
    /// [`DebugLocKind::Normal`] with one.
    pub fn merge<'a>(&mut self, others: impl IntoIterator<Item = &'a Self>) {
        for other in others {
            self.insert_spans(other.spans());
            if self.modifier_depth != other.modifier_depth {
                self.modifier_depth = 0;
            }
            self.kind =
                if self.has_spans() { DebugLocKind::Normal } else { self.kind.merge(other.kind) };
        }
    }

    /// Rule R3, drop: clears the origins and records why there are none, like LLVM's
    /// `Instruction::dropLocation`. The modifier depth stays.
    pub fn drop(&mut self, kind: DebugLocKind) {
        self.set_span(None);
        self.kind = kind;
    }

    fn insert_spans(&mut self, spans: impl IntoIterator<Item = Span>) {
        for span in spans {
            if span.is_dummy() || span == self.span {
                continue;
            }
            if self.span.is_dummy() {
                self.span = span;
            } else {
                self.additional_spans.get_or_insert_default().insert(span);
            }
        }
    }
}

/// The origins of shared code after the first, in merge order.
///
/// Instructions that merge many origins, such as a shared revert body, keep a hash set beside the
/// list once it passes [`Self::SCAN_LIMIT`], so merging them one origin at a time stays linear.
#[derive(Clone, Debug, Default)]
struct AdditionalSpans {
    spans: DebugSpans,
    seen: Option<FxHashSet<Span>>,
}

impl AdditionalSpans {
    const SCAN_LIMIT: usize = 16;

    /// Appends `span` unless the list has it.
    fn insert(&mut self, span: Span) {
        let is_new = match &mut self.seen {
            Some(seen) => seen.insert(span),
            None => !self.spans.contains(&span),
        };
        if !is_new {
            return;
        }
        self.spans.push(span);
        if self.seen.is_none() && self.spans.len() > Self::SCAN_LIMIT {
            self.seen = Some(self.spans.iter().copied().collect());
        }
    }
}

// The set only speeds up lookups, so two lists are equal when their origins are.
impl PartialEq for AdditionalSpans {
    fn eq(&self, other: &Self) -> bool {
        self.spans == other.spans
    }
}

impl Eq for AdditionalSpans {}

impl std::hash::Hash for AdditionalSpans {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.spans.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::BytePos;

    fn span(lo: u32) -> Span {
        Span::new(BytePos(lo), BytePos(lo + 1))
    }

    fn loc(spans: impl IntoIterator<Item = Span>, depth: u32) -> DebugLoc {
        let mut loc = DebugLoc::default();
        loc.set_spans(spans);
        loc.set_modifier_depth(depth);
        loc
    }

    #[test]
    fn preserve_copies_everything() {
        let from = loc([span(1), span(2)], 3);
        let mut to = loc([span(9)], 1);
        to.preserve(&from);
        assert_eq!(to, from);
    }

    #[test]
    fn merge_keeps_agreeing_depth_and_zeroes_conflicts() {
        let mut merged = loc([span(1)], 2);
        merged.merge([&loc([span(2)], 2), &loc([], 2)]);
        assert_eq!(merged.spans().collect::<Vec<_>>(), [span(1), span(2)]);
        assert_eq!(merged.modifier_depth(), 2);

        // A spanless input votes too.
        merged.merge([&DebugLoc::EMPTY]);
        assert_eq!(merged.modifier_depth(), 0);

        // Once the depths conflict, a later agreeing input does not bring the depth back.
        merged.set_modifier_depth(2);
        merged.merge([&loc([], 1), &loc([], 2)]);
        assert_eq!(merged.modifier_depth(), 0);
    }

    #[test]
    fn drop_keeps_depth() {
        let mut dropped = loc([span(1), span(2)], 2);
        dropped.drop(DebugLocKind::CompilerGenerated);
        assert!(!dropped.has_spans());
        assert_eq!(dropped.modifier_depth(), 2);
        assert_eq!(dropped.kind(), DebugLocKind::CompilerGenerated);
    }

    #[test]
    fn merge_without_spans_merges_kinds() {
        let mut merged = DebugLoc::EMPTY;
        merged.drop(DebugLocKind::Dropped);
        let mut other = DebugLoc::EMPTY;
        other.drop(DebugLocKind::CompilerGenerated);
        merged.merge([&other]);
        assert_eq!(merged.kind(), DebugLocKind::Unknown);
        merged.merge([&loc([span(1)], 0)]);
        assert_eq!(merged.kind(), DebugLocKind::Normal);
    }
}
