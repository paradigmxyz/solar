use crate::{
    hir::{self, Hir, IdCounter},
    parse::Sources,
    ty::{Gcx, GcxMut},
};
use alloy_primitives::Address;
use solar_ast as ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashMap,
};
use solar_interface::{
    Session, Span,
    diagnostics::{Applicability, DiagCtxt},
    sym,
};

mod lower;

mod linearize;

pub(crate) mod resolve;
pub(crate) use resolve::{Res, SymbolResolver};

pub(crate) fn lower(mut gcx: GcxMut<'_>) {
    let mut lcx = LoweringContext::new(gcx.get());

    // Lower AST to HIR.
    lcx.lower_sources();

    // Resolve source scopes.
    lcx.collect_exports();
    lcx.perform_imports();

    // Resolve contract scopes.
    lcx.collect_contract_declarations();
    lcx.resolve_base_contracts();
    lcx.linearize_contracts();
    lcx.assign_constructors();

    let mut rcx = resolve::ResolveContext::new(lcx);
    // Resolve declarations and top-level symbols, and finish lowering to HIR.
    rcx.resolve_symbols();
    // Resolve constructor base args.
    rcx.resolve_base_args();
    let lcx = rcx.lcx;

    let gcx = gcx.get_mut();
    (gcx.hir, gcx.symbol_resolver) = lcx.finish();
}

struct LoweringContext<'gcx> {
    sess: &'gcx Session,
    arena: &'gcx hir::Arena,
    hir: Hir<'gcx>,

    sources: &'gcx Sources<'gcx>,
    /// Mapping from Hir ItemId to AST Item. Does not include function parameters or bodies.
    hir_to_ast: FxHashMap<hir::ItemId, &'gcx ast::Item<'gcx>>,

    /// Current source being lowered.
    current_source_id: hir::SourceId,
    /// Current contract being lowered.
    current_contract_id: Option<hir::ContractId>,

    resolver: SymbolResolver<'gcx>,
    next_id: IdCounter,
}

