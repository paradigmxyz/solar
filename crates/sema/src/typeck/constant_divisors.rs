//! Constant zero divisors.
//!
//! This ports the division by zero (1211) and modulo zero (4195) checks of solc's
//! `StaticAnalyzer`. They only apply to values that solc's constant evaluator computes, which
//! [`Gcx::basic_const_eval`] computes too: `x / 0` is not reported unless `x` is constant too, and
//! neither are operands that only the full evaluation folds.
//!
//! As in solc, this runs after type checking, before the view and pure checks, and only when no
//! other error was reported. Array lengths are not checked: their evaluation already reports a
//! division by zero.
//!
//! Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/analysis/StaticAnalyzer.cpp#L314-L360>

use crate::{
    builtins::Builtin,
    eval::ConstValue,
    hir::{self, Visit},
    ty::Gcx,
};
use solar_data_structures::Never;
use solar_interface::error_code;
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }
    let mut checker = Checker { gcx };
    for id in gcx.hir.source_ids() {
        let _ = checker.visit_nested_source(id);
    }
}

struct Checker<'gcx> {
    gcx: Gcx<'gcx>,
}

impl Checker<'_> {
    /// Returns whether solc's constant evaluator gives the given expression the value zero.
    fn is_zero(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.basic_const_eval().eval_expr(expr).is_ok_and(ConstValue::is_zero)
    }
}

impl<'gcx> Visit<'gcx> for Checker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            hir::ExprKind::Binary(lhs, op, rhs)
                if matches!(op.kind, hir::BinOpKind::Div | hir::BinOpKind::Rem)
                    && self.gcx.user_operator(expr.id).is_none()
                    && self.is_zero(rhs)
                    && self.gcx.basic_const_eval().eval_expr(lhs).is_ok() =>
            {
                let msg =
                    if op.kind == hir::BinOpKind::Div { "division by zero" } else { "modulo zero" };
                self.gcx.dcx().err(msg).code(error_code!(1211)).span(expr.span).emit();
            }
            hir::ExprKind::Call(
                callee,
                hir::CallArgs { kind: hir::CallArgsKind::Unnamed([_, _, modulus]), .. },
            ) if matches!(
                self.gcx.resolved_builtin(callee),
                Some(Builtin::AddMod | Builtin::MulMod)
            ) && self.is_zero(modulus) =>
            {
                let msg = "arithmetic modulo zero";
                self.gcx.dcx().err(msg).code(error_code!(4195)).span(expr.span).emit();
            }
            _ => {}
        }
        self.walk_expr(expr)
    }

    // Array lengths report a division by zero when they are evaluated.
    fn visit_ty(&mut self, _ty: &'gcx hir::Type<'gcx>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}
