//! Source debug information that MIR and EVM IR instructions carry.

use smallvec::SmallVec;
use solar_data_structures::map::FxHashSet;
use solar_interface::{Span, Symbol};

/// Source origins associated with one machine instruction.
///
/// A shared instruction keeps every distinct origin of the code merged into it. More than one
/// origin means the instruction is ambiguous: consumers must not select one of them.
pub type DebugSpans = SmallVec<[Span; 2]>;

/// Source-language identity of a function activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugFunction {
    /// Function identifier in the source language.
    pub identifier: Symbol,
    /// Source range of the complete declaration.
    pub declaration: Span,
    /// Whether the dispatcher or the deployment code enters the function, instead of a call from
    /// other code.
    pub dispatched: bool,
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
    /// Whether every origin holds for the instruction, as `gather` in ETHDebug, instead of one of
    /// them, as `pick`. Only meaningful with several origins.
    all_spans: bool,
}

impl DebugLoc {
    /// A location without origins, at modifier depth 0.
    pub const EMPTY: Self = Self {
        span: Span::DUMMY,
        additional_spans: None,
        modifier_depth: 0,
        kind: DebugLocKind::Normal,
        all_spans: false,
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

    /// Returns whether several origins all hold for the instruction, which [`Self::combine`] sets,
    /// instead of one of them holding on each path, as after [`Self::merge`].
    #[must_use]
    pub fn all_spans(&self) -> bool {
        self.all_spans
    }

    /// Sets whether the origins all hold, for text that spells them. Without several origins the
    /// flag stays clear.
    pub fn set_all_spans(&mut self, all_spans: bool) {
        self.all_spans = all_spans && self.additional_spans.is_some();
    }

    /// Replaces the origins by `span` and resets the kind to [`DebugLocKind::Normal`].
    pub fn set_span(&mut self, span: Option<Span>) {
        self.span = span.filter(|span| !span.is_dummy()).unwrap_or(Span::DUMMY);
        self.additional_spans = None;
        self.all_spans = false;
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

    /// Rule R2, merge: the code of equivalent instructions `others`, which run on different paths,
    /// is now this one, like LLVM's `DILocation::getMergedLocation`, but keeping every origin
    /// instead of collapsing to line 0.
    ///
    /// If every input, this one included, has origins, the result has all of them. If any input
    /// has none, the result has none and its kind is [`DebugLocKind::Unknown`]: the shared code
    /// also runs for the path that has no origin, and the origins of the other paths would claim
    /// a source for it. Without any origin on any input the kind is [`DebugLocKind::merge`] of
    /// the kinds.
    ///
    /// The depth stays when it is the same on every input, this one included, and becomes 0
    /// otherwise.
    ///
    /// NOTE: Callers must not select one of the origins, such as with a source-map `pick` or a
    /// single span: that claims one path's source on every path.
    ///
    /// NOTE: Merging an input whose origins all hold (see [`Self::combine`]) keeps the union as
    /// plain alternatives and clears the flag. A `pick` of `gather`s would be exact, but nothing
    /// produces one today.
    pub fn merge<'a>(&mut self, others: impl IntoIterator<Item = &'a Self>) {
        self.all_spans = false;
        let mut located = self.has_spans();
        let mut unlocated_kind = (!located).then_some(self.kind);
        for other in others {
            self.vote_depth(other);
            if other.has_spans() {
                located = true;
                if unlocated_kind.is_none() {
                    self.insert_spans(other.spans());
                }
            } else {
                let kind = unlocated_kind.map_or(other.kind, |kind| kind.merge(other.kind));
                unlocated_kind = Some(kind);
            }
        }
        if let Some(kind) = unlocated_kind {
            self.set_span(None);
            self.kind = if located { DebugLocKind::Unknown } else { kind };
        }
    }

    /// Rule R2b, combine: one instruction now does the work of the instructions `others`, which
    /// all run on one path, such as several stores folded into one copy. It keeps the union of
    /// their origins, and an input without origins adds no origin.
    ///
    /// Without any origin the kind becomes [`DebugLocKind::merge`] of the kinds, and
    /// [`DebugLocKind::Normal`] with one. The depth follows the same rule as in [`Self::merge`].
    ///
    /// With several origins they all hold, so [`Self::all_spans`] is set, unless an input already
    /// has several alternatives: all of those and the rest cannot be told from one flag, so the
    /// union stays plain alternatives.
    pub fn combine<'a>(&mut self, others: impl IntoIterator<Item = &'a Self>) {
        let mut exact = !self.has_alternatives();
        for other in others {
            exact &= !other.has_alternatives();
            self.vote_depth(other);
            self.insert_spans(other.spans());
            self.kind =
                if self.has_spans() { DebugLocKind::Normal } else { self.kind.merge(other.kind) };
        }
        self.all_spans = exact && self.additional_spans.is_some();
    }

