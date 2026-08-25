//! Literal, member, environment, and shared expression lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_string_literal_word(&mut self, bytes: &[u8]) -> ValueId {
        let len = bytes.len().min(32);
        let mut padded = [0_u8; 32];
        padded[..len].copy_from_slice(&bytes[..len]);
        self.builder.imm_u256(U256::from_be_bytes(padded))
    }

    pub(super) fn lower_fixed_bytes_literal(
        &mut self,
        ty: Ty<'gcx>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            ty.peel_refs().kind
        else {
            return None;
        };
        let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind else {
            return None;
        };
        match &lit.kind {
            LitKind::Str(_, bytes, _) => Some(self.lower_string_literal_word(bytes.as_byte_str())),
            LitKind::Number(value) => {
                let shift = usize::from(32 - size.bytes()) * 8;
                Some(self.builder.imm_u256(*value << shift))
            }
            _ => None,
        }
    }

    pub(super) fn lower_literal(&mut self, kind: LitKind<'_>, span: Span) -> Option<ValueId> {
        match kind {
            LitKind::Str(_, value, _) => self.lower_shared_bytes_literal(value),
            LitKind::Number(value) => Some(self.builder.imm_u256(value)),
            LitKind::Bool(value) => Some(self.builder.imm_bool(value)),
            LitKind::Address(value) => {
                Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())))
            }
            LitKind::Rational(value) if *value.denom() == U256::from(1) => {
                Some(self.builder.imm_u256(*value.numer()))
            }
            _ => report_unsupported(self.context.gcx, span, "literal"),
        }
    }

    pub(super) fn lower_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: Ident,
    ) -> Option<ValueId> {
        // value = member(receiver)
        if let Some(builtin) = self.context.gcx.resolved_builtin(expr) {
            return self.lower_builtin_value(expr, builtin);
        }
        if let Some(value) = self.lower_internal_function_value(expr) {
            return Some(value);
        }
        if let Some(TyKind::Fn(function)) = self.context.gcx.type_of_expr(expr.id).map(|ty| ty.kind)
            && function.is_external()
            && let Some(function_id) = self.context.gcx.resolved_function(expr)
        {
            let address = self.lower_expr(receiver)?;
            let address_shift = self.builder.imm_u64(32);
            let address = self.builder.shl(address_shift, address);
            let selector = self.context.gcx.function_selector(function_id).0;
            let selector = self.builder.imm_u256(U256::from_be_slice(&selector));
            return Some(self.builder.or(address, selector));
        }
        if name.name == sym::offset
            && self
                .type_of_expr_or_variable(receiver)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))
        {
            return self.lower_yul_member(expr, receiver, name);
        }
        if let Some(access) = self.storage_access(expr) {
            return self.load_storage_access(expr, access);
        }
        if name.name == sym::length {
            let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?;
            if matches!(
                receiver_ty.peel_refs().kind,
                TyKind::StringLiteral(..)
                    | TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
            ) && let ExprKind::Lit(lit) = self.peel_bytes_conversion(receiver).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                return Some(self.builder.imm_u64(bytes.as_byte_str().len() as u64));
            }
            return self.lower_array_length(receiver, receiver_ty, expr.span, "length member");
        }

        let resolved = self.context.gcx.resolved_expr(expr)?;
        let id = resolved.as_variable()?;
        let variable = self.context.gcx.hir.variable(id);
        if variable.is_constant() {
            return self.lower_constant_variable(id, expr.span);
        }
        if let Some(index) = resolved.enum_variant_index(&self.context.gcx.hir) {
            return Some(self.builder.imm_u256(U256::from(index)));
        }
        if variable.is_state_variable() {
            return self.load_variable(id, expr.span);
        }
        let Some(field) = resolved.struct_field_index(&self.context.gcx.hir) else {
            return report_unsupported(self.context.gcx, expr.span, "struct field");
        };
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        let object = self.lower_expr(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && self.builder.func().value_slice_location(object) == Some(SliceLocation::Calldata)
        {
            let AbiType::Tuple(fields) = self.types.abi_type(receiver_ty)? else {
                return report_unsupported(self.context.gcx, expr.span, "calldata struct field");
            };
            let offset = self.builder.imm_u64(fields[..field].iter().map(AbiType::head_size).sum());
            let base = self.builder.slice_ptr(object);
            let head = self.builder.add(base, offset);
            let field_ty = self
                .context
                .gcx
                .type_of_item(id.into())
                .with_loc_if_ref(self.context.gcx, DataLocation::Calldata);
            let validate_bounds = fields[field].is_dynamic();
            return self.materialize_calldata_value_at_inner(
                field_ty,
                head,
                base,
                expr.span,
                validate_bounds,
            );
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        let value = self.builder.memory_object_load_field(object, layout, field as u64);
        let field_ty = self.context.gcx.type_of_item(id.into());
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && let TyKind::Fn(function) = field_ty.peel_refs().kind
            && function.is_external()
        {
            let inst = match self.builder.func().value(value) {
                Value::Inst(inst) => Some(*inst),
                _ => None,
            };
            if let Some(inst) = inst {
                self.builder.func_mut().inst_mut(inst).metadata.set_abi_validation(true);
            }
        }
        Some(self.normalize_memory_scalar(field_ty, value))
    }

    pub(super) fn lower_array_length(
        &mut self,
        receiver: &hir::Expr<'_>,
        receiver_ty: Ty<'gcx>,
        span: Span,
        what: &'static str,
    ) -> Option<ValueId> {
        // length = sload(slot)
        // length = slice.len
        if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
            if !matches!(receiver.peel_parens().kind, ExprKind::Ident(_)) {
                self.lower_expr(receiver)?;
            }
            return Some(self.builder.imm_u64(u64::try_from(len).ok()?));
        }
        if receiver_ty.is_ref_at(DataLocation::Storage) {
            if let Some(access) = self.storage_access(receiver) {
                return match receiver_ty.peel_refs().kind {
                    TyKind::DynArray(_) => Some(self.builder.sload(access.slot)),
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                        let object = self.load_storage_bytes(access.slot);
                        Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                    }
                    _ => report_unsupported(self.context.gcx, span, what),
                };
            }
            let object = self.lower_expr(receiver)?;
            return match self.builder.func().value_ty(object) {
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                    Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                }
                _ => report_unsupported(self.context.gcx, span, what),
            };
        }
        let object = self.lower_expr(receiver)?;
        if matches!(self.builder.func().value_ty(object), Some(MirType::Slice(_))) {
            return Some(self.builder.slice_len(object));
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        match layout.kind() {
            MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                Some(self.builder.memory_object_len(object, layout.kind()))
            }
            _ => report_unsupported(self.context.gcx, span, what),
        }
    }

    pub(super) fn lower_yul_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: Ident,
    ) -> Option<ValueId> {
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata) {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                sym::offset => Some(self.builder.slice_ptr(value)),
                sym::length => Some(self.builder.slice_len(value)),
                _ => report_unsupported(self.context.gcx, expr.span, "Yul calldata member"),
            };
        }

        if let TyKind::Fn(function) = receiver_ty.peel_refs().kind
            && function.is_external()
        {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                kw::Address => Some(self.external_function_address(value)),
                sym::selector => {
                    let mask = self.builder.imm_u256(U256::from(u32::MAX));
                    Some(self.builder.and(value, mask))
                }
                _ => report_unsupported(self.context.gcx, expr.span, "Yul function member"),
            };
        }

        let Some(access) = self.storage_access(receiver) else {
            return report_unsupported(self.context.gcx, expr.span, "Yul storage member");
        };
        match name.name {
            sym::slot => Some(access.slot),
            sym::offset => Some(
                access
                    .offset
                    .unwrap_or_else(|| self.builder.imm_u64(u64::from(access.location.offset))),
            ),
            _ => report_unsupported(self.context.gcx, expr.span, "Yul storage member"),
        }
    }

    pub(super) fn type_of_expr_or_variable(&self, expr: &hir::Expr<'_>) -> Option<Ty<'gcx>> {
        self.context.gcx.type_of_expr(expr.id).or_else(|| {
            self.context
                .gcx
                .resolved_variable(expr)
                .map(|id| self.context.gcx.type_of_item(id.into()))
        })
    }

    pub(super) fn normalize_byte_value(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let Some(ty) = self.context.gcx.type_of_expr(expr.id) else { return value };
        self.normalize_byte_type(ty, value)
    }

    pub(super) fn normalize_byte_type(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let TyKind::Elementary(ElementaryType::FixedBytes(size)) = ty.peel_refs().kind else {
            return value;
        };
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Type(ty) = &callee.kind
            && matches!(
                ty.kind,
                hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
            && let hir::CallArgsKind::Unnamed([inner]) = args.kind
        {
            return inner;
        }
        expr
    }

    pub(super) fn lower_constant_variable(
        &mut self,
        id: VariableId,
        span: Span,
    ) -> Option<ValueId> {
        let variable = self.context.gcx.hir.variable(id);
        let Some(initializer) = variable.initializer else {
            return report_unsupported(self.context.gcx, span, "constant initializer");
        };
        let ty = self.context.gcx.type_of_item(id.into());
        if let Ok(value) = self.context.gcx.try_eval_const_value(initializer) {
            return match value {
                ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
                ConstValue::Integer(value) => {
                    let value = value.as_u256()?;
                    if let TyKind::Elementary(ElementaryType::FixedBytes(size)) =
                        ty.peel_refs().kind
                    {
                        let shift = usize::from(32 - size.bytes()) * 8;
                        Some(self.builder.imm_u256(value << shift))
                    } else {
                        Some(self.builder.imm_u256(value))
                    }
                }
                ConstValue::String(value) => {
                    let bytes = value.as_byte_str_in(self.context.gcx.sess);
                    if matches!(
                        ty.peel_refs().kind,
                        TyKind::Elementary(ElementaryType::FixedBytes(_))
                    ) {
                        Some(self.lower_string_literal_word(bytes))
                    } else {
                        self.lower_shared_bytes_literal(*value)
                    }
                }
            };
        }
        self.lower_expr(initializer)
    }
}
