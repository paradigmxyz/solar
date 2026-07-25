use crate::{EarlyLintPass, LateLintPass, ProjectLintPass};
use std::{marker::PhantomData, sync::Arc};

/// Factory for an AST lint pass.
pub(crate) type EarlyLintFactory =
    Arc<dyn for<'ast> Fn(PhantomData<&'ast ()>) -> Box<dyn EarlyLintPass<'ast>> + Send + Sync>;

/// Factory for a HIR lint pass.
pub(crate) type LateLintFactory =
    Arc<dyn for<'hir> Fn(PhantomData<&'hir ()>) -> Box<dyn LateLintPass<'hir>> + Send + Sync>;

/// Factory for a project-wide lint pass.
pub(crate) type ProjectLintFactory =
    Arc<dyn for<'ast> Fn(PhantomData<&'ast ()>) -> Box<dyn ProjectLintPass<'ast>> + Send + Sync>;

/// A registered lint pass and the lint IDs it may emit.
pub(crate) struct LintPassFactory<F> {
    lint_ids: &'static [&'static str],
    factory: F,
}

impl<F> LintPassFactory<F> {
    /// Creates a pass registration.
    const fn new(lint_ids: &'static [&'static str], factory: F) -> Self {
        Self { lint_ids, factory }
    }

    /// Returns the lint IDs this pass may emit.
    pub(crate) const fn lint_ids(&self) -> &'static [&'static str] {
        self.lint_ids
    }
}

impl LintPassFactory<EarlyLintFactory> {
    pub(crate) fn create_early<'ast>(&self) -> Box<dyn EarlyLintPass<'ast>> {
        (self.factory)(PhantomData)
    }
}

impl LintPassFactory<LateLintFactory> {
    pub(crate) fn create_late<'hir>(&self) -> Box<dyn LateLintPass<'hir>> {
        (self.factory)(PhantomData)
    }
}

impl LintPassFactory<ProjectLintFactory> {
    pub(crate) fn create_project<'ast>(&self) -> Box<dyn ProjectLintPass<'ast>> {
        (self.factory)(PhantomData)
    }
}

/// Reusable fresh-pass registrations supplied by a lint suite.
///
/// Passes execute in registration order. Duplicate registrations are retained and execute
/// independently.
#[derive(Default)]
pub struct LintRegistry {
    pub(crate) early: Vec<LintPassFactory<EarlyLintFactory>>,
    pub(crate) late: Vec<LintPassFactory<LateLintFactory>>,
    pub(crate) project: Vec<LintPassFactory<ProjectLintFactory>>,
}

impl LintRegistry {
    /// Creates an empty registry.
    pub const fn new() -> Self {
        Self { early: Vec::new(), late: Vec::new(), project: Vec::new() }
    }

    fn register_early(&mut self, lint_ids: &'static [&'static str], factory: EarlyLintFactory) {
        self.early.push(LintPassFactory::new(lint_ids, factory));
    }

    /// Registers a constructor for an owned AST pass.
    pub fn register_early_pass<P, F>(&mut self, lint_ids: &'static [&'static str], factory: F)
    where
        P: for<'ast> EarlyLintPass<'ast> + 'static,
        F: Fn() -> P + Send + Sync + 'static,
    {
        self.register_early(lint_ids, Arc::new(move |_| Box::new(factory())));
    }

    fn register_late(&mut self, lint_ids: &'static [&'static str], factory: LateLintFactory) {
        self.late.push(LintPassFactory::new(lint_ids, factory));
    }

    /// Registers a constructor for an owned HIR pass.
    pub fn register_late_pass<P, F>(&mut self, lint_ids: &'static [&'static str], factory: F)
    where
        P: for<'hir> LateLintPass<'hir> + 'static,
        F: Fn() -> P + Send + Sync + 'static,
    {
        self.register_late(lint_ids, Arc::new(move |_| Box::new(factory())));
    }

    fn register_project(&mut self, lint_ids: &'static [&'static str], factory: ProjectLintFactory) {
        self.project.push(LintPassFactory::new(lint_ids, factory));
    }

    /// Registers a constructor for an owned project-wide pass.
    pub fn register_project_pass<P, F>(&mut self, lint_ids: &'static [&'static str], factory: F)
    where
        P: for<'ast> ProjectLintPass<'ast> + 'static,
        F: Fn() -> P + Send + Sync + 'static,
    {
        self.register_project(lint_ids, Arc::new(move |_| Box::new(factory())));
    }
}
