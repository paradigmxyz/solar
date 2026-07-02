//! Expression type queries used by lowering.

use super::Lowerer;
use solar_ast::{DataLocation, LitKind};
use solar_sema::{
    builtins::Builtin,
    hir::{self, ElementaryType, ExprKind},
    ty::{ResolvedMember, TyKind},
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

    /// Gets the non-call member target selected by sema's type checker.
    pub(super) fn resolved_member(&self, expr: &hir::Expr<'_>) -> Option<ResolvedMember> {
        self.gcx.resolved_member(expr.id)
    }

    /// Returns the resolution of an identifier expression used as a value.
    ///
    /// `hir::ExprKind::Ident` carries the raw candidate set from name
    /// resolution, which does not account for overloading. This mirrors the
    /// type checker's value-position rule: a single candidate wins, and among
    /// several candidates the unique variable wins (a public state variable is
    /// accompanied by its getter function). Anything else, such as an
    /// overloaded function referenced as a value, is ambiguous and yields
    /// `None`. Overloaded callees must go through [`Self::callee_res`], which
    /// uses the target selected by sema's type checker.
    pub(super) fn ident_res(&self, expr: &hir::Expr<'_>) -> Option<hir::Res> {
        let ExprKind::Ident(res_slice) = &expr.kind else { return None };
        match res_slice {
            [] => None,
            [res] => Some(*res),
            _ => {
                let mut vars = res_slice.iter().filter(|res| res.as_variable().is_some());
                match (vars.next(), vars.next()) {
                    (Some(&res), None) => Some(res),
                    _ => None,
                }
            }
        }
    }

    /// Returns the variable an identifier expression resolves to.
    ///
    /// Variables cannot be overloaded, so a single-entry resolution is the
    /// only valid form here.
    pub(super) fn ident_variable(&self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        match self.ident_res(expr)? {
            hir::Res::Item(hir::ItemId::Variable(var_id)) => Some(var_id),
            _ => None,
        }
    }

    /// Returns the builtin an identifier expression resolves to.
    pub(super) fn ident_builtin(&self, expr: &hir::Expr<'_>) -> Option<Builtin> {
        match self.ident_res(expr)? {
            hir::Res::Builtin(builtin) => Some(builtin),
            _ => None,
        }
    }

    /// Returns the resolution of a call callee expression.
    ///
    /// Prefers the overload target selected by sema's type checker; falls back
    /// to the callee identifier's single resolution when the type checker did
    /// not need to disambiguate.
    pub(super) fn callee_res(&self, callee: &hir::Expr<'_>) -> Option<hir::Res> {
        if let Some(resolved) = self.gcx.resolved_callee(callee.id) {
            return Some(resolved.res);
        }
        self.ident_res(callee)
    }

    pub(super) fn resolved_builtin_member(&self, expr: &hir::Expr<'_>) -> Option<Builtin> {
        self.gcx.builtin_member(expr.id)
    }

    pub(super) fn resolved_struct_field(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::StructId, usize)> {
        match self.resolved_member(expr)? {
            ResolvedMember::StructField { struct_id, field_index } => {
                Some((struct_id, field_index))
            }
            _ => None,
        }
    }

    pub(super) fn resolved_enum_variant(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::EnumId, usize)> {
        match self.resolved_member(expr)? {
            ResolvedMember::EnumVariant { enum_id, variant_index } => {
                Some((enum_id, variant_index))
            }
            _ => None,
        }
    }

    pub(super) fn is_dynamic_memory_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.get_expr_type(expr) else { return false };
        match ty.kind {
            TyKind::Ref(inner, DataLocation::Memory) => matches!(inner.kind, TyKind::DynArray(_)),
            _ => false,
        }
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

    pub(super) fn expr_struct_id(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        if let Some(var_id) = self.ident_variable(expr)
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