impl<'gcx> LoweringContext<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            sess: gcx.sess,
            arena: gcx.arena(),
            sources: &gcx.sources,
            hir: Hir::new(),
            current_source_id: hir::SourceId::MAX,
            current_contract_id: None,
            hir_to_ast: FxHashMap::default(),
            resolver: SymbolResolver::new(&gcx.sess.dcx),
            next_id: IdCounter::new(),
        }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    fn validate_pragma_directive(&self, item_span: Span, pragma: &ast::PragmaDirective<'_>) {
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
                        self.dcx().err(msg).span(item_span).emit();
                    }
                    _ => {
                        self.dcx().err("unknown pragma").span(item_span).emit();
                    }
                }
            }
            ast::PragmaTokens::Verbatim(_) => {
                self.dcx().err("unknown pragma").span(item_span).emit();
            }
        }
    }

    fn validate_using_directive(
        &self,
        item_span: Span,
        using: &ast::UsingDirective<'_>,
        contract_kind: Option<ast::ContractKind>,
    ) {
        let ast::UsingDirective { list: _, ty, global } = using;
        let with_ty = ty.is_some();
        if contract_kind.is_none() && !with_ty {
            self.dcx()
                .err("the type has to be specified explicitly at file level (cannot use `*`)")
                .span(item_span)
                .emit();
        }
        if *global && !with_ty {
            self.dcx()
                .err("can only globally attach functions to specific types")
                .span(item_span)
                .emit();
        }
        if *global && contract_kind.is_some() {
            self.dcx().err("`global` can only be used at file level").span(item_span).emit();
        }
        if contract_kind.is_some_and(|kind| kind.is_interface()) {
            self.dcx()
                .err("the `using for` directive is not allowed inside interfaces")
                .span(item_span)
                .emit();
        }
        if let Some(ty) = ty {
            self.validate_type_ast(ty);
        }
    }

    fn validate_literal(&self, lit: &ast::Lit<'_>, subdenomination: &Option<ast::SubDenomination>) {
        let is_number = matches!(lit.kind, ast::LitKind::Number(_) | ast::LitKind::Rational(_));
        if is_number {
            self.check_underscores_in_number_literal(lit);
            self.check_subdenomination_for_number_literal(lit, subdenomination);
        }

        if let ast::LitKind::Address(addr) = lit.kind
            && Address::parse_checksummed(lit.symbol.as_str(), None).is_err()
        {
            self.dcx()
                .err("invalid checksummed address")
                .span(lit.span)
                .help(format!("correct checksummed address: \"{}\"", addr.to_checksum(None)))
                .note("if this is not used as an address, please prepend \"00\"")
                .emit();
        }
    }

    fn check_underscores_in_number_literal(&self, lit: &ast::Lit<'_>) {
        let value = lit.symbol.as_str();
        if !value.as_bytes().contains(&b'_') {
            return;
        }

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

    fn check_subdenomination_for_number_literal(
        &self,
        lit: &ast::Lit<'_>,
        subdenomination: &Option<ast::SubDenomination>,
    ) {
        let Some(denom) = subdenomination else {
            return;
        };

        debug_assert!(matches!(lit.kind, ast::LitKind::Number(_) | ast::LitKind::Rational(_)));

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

    fn check_function_type_returns(&self, ty: &ast::Type<'_>) {
        let ast::TypeKind::Function(f) = &ty.kind else {
            return;
        };
        for ret in f.returns().iter() {
            if let Some(ret_name) = ret.name {
                self.dcx()
                    .err("return parameters in function types may not be named")
                    .span(ret.span)
                    .span_suggestion(
                        ret_name.span.with_lo(ret.ty.span.hi()),
                        format!("remove `{ret_name}`"),
                        "",
                        Applicability::MachineApplicable,
                    )
                    .emit();
            }
        }
    }

    fn validate_type_ast(&self, ty: &ast::Type<'_>) {
        self.check_function_type_returns(ty);
        match &ty.kind {
            ast::TypeKind::Elementary(_) | ast::TypeKind::Custom(_) => {}
            ast::TypeKind::Array(array) => {
                self.validate_type_ast(&array.element);
                if let Some(size) = &array.size {
                    self.validate_expr_ast(size);
                }
            }
            ast::TypeKind::Function(function) => {
                for param in function.parameters.iter() {
                    self.validate_variable_definition_ast(param);
                }
                if let Some(returns) = &function.returns {
                    for ret in returns.iter() {
                        self.validate_variable_definition_ast(ret);
                    }
                }
            }
            ast::TypeKind::Mapping(mapping) => {
                self.validate_type_ast(&mapping.key);
                self.validate_type_ast(&mapping.value);
            }
        }
    }

    fn validate_variable_definition_ast(&self, var: &ast::VariableDefinition<'_>) {
        self.validate_type_ast(&var.ty);
        if let Some(initializer) = &var.initializer {
            self.validate_expr_ast(initializer);
        }
    }

    fn validate_call_args_ast(&self, args: &ast::CallArgs<'_>) {
        match &args.kind {
            ast::CallArgsKind::Unnamed(args) => {
                for arg in args.iter() {
                    self.validate_expr_ast(arg);
                }
            }
            ast::CallArgsKind::Named(args) => {
                for arg in args.iter() {
                    self.validate_expr_ast(arg.value);
                }
            }
        }
    }

    fn validate_expr_ast(&self, expr: &ast::Expr<'_>) {
        match &expr.kind {
            ast::ExprKind::Array(exprs) => {
                for expr in exprs.iter() {
                    self.validate_expr_ast(expr);
                }
            }
            ast::ExprKind::Assign(lhs, _, rhs) | ast::ExprKind::Binary(lhs, _, rhs) => {
                self.validate_expr_ast(lhs);
                self.validate_expr_ast(rhs);
            }
            ast::ExprKind::Call(callee, args) => {
                self.validate_expr_ast(callee);
                self.validate_call_args_ast(args);
            }
            ast::ExprKind::CallOptions(callee, options) => {
                self.validate_expr_ast(callee);
                for option in options.iter() {
                    self.validate_expr_ast(option.value);
                }
            }
            ast::ExprKind::Delete(expr)
            | ast::ExprKind::Member(expr, _)
            | ast::ExprKind::Unary(_, expr) => self.validate_expr_ast(expr),
            ast::ExprKind::Ident(_) => {}
            ast::ExprKind::Index(expr, index) => {
                self.validate_expr_ast(expr);
                match index {
                    ast::IndexKind::Index(index) => {
                        if let Some(index) = index {
                            self.validate_expr_ast(index);
                        }
                    }
                    ast::IndexKind::Range(start, end) => {
                        if let Some(start) = start {
                            self.validate_expr_ast(start);
                        }
                        if let Some(end) = end {
                            self.validate_expr_ast(end);
                        }
                    }
                }
            }
            ast::ExprKind::Lit(lit, subdenomination) => {
                self.validate_literal(lit, subdenomination);
            }
            ast::ExprKind::New(ty) | ast::ExprKind::TypeCall(ty) | ast::ExprKind::Type(ty) => {
                self.validate_type_ast(ty);
            }
            ast::ExprKind::Payable(args) => self.validate_call_args_ast(args),
            ast::ExprKind::Ternary(cond, true_, false_) => {
                self.validate_expr_ast(cond);
                self.validate_expr_ast(true_);
                self.validate_expr_ast(false_);
            }
            ast::ExprKind::Tuple(exprs) => {
                for expr in exprs.iter() {
                    if let Some(expr) = expr.as_deref().unspan() {
                        self.validate_expr_ast(expr);
                    }
                }
            }
        }
    }

    #[instrument(name = "drop_lcx", level = "debug", skip_all)]
    fn finish(self) -> (Hir<'gcx>, SymbolResolver<'gcx>) {
        // NOTE: Explicit scope to drop `self` before the span.
        {
            let this = self;
            (this.hir, this.resolver)
        }
    }
}

#[inline]
#[track_caller]
fn get_two_mut_idx<I: Idx, T>(sl: &mut IndexVec<I, T>, idx_1: I, idx_2: I) -> (&mut T, &mut T) {
    get_two_mut(&mut sl.raw, idx_1.index(), idx_2.index())
}

#[inline]
#[track_caller]
fn get_two_mut<T>(sl: &mut [T], idx_1: usize, idx_2: usize) -> (&mut T, &mut T) {
    sl.get_disjoint_mut([idx_1, idx_2]).unwrap().into()
}
