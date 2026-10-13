//! L-value place resolution and semantic memory access.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    fn can_access_variable(&self, id: hir::VariableId) -> bool {
        let variable = self.cx.gcx.hir.variable(id);
        variable.is_state_variable()
            || self.values.contains_key(&id)
            || self.default_bindings.contains(&id)
            || self.deferred_bindings.contains(&id)
            || variable.parent.is_none()
    }

    pub(super) fn resolve_lvalue_place(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<LValuePlace<'gcx>> {
        let expr = expr.peel_parens();
        if let ExprKind::YulMember(receiver, name) = &expr.kind {
            return self.resolve_yul_member_place(receiver, *name, expr.span);
        }
        if let Some(place) = self.resolve_storage_byte_place(expr) {
            return Some(place);
        }
        if let Some(access) = self.storage_access(expr) {
            let ty = self.type_of_expr_or_variable(expr)?;
            return Some(LValuePlace::Storage { ty, access, span: expr.span });
        }
        if let Some(id) = self.cx.gcx.resolved_variable(expr)
            && self.can_access_variable(id)
        {
            return Some(LValuePlace::Variable { id, span: expr.span });
        }

        match &expr.kind {
            ExprKind::Member(receiver, name) => {
                if self.cx.gcx.resolved_builtin(expr) == Some(Builtin::ArrayLength) {
                    return self.cx.report_unsupported(expr.span, "l-value");
                }
                let resolved = self.cx.gcx.resolved_expr(expr)?;
                let id = resolved.as_variable()?;
                let Some(field) = resolved.struct_field_index(&self.cx.gcx.hir) else {
                    return self.cx.report_unsupported(name.span, "struct field l-value");
                };
                let object = self.lower_expr(receiver)?;
                let receiver_ty = self.type_of_expr_or_variable(receiver)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                let ty = self.cx.gcx.type_of_item(id.into());
                Some(LValuePlace::MemoryField { object, layout, field: field as u64, ty })
            }
            ExprKind::Index(receiver, index) => {
                let Some(index) = index else {
                    return self.cx.report_unsupported(expr.span, "index l-value");
                };
                let object = self.lower_expr(receiver)?;
                let index = self.lower_typed_expr(index, self.cx.gcx.types.uint(256))?;
                let receiver_ty = self.type_of_expr_or_variable(receiver)?;
                let layout = self.types.memory_layout(receiver_ty)?;
                match layout {
                    MemoryObjectLayout::DynamicArray { .. }
                    | MemoryObjectLayout::FixedArray { .. } => {
                        let Some((ty, length)) =
                            self.array_element_and_length(receiver_ty, object, layout)
                        else {
                            return self.cx.report_unsupported(expr.span, "index l-value");
                        };
                        self.builder.bounds_check(index, length);
                        Some(LValuePlace::MemoryElement { object, layout, index, ty })
                    }
                    MemoryObjectLayout::Bytes => {
                        let length = self.builder.memory_object_len(object, layout.kind());
                        self.builder.bounds_check(index, length);
                        let ty = self.type_of_expr_or_variable(expr)?;
                        Some(LValuePlace::MemoryByte { object, index, ty })
                    }
                    MemoryObjectLayout::Struct { .. } => {
                        self.cx.report_unsupported(expr.span, "struct index l-value")
                    }
                }
            }
            _ => self.cx.report_unsupported(expr.span, "l-value"),
        }
    }

    fn resolve_yul_member_place(
        &mut self,
        receiver: &hir::Expr<'_>,
        name: Ident,
        span: Span,
    ) -> Option<LValuePlace<'gcx>> {
        let Some(id) = self.cx.gcx.resolved_variable(receiver) else {
            return self.cx.report_unsupported(span, "Yul member");
        };
        let ty = self.cx.gcx.type_of_item(id.into());
        let storage =
            self.cx.gcx.hir.variable(id).is_state_variable() || ty.is_ref_at(DataLocation::Storage);
        let calldata = ty.is_ref_at(DataLocation::Calldata);
        let function = matches!(ty.kind, TyKind::Fn(function) if function.is_external());
        let member = match name.name {
            sym::slot if storage => YulMember::StorageSlot,
            sym::offset if storage => YulMember::StorageOffset,
            sym::offset if calldata => YulMember::CalldataOffset,
            sym::length if calldata => YulMember::CalldataLength,
            kw::Address if function => YulMember::FunctionAddress,
            sym::selector if function => YulMember::FunctionSelector,
            _ => return self.cx.report_unsupported(span, "Yul member"),
        };
        Some(LValuePlace::YulMember { id, member, span })
    }

    pub(super) fn load_lvalue_place(&mut self, place: &LValuePlace<'gcx>) -> Option<ValueId> {
        match *place {
            LValuePlace::Variable { id, span } => self.load_variable(id, span),
            LValuePlace::Storage { ty, access, span } => self.load_storage_value(ty, access, span),
            LValuePlace::MemoryField { object, layout, field, ty } => {
                // value = memory_object_load_field layout, object, field
                let value = self.builder.memory_object_load_field_as(
                    object,
                    layout,
                    field,
                    types::TypeLowerer::mir_type(ty),
                );
                Some(self.normalize_memory_scalar(ty, value))
            }
            LValuePlace::MemoryElement { object, layout, index, ty } => {
                // value = memory_object_load_element layout, object, index
                let value = self.builder.memory_object_load_element_as(
                    object,
                    layout,
                    index,
                    types::TypeLowerer::mir_type(ty),
                );
                Some(self.normalize_memory_scalar(ty, value))
            }
            LValuePlace::MemoryByte { object, index, ty } => {
                let value = self.builder.memory_object_load_byte(object, index);
                Some(self.normalize_byte_type(ty, value))
            }
            LValuePlace::YulMember { id, member, span } => self.load_yul_member(id, member, span),
        }
    }

    pub(super) fn store_lvalue_place(
        &mut self,
        place: &LValuePlace<'gcx>,
        value: ValueId,
    ) -> Option<()> {
        self.store_lvalue_place_with_source(place, value, None)
    }

    pub(super) fn store_lvalue_place_with_source(
        &mut self,
        place: &LValuePlace<'gcx>,
        value: ValueId,
        source_ty: Option<Ty<'gcx>>,
    ) -> Option<()> {
        match *place {
            LValuePlace::Variable { id, span } => self.store_variable(id, value, span),
            LValuePlace::Storage { ty, access, span } => match source_ty {
                Some(source_ty) => {
                    self.store_storage_value_with_source(ty, source_ty, access, value, span)
                }
                None => self.store_storage_value(ty, access, value, span),
            },
            LValuePlace::MemoryField { object, layout, field, ty } => {
                let value = self.encode_memory_scalar(ty, value);
                self.builder.memory_object_store_field(object, layout, field, value);
                Some(())
            }
            LValuePlace::MemoryElement { object, layout, index, ty } => {
                let value = self.encode_memory_scalar(ty, value);
                self.builder.memory_object_store_element(object, layout, index, value);
                Some(())
            }
            LValuePlace::MemoryByte { object, index, .. } => {
                self.store_byte(object, index, value);
                Some(())
            }
            LValuePlace::YulMember { id, member, span } => {
                self.store_yul_member(id, member, value, span)
            }
        }
    }

    fn store_byte(&mut self, object: ValueId, index: ValueId, value: ValueId) {
        let zero = self.builder.imm(U256::ZERO);
        let value = self.builder.byte(zero, value);
        self.builder.memory_object_store_byte(object, index, value);
    }

    pub(super) fn resolve_storage_byte_place(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<LValuePlace<'gcx>> {
        let expr = expr.peel_parens();
        if let Some((callee, arguments, _)) = expr.as_call()
            && arguments.is_empty()
            && self.cx.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0)
            && let ExprKind::Member(receiver, _) = &callee.kind
            && self.cx.gcx.type_of_expr(receiver.id).is_some_and(|ty| {
                ty.is_ref_at(DataLocation::Storage)
                    && matches!(ty.peel_refs().kind, TyKind::Elementary(ElementaryType::Bytes))
            })
        {
            let Some(access) = self.storage_access(receiver) else {
                return self.cx.report_unsupported(receiver.span, "storage access");
            };
            let access = self.grow_storage_bytes(access.slot);
            let ty = self.type_of_expr_or_variable(expr)?;
            return Some(LValuePlace::Storage { ty, access, span: expr.span });
        }

        let ExprKind::Index(receiver, Some(index)) = &expr.kind else { return None };
        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?;
        if !receiver_ty.is_ref_at(DataLocation::Storage)
            || !matches!(
                receiver_ty.peel_refs().kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
        {
            return None;
        }
        let receiver = self.peel_bytes_conversion(receiver);
        let Some(access) = self.storage_access(receiver) else {
            return self.cx.report_unsupported(receiver.span, "storage access");
        };
        let index = self.lower_typed_expr(index, self.cx.gcx.types.uint(256))?;
        let ty = self.type_of_expr_or_variable(expr)?;
        let access = self.storage_bytes_byte_access(access.slot, index);
        Some(LValuePlace::Storage { ty, access, span: expr.span })
    }

    pub(super) fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        if let Some(value) = self.values.get(&id).copied() {
            return Some(value);
        }
        if self.default_bindings.contains(&id) || self.deferred_bindings.contains(&id) {
            let ty = self.cx.gcx.type_of_item(id.into());
            let value = self.default_binding_value(ty);
            let value = self.materialize_raw_scalar(id, value);
            self.values.insert(id, value);
            return Some(value);
        }
        if let Some(&immutable_id) = self.cx.immutable_ids.get(&id) {
            let ty = self.cx.module.immutable_type(immutable_id).mir_type();
            let value = self.builder.load_immutable(immutable_id, ty);
            let source_ty = self.cx.gcx.type_of_item(id.into());
            if ty != types::TypeLowerer::mir_type(source_ty) {
                self.dirty_values.insert(value);
            }
            return Some(value);
        }
        if let Some(access) = self.storage_refs.get(&id).copied() {
            let ty = self.cx.gcx.type_of_item(id.into());
            if ty.is_ref_at(DataLocation::Storage) {
                return Some(access.slot);
            }
            let value = self.cx.storage.load_at(&mut self.builder, access.location, access.slot);
            self.validate_enum(ty, value);
            return Some(value);
        }
        let var = self.cx.gcx.hir.variable(id);
        if var.is_constant() {
            return self.lower_constant_variable(id, span);
        }
        if let Some(location) = self.cx.storage.get(id) {
            let ty = self.cx.gcx.type_of_item(id.into());
            if matches!(ty.peel_refs().kind, TyKind::Mapping(..)) {
                return Some(self.builder.imm(location.slot));
            }
            let slot = self.builder.imm(location.slot);
            return self.load_storage_value(
                ty,
                StorageAccess { slot, location, offset: None },
                span,
            );
        }
        self.cx.report_unsupported(span, "identifier")
    }

    pub(super) fn store_variable(
        &mut self,
        id: VariableId,
        mut value: ValueId,
        span: Span,
    ) -> Option<()> {
        if !self.in_inline_assembly {
            value = self.materialize_raw_scalar(id, value);
        }
        if self.in_inline_assembly {
            let ty = self.cx.gcx.type_of_item(id.into());
            if self.builder.func().value_slice_location(value) != Some(SliceLocation::Calldata)
                && ty.is_ref_at(DataLocation::Calldata)
            {
                // slice = calldata(value, previous.length | static.length | 0)
                let length = match self.values.get(&id).copied() {
                    Some(previous)
                        if self.builder.func().value_slice_location(previous)
                            == Some(SliceLocation::Calldata) =>
                    {
                        self.builder.slice_len(previous)
                    }
                    _ => match ty.peel_refs().kind {
                        TyKind::Array(_, length) => {
                            self.builder.imm(u64::try_from(length).unwrap_or(0))
                        }
                        _ => self.builder.imm(U256::ZERO),
                    },
                };
                value = self.builder.make_slice(value, length, SliceLocation::Calldata);
            }
            self.dirty_values.insert(value);
        }
        if let StdEntry::Occupied(mut entry) = self.values.entry(id) {
            entry.insert(value);
            return Some(());
        }
        if self.default_bindings.contains(&id) || self.deferred_bindings.contains(&id) {
            self.values.insert(id, value);
            return Some(());
        }
        if let Some(&immutable_id) = self.cx.immutable_ids.get(&id) {
            // value = cast to the immutable value type
            let ty = self.cx.module.immutable_type(immutable_id).mir_type();
            let value = self.builder.cast(value, ty);
            self.builder.store_immutable(immutable_id, value);
            return Some(());
        }
        if let Some(location) = self.cx.storage.get(id) {
            let ty = self.cx.gcx.type_of_item(id.into());
            self.validate_enum(ty, value);
            self.cx.storage.store(&mut self.builder, location, value);
            return Some(());
        }
        self.cx.report_unsupported(span, "assignment target")
    }

    pub(super) fn store_state_variable(
        &mut self,
        id: VariableId,
        value: ValueId,
        source_ty: Ty<'gcx>,
        span: Span,
    ) -> Option<()> {
        let ty = self.cx.gcx.type_of_item(id.into());
        let Some(location) = self.cx.storage.get(id) else {
            return self.cx.report_unsupported(span, "state initializer target");
        };
        if self.types.memory_layout(ty).is_some() {
            let slot = self.builder.imm(location.slot);
            self.store_storage_object_with_source(ty, source_ty, slot, value, span)
        } else {
            self.validate_enum(ty, value);
            self.cx.storage.store(&mut self.builder, location, value);
            Some(())
        }
    }

    pub(super) fn store_lvalue(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> Option<()> {
        self.store_lvalue_with_source(expr, value, None)
    }

    pub(super) fn store_lvalue_with_source(
        &mut self,
        expr: &hir::Expr<'_>,
        value: ValueId,
        source_ty: Option<Ty<'gcx>>,
    ) -> Option<()> {
        let place = self.resolve_lvalue_place(expr)?;
        self.store_lvalue_place_with_source(&place, value, source_ty)
    }

    fn load_yul_member(
        &mut self,
        id: VariableId,
        member: YulMember,
        span: Span,
    ) -> Option<ValueId> {
        if let YulMember::StorageSlot | YulMember::StorageOffset = member {
            let Some(access) = self.variable_storage_access(id) else {
                return self.cx.report_unsupported(span, "Yul storage member");
            };
            return Some(match (member, access.offset) {
                (YulMember::StorageSlot, _) => access.slot,
                (_, Some(offset)) => offset,
                (_, None) => self.builder.imm(u64::from(access.location.offset)),
            });
        }
        let current = self.load_variable(id, span)?;
        Some(match member {
            YulMember::CalldataOffset => self.builder.slice_ptr(current),
            YulMember::CalldataLength => self.builder.slice_len(current),
            YulMember::FunctionAddress => self.external_function_address(current),
            YulMember::FunctionSelector => {
                let mask = self.builder.imm(u32::MAX);
                self.builder.and(current, mask)
            }
            YulMember::StorageSlot | YulMember::StorageOffset => unreachable!(),
        })
    }

    fn store_yul_member(
        &mut self,
        id: VariableId,
        member: YulMember,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        match member {
            YulMember::StorageSlot if self.cx.gcx.hir.variable(id).is_state_variable() => {
                return self.cx.report_unsupported(span, "Yul state-variable slot assignment");
            }
            YulMember::StorageSlot => {
                self.set_storage_ref_slot(id, value);
                return Some(());
            }
            YulMember::StorageOffset => {
                return self.cx.report_unsupported(span, "Yul storage offset assignment");
            }
            _ => {}
        }
        let current = self.load_variable(id, span)?;
        let mask = self.builder.imm(u32::MAX);
        let value = match member {
            YulMember::CalldataOffset => {
                // slice = (value, slice_len(current))
                let length = self.builder.slice_len(current);
                self.builder.make_slice(value, length, SliceLocation::Calldata)
            }
            YulMember::CalldataLength => {
                // slice = (slice_ptr(current), value)
                let pointer = self.builder.slice_ptr(current);
                self.builder.make_slice(pointer, value, SliceLocation::Calldata)
            }
            YulMember::FunctionAddress => {
                // pointer = ((value & address_mask) << 32) | (current & 0xffffffff)
                let address_mask = self.builder.imm(U256::MAX >> 96);
                let address = self.builder.and(value, address_mask);
                let shift = self.builder.imm(32);
                let address = self.builder.shl(shift, address);
                let selector = self.builder.and(current, mask);
                self.builder.or(address, selector)
            }
            YulMember::FunctionSelector => {
                // pointer = (current & ~0xffffffff) | (value & 0xffffffff)
                let selector = self.builder.and(value, mask);
                let address_mask = self.builder.not(mask);
                let address = self.builder.and(current, address_mask);
                self.builder.or(address, selector)
            }
            YulMember::StorageSlot | YulMember::StorageOffset => unreachable!(),
        };
        self.store_variable(id, value, span)
    }

    pub(super) fn delete_lvalue(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        if ty.is_ref_at(DataLocation::Storage)
            && let Some(access) = self.storage_access(expr)
        {
            return self.clear_storage_access(ty, access, expr.span);
        }
        if self.types.memory_layout(ty).is_none() {
            let zero = self.builder.imm(U256::ZERO);
            return self.store_lvalue(expr, zero);
        }
        let place = self.resolve_lvalue_place(expr)?;
        let object = self.default_object(ty)?;
        self.store_lvalue_place(&place, object)?;
        Some(())
    }
}
