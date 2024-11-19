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
    in_modifier: bool,
}

impl<'sess> AstValidator<'sess> {
    fn new(sess: &'sess Session) -> Self {
        Self {
            span: Span::DUMMY,
            dcx: &sess.dcx,
            in_loop_depth: 0,
            in_unchecked_block: false,
            in_modifier: false,
        }
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

    fn visit_item_struct(
        &mut self,
        item: &'ast ast::ItemStruct<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        let ast::ItemStruct { name, fields, .. } = item;
        if fields.is_empty() {
            self.dcx().err("structs must have at least one field").span(name.span).emit();
        }
        ControlFlow::Continue(())
    }

    fn visit_item_enum(
        &mut self,
        enum_: &'ast ast::ItemEnum<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        let ast::ItemEnum { name, variants } = enum_;
        if variants.is_empty() {
            self.dcx().err("enum must have at least one variant").span(name.span).emit();
        }
        if variants.len() > 256 {
            self.dcx().err("enum cannot have more than 256 variants").span(name.span).emit();
        }
        ControlFlow::Continue(())
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
                let r = self.walk_stmt(body);
                self.in_loop_depth -= 1;
                return r;
            }
            StmtKind::Break | StmtKind::Continue => {
                if !self.in_loop() {
                    let kind = if matches!(kind, StmtKind::Break) { "break" } else { "continue" };
                    let msg = format!("`{kind}` outside of a loop");
                    self.dcx().err(msg).span(stmt.span).emit();
                }
            }
            StmtKind::UncheckedBlock(block) => {
                if self.in_unchecked_block {
                    self.dcx().err("`unchecked` blocks cannot be nested").span(stmt.span).emit();
                }

                let prev = self.in_unchecked_block;
                self.in_unchecked_block = true;
                let r = self.walk_block(block);
                self.in_unchecked_block = prev;
                return r;
            }
            StmtKind::Placeholder => {
                if !self.in_modifier {
                    self.dcx()
                        .err("placeholder statements can only be used in modifiers")
                        .span(stmt.span)
                        .emit();
                }
            }
            _ => {}
        }

        self.walk_stmt(stmt)
    }

    fn visit_item_contract(
        &mut self,
        contract: &'ast ast::ItemContract<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        let ast::ItemContract { kind: _, name: _, bases: _, body } = contract;

        for item in body.iter() {
            if let ast::ItemKind::Function(ast::ItemFunction { kind, header, body: _ }) = &item.kind
            {
                if *kind == ast::FunctionKind::Function {
                    if let Some(func_name) = header.name {
                        if func_name == contract.name {
                            self.dcx()
                            .err("functions are not allowed to have the same name as the contract")
                            .note("if you intend this to be a constructor, use `constructor(...) { ... }` to define it")
                            .span(func_name.span)
                            .emit();
                        }
                    }
                }
            }
        }

        self.walk_item_contract(contract)
    }

    // Intentionally override unused default implementations to reduce bloat.
    fn visit_expr(&mut self, _expr: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, _ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    fn visit_item_function(
        &mut self,
        func: &'ast ast::ItemFunction<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        let ast::ItemFunction { kind, .. } = func;
        self.in_modifier = *kind == ast::FunctionKind::Modifier;
        let r = self.walk_item_function(func);
        self.in_modifier = false;
        r
    }
}
