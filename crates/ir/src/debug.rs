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
