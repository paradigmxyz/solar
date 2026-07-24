use crate::{Lint, LintContext, LintPolicy};
use solar_ast as ast;
use solar_interface::{Session, Span, diagnostics::DiagMsg, source_map::SourceFile};
use solar_sema::Gcx;
use std::{path::PathBuf, sync::Arc};

/// A project-owned source visible to a project-wide lint pass.
pub struct ProjectSource<'ast> {
    /// Canonical path used to select this source.
    pub path: PathBuf,
    /// Source map entry for this source.
    pub file: Arc<SourceFile>,
    /// Parsed source unit.
    pub ast: &'ast ast::SourceUnit<'ast>,
    /// Host policy applied when a project-wide pass emits against this source.
    pub policy: Arc<dyn LintPolicy>,
}

/// A lint pass that inspects all project-owned sources together.
pub trait ProjectLintPass<'ast>: Send + Sync {
    fn check_project(&mut self, ctx: &ProjectLintContext<'_, '_>, sources: &[ProjectSource<'ast>]);
}

/// Context supplied to project-wide lint passes.
pub struct ProjectLintContext<'s, 'gcx> {
    sess: &'s Session,
    gcx: Gcx<'gcx>,
    policy: Arc<dyn LintPolicy>,
    with_description: bool,
    with_ansi_help: bool,
}

impl<'s, 'gcx> ProjectLintContext<'s, 'gcx> {
    /// Creates a project-wide lint context.
    pub fn new(
        sess: &'s Session,
        gcx: Gcx<'gcx>,
        policy: Arc<dyn LintPolicy>,
        with_description: bool,
        with_ansi_help: bool,
    ) -> Self {
        Self { sess, gcx, policy, with_description, with_ansi_help }
    }

    /// Returns the fully analyzed compiler context.
    pub const fn gcx(&self) -> Gcx<'gcx> {
        self.gcx
    }

    /// Returns whether a lint is active for this project run.
    pub fn is_lint_enabled(&self, id: &str) -> bool {
        self.policy.is_lint_enabled(id)
    }

    /// Emits a lint's default diagnostic.
    pub fn emit<L: Lint>(&self, source: &ProjectSource<'_>, lint: &'static L, span: Span) {
        self.source_context(source).emit(lint, span);
    }

    /// Emits a lint diagnostic with a caller-provided message.
    pub fn emit_with_msg<L: Lint>(
        &self,
        source: &ProjectSource<'_>,
        lint: &'static L,
        span: Span,
        msg: impl Into<DiagMsg>,
    ) {
        self.source_context(source).emit_with_msg(lint, span, msg);
    }

    fn source_context<'a>(&self, source: &'a ProjectSource<'_>) -> LintContext<'s, 'a>
    where
        's: 'a,
    {
        LintContext::new(
            self.sess,
            source.policy.as_ref(),
            self.with_description,
            self.with_ansi_help,
            Some(source.file.clone()),
        )
    }
}
