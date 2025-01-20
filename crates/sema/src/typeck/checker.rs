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
                    // TODO: https://github.com/ethereum/solidity/blob/9d7cc42bc1c12bb43e9dccf8c6c36833fdfcbbca/libsolidity/analysis/TypeChecker.cpp#L1583
                    self.gcx.mk_ty(TyKind::ArrayLiteral(common, exprs.len()))
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
            hir::ExprKind::Call(expr, ref _call_args, ref _opts) => {
                let _ty = self.check_expr(expr);

                // TODO: `array.push() = x;` is the only valid call lvalue
                let is_array_push = false;
                if !is_array_push {
                    self.not_lvalue();
                }

                todo!()
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
                let expr_ty = self.check_expr_with(expr, expected);
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
            hir::ExprKind::Tuple(exprs) => match exprs {
                [] | [None] => unreachable!("shouldn't be able to parse"),
                [Some(expr)] => self.check_expr_with(expr, expected),
                _ => {
                    for &expr in exprs.iter().flatten() {
                        let _ = self.check_expr(expr);
                    }
                    todo!()
                }
            },
            hir::ExprKind::TypeCall(ref ty) => {
                self.gcx.mk_ty(TyKind::Meta(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Type(ref ty) => {
                self.gcx.mk_ty(TyKind::Type(self.gcx.type_of_hir_ty(ty)))
            }
            hir::ExprKind::Unary(op, expr) => {
                let ty = if op.kind.is_modifying() {
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
        let result = binop_result_type(self.gcx, lhs, rhs, op.kind);
        // TODO: custom operators
        if let Some(result) = result {
            if !(assign && result != lhs) {
                return result;
            }
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
        let prev = self.lvalue_context.replace(true);
        let ty = self.check_expr(expr);
        let ctx = self.lvalue_context;
        debug_assert!(ctx.is_some());
        self.lvalue_context = prev;
        if ctx == Some(true) && is_syntactic_lvalue(expr) {
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
        match stmt.kind {
            hir::StmtKind::If(cond, body, else_) => {
                let _ = self.expect_ty(cond, self.gcx.types.bool);
                self.visit_stmt(body);
                if let Some(else_) = else_ {
                    self.visit_stmt(else_);
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
        hir::ExprKind::Ident(_) | hir::ExprKind::Err(_) => true,

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

fn res_is_lvalue(gcx: Gcx<'_>, res: hir::Res) -> bool {
    match res {
        hir::Res::Item(hir::ItemId::Variable(var)) => !gcx.hir.variable(var).is_constant(),
        _ => false,
    }
}

fn valid_delete(ty: Ty<'_>) -> bool {
    match ty.kind {
        TyKind::Elementary(_) | TyKind::Contract(_) | TyKind::Enum(_) | TyKind::FnPtr(_) => true,
        TyKind::Ref(_, loc) => !matches!(loc, DataLocation::Calldata),

        TyKind::Err(_) => true,

        _ => false,
    }
}

fn valid_unop(ty: Ty<'_>, op: hir::UnOpKind) -> bool {
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

fn binop_result_type<'gcx>(
    gcx: Gcx<'gcx>,
    ty: Ty<'gcx>,
    other: Ty<'gcx>,
    op: hir::BinOpKind,
) -> Option<Ty<'gcx>> {
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
            if let Some(common_type) = ty.common_type(other, gcx) {
                if common_type.is_fixed_bytes() {
                    return Some(common_type);
                }
            }
            None
        }
        TyKind::Elementary(hir::ElementaryType::Bool) => (other == ty
            && matches!(
                op,
                hir::BinOpKind::Eq | hir::BinOpKind::Ne | hir::BinOpKind::And | hir::BinOpKind::Or
            ))
        .then_some(ty),

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
        | TyKind::ArrayLiteral(..)
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
