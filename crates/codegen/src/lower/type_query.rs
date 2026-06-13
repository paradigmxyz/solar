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

    /// Gets the type of an expression. Prefers the type computed by sema's type
    /// checker (recorded during analysis whenever codegen runs); falls back to
    /// re-deriving it from the HIR for any expression the checker didn't record.
    pub(super) fn get_expr_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        if let Some(ty) = self.gcx.type_of_expr(expr.id) {
            return Some(ty);
        }
        self.get_expr_type_fallback(expr)
    }

    /// Re-derives the type of an expression by walking the HIR. Used as a
    /// fallback when sema's type checker did not record a type for it.
    fn get_expr_type_fallback(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        // Case 1: Variable - get its declared type
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            return Some(self.gcx.type_of_hir_ty(&var.ty));
        }

        // Case 2: Struct field access - get the declared field type.
        if let ExprKind::Member(base, member) = &expr.kind
            && let Some(struct_id) = self.struct_id_of_expr(base)
        {
            let strukt = self.gcx.hir.strukt(struct_id);
            for &field_id in strukt.fields {
                let field = self.gcx.hir.variable(field_id);
                if field.name.is_some_and(|name| name.name == member.name) {
                    return Some(self.gcx.type_of_hir_ty(&field.ty));
                }
            }
        }

        // Case 3: Indexing - use the array element or mapping value type.
        if let ExprKind::Index(base, _) = &expr.kind
            && let Some(element_ty) = self.indexed_value_type(base)
        {
            return Some(element_ty);
        }

        // Case 3: Call result - use the callee's first return type.
        if let ExprKind::Call(callee, args, _) = &expr.kind {
            // Type conversion: `address(this)`, `uint160(x)`, ...
            if let ExprKind::Type(ty) = &callee.kind {
                return Some(self.gcx.type_of_hir_ty(ty));
            }

            if let ExprKind::Ident(res_slice) = &callee.kind
                && let Some(hir::Res::Item(hir::ItemId::Function(func_id))) = res_slice.first()
            {
                return self.first_return_type(*func_id);
            }

            if let ExprKind::Member(base, member) = &callee.kind {
                if let Some(func_id) = self.resolve_using_directive_call(base, member.name) {
                    return self.first_return_type(func_id);
                }

                if let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
                        res_slice.first()
                {
                    let arg_count = args.exprs().count();
                    if let Some(func_id) =
                        self.find_library_function(*contract_id, member.name, arg_count)
                    {
                        return self.first_return_type(func_id);
                    }
                }
            }
        }

        // Case 2: Literal - could be integer, string, etc.
        if let ExprKind::Lit(lit) = &expr.kind {
            use solar_ast::{LitKind, StrKind};
            return match &lit.kind {
                LitKind::Number(_) => Some(self.gcx.types.uint(256)),
                LitKind::Bool(_) => Some(self.gcx.types.bool),
                LitKind::Address(_) => Some(self.gcx.types.address),
                LitKind::Str(StrKind::Hex, ..) => Some(self.gcx.types.bytes),
                LitKind::Str(..) => Some(self.gcx.types.string),
                LitKind::Rational(_) | LitKind::Err(_) => None,
            };
        }

        // Case 3: Binary/unary operations - typically return the operand type
        if let ExprKind::Binary(lhs, _, rhs) = &expr.kind {
            return self.get_expr_type(lhs).or_else(|| self.get_expr_type(rhs));
        }
        if let ExprKind::Unary(_, operand) = &expr.kind {
            return self.get_expr_type(operand);
        }
        if let ExprKind::Tuple(elements) = &expr.kind {
            return elements.iter().flatten().find_map(|expr| self.get_expr_type(expr));
        }

        None
    }

    fn indexed_value_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        let ty = self.get_expr_type(expr)?;
        let loc = ty.loc();
        match ty.peel_refs().kind {
            TyKind::Mapping(_, value) => {
                Some(value.with_loc_if_ref(self.gcx, solar_ast::DataLocation::Storage))
            }
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                Some(element.with_loc_if_ref_opt(self.gcx, loc))
            }
            TyKind::Slice(array) => array.with_loc_if_ref_opt(self.gcx, loc).base_type(self.gcx),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(self.gcx.types.fixed_bytes(1))
            }
            _ => None,
        }
    }

    fn first_return_type(&self, func_id: hir::FunctionId) -> Option<solar_sema::ty::Ty<'gcx>> {
        let func = self.gcx.hir.function(func_id);
        let ret_id = *func.returns.first()?;
        let ret = self.gcx.hir.variable(ret_id);
        Some(self.gcx.type_of_hir_ty(&ret.ty))
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
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return None;
                };
                if self.struct_storage_base_slots.contains_key(var_id) {
                    return None;
                }
                let var = self.gcx.hir.variable(*var_id);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    Some(*struct_id)
                } else {
                    None
                }
            }
            ExprKind::Member(base, member) => {
                let base_struct_id = self.expr_struct_id(base)?;
                let strukt = self.gcx.hir.strukt(base_struct_id);
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    if field.name.is_some_and(|name| name.name == member.name)
                        && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) =
                            &field.ty.kind
                    {
                        return Some(*struct_id);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Checks if an expression type matches a using directive target type.
    pub(super) fn types_match_for_using(
        &self,
        expr_ty: &solar_sema::ty::Ty<'_>,
        target_ty: &hir::Type<'_>,
    ) -> bool {
        use hir::TypeKind;
        use solar_sema::ty::TyKind;

        // A reference-type receiver (struct/array/bytes) has type `Ref(_, loc)`;
        // strip the data location so it matches the location-less target type.
        let expr_ty = expr_ty.peel_refs();

        match (&expr_ty.kind, &target_ty.kind) {
            // Elementary types (uint256, bool, etc.)
            (TyKind::Elementary(e1), TypeKind::Elementary(e2)) => e1 == e2,
            // Contract types
            (TyKind::Contract(c1), TypeKind::Custom(hir::ItemId::Contract(c2))) => c1 == c2,
            // Struct types
            (TyKind::Struct(s1), TypeKind::Custom(hir::ItemId::Struct(s2))) => s1 == s2,
            // Enum types
            (TyKind::Enum(e1), TypeKind::Custom(hir::ItemId::Enum(e2))) => e1 == e2,
            _ => false,
        }
    }

    /// Gets struct info for an expression if it has a struct type.
    /// Returns (struct_id, field_count) if the expression is a struct.
    pub(super) fn get_expr_struct_info(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::StructId, usize)> {
        // Case 1: Variable with struct type (e.g., `p` where `Point memory p`)
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                let strukt = self.gcx.hir.strukt(*struct_id);
                return Some((*struct_id, strukt.fields.len()));
            }
        }

        // Case 2: Struct constructor call like Point(x, y)
        if let ExprKind::Call(callee, _, _) = &expr.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Struct(struct_id))) = res_slice.first()
        {
            let strukt = self.gcx.hir.strukt(*struct_id);
            return Some((*struct_id, strukt.fields.len()));
        }

        None
    }
}
