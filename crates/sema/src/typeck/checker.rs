use crate::{
    builtins::Builtin,
    hir::{self, Visit},
    ty::{Gcx, Ty, TyKind},
};
use alloy_primitives::U256;
use solar_ast::{DataLocation, ElementaryType, Span};
use solar_data_structures::{Never, map::FxHashMap, pluralize, smallvec::SmallVec};
use solar_interface::diagnostics::DiagCtxt;
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>, source: hir::SourceId) {
    let mut checker = TypeChecker::new(gcx, source);
    let _ = checker.visit_nested_source(source);
}

struct TypeChecker<'gcx> {
    gcx: Gcx<'gcx>,
    source: hir::SourceId,
    contract: Option<hir::ContractId>,

    types: FxHashMap<hir::ExprId, Ty<'gcx>>,

    lvalue_context: Option<bool>,
}

impl<'gcx> TypeChecker<'gcx> {
    fn new(gcx: Gcx<'gcx>, source: hir::SourceId) -> Self {
        Self { gcx, source, contract: None, types: Default::default(), lvalue_context: None }
    }

    fn dcx(&self) -> &'gcx DiagCtxt {
        self.gcx.dcx()
    }

    fn get(&self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        self.types[&expr.id]
    }

    #[must_use]
    fn check_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        self.check_expr_with(expr, None)
    }

    #[must_use]
    fn expect_ty(&mut self, expr: &'gcx hir::Expr<'gcx>, expected: Ty<'gcx>) -> Ty<'gcx> {
        self.check_expr_with(expr, Some(expected))
    }

    #[track_caller]
    #[must_use]
    fn check_expr_with(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        expected: Option<Ty<'gcx>>,
    ) -> Ty<'gcx> {
        let ty = self.check_expr_with_noexpect(expr, expected);
        if let Some(expected) = expected {
            self.check_expected(expr, ty, expected);
        }
        ty
    }

    #[track_caller]
    #[must_use]
    fn check_expr_with_noexpect(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        expected: Option<Ty<'gcx>>,
    ) -> Ty<'gcx> {
        let ty = self.check_expr_kind(expr, expected);
        self.register_ty(expr, ty);
        ty
    }

    #[must_use]
    fn check_expr_kind(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        expected: Option<Ty<'gcx>>,
    ) -> Ty<'gcx> {
        macro_rules! todo {
            () => {{
                let msg = format!("not yet implemented: {expr:?}");
                return self.gcx.mk_ty_err(self.dcx().err(msg).span(expr.span).emit());
            }};
        }
        match expr.kind {
            hir::ExprKind::Array(exprs) => {
                let mut common = expected.and_then(|arr| arr.base_type(self.gcx));
                for (i, expr) in exprs.iter().enumerate() {
                    let expr_ty = self.check_expr(expr);
                    if let Some(common_ty) = common {
                        common = common_ty.common_type(expr_ty, self.gcx);
                    } else if i == 0 {
                        common = expr_ty.mobile(self.gcx);
                    }
                }
                if let Some(common) = common {
                    // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1583
                    self.gcx.mk_ty(TyKind::Array(common, U256::from(exprs.len())))
                } else {
                    self.gcx.mk_ty_err(
                        self.dcx().err("cannot infer array element type").span(expr.span).emit(),
                    )
                }
            }
            hir::ExprKind::Assign(lhs, op, rhs) => {
                let ty = self.require_lvalue(lhs);
                self.check_assign(ty, lhs);
                if ty.is_tuple() {
                    if op.is_some() {
                        let err = self
                            .dcx()
                            .err("compound assignment is not allowed for tuples")
                            .span(expr.span);
                        return self.gcx.mk_ty_err(err.emit());
                    }
                    let _ = self.expect_ty(rhs, ty);
                    ty
                } else if let Some(op) = op {
                    let rhs_ty = self.check_expr(rhs);
                    let result = self.check_binop(lhs, ty, rhs, rhs_ty, op, true);
                    debug_assert!(
                        result.references_error() || result == ty,
                        "compound assignment should not consider custom operators: {result:?} != {ty:?}"
                    );
                    result
                } else {
                    let _ = self.expect_ty(rhs, ty);
                    ty
                }
            }
            hir::ExprKind::Binary(lhs_e, op, rhs_e) => {
                let lhs = self.check_expr(lhs_e);
                let rhs = self.check_expr(rhs_e);
                self.check_binop(lhs_e, lhs, rhs_e, rhs, op, false)
            }
            hir::ExprKind::Call(callee, ref args, ref _opts) => {
                let mut callee_ty = self.check_expr(callee);
                // Get the function type for struct constructors.
                if let TyKind::Type(struct_ty) = callee_ty.kind
                    && let TyKind::Struct(id) = struct_ty.kind
                {
                    callee_ty = struct_constructor(self.gcx, struct_ty, id);
                }

                // TODO: `array.push() = x;` is the only valid call lvalue
                let is_array_push = false;
                let ty = match callee_ty.kind {
                    TyKind::FnPtr(_f) => {
                        // dbg!(callee_ty);
                        todo!()
                    }
                    TyKind::Type(to) => self.check_explicit_cast(expr.span, to, args),
                    TyKind::Event(..) | TyKind::Error(..) | TyKind::Err(_) => callee_ty,
                    _ => {
                        let msg =
                            format!("expected function, found `{}`", callee_ty.display(self.gcx));
                        let mut err = self.dcx().err(msg).span(callee.span);
                        err = err.span_note(expr.span, "call expression requires function");
                        self.gcx.mk_ty_err(err.emit())
                    }
                };

                if !is_array_push {
                    self.not_lvalue();
                }

                ty
            }
            hir::ExprKind::Delete(expr) => {
                let ty = self.require_lvalue(expr);
                if valid_delete(ty) {
                    self.gcx.types.unit
                } else {
                    let msg = format!("cannot delete `{}`", ty.display(self.gcx));
                    let err = self.dcx().err(msg).span(expr.span);
                    self.gcx.mk_ty_err(err.emit())
                }
            }
            hir::ExprKind::Ident(res) => {
                let res = self.resolve_overloads(res, expr.span);
                if !res_is_lvalue(self.gcx, res) {
                    self.not_lvalue();
                }
                self.type_of_res(res)
            }
            hir::ExprKind::Index(lhs, index) => {
                let ty = self.check_expr(lhs);
                if ty.references_error() {
                    return ty;
                }
                if ty.loc() == Some(DataLocation::Calldata) {
                    self.not_lvalue();
                }
                if let Some((index_ty, result_ty)) = self.index_types(ty) {
                    // Index expression.
                    if let Some(index) = index {
                        let _ = self.expect_ty(index, index_ty);
                    } else {
                        self.dcx().err("index expression cannot be omitted").span(expr.span).emit();
                    }
                    result_ty
                } else if let TyKind::Type(elem_ty) = ty.kind {
                    // `elem_ty` array type expression.
                    let arr = if let Some(index) = index {
                        let index_ty = self.expect_ty(index, self.gcx.types.uint(256));
                        let len = index_ty
                            .error_reported()
                            .and_then(|()| crate::eval::eval_array_len(self.gcx, index));
                        match len {
                            Ok(len) => TyKind::Array(elem_ty, len),
                            Err(guar) => TyKind::Array(self.gcx.mk_ty_err(guar), U256::from(1)),
                        }
                    } else {
                        TyKind::DynArray(elem_ty)
                    };
                    self.gcx.mk_ty(TyKind::Type(self.gcx.mk_ty(arr)))
                } else {
                    let msg = format!("cannot index into {}", ty.display(self.gcx));
                    self.gcx.mk_ty_err(self.dcx().err(msg).span(expr.span).emit())
                }
            }
            hir::ExprKind::Slice(lhs, start, end) => {
                let ty = self.check_expr(lhs);
                if !ty.is_sliceable() {
                    self.dcx().err("can only slice arrays").span(expr.span).emit();
                } else if !ty.is_ref_at(DataLocation::Calldata) {
                    self.dcx().err("can only slice dynamic calldata arrays").span(expr.span).emit();
                }
                if let Some((_index_ty, _result_ty)) = self.index_types(ty) {
                    if let Some(start) = start {
                        let _ = self.expect_ty(start, self.gcx.types.uint(256));
                    }
                    if let Some(end) = end {
                        let _ = self.expect_ty(end, self.gcx.types.uint(256));
                    }
                    if let TyKind::Slice(_) = ty.kind {
                        ty
                    } else {
                        self.gcx.mk_ty(TyKind::Slice(ty))
                    }
                } else {
                    self.gcx.mk_ty_err(self.dcx().err("cannot index").span(expr.span).emit())
                }
            }
            hir::ExprKind::Lit(lit) => self.gcx.type_of_lit(lit),
            hir::ExprKind::Member(expr, ident) => {
                let expr_ty = self.check_expr(expr);
                if expr_ty.references_error() {
                    return expr_ty;
                }

                let possible_members = self
                    .gcx
                    .members_of(expr_ty, self.source, self.contract)
                    .iter()
                    .filter(|m| m.name == ident.name)
                    .collect::<SmallVec<[_; 4]>>();

                // TODO: overload resolution

                let ty = match possible_members[..] {
                    [] => {
                        let msg = format!(
                            "member `{ident}` not found on type `{}`",
                            expr_ty.display(self.gcx)
                        );
                        // TODO: Did you mean ...?
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                    [member] => member.ty,
                    [..] => {
                        let msg = format!(
                            "member `{ident}` not unique on type `{}`",
                            expr_ty.display(self.gcx)
                        );
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                };

                // Validate lvalue.
                match expr_ty.kind {
                    TyKind::Ref(_, d) if d.is_calldata() => self.not_lvalue(),
                    TyKind::Type(ty)
                        if matches!(ty.kind, TyKind::Contract(_))
                            && possible_members.len() == 1
                            && !possible_members[0]
                                .res
                                .is_some_and(|res| res_is_lvalue(self.gcx, res)) =>
                    {
                        self.not_lvalue();
                    }
                    _ => {}
                }

                ty
            }
            hir::ExprKind::New(ref hir_ty) => {
                let ty = self.gcx.type_of_hir_ty(hir_ty);
                match ty.kind {
                    TyKind::Contract(id) => {
                        let c = self.gcx.hir.contract(id);
                        let kind = c.kind;
                        if !kind.is_contract() {
                            let msg = format!("cannot instantiate {kind}s");
                            self.gcx.mk_ty_err(self.dcx().err(msg).span(hir_ty.span).emit())
                        } else {
                            let mut parameters: &[Ty<'_>] = &[];
                            let mut sm = hir::StateMutability::NonPayable;
                            if let Some(ctor) = c.ctor {
                                let func_ty = self.gcx.type_of_item(ctor.into());
                                let TyKind::FnPtr(f) = func_ty.kind else { unreachable!() };
                                parameters = f.parameters;
                                sm = f.state_mutability;
                                debug_assert!(
                                    f.returns.is_empty(),
                                    "non-empty constructor returns"
                                );
                            }
                            self.gcx.mk_builtin_fn(parameters, sm, &[ty])
                        }
                    }
                    TyKind::Array(..) => {
                        let mut err = self.dcx().err("cannot instantiate static arrays");
                        if let hir::TypeKind::Array(hir::TypeArray {
                            element: _,
                            size: Some(size_expr),
                        }) = hir_ty.kind
                        {
                            err = err.span_help(
                                size_expr.span,
                                "the length must be placed inside the parentheses after the array type",
                            );
                        }
                        self.gcx.mk_ty_err(err.emit())
                    }
                    _ if ty.is_array_like() => {
                        if ty.has_mapping() {
                            self.gcx.mk_ty_err(
                                self.dcx()
                                    .err("cannot instantiate mappings")
                                    .span(hir_ty.span)
                                    .emit(),
                            )
                        } else {
                            let ty = ty.with_loc(self.gcx, DataLocation::Memory);
                            self.gcx.mk_builtin_fn(&[], hir::StateMutability::Pure, &[ty])
                        }
                    }
                    TyKind::Err(_) => ty,
                    _ => self.gcx.mk_ty_err(
                        self.dcx()
                            .err("expected contract or dynamic array type")
                            .span(hir_ty.span)
                            .emit(),
                    ),
                }
            }
            hir::ExprKind::Payable(expr) => {
                let ty = self.expect_ty(expr, self.gcx.types.address);
                if ty.references_error() { ty } else { self.gcx.types.address_payable }
            }
            hir::ExprKind::Ternary(cond, true_, false_) => {
                let _ = self.expect_ty(cond, self.gcx.types.bool);
                // TODO: Does mobile need to return None?
                let true_ty = self.check_expr_with(true_, expected).mobile(self.gcx);
                let false_ty = self.check_expr_with(false_, expected).mobile(self.gcx);
                match (true_ty, false_ty) {
                    (Some(true_ty), Some(false_ty)) => {
                        true_ty.common_type(false_ty, self.gcx).unwrap_or_else(|| {
                            self.gcx.mk_ty_err(
                                self.dcx()
                                    .err("incompatible conditional types")
                                    //.span(vec![true_.span, false_.span])
                                    .span(expr.span)
                                    .emit(),
                            )
                        })
                    }
                    (true_ty, false_ty) => {
                        let mut guar = None;
                        if true_ty.is_none() {
                            guar =
                                Some(self.dcx().err("invalid true type").span(true_.span).emit());
                        }
                        if false_ty.is_none() {
                            guar =
                                Some(self.dcx().err("invalid false type").span(false_.span).emit());
                        }
                        true_ty.or(false_ty).unwrap_or_else(|| self.gcx.mk_ty_err(guar.unwrap()))
                    }
                }
            }
            hir::ExprKind::Tuple(exprs) => {
                let gcx = self.gcx;
                let mut tys = exprs.iter().map(|&expr_opt| {
                    let empty_err = |this: &Self, span| {
                        this.gcx.mk_ty_err(
                            this.dcx().err("tuple components cannot be empty").span(span).emit(),
                        )
                    };
                    if let Some(expr) = expr_opt {
                        let ty = if self.in_lvalue() {
                            self.require_lvalue(expr)
                        } else {
                            self.check_expr(expr)
                        };
                        if ty.is_unit() { empty_err(self, expr.span) } else { ty }
                    } else {
                        // TODO: allow lvalue empty tuple component with a placeholder type
                        empty_err(self, expr.span)
                    }
                });
                if tys.len() == 1 {
                    tys.next().unwrap()
                } else {
                    gcx.mk_ty_tuple(gcx.mk_ty_iter(tys))
                }
            }
            hir::ExprKind::TypeCall(ref hir_ty) => {
                let ty = self.gcx.type_of_hir_ty(hir_ty);
                if valid_meta_type(ty) {
                    self.gcx.mk_ty(TyKind::Meta(ty))
                } else {
                    self.gcx.mk_ty_err(self.dcx().err("invalid type").span(hir_ty.span).emit())
                }
            }
            hir::ExprKind::Type(ref ty) => {
                debug_assert!(ty.kind.is_elementary(), "non-elementary ExprKind::Type: {ty:?}");
                self.gcx.mk_ty(TyKind::Type(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Unary(op, expr) => {
                let ty = if op.kind.has_side_effects() {
                    self.require_lvalue(expr)
                } else {
                    self.check_expr_with(expr, expected)
                };
                // TODO: custom operators
                if valid_unop(ty, op.kind) {
                    ty
                } else {
                    let msg = format!(
                        "cannot apply unary operator `{op}` to `{}`",
                        ty.display(self.gcx),
                    );
                    let err = self.dcx().err(msg).span(expr.span);
                    self.gcx.mk_ty_err(err.emit())
                }
            }
            hir::ExprKind::Err(guar) => self.gcx.mk_ty_err(guar),
        }
    }

    fn check_assign(&self, ty: Ty<'gcx>, expr: &'gcx hir::Expr<'gcx>) {
        // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1421
        let _ = (ty, expr);
    }

    fn check_binop(
        &mut self,
        lhs_e: &'gcx hir::Expr<'gcx>,
        lhs: Ty<'gcx>,
        rhs_e: &'gcx hir::Expr<'gcx>,
        rhs: Ty<'gcx>,
        op: hir::BinOp,
        assign: bool,
    ) -> Ty<'gcx> {
        let common = binop_common_type(self.gcx, lhs, rhs, op.kind);
        // TODO: custom operators
        if let Some(common) = common
            && !(assign && common != lhs)
        {
            return if op.kind.is_cmp() { self.gcx.types.bool } else { common };
        }

        let msg = format!(
            "cannot apply builtin operator `{op}` to `{}` and `{}`",
            lhs.display(self.gcx),
            rhs.display(self.gcx),
        );
        let mut err = self.dcx().err(msg).span(op.span);
        err = err.span_label(lhs_e.span, lhs.display(self.gcx).to_string());
        err = err.span_label(rhs_e.span, rhs.display(self.gcx).to_string());
        self.gcx.mk_ty_err(err.emit())
    }

    /// Returns `(index_ty, result_ty)` for the given value type, if it is indexable.
    ///
    /// Does not consider `TypeKind::Type`.
    #[must_use]
    fn index_types(&self, ty: Ty<'gcx>) -> Option<(Ty<'gcx>, Ty<'gcx>)> {
        let loc = ty.loc();
        Some(match ty.peel_refs().kind {
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                (self.gcx.types.uint(256), element.with_loc_if_ref_opt(self.gcx, loc))
            }
            TyKind::Elementary(ElementaryType::Bytes)
            | TyKind::Elementary(ElementaryType::FixedBytes(_)) => {
                (self.gcx.types.uint(256), self.gcx.types.fixed_bytes(1))
            }
            TyKind::Mapping(key, value) => {
                debug_assert!(!key.is_reference_type(), "invalid mapping key {key:?}");
                (key, value.with_loc_if_ref_opt(self.gcx, loc))
            }
            _ => return None,
        })
    }

    #[must_use]
    fn check_explicit_cast(
        &mut self,
        span: Span,
        to: Ty<'gcx>,
        args: &'gcx hir::CallArgs<'gcx>,
    ) -> Ty<'gcx> {
        let WantOne::One(from_expr) = args.exprs().collect::<WantOne<_>>() else {
            return self.gcx.mk_ty_err(
                self.dcx().err("expected exactly one unnamed argument").span(args.span).emit(),
            );
        };
        let from = self.check_expr(from_expr);
        let Err(()) = from.try_convert_explicit_to(to) else { return to };

        let msg =
            format!("cannot convert `{}` to `{}`", from.display(self.gcx), to.display(self.gcx));
        let mut err = self.dcx().err(msg).span(span);
        err = err.span_label(span, "invalid explicit type conversion");
        self.gcx.mk_ty_err(err.emit())
    }

    #[track_caller]
    fn check_expected(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        actual: Ty<'gcx>,
        expected: Ty<'gcx>,
    ) {
        let Err(()) = actual.try_convert_implicit_to(expected) else { return };

        let mut err = self.dcx().err("mismatched types").span(expr.span);
        err = err.span_label(
            expr.span,
            format!(
                "expected `{}`, found `{}`",
                expected.display(self.gcx),
                actual.display(self.gcx)
            ),
        );
        err.emit();
    }

    #[must_use]
    fn check_var(&mut self, id: hir::VariableId) -> Ty<'gcx> {
        self.check_var_(id, true)
    }

    #[must_use]
    fn check_var_(&mut self, id: hir::VariableId, expect: bool) -> Ty<'gcx> {
        let var = self.gcx.hir.variable(id);
        let _ = self.visit_ty(&var.ty);
        let ty = self.gcx.type_of_item(id.into());
        if let Some(init) = var.initializer {
            // TODO: might have different logic vs assignment
            self.check_assign(ty, init);
            if expect {
                let _ = self.expect_ty(init, ty);
            }
        }
        // TODO: checks from https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L472
        ty
    }

    fn check_decl(
        &mut self,
        span: Span,
        decls: &[Option<hir::VariableId>],
        init_opt: Option<&'gcx hir::Expr<'gcx>>,
    ) {
        let Some(init) = init_opt else {
            if let &[Some(id)] = decls {
                let _ = self.check_var(id);
                return;
            }
            unreachable!("no initializer for multiple declarations")
        };

        let expected =
            if let &[Some(id)] = decls { Some(self.gcx.type_of_item(id.into())) } else { None };
        let ty = self.check_expr_with_noexpect(init, expected);
        let value_types =
            if let TyKind::Tuple(types) = ty.kind { types } else { std::slice::from_ref(&ty) };

        debug_assert!(!decls.is_empty());
        if value_types.len() != decls.len() {
            self.dcx()
                .err("mismatched number of components")
                .span(span)
                .span_label(
                    init.span,
                    format!(
                        "expected a tuple with {} element{}, found one with {} element{}",
                        decls.len(),
                        pluralize!(decls.len()),
                        value_types.len(),
                        pluralize!(value_types.len())
                    ),
                )
                .emit();
        }

        let exprs = if let hir::ExprKind::Tuple(exprs) = init.kind {
            exprs
        } else {
            std::slice::from_ref(&init_opt)
        };
        for ((&var, &ty), &expr) in decls.iter().zip(value_types).zip(exprs) {
            let (Some(var), Some(expr)) = (var, expr) else { continue };
            let var_ty = self.check_var_(var, false);
            self.check_expected(expr, ty, var_ty);
        }
        // TODO: checks from https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1219
    }

    #[must_use]
    fn check_mapping_key_type(&mut self, key: &'gcx hir::Type<'gcx>) -> Ty<'gcx> {
        let ty = self.gcx.type_of_hir_ty(key);
        if !matches!(
            ty.kind,
            TyKind::Elementary(_) | TyKind::Udvt(_, _) | TyKind::Contract(_) | TyKind::Enum(_)
        ) {
            self.dcx().err("only elementary types, user defined value types, contract types or enums are allowed as mapping keys.").span(key.span).emit();
        }
        ty
    }

    #[must_use]
    fn require_lvalue(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        let prev = self.lvalue_context.replace(true);
        let ty = self.check_expr(expr);
        let ctx = self.lvalue_context;
        debug_assert!(ctx.is_some());
        self.lvalue_context = prev;
        // TODO: check ctx
        if is_syntactic_lvalue(expr) {
            return ty;
        }

        // TODO: better error message https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L4143

        self.dcx().err("expected lvalue").span(expr.span).emit();

        ty
    }

    fn not_lvalue(&mut self) {
        if let Some(v) = &mut self.lvalue_context {
            *v = false;
        }
    }

    fn in_lvalue(&self) -> bool {
        self.lvalue_context.is_some()
    }

    fn resolve_overloads(&self, res: &[hir::Res], span: Span) -> hir::Res {
        match self.try_resolve_overloads(res) {
            Ok(res) => res,
            Err(e) => {
                let msg = match e {
                    OverloadError::NotFound => "no matching declarations found",
                    OverloadError::Ambiguous => "no unique declarations found",
                };
                hir::Res::Err(self.dcx().err(msg).span(span).emit())
            }
        }
    }

    fn try_resolve_overloads(&self, res: &[hir::Res]) -> Result<hir::Res, OverloadError> {
        match res {
            [] => unreachable!("no candidates for overload resolution"),
            &[res] => return Ok(res),
            _ => {}
        }

        match res.iter().filter(|res| res.as_variable().is_some()).collect::<WantOne<_>>() {
            WantOne::Zero => Err(OverloadError::NotFound),
            WantOne::One(var) => Ok(*var),
            WantOne::Many => Err(OverloadError::Ambiguous),
        }
    }

    fn type_of_res(&self, res: hir::Res) -> Ty<'gcx> {
        match res {
            hir::Res::Builtin(Builtin::This | Builtin::Super) => self
                .contract
                .map(|contract| self.gcx.type_of_item(contract.into()))
                .unwrap_or_else(|| self.gcx.mk_ty_misc_err()),
            // TODO: Different type for super
            // hir::Res::Builtin(Builtin::Super) => {}
            res => self.gcx.type_of_res(res),
        }
    }

    fn register_ty(&mut self, expr: &'gcx hir::Expr<'gcx>, ty: Ty<'gcx>) {
        if let Some(prev_ty) = self.types.insert(expr.id, ty) {
            self.dcx()
                .bug("already typechecked")
                .span(expr.span)
                .span_label(
                    expr.span,
                    format!(
                        "{} -> {} for {:?}",
                        prev_ty.display(self.gcx),
                        ty.display(self.gcx),
                        expr.id
                    ),
                )
                .emit();
        }
    }
}

