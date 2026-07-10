use crate::{
    builtins::{Builtin, members},
    eval::{ConstValue, ConstantEvaluator, EvalErrorKind},
    hir::{self, Visit},
    ty::{
        CallableParamSource, Gcx, ResolvedCallee, Ty, TyConvertError, TyFn, TyFnKind, TyKind,
        TypeckResults,
    },
};
use alloy_primitives::U256;
use solar_ast::{
    DataLocation, ElementaryType, LitKind, Span, StateMutability, TypeSize, UserDefinableOperator,
};
use solar_data_structures::{Never, pluralize, smallvec::SmallVec};
use solar_interface::{
    Ident, Symbol,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    kw, sym,
};
use std::ops::ControlFlow;

mod yul;

#[derive(Clone, Copy)]
enum AbiDecodeArg {
    Data,
    Types,
}

pub(super) fn check<'gcx>(gcx: Gcx<'gcx>, source: hir::SourceId) -> TypeckResults<'gcx> {
    let mut checker = TypeChecker::new(gcx, source);
    let _ = checker.visit_nested_source(source);
    checker.results
}

struct TypeChecker<'gcx> {
    gcx: Gcx<'gcx>,
    source: hir::SourceId,
    contract: Option<hir::ContractId>,
    function: Option<hir::FunctionId>,
    construction_context: u32,

    results: TypeckResults<'gcx>,

    lvalue_context: Option<Result<(), NotLvalueReason>>,

    /// Whether we're directly inside an emit statement (for the immediate call only).
    in_emit: bool,
    /// Whether we're directly inside a revert statement (for the immediate call only).
    in_revert: bool,
    /// Whether we're checking expressions lowered from inline assembly.
    in_yul: bool,
}

#[derive(Clone, Copy)]
enum NotLvalueReason {
    Constant,
    Immutable,
    CalldataArray,
    CalldataStruct,
    FixedBytesIndex,
    ArrayLength,
    Generic,
}

impl<'gcx> TypeChecker<'gcx> {
    fn new(gcx: Gcx<'gcx>, source: hir::SourceId) -> Self {
        Self {
            gcx,
            source,
            contract: None,
            function: None,
            construction_context: 0,
            results: Default::default(),
            lvalue_context: None,
            in_emit: false,
            in_revert: false,
            in_yul: false,
        }
    }