    /// Rule R3, drop: clears the origins and records why there are none, like LLVM's
    /// `Instruction::dropLocation`. The modifier depth stays.
    pub fn drop(&mut self, kind: DebugLocKind) {
        self.set_span(None);
        self.kind = kind;
    }

    /// Returns whether several origins stand for one of them holding.
    fn has_alternatives(&self) -> bool {
        self.additional_spans.is_some() && !self.all_spans
    }

    fn vote_depth(&mut self, other: &Self) {
        if self.modifier_depth != other.modifier_depth {
            self.modifier_depth = 0;
        }
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
        merged.merge([&loc([span(2)], 2), &loc([span(3)], 2)]);
        assert_eq!(merged.modifier_depth(), 2);

        merged.merge([&loc([span(4)], 1)]);
        assert_eq!(merged.modifier_depth(), 0);

        // Once the depths conflict, a later agreeing input does not bring the depth back.
        merged.set_modifier_depth(2);
        merged.merge([&loc([span(5)], 1), &loc([span(6)], 2)]);
        assert_eq!(merged.modifier_depth(), 0);
    }

    #[test]
    fn merge_votes_depth_of_inputs_without_spans() {
        let mut merged = loc([], 2);
        merged.merge([&loc([], 2)]);
        assert_eq!(merged.modifier_depth(), 2);
        merged.merge([&DebugLoc::EMPTY]);
        assert_eq!(merged.modifier_depth(), 0);
    }

    #[test]
    fn merge_with_an_unlocated_input_has_no_spans() {
        let mut merged = loc([span(1), span(2)], 0);
        merged.merge([&DebugLoc::EMPTY, &loc([span(3)], 0)]);
        assert!(!merged.has_spans());
        assert_eq!(merged.kind(), DebugLocKind::Unknown);

        let mut unlocated = DebugLoc::EMPTY;
        unlocated.merge([&loc([span(1)], 0)]);
        assert!(!unlocated.has_spans());
        assert_eq!(unlocated.kind(), DebugLocKind::Unknown);
    }

    #[test]
    fn merge_kind_does_not_depend_on_input_order() {
        let mut dropped = DebugLoc::EMPTY;
        dropped.drop(DebugLocKind::Dropped);
        let located = loc([span(1)], 0);
        for inputs in [[&dropped, &DebugLoc::EMPTY], [&DebugLoc::EMPTY, &dropped]] {
            let mut merged = located.clone();
            merged.merge(inputs);
            assert_eq!(merged.kind(), DebugLocKind::Unknown);
        }
    }

    #[test]
    fn merge_unions_the_spans_of_located_inputs() {
        let mut merged = loc([span(1)], 0);
        merged.merge([&loc([span(2), span(1)], 0)]);
        assert_eq!(merged.spans().collect::<Vec<_>>(), [span(1), span(2)]);
        assert_eq!(merged.kind(), DebugLocKind::Normal);
    }

    #[test]
    fn combine_marks_several_spans_as_all_holding() {
        let mut combined = loc([span(1)], 0);
        combined.combine([&loc([span(1)], 0)]);
        assert!(!combined.all_spans());

        combined.combine([&loc([span(2)], 0)]);
        assert!(combined.all_spans());
        assert_eq!(combined.spans().collect::<Vec<_>>(), [span(1), span(2)]);

        let mut preserved = DebugLoc::EMPTY;
        preserved.preserve(&combined);
        assert!(preserved.all_spans());

        let mut replaced = combined.clone();
        replaced.set_span(Some(span(3)));
        assert!(!replaced.all_spans());
    }

    #[test]
    fn combine_over_alternatives_stays_alternatives() {
        let alternatives = loc([span(1), span(2)], 0);
        let mut combined = loc([span(3)], 0);
        combined.combine([&alternatives]);
        assert!(!combined.all_spans());
        assert_eq!(combined.spans().count(), 3);
    }

    #[test]
    fn merge_clears_the_all_flag_and_keeps_the_union() {
        let mut all = loc([span(1)], 0);
        all.combine([&loc([span(2)], 0)]);
        assert!(all.all_spans());

        let mut merged = loc([span(3)], 0);
        merged.merge([&all]);
        assert!(!merged.all_spans());
        assert_eq!(merged.spans().collect::<Vec<_>>(), [span(3), span(1), span(2)]);

        all.merge([&loc([span(4)], 0)]);
        assert!(!all.all_spans());
    }

    #[test]
    fn combine_unions_spans_and_ignores_unlocated_inputs() {
        let mut combined = loc([span(1)], 2);
        combined.combine([&DebugLoc::EMPTY.clone(), &loc([span(2)], 2)]);
        assert_eq!(combined.spans().collect::<Vec<_>>(), [span(1), span(2)]);
        assert_eq!(combined.kind(), DebugLocKind::Normal);
        assert_eq!(combined.modifier_depth(), 0);
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
        merged.combine([&loc([span(1)], 0)]);
        assert_eq!(merged.kind(), DebugLocKind::Normal);
    }
}
