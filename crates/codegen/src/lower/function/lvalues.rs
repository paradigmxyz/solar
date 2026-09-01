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
                if self.cx.gcx.resolved_builtin(expr) == Some(Builtin::ArrayLength)
                    || (name.name == sym::offset
                        && self
                            .type_of_expr_or_variable(receiver)
                            .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata)))
                {
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
            LValuePlace::MemoryByte { object, index, ty }
            | LValuePlace::StorageByte { object, index, ty, .. }
            | LValuePlace::StorageBytePush { object, index, ty, .. } => {
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
            LValuePlace::StorageByte { slot, object, index, .. }
            | LValuePlace::StorageBytePush { slot, object, index, .. } => {
                self.store_byte(object, index, value);
                self.store_storage_bytes(slot, object)
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
        if let ExprKind::Call(callee, arguments, _) = &expr.kind
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
            let (object, index) = self.grow_storage_bytes(access.slot);
            let ty = self.type_of_expr_or_variable(expr)?;
            return Some(LValuePlace::StorageBytePush { slot: access.slot, object, index, ty });
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
        let object = self.load_storage_bytes(access.slot);
        let index = self.lower_typed_expr(index, self.cx.gcx.types.uint(256))?;
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        self.builder.bounds_check(index, length);
        let ty = self.type_of_expr_or_variable(expr)?;
        Some(LValuePlace::StorageByte { slot: access.slot, object, index, ty })
    }

    pub(super) fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        if let Some(value) = self.values.get(&id).copied() {
            return Some(value);
        }
        if self.default_bindings.contains(&id) || self.deferred_bindings.contains(&id) {
            let ty = self.cx.gcx.type_of_item(id.into());
            let value = self.default_binding_value(ty);
            self.values.insert(id, value);
            return Some(value);
        }
        if let Some(&immutable_id) = self.cx.immutable_ids.get(&id) {
            let ty = self.cx.gcx.type_of_item(id.into());
            return Some(
                self.builder.load_immutable(immutable_id, types::TypeLowerer::mir_type(ty)),
            );
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
                return self.cx.report_unsupported(span, "mapping value");
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
        if self.in_inline_assembly {
            self.dirty_values.insert(value);
            if self.builder.func().value_slice_location(value) != Some(SliceLocation::Calldata)
                && let Some(previous) = self.values.get(&id).copied()
                && self.builder.func().value_slice_location(previous)
                    == Some(SliceLocation::Calldata)
            {
                let length = self.builder.slice_len(previous);
                value = self.builder.make_slice(value, length, SliceLocation::Calldata);
            }
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
        let expr = expr.peel_parens();
        match &expr.kind {
            ExprKind::Member(receiver, name)
                if self.cx.gcx.resolved_builtin(expr) == Some(Builtin::ArrayLength)
                    || (name.name == sym::offset
                        && self
                            .type_of_expr_or_variable(receiver)
                            .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))) =>
            {
                return self.store_yul_member(receiver, *name, value, expr.span);
            }
            ExprKind::YulMember(receiver, name) => {
                return self.store_yul_member(receiver, *name, value, expr.span);
            }
            _ => {}
        }
        let place = self.resolve_lvalue_place(expr)?;
        self.store_lvalue_place_with_source(&place, value, source_ty)
    }

    fn store_yul_member(
        &mut self,
        receiver: &hir::Expr<'_>,
        name: Ident,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata) {
            // pointer, length = slice(receiver)
            // slice = name == offset ? (value, length) : (pointer, value)
            // store(receiver, slice)
            let base = self.lower_expr(receiver)?;
            let pointer = self.builder.slice_ptr(base);
            let length = self.builder.slice_len(base);
            let slice = match name.name {
                sym::offset => self.builder.make_slice(value, length, SliceLocation::Calldata),
                sym::length => self.builder.make_slice(pointer, value, SliceLocation::Calldata),
                _ => return self.cx.report_unsupported(span, "Yul calldata assignment"),
            };
            return self.store_lvalue(receiver, slice);
        }

        if let TyKind::Fn(function) = receiver_ty.peel_refs().kind
            && function.is_external()
        {
            // pointer = load(receiver)
            // pointer = name == address ? (value, pointer.selector) : (pointer.address, value)
            // store(receiver, pointer)
            let Some(id) = self.cx.gcx.resolved_variable(receiver) else {
                return self.cx.report_unsupported(span, "Yul function assignment target");
            };
            let pointer = self.load_variable(id, span)?;
            let mask = self.builder.imm(u32::MAX);
            let value = match name.name {
                kw::Address => {
                    let address_mask = self.builder.imm(U256::MAX >> 96);
                    let address = self.builder.and(value, address_mask);
                    let shift = self.builder.imm(32);
                    let address = self.builder.shl(shift, address);
                    let selector = self.builder.and(pointer, mask);
                    self.builder.or(address, selector)
                }
                sym::selector => {
                    let selector = self.builder.and(value, mask);
                    let address_mask = self.builder.not(mask);
                    let address = self.builder.and(pointer, address_mask);
                    self.builder.or(address, selector)
                }
                _ => return self.cx.report_unsupported(span, "Yul function assignment"),
            };
            return self.store_variable(id, value, span);
        }

        if name.name != sym::slot {
            return self.cx.report_unsupported(span, "Yul storage assignment");
        }
        let Some(id) = self.cx.gcx.resolved_variable(receiver) else {
            return self.cx.report_unsupported(span, "Yul storage assignment target");
        };
        if self.cx.gcx.hir.variable(id).is_state_variable() {
            return self.cx.report_unsupported(span, "Yul state-variable slot assignment");
        }
        let Some(access) = self.storage_refs.get(&id).copied() else {
            return self.cx.report_unsupported(span, "Yul storage assignment target");
        };

        // storage_ref.slot = value
        self.storage_refs.insert(id, StorageAccess { slot: value, ..access });
        Some(())
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
