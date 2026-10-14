//! Shared bounds for source-origin metadata across compiler IR layers.

use solar_interface::{Span, Symbol};

/// Maximum retained origins of a shared instruction.
///
/// Retaining several origins identifies ambiguity without allowing repeated
/// merging to grow metadata without bound. Consumers must not select an
/// arbitrary origin when more than one is present.
pub const MAX_DEBUG_SPANS: usize = 8;

/// Source-language identity of a function activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugFunction {
    /// Function identifier in the source language.
    pub identifier: Symbol,
    /// Source range of the complete declaration.
    pub declaration: Span,
}

/// One lexical inlining instance, analogous to LLVM's `inlinedAt` chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugInlineFrame {
    /// Source identity of the inlined callee.
    pub function: DebugFunction,
    /// Source location of the call that introduced this instance.
    pub call_site: Span,
    /// Distinguishes copies introduced at the same source location.
    pub instance: u32,
}
