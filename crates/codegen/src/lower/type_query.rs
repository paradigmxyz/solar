//! Expression type queries used by lowering.

use super::Lowerer;
use solar_ast::LitKind;
use solar_sema::{
    builtins::Builtin,
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

    pub(super) fn is_dynamic_memory_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return false;
                };
                if self.storage_slots.contains_key(var_id) {
                    return false;
                }
                let var = self.gcx.hir.variable(*var_id);
                matches!(&var.ty.kind, hir::TypeKind::Array(array) if array.size.is_none())
            }
            ExprKind::Member(base, member) => {
                let Some(struct_id) = self.expr_struct_id(base) else {
                    return false;
                };
                let strukt = self.gcx.hir.strukt(struct_id);
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    if field.name.is_some_and(|name| name.name == member.name) {
                        return matches!(
                            &field.ty.kind,
                            hir::TypeKind::Array(array) if array.size.is_none()
                        );
                    }
                }
                false
            }
            ExprKind::Call(callee, _, _) => {
                matches!(&callee.kind, ExprKind::New(ty) if matches!(&ty.kind, hir::TypeKind::Array(array) if array.size.is_none()))
            }
            _ => false,
        }
    }

    pub(super) fn new_dynamic_memory_array_const_len(&self, expr: &hir::Expr<'_>) -> Option<u64> {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return None;
        };
        if !matches!(
            &callee.kind,
            ExprKind::New(ty) if matches!(&ty.kind, hir::TypeKind::Array(array) if array.size.is_none())
        ) {
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
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return false;
                };
                let var = self.gcx.hir.variable(*var_id);
                matches!(
                    var.ty.kind,
                    hir::TypeKind::Elementary(
                        hir::ElementaryType::String | hir::ElementaryType::Bytes
                    )
                )
            }
            ExprKind::Call(callee, _, _) => {
                let ExprKind::Member(base, member) = &callee.kind else {
                    return false;
                };
                let ExprKind::Ident(res_slice) = &base.kind else {
                    return false;
                };
                matches!(res_slice.first(), Some(hir::Res::Builtin(Builtin::Abi)))
                    && matches!(
                        member.name.as_str(),
                        "encode" | "encodePacked" | "encodeWithSelector" | "encodeWithSignature"
                    )
            }
            _ => false,
        }
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