impl<'gcx> hir::Visit<'gcx> for TypeChecker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        let prev = self.contract.replace(id);
        let r = self.walk_nested_contract(id);
        self.contract = prev;
        r
    }

    fn visit_nested_var(&mut self, id: hir::VariableId) -> ControlFlow<Self::BreakValue> {
        let _ = self.check_var(id);
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, hir_ty: &'gcx hir::Type<'gcx>) -> ControlFlow<Self::BreakValue> {
        match hir_ty.kind {
            hir::TypeKind::Array(array) => {
                if let Some(size) = array.size {
                    let _ = self.expect_ty(size, self.gcx.types.uint(256));
                }
                return self.visit_ty(&array.element);
            }
            hir::TypeKind::Mapping(mapping) => {
                let _ = self.check_mapping_key_type(&mapping.key);
                self.visit_ty(&mapping.value)?;
            }
            // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L713
            // hir::TypeKind::Function(func) => {
            //     if func.visibility == hir::Visibility::External {

            //     }
            // }
            _ => {}
        }
        self.walk_ty(hir_ty)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        let _ = self.check_expr(expr);
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            hir::StmtKind::DeclSingle(var) => {
                let init = self.gcx.hir.variable(var).initializer;
                self.check_decl(stmt.span, &[Some(var)], init);
                return ControlFlow::Continue(());
            }
            hir::StmtKind::DeclMulti(decls, init) => {
                self.check_decl(stmt.span, decls, Some(init));
                return ControlFlow::Continue(());
            }
            hir::StmtKind::If(cond, body, else_) => {
                let _ = self.expect_ty(cond, self.gcx.types.bool);
                self.visit_stmt(body)?;
                if let Some(else_) = else_ {
                    self.visit_stmt(else_)?;
                }
                return ControlFlow::Continue(());
            }
            hir::StmtKind::Emit(expr) | hir::StmtKind::Revert(expr) => {
                let _ty = self.check_expr(expr);
                let hir::ExprKind::Call(callee, ..) = expr.kind else {
                    unreachable!("bad Emit|Revert");
                };
                let callee_ty = self.get(callee);
                if !callee_ty.references_error() {
                    match stmt.kind {
                        hir::StmtKind::Emit(_) => {
                            if !matches!(callee_ty.kind, TyKind::Event(..)) {
                                self.dcx()
                                    .err("expression is not an event")
                                    .span(callee.span)
                                    .emit();
                            }
                        }
                        hir::StmtKind::Revert(_) => {
                            if !matches!(callee_ty.kind, TyKind::Error(..)) {
                                self.dcx()
                                    .err("expression is not an error")
                                    .span(callee.span)
                                    .emit();
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        self.walk_stmt(stmt)
    }
}

/// Returns `true` if the given expression can be an lvalue.
///
/// If `false`, it cannot be an lvalue.
fn is_syntactic_lvalue(expr: &hir::Expr<'_>) -> bool {
    match expr.kind {
        hir::ExprKind::Ident(_)
        | hir::ExprKind::Index(..)
        | hir::ExprKind::Member(..)
        | hir::ExprKind::Call(..)
        | hir::ExprKind::Tuple(..)
        | hir::ExprKind::Err(_) => true,

        hir::ExprKind::Array(_)
        | hir::ExprKind::Assign(..)
        | hir::ExprKind::Binary(..)
        | hir::ExprKind::Delete(_)
        | hir::ExprKind::Slice(..)
        | hir::ExprKind::Lit(_)
        | hir::ExprKind::Payable(_)
        | hir::ExprKind::New(_)
        | hir::ExprKind::Ternary(..)
        | hir::ExprKind::TypeCall(_)
        | hir::ExprKind::Type(_)
        | hir::ExprKind::Unary(..) => false,
    }
}

enum OverloadError {
    NotFound,
    Ambiguous,
}

enum WantOne<T> {
    Zero,
    One(T),
    Many,
}

impl<T> FromIterator<T> for WantOne<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut iter = iter.into_iter().peekable();
        match iter.peek() {
            None => Self::Zero,
            Some(_) => {
                let first = iter.next().unwrap();
                match iter.peek() {
                    None => Self::One(first),
                    Some(_) => Self::Many, // (std::iter::once(first).chain(iter).collect()),
                }
            }
        }
    }
}

fn res_is_lvalue(gcx: Gcx<'_>, res: hir::Res) -> bool {
    match res {
        hir::Res::Item(hir::ItemId::Variable(var)) => !gcx.hir.variable(var).is_constant(),
        hir::Res::Err(_) => true,
        _ => false,
    }
}

fn valid_delete(ty: Ty<'_>) -> bool {
    if ty.references_error() {
        return true;
    }

    match ty.kind {
        TyKind::Elementary(_) | TyKind::Contract(_) | TyKind::Enum(_) | TyKind::FnPtr(_) => true,
        TyKind::Ref(_, loc) => !matches!(loc, DataLocation::Calldata),

        TyKind::Err(_) => true,

        _ => false,
    }
}

fn valid_unop(ty: Ty<'_>, op: hir::UnOpKind) -> bool {
    if ty.references_error() {
        return true;
    }

    let ty = ty.peel_refs();
    match ty.kind {
        TyKind::Elementary(hir::ElementaryType::Int(_) | hir::ElementaryType::UInt(_))
        | TyKind::IntLiteral(..) => match op {
            hir::UnOpKind::Neg => ty.is_signed(),
            hir::UnOpKind::Not => false,
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::BitNot
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => true,
        },
        TyKind::Elementary(hir::ElementaryType::FixedBytes(_)) => op == hir::UnOpKind::BitNot,
        TyKind::Elementary(hir::ElementaryType::Bool) => op == hir::UnOpKind::Not,

        TyKind::Err(_) => true,

        _ => false,
    }
}

fn binop_common_type<'gcx>(
    gcx: Gcx<'gcx>,
    ty: Ty<'gcx>,
    other: Ty<'gcx>,
    op: hir::BinOpKind,
) -> Option<Ty<'gcx>> {
    if let Err(guar) = ty.error_reported().and_then(|()| other.error_reported()) {
        return Some(gcx.mk_ty_err(guar));
    }

    let ty = ty.peel_refs();
    let other = other.peel_refs();
    match ty.kind {
        TyKind::Elementary(hir::ElementaryType::Int(_))
        | TyKind::Elementary(hir::ElementaryType::UInt(_))
        | TyKind::IntLiteral(..) => {
            use hir::BinOpKind::*;

            if !other.is_integer() {
                return None;
            }
            match op {
                Shl | Shr | Sar => valid_shift(ty, other, op),
                Pow => (!other.is_signed()).then_some(ty),
                And | Or => None,
                _ => ty.common_type(other, gcx),
            }
        }

        TyKind::Elementary(hir::ElementaryType::FixedBytes(_)) => {
            if op.is_shift() {
                return valid_shift(ty, other, op);
            }
            if let Some(common_type) = ty.common_type(other, gcx)
                && common_type.is_fixed_bytes()
            {
                return Some(common_type);
            }
            None
        }
        TyKind::Elementary(hir::ElementaryType::Bool) => {
            use hir::BinOpKind::*;

            (other == ty && matches!(op, Eq | Ne | And | Or)).then_some(ty)
        }

        TyKind::Elementary(hir::ElementaryType::Address(_))
        | TyKind::Contract(_)
        | TyKind::Struct(_)
        | TyKind::Enum(_)
        | TyKind::Error(..)
        | TyKind::Event(..) => {
            if op.is_cmp() {
                ty.common_type(other, gcx)
            } else {
                None
            }
        }

        TyKind::FnPtr(_) => {
            // TODO: Compare internal function pointers
            // https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/ast/Types.cpp#L3193
            None
        }

        TyKind::Elementary(hir::ElementaryType::String)
        | TyKind::Elementary(hir::ElementaryType::Bytes)
        | TyKind::Elementary(hir::ElementaryType::Fixed(..))
        | TyKind::Elementary(hir::ElementaryType::UFixed(..))
        | TyKind::StringLiteral(..)
        | TyKind::DynArray(_)
        | TyKind::Array(..)
        | TyKind::Slice(_)
        | TyKind::Tuple(_)
        | TyKind::Mapping(..)
        | TyKind::Udvt(..)
        | TyKind::Module(_)
        | TyKind::BuiltinModule(_)
        | TyKind::Type(_)
        | TyKind::Meta(_) => None,

        TyKind::Err(_) => Some(ty),

        TyKind::Ref(..) => unreachable!(),
    }
}

