use crate::{
    EarlyLintVisitor, LateLintVisitor, LintContext, LintSuite, ProjectLintContext, ProjectSource,
};
use rayon::prelude::*;
use solar_ast::{self as ast, visit::Visit as _};
use solar_interface::{Session, source_map::SourceFile};
use solar_sema::{Gcx, hir::Visit as _};
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

/// An analyzed source presented to a lint suite's policy adapter.
#[derive(Clone, Copy)]
pub struct LintSource<'a, 'ast> {
    /// Compiler session for policy parsing and source-map lookups.
    pub session: &'a Session,
    /// Target path supplied to the runner.
    pub path: &'a Path,
    /// Source map entry.
    pub file: &'a Arc<SourceFile>,
    /// Parsed source unit.
    pub ast: &'ast ast::SourceUnit<'ast>,
}

/// Inputs made available to a registered lint suite.
#[derive(Clone, Copy)]
pub struct LintRunContext<'a, 'gcx> {
    /// The fully analyzed Solar compiler context.
    pub gcx: Gcx<'gcx>,
    /// Project-owned files that should receive lint diagnostics.
    pub targets: &'a [PathBuf],
    /// Whether default lint descriptions should be emitted.
    pub with_description: bool,
    /// Whether help URLs should use ANSI terminal hyperlinks.
    pub with_ansi_help: bool,
}

/// Summary of a completed lint run.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LintRunResult {
    /// Number of target sources successfully visited.
    pub visited_sources: usize,
}

/// An invalid source selected for a lint run.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LintRunError {
    /// No parsed source is registered for the target.
    MissingAstSource(PathBuf),
    /// The target's parsed source does not contain an AST.
    MissingAst(PathBuf),
    /// No lowered HIR source is registered for the target.
    MissingHir(PathBuf),
}

impl fmt::Display for LintRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, path) = match self {
            Self::MissingAstSource(path) => ("AST source not found", path),
            Self::MissingAst(path) => ("AST missing", path),
            Self::MissingHir(path) => ("HIR source not found", path),
        };
        write!(f, "{kind} for {}", path.display())
    }
}

impl Error for LintRunError {}

/// Runs a suite against an already analyzed compiler context.
///
/// All targets are validated before any policy, pass factory, or lint pass is invoked.
pub fn run_lints(
    suite: &dyn LintSuite,
    cx: LintRunContext<'_, '_>,
) -> Result<LintRunResult, LintRunError> {
    let mut targets = Vec::with_capacity(cx.targets.len());
    for path in cx.targets {
        let Some((_, source)) = cx.gcx.get_ast_source(path) else {
            return Err(LintRunError::MissingAstSource(path.clone()));
        };
        let Some(ast) = source.ast.as_ref() else {
            return Err(LintRunError::MissingAst(path.clone()));
        };
        let Some((source_id, _)) = cx.gcx.get_hir_source(path) else {
            return Err(LintRunError::MissingHir(path.clone()));
        };
        targets.push((path, source.file.clone(), ast, source_id));
    }

    let registry = suite.registry();

    let sources = targets
        .par_iter()
        .map(|(path, file, ast, source_id)| {
            let source_view = LintSource { session: cx.gcx.sess, path, file, ast };
            let policy = suite.source_policy(source_view);

            let mut early_passes = Vec::new();
            for registration in &registry.early {
                if !registration.lint_ids().iter().any(|id| policy.is_lint_enabled(id)) {
                    continue;
                }
                early_passes.push(registration.create_early());
            }
            if !early_passes.is_empty() {
                let lint_context = LintContext::new(
                    cx.gcx.sess,
                    policy.as_ref(),
                    cx.with_description,
                    cx.with_ansi_help,
                    Some(file.clone()),
                );
                let mut visitor = EarlyLintVisitor::new(&lint_context, &mut early_passes);
                _ = visitor.visit_source_unit(ast);
                visitor.post_source_unit(ast);
            }

            let mut late_passes = Vec::new();
            for registration in &registry.late {
                if !registration.lint_ids().iter().any(|id| policy.is_lint_enabled(id)) {
                    continue;
                }
                late_passes.push(registration.create_late());
            }
            if !late_passes.is_empty() {
                let lint_context = LintContext::new(
                    cx.gcx.sess,
                    policy.as_ref(),
                    cx.with_description,
                    cx.with_ansi_help,
                    Some(file.clone()),
                );
                let mut visitor =
                    LateLintVisitor::new(&lint_context, &mut late_passes, cx.gcx, &cx.gcx.hir);
                _ = visitor.visit_nested_source(*source_id);
            }

            ProjectSource { path: (*path).clone(), file: file.clone(), ast, policy }
        })
        .collect::<Vec<_>>();
    let result = LintRunResult { visited_sources: sources.len() };

    let policy = suite.project_policy();
    let context = ProjectLintContext::new(
        cx.gcx.sess,
        cx.gcx,
        policy.clone(),
        cx.with_description,
        cx.with_ansi_help,
    );
    for registration in &registry.project {
        if !registration.lint_ids().iter().any(|id| policy.is_lint_enabled(id)) {
            continue;
        }
        let mut pass = registration.create_project();
        pass.check_project(&context, &sources);
    }
    Ok(result)
}
