//! AST-related passes.

use alloy_primitives::Address;
use solar_ast::{self as ast, visit::Visit};
use solar_data_structures::Never;
use solar_interface::{diagnostics::DiagCtxt, sym, Session, Span};
use std::ops::ControlFlow;

#[instrument(name = "ast_passes", level = "debug", skip_all)]
pub(crate) fn run(sess: &Session, ast: &ast::SourceUnit<'_>) {
    validate(sess, ast);
}

/// Performs AST validation.
#[instrument(name = "validate", level = "debug", skip_all)]
fn validate(sess: &Session, ast: &ast::SourceUnit<'_>) {
    let mut validator = AstValidator::new(sess);
    let _ = validator.visit_source_unit(ast);
}

/// AST validator.
struct AstValidator<'sess, 'ast> {
    item_span: Span,
    dcx: &'sess DiagCtxt,
    contract: Option<&'ast ast::ItemContract<'ast>>,
    function_kind: Option<ast::FunctionKind>,
    in_unchecked_block: bool,
    loop_depth: u32,
    placeholder_count: u32,
}

impl<'sess> AstValidator<'sess, '_> {
    fn new(sess: &'sess Session) -> Self {
        Self {
            item_span: Span::DUMMY,
            dcx: &sess.dcx,
            contract: None,
            function_kind: None,
            in_unchecked_block: false,
            loop_depth: 0,
            placeholder_count: 0,
        }
    }

    /// Returns the diagnostics context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        self.dcx
    }

    fn in_loop(&self) -> bool {
        self.loop_depth != 0
    }

    fn check_single_statement_variable_declaration(&self, stmt: &ast::Stmt<'_>) {
        if matches!(stmt.kind, ast::StmtKind::DeclSingle(..) | ast::StmtKind::DeclMulti(..)) {
            self.dcx()
                .err("variable declarations can only be used inside blocks")
                .span(stmt.span)
                .help("wrap the statement in a block (`{ ... }`)")
                .emit();
        }
    }

    fn check_underscores_in_number_literals(&self, lit: &ast::Lit) {
        let (ast::LitKind::Number(_) | ast::LitKind::Rational(_)) = lit.kind else {
            return;
        };
        let value = lit.symbol.as_str();

        let report = |help: &'static str| {
            let _ = self
                .dcx()
                .err("invalid use of underscores in number literal")
                .span(lit.span)
                .help(help)
                .emit();
        };

        if value.ends_with('_') {
            report("remove trailing underscores");
            return;
        }
        if value.contains("__") {
            report("only 1 consecutive underscore `_` is allowed between digits");
            return;
        }

        if value.starts_with("0x") {
            return;
        }
        if value.contains("._") || value.contains("_.") {
            report("remove underscores in front of the fraction part");
        }
        if value.contains("_e") || value.contains("_E") {
            report("remove underscores at the end of the mantissa");
        }
        if value.contains("e_") || value.contains("E_") {
            report("remove underscores in front of the exponent");
        }
    }

    fn check_subdenominations_for_number_literals(
        &self,
        lit: &ast::Lit,
        subdenomination: &Option<ast::SubDenomination>,
    ) {
        let Some(denom) = subdenomination else {
            return;
        };

        let (ast::LitKind::Number(_) | ast::LitKind::Rational(_)) = lit.kind else {
            panic!("non-number literal with denomination {:?}", lit.kind)
        };

        if lit.symbol.as_str().starts_with("0x") {
            self.dcx()
                .err("hexadecimal numbers cannot be used with unit denominations")
                .span(lit.span)
                .help("you can use an expression of the form \"0x1234 * 1 days\" instead")
                .emit();
        }

        if let ast::SubDenomination::Time(ast::TimeSubDenomination::Years) = denom {
            self.dcx()
                .err("using \"years\" as a unit denomination is deprecated")
                .span(lit.span)
                .emit();
        }
    }

    fn check_address_checksums(&self, lit: &ast::Lit) {
        let ast::LitKind::Address(addr) = lit.kind else {
            return;
        };

        if Address::parse_checksummed(lit.symbol.as_str(), None).is_err() {
            self.dcx()
                .err("invalid checksummed address")
                .span(lit.span)
                .help(format!("correct checksummed address: \"{}\"", addr.to_checksum(None)))
                .note("if this is not used as an address, please prepend \"00\"")
                .emit();
        }
    }
}

