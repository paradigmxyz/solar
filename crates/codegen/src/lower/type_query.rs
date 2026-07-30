//! Expression type queries used by lowering.

use super::Lowerer;
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::{
    hir::{self, ElementaryType},
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

    pub(super) fn is_dynamic_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr)
            .is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::DynArray(_)))
    }

    pub(super) fn is_dynamic_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.expr_has_bytes_or_string_type(expr)
    }

    pub(super) fn expr_struct_id(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        if let Some(var_id) = self.gcx.resolved_variable(expr)
            && self.struct_storage_base_slots.contains_key(&var_id)
        {
            return None;
        }

        let ty = self.get_expr_type(expr)?;
        let TyKind::Struct(struct_id) = ty.peel_refs().kind else { return None };
        Some(struct_id)
    }

    /// Gets struct info for an expression if it has a struct type.
    /// Returns (struct_id, field_count) if the expression is a struct.
    pub(super) fn get_expr_struct_info(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::StructId, usize)> {
        let struct_id = self.expr_struct_id(expr)?;
        let field_count = self.gcx.struct_field_types(struct_id).len();
        Some((struct_id, field_count))
    }
}
