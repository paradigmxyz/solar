use crate::{Lint, LintContext, LintPolicy};
use solar_ast as ast;
use solar_interface::{Session, Span, diagnostics::Diag, source_map::SourceFile};
use solar_sema::Gcx;
use std::{cell::RefCell, collections::HashSet, path::PathBuf, sync::Arc};

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
    emitted: RefCell<HashSet<u64>>,
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
        Self { sess, gcx, policy, with_description, with_ansi_help, emitted: RefCell::default() }
    }

    /// Returns the fully analyzed compiler context.
    pub const fn gcx(&self) -> Gcx<'gcx> {
        self.gcx
    }

    /// Returns whether a lint is active for this project run.
    pub fn is_lint_enabled(&self, id: &str) -> bool {
        self.policy.is_lint_enabled(id)
    }

    /// Decorates and emits a lint diagnostic using the source's suppression policy.
    ///
    /// See [`LintContext::span_lint`] for decoration and presentation behavior.
    pub fn span_lint<L: Lint>(
        &self,
        source: &ProjectSource<'_>,
        lint: &'static L,
        span: Span,
        decorate: impl FnOnce(&mut Diag),
    ) {
        self.source_context(source).span_lint_with_dedup(lint, span, decorate, &self.emitted);
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

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, diagnostics::Level};
    use solar_sema::Compiler;
    use std::ops::ControlFlow;

    struct HelpLint;

    impl Lint for HelpLint {
        fn id(&self) -> &'static str {
            "project-help"
        }

        fn level(&self) -> Level {
            Level::Warning
        }

        fn help(&self) -> &'static str {
            "https://example.com/project-lint"
        }
    }

    struct Policy {
        enabled: bool,
        suppressed: bool,
    }

    impl LintPolicy for Policy {
        fn is_lint_enabled(&self, _id: &str) -> bool {
            self.enabled
        }

        fn is_lint_suppressed(&self, _id: &str, _span: Span) -> bool {
            self.suppressed
        }
    }

    fn emit_project_help(policy: Policy) -> String {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        session.dcx.set_flags(|flags| {
            flags.track_diagnostics = false;
            flags.deduplicate_diagnostics = false;
        });
        let mut compiler = Compiler::new(session);
        let path = PathBuf::from("test.sol");
        compiler.enter_mut(|compiler| {
            let mut parser = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(path.clone(), "contract Test {}")
                .unwrap();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
        });
        compiler.enter(|compiler| {
            let gcx = compiler.gcx();
            let (_, ast_source) = gcx.get_ast_source(&path).unwrap();
            let source = ProjectSource {
                path: path.clone(),
                file: ast_source.file.clone(),
                ast: ast_source.ast.as_ref().unwrap(),
                policy: Arc::new(policy),
            };
            let ctx = ProjectLintContext::new(
                gcx.sess,
                gcx,
                Arc::new(Policy { enabled: true, suppressed: false }),
                true,
                false,
            );
            let span = source.ast.items.first().unwrap().span;
            for _ in 0..2 {
                ctx.span_lint(&source, &HelpLint, span, |diag| {
                    assert!(source.policy.is_lint_enabled("project-help"));
                    assert!(!source.policy.is_lint_suppressed("project-help", span));
                    diag.primary_message("project lint message");
                    diag.help("review the project declaration");
                });
            }
            ctx.span_lint(&source, &HelpLint, span, |diag| {
                diag.primary_message("project lint message");
                diag.help("custom project advice");
            });
            ctx.span_lint(&source, &HelpLint, span, |diag| {
                diag.primary_message("custom project message");
                diag.help("review the project declaration");
            });
            ctx.span_lint(&source, &HelpLint, span, |diag| {
                diag.primary_message("conditional project message");
                diag.help("review this declaration instead");
            });
        });
        compiler.dcx().emitted_diagnostics().unwrap().to_string()
    }

    #[test]
    fn project_diagnostic_help() {
        assert_data_eq!(
            emit_project_help(Policy { enabled: true, suppressed: false }),
            str![[r#"
warning[project-help]: project lint message
  ╭▸ test.sol:1:1
  │
1 │ contract Test {}
  │ ━━━━━━━━━━━━━━━━
  │
  ├ help: review the project declaration
  ╰ help: https://example.com/project-lint

warning[project-help]: project lint message
  ╭▸ test.sol:1:1
  │
1 │ contract Test {}
  │ ━━━━━━━━━━━━━━━━
  │
  ├ help: custom project advice
  ╰ help: https://example.com/project-lint

warning[project-help]: custom project message
  ╭▸ test.sol:1:1
  │
1 │ contract Test {}
  │ ━━━━━━━━━━━━━━━━
  │
  ├ help: review the project declaration
  ╰ help: https://example.com/project-lint

warning[project-help]: conditional project message
  ╭▸ test.sol:1:1
  │
1 │ contract Test {}
  │ ━━━━━━━━━━━━━━━━
  │
  ├ help: review this declaration instead
  ╰ help: https://example.com/project-lint


"#]]
        );
    }

    #[test]
    fn project_diagnostic_help_uses_source_policy() {
        for policy in [
            Policy { enabled: false, suppressed: false },
            Policy { enabled: true, suppressed: true },
        ] {
            assert_data_eq!(emit_project_help(policy), str![""]);
        }
    }
}
