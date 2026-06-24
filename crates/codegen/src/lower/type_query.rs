//! Expression type queries used by lowering.

use super::Lowerer;
use solar_ast::{DataLocation, LitKind};
use solar_sema::{
    builtins::Builtin,
    hir::{self, ElementaryType, ExprKind},
    ty::{ResolvedMember, Ty, TyKind},
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
        self.gcx.type_of_expr(expr.id).or_else(|| self.get_expr_type_from_hir(expr))
    }

    fn get_expr_type_from_hir(&self, expr: &hir::Expr<'_>) -> Option<Ty<'gcx>> {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let hir::Res::Item(hir::ItemId::Variable(var_id)) = res_slice.first()? else {
                    return None;
                };
                Some(self.gcx.type_of_hir_ty(&self.gcx.hir.variable(*var_id).ty))
            }
            ExprKind::Index(base, _) => self.index_element_type(self.get_expr_type(base)?),
            ExprKind::Member(_, _) => {
                if let Some((struct_id, field_index)) = self.resolved_struct_field(expr) {
                    return self.gcx.struct_field_types(struct_id).get(field_index).copied();
                }
                None
            }
            ExprKind::Binary(lhs, op, _) if Self::binary_result_matches_lhs_type(op.kind) => {
                self.get_expr_type(lhs)
            }
            ExprKind::Assign(lhs, Some(_), _) => self.get_expr_type(lhs),
            ExprKind::Assign(_, None, rhs) => self.get_expr_type(rhs),
            ExprKind::Tuple(elements) => {
                elements.iter().flatten().next().and_then(|expr| self.get_expr_type(expr))
            }
            _ => None,
        }
    }

    fn binary_result_matches_lhs_type(op: hir::BinOpKind) -> bool {
        use hir::BinOpKind::*;
        matches!(op, Shl | Shr | Sar | BitAnd | BitOr | BitXor | Add | Sub | Pow | Mul | Div | Rem)
    }

    fn index_element_type(&self, ty: Ty<'gcx>) -> Option<Ty<'gcx>> {
        match ty.peel_refs().kind {
            TyKind::Mapping(_, value) | TyKind::Array(value, _) | TyKind::DynArray(value) => {
                Some(value)
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(self.gcx.types.uint(8))
            }
            _ => None,
        }
    }

    /// Gets the non-call member target selected by sema's type checker.
    pub(super) fn resolved_member(&self, expr: &hir::Expr<'_>) -> Option<ResolvedMember> {
        self.gcx.resolved_member(expr.id)
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
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
            && self.struct_storage_base_slots.contains_key(var_id)
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