fn valid_shift<'gcx>(ty: Ty<'gcx>, other: Ty<'gcx>, op: hir::BinOpKind) -> Option<Ty<'gcx>> {
    debug_assert!(op.is_shift());
    // `>>>` is only allowed in fixed-point numbers.
    if matches!(op, hir::BinOpKind::Sar) {
        return None;
    }
    if !matches!(
        other.kind,
        TyKind::Elementary(hir::ElementaryType::UInt(_)) | TyKind::IntLiteral(false, ..)
    ) {
        return None;
    }
    Some(ty)
}

fn valid_meta_type(ty: Ty<'_>) -> bool {
    debug_assert!(!matches!(ty.kind, TyKind::Type(_)));
    // TODO: Disallow super
    matches!(
        ty.kind,
        TyKind::Elementary(hir::ElementaryType::Int(_) | hir::ElementaryType::UInt(_))
            | TyKind::Contract(_)
            | TyKind::Enum(_)
    )
}

fn struct_constructor<'gcx>(gcx: Gcx<'gcx>, ty: Ty<'gcx>, id: hir::StructId) -> Ty<'gcx> {
    gcx.mk_builtin_fn(
        &gcx.struct_field_types(id)
            .iter()
            .map(|&ty| ty.with_loc_if_ref(gcx, DataLocation::Memory))
            .collect::<Vec<_>>(),
        hir::StateMutability::Pure,
        &[ty.with_loc(gcx, DataLocation::Memory)],
    )
}
