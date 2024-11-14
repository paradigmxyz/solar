//! AST-related passes.

use solar_ast::{
    ast,
    ast::{Stmt, StmtKind},
    visit::Visit,
};
use solar_data_structures::Never;
use solar_interface::{diagnostics::DiagCtxt, sym, Session, Span};
use std::ops::ControlFlow;

#[instrument(name = "ast_passes", level = "debug", skip_all)]
pub(crate) fn run(sess: &Session, ast: &ast::SourceUnit<'_>) {
    validate(sess, ast);
}

/// Performs AST validation.
#[instrument(name = "validate", level = "debug", skip_all)]
pub fn validate(sess: &Session, ast: &ast::SourceUnit<'_>) {
    let mut validator = AstValidator::new(sess);
    validator.visit_source_unit(ast);
}

/// AST validator.
struct AstValidator<'sess> {
    span: Span,
    dcx: &'sess DiagCtxt,
    in_loop_depth: u64,
    in_unchecked_block: bool,
}

impl<'sess> AstValidator<'sess> {
    fn new(sess: &'sess Session) -> Self {
        Self { span: Span::DUMMY, dcx: &sess.dcx, in_loop_depth: 0, in_unchecked_block: false }
    }

    /// Returns the diagnostics context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        self.dcx
    }

    fn in_loop(&self) -> bool {
        self.in_loop_depth != 0
    }
}

impl<'ast> Visit<'ast> for AstValidator<'_> {
    type BreakValue = Never;

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        self.span = item.span;
        self.walk_item(item)
    }

    fn visit_pragma_directive(
        &mut self,
        pragma: &'ast ast::PragmaDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        match &pragma.tokens {
            ast::PragmaTokens::Version(name, _version) => {
                if name.name != sym::solidity {
                    let msg = "only `solidity` is supported as a version pragma";
                    self.dcx().err(msg).span(name.span).emit();
                }
            }
            ast::PragmaTokens::Custom(name, value) => {
                let name = name.as_str();
                let value = value.as_ref().map(ast::IdentOrStrLit::as_str);
                match (name, value) {
                    ("abicoder", Some("v1" | "v2")) => {}
                    ("experimental", Some("ABIEncoderV2")) => {}
                    ("experimental", Some("SMTChecker")) => {}
                    ("experimental", Some("solidity")) => {
                        let msg = "experimental solidity features are not supported";
                        self.dcx().err(msg).span(self.span).emit();
                    }
                    _ => {
                        self.dcx().err("unknown pragma").span(self.span).emit();
                    }
                }
            }
            ast::PragmaTokens::Verbatim(_) => {
                self.dcx().err("unknown pragma").span(self.span).emit();
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        let Stmt { kind, .. } = stmt;

        match kind {
            StmtKind::While(_, body, ..)
            | StmtKind::DoWhile(body, ..)
            | StmtKind::For { body, .. } => {
                self.in_loop_depth += 1;
                self.walk_stmt(body)?;
                self.in_loop_depth -= 1;
            }
            StmtKind::Break => {
                if !self.in_loop() {
                    self.dcx()
                        .err("'break' has to be in a 'for' or 'while' loop")
                        .span(stmt.span)
                        .emit();
                }
            }
            StmtKind::Continue => {
                if !self.in_loop() {
                    self.dcx()
                        .err("'continue' has to be in a 'for' or 'while' loop")
                        .span(stmt.span)
                        .emit();
                }
            }
            StmtKind::UncheckedBlock(block) => {
                if self.in_unchecked_block {
                    self.dcx().err("'unchecked' blocks cannot be nested").span(stmt.span).emit();
                }

                let prev = self.in_unchecked_block;
                self.in_unchecked_block = true;
                self.walk_block(block);
                self.in_unchecked_block = prev;
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    // Intentionally override unused default implementations to reduce bloat.
    fn visit_expr(&mut self, _expr: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, _ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}
