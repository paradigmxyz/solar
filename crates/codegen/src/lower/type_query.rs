//! Expression type queries used by lowering.

use super::Lowerer;
use solar_ast::{DataLocation, LitKind};
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::{
    hir::{self, ElementaryType, ExprKind},
    ty::TyKind,
};

impl<'gcx> Lowerer<'gcx> {
    pub(super) fn expr_has_bytes_or_string_type(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr).is_some_and(|ty| {
            matches!(
                ty.peel_refs().kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
        })
    }

    /// Gets the type of an expression computed by sema's type checker.
    pub(super) fn get_expr_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        self.gcx.type_of_expr(expr.id)
    }

    /// Returns an error referenced by an expression or its computed type.
    pub(super) fn expr_references_error(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Result<(), ErrorGuaranteed> {
        if let Some(ty) = self.get_expr_type(expr) {
            ty.error_reported()?;
        }
        expr.references_error(&self.gcx.hir)
    }

    pub(super) fn resolved_struct_field(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::StructId, usize)> {
        let res = self.gcx.resolved_expr(expr)?;
        let variable = self.gcx.hir.variable(res.as_variable()?);
        Some((variable.parent?.as_struct()?, res.struct_field_index(&self.gcx.hir)?))
    }

    pub(super) fn resolved_enum_variant(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::EnumId, usize)> {
        let res = self.gcx.resolved_expr(expr)?;
        let variable = self.gcx.hir.variable(res.as_variable()?);
        Some((variable.parent?.as_enum()?, res.enum_variant_index(&self.gcx.hir)?))
    }

    pub(super) fn is_dynamic_memory_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.get_expr_type(expr) else { return false };
        match ty.kind {
            TyKind::Ref(inner, DataLocation::Memory) => matches!(inner.kind, TyKind::DynArray(_)),
            _ => false,
        }
    }

    pub(super) fn is_dynamic_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr)
            .is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::DynArray(_)))
    }

    pub(super) fn new_dynamic_memory_array_const_len(&self, expr: &hir::Expr<'_>) -> Option<u64> {
        if !self.is_dynamic_memory_array_expr(expr) {
            return None;
        }

        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return None;
        };
        if !matches!(&callee.kind, ExprKind::New(_)) {
            return None;
        }

        let len = args.exprs().next()?;
        let ExprKind::Lit(lit) = &len.kind else {
            return None;
        };
        let LitKind::Number(value) = &lit.kind else {
            return None;
        };
        u64::try_from(*value).ok()
    }

    pub(super) fn is_dynamic_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.expr_has_bytes_or_string_type(expr)
    }
}
