use crate::{
    builtins::Builtin,
    hir::{self, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_ast::{DataLocation, ElementaryType, Span};
use solar_data_structures::{map::FxHashMap, smallvec::SmallVec, Never};
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

    #[must_use]
    fn check_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        self.check_expr_with(expr, None)
    }

    #[must_use]
    fn expect_ty(&mut self, expr: &'gcx hir::Expr<'gcx>, expected: Ty<'gcx>) -> Ty<'gcx> {
        self.check_expr_with(expr, Some(expected))
    }

    #[must_use]
    fn check_expr_with(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        expected: Option<Ty<'gcx>>,
    ) -> Ty<'gcx> {
        let ty = self.check_expr_kind(expr, expected);
        if let Some(expected) = expected {
            self.check_expected(expr, ty, expected);
        }
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
                    let expr_ty = self.check_expr_with(expr, expected);
                    if let Some(common_ty) = &mut common {
                        common = common_ty.common_type(expr_ty, self.gcx);
                    } else if i == 0 {
                        common = expr_ty.mobile(self.gcx);
                    }
                }
                if let Some(common) = common {
                    self.gcx.mk_ty(TyKind::ArrayLiteral(common, exprs.len()))
                } else {
                    self.gcx.mk_ty_err(
                        self.dcx().err("cannot infer array element type").span(expr.span).emit(),
                    )
                }
            }
            hir::ExprKind::Assign(lhs, bin_op, rhs) => {
                let ty = self.require_lvalue(lhs);
                self.check_assign(ty, lhs);
                if let Some(bin_op) = bin_op {
                    let _ = bin_op;
                    todo!()
                } else {
                    let _ = self.expect_ty(rhs, ty);
                    ty
                }
            }
            hir::ExprKind::Binary(_, _bin_op, _) => todo!(),
            hir::ExprKind::Call(_expr, ref _call_args, ref _opts) => todo!(),
            hir::ExprKind::Delete(expr) => {
                let _ = self.require_lvalue(expr);
                self.gcx.types.unit
            }
            hir::ExprKind::Ident(res) => self.type_of_res(self.resolve_overloads(res, expr.span)),
            hir::ExprKind::Index(lhs, index) => {
                let ty = self.check_expr_with(lhs, expected);
                if let Some((index_ty, result_ty)) = self.index_types(ty) {
                    let _ = self.expect_ty(index, index_ty);
                    result_ty
                } else {
                    self.gcx.mk_ty_err(self.dcx().err("cannot index").span(expr.span).emit())
                }
            }
            hir::ExprKind::Slice(lhs, start, end) => {
                let ty = self.check_expr_with(lhs, expected);
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
                let ty = self.check_expr_with(expr, expected);
                let possible_members = self
                    .gcx
                    .members_of(ty, self.source, self.contract)
                    .iter()
                    .filter(|m| m.name == ident.name)
                    .collect::<SmallVec<[_; 4]>>();

                // TODO: overload resolution

                match possible_members[..] {
                    [] => {
                        let msg = format!(
                            "member `{ident}` not found on type `{}`",
                            ty.display(self.gcx)
                        );
                        // TODO: Did you mean ...?
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                    [member] => member.ty,
                    [..] => {
                        let msg = format!(
                            "member `{ident}` not unique on type `{}`",
                            ty.display(self.gcx)
                        );
                        let err = self.dcx().err(msg).span(ident.span);
                        self.gcx.mk_ty_err(err.emit())
                    }
                }
            }
            hir::ExprKind::New(_) => todo!(),
            hir::ExprKind::Payable(expr) => {
                let ty = self.expect_ty(expr, self.gcx.types.address);
                if ty.references_error() {
                    ty
                } else {
                    self.gcx.types.address_payable
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
            hir::ExprKind::Tuple(_) => todo!(),
            hir::ExprKind::TypeCall(ref ty) => {
                self.gcx.mk_ty(TyKind::Meta(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Type(ref ty) => {
                self.gcx.mk_ty(TyKind::Type(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Unary(un_op, expr) => {
                // TODO: un_op
                let ty = if un_op.kind.is_modifying() {
                    self.require_lvalue(expr)
                } else {
                    self.check_expr_with(expr, expected)
                };
                // TODO: Allow only on int, int literal, bool
                ty
            }
            hir::ExprKind::Err(guar) => self.gcx.mk_ty_err(guar),
        }
    }

    fn check_assign(&self, ty: Ty<'gcx>, expr: &'gcx hir::Expr<'gcx>) {
        // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1421
        let _ = (ty, expr);
    }

    /// Returns `(index_ty, result_ty)` for the given type, if it is indexable.
    #[must_use]
    fn index_types(&self, ty: Ty<'gcx>) -> Option<(Ty<'gcx>, Ty<'gcx>)> {
        Some(match ty.peel_refs().kind {
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                (self.gcx.types.uint(256), element)
            }
            TyKind::Elementary(ElementaryType::Bytes)
            | TyKind::Elementary(ElementaryType::FixedBytes(_)) => {
                (self.gcx.types.uint(256), self.gcx.types.fixed_bytes(1))
            }
            TyKind::Mapping(key, value) => (key, value),
            _ => return None,
        })
    }

    fn check_expected(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        actual: Ty<'gcx>,
        expected: Ty<'gcx>,
    ) {
        if actual.convert_implicit_to(expected) {
            return;
        }
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
    fn require_lvalue(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Ty<'gcx> {
        let prev = self.lvalue_context.replace(false);
        let ty = self.check_expr(expr);
        let ctx = self.lvalue_context;
        debug_assert!(ctx.is_some());
        self.lvalue_context = prev;
        if ctx != Some(true) || !is_syntactic_lvalue(expr) {
            return ty;
        }

        // TODO: better error message https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L4143

        self.dcx().err("expected lvalue").span(expr.span).emit();

        ty
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

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        let _ = self.check_expr(expr);
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        // TODO
        self.walk_stmt(stmt)
    }
}

fn is_syntactic_lvalue(expr: &hir::Expr<'_>) -> bool {
    match expr.kind {
        hir::ExprKind::Ident(_) | hir::ExprKind::Err(_) => true,

        // The only lvalue call allowed is `array.push() = x;`
        hir::ExprKind::Member(expr, _)
        | hir::ExprKind::Call(expr, _, _)
        | hir::ExprKind::Index(expr, _) => is_syntactic_lvalue(expr),
        hir::ExprKind::Tuple(exprs) => exprs.iter().copied().flatten().all(is_syntactic_lvalue),

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
                    Some(_) => Self::Many
                    // (std::iter::once(first).chain(iter).collect())
                    ,
                }
            }
        }
    }
}
