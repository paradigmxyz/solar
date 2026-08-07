//! L-value place resolution and semantic memory access.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn resolve_lvalue_place(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<LValuePlace<'gcx>> {
        if let Some(place) = self.resolve_storage_byte_place(expr) {
            return Some(place);
        }
        if let Some(access) = self.storage_access(expr) {
            let ty = self.type_of_expr_or_variable(expr)?;
            return Some(LValuePlace::Storage { ty, access, span: expr.span });
        }
        if let Some(id) = self.gcx.resolved_variable(expr) {
            let variable = self.gcx.hir.variable(id);
            if variable.is_state_variable()
                || self.values.contains_key(&id)
                || variable.parent.is_none()
            {
                return Some(LValuePlace::Variable { id, span: expr.span });
            }
        }

        match &expr.kind {
            ExprKind::Member(receiver, name) => {
                if self.gcx.resolved_builtin(expr) == Some(Builtin::ArrayLength)
                    || name.name == sym::offset
                {
                    return report_unsupported(self.gcx, expr.span, "l-value");
                }
                let id = self.gcx.resolved_variable(expr)?;
                let variable = self.gcx.hir.variable(id);
                let Some(hir::ItemId::Struct(struct_id)) = variable.parent else {
                    return report_unsupported(self.gcx, expr.span, "member l-value");
                };
                let Some(field) =
                    self.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)
                else {
                    return report_unsupported(self.gcx, name.span, "struct field l-value");
                };
                let object = self.lower_expr(receiver)?;
                let receiver_ty = self.type_of_expr_or_variable(receiver)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                let ty = self.gcx.type_of_item(id.into());
                Some(LValuePlace::MemoryField { object, layout, field: field as u64, ty })
            }
            ExprKind::Index(receiver, index) => {
                let Some(index) = index else {
                    return report_unsupported(self.gcx, expr.span, "index l-value");
                };
                let object = self.lower_expr(receiver)?;
                let index = self.lower_expr(index)?;
                let receiver_ty = self.type_of_expr_or_variable(receiver)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                match layout {
                    MemoryObjectLayout::DynamicArray { .. } => {
                        let TyKind::DynArray(ty) = receiver_ty.peel_refs().kind else {
                            return report_unsupported(self.gcx, expr.span, "index l-value");
                        };
                        let length = self.builder.memory_object_len(object, layout.kind());
                        self.bounds_check(index, length);
                        Some(LValuePlace::MemoryElement { object, layout, index, ty })
                    }
                    MemoryObjectLayout::FixedArray { len, .. } => {
                        let TyKind::Array(ty, _) = receiver_ty.peel_refs().kind else {
                            return report_unsupported(self.gcx, expr.span, "index l-value");
                        };
                        let length = self.builder.imm_u64(len);
                        self.bounds_check(index, length);
                        Some(LValuePlace::MemoryElement { object, layout, index, ty })
                    }
                    MemoryObjectLayout::Bytes => {
                        let length = self.builder.memory_object_len(object, layout.kind());
                        self.bounds_check(index, length);
                        let ty = self.type_of_expr_or_variable(expr)?;
                        Some(LValuePlace::MemoryByte { object, index, ty })
                    }
                    MemoryObjectLayout::Struct { .. } => {
                        report_unsupported(self.gcx, expr.span, "struct index l-value")
                    }
                }
            }
            _ => report_unsupported(self.gcx, expr.span, "l-value"),
        }
    }

    pub(super) fn load_lvalue_place(&mut self, place: &LValuePlace<'gcx>) -> Option<ValueId> {
        match *place {
            LValuePlace::Variable { id, span } => self.load_variable(id, span),
            LValuePlace::Storage { ty, access, span } => self.load_storage_value(ty, access, span),
            LValuePlace::MemoryField { object, layout, field, ty } => {
                let value = self.builder.memory_object_load_field(object, layout, field);
                Some(self.normalize_memory_scalar(ty, value))
            }
            LValuePlace::MemoryElement { object, layout, index, ty } => {
                let value = self.builder.memory_object_load_element(object, layout, index);
                Some(self.normalize_memory_scalar(ty, value))
            }
            LValuePlace::MemoryByte { object, index, ty } => {
                let value = self.builder.memory_object_load_byte(object, index);
                Some(self.normalize_byte_type(ty, value))
            }
            LValuePlace::StorageByte { object, index, ty, .. } => {
                let value = self.builder.memory_object_load_byte(object, index);
                Some(self.normalize_byte_type(ty, value))
            }
        }
    }

    pub(super) fn store_lvalue_place(
        &mut self,
        place: &LValuePlace<'gcx>,
        value: ValueId,
    ) -> Option<()> {
        match *place {
            LValuePlace::Variable { id, span } => self.store_variable(id, value, span),
            LValuePlace::Storage { ty, access, span } => {
                self.store_storage_value(ty, access, value, span)
            }
            LValuePlace::MemoryField { object, layout, field, .. } => {
                self.builder.memory_object_store_field(object, layout, field, value);
                Some(())
            }
            LValuePlace::MemoryElement { object, layout, index, .. } => {
                self.builder.memory_object_store_element(object, layout, index, value);
                Some(())
            }
            LValuePlace::MemoryByte { object, index, .. } => {
                let zero = self.builder.imm_u256(U256::ZERO);
                let value = self.builder.byte(zero, value);
                self.builder.memory_object_store_byte(object, index, value);
                Some(())
            }
            LValuePlace::StorageByte { slot, object, index, .. } => {
                let zero = self.builder.imm_u256(U256::ZERO);
                let value = self.builder.byte(zero, value);
                self.builder.memory_object_store_byte(object, index, value);
                self.store_storage_bytes(slot, object)
            }
        }
    }

    pub(super) fn resolve_storage_byte_place(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<LValuePlace<'gcx>> {
        let ExprKind::Index(receiver, Some(index)) = &expr.peel_parens().kind else { return None };
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?;
        if !receiver_ty.is_ref_at(DataLocation::Storage)
            || !matches!(
                receiver_ty.peel_refs().kind,
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String
                )
            )
        {
            return None;
        }
        let access = self.storage_access(self.peel_bytes_conversion(receiver))?;
        let object = self.load_storage_bytes(access.slot)?;
        let index = self.lower_expr(index)?;
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        self.bounds_check(index, length);
        let ty = self.type_of_expr_or_variable(expr)?;
        Some(LValuePlace::StorageByte { slot: access.slot, object, index, ty })
    }
}