    fn dcx(&self) -> &'gcx DiagCtxt {
        self.gcx.dcx()
    }

    fn check_storage_layout_base_slot(&mut self, slot: &'gcx hir::Expr<'gcx>) {
        if matches!(slot.kind, hir::ExprKind::Lit(lit) if matches!(lit.kind, LitKind::Address(_))) {
            self.dcx()
                .emit_err(slot.span, "base slot of storage layout must evaluate to an integer");
            return;
        }

        if let hir::ExprKind::Binary(lhs, op, rhs) = slot.kind
            && op.kind == hir::BinOpKind::Div
        {
            let mut evaluator = ConstantEvaluator::new(self.gcx);
            if let Ok(lhs) = evaluator.try_eval(lhs)
                && let Ok(rhs) = evaluator.try_eval(rhs)
                && let Ok(remainder) = lhs.binop(rhs, hir::BinOpKind::Rem)
                && !remainder.is_zero()
            {
                self.dcx()
                    .emit_err(slot.span, "base slot of storage layout must evaluate to an integer");
                return;
            }
        }

        let mut evaluator = ConstantEvaluator::new(self.gcx);
        match evaluator.try_eval_value(slot) {
            Ok(ConstValue::Integer(value)) => {
                if value.as_u256().is_none() {
                    self.dcx().emit_err(slot.span, "base slot of storage layout evaluates to a value outside the range of type `uint256`");
                }
            }
            Ok(ConstValue::Bool(_)) => {
                self.dcx()
                    .emit_err(slot.span, "base slot of storage layout must evaluate to an integer");
            }
            Ok(ConstValue::String(_)) => {
                let err = EvalErrorKind::UnsupportedLiteral.spanned(slot.span);
                evaluator.emit_eval_error(slot, err);
            }
            Err(err) => {
                evaluator.emit_eval_error(slot, err);
            }
        }
    }

    fn get(&self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        self.results.expr_types[&expr.id]
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
            let _ = self.check_expected(expr, ty, expected);
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
        match expr.kind {
            hir::ExprKind::Array(exprs) => {
                let mut common = expected.and_then(|arr| arr.base_type(self.gcx));
                let mut guar: Option<ErrorGuaranteed> = None;
                for (i, expr) in exprs.iter().enumerate() {
                    let expr_ty = self.check_expr(expr);
                    if (i == 0 || common.is_some())
                        && let None = expr_ty.mobile(self.gcx)
                    {
                        let g = self.dcx().emit_err(expr.span, "invalid mobile type");
                        guar.get_or_insert(g);
                    }
                    if let Some(common_ty) = common {
                        common = common_ty.common_type(expr_ty, self.gcx);
                    } else if i == 0 {
                        common = expr_ty.mobile(self.gcx);
                    }
                }
                if let Some(guar) = guar {
                    return self.gcx.mk_ty_err(guar);
                }
                if let Some(common) = common {
                    if common.has_mapping(self.gcx) {
                        let msg = format!(
                            "type `{}` is only valid in storage because it contains a (nested) mapping",
                            common.display(self.gcx),
                        );
                        self.gcx.mk_ty_err(self.dcx().emit_err(expr.span, msg))
                    } else if !common.nameable() {
                        self.gcx.mk_ty_err(
                            self.dcx()
                                .err("cannot infer nameable array element type")
                                .span(expr.span)
                                .help("add an explicit type conversion for the first element")
                                .emit(),
                        )
                    } else {
                        self.gcx
                            .mk_ty(TyKind::Array(common, U256::from(exprs.len())))
                            .with_loc(self.gcx, DataLocation::Memory)
                    }
                } else {
                    self.gcx.mk_ty_err(
                        self.dcx().emit_err(expr.span, "cannot infer array element type"),
                    )
                }
            }
            hir::ExprKind::Assign(lhs, op, rhs) => {
                let lhs = lhs.peel_parens();
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
                    self.check_tuple_assign_rhs(lhs, ty, rhs);
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

                // When both operands are IntLiteral, evaluate the expression to preserve
                // literal type through binary operations (needed for -(1 + 2) to work).
                if let (TyKind::IntLiteral(..), TyKind::IntLiteral(..)) = (lhs.kind, rhs.kind)
                    && !op.kind.is_cmp()
                    && let Some(lit_ty) = self.try_eval_int_literal_expr(expr)
                {
                    return lit_ty;
                }

                self.check_binop(lhs_e, lhs, rhs_e, rhs, op, false)
            }
            hir::ExprKind::Call(callee, ref args, opts) => {
                let mut callee_ty = if let hir::ExprKind::Member(receiver, ident) = callee.kind {
                    self.check_member_call_callee(callee, receiver, ident, args)
                } else if let hir::ExprKind::Ident(res) = callee.kind {
                    self.check_ident_call_callee(callee, res, args)
                } else {
                    self.check_expr(callee)
                };
                if let Some(opts) = opts {
                    callee_ty = self.check_call_options(callee_ty, opts.args, opts.span);
                }

                let callee_signature = self.gcx.callable_signature_of_ty(callee_ty);
                let callee_param_source =
                    callee_signature.and_then(|signature| signature.param_source);
                if let TyKind::Type(_) = callee_ty.kind
                    && let Some(signature) = callee_signature
                {
                    // Get the function type for struct constructors, keeping field names.
                    callee_ty = self.gcx.mk_builtin_fn(
                        signature.parameters,
                        StateMutability::Pure,
                        signature.returns,
                    );
                }

                let ty = match callee_ty.kind {
                    TyKind::Fn(f) => {
                        if f.is_declaration() {
                            let message = if f
                                .function_id
                                .is_some_and(|id| self.gcx.hir.function(id).body.is_none())
                            {
                                "cannot call unimplemented base function"
                            } else {
                                "cannot call function via contract type name"
                            };
                            return self.gcx.mk_ty_err(self.dcx().emit_err(expr.span, message));
                        }
                        if self.results.builtin_callee(callee.id) == Some(Builtin::AbiDecode) {
                            let args_result = self.check_abi_decode_call_args(expr.span, args);
                            if let Err(guar) = args_result {
                                return self.gcx.mk_ty_err(guar);
                            }
                            return self.abi_decode_return_type(args);
                        }

                        let builtin = self.results.builtin_callee(callee.id);
                        if builtin != Some(Builtin::Require) {
                            let _ = self.check_call_args(
                                expr.span,
                                args,
                                f.parameters,
                                callee_param_source,
                            );
                        }
                        if let Some(builtin) = builtin {
                            let _ = self.check_builtin_call_args(expr.span, args, builtin);
                        }
                        self.fn_call_return_type(f.returns)
                    }
                    TyKind::Type(to) => self.check_explicit_cast(expr.span, to, args),
                    TyKind::Event(param_tys, _) => {
                        if !self.in_emit {
                            self.dcx().emit_err(
                                expr.span,
                                "event invocations have to be prefixed by `emit`",
                            );
                        }
                        // Clear context so nested calls in args are not considered in emit/revert.
                        self.in_emit = false;
                        self.in_revert = false;
                        let _ =
                            self.check_call_args(expr.span, args, param_tys, callee_param_source);
                        self.gcx.types.unit
                    }
                    TyKind::Error(param_tys, _) => {
                        if !self.in_revert {
                            self.dcx().emit_err(
                                expr.span,
                                "errors can only be used with revert statements",
                            );
                        }
                        // Clear context so nested calls in args are not considered in emit/revert.
                        self.in_emit = false;
                        self.in_revert = false;
                        let _ =
                            self.check_call_args(expr.span, args, param_tys, callee_param_source);
                        self.gcx.types.unit
                    }
                    TyKind::Err(_) => callee_ty,
                    _ => {
                        let msg =
                            format!("expected function, found `{}`", callee_ty.display(self.gcx));
                        let mut err = self.dcx().err(msg).span(callee.span);
                        err = err.span_note(expr.span, "call expression requires function");
                        self.gcx.mk_ty_err(err.emit())
                    }
                };

                // No-argument storage array `.push()` is the only call that can be an lvalue.
                if self.results.builtin_callee(callee.id) != Some(Builtin::ArrayPush0) {
                    self.try_set_not_lvalue(NotLvalueReason::Generic);
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
                let res = if let Some(expected) = expected {
                    self.resolve_overloads_with_expected(res, expected, expr.span)
                } else {
                    self.resolve_overloads(res, expr.span)
                };
                if let Some(reason) = self.res_not_lvalue_reason(res) {
                    self.try_set_not_lvalue(reason);
                }
                let ty = self.type_of_res(res);
                if self.in_yul {
                    match self.check_yul_external_ident(res, ty, expr.span) {
                        Ok(true) => return self.gcx.types.uint(256),
                        Ok(false) => {}
                        Err(guar) => return self.gcx.mk_ty_err(guar),
                    }
                }
                ty
            }
            hir::ExprKind::Index(lhs, index) => {
                let ty = self.check_expr_outside_lvalue_context(lhs, None);
                if ty.references_error() {
                    return ty;
                }
                if ty.loc() == Some(DataLocation::Calldata) {
                    self.try_set_not_lvalue(NotLvalueReason::CalldataArray);
                }
                if matches!(ty.peel_refs().kind, TyKind::Elementary(ElementaryType::FixedBytes(_)))
                {
                    self.try_set_not_lvalue(NotLvalueReason::FixedBytesIndex);
                }
                if let Some((index_ty, result_ty)) = self.index_types(ty) {
                    // Index expression.
                    if let Some(index) = index {
                        let _ = self.check_expr_outside_lvalue_context(index, Some(index_ty));
                    } else {
                        self.dcx().emit_err(expr.span, "index expression cannot be omitted");
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
                    self.gcx.mk_ty_err(self.dcx().emit_err(expr.span, msg))
                }
            }
            hir::ExprKind::Slice(lhs, start, end) => {
                let ty = self.check_expr(lhs);
                if !ty.is_sliceable() {
                    self.dcx().emit_err(expr.span, "can only slice arrays");
                } else if !is_calldata_sliceable(ty) {
                    self.dcx().emit_err(expr.span, "can only slice dynamic calldata arrays");
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
                    self.gcx.mk_ty_err(self.dcx().emit_err(expr.span, "cannot index"))
                }
            }
            hir::ExprKind::Lit(lit) if self.in_yul => self.check_yul_lit(lit),
            hir::ExprKind::Lit(lit) => self.gcx.type_of_lit(lit),
            hir::ExprKind::Member(receiver, ident) => {
                let receiver_ty = self.check_expr_outside_lvalue_context(receiver, None);
                if receiver_ty.references_error() {
                    return receiver_ty;
                }
                if ident.name == Symbol::DUMMY {
                    return self.gcx.mk_ty_misc_err();
                }

                let possible_members = self
                    .gcx
                    .members_of(receiver_ty, self.source, self.contract)
                    .filter(|m| m.name == ident.name)
                    .collect::<SmallVec<[_; 4]>>();

                let ty = match self.select_member_access(&possible_members) {
                    Ok(member) => {
                        self.register_resolved_member(expr, receiver_ty, member);
                        member.ty
                    }
                    Err(MemberAccessError::NotFound) => {
                        let msg = format!(
                            "member `{ident}` not found on type `{}`",
                            receiver_ty.display(self.gcx)
                        );
                        // TODO: Did you mean ...?
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                    Err(MemberAccessError::Ambiguous) => {
                        let msg = format!(
                            "member `{ident}` not unique on type `{}`",
                            receiver_ty.display(self.gcx)
                        );
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                };

                // Validate lvalue.
                let not_lvalue_reason = match receiver_ty.kind {
                    _ if matches!(
                        receiver_ty.peel_refs().kind,
                        TyKind::Array(..) | TyKind::DynArray(_)
                    ) && possible_members.len() == 1
                        && possible_members[0].name == sym::length =>
                    {
                        Some(NotLvalueReason::ArrayLength)
                    }
                    TyKind::Ref(inner, d) if d.is_calldata() => {
                        let reason = if matches!(inner.kind, TyKind::Struct(_)) {
                            NotLvalueReason::CalldataStruct
                        } else {
                            NotLvalueReason::CalldataArray
                        };
                        Some(reason)
                    }
                    TyKind::Ref(inner, _) if matches!(inner.kind, TyKind::Struct(_)) => None,
                    TyKind::Type(ty)
                        if matches!(ty.kind, TyKind::Contract(_))
                            && let [member] = possible_members.as_slice()
                            && let Some(res) = member.res
                            && res.as_variable().is_some() =>
                    {
                        self.res_not_lvalue_reason(res)
                    }
                    _ => Some(NotLvalueReason::Generic),
                };
                if let Some(reason) = not_lvalue_reason {
                    self.try_set_not_lvalue(reason);
                }

                ty
            }
            hir::ExprKind::New(ref hir_ty) => {
                let ty = self.gcx.type_of_hir_ty(hir_ty);
                match ty.kind {
                    TyKind::Contract(id) => {
                        let c = self.gcx.hir.contract(id);
                        if !c.kind.is_contract() {
                            let msg = if c.kind.is_library() {
                                "cannot instantiate libraries".to_owned()
                            } else {
                                format!("cannot instantiate {}s", c.kind)
                            };
                            self.gcx.mk_ty_err(self.dcx().emit_err(hir_ty.span, msg))
                        } else {
                            let mut parameters: &[Ty<'_>] = &[];
                            let mut sm = hir::StateMutability::NonPayable;
                            if let Some(ctor) = c.ctor {
                                let f = self.gcx.hir.function(ctor);
                                parameters = self.gcx.mk_item_tys(f.parameters);
                                sm = f.state_mutability;
                            }
                            self.gcx.mk_creation_fn(parameters, sm, &[ty])
                        }
                    }
                    TyKind::Array(..) => {
                        let mut err = self
                            .dcx()
                            .err("length has to be placed in parentheses after the array type for new expression")
                            .span(hir_ty.span);
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
                        if ty.has_mapping(self.gcx) {
                            self.gcx.mk_ty_err(
                                self.dcx().emit_err(hir_ty.span, "cannot instantiate mappings"),
                            )
                        } else if ty.contains_library(self.gcx) {
                            self.gcx.mk_ty_err(
                                self.dcx().emit_err(hir_ty.span, "invalid use of a library name"),
                            )
                        } else {
                            let ty = ty.with_loc(self.gcx, DataLocation::Memory);
                            self.gcx.mk_builtin_fn(
                                &[self.gcx.types.uint(256)],
                                hir::StateMutability::Pure,
                                &[ty],
                            )
                        }
                    }
                    TyKind::Err(_) => ty,
                    _ => self.gcx.mk_ty_err(
                        self.dcx().emit_err(hir_ty.span, "expected contract or dynamic array type"),
                    ),
                }
            }
            hir::ExprKind::Payable(expr) => {
                let ty = self.check_expr(expr);
                if ty.references_error() {
                    return ty;
                }

                let target_ty = self.gcx.types.address_payable;
                match ty.try_convert_explicit_to(target_ty, self.gcx) {
                    Ok(target_ty) => target_ty,
                    Err(err) => {
                        let mut diag =
                            self.dcx().err("invalid explicit type conversion").span(expr.span);
                        diag = diag.span_label(expr.span, err.message(ty, target_ty, self.gcx));
                        self.gcx.mk_ty_err(diag.emit())
                    }
                }
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
                            guar = Some(self.dcx().emit_err(true_.span, "invalid true type"));
                        }
                        if false_ty.is_none() {
                            guar = Some(self.dcx().emit_err(false_.span, "invalid false type"));
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
                            this.dcx().emit_err(span, "tuple components cannot be empty"),
                        )
                    };
                    if let Some(expr) = expr_opt {
                        let ty = if self.in_lvalue() {
                            self.require_lvalue(expr)
                        } else {
                            self.check_expr(expr)
                        };
                        if ty.is_unit() { empty_err(self, expr.span) } else { ty }
                    } else if self.in_lvalue() {
                        self.gcx.types.unit
                    } else {
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
                    self.gcx.mk_ty_err(self.dcx().emit_err(hir_ty.span, "invalid type"))
                }
            }
            hir::ExprKind::Type(ref ty) => {
                self.gcx.mk_ty(TyKind::Type(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Unary(op, inner) => {
                // For negation, don't propagate expected type to the inner expression
                // because we'll modify the type (flipping the sign for int literals).
                let propagate_expected = op.kind != hir::UnOpKind::Neg
                    || !matches!(expected, Some(ty) if ty.is_signed());
                let ty = if op.kind.has_side_effects() {
                    self.require_lvalue(inner)
                } else if propagate_expected {
                    self.check_expr_with(inner, expected)
                } else {
                    self.check_expr(inner)
                };
                if valid_unop(ty, op.kind) {
                    if op.kind == hir::UnOpKind::Neg
                        && let TyKind::IntLiteral(..) = ty.kind
                        && let Some(lit_ty) = self.try_eval_int_literal_expr(expr)
                    {
                        return lit_ty;
                    }
                    if op.kind == hir::UnOpKind::Neg
                        && let TyKind::IntLiteral(neg, size, fixed_bytes_size) = ty.kind
                    {
                        let fixed_bytes_size =
                            fixed_bytes_size.filter(|&size| size == TypeSize::ZERO);
                        return self.gcx.mk_ty(TyKind::IntLiteral(!neg, size, fixed_bytes_size));
                    }
                    ty
                } else if let Some(ty) = self.check_user_unop(expr.span, ty, op.kind) {
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
            hir::ExprKind::YulMember(expr, member) => self.check_yul_member(expr, member),
            hir::ExprKind::Err(guar) => self.gcx.mk_ty_err(guar),
        }
    }

    fn check_assign(&self, ty: Ty<'gcx>, expr: &'gcx hir::Expr<'gcx>) {
        // https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1421
        if let hir::ExprKind::Tuple(components) = &expr.kind {
            if components.is_empty() {
                self.dcx().emit_err(expr.span, "empty tuple on the left hand side");
                return;
            }
            let types =
                if let TyKind::Tuple(types) = ty.kind { types } else { std::slice::from_ref(&ty) };
            for (component, &component_ty) in components.iter().zip(types.iter()) {
                if let Some(component) = component {
                    self.check_assign(component_ty, component);
                }
            }
            return;
        }

        // Types containing mappings cannot be assigned to, unless the lvalue is a local/return
        // variable (local storage pointers are OK).
        if ty.has_mapping(self.gcx) && !self.is_local_or_return_variable(expr) {
            self.dcx().emit_err(
                expr.span,
                "types in storage containing (nested) mappings cannot be assigned to",
            );
        }
    }

    fn check_tuple_assign_rhs(
        &mut self,
        lhs: &'gcx hir::Expr<'gcx>,
        lhs_ty: Ty<'gcx>,
        rhs: &'gcx hir::Expr<'gcx>,
    ) {
        let hir::ExprKind::Tuple(lhs_components) = &lhs.kind else { return };
        let lhs_types = if let TyKind::Tuple(types) = lhs_ty.kind {
            types
        } else {
            std::slice::from_ref(&lhs_ty)
        };

        let rhs_ty = self.check_expr(rhs);
        let rhs_types = if let TyKind::Tuple(types) = rhs_ty.kind {
            types
        } else {
            std::slice::from_ref(&rhs_ty)
        };

        if lhs_components.len() != rhs_types.len() {
            self.dcx().emit_err_label(
                lhs.span,
                "mismatched number of components",
                rhs.span,
                format!(
                    "expected a tuple with {} element{}, found one with {} element{}",
                    lhs_components.len(),
                    pluralize!(lhs_components.len()),
                    rhs_types.len(),
                    pluralize!(rhs_types.len())
                ),
            );
            return;
        }

        let rhs_components =
            if let hir::ExprKind::Tuple(components) = &rhs.kind { Some(*components) } else { None };
        for (i, (&lhs_component, &lhs_component_ty)) in
            lhs_components.iter().zip(lhs_types).enumerate()
        {
            if let Some(_) = lhs_component
                && let Some(rhs_component) = rhs_components.and_then(|components| components[i])
            {
                let _ = self.check_expected(rhs_component, rhs_types[i], lhs_component_ty);
            }
        }
    }

    /// Returns true if the expression refers to a local or return variable.
    fn is_local_or_return_variable(&self, expr: &'gcx hir::Expr<'gcx>) -> bool {
        if let hir::ExprKind::Ident(res_slice) = &expr.kind {
            let res = self.resolve_overloads(res_slice, expr.span);
            if let hir::Res::Item(hir::ItemId::Variable(var_id)) = res {
                let var = self.gcx.hir.variable(var_id);
                return var.is_local_or_return();
            }
        }
        false
    }

    /// Tries to evaluate an expression made up of int literals.
    ///
    /// Returns the resulting IntLiteral type if successful, or None if evaluation fails.
    /// This is used to preserve literal type through literal expressions.
    fn try_eval_int_literal_expr(&self, expr: &'gcx hir::Expr<'gcx>) -> Option<Ty<'gcx>> {
        let mut evaluator = ConstantEvaluator::new(self.gcx);
        let result = evaluator.try_eval(expr).ok()?;
        let compatible_fixed_bytes = result.is_zero().then_some(TypeSize::ZERO);
        self.gcx.mk_ty_int_literal_with_fixed_bytes(
            result.is_negative(),
            result.bit_len(),
            compatible_fixed_bytes,
        )
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
        if let Some(common) = common
            && !(assign && common != lhs)
        {
            return if op.kind.is_cmp() { self.gcx.types.bool } else { common };
        }
        if !assign && let Some(ty) = self.check_user_binop(op.span, lhs, rhs, op.kind) {
            return ty;
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

    fn check_user_unop(&self, span: Span, ty: Ty<'gcx>, op: hir::UnOpKind) -> Option<Ty<'gcx>> {
        let op = UserDefinableOperator::from_unop(op)?;
        let mut functions = WantOne::Zero;
        self.gcx.for_each_user_operator(
            ty,
            self.source,
            self.contract,
            op,
            true,
            &mut |function| {
                functions.push(function);
            },
        );
        self.check_user_operator(span, functions)
    }

    fn check_user_binop(
        &self,
        span: Span,
        lhs: Ty<'gcx>,
        rhs: Ty<'gcx>,
        op: hir::BinOpKind,
    ) -> Option<Ty<'gcx>> {
        let op = UserDefinableOperator::from_binop(op)?;
        let mut functions = WantOne::Zero;
        self.gcx.for_each_user_operator(
            lhs,
            self.source,
            self.contract,
            op,
            false,
            &mut |function| {
                let TyKind::Fn(function_ty) = self.gcx.type_of_item(function.into()).kind else {
                    return;
                };
                if rhs.convert_implicit_to(function_ty.parameters[1], self.gcx) {
                    functions.push(function);
                }
            },
        );
        self.check_user_operator(span, functions)
    }

    fn check_user_operator(
        &self,
        span: Span,
        functions: WantOne<hir::FunctionId>,
    ) -> Option<Ty<'gcx>> {
        match functions {
            WantOne::Zero => None,
            WantOne::One(function) => {
                let TyKind::Fn(function_ty) = self.gcx.type_of_item(function.into()).kind else {
                    unreachable!()
                };
                Some(self.fn_call_return_type(function_ty.returns))
            }
            WantOne::Many => {
                Some(self.gcx.mk_ty_err(
                    self.dcx().emit_err(
                        span,
                        "user-defined operator has more than one matching definition",
                    ),
                ))
            }
        }
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
            TyKind::Slice(array) => (self.gcx.types.uint(256), array.base_type(self.gcx)?),
            TyKind::Elementary(ElementaryType::Bytes)
            | TyKind::Elementary(ElementaryType::FixedBytes(_)) => {
                (self.gcx.types.uint(256), self.gcx.types.fixed_bytes(1))
            }
            TyKind::Mapping(key, value) => (key, value.with_loc_if_ref_opt(self.gcx, loc)),
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
        if matches!(to.kind, TyKind::Super(_)) {
            return self
                .gcx
                .mk_ty_err(self.dcx().err("cannot convert to the super type").span(span).emit());
        }
        let WantOne::One(from_expr) = args.exprs().collect::<WantOne<_>>() else {
            return self.gcx.mk_ty_err(
                self.dcx().emit_err(args.span, "expected exactly one unnamed argument"),
            );
        };
        let from = self.check_expr(from_expr);
        match from.try_convert_explicit_to(to, self.gcx) {
            Ok(result_ty) => result_ty,
            Err(err) => {
                let mut diag = self.dcx().err("invalid explicit type conversion").span(span);
                diag = diag.span_label(span, err.message(from, to, self.gcx));
                self.gcx.mk_ty_err(diag.emit())
            }
        }
    }

    #[track_caller]
    fn check_expected(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        actual: Ty<'gcx>,
        expected: Ty<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        match self.expr_matches_expected(expr, actual, expected) {
            Ok(()) => Ok(()),
            Err(err) => {
                let mut diag = self.dcx().err("mismatched types").span(expr.span);
                diag = diag.span_label(expr.span, err.message(actual, expected, self.gcx));
                Err(diag.emit())
            }
        }
    }

    fn expr_matches_expected(
        &self,
        expr: &'gcx hir::Expr<'gcx>,
        actual: Ty<'gcx>,
        expected: Ty<'gcx>,
    ) -> Result<(), TyConvertError> {
        let Err(err) = actual.try_convert_implicit_to(expected, self.gcx) else { return Ok(()) };

        if let TyKind::Tuple([ty]) = expected.kind
            && matches!(ty.kind, TyKind::Variadic)
            && matches!(expr.kind, hir::ExprKind::Tuple(_))
        {
            return Ok(());
        }

        Err(err)
    }

    fn fn_call_return_type(&self, returns: &'gcx [Ty<'gcx>]) -> Ty<'gcx> {
        match returns {
            [] => self.gcx.types.unit,
            [ty] => *ty,
            tys => self.gcx.mk_ty_tuple(tys),
        }
    }

    fn check_call_args(
        &mut self,
        call_span: Span,
        args: &hir::CallArgs<'gcx>,
        param_tys: &[Ty<'gcx>],
        param_names: Option<CallableParamSource>,
    ) -> Result<(), ErrorGuaranteed> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                self.check_positional_call_args(call_span, args.span, exprs, param_tys)
            }
            hir::CallArgsKind::Named(named_args) => {
                self.check_named_call_args(call_span, args.span, named_args, param_tys, param_names)
            }
        }
    }

    fn check_builtin_call_args(
        &mut self,
        call_span: Span,
        args: &hir::CallArgs<'gcx>,
        builtin: Builtin,
    ) -> Result<(), ErrorGuaranteed> {
        if builtin == Builtin::Require {
            return self.check_require_args(call_span, args);
        }

        let hir::CallArgsKind::Unnamed(exprs) = args.kind else { return Ok(()) };
        match builtin {
            Builtin::StringConcat => {
                let mut result = Ok(());
                for expr in exprs {
                    let ty = self.check_expr_once(expr);
                    if !valid_string_concat_arg(ty) {
                        result = result.and(Err(self.dcx().emit_err_label(
                            expr.span,
                            "`string.concat` arguments must be strings",
                            expr.span,
                            format!("found `{}`", ty.display(self.gcx)),
                        )));
                    }
                }
                result
            }
            Builtin::BytesConcat => {
                let mut result = Ok(());
                for expr in exprs {
                    let ty = self.check_expr_once(expr);
                    if !valid_bytes_concat_arg(ty) {
                        result = result.and(Err(self.dcx().emit_err_label(
                            expr.span,
                            "`bytes.concat` arguments must be bytes or fixed bytes",
                            expr.span,
                            format!("found `{}`", ty.display(self.gcx)),
                        )));
                    }
                }
                result
            }
            Builtin::AbiEncode | Builtin::AbiEncodePacked => {
                self.check_abi_encodable_args(exprs, builtin)
            }
            Builtin::AbiEncodeWithSelector | Builtin::AbiEncodeWithSignature => {
                if let Some((_, exprs)) = exprs.split_first() {
                    self.check_abi_encodable_args(exprs, builtin)
                } else {
                    Ok(())
                }
            }
            Builtin::AbiEncodeCall => self.check_abi_encode_call_args(call_span, args.span, exprs),
            Builtin::AbiDecode => Ok(()),
            _ => Ok(()),
        }
    }

    fn check_require_args(
        &mut self,
        call_span: Span,
        args: &hir::CallArgs<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        let hir::CallArgsKind::Unnamed(exprs) = args.kind else {
            let hir::CallArgsKind::Named(named_args) = args.kind else { unreachable!() };
            let guar = self.dcx().emit_err(
                args.span,
                "named arguments cannot be used for functions that take arbitrary parameters",
            );
            for arg in named_args {
                let _ = self.check_expr_once(&arg.value);
            }
            return Err(guar);
        };

        match exprs {
            [condition] => {
                let actual = self.check_expr_once(condition);
                self.check_expected(condition, actual, self.gcx.types.bool)
            }
            [condition, message_or_error] => {
                let actual = self.check_expr_once(condition);
                let result = self.check_expected(condition, actual, self.gcx.types.bool);
                result.and(self.check_require_message_or_error(message_or_error))
            }
            _ => Err(self.dcx().emit_err_label(
                call_span,
                format!(
                    "wrong argument count for function call: {} arguments given but expected 1 or 2",
                    exprs.len()
                ),
                args.span,
                format!("expected 1 or 2 arguments, found {}", exprs.len()),
            )),
        }
    }

    fn check_require_message_or_error(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        if self.results.expr_types.contains_key(&expr.id) {
            let ty = self.get(expr);
            return if ty.is_unit() {
                Ok(())
            } else {
                self.check_expected(expr, ty, self.gcx.types.string_ref.memory)
            };
        }

        let hir::ExprKind::Call(callee, args, opts) = expr.kind else {
            let actual = self.check_expr_once(expr);
            return self.check_expected(expr, actual, self.gcx.types.string_ref.memory);
        };
        if let Some(opts) = opts {
            let callee_ty = self.check_expr(callee);
            let _ = self.check_call_options(callee_ty, opts.args, opts.span);
            return self.check_expected(expr, callee_ty, self.gcx.types.string_ref.memory);
        }

        let hir::ExprKind::Ident(res) = callee.kind else {
            let actual = self.check_expr_once(expr);
            return self.check_expected(expr, actual, self.gcx.types.string_ref.memory);
        };
        let error_res = res
            .iter()
            .copied()
            .filter(|res| matches!(res, hir::Res::Item(hir::ItemId::Error(_))))
            .collect::<SmallVec<[_; 4]>>();
        if error_res.is_empty() {
            let actual = self.check_expr_once(expr);
            return self.check_expected(expr, actual, self.gcx.types.string_ref.memory);
        }

        let selected = match self.select_call_overload(&error_res, &args) {
            Ok(res) => res,
            Err(e) => {
                let msg = match e {
                    OverloadError::NotFound => "no matching declarations found",
                    OverloadError::Ambiguous => "no unique declarations found",
                };
                hir::Res::Err(self.dcx().emit_err(callee.span, msg))
            }
        };
        let callee_ty = self.type_of_res(selected);
        let TyKind::Error(param_tys, _) = callee_ty.kind else {
            return self.check_expected(expr, callee_ty, self.gcx.types.string_ref.memory);
        };
        let param_source = self
            .gcx
            .callable_signature_of_ty(callee_ty)
            .and_then(|signature| signature.param_source);
        self.results.resolved_callees.insert(callee.id, ResolvedCallee::new(selected, false));
        if !self.results.expr_types.contains_key(&callee.id) {
            self.register_ty(callee, callee_ty);
        }
        let result = self.check_call_args(expr.span, &args, param_tys, param_source);
        self.register_ty(expr, self.gcx.types.unit);
        result
    }

    fn check_abi_encodable_args(
        &mut self,
        exprs: &'gcx [hir::Expr<'gcx>],
        builtin: Builtin,
    ) -> Result<(), ErrorGuaranteed> {
        let mut result = Ok(());
        let is_packed = builtin == Builtin::AbiEncodePacked;
        for expr in exprs {
            let ty = self.check_expr_once(expr);
            if is_packed && matches!(ty.kind, TyKind::IntLiteral(..)) {
                result = result.and(Err(self
                    .dcx()
                    .err("cannot perform packed encoding for a literal")
                    .span(expr.span)
                    .help("convert it to an explicit type first")
                    .emit()));
                continue;
            }
            if is_packed && !type_supported_by_old_abi_encoder(ty) {
                result = result.and(Err(self.dcx().emit_err_label(
                    expr.span,
                    "type not supported in packed mode",
                    expr.span,
                    format!("found `{}`", ty.display(self.gcx)),
                )));
                continue;
            }
            if !valid_abi_encodable_arg(ty, self.gcx) {
                result = result.and(Err(self.dcx().emit_err_label(
                    expr.span,
                    format!("`{}` argument cannot be ABI-encoded", builtin.name()),
                    expr.span,
                    format!("found `{}`", ty.display(self.gcx)),
                )));
            }
        }
        result
    }

    fn check_abi_encode_call_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        exprs: &'gcx [hir::Expr<'gcx>],
    ) -> Result<(), ErrorGuaranteed> {
        let [function, arguments] = exprs else {
            return Err(self.dcx().emit_err_label(
                call_span,
                format!(
                    "wrong argument count for function call: {} arguments given but expected 2",
                    exprs.len()
                ),
                args_span,
                format!("expected 2 arguments, found {}", exprs.len()),
            ));
        };

        let function_ty = self.check_abi_encode_call_function_arg(function)?;

        let mut result = Ok(());
        let arguments_ty = self.check_expr_once(arguments);
        let TyKind::Tuple(_) = arguments_ty.kind else {
            if function_ty.parameters.len() != 1 {
                result = result.and(Err(self.dcx().emit_err(
                    arguments.span,
                    format!(
                        "wrong argument count for `abi.encodeCall`: 1 argument given but expected {}",
                        function_ty.parameters.len()
                    ),
                )));
            }
            if let Some(&expected) = function_ty.parameters.first() {
                result = result.and(self.check_expected(arguments, arguments_ty, expected));
            }
            return result;
        };

        let hir::ExprKind::Tuple(components) = arguments.kind else {
            return Err(self.dcx().emit_err(
                arguments.span,
                "second argument to `abi.encodeCall` must be an inline tuple",
            ));
        };

        let has_empty_component = components.iter().any(|component| component.is_none());
        if !has_empty_component && components.len() != function_ty.parameters.len() {
            result = result.and(Err(self.dcx().emit_err(
                arguments.span,
                format!(
                    "wrong argument count for `abi.encodeCall`: {} arguments given but expected {}",
                    components.len(),
                    function_ty.parameters.len()
                ),
            )));
        }

        for (component, &expected) in components.iter().zip(function_ty.parameters) {
            let Some(component) = component else {
                continue;
            };
            let actual = self.check_expr_once(component);
            result = result.and(self.check_expected(component, actual, expected));
        }
        result
    }

    fn check_abi_encode_call_function_arg(
        &mut self,
        function: &'gcx hir::Expr<'gcx>,
    ) -> Result<&'gcx TyFn<'gcx>, ErrorGuaranteed> {
        let ty = self.check_expr_once(function);
        let externally_callable_ty = ty.as_externally_callable_function(false, self.gcx);
        match externally_callable_ty.kind {
            TyKind::Fn(function_ty)
                if matches!(function_ty.kind, TyFnKind::External | TyFnKind::Declaration) =>
            {
                Ok(function_ty)
            }
            TyKind::Fn(function_ty) => Err(self.dcx().emit_err_label(
                function.span,
                abi_encode_call_function_kind_message(function_ty.kind),
                function.span,
                format!("found `{}`", ty.display(self.gcx)),
            )),
            TyKind::Event(..) => Err(self.dcx().emit_err_label(
                function.span,
                "first argument to `abi.encodeCall` cannot be an event",
                function.span,
                format!("found `{}`", ty.display(self.gcx)),
            )),
            TyKind::Error(..) => Err(self.dcx().emit_err_label(
                function.span,
                "first argument to `abi.encodeCall` cannot be an error",
                function.span,
                format!("found `{}`", ty.display(self.gcx)),
            )),
            _ => Err(self.dcx().emit_err_label(
                function.span,
                "first argument to `abi.encodeCall` must be a function",
                function.span,
                format!("found `{}`", ty.display(self.gcx)),
            )),
        }
    }

    fn check_abi_decode_call_args(
        &mut self,
        call_span: Span,
        args: &hir::CallArgs<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                self.check_abi_decode_positional_args(call_span, args.span, exprs)
            }
            hir::CallArgsKind::Named(named_args) => {
                self.check_abi_decode_named_args(call_span, args.span, named_args)
            }
        }
    }

    fn check_abi_decode_positional_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        exprs: &'gcx [hir::Expr<'gcx>],
    ) -> Result<(), ErrorGuaranteed> {
        let mut result = Ok(());
        result = result.and(self.check_exact_arg_count(call_span, args_span, exprs.len(), 2));

        if let Some(data) = exprs.first() {
            result = result.and(self.check_abi_decode_arg(AbiDecodeArg::Data, data));
        }
        if let Some(types) = exprs.get(1) {
            result = result.and(self.check_abi_decode_arg(AbiDecodeArg::Types, types));
        }
        for expr in exprs.iter().skip(2) {
            let _ = self.check_expr_once(expr);
        }

        result
    }

    fn check_abi_decode_named_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        named_args: &'gcx [hir::NamedArg<'gcx>],
    ) -> Result<(), ErrorGuaranteed> {
        let mut result = Ok(());
        result = result.and(self.check_exact_arg_count(call_span, args_span, named_args.len(), 2));

        let mut seen_names: SmallVec<[solar_interface::Symbol; 2]> = SmallVec::new();
        for arg in named_args {
            let arg_name = arg.name.name;
            if seen_names.contains(&arg_name) {
                result = result.and(Err(self
                    .dcx()
                    .emit_err(arg.name.span, format!("duplicate named argument `{arg_name}`"))));
                let _ = self.check_expr_once(&arg.value);
                continue;
            }
            seen_names.push(arg_name);

            if let Some(kind) = abi_decode_arg_kind(arg_name) {
                result = result.and(self.check_abi_decode_arg(kind, &arg.value));
            } else {
                result = result.and(Err(self.dcx().emit_err(
                    arg.name.span,
                    format!("named argument `{arg_name}` does not match function declaration"),
                )));
                let _ = self.check_expr_once(&arg.value);
            }
        }

        result
    }

    fn check_exact_arg_count(
        &self,
        call_span: Span,
        args_span: Span,
        found: usize,
        expected: usize,
    ) -> Result<(), ErrorGuaranteed> {
        if found == expected {
            return Ok(());
        }
        Err(self.dcx().emit_err_label(
            call_span,
            format!(
                "wrong argument count for function call: {found} arguments given but expected {expected}"
            ),
            args_span,
            format!("expected {expected} arguments, found {found}"),
        ))
    }

    fn check_abi_decode_arg(
        &mut self,
        kind: AbiDecodeArg,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        match kind {
            AbiDecodeArg::Data => {
                let actual = self.check_expr_once(expr);
                self.check_expected(expr, actual, self.gcx.types.bytes_ref.memory)
            }
            AbiDecodeArg::Types => self.check_abi_decode_types_arg(expr),
        }
    }

    fn check_abi_decode_types_arg(
        &mut self,
        types: &'gcx hir::Expr<'gcx>,
    ) -> Result<(), ErrorGuaranteed> {
        if matches!(types.kind, hir::ExprKind::Tuple(_)) {
            Ok(())
        } else {
            Err(self.dcx().emit_err(
                types.span,
                "the second argument to `abi.decode` must be a tuple of types",
            ))
        }
    }

    fn abi_decode_return_type(&mut self, args: &hir::CallArgs<'gcx>) -> Ty<'gcx> {
        let types_expr = match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                let [_, types_expr] = exprs else {
                    unreachable!("`abi.decode` should have exactly two checked arguments");
                };
                types_expr
            }
            hir::CallArgsKind::Named(named_args) => {
                let Some(arg) = named_args.iter().find(|arg| arg.name.name == sym::types) else {
                    unreachable!(
                        "`abi.decode` named arguments should be checked before deriving return type"
                    );
                };
                &arg.value
            }
        };
        let types = self.abi_decode_types(types_expr);
        self.fn_call_return_type(types)
    }

    fn abi_decode_types(&mut self, expr: &'gcx hir::Expr<'gcx>) -> &'gcx [Ty<'gcx>] {
        let hir::ExprKind::Tuple(type_exprs) = expr.kind else {
            let guar = self.dcx().emit_err(
                expr.span,
                "the second argument to `abi.decode` must be a tuple of types",
            );
            return self.gcx.mk_tys(&[self.gcx.mk_ty_err(guar)]);
        };

        let mut tys = Vec::with_capacity(type_exprs.len());
        for type_expr in type_exprs {
            let Some(type_expr) = type_expr else {
                let guar = self
                    .dcx()
                    .emit_err(expr.span, "`abi.decode` type tuple components cannot be empty");
                tys.push(self.gcx.mk_ty_err(guar));
                continue;
            };
            let ty = self.check_expr_once(type_expr);
            let TyKind::Type(ty) = ty.kind else {
                let guar = self
                    .dcx()
                    .emit_err(type_expr.span, "`abi.decode` type tuple components must be types");
                tys.push(self.gcx.mk_ty_err(guar));
                continue;
            };
            let mut ty = ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
            if matches!(ty.kind, TyKind::Elementary(ElementaryType::Address(false))) {
                ty = self.gcx.types.address_payable;
            }
            if !valid_abi_decodable_type(ty, self.gcx) {
                let guar = self.dcx().emit_err_label(
                    type_expr.span,
                    "decoding type not supported",
                    type_expr.span,
                    format!("found `{}`", ty.display(self.gcx)),
                );
                tys.push(self.gcx.mk_ty_err(guar));
                continue;
            }
            tys.push(ty);
        }
        self.gcx.mk_tys(&tys)
    }

    fn check_member_call_callee(
        &mut self,
        callee: &'gcx hir::Expr<'gcx>,
        receiver: &'gcx hir::Expr<'gcx>,
        ident: Ident,
        args: &hir::CallArgs<'gcx>,
    ) -> Ty<'gcx> {
        let receiver_ty = self.check_expr_outside_lvalue_context(receiver, None);
        if let Err(e) = receiver_ty.error_reported() {
            let ty = self.gcx.mk_ty_err(e);
            self.register_ty(callee, ty);
            return ty;
        }
        if ident.name == Symbol::DUMMY {
            let ty = self.gcx.mk_ty_misc_err();
            self.register_ty(callee, ty);
            return ty;
        }

        let possible_members = self
            .gcx
            .members_of(receiver_ty, self.source, self.contract)
            .filter(|m| m.name == ident.name)
            .collect::<SmallVec<[_; 4]>>();

        let ty = match self.select_member_call_overload(receiver_ty, &possible_members, args) {
            Ok(member) => {
                self.check_library_self_call(member, ident.span);
                if let Some(res) = member.res {
                    self.results
                        .resolved_callees
                        .insert(callee.id, ResolvedCallee::new(res, member.attached));
                }
                self.member_call_ty(receiver_ty, member)
            }
            Err(e) => {
                let msg = match e {
                    OverloadError::NotFound if possible_members.is_empty() => format!(
                        "member `{ident}` not found on type `{}`",
                        receiver_ty.display(self.gcx)
                    ),
                    OverloadError::NotFound => {
                        format!(
                            "no matching member `{ident}` found on type `{}`",
                            receiver_ty.display(self.gcx)
                        )
                    }
                    OverloadError::Ambiguous => {
                        format!(
                            "member `{ident}` not unique on type `{}`",
                            receiver_ty.display(self.gcx)
                        )
                    }
                };
                self.gcx.mk_ty_err(self.dcx().emit_err(ident.span, msg))
            }
        };
        self.register_ty(callee, ty);
        ty
    }

    fn select_member_access<'a>(
        &self,
        members: &'a [members::Member<'gcx>],
    ) -> Result<&'a members::Member<'gcx>, MemberAccessError> {
        match members {
            [] => Err(MemberAccessError::NotFound),
            [member] => Ok(member),
            // Mirror value-position overload resolution: a public state
            // variable or constant is accompanied by its getter function, and
            // the unique variable candidate wins for a non-call member access.
            [..] => {
                match members
                    .iter()
                    .filter(|member| member.res.is_some_and(|res| res.as_variable().is_some()))
                    .collect::<WantOne<_>>()
                {
                    WantOne::One(member) => Ok(member),
                    WantOne::Zero | WantOne::Many => Err(MemberAccessError::Ambiguous),
                }
            }
        }
    }

    fn register_resolved_member(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        receiver_ty: Ty<'gcx>,
        member: &members::Member<'gcx>,
    ) {
        let Some(resolved) = self.gcx.resolve_member_target(receiver_ty, member.name, member.res)
        else {
            return;
        };
        self.results.resolved_members.insert(expr.id, resolved);
    }

    fn check_ident_call_callee(
        &mut self,
        callee: &'gcx hir::Expr<'gcx>,
        res: &'gcx [hir::Res],
        args: &hir::CallArgs<'gcx>,
    ) -> Ty<'gcx> {
        let res = match self.select_call_overload(res, args) {
            Ok(res) => res,
            Err(e) => {
                let msg = match e {
                    OverloadError::NotFound => "no matching declarations found",
                    OverloadError::Ambiguous => "no unique declarations found",
                };
                hir::Res::Err(self.dcx().emit_err(callee.span, msg))
            }
        };
        let ty = self.type_of_res(res);
        self.results.resolved_callees.insert(callee.id, ResolvedCallee::new(res, false));
        self.register_ty(callee, ty);
        ty
    }

    fn check_library_self_call(&self, member: &members::Member<'gcx>, span: Span) {
        let Some(contract_id) = self.contract else { return };
        if !self.gcx.hir.contract(contract_id).kind.is_library() {
            return;
        }
        let Some(hir::Res::Item(hir::ItemId::Function(function_id))) = member.res else {
            return;
        };
        let function = self.gcx.hir.function(function_id);
        if function.contract == Some(contract_id)
            && function.visibility >= solar_ast::Visibility::Public
        {
            self.dcx().emit_err(span, "libraries cannot call their own functions externally");
        }
    }

    fn member_call_ty(&self, receiver_ty: Ty<'gcx>, member: &members::Member<'gcx>) -> Ty<'gcx> {
        if !member.attached {
            return member.ty;
        }

        let TyKind::Fn(function_ty) = member.ty.kind else { return member.ty };
        let Some((&self_ty, parameters)) = function_ty.parameters.split_first() else {
            return member.ty;
        };
        debug_assert!(receiver_ty.convert_implicit_to(self_ty, self.gcx));

        self.gcx.mk_ty_fn(TyFn {
            kind: function_ty.kind,
            parameters,
            returns: function_ty.returns,
            state_mutability: function_ty.state_mutability,
            function_id: function_ty.function_id,
            attached: false,
        })
    }

    fn select_call_overload(
        &mut self,
        res: &[hir::Res],
        args: &hir::CallArgs<'gcx>,
    ) -> Result<hir::Res, OverloadError> {
        match res {
            [] => unreachable!("no candidates for overload resolution"),
            &[res] => return Ok(res),
            _ => {}
        }
        if let Some(&res @ hir::Res::Err(_)) = res.iter().find(|res| res.is_err()) {
            return Ok(res);
        }

        let mut selected = SmallVec::<[_; 4]>::new();
        for &res in res {
            let ty = self.type_of_res(res);
            let Some(signature) = self.gcx.callable_signature_of_ty(ty) else {
                continue;
            };
            if self.call_args_match(args, signature.parameters, signature.param_source) {
                selected.push(res);
            }
        }
        match selected.as_slice() {
            [] => Err(OverloadError::NotFound),
            [res] => Ok(*res),
            selected => self.select_most_derived_function(selected).ok_or(OverloadError::Ambiguous),
        }
    }

    fn select_most_derived_function(&self, candidates: &[hir::Res]) -> Option<hir::Res> {
        let contract = self.contract?;
        let bases = self.gcx.hir.contract(contract).linearized_bases;

        let mut selected = None;
        let mut selected_depth = usize::MAX;
        let mut parameter_types = None;
        for &candidate in candidates {
            let hir::Res::Item(hir::ItemId::Function(id)) = candidate else { return None };
            let function = self.gcx.hir.function(id);
            let depth = bases.iter().position(|&base| Some(base) == function.contract)?;
            let params = self.gcx.item_parameter_types(id);
            if let Some(parameter_types) = parameter_types {
                if parameter_types != params {
                    return None;
                }
            } else {
                parameter_types = Some(params);
            }

            match depth.cmp(&selected_depth) {
                std::cmp::Ordering::Less => {
                    selected = Some(candidate);
                    selected_depth = depth;
                }
                std::cmp::Ordering::Equal => return None,
                std::cmp::Ordering::Greater => {}
            }
        }
        selected
    }

    fn select_member_call_overload<'a>(
        &mut self,
        receiver_ty: Ty<'gcx>,
        members: &'a [members::Member<'gcx>],
        args: &hir::CallArgs<'gcx>,
    ) -> Result<&'a members::Member<'gcx>, OverloadError> {
        match members {
            [] => return Err(OverloadError::NotFound),
            [member] => return Ok(member),
            _ => {}
        }

        let mut selected = WantOne::Zero;
        for member in members {
            if let Some(signature) = self.gcx.callable_signature_of_member(receiver_ty, member)
                && self.call_args_match(args, signature.parameters, signature.param_source)
            {
                selected.push(member);
            }
        }
        match selected {
            WantOne::Zero => Err(OverloadError::NotFound),
            WantOne::One(member) => Ok(member),
            WantOne::Many => Err(OverloadError::Ambiguous),
        }
    }

    fn call_args_match(
        &mut self,
        args: &hir::CallArgs<'gcx>,
        param_tys: &[Ty<'gcx>],
        param_names: Option<CallableParamSource>,
    ) -> bool {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => self.positional_call_args_match(exprs, param_tys),
            hir::CallArgsKind::Named(named_args) => {
                let Some(param_names) = param_names else { return false };
                let names = self.gcx.callable_param_names(param_names);
                if named_args.len() != param_tys.len() {
                    return false;
                }
                let mut seen = vec![false; param_tys.len()];
                for arg in named_args {
                    let Some(index) = names.iter().position(|&name| name == Some(arg.name.name))
                    else {
                        return false;
                    };
                    if seen[index] {
                        return false;
                    }
                    seen[index] = true;
                    if !self.arg_matches(&arg.value, param_tys[index]) {
                        return false;
                    }
                }
                true
            }
        }
    }

    fn arg_matches(&mut self, expr: &'gcx hir::Expr<'gcx>, param_ty: Ty<'gcx>) -> bool {
        let ty = self.check_expr_once(expr);
        ty.try_convert_implicit_to(param_ty, self.gcx).is_ok()
    }

    fn check_call_options(
        &mut self,
        ty: Ty<'gcx>,
        opts: &'gcx [hir::NamedArg<'gcx>],
        span: Span,
    ) -> Ty<'gcx> {
        let TyKind::Fn(f) = ty.kind else {
            for opt in opts {
                let _ = self.check_expr(&opt.value);
            }
            if !ty.references_error() {
                self.dcx().emit_err(span, "function call options can only be set on external function calls or contract creations");
            }
            return ty;
        };

        let creation = f.is_creation();
        if !creation && !f.is_external() && !f.is_bare_call() {
            self.dcx().emit_err(span, "function call options can only be set on external function calls or contract creations");
        }

        let mut gas_set = false;
        let mut value_set = false;
        let mut salt_set = false;
        for opt in opts {
            let name = opt.name.name;
            let duplicate = match name {
                kw::Gas => {
                    if creation {
                        self.dcx().emit_err(
                            opt.name.span,
                            "function call option `gas` cannot be used with `new`",
                        );
                    } else {
                        let _ = self.expect_ty(&opt.value, self.gcx.types.uint(256));
                    }
                    std::mem::replace(&mut gas_set, true)
                }
                sym::value => {
                    if f.kind == TyFnKind::BareDelegateCall {
                        self.dcx()
                            .emit_err(opt.name.span, "cannot set option `value` for delegatecall");
                    } else if f.kind == TyFnKind::BareStaticCall {
                        self.dcx()
                            .emit_err(opt.name.span, "cannot set option `value` for staticcall");
                    } else if f.state_mutability != StateMutability::Payable {
                        let msg = if creation
                            && let Some(ret) = f.returns.first()
                            && let TyKind::Contract(id) = ret.kind
                        {
                            let name = self.gcx.item_name(hir::ItemId::from(id)).name;
                            format!(
                                "cannot set option `value`, since the constructor of contract `{name}` is not payable"
                            )
                        } else {
                            "cannot set option `value` on a non-payable function type".to_string()
                        };
                        self.dcx().emit_err(opt.name.span, msg);
                    }
                    let _ = self.expect_ty(&opt.value, self.gcx.types.uint(256));
                    std::mem::replace(&mut value_set, true)
                }
                sym::salt => {
                    if !creation {
                        self.dcx().emit_err(
                            opt.name.span,
                            "function call option `salt` can only be used with `new`",
                        );
                    }
                    let _ = self.expect_ty(&opt.value, self.gcx.types.fixed_bytes(32));
                    std::mem::replace(&mut salt_set, true)
                }
                _ => {
                    self.dcx().emit_err(opt.name.span, format!("unknown call option `{name}`"));
                    let _ = self.check_expr(&opt.value);
                    false
                }
            };
            if duplicate {
                self.dcx().emit_err(opt.name.span, format!("duplicate call option `{name}`"));
            }
        }

        ty
    }

    fn check_positional_call_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        exprs: &'gcx [hir::Expr<'gcx>],
        param_tys: &[Ty<'gcx>],
    ) -> Result<(), ErrorGuaranteed> {
        let (fixed_params, variadic) = split_variadic_params(param_tys);
        let mut result = Ok(());

        if !variadic && exprs.len() != fixed_params.len() {
            result = result.and(Err(self.dcx().emit_err_label(
                call_span,
                format!(
                    "wrong argument count for function call: {} arguments given but expected {}",
                    exprs.len(),
                    fixed_params.len()
                ),
                args_span,
                format!(
                    "expected {} argument{}, found {}",
                    fixed_params.len(),
                    pluralize!(fixed_params.len()),
                    exprs.len()
                ),
            )));
        } else if variadic && exprs.len() < fixed_params.len() {
            result = result.and(Err(self.dcx().emit_err_label(
                call_span,
                format!(
                    "wrong argument count for function call: {} arguments given but expected at least {}",
                    exprs.len(),
                    fixed_params.len()
                ),
                args_span,
                format!(
                    "expected at least {} argument{}, found {}",
                    fixed_params.len(),
                    pluralize!(fixed_params.len()),
                    exprs.len()
                ),
            )));
        }

        let count = std::cmp::min(exprs.len(), fixed_params.len());
        for i in 0..count {
            let actual = self.check_expr_once(&exprs[i]);
            result = result.and(self.check_expected(&exprs[i], actual, fixed_params[i]));
        }
        for expr in exprs.iter().skip(count) {
            let _ = self.check_expr_once(expr);
        }

        result
    }

    fn positional_call_args_match(
        &mut self,
        exprs: &'gcx [hir::Expr<'gcx>],
        param_tys: &[Ty<'gcx>],
    ) -> bool {
        let (fixed_params, variadic) = split_variadic_params(param_tys);
        if (!variadic && exprs.len() != fixed_params.len())
            || (variadic && exprs.len() < fixed_params.len())
        {
            return false;
        }

        let count = std::cmp::min(exprs.len(), fixed_params.len());
        for i in 0..count {
            let actual = self.check_expr_once(&exprs[i]);
            if self.expr_matches_expected(&exprs[i], actual, fixed_params[i]).is_err() {
                return false;
            }
        }
        for expr in exprs.iter().skip(count) {
            let _ = self.check_expr_once(expr);
        }

        true
    }

    fn check_named_call_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        named_args: &'gcx [hir::NamedArg<'gcx>],
        param_tys: &[Ty<'gcx>],
        param_names: Option<CallableParamSource>,
    ) -> Result<(), ErrorGuaranteed> {
        let Some(param_names) = param_names else {
            let guar = self.dcx().emit_err(
                args_span,
                "named arguments cannot be used for functions that take arbitrary parameters",
            );
            for arg in named_args {
                let _ = self.check_expr(&arg.value);
            }
            return Err(guar);
        };

        let param_names = self.gcx.callable_param_names(param_names);
        debug_assert_eq!(param_tys.len(), param_names.len());
        let mut result = Ok(());

        if named_args.len() != param_tys.len() {
            result = result.and(Err(self.dcx().emit_err_label(
                call_span,
                format!(
                    "wrong argument count for function call: {} arguments given but expected {}",
                    named_args.len(),
                    param_tys.len()
                ),
                args_span,
                format!(
                    "expected {} argument{}, found {}",
                    param_tys.len(),
                    pluralize!(param_tys.len()),
                    named_args.len()
                ),
            )));
        }

        let mut seen_names: SmallVec<[solar_interface::Symbol; 8]> = SmallVec::new();

        for arg in named_args {
            let arg_name = arg.name.name;

            if seen_names.contains(&arg_name) {
                result = result.and(Err(self
                    .dcx()
                    .emit_err(arg.name.span, format!("duplicate named argument `{arg_name}`"))));
                let _ = self.check_expr_once(&arg.value);
                continue;
            }
            seen_names.push(arg_name);

            let param_idx = param_names.iter().position(|n| n.is_some_and(|name| name == arg_name));

            match param_idx {
                Some(idx) => {
                    let actual = self.check_expr_once(&arg.value);
                    result = result.and(self.check_expected(&arg.value, actual, param_tys[idx]));
                }
                None => {
                    result = result.and(Err(self.dcx().emit_err(
                        arg.name.span,
                        format!("named argument `{arg_name}` does not match function declaration"),
                    )));
                    let _ = self.check_expr_once(&arg.value);
                }
            }
        }

        result
    }

    fn check_expr_once(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        if let Some(&ty) = self.results.expr_types.get(&expr.id) {
            ty
        } else {
            self.check_expr(expr)
        }
    }

    fn check_expr_outside_lvalue_context(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        expected: Option<Ty<'gcx>>,
    ) -> Ty<'gcx> {
        let prev = self.lvalue_context.take();
        let ty = self.check_expr_with(expr, expected);
        self.lvalue_context = prev;
        ty
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
        self.check_var_type_size(var, ty);

        if let Some(init) = var.initializer {
            if var.is_state_variable() && ty.has_mapping(self.gcx) {
                self.dcx().emit_err(
                    var.span,
                    "types in storage containing (nested) mappings cannot be assigned to",
                );
            } else if expect {
                let _ = if var.is_state_variable() {
                    self.with_construction_context(|this| this.expect_ty(init, ty))
                } else {
                    self.expect_ty(init, ty)
                };
            }
        }

        if var.is_immutable() {
            if !ty.is_value_type() {
                self.dcx().emit_err(var.span, "immutable variables cannot have a non-value type");
            }
            if let TyKind::Fn(f) = ty.kind
                && f.is_external()
            {
                self.dcx().emit_err(
                    var.span,
                    "immutable variables of external function type are not yet supported",
                );
            }
        }

        if !var.is_state_variable()
            && matches!(
                var.data_location,
                Some(DataLocation::Calldata) | Some(DataLocation::Memory)
            )
            && ty.has_mapping(self.gcx)
        {
            self.dcx().emit_err(
                var.span,
                format!(
                    "type `{}` is only valid in storage because it contains a (nested) mapping",
                    ty.display(self.gcx)
                ),
            );
        }

        // Uninitialized local mapping variables are invalid (error 4182).
        if var.kind == hir::VarKind::Statement
            && var.initializer.is_none()
            && matches!(ty.peel_refs().kind, TyKind::Mapping(..))
        {
            self.dcx()
                .err("uninitialized mapping")
                .note("mappings cannot be created dynamically, you have to assign them from a state variable")
                .span(var.span)
                .emit();
        }

        ty
    }

    fn check_var_type_size(&self, var: &hir::Variable<'gcx>, ty: Ty<'gcx>) {
        if let Some(loc @ (DataLocation::Memory | DataLocation::Calldata)) = ty.loc()
            && let Some(size) = self.ty_memory_static_size(ty.peel_refs())
            && size >= u32::MAX
        {
            self.dcx().err(format!("type too large for {loc}")).span(var.ty.span).emit();
        }
    }

    fn ty_memory_static_size(&self, ty: Ty<'gcx>) -> Option<U256> {
        match ty.kind {
            TyKind::Array(elem, len) => {
                let elem_size = if elem.is_dynamically_sized() {
                    U256::from(32)
                } else {
                    self.ty_memory_static_size(elem)?
                };
                len.checked_mul(elem_size)
            }
            TyKind::Struct(id) => {
                let mut size = U256::ZERO;
                for &field_ty in self.gcx.struct_field_types(id) {
                    let field_size = if field_ty.is_dynamically_sized() {
                        U256::from(32)
                    } else {
                        self.ty_memory_static_size(field_ty)?
                    };
                    size = size.checked_add(field_size)?;
                }
                Some(size)
            }
            TyKind::Ref(inner, _) => self.ty_memory_static_size(inner),
            _ => Some(U256::from(32)),
        }
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
            self.dcx().emit_err_label(
                span,
                "mismatched number of components",
                init.span,
                format!(
                    "expected a tuple with {} element{}, found one with {} element{}",
                    decls.len(),
                    pluralize!(decls.len()),
                    value_types.len(),
                    pluralize!(value_types.len())
                ),
            );
        }

        let exprs = if let hir::ExprKind::Tuple(exprs) = init.kind {
            exprs
        } else {
            std::slice::from_ref(&init_opt)
        };
        for ((&var, &ty), &expr) in decls.iter().zip(value_types).zip(exprs) {
            let (Some(var), Some(expr)) = (var, expr) else { continue };
            let var_ty = self.check_var_(var, false);
            let _ = self.check_expected(expr, ty, var_ty);
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
            self.dcx().emit_err(key.span, "only elementary types, user defined value types, contract types or enums are allowed as mapping keys.");
        }
        ty
    }

    #[must_use]
    fn require_lvalue(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        let prev = self.lvalue_context.replace(Ok(()));
        let ty = self.check_expr(expr);
        let result = self.lvalue_context.unwrap();
        self.lvalue_context = prev;

        if result.is_ok() && self.is_lvalue_expr(expr) {
            return ty;
        }

        let (msg, help) = match result {
            Err(NotLvalueReason::Constant) => ("cannot assign to a constant variable", None),
            Err(NotLvalueReason::Immutable) => (
                "cannot assign to immutable here",
                Some(
                    "immutables can only be assigned in state variable initializers, constructor arguments, or constructor bodies",
                ),
            ),
            Err(NotLvalueReason::CalldataArray) => ("calldata arrays are read-only", None),
            Err(NotLvalueReason::CalldataStruct) => ("calldata structs are read-only", None),
            Err(NotLvalueReason::FixedBytesIndex) => {
                ("single bytes in fixed bytes arrays cannot be modified", None)
            }
            Err(NotLvalueReason::ArrayLength) => {
                ("member `length` is read-only and cannot be used to resize arrays", None)
            }
            Err(NotLvalueReason::Generic) | Ok(()) => ("expression has to be an lvalue", None),
        };
        let mut diag = self.dcx().err(msg).span(expr.span);
        if let Some(help) = help {
            diag = diag.help(help);
        }
        diag.emit();

        ty
    }

    fn try_set_not_lvalue(&mut self, reason: NotLvalueReason) {
        if let Some(Ok(())) = self.lvalue_context {
            self.lvalue_context = Some(Err(reason));
        }
    }

    fn in_lvalue(&self) -> bool {
        self.lvalue_context.is_some()
    }

    /// Returns `true` if the given expression can be an lvalue.
    ///
    /// If `false`, it cannot be an lvalue.
    fn is_lvalue_expr(&self, expr: &hir::Expr<'_>) -> bool {
        match expr.kind {
            hir::ExprKind::Ident(_)
            | hir::ExprKind::Index(..)
            | hir::ExprKind::Member(..)
            | hir::ExprKind::YulMember(..)
            | hir::ExprKind::Tuple(..)
            | hir::ExprKind::Err(_) => true,

            hir::ExprKind::Call(callee, ..) => {
                self.results.builtin_callee(callee.id) == Some(Builtin::ArrayPush0)
            }

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

    fn in_constructor_context(&self) -> bool {
        self.construction_context != 0
            || self.function.is_some_and(|id| self.gcx.hir.function(id).kind.is_constructor())
    }

    fn with_construction_context<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.construction_context += 1;
        let result = f(self);
        self.construction_context -= 1;
        result
    }

    fn res_not_lvalue_reason(&self, res: hir::Res) -> Option<NotLvalueReason> {
        res_not_lvalue_reason(self.gcx, res, self.in_constructor_context())
    }

    fn resolve_overloads(&self, res: &[hir::Res], span: Span) -> hir::Res {
        match self.try_resolve_overloads(res) {
            Ok(res) => res,
            Err(e) => {
                let msg = match e {
                    OverloadError::NotFound => "no matching declarations found",
                    OverloadError::Ambiguous => "no unique declarations found",
                };
                hir::Res::Err(self.dcx().emit_err(span, msg))
            }
        }
    }

    fn resolve_overloads_with_expected(
        &self,
        res: &[hir::Res],
        expected: Ty<'gcx>,
        span: Span,
    ) -> hir::Res {
        if let &[res] = res {
            return res;
        }
        if let Some(&res @ hir::Res::Err(_)) = res.iter().find(|res| res.is_err()) {
            return res;
        }

        match res
            .iter()
            .copied()
            .filter(|&res| {
                self.type_of_res(res).try_convert_implicit_to(expected, self.gcx).is_ok()
            })
            .collect::<WantOne<_>>()
        {
            WantOne::One(res) => res,
            WantOne::Zero => {
                hir::Res::Err(self.dcx().emit_err(span, "no matching declarations found"))
            }
            WantOne::Many => {
                hir::Res::Err(self.dcx().emit_err(span, "no unique declarations found"))
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
            hir::Res::Builtin(Builtin::This) => self
                .contract
                .map(|contract| self.gcx.type_of_item(contract.into()))
                .unwrap_or_else(|| self.gcx.mk_ty_misc_err()),
            hir::Res::Builtin(Builtin::Super) => self
                .contract
                .map(|contract| {
                    self.gcx.mk_ty(TyKind::Type(self.gcx.mk_ty(TyKind::Super(contract))))
                })
                .unwrap_or_else(|| self.gcx.mk_ty_misc_err()),
            res => self.gcx.type_of_res(res),
        }
    }

    fn register_ty(&mut self, expr: &'gcx hir::Expr<'gcx>, ty: Ty<'gcx>) {
        if self.unsupported_codegen_udvt_operator(expr, ty) {
            self.results.unsupported_udvt_operators.insert(expr.id);
        }

        if let Some(prev_ty) = self.results.expr_types.insert(expr.id, ty) {
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

    /// Returns whether `expr` uses a user-defined value type operator that the
    /// EVM code generator cannot lower yet.
    ///
    /// This is a temporary restriction: we flag such expressions during type
    /// checking so codegen can reject them with a diagnostic (see
    /// `emit_unsupported_udvt_operator` in `solar-codegen`). Remove this check,
    /// its call in `register_ty`, and `TypeckResults::unsupported_udvt_operators`
    /// once codegen supports user-defined operators on UDVTs.
    fn unsupported_codegen_udvt_operator(&self, expr: &'gcx hir::Expr<'gcx>, ty: Ty<'gcx>) -> bool {
        match &expr.kind {
            hir::ExprKind::Assign(lhs, Some(_), rhs) => {
                Self::ty_is_udvt(ty) || self.expr_type_is_udvt(lhs) || self.expr_type_is_udvt(rhs)
            }
            hir::ExprKind::Binary(lhs, op, rhs)
                if !matches!(op.kind, hir::BinOpKind::Eq | hir::BinOpKind::Ne) =>
            {
                Self::ty_is_udvt(ty) || self.expr_type_is_udvt(lhs) || self.expr_type_is_udvt(rhs)
            }
            hir::ExprKind::Unary(op, inner) if op.kind != hir::UnOpKind::Not => {
                Self::ty_is_udvt(ty) || self.expr_type_is_udvt(inner)
            }
            _ => false,
        }
    }

    fn expr_type_is_udvt(&self, expr: &'gcx hir::Expr<'gcx>) -> bool {
        self.results.expr_types.get(&expr.id).is_some_and(|&ty| Self::ty_is_udvt(ty))
    }

    fn ty_is_udvt(ty: Ty<'gcx>) -> bool {
        matches!(ty.peel_refs().kind, TyKind::Udvt(..))
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

    fn visit_nested_function(&mut self, id: hir::FunctionId) -> ControlFlow<Self::BreakValue> {
        let contract = self.gcx.hir.function(id).contract;
        let prev = self.contract;
        let prev_function = self.function.replace(id);
        self.contract = contract.or(prev);
        let r = self.visit_function(self.gcx.hir.function(id));
        self.contract = prev;
        self.function = prev_function;
        r
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if matches!(modifier.id, hir::ItemId::Contract(_)) {
            return ControlFlow::Continue(());
        }
        self.walk_modifier(modifier)
    }

    fn visit_contract(
        &mut self,
        contract: &'gcx hir::Contract<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let Some(slot) = contract.layout {
            self.check_storage_layout_base_slot(slot);
        }

        // Check base constructor arguments
        for (&base_id, modifier) in
            contract.linearized_bases.iter().skip(1).zip(contract.linearized_bases_args.iter())
        {
            // Get constructor parameters if the base has a constructor
            let base_contract = self.gcx.hir.contract(base_id);
            if let Some(ctor_id) = base_contract.ctor {
                let ctor_param_types = self.gcx.item_parameter_types(ctor_id);
                // Check if arguments were provided and validate count. `is Base`
                // without parentheses provides no arguments here (deferred to a
                // derived contract, or the contract is abstract), so only
                // validate when arguments are actually given.
                if let Some(modifier) = modifier
                    && !modifier.args.is_dummy()
                {
                    let arg_count = modifier.args.exprs().len();
                    if arg_count != ctor_param_types.len() {
                        self.dcx().emit_err(modifier.span, format!(
                            "wrong number of arguments for base constructor: expected {}, found {}",
                            ctor_param_types.len(),
                            arg_count
                        ));
                    } else {
                        for (arg_expr, expected_arg_ty) in
                            modifier.args.exprs().zip(ctor_param_types.iter())
                        {
                            let actual_arg_ty = self.with_construction_context(|this| {
                                this.check_expr_kind(arg_expr, Some(*expected_arg_ty))
                            });
                            let _ = self.check_expected(arg_expr, actual_arg_ty, *expected_arg_ty);
                        }
                    }
                }
            }
        }
        for &item in contract.items {
            self.visit_nested_item(item)?;
        }
        ControlFlow::Continue(())
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
                self.visit_ty(&mapping.key)?;
                // Return after visiting the children: falling through to `walk_ty`
                // would visit them a second time, double-typechecking (e.g. the
                // size expression of a fixed-array value type).
                return self.visit_ty(&mapping.value);
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
            hir::StmtKind::Switch(switch) => {
                let _ = self.check_expr(switch.selector);
                for case in switch.cases {
                    for stmt in case.body.iter() {
                        self.visit_stmt(stmt)?;
                    }
                }
                return ControlFlow::Continue(());
            }
            hir::StmtKind::Emit(call_expr) | hir::StmtKind::Revert(call_expr) => {
                let is_emit = matches!(stmt.kind, hir::StmtKind::Emit(_));
                if is_emit {
                    self.in_emit = true;
                } else {
                    self.in_revert = true;
                }
                let _ty = self.check_expr(call_expr);
                self.in_emit = false;
                self.in_revert = false;

                let hir::ExprKind::Call(callee, ..) = call_expr.kind else {
                    unreachable!("bad Emit|Revert");
                };
                let callee_ty = self.get(callee);
                if !callee_ty.references_error() {
                    match stmt.kind {
                        hir::StmtKind::Emit(_) => {
                            if !matches!(callee_ty.kind, TyKind::Event(..)) {
                                self.dcx().emit_err(
                                    callee.span,
                                    "expression has to be an event invocation",
                                );
                            }
                        }
                        hir::StmtKind::Revert(_) => {
                            if !matches!(callee_ty.kind, TyKind::Error(..)) {
                                self.dcx().emit_err(callee.span, "expression has to be an error");
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                return ControlFlow::Continue(());
            }
            hir::StmtKind::AssemblyBlock(block) => {
                let prev = std::mem::replace(&mut self.in_yul, true);
                for stmt in block.stmts {
                    self.visit_stmt(stmt)?;
                }
                self.in_yul = prev;
                return ControlFlow::Continue(());
            }
            hir::StmtKind::Expr(expr) if self.in_yul => {
                let ty = self.check_expr(expr);
                if !matches!(expr.kind, hir::ExprKind::Assign(..))
                    && !ty.is_unit()
                    && !ty.references_error()
                {
                    self.dcx().emit_err(
                        expr.span,
                        "inline assembly expression statements cannot return values",
                    );
                }
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        self.walk_stmt(stmt)
    }
}

enum OverloadError {
    NotFound,
    Ambiguous,
}

enum MemberAccessError {
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
        let mut iter = iter.into_iter();
        match iter.next() {
            None => Self::Zero,
            Some(first) => match iter.next() {
                None => Self::One(first),
                Some(_) => Self::Many,
            },
        }
    }
}

impl<T> WantOne<T> {
    fn push(&mut self, value: T) {
        *self = match self {
            Self::Zero => Self::One(value),
            Self::One(_) | Self::Many => Self::Many,
        };
    }
}

fn res_not_lvalue_reason(
    gcx: Gcx<'_>,
    res: hir::Res,
    allow_immutable: bool,
) -> Option<NotLvalueReason> {
    match res {
        hir::Res::Item(hir::ItemId::Variable(var)) => {
            let var = gcx.hir.variable(var);
            match var.mutability {
                Some(m) if m.is_constant() => Some(NotLvalueReason::Constant),
                Some(m) if m.is_immutable() && !allow_immutable => Some(NotLvalueReason::Immutable),
                _ => None,
            }
        }
        hir::Res::Err(_) => None,
        _ => Some(NotLvalueReason::Generic),
    }
}

fn valid_delete(ty: Ty<'_>) -> bool {
    if ty.references_error() {
        return true;
    }

    match ty.kind {
        TyKind::Elementary(_) | TyKind::Contract(_) | TyKind::Enum(_) | TyKind::Fn(_) => true,
        TyKind::Ref(_, loc) => !matches!(loc, DataLocation::Calldata),

        TyKind::Err(_) => true,

        _ => false,
    }
}

fn is_calldata_sliceable(ty: Ty<'_>) -> bool {
    ty.is_ref_at(DataLocation::Calldata)
        || matches!(ty.kind, TyKind::Slice(array) if array.data_stored_in(DataLocation::Calldata))
}

fn valid_string_concat_arg(ty: Ty<'_>) -> bool {
    let ty = ty.peel_refs();
    matches!(ty.kind, TyKind::StringLiteral(true, _) | TyKind::Elementary(ElementaryType::String))
        || matches!(
            ty.kind,
            TyKind::Slice(array)
                if matches!(array.peel_refs().kind, TyKind::Elementary(ElementaryType::String))
        )
}

fn valid_bytes_concat_arg(ty: Ty<'_>) -> bool {
    matches!(ty.kind, TyKind::StringLiteral(..))
        || matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::FixedBytes(_))
        )
        || matches!(
            ty.kind,
            TyKind::Slice(array)
                if matches!(array.peel_refs().kind, TyKind::Elementary(ElementaryType::Bytes))
        )
}

fn abi_decode_arg_kind(name: Symbol) -> Option<AbiDecodeArg> {
    if name == sym::data {
        Some(AbiDecodeArg::Data)
    } else if name == sym::types {
        Some(AbiDecodeArg::Types)
    } else {
        None
    }
}

fn abi_encode_call_function_kind_message(kind: TyFnKind) -> &'static str {
    match kind {
        TyFnKind::Internal | TyFnKind::InternalWithSelector => {
            "first argument to `abi.encodeCall` must be an external function"
        }
        TyFnKind::DelegateCall => "first argument to `abi.encodeCall` cannot be a library function",
        TyFnKind::Creation => "first argument to `abi.encodeCall` cannot be a creation function",
        TyFnKind::BareCall | TyFnKind::BareDelegateCall | TyFnKind::BareStaticCall => {
            "first argument to `abi.encodeCall` cannot be a special function"
        }
        TyFnKind::External | TyFnKind::Declaration => unreachable!(),
    }
}

fn type_supported_by_old_abi_encoder(ty: Ty<'_>) -> bool {
    let ty = ty.peel_refs();
    match ty.kind {
        TyKind::Struct(_) => false,
        TyKind::Array(base, _) | TyKind::DynArray(base) => {
            type_supported_by_old_abi_encoder(base) && !base.peel_refs().is_dynamically_sized()
        }
        TyKind::Tuple([ty]) => type_supported_by_old_abi_encoder(*ty),
        TyKind::Tuple(_) => false,
        TyKind::Slice(array) => type_supported_by_old_abi_encoder(array),
        _ => true,
    }
}

fn valid_abi_decodable_type<'gcx>(ty: Ty<'gcx>, gcx: Gcx<'gcx>) -> bool {
    if ty.references_error() {
        return true;
    }
    match ty.kind {
        TyKind::Error(..)
        | TyKind::Event(..)
        | TyKind::Module(..)
        | TyKind::BuiltinModule(..)
        | TyKind::Type(_)
        | TyKind::Meta(_)
        | TyKind::Variadic
        | TyKind::Tuple(_) => false,
        _ => ty.can_be_exported(gcx),
    }
}

fn valid_abi_encodable_arg<'gcx>(ty: Ty<'gcx>, gcx: Gcx<'gcx>) -> bool {
    if ty.references_error() {
        return true;
    }
    match ty.kind {
        TyKind::Tuple([ty]) => valid_abi_encodable_arg(*ty, gcx),
        TyKind::Tuple(_) => false,
        TyKind::Error(..)
        | TyKind::Event(..)
        | TyKind::Module(..)
        | TyKind::BuiltinModule(..)
        | TyKind::Type(_)
        | TyKind::Meta(_)
        | TyKind::Variadic => false,
        _ => ty.can_be_exported(gcx),
    }
}

fn split_variadic_params<'a, 'gcx>(param_tys: &'a [Ty<'gcx>]) -> (&'a [Ty<'gcx>], bool) {
    let Some((&last, fixed)) = param_tys.split_last() else {
        return (param_tys, false);
    };
    if matches!(last.kind, TyKind::Variadic) {
        return (fixed, true);
    }
    debug_assert!(
        !param_tys.iter().any(|ty| matches!(ty.kind, TyKind::Variadic)),
        "variadic param must be last"
    );
    (param_tys, false)
}

