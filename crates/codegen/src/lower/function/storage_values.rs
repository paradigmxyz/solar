//! Storage-reference access and aggregate materialization for one lowered function.

use super::*;

/// Adds the module-wide helper for decoding one storage `bytes`/`string` slot.
pub(super) fn synthesize_storage_bytes_helper(module: &mut Module) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::__load_storage_bytes));
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let slot = builder.add_param(MirType::uint256());
        builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
        let object = lower_storage_bytes_inline(&mut builder, slot);
        builder.ret([object]);
    }
    module.add_function(function)
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn is_constant_storage_assignment(
        &self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
    ) -> bool {
        let ExprKind::Ident(_) = lhs.peel_parens().kind else { return false };
        let Some(id) = self.gcx.resolved_variable(lhs) else { return false };
        if !self.gcx.hir.variable(id).is_state_variable() {
            return false;
        }
        let Some(lhs_ty) = self.gcx.type_of_expr(lhs.id) else { return false };
        let Some(rhs_ty) = self.gcx.type_of_expr(rhs.id) else { return false };
        let target_ty = lhs_ty.peel_refs();
        if self.types.memory_layout(target_ty).is_none() || target_ty != rhs_ty.peel_refs() {
            return false;
        }
        match target_ty.kind {
            TyKind::Struct(_) => {
                matches!(rhs.peel_parens().kind, ExprKind::Call(..))
                    && self.is_constant_storage_value(rhs, target_ty)
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => {
                matches!(rhs.peel_parens().kind, ExprKind::Lit(..))
                    && self.is_constant_storage_value(rhs, target_ty)
            }
            _ => false,
        }
    }

    fn is_constant_storage_value(&self, expr: &hir::Expr<'_>, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => matches!(self.gcx.try_eval_const_value(expr), Ok(ConstValue::String(_))),
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return false;
                };
                let Some(hir::Res::Item(hir::ItemId::Struct(id))) = self.gcx.resolved_expr(callee)
                else {
                    return false;
                };
                if id != struct_id {
                    return false;
                }
                let fields = self.gcx.hir.strukt(struct_id).fields;
                if args.len() != fields.len() {
                    return false;
                }
                let names = self.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                fields.iter().enumerate().all(|(index, &field)| {
                    args.argument_for_parameter(index, Some(names.as_slice())).is_some_and(
                        |argument| {
                            self.is_constant_storage_value(
                                argument,
                                self.gcx.type_of_item(field.into()),
                            )
                        },
                    )
                })
            }
            TyKind::Array(..) | TyKind::DynArray(..) => false,
            TyKind::Elementary(_) => self.gcx.try_eval_const_value(expr).is_ok(),
            _ => false,
        }
    }

    pub(super) fn lower_constant_storage_assignment(
        &mut self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let ty = self.gcx.type_of_expr(lhs.id)?.peel_refs();
        let access = self.storage_access(lhs)?;
        self.store_constant_storage_value(ty, access, rhs)
    }

    fn store_constant_storage_value(
        &mut self,
        ty: Ty<'gcx>,
        access: StorageAccess,
        expr: &hir::Expr<'_>,
    ) -> Option<()> {
        match ty.peel_refs().kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => {
                let Ok(ConstValue::String(value)) = self.gcx.try_eval_const_value(expr) else {
                    return None;
                };
                self.store_constant_storage_bytes(access.slot, value.as_byte_str_in(self.gcx.sess));
                Some(())
            }
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return None;
                };
                let hir::Res::Item(hir::ItemId::Struct(id)) = self.gcx.resolved_expr(callee)?
                else {
                    return None;
                };
                if id != struct_id {
                    return None;
                }
                let fields = self.gcx.hir.strukt(struct_id).fields;
                let names = self.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                for (index, &field) in fields.iter().enumerate() {
                    let argument = args.argument_for_parameter(index, Some(names.as_slice()))?;
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(struct_id, index)?;
                    let slot = self.add_storage_offset(access.slot, location.slot);
                    self.store_constant_storage_value(
                        field_ty,
                        StorageAccess { slot, location, offset: None },
                        argument,
                    )?;
                }
                Some(())
            }
            _ => {
                let source_ty = self.gcx.type_of_expr(expr.id)?;
                let value = match self.gcx.try_eval_const_value(expr) {
                    Ok(ConstValue::Bool(value)) => self.builder.imm_bool(*value),
                    Ok(ConstValue::Integer(value)) => self.builder.imm_u256(value.as_u256()?),
                    _ => self.lower_typed_expr(expr, ty)?,
                };
                let value = self.coerce_value(value, source_ty, ty);
                self.store_storage_value(ty, access, value, expr.span)
            }
        }
    }

    pub(super) fn storage_access(&mut self, expr: &hir::Expr<'_>) -> Option<StorageAccess> {
        match &expr.peel_parens().kind {
            ExprKind::Ident(_) => {
                let id = self.gcx.resolved_variable(expr)?;
                if let Some(access) = self.storage_refs.get(&id).copied() {
                    return Some(access);
                }
                let var = self.gcx.hir.variable(id);
                if !var.is_state_variable() {
                    return None;
                }
                let location = self.storage.get(id)?;
                let slot = self.builder.imm_u256(location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Member(receiver, _) => {
                let id = self.gcx.resolved_variable(expr)?;
                let variable = self.gcx.hir.variable(id);
                let hir::ItemId::Struct(struct_id) = variable.parent? else { return None };
                let field =
                    self.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)?;
                let base = self.storage_access(receiver)?;
                let location = self.storage.field_location(struct_id, field)?;
                let slot = self.add_storage_offset(base.slot, location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Index(receiver, Some(index)) => {
                let base = self.storage_access(receiver)?;
                let ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
                let index_ty = self.gcx.type_of_expr(index.id)?;
                let index = self.lower_expr(index)?;
                if let TyKind::Mapping(_, value) = ty.kind {
                    let slot = self.mapping_slot(index, index_ty, base.slot);
                    if let Some((size, encoding)) = self.storage.packed_encoding(value) {
                        let location =
                            StorageLocation { slot: U256::ZERO, offset: 0, size, encoding };
                        return Some(StorageAccess { slot, location, offset: None });
                    }
                    return Some(StorageAccess {
                        slot,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    });
                }
                let (element, dynamic, length) = match ty.kind {
                    TyKind::Array(element, len) => {
                        (element, false, self.builder.imm_u64(u64::try_from(len).ok()?))
                    }
                    TyKind::DynArray(element) => (element, true, self.builder.sload(base.slot)),
                    _ => return None,
                };
                self.bounds_check(index, length);
                self.storage_array_element_access(base.slot, index, element, dynamic)
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                self.storage_access_ternary(condition, then_expr, else_expr)
            }
            ExprKind::Call(callee, arguments, _)
                if arguments.is_empty()
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0) =>
            {
                let ExprKind::Member(receiver, _) = &callee.kind else { return None };
                let (access, _, new_length, base_slot) =
                    self.storage_array_push_access(receiver)?;
                self.builder.sstore(base_slot, new_length);
                Some(access)
            }
            ExprKind::Call(callee, ..) if self.call_returns_storage_ref(callee) => {
                let slot = self.lower_expr(expr)?;
                Some(StorageAccess {
                    slot,
                    location: StorageLocation::word(U256::ZERO),
                    offset: None,
                })
            }
            _ => None,
        }
    }

    fn call_returns_storage_ref(&self, callee: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_function(callee).is_some_and(|function_id| {
            self.gcx.hir.function(function_id).returns.first().is_some_and(|&ret| {
                self.gcx.type_of_item(ret.into()).is_ref_at(DataLocation::Storage)
            })
        })
    }

    fn storage_access_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<StorageAccess> {
        let (then_branch, else_branch) =
            self.lower_ternary_branches(condition, then_expr, else_expr, |this, expr| {
                this.storage_access(expr)
            })?;
        let mut incoming = Vec::with_capacity(2);
        if !then_branch.terminated {
            incoming.push((then_branch.block, then_branch.value));
        }
        if !else_branch.terminated {
            incoming.push((else_branch.block, else_branch.value));
        }
        self.merge_storage_accesses(incoming)
    }

    pub(super) fn is_storage_reference_binding(&self, expr: &hir::Expr<'_>) -> bool {
        if !matches!(expr.peel_parens().kind, ExprKind::Ident(_)) {
            return false;
        }
        self.gcx
            .resolved_variable(expr)
            .is_some_and(|id| !self.gcx.hir.variable(id).is_state_variable())
            && self.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
    }

    pub(super) fn storage_array_push_access(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>, ValueId, ValueId)> {
        let base = self.storage_access(receiver)?;
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = receiver_ty.kind else { return None };
        let length = self.builder.sload(base.slot);
        let one = self.builder.imm_u64(1);
        let new_length = self.checked_add(length, one);
        let access = self.storage_array_element_access(base.slot, length, element, true)?;
        Some((access, element, new_length, base.slot))
    }

    pub(super) fn lower_storage_array_push(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        builtin: Builtin,
        arguments: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return report_unsupported(self.gcx, expr.span, "storage array push target");
        };
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String
            )
        ) {
            return self.lower_storage_bytes_push(expr, receiver, builtin, arguments);
        }
        let Some((base, element)) = self.storage_array_base(receiver) else {
            return report_unsupported(self.gcx, expr.span, "storage array push target");
        };
        let value = if builtin == Builtin::ArrayPush {
            let [argument] = arguments else {
                return report_unsupported(self.gcx, expr.span, "storage array push arguments");
            };
            let value = self.lower_typed_expr(argument, element)?;
            let value = self.coerce_value(value, self.gcx.type_of_expr(argument.id)?, element);
            if self.types.memory_layout(element).is_some() {
                self.materialize_memory_argument(element, value, argument.span)?
            } else {
                value
            }
        } else {
            if !arguments.is_empty() {
                return report_unsupported(self.gcx, expr.span, "storage array push arguments");
            }
            self.default_value(element)
        };
        let length = self.builder.sload(base.slot);
        let one = self.builder.imm_u64(1);
        let new_length = self.checked_add(length, one);
        let element_access = self.storage_array_element_access(base.slot, length, element, true)?;
        self.store_storage_value(element, element_access, value, expr.span)?;
        self.builder.sstore(base.slot, new_length);
        Some(self.builder.imm_u256(U256::ZERO))
    }

    fn lower_storage_bytes_push(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        arguments: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        let access = self.storage_access(receiver)?;
        let value = if builtin == Builtin::ArrayPush {
            let [argument] = arguments else {
                return report_unsupported(self.gcx, expr.span, "storage bytes push arguments");
            };
            let source_ty = self.gcx.type_of_expr(argument.id)?;
            let value = self.lower_expr(argument)?;
            let value = self.coerce_value(value, source_ty, self.gcx.types.fixed_bytes(1));
            let shift = self.builder.imm_u64(248);
            self.builder.shr(shift, value)
        } else {
            if !arguments.is_empty() {
                return report_unsupported(self.gcx, expr.span, "storage bytes push arguments");
            }
            self.builder.imm_u64(0)
        };
        let old = self.load_storage_bytes(access.slot)?;
        let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
        let one = self.builder.imm_u64(1);
        let length = self.checked_add(old_length, one);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let layout = MemoryObjectLayout::Bytes;
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        self.builder.set_memory_object_len(object, length, layout.kind());
        self.builder.memory_object_copy(object, layout.kind(), old, layout.kind(), old_length);
        self.builder.memory_object_store_byte(object, old_length, value);
        self.store_storage_bytes(access.slot, object)?;
        Some(self.builder.imm_u256(U256::ZERO))
    }

    pub(super) fn lower_storage_array_pop(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return report_unsupported(self.gcx, expr.span, "storage array pop target");
        };
        let receiver_ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String
            )
        ) {
            let access = self.storage_access(receiver)?;
            let old = self.load_storage_bytes(access.slot)?;
            let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
            let zero = self.builder.imm_u64(0);
            let empty = self.builder.eq(old_length, zero);
            self.panic_if(empty, PanicCode::EmptyArrayPop);
            let one = self.builder.imm_u64(1);
            let length = self.builder.sub(old_length, one);
            let word_size = self.builder.imm_u64(32);
            let thirty_one = self.builder.imm_u64(31);
            let rounded = self.checked_add(length, thirty_one);
            let words = self.builder.div(rounded, word_size);
            let total_words = self.checked_add(words, one);
            let size = self.checked_mul(total_words, word_size);
            let layout = MemoryObjectLayout::Bytes;
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, length, layout.kind());
            self.builder.memory_object_copy(object, layout.kind(), old, layout.kind(), length);
            self.store_storage_bytes(access.slot, object)?;
            return Some(zero);
        }

        let Some((base, element)) = self.storage_array_base(receiver) else {
            return report_unsupported(self.gcx, expr.span, "storage array pop target");
        };
        let length = self.builder.sload(base.slot);
        let zero = self.builder.imm_u64(0);
        let empty = self.builder.eq(length, zero);
        self.panic_if(empty, PanicCode::EmptyArrayPop);
        let one = self.builder.imm_u64(1);
        let last = self.builder.sub(length, one);
        self.builder.sstore(base.slot, last);
        let access = self.storage_array_element_access(base.slot, last, element, true)?;
        let value = self.default_value(element);
        self.store_storage_value(element, access, value, expr.span)?;
        Some(zero)
    }

    fn storage_array_base(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>)> {
        let base = self.storage_access(receiver)?;
        let ty = self.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = ty.kind else { return None };
        Some((base, element))
    }

    fn mapping_slot(&mut self, key: ValueId, key_ty: Ty<'gcx>, slot: ValueId) -> ValueId {
        let is_calldata = key_ty.data_stored_in(DataLocation::Calldata)
            || matches!(key_ty.kind, TyKind::Slice(inner) if inner.data_stored_in(DataLocation::Calldata));
        let is_dynamic = matches!(
            key_ty.peel_refs().kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::String | solar_sema::hir::ElementaryType::Bytes
            ) | TyKind::Slice(_)
                | TyKind::StringLiteral(..)
        );
        if is_dynamic {
            if is_calldata {
                self.builder.mapping_slot_calldata(key, slot)
            } else {
                self.builder.mapping_slot_memory(key, slot)
            }
        } else {
            self.builder.mapping_slot(key, slot)
        }
    }

    pub(super) fn storage_array_element_access(
        &mut self,
        base_slot: ValueId,
        index: ValueId,
        element: solar_sema::ty::Ty<'gcx>,
        dynamic: bool,
    ) -> Option<StorageAccess> {
        if let Some((size, encoding)) = self.storage.packed_encoding(element)
            && size.bits() < 256
        {
            let bytes = u64::from(size.bytes());
            let per_slot_value = self.builder.imm_u64(32 / bytes);
            let slot_base =
                if dynamic { self.builder.storage_array_data_slot(base_slot) } else { base_slot };
            let slot_delta = self.builder.div(index, per_slot_value);
            let slot = self.builder.add(slot_base, slot_delta);
            let index_in_slot = self.builder.mod_(index, per_slot_value);
            let byte_size = self.builder.imm_u64(bytes);
            let offset = self.builder.mul(index_in_slot, byte_size);
            let location = StorageLocation { slot: U256::ZERO, offset: 0, size, encoding };
            return Some(StorageAccess { slot, location, offset: Some(offset) });
        }
        let element_slots = self.storage.element_slots(element);
        let slot = if dynamic {
            self.builder.storage_array_element_slot(base_slot, index, element_slots)
        } else {
            self.fixed_array_element_slot(base_slot, index, element_slots)
        };
        Some(StorageAccess { slot, location: StorageLocation::word(U256::ZERO), offset: None })
    }

    fn fixed_array_element_slot(
        &mut self,
        base_slot: ValueId,
        index: ValueId,
        element_slots: u64,
    ) -> ValueId {
        if element_slots == 1 {
            self.builder.add(base_slot, index)
        } else {
            let stride = self.builder.imm_u64(element_slots);
            let offset = self.builder.mul(index, stride);
            self.builder.add(base_slot, offset)
        }
    }

    pub(super) fn add_storage_offset(&mut self, slot: ValueId, offset: U256) -> ValueId {
        if offset.is_zero() {
            slot
        } else {
            let offset = self.builder.imm_u256(offset);
            self.builder.add(slot, offset)
        }
    }

    pub(super) fn load_storage_access(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
    ) -> Option<ValueId> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        if ty.is_ref_at(DataLocation::Storage) {
            return Some(access.slot);
        }
        self.load_storage_value(ty, access, expr.span)
    }

    pub(super) fn store_storage_access(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
        value: ValueId,
    ) -> Option<()> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        self.store_storage_value(ty, access, value, expr.span)
    }

    pub(super) fn store_storage_access_with_source(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
        value: ValueId,
        source_ty: Ty<'gcx>,
    ) -> Option<()> {
        let ty = self.gcx.type_of_expr(expr.id)?;
        self.store_storage_value_with_source(ty, source_ty, access, value, expr.span)
    }

    pub(super) fn load_storage_value(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        access: StorageAccess,
        span: Span,
    ) -> Option<ValueId> {
        if self.types.memory_layout(ty).is_some() {
            return self.load_storage_object(ty, access.slot, span);
        }
        let value = if let Some(offset) = access.offset {
            self.storage.load_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
            )
        } else {
            self.storage.load_at_slot(&mut self.builder, access.location, access.slot)
        };
        self.validate_enum_value(ty, value);
        Some(value)
    }

    pub(super) fn store_storage_value(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        access: StorageAccess,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        self.store_storage_value_with_source(ty, ty, access, value, span)
    }

    fn store_storage_value_with_source(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        source_ty: solar_sema::ty::Ty<'gcx>,
        access: StorageAccess,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        if self.types.memory_layout(ty).is_some() {
            return self.store_storage_object_with_source(ty, source_ty, access.slot, value, span);
        }
        self.validate_enum_value(ty, value);
        if let Some(offset) = access.offset {
            self.storage.store_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.storage.store_at_slot(&mut self.builder, access.location, access.slot, value);
        }
        Some(())
    }

    pub(super) fn load_storage_object(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        match ty.peel_refs().kind {
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.load_storage_bytes(slot),
            solar_sema::ty::TyKind::Struct(struct_id) => {
                let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                let layout = MemoryObjectLayout::Struct { fields };
                let size = self.builder.imm_u64(fields.saturating_mul(32));
                let object =
                    self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = self.load_storage_value(
                        field_ty,
                        StorageAccess { slot: field_slot, location, offset: None },
                        span,
                    )?;
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
                Some(object)
            }
            solar_sema::ty::TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                let element_words = self.types.element_words(element);
                let layout = MemoryObjectLayout::FixedArray { len, element_words };
                let size = self
                    .builder
                    .imm_u64(len.checked_mul(u64::from(element_words))?.saturating_mul(32));
                let object =
                    self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let access =
                        self.storage_array_element_access(slot, index_value, element, false)?;
                    let value = self.load_storage_value(element, access, span)?;
                    self.builder.memory_object_store_element(object, layout, index_value, value);
                }
                Some(object)
            }
            solar_sema::ty::TyKind::DynArray(element) => {
                self.load_dynamic_storage_object(element, slot, span)
            }
            _ => report_unsupported(self.gcx, span, "storage object copy"),
        }
    }

    fn load_dynamic_storage_object(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let length = self.builder.sload(slot);
        let element_words = self.types.element_words(element);
        let stride = self.builder.imm_u64(u64::from(element_words));
        let words = self.checked_mul(length, stride);
        let one = self.builder.imm_u64(1);
        let words = self.checked_add(words, one);
        let word_size = self.builder.imm_u64(32);
        let size = self.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
        self.builder.set_memory_object_len(object, length, layout.kind());

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let access = self.storage_array_element_access(slot, index, element, true)?;
        let value = self.load_storage_value(element, access, span)?;
        self.builder.memory_object_store_element(object, layout, index, value);
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        Some(object)
    }

    pub(super) fn store_storage_object(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        self.store_storage_object_with_source(ty, ty, slot, object, span)
    }

    pub(super) fn store_storage_object_with_source(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        source_ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        // MIR object values retain only their coarse kind; HIR types preserve
        // the nested shape needed when fixed arrays convert to storage arrays.
        match ty.peel_refs().kind {
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.store_storage_bytes(slot, object),
            solar_sema::ty::TyKind::Struct(struct_id) => {
                let solar_sema::ty::TyKind::Struct(source_struct_id) = source_ty.peel_refs().kind
                else {
                    return report_unsupported(self.gcx, span, "storage struct conversion");
                };
                let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                let source_fields = self.gcx.hir.strukt(source_struct_id).fields.len() as u64;
                if fields != source_fields {
                    return report_unsupported(self.gcx, span, "storage struct conversion");
                }
                let layout = self.types.memory_layout(source_ty)?;
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = self.builder.memory_object_load_field(object, layout, index as u64);
                    let source_field = self.gcx.hir.strukt(source_struct_id).fields[index];
                    let source_field_ty = self.gcx.type_of_item(source_field.into());
                    let access = StorageAccess { slot: field_slot, location, offset: None };
                    if self.types.memory_layout(field_ty).is_some() {
                        self.store_storage_object_with_source(
                            field_ty,
                            source_field_ty,
                            field_slot,
                            value,
                            span,
                        )?;
                    } else {
                        self.store_storage_value(field_ty, access, value, span)?;
                    }
                }
                Some(())
            }
            solar_sema::ty::TyKind::Array(element, len) => {
                let solar_sema::ty::TyKind::Array(source_element, source_len) =
                    source_ty.peel_refs().kind
                else {
                    return report_unsupported(self.gcx, span, "storage array conversion");
                };
                let len = u64::try_from(len).ok()?;
                let source_len = u64::try_from(source_len).ok()?;
                let layout = self.types.memory_layout(source_ty)?;
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let access =
                        self.storage_array_element_access(slot, index_value, element, false)?;
                    if index < source_len {
                        let value =
                            self.builder.memory_object_load_element(object, layout, index_value);
                        if self.types.memory_layout(element).is_some() {
                            self.store_storage_object_with_source(
                                element,
                                source_element,
                                access.slot,
                                value,
                                span,
                            )?;
                        } else {
                            self.store_storage_value(element, access, value, span)?;
                        }
                    } else {
                        let value = self.default_value(element);
                        if self.types.memory_layout(element).is_some() {
                            self.store_storage_object(element, access.slot, value, span)?;
                        } else {
                            self.store_storage_value(element, access, value, span)?;
                        }
                    }
                }
                Some(())
            }
            solar_sema::ty::TyKind::DynArray(element) => {
                self.store_dynamic_storage_object(element, source_ty, slot, object, span)
            }
            _ => report_unsupported(self.gcx, span, "storage object copy"),
        }
    }

    pub(super) fn load_storage_bytes(&mut self, slot: ValueId) -> Option<ValueId> {
        if !self.share_storage_bytes {
            return Some(lower_storage_bytes_inline(&mut self.builder, slot));
        }
        let helper = match *self.storage_bytes_helper {
            Some(helper) => helper,
            None => {
                let helper = synthesize_storage_bytes_helper(self.module);
                *self.storage_bytes_helper = Some(helper);
                helper
            }
        };
        Some(self.builder.internal_call(
            helper,
            vec![slot],
            MirType::MemoryObject(MemoryObjectKind::Bytes),
            1,
        ))
    }

    fn load_storage_bytes_header(&mut self, slot: ValueId) -> (ValueId, ValueId, ValueId) {
        let header = self.builder.sload(slot);
        let one = self.builder.imm_u64(1);
        let flag = self.builder.and(header, one);
        let is_long = self.builder.eq(flag, one);
        let short_tag = self.builder.imm_u64(0xfe);
        let short_len_tag = self.builder.and(header, short_tag);
        let shift = self.builder.imm_u64(1);
        let short_len = self.builder.shr(shift, short_len_tag);
        let long_len = self.builder.shr(shift, header);
        let length = self.builder.select(is_long, long_len, short_len);
        let thirty_two = self.builder.imm_u64(32);
        let short_length = self.builder.lt(length, thirty_two);
        let invalid_encoding = self.builder.eq(is_long, short_length);
        self.panic_if(invalid_encoding, PanicCode::StorageEncoding);
        (header, is_long, length)
    }

    pub(super) fn store_storage_bytes(&mut self, slot: ValueId, object: ValueId) -> Option<()> {
        let (_, old_is_long, old_length) = self.load_storage_bytes_header(slot);
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        let data_ptr = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let data = self.builder.make_slice(data_ptr, length, SliceLocation::Memory);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let old_rounded = self.checked_add(old_length, thirty_one);
        let old_words = self.builder.div(old_rounded, word_size);
        let rounded = self.checked_add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let zero = self.builder.imm_u64(0);
        let short = self.builder.lt(length, word_size);
        let new_words = self.builder.select(short, zero, words);
        let shrunk = self.builder.gt(old_length, length);
        let needs_cleanup = self.builder.and(old_is_long, shrunk);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(needs_cleanup, cleanup_block, write_block);

        self.builder.switch_to_block(cleanup_block);
        self.clear_storage_words(slot, new_words, old_words, zero);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        let short_block = self.builder.create_block();
        let long_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.branch(short, short_block, long_block);

        self.builder.switch_to_block(short_block);
        let data_word = self.builder.memory_slice_load_word(data, zero);
        let unused_bytes = self.builder.sub(word_size, length);
        let bits = self.builder.imm_u64(8);
        let shift = self.builder.mul(unused_bytes, bits);
        let one = self.builder.imm_u64(1);
        let high_bit = self.builder.shl(shift, one);
        let low_mask = self.builder.sub(high_bit, one);
        let data_mask = self.builder.not(low_mask);
        let data_word = self.builder.and(data_word, data_mask);
        let two = self.builder.imm_u64(2);
        let tag = self.builder.mul(length, two);
        let header = self.builder.or(data_word, tag);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(long_block);
        let one = self.builder.imm_u64(1);
        let shifted = self.builder.shl(one, length);
        let tag = self.builder.or(shifted, one);
        self.builder.sstore(slot, tag);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let preheader = self.builder.current_block();
        let header_block = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header_block);
        self.builder.switch_to_block(header_block);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, words);
        self.builder.branch(condition, body, exit);
        self.builder.switch_to_block(body);
        let byte_offset = self.builder.mul(index, word_size);
        let value = self.builder.memory_slice_load_word(data, byte_offset);
        let element_slot = self.builder.add(data_slot, index);
        self.builder.sstore(element_slot, value);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header_block);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        Some(())
    }

    fn clear_storage_words(
        &mut self,
        slot: ValueId,
        first_word: ValueId,
        words: ValueId,
        zero: ValueId,
    ) {
        let data_slot = self.builder.storage_array_data_slot(slot);
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let index = self.builder.phi(vec![(preheader, first_word)]);
        let condition = self.builder.lt(index, words);
        self.builder.branch(condition, body, exit);
        self.builder.switch_to_block(body);
        let element_slot = self.builder.add(data_slot, index);
        self.builder.sstore(element_slot, zero);
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
    }

    fn store_constant_storage_bytes(&mut self, slot: ValueId, bytes: &[u8]) {
        let (_, old_is_long, old_length) = self.load_storage_bytes_header(slot);
        let length = self.builder.imm_u64(bytes.len() as u64);
        let shrunk = self.builder.gt(old_length, length);
        let needs_cleanup = self.builder.and(old_is_long, shrunk);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(needs_cleanup, cleanup_block, write_block);

        self.builder.switch_to_block(cleanup_block);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let old_rounded = self.checked_add(old_length, thirty_one);
        let old_words = self.builder.div(old_rounded, word_size);
        let new_words = if bytes.len() < 32 {
            self.builder.imm_u64(0)
        } else {
            self.builder.imm_u64(bytes.len().div_ceil(32) as u64)
        };
        let zero = self.builder.imm_u64(0);
        self.clear_storage_words(slot, new_words, old_words, zero);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        if bytes.len() < 32 {
            let word = if bytes.is_empty() {
                U256::ZERO
            } else {
                U256::from_be_slice(bytes) << ((32 - bytes.len()) * 8)
            };
            let tag = U256::from((bytes.len() as u64) * 2);
            let value = self.builder.imm_u256(word | tag);
            self.builder.sstore(slot, value);
        } else {
            let tag = U256::from((bytes.len() as u64) * 2 + 1);
            let value = self.builder.imm_u256(tag);
            self.builder.sstore(slot, value);
            let data_slot = self.builder.storage_array_data_slot(slot);
            for (index, chunk) in bytes.chunks(32).enumerate() {
                let word = U256::from_be_slice(chunk) << ((32 - chunk.len()) * 8);
                let index = self.builder.imm_u64(index as u64);
                let element_slot = self.builder.add(data_slot, index);
                let value = self.builder.imm_u256(word);
                self.builder.sstore(element_slot, value);
            }
        }
    }

    fn store_dynamic_storage_object(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        source_ty: solar_sema::ty::Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        let source_ty = source_ty.peel_refs();
        let source_layout = self.types.memory_layout(source_ty)?;
        let (source_element, length) = match source_ty.kind {
            solar_sema::ty::TyKind::DynArray(source_element)
            | solar_sema::ty::TyKind::Slice(source_element) => {
                (source_element, self.builder.memory_object_len(object, source_layout.kind()))
            }
            solar_sema::ty::TyKind::Array(source_element, source_len) => {
                let source_len = self.builder.imm_u64(u64::try_from(source_len).ok()?);
                (source_element, source_len)
            }
            _ => return report_unsupported(self.gcx, span, "storage array conversion"),
        };

        let old_length = self.builder.sload(slot);
        let needs_cleanup = self.builder.gt(old_length, length);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(needs_cleanup, cleanup_block, write_block);

        self.builder.switch_to_block(cleanup_block);
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let index = self.builder.phi(vec![(preheader, length)]);
        let condition = self.builder.lt(index, old_length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let access = self.storage_array_element_access(slot, index, element, true)?;
        self.clear_storage_access(element, access)?;
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        self.builder.sstore(slot, length);

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let condition = self.builder.lt(index, length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let value = self.builder.memory_object_load_element(object, source_layout, index);
        let value = if self.types.memory_layout(source_element).is_some() {
            self.materialize_array_element(object, source_layout, index, source_element, value)?
        } else {
            value
        };
        let access = self.storage_array_element_access(slot, index, element, true)?;
        if self.types.memory_layout(element).is_some() {
            self.store_storage_object_with_source(
                element,
                source_element,
                access.slot,
                value,
                span,
            )?;
        } else {
            self.store_storage_value(element, access, value, span)?;
        }
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        Some(())
    }

    pub(super) fn clear_storage_access(
        &mut self,
        ty: solar_sema::ty::Ty<'gcx>,
        access: StorageAccess,
    ) -> Option<()> {
        let zero = self.builder.imm_u256(U256::ZERO);
        match ty.peel_refs().kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => self.clear_storage_bytes(access.slot),
            TyKind::DynArray(element) => {
                let length = self.builder.sload(access.slot);
                self.builder.sstore(access.slot, zero);

                let preheader = self.builder.current_block();
                let header = self.builder.create_block();
                let body = self.builder.create_block();
                let exit = self.builder.create_block();
                self.builder.jump(header);
                self.builder.switch_to_block(header);
                let index = self.builder.phi(vec![(preheader, zero)]);
                let condition = self.builder.lt(index, length);
                self.builder.branch(condition, body, exit);

                self.builder.switch_to_block(body);
                let element_access =
                    self.storage_array_element_access(access.slot, index, element, true)?;
                self.clear_storage_access(element, element_access)?;
                let one = self.builder.imm_u64(1);
                let next = self.builder.add(index, one);
                let backedge = self.builder.current_block();
                self.builder.jump(header);
                self.builder.add_phi_incoming(index, backedge, next);
                self.builder.switch_to_block(exit);
            }
            TyKind::Struct(struct_id) => {
                for (index, &field) in self.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let location = self.storage.field_location(struct_id, index)?;
                    let field_slot = self.add_storage_offset(access.slot, location.slot);
                    self.clear_storage_access(
                        field_ty,
                        StorageAccess { slot: field_slot, location, offset: None },
                    )?;
                }
            }
            TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                for index in 0..len {
                    let index = self.builder.imm_u64(index);
                    let element_access =
                        self.storage_array_element_access(access.slot, index, element, false)?;
                    self.clear_storage_access(element, element_access)?;
                }
            }
            _ => {
                if let Some(offset) = access.offset {
                    self.storage.store_packed_at_slot(
                        &mut self.builder,
                        access.location,
                        access.slot,
                        offset,
                        zero,
                    );
                } else {
                    self.storage.store_at_slot(
                        &mut self.builder,
                        access.location,
                        access.slot,
                        zero,
                    );
                }
            }
        }
        Some(())
    }

    fn clear_storage_bytes(&mut self, slot: ValueId) {
        let (_, is_long, length) = self.load_storage_bytes_header(slot);
        let zero = self.builder.imm_u64(0);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(is_long, cleanup_block, write_block);

        self.builder.switch_to_block(cleanup_block);
        self.clear_storage_words(slot, zero, words, zero);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        self.builder.sstore(slot, zero);
    }
}

