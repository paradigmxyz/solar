//! Extensible lint infrastructure for Solar.
//!
//! This crate provides AST, HIR, and project-wide lint pass interfaces without
//! prescribing toolchain-specific lint selection or configuration.

mod context;
pub use context::{Lint, LintContext, LintPolicy, Suggestion, SuggestionKind};

mod early;
pub use early::{EarlyLintPass, EarlyLintVisitor};

mod late;
pub use late::{LateLintPass, LateLintVisitor};

mod project;
pub use project::{ProjectLintContext, ProjectLintPass, ProjectSource};

mod registry;
pub use registry::LintRegistry;

mod runner;
pub use runner::{LintRunContext, LintRunError, LintRunResult, LintSource, run_lints};

use std::sync::Arc;

/// A configured collection of lint passes supplied by a compiler consumer.
///
/// Implementations build their registry once and reuse it across invocations. Registered
/// factories must create fresh pass instances for every invocation.
pub trait LintSuite: Send + Sync {
    /// Returns this suite's immutable pass registry.
    fn registry(&self) -> &LintRegistry;

    /// Creates the policy applied to one target source.
    fn source_policy(&self, source: LintSource<'_, '_>) -> Arc<dyn LintPolicy>;

    /// Creates the policy used to decide which project-wide passes are active.
    ///
    /// Diagnostics emitted by those passes are governed by each [`ProjectSource`]'s source policy.
    fn project_policy(&self) -> Arc<dyn LintPolicy>;
}
