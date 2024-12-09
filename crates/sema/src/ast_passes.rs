//! AST-related passes.

use solar_ast::{self as ast, visit::Visit};
use solar_data_structures::Never;
use solar_interface::{diagnostics::DiagCtxt, sym, Session, Span};
use std::ops::ControlFlow;

mod utils;

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
struct AstValidator<'sess, 'ast> {
    span: Span,
    dcx: &'sess DiagCtxt,
    contract: Option<&'ast ast::ItemContract<'ast>>,
    function_kind: Option<ast::FunctionKind>,
    in_unchecked_block: bool,
    in_loop_depth: u64,
    x_depth: u64, // How far away are you from the nearest loop
    placeholder_count: u64,
}

impl<'sess> AstValidator<'sess, '_> {
    fn new(sess: &'sess Session) -> Self {
        Self {
            span: Span::DUMMY,
            dcx: &sess.dcx,
            contract: None,
            function_kind: None,
            in_unchecked_block: false,
            in_loop_depth: 0,
            x_depth: 0,
            placeholder_count: 0,
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

impl<'ast> Visit<'ast> for AstValidator<'_, 'ast> {
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
        match &stmt.kind {
            ast::StmtKind::While(cond, body) => {
                self.x_depth = 0;
                self.visit_expr(cond)?;
                self.x_depth = 1;
                self.in_loop_depth += 1;
                let r = self.visit_stmt(body);
                utils::check_if_loop_body_is_a_variable_declaration(body, self.dcx());
                self.in_loop_depth -= 1;
                return r;
            }
            ast::StmtKind::DoWhile(body, ..) => {
                self.x_depth = 1;
                self.in_loop_depth += 1;
                let r = self.walk_stmt(body);
                utils::check_if_loop_body_is_a_variable_declaration(body, self.dcx());
                self.in_loop_depth -= 1;
                return r;
            }
            ast::StmtKind::For { init, cond, next, body } => {
                if let Some(init) = init {
                    self.x_depth = 0;
                    self.visit_stmt(init)?;
                }
                if let Some(cond) = cond {
                    self.visit_expr(cond)?;
                }
                if let Some(next) = next {
                    self.visit_expr(next)?;
                }
                self.in_loop_depth += 1;
                self.x_depth = 1;
                let r = self.visit_stmt(body);
                utils::check_if_loop_body_is_a_variable_declaration(body, self.dcx());
                self.in_loop_depth -= 1;
                return r;
            }
            ast::StmtKind::Break | ast::StmtKind::Continue => {
                if !self.in_loop() {
                    let kind = if matches!(stmt.kind, ast::StmtKind::Break) {
                        "break"
                    } else {
                        "continue"
                    };
                    let msg = format!("`{kind}` outside of a loop");
                    self.dcx().err(msg).span(stmt.span).emit();
                }
            }
            ast::StmtKind::UncheckedBlock(block) => {
                if self.in_unchecked_block {
                    self.dcx().err("`unchecked` blocks cannot be nested").span(stmt.span).emit();
                }

                let prev = self.in_unchecked_block;
                self.in_unchecked_block = true;
                let r = self.visit_block(block);
                self.in_unchecked_block = prev;
                return r;
            }
            ast::StmtKind::Placeholder => {
                self.placeholder_count += 1;
                if !self.function_kind.is_some_and(|k| k.is_modifier()) {
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
        self.contract = Some(contract);
        let r = self.walk_item_contract(contract);
        self.contract = None;
        r
    }

    fn visit_item_function(
        &mut self,
        func: &'ast ast::ItemFunction<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.function_kind = Some(func.kind);

        if let Some(contract) = self.contract {
            if func.kind.is_function() {
                if let Some(func_name) = func.header.name {
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

        let current_placeholder_count = self.placeholder_count;
        let r = self.walk_item_function(func);
        self.function_kind = None;

        if func.kind.is_modifier() {
            let num_placeholders_increased = self.placeholder_count - current_placeholder_count;
            if num_placeholders_increased == 0 {
                if let Some(func_name) = func.header.name {
                    self.dcx()
                        .err("modifier must have a `_;` placeholder statement")
                        .span(func_name.span)
                        .emit();
                }
            }
        }
        r
    }

    fn visit_using_directive(
        &mut self,
        using: &'ast ast::UsingDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        let ast::UsingDirective { list: _, ty, global } = using;
        let with_typ = ty.is_some();
        if self.contract.is_none() && !with_typ {
            self.dcx()
                .err("the type has to be specified explicitly at file level (cannot use `*`)")
                .span(self.span)
                .emit();
        }
        if *global && !with_typ {
            self.dcx()
                .err("can only globally attach functions to specific types")
                .span(self.span)
                .emit();
        }
        if *global && self.contract.is_some() {
            self.dcx().err("`global` can only be used at file level").span(self.span).emit();
        }
        if let Some(contract) = self.contract {
            if contract.kind.is_interface() {
                self.dcx()
                    .err("the `using for` directive is not allowed inside interfaces")
                    .span(self.span)
                    .emit();
            }
        }
        self.walk_using_directive(using)
    }

    fn visit_block(
        &mut self,
        block: &'ast solar_ast::Block<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.x_depth += 1;
        self.walk_block(block)
    }

    fn visit_variable_definition(
        &mut self,
        var: &'ast solar_ast::VariableDefinition<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        if self.in_loop() && self.x_depth == 1 {
            //self.dcx()
            //    .err("variable declarations are not allowed as the body of a loop")
            //    .span(var.span)
            //    .help("wrap the statement in a block (`{ ... }`)")
            //    .emit();
        }
        self.walk_variable_definition(var)
    }
}