impl<'ast> Visit<'ast> for AstValidator<'_, 'ast> {
    type BreakValue = Never;

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        self.item_span = item.span;
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
                        self.dcx().err(msg).span(self.item_span).emit();
                    }
                    _ => {
                        self.dcx().err("unknown pragma").span(self.item_span).emit();
                    }
                }
            }
            ast::PragmaTokens::Verbatim(_) => {
                self.dcx().err("unknown pragma").span(self.item_span).emit();
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        match &stmt.kind {
            ast::StmtKind::While(_, body)
            | ast::StmtKind::DoWhile(body, _)
            | ast::StmtKind::For { body, .. } => {
                self.loop_depth += 1;
                self.check_single_statement_variable_declaration(body);
                let r = self.walk_stmt(stmt);
                self.loop_depth -= 1;
                return r;
            }
            ast::StmtKind::If(_cond, then, else_) => {
                self.check_single_statement_variable_declaration(then);
                if let Some(else_) = else_ {
                    self.check_single_statement_variable_declaration(else_);
                }
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
            ast::StmtKind::UncheckedBlock(_block) => {
                if self.in_unchecked_block {
                    self.dcx().err("`unchecked` blocks cannot be nested").span(stmt.span).emit();
                }

                let prev = self.in_unchecked_block;
                self.in_unchecked_block = true;
                let r = self.walk_stmt(stmt);
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
                if self.in_unchecked_block {
                    self.dcx()
                        .err("placeholder statements cannot be used inside unchecked blocks")
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

        if contract.kind.is_library() {
            if !contract.bases.is_empty() {
                self.dcx().err("library is not allowed to inherit").span(contract.name.span).emit();
            }
            for item in contract.body.iter() {
                if let ast::ItemKind::Variable(var) = &item.kind {
                    if !var.mutability.is_some_and(|m| m.is_constant()) {
                        self.dcx()
                            .err("library cannot have non-constant state variable")
                            .span(var.span)
                            .emit();
                    }
                }
            }
        }

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
            if contract.kind.is_interface() && !func.header.modifiers.is_empty() {
                self.dcx()
                    .err("functions in interfaces cannot have modifiers")
                    .span(self.item_span)
                    .emit();
            } else if !func.is_implemented() && !func.header.modifiers.is_empty() {
                self.dcx()
                    .err("functions without implementation cannot have modifiers")
                    .span(self.item_span)
                    .emit();
            }
        }

        if func.kind.is_receive() {
            if self.contract.is_some_and(|c| c.kind.is_library()) {
                self.dcx()
                    .err("libraries cannot have receive ether functions")
                    .span(self.item_span)
                    .emit();
            }

            if func.header.state_mutability.as_deref() != Some(&ast::StateMutability::Payable) {
                self.dcx()
                    .err("receive ether function must be payable")
                    .span(self.item_span)
                    .help("add `payable` state mutability")
                    .emit();
            }

            if !func.header.parameters.is_empty() {
                self.dcx()
                    .err("receive ether function cannot take parameters")
                    .span(self.item_span)
                    .emit();
            }
        }

        if func.header.visibility.is_none() {
            if let Some(contract) = self.contract {
                if let Some(suggested_visibility) = if func.kind.is_function() {
                    Some(if contract.kind.is_interface() { "external" } else { "public" })
                } else if func.kind.is_fallback() || func.kind.is_receive() {
                    Some("external")
                } else {
                    None
                } {
                    self.dcx()
                        .err("no visibility specified")
                        .span(self.item_span)
                        .help(format!("add `{suggested_visibility}` to the declaration"))
                        .emit();
                }
            }
        }

        if self.contract.is_none() && func.kind.is_function() {
            if !func.is_implemented() {
                self.dcx().err("free functions must be implemented").span(self.item_span).emit();
            }
            if let Some(visibility) = func.header.visibility {
                self.dcx()
                    .err("free functions cannot have visibility")
                    .span(self.item_span)
                    .help(format!("remove `{}` from the declaration", *visibility))
                    .emit();
            }
        }

        let current_placeholder_count = self.placeholder_count;
        let r = self.walk_item_function(func);
        self.function_kind = None;

        if func.kind.is_modifier() && func.is_implemented() {
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
                .span(self.item_span)
                .emit();
        }
        if *global && !with_typ {
            self.dcx()
                .err("can only globally attach functions to specific types")
                .span(self.item_span)
                .emit();
        }
        if *global && self.contract.is_some() {
            self.dcx().err("`global` can only be used at file level").span(self.item_span).emit();
        }
        if let Some(contract) = self.contract {
            if contract.kind.is_interface() {
                self.dcx()
                    .err("the `using for` directive is not allowed inside interfaces")
                    .span(self.item_span)
                    .emit();
            }
        }
        self.walk_using_directive(using)
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        let ast::Expr { kind, .. } = expr;
        if let ast::ExprKind::Lit(lit, subdenomination) = kind {
            self.check_underscores_in_number_literals(lit);
            self.check_subdenominations_for_number_literals(lit, subdenomination);
            self.check_address_checksums(lit);
        }
        self.walk_expr(expr)
    }

    fn visit_ty(&mut self, ty: &'ast solar_ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        if let ast::TypeKind::Function(f) = &ty.kind {
            for param in f.returns.iter() {
                if let Some(param_name) = param.name {
                    self.dcx()
                        .err("return parameters in function types may not be named")
                        .span(param.span)
                        .span_help(param_name.span, format!("remove `{param_name}`"))
                        .emit();
                }
            }
        }
        self.walk_ty(ty)
    }
}