fn lower_storage_bytes_inline(builder: &mut FunctionBuilder<'_>, slot: ValueId) -> ValueId {
    let header = builder.sload(slot);
    let one = builder.imm_u64(1);
    let flag = builder.and(header, one);
    let is_long = builder.eq(flag, one);
    let short_tag = builder.imm_u64(0xfe);
    let short_len_tag = builder.and(header, short_tag);
    let shift = builder.imm_u64(1);
    let short_len = builder.shr(shift, short_len_tag);
    let long_len = builder.shr(shift, header);
    let length = builder.select(is_long, long_len, short_len);
    let thirty_two = builder.imm_u64(32);
    let short_length = builder.lt(length, thirty_two);
    let invalid_encoding = builder.eq(is_long, short_length);
    builder.panic_if(invalid_encoding, PanicCode::StorageEncoding);

    let rounded = {
        let thirty_one = builder.imm_u64(31);
        builder.checked_add(length, thirty_one)
    };
    let words = builder.div(rounded, thirty_two);
    let total_words = builder.checked_add(words, one);
    let size = builder.checked_mul(total_words, thirty_two);
    let object =
        builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::SOLIDITY_ZEROED);
    builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);

    let short_block = builder.create_block();
    let long_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.branch(is_long, long_block, short_block);

    builder.switch_to_block(short_block);
    let zero = builder.imm_u64(0);
    let short_mask = builder.imm_u256(U256::MAX << 8);
    let short_data = builder.and(header, short_mask);
    builder.memory_object_store_word(object, zero, short_data);
    builder.jump(merge_block);

    builder.switch_to_block(long_block);
    let data_slot = builder.storage_array_data_slot(slot);
    let preheader = builder.current_block();
    let header_block = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();
    builder.jump(header_block);
    builder.switch_to_block(header_block);
    let index = builder.phi(vec![(preheader, zero)]);
    let condition = builder.lt(index, words);
    builder.branch(condition, body, exit);
    builder.switch_to_block(body);
    let element_slot = builder.add(data_slot, index);
    let value = builder.sload(element_slot);
    let byte_offset = builder.mul(index, thirty_two);
    builder.memory_object_store_word(object, byte_offset, value);
    let next = builder.add(index, one);
    let backedge = builder.current_block();
    builder.jump(header_block);
    builder.add_phi_incoming(index, backedge, next);
    builder.switch_to_block(exit);
    builder.jump(merge_block);

    builder.switch_to_block(merge_block);
    object
}
