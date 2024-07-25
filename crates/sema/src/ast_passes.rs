//! AST-related passes.

use sulk_ast::{ast, visit::Visit};
use sulk_interface::{diagnostics::DiagCtxt, sym, Session, Span};

#[instrument(name = "ast_passes", level = "debug", skip_all)]
pub(crate) fn run(sess: &Session, ast: &ast::SourceUnit<'_>) {
    // TODO: Desugar here.
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
}

impl<'sess> AstValidator<'sess> {
    fn new(sess: &'sess Session) -> Self {
        Self { span: Span::DUMMY, dcx: &sess.dcx }
    }

    /// Returns the diagnostics context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        self.dcx
    }
}

impl<'ast, 'sess> Visit<'ast> for AstValidator<'sess> {
    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) {
        self.span = item.span;
        self.walk_item(item);
    }

    fn visit_pragma_directive(&mut self, pragma: &'ast ast::PragmaDirective<'ast>) {
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
    }

    // Intentionally override unused default implementations to reduce bloat.

    fn visit_expr(&mut self, _expr: &'ast ast::Expr<'ast>) {}

    fn visit_stmt(&mut self, _stmt: &'ast ast::Stmt<'ast>) {}

    fn visit_ty(&mut self, _ty: &'ast ast::Type<'ast>) {}
}