fn valid_unop(ty: Ty<'_>, op: hir::UnOpKind) -> bool {
    if ty.references_error() {
        return true;
    }

    let ty = ty.peel_refs();
    match ty.kind {
        TyKind::Elementary(hir::ElementaryType::Int(_) | hir::ElementaryType::UInt(_)) => {
            match op {
                hir::UnOpKind::Neg => ty.is_signed(),
                hir::UnOpKind::Not => false,
                hir::UnOpKind::PreInc
                | hir::UnOpKind::PreDec
                | hir::UnOpKind::BitNot
                | hir::UnOpKind::PostInc
                | hir::UnOpKind::PostDec => true,
            }
        }
        // IntLiteral can always be negated (it becomes a negative literal).
        TyKind::IntLiteral(..) => match op {
            hir::UnOpKind::Neg | hir::UnOpKind::BitNot => true,
            hir::UnOpKind::Not
            | hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => false,
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
                Shl | Shr | Sar => valid_shift(gcx, ty, other, op),
                Pow if other.is_signed() => None,
                // A literal base raised to a non-constant exponent is a runtime
                // value, not a constant; solc types it `uint256` (`int256` for a
                // negative literal), matching the shift rule.
                Pow if matches!(ty.kind, TyKind::IntLiteral(..))
                    && !matches!(other.kind, TyKind::IntLiteral(..)) =>
                {
                    Some(if ty.is_signed() { gcx.types.int(256) } else { gcx.types.uint(256) })
                }
                Pow => Some(ty),
                And | Or => None,
                _ => ty.common_type(other, gcx),
            }
        }

        TyKind::Elementary(hir::ElementaryType::FixedBytes(_)) => {
            if op.is_shift() {
                return valid_shift(gcx, ty, other, op);
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

        TyKind::Fn(f) => {
            use hir::BinOpKind::*;

            let TyKind::Fn(other_fn) = other.kind else { return None };
            if !matches!(op, Eq | Ne) {
                return None;
            }
            if !((f.is_internal() && other_fn.is_internal())
                || (f.is_external() && other_fn.is_external()))
            {
                return None;
            }
            ty.common_type(other, gcx)
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
        | TyKind::Variadic
        | TyKind::Super(_)
        | TyKind::Type(_)
        | TyKind::Meta(_) => None,

        TyKind::Err(_) => Some(ty),

        TyKind::Ref(..) => unreachable!(),
    }
}

fn valid_shift<'gcx>(
    gcx: Gcx<'gcx>,
    ty: Ty<'gcx>,
    other: Ty<'gcx>,
    op: hir::BinOpKind,
) -> Option<Ty<'gcx>> {
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
    // A literal left operand shifted by a non-constant amount produces a runtime
    // value, not a compile-time constant. solc gives it a full-width type
    // (`uint256`, or `int256` for a negative literal); match that so e.g.
    // `bytes32(1 << role)` type-checks.
    if matches!(ty.kind, TyKind::IntLiteral(..)) && !matches!(other.kind, TyKind::IntLiteral(..)) {
        return Some(if ty.is_signed() { gcx.types.int(256) } else { gcx.types.uint(256) });
    }
    Some(ty)
}

fn valid_meta_type(ty: Ty<'_>) -> bool {
    debug_assert!(!matches!(ty.kind, TyKind::Type(_)));
    matches!(
        ty.kind,
        TyKind::Elementary(hir::ElementaryType::Int(_) | hir::ElementaryType::UInt(_))
            | TyKind::Contract(_)
            | TyKind::Enum(_)
    )
}
