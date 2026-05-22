use crate::{
    builtins::{Builtin, members},
    eval::{ConstantEvaluator, EvalErrorKind},
    hir::{self, Visit},
    ty::{Gcx, Ty, TyFn, TyFnKind, TyKind},
};
use alloy_primitives::U256;
use solar_ast::{
    DataLocation, ElementaryType, Span, StateMutability, TypeSize, UserDefinableOperator,
};
use solar_data_structures::{Never, map::FxHashMap, pluralize, smallvec::SmallVec};
use solar_interface::{Ident, Symbol, diagnostics::DiagCtxt, kw, sym};
use std::ops::ControlFlow;

type ParamNames = SmallVec<[Option<Symbol>; 8]>;
type CallCandidateParams<'gcx> = (&'gcx [Ty<'gcx>], Option<ParamNames>);

mod yul;

pub(super) fn check(gcx: Gcx<'_>, source: hir::SourceId) {
    let mut checker = TypeChecker::new(gcx, source);
    let _ = checker.visit_nested_source(source);
}

struct TypeChecker<'gcx> {
    gcx: Gcx<'gcx>,
    source: hir::SourceId,
    contract: Option<hir::ContractId>,
    function: Option<hir::FunctionId>,
    construction_context: u32,

    types: FxHashMap<hir::ExprId, Ty<'gcx>>,

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
            types: Default::default(),
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
        let mut evaluator = ConstantEvaluator::new(self.gcx);
        match evaluator.try_eval_storage_layout_base_slot(slot) {
            Ok(value) => {
                if value.as_u256().is_none() {
                    self.dcx()
                        .err(format!(
                            "base slot of storage layout evaluates to {value}, which is outside the range of type `uint256`"
                        ))
                        .span(slot.span)
                        .emit();
                }
            }
            Err(err) if matches!(err.kind, EvalErrorKind::NonInteger) => {
                self.dcx()
                    .err("base slot of storage layout must evaluate to an integer")
                    .span(slot.span)
                    .emit();
            }
            Err(err) => {
                if matches!(err.kind, EvalErrorKind::AlreadyEmitted(_)) {
                    return;
                }
                let _ = self.check_expr(slot);
                self.dcx()
                    .err("base slot of storage layout must be a compile-time constant expression")
                    .span(slot.span)
                    .emit();
            }
        }
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

                // Get the function type for struct constructors, keeping struct_id for field names.
                let struct_id = if let TyKind::Type(struct_ty) = callee_ty.kind
                    && let TyKind::Struct(id) = struct_ty.kind
                {
                    callee_ty = struct_constructor(self.gcx, struct_ty, id);
                    Some(id)
                } else {
                    None
                };

                // TODO: `array.push() = x;` is the only valid call lvalue
                let is_array_push = false;

                let ty = match callee_ty.kind {
                    TyKind::Fn(f) => {
                        if f.is_declaration() {
                            return self.gcx.mk_ty_err(
                                self.dcx()
                                    .err("cannot call function via contract type name")
                                    .span(expr.span)
                                    .emit(),
                            );
                        }
                        let param_names = if let Some(struct_id) = struct_id {
                            Some(self.get_struct_field_names(struct_id))
                        } else {
                            f.function_id
                                .map(|id| self.get_call_param_names(id, f.parameters.len()))
                        };
                        self.check_call_args(expr.span, args, f.parameters, param_names.as_deref());
                        self.fn_call_return_type(f.returns)
                    }
                    TyKind::Type(to) => self.check_explicit_cast(expr.span, to, args),
                    TyKind::Event(param_tys, id) => {
                        if !self.in_emit {
                            self.dcx()
                                .err("event invocations have to be prefixed by `emit`")
                                .span(expr.span)
                                .emit();
                        }
                        // Clear context so nested calls in args are not considered in emit/revert.
                        self.in_emit = false;
                        self.in_revert = false;
                        let event = self.gcx.hir.event(id);
                        let param_names = self.get_param_names(event.parameters);
                        self.check_call_args(expr.span, args, param_tys, Some(&param_names));
                        self.gcx.types.unit
                    }
                    TyKind::Error(param_tys, id) => {
                        // TODO: Also allow in require(condition, MyError(...)).
                        if !self.in_revert {
                            self.dcx()
                                .err("errors can only be used with revert statements")
                                .span(expr.span)
                                .emit();
                        }
                        // Clear context so nested calls in args are not considered in emit/revert.
                        self.in_emit = false;
                        self.in_revert = false;
                        let error = self.gcx.hir.error(id);
                        let param_names = self.get_param_names(error.parameters);
                        self.check_call_args(expr.span, args, param_tys, Some(&param_names));
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

                if !is_array_push {
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
                let res = self.resolve_overloads(res, expr.span);
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
                let ty = self.check_expr(lhs);
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
            hir::ExprKind::Lit(lit) if self.in_yul => self.check_yul_lit(lit),
            hir::ExprKind::Lit(lit) => self.gcx.type_of_lit(lit),
            hir::ExprKind::Member(expr, ident) => {
                let expr_ty = self.check_expr(expr);
                if expr_ty.references_error() {
                    return expr_ty;
                }

                let possible_members = self
                    .gcx
                    .members_of(expr_ty, self.source, self.contract)
                    .filter(|m| m.name == ident.name)
                    .collect::<SmallVec<[_; 4]>>();

                let ty = match self.select_member_access(&possible_members) {
                    Ok(member) => member.ty,
                    Err(MemberAccessError::NotFound) => {
                        let msg = format!(
                            "member `{ident}` not found on type `{}`",
                            expr_ty.display(self.gcx)
                        );
                        // TODO: Did you mean ...?
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                    Err(MemberAccessError::Ambiguous) => {
                        let msg = format!(
                            "member `{ident}` not unique on type `{}`",
                            expr_ty.display(self.gcx)
                        );
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                };

                // Validate lvalue.
                let not_lvalue_reason = match expr_ty.kind {
                    _ if matches!(
                        expr_ty.peel_refs().kind,
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
                    TyKind::Type(ty)
                        if matches!(ty.kind, TyKind::Contract(_))
                            && possible_members.len() == 1
                            && possible_members[0]
                                .res
                                .is_some_and(|res| self.res_not_lvalue_reason(res).is_some()) =>
                    {
                        Some(NotLvalueReason::Generic)
                    }
                    _ => None,
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
                            let msg = format!("cannot instantiate {}s", c.kind);
                            self.gcx.mk_ty_err(self.dcx().err(msg).span(hir_ty.span).emit())
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
                        let mut err =
                            self.dcx().err("cannot instantiate static arrays").span(hir_ty.span);
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
                                self.dcx()
                                    .err("cannot instantiate mappings")
                                    .span(hir_ty.span)
                                    .emit(),
                            )
                        } else if ty.contains_library(self.gcx) {
                            self.gcx.mk_ty_err(
                                self.dcx()
                                    .err("invalid use of a library name")
                                    .span(hir_ty.span)
                                    .emit(),
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
                        self.dcx()
                            .err("expected contract or dynamic array type")
                            .span(hir_ty.span)
                            .emit(),
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
                self.dcx().err("empty tuple on the left hand side").span(expr.span).emit();
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
            self.dcx()
                .err("types in storage containing (nested) mappings cannot be assigned to")
                .span(expr.span)
                .emit();
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
            WantOne::Many => Some(
                self.gcx.mk_ty_err(
                    self.dcx()
                        .err("user-defined operator has more than one matching definition")
                        .span(span)
                        .emit(),
                ),
            ),
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
        let WantOne::One(from_expr) = args.exprs().collect::<WantOne<_>>() else {
            return self.gcx.mk_ty_err(
                self.dcx().err("expected exactly one unnamed argument").span(args.span).emit(),
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
    ) {
        let Err(err) = actual.try_convert_implicit_to(expected, self.gcx) else { return };

        let mut diag = self.dcx().err("mismatched types").span(expr.span);
        diag = diag.span_label(expr.span, err.message(actual, expected, self.gcx));
        diag.emit();
    }

    fn fn_call_return_type(&self, returns: &'gcx [Ty<'gcx>]) -> Ty<'gcx> {
        match returns {
            [] => self.gcx.types.unit,
            [ty] => *ty,
            tys => self.gcx.mk_ty_tuple(tys),
        }
    }

    fn get_param_names(&self, params: &[hir::VariableId]) -> ParamNames {
        params.iter().map(|&id| self.gcx.hir.variable(id).name.map(|i| i.name)).collect()
    }

    fn get_call_param_names(&self, function: hir::FunctionId, param_count: usize) -> ParamNames {
        let mut names = self.get_param_names(self.gcx.hir.function(function).parameters);
        if names.len() > param_count {
            names.drain(..names.len() - param_count);
        }
        names
    }

    fn get_struct_field_names(
        &self,
        id: hir::StructId,
    ) -> SmallVec<[Option<solar_interface::Symbol>; 8]> {
        self.gcx
            .hir
            .strukt(id)
            .fields
            .iter()
            .map(|&fid| self.gcx.hir.variable(fid).name.map(|i| i.name))
            .collect()
    }

    fn check_call_args(
        &mut self,
        call_span: Span,
        args: &hir::CallArgs<'gcx>,
        param_tys: &[Ty<'gcx>],
        param_names: Option<&[Option<solar_interface::Symbol>]>,
    ) {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                self.check_positional_call_args(call_span, args.span, exprs, param_tys);
            }
            hir::CallArgsKind::Named(named_args) => {
                if let Some(names) = param_names {
                    self.check_named_call_args(call_span, args.span, named_args, param_tys, names);
                } else {
                    self.dcx()
                        .err("named arguments cannot be used for functions that take arbitrary parameters")
                        .span(args.span)
                        .emit();
                    for arg in named_args {
                        let _ = self.check_expr(&arg.value);
                    }
                }
            }
        }
    }

    fn check_member_call_callee(
        &mut self,
        callee: &'gcx hir::Expr<'gcx>,
        receiver: &'gcx hir::Expr<'gcx>,
        ident: Ident,
        args: &hir::CallArgs<'gcx>,
    ) -> Ty<'gcx> {
        let receiver_ty = self.check_expr(receiver);
        if let Err(e) = receiver_ty.error_reported() {
            let ty = self.gcx.mk_ty_err(e);
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
                self.gcx.mk_ty_err(self.dcx().err(msg).span(ident.span).emit())
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
            [..] => Err(MemberAccessError::Ambiguous),
        }
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
                hir::Res::Err(self.dcx().err(msg).span(callee.span).emit())
            }
        };
        let ty = self.type_of_res(res);
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
            self.dcx()
                .err("libraries cannot call their own functions externally")
                .span(span)
                .emit();
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

        let mut selected = WantOne::Zero;
        for &res in res {
            let ty = self.type_of_res(res);
            let Some((param_tys, param_names)) = self.call_candidate_params(ty) else {
                continue;
            };
            if self.call_args_match(args, param_tys, param_names.as_deref()) {
                selected.push(res);
            }
        }
        match selected {
            WantOne::Zero => Err(OverloadError::NotFound),
            WantOne::One(res) => Ok(res),
            WantOne::Many => Err(OverloadError::Ambiguous),
        }
    }

    fn call_candidate_params(&self, ty: Ty<'gcx>) -> Option<CallCandidateParams<'gcx>> {
        match ty.kind {
            TyKind::Fn(function_ty) => {
                let param_names = function_ty
                    .function_id
                    .map(|id| self.get_call_param_names(id, function_ty.parameters.len()));
                Some((function_ty.parameters, param_names))
            }
            TyKind::Event(param_tys, id) => {
                let event = self.gcx.hir.event(id);
                Some((param_tys, Some(self.get_param_names(event.parameters))))
            }
            TyKind::Error(param_tys, id) => {
                let error = self.gcx.hir.error(id);
                Some((param_tys, Some(self.get_param_names(error.parameters))))
            }
            TyKind::Err(_) => None,
            _ => None,
        }
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
            if let Some((parameters, param_names)) =
                self.member_call_candidate_params(receiver_ty, member)
                && self.call_args_match(args, parameters, param_names.as_deref())
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

    fn member_call_candidate_params(
        &self,
        receiver_ty: Ty<'gcx>,
        member: &members::Member<'gcx>,
    ) -> Option<CallCandidateParams<'gcx>> {
        let TyKind::Fn(function_ty) = member.ty.kind else { return None };
        let parameters = if member.attached {
            let (&self_ty, parameters) = function_ty.parameters.split_first()?;
            if !receiver_ty.convert_implicit_to(self_ty, self.gcx) {
                return None;
            }
            parameters
        } else {
            function_ty.parameters
        };
        let param_names =
            function_ty.function_id.map(|id| self.get_call_param_names(id, parameters.len()));
        Some((parameters, param_names))
    }

    fn call_args_match(
        &mut self,
        args: &hir::CallArgs<'gcx>,
        param_tys: &[Ty<'gcx>],
        param_names: Option<&[Option<solar_interface::Symbol>]>,
    ) -> bool {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => {
                if exprs.len() != param_tys.len() {
                    return false;
                }
                exprs
                    .iter()
                    .zip(param_tys)
                    .all(|(expr, &param_ty)| self.arg_matches(expr, param_ty))
            }
            hir::CallArgsKind::Named(named_args) => {
                let Some(names) = param_names else { return false };
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
                self.dcx()
                    .err("function call options can only be set on external function calls or contract creations")
                    .span(span)
                    .emit();
            }
            return ty;
        };

        let creation = f.is_creation();
        if !creation && !f.is_external() && !f.is_bare_call() {
            self.dcx()
                .err("function call options can only be set on external function calls or contract creations")
                .span(span)
                .emit();
        }

        let mut gas_set = false;
        let mut value_set = false;
        let mut salt_set = false;
        for opt in opts {
            let name = opt.name.name;
            let duplicate = match name {
                kw::Gas => {
                    if creation {
                        self.dcx()
                            .err("function call option `gas` cannot be used with `new`")
                            .span(opt.name.span)
                            .emit();
                    } else {
                        let _ = self.expect_ty(&opt.value, self.gcx.types.uint(256));
                    }
                    std::mem::replace(&mut gas_set, true)
                }
                sym::value => {
                    if f.kind == TyFnKind::BareDelegateCall {
                        self.dcx()
                            .err("cannot set option `value` for delegatecall")
                            .span(opt.name.span)
                            .emit();
                    } else if f.kind == TyFnKind::BareStaticCall {
                        self.dcx()
                            .err("cannot set option `value` for staticcall")
                            .span(opt.name.span)
                            .emit();
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
                        self.dcx().err(msg).span(opt.name.span).emit();
                    }
                    let _ = self.expect_ty(&opt.value, self.gcx.types.uint(256));
                    std::mem::replace(&mut value_set, true)
                }
                sym::salt => {
                    if !creation {
                        self.dcx()
                            .err("function call option `salt` can only be used with `new`")
                            .span(opt.name.span)
                            .emit();
                    }
                    let _ = self.expect_ty(&opt.value, self.gcx.types.fixed_bytes(32));
                    std::mem::replace(&mut salt_set, true)
                }
                _ => {
                    self.dcx()
                        .err(format!("unknown call option `{name}`"))
                        .span(opt.name.span)
                        .emit();
                    let _ = self.check_expr(&opt.value);
                    false
                }
            };
            if duplicate {
                self.dcx()
                    .err(format!("duplicate call option `{name}`"))
                    .span(opt.name.span)
                    .emit();
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
    ) {
        if exprs.len() != param_tys.len() {
            self.dcx()
                .err(format!(
                    "wrong argument count for function call: {} arguments given but expected {}",
                    exprs.len(),
                    param_tys.len()
                ))
                .span(call_span)
                .span_label(
                    args_span,
                    format!(
                        "expected {} argument{}, found {}",
                        param_tys.len(),
                        pluralize!(param_tys.len()),
                        exprs.len()
                    ),
                )
                .emit();
        }

        let count = std::cmp::min(exprs.len(), param_tys.len());
        for i in 0..count {
            let actual = self.check_expr_once(&exprs[i]);
            self.check_expected(&exprs[i], actual, param_tys[i]);
        }
        for expr in exprs.iter().skip(count) {
            let _ = self.check_expr_once(expr);
        }
    }

    fn check_named_call_args(
        &mut self,
        call_span: Span,
        args_span: Span,
        named_args: &'gcx [hir::NamedArg<'gcx>],
        param_tys: &[Ty<'gcx>],
        param_names: &[Option<solar_interface::Symbol>],
    ) {
        debug_assert_eq!(param_tys.len(), param_names.len());

        if named_args.len() != param_tys.len() {
            self.dcx()
                .err(format!(
                    "wrong argument count for function call: {} arguments given but expected {}",
                    named_args.len(),
                    param_tys.len()
                ))
                .span(call_span)
                .span_label(
                    args_span,
                    format!(
                        "expected {} argument{}, found {}",
                        param_tys.len(),
                        pluralize!(param_tys.len()),
                        named_args.len()
                    ),
                )
                .emit();
        }

        let mut seen_names: SmallVec<[solar_interface::Symbol; 8]> = SmallVec::new();

        for arg in named_args {
            let arg_name = arg.name.name;

            if seen_names.contains(&arg_name) {
                self.dcx()
                    .err(format!("duplicate named argument `{arg_name}`"))
                    .span(arg.name.span)
                    .emit();
                let _ = self.check_expr_once(&arg.value);
                continue;
            }
            seen_names.push(arg_name);

            let param_idx = param_names.iter().position(|n| n.is_some_and(|name| name == arg_name));

            match param_idx {
                Some(idx) => {
                    let actual = self.check_expr_once(&arg.value);
                    self.check_expected(&arg.value, actual, param_tys[idx]);
                }
                None => {
                    self.dcx()
                        .err(format!(
                            "named argument `{arg_name}` does not match function declaration"
                        ))
                        .span(arg.name.span)
                        .emit();
                    let _ = self.check_expr_once(&arg.value);
                }
            }
        }
    }

    fn check_expr_once(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        if let Some(&ty) = self.types.get(&expr.id) { ty } else { self.check_expr(expr) }
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
            if var.is_state_variable() && ty.has_mapping(self.gcx) {
                self.dcx()
                    .err("types in storage containing (nested) mappings cannot be assigned to")
                    .span(var.span)
                    .emit();
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
                self.dcx()
                    .err("immutable variables cannot have a non-value type")
                    .span(var.span)
                    .emit();
            }
            if let TyKind::Fn(f) = ty.kind
                && f.is_external()
            {
                self.dcx()
                    .err("immutable variables of external function type are not yet supported")
                    .span(var.span)
                    .emit();
            }
        }

        if !var.is_state_variable()
            && matches!(
                var.data_location,
                Some(DataLocation::Calldata) | Some(DataLocation::Memory)
            )
            && ty.has_mapping(self.gcx)
        {
            self.dcx()
                .err(format!(
                    "type `{}` is only valid in storage because it contains a (nested) mapping",
                    ty.display(self.gcx)
                ))
                .span(var.span)
                .emit();
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
        let prev = self.lvalue_context.replace(Ok(()));
        let ty = self.check_expr(expr);
        let result = self.lvalue_context.unwrap();
        self.lvalue_context = prev;

        if result.is_ok() && is_syntactic_lvalue(expr) {
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
                // Check if arguments were provided and validate count
                if let Some(modifier) = modifier {
                    let arg_count = modifier.args.exprs().len();
                    if arg_count != ctor_param_types.len() {
                        self.dcx()
                            .err(format!(
                                "wrong number of arguments for base constructor: expected {}, found {}",
                                ctor_param_types.len(),
                                arg_count
                            ))
                            .span(modifier.span)
                            .emit();
                    } else {
                        for (arg_expr, expected_arg_ty) in
                            modifier.args.exprs().zip(ctor_param_types.iter())
                        {
                            let actual_arg_ty = self.with_construction_context(|this| {
                                this.check_expr_kind(arg_expr, Some(*expected_arg_ty))
                            });
                            self.check_expected(arg_expr, actual_arg_ty, *expected_arg_ty);
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
                                self.dcx()
                                    .err("expression has to be an event invocation")
                                    .span(callee.span)
                                    .emit();
                            }
                        }
                        hir::StmtKind::Revert(_) => {
                            if !matches!(callee_ty.kind, TyKind::Error(..)) {
                                self.dcx()
                                    .err("expression has to be an error")
                                    .span(callee.span)
                                    .emit();
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
                    self.dcx()
                        .err("inline assembly expression statements cannot return values")
                        .span(expr.span)
                        .emit();
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
        | hir::ExprKind::YulMember(..)
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
