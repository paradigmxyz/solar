//! Storage-reference access and aggregate materialization for one lowered function.

use super::*;

#[derive(Clone, Copy)]
enum StorageStructField {
    Scalar { location: StorageLocation },
    Enum { location: StorageLocation, variants: u64 },
    Bytes { location: StorageLocation, helper: FunctionId },
    Array { location: StorageLocation, helper: FunctionId },
}

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

/// Adds a helper for copying a dynamic storage array of `bytes` or `string`.
fn synthesize_storage_bytes_array_helper(
    module: &mut Module,
    bytes_helper: FunctionId,
) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::__load_storage_bytes_array));
    function.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let slot = builder.add_param(MirType::uint256());
        builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

        let length = builder.sload(slot);
        let one = builder.imm_u64(1);
        let words = builder.checked_add(length, one);
        let word_size = builder.imm_u64(32);
        let size = builder.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words: 1 };
        let object =
            builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        builder.set_memory_object_len(object, length, layout.kind());

        let data_slot = builder.storage_array_data_slot(slot);
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);
        builder.switch_to_block(header);
        let zero = builder.imm_u64(0);
        let index = builder.phi(vec![(preheader, zero)]);
        let condition = builder.lt(index, length);
        builder.branch(condition, body, exit);

        builder.switch_to_block(body);
        let element_slot = builder.add(data_slot, index);
        let value = builder.internal_call(
            bytes_helper,
            vec![element_slot],
            MirType::MemoryObject(MemoryObjectKind::Bytes),
            1,
        );
        builder.memory_object_store_element(object, layout, index, value);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let backedge = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index, backedge, next);
        builder.switch_to_block(exit);
        builder.ret([object]);
    }
    module.add_function(function)
}

/// Adds the module-wide helper for copying a full-word dynamic storage array.
pub(super) fn synthesize_storage_word_array_helper(module: &mut Module) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::__load_storage_word_array));
    function.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let slot = builder.add_param(MirType::uint256());
        builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

        let length = builder.sload(slot);
        let one = builder.imm_u64(1);
        let words = builder.checked_add(length, one);
        let word_size = builder.imm_u64(32);
        let size = builder.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words: 1 };
        let object =
            builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        builder.set_memory_object_len(object, length, layout.kind());

        let data_slot = builder.storage_array_data_slot(slot);
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);
        builder.switch_to_block(header);
        let zero = builder.imm_u64(0);
        let index = builder.phi(vec![(preheader, zero)]);
        let condition = builder.lt(index, length);
        builder.branch(condition, body, exit);
        builder.switch_to_block(body);
        let element_slot = builder.add(data_slot, index);
        let value = builder.sload(element_slot);
        builder.memory_object_store_element(object, layout, index, value);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let backedge = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index, backedge, next);
        builder.switch_to_block(exit);
        builder.ret([object]);
    }
    module.add_function(function)
}

/// Adds a helper for copying one packed dynamic storage array shape.
fn synthesize_storage_packed_array_helper(
    module: &mut Module,
    bytes: u8,
    encoding: u8,
) -> FunctionId {
    let name = format!("__load_storage_packed_array_{bytes}_{encoding}");
    let mut function = Function::new(Ident::from_str(&name));
    function.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let slot = builder.add_param(MirType::uint256());
        builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

        let length = builder.sload(slot);
        let one = builder.imm_u64(1);
        let words = builder.checked_add(length, one);
        let word_size = builder.imm_u64(32);
        let size = builder.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words: 1 };
        let object =
            builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        builder.set_memory_object_len(object, length, layout.kind());

        let data_slot = builder.storage_array_data_slot(slot);
        let preheader = builder.current_block();
        let loop_header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(loop_header);
        builder.switch_to_block(loop_header);
        let zero = builder.imm_u64(0);
        let index = builder.phi(vec![(preheader, zero)]);
        let condition = builder.lt(index, length);
        builder.branch(condition, body, exit);
        builder.switch_to_block(body);

        let per_slot = u64::from(32 / bytes);
        let per_slot_value = builder.imm_u64(per_slot);
        let (slot_index, index_in_slot) = if per_slot.is_power_of_two() {
            let slot_shift = builder.imm_u64(u64::from(per_slot.trailing_zeros()));
            let slot_index = builder.shr(slot_shift, index);
            let slot_mask = builder.imm_u64(per_slot - 1);
            let index_in_slot = builder.and(index, slot_mask);
            (slot_index, index_in_slot)
        } else {
            let slot_index = builder.div(index, per_slot_value);
            let index_in_slot = builder.mod_(index, per_slot_value);
            (slot_index, index_in_slot)
        };
        let storage_slot = builder.add(data_slot, slot_index);
        let word = builder.sload(storage_slot);
        let byte_shift = u64::from(bytes) * 8;
        let shift = if byte_shift.is_power_of_two() {
            let shift = builder.imm_u64(u64::from(byte_shift.trailing_zeros()));
            builder.shl(shift, index_in_slot)
        } else {
            let byte_shift = builder.imm_u64(byte_shift);
            builder.mul(index_in_slot, byte_shift)
        };
        let shifted = builder.shr(shift, word);
        let mask = builder.imm_u256((U256::from(1) << (u32::from(bytes) * 8)) - U256::from(1));
        let value = builder.and(shifted, mask);
        let value = match encoding {
            0 => value,
            1 => {
                let sign_index = builder.imm_u64(u64::from(bytes - 1));
                builder.signextend(sign_index, value)
            }
            2 => {
                let align_shift = builder.imm_u64(u64::from(32 - bytes) * 8);
                builder.shl(align_shift, value)
            }
            _ => unreachable!("unknown storage encoding"),
        };
        let address = builder.memory_object_element_addr(object, layout, index);
        builder.mstore(address, value);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let backedge = builder.current_block();
        builder.jump(loop_header);
        builder.add_phi_incoming(index, backedge, next);
        builder.switch_to_block(exit);
        builder.ret([object]);
    }
    module.add_function(function)
}

fn synthesize_storage_struct_array_helper(
    module: &mut Module,
    function_id: FunctionId,
    storage: &StorageLayout<'_>,
    fields: &[StorageStructField],
    element_slots: u64,
    field_count: u64,
) {
    let function = module.function_mut(function_id);
    function.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(function);
        let slot = builder.add_param(MirType::uint256());
        builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

        let length = builder.sload(slot);
        let one = builder.imm_u64(1);
        let words = builder.checked_add(length, one);
        let word_size = builder.imm_u64(32);
        let size = builder.checked_mul(words, word_size);
        let array_layout = MemoryObjectLayout::WORD_ARRAY;
        let object =
            builder.alloc_object(size, array_layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        builder.set_memory_object_len(object, length, MemoryObjectKind::DynamicArray);

        let data_slot = builder.storage_array_data_slot(slot);
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);
        builder.switch_to_block(header);
        let zero = builder.imm_u64(0);
        let index = builder.phi(vec![(preheader, zero)]);
        let element_slot = builder.phi(vec![(preheader, data_slot)]);
        let condition = builder.lt(index, length);
        builder.branch(condition, body, exit);

        builder.switch_to_block(body);
        let field_size = builder.imm_u64(field_count.saturating_mul(32));
        let field_layout = MemoryObjectLayout::Struct { fields: field_count };
        let value = builder.alloc_object(
            field_size,
            field_layout,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        for (field_index, field) in fields.iter().enumerate() {
            let location = match field {
                StorageStructField::Scalar { location }
                | StorageStructField::Enum { location, .. }
                | StorageStructField::Bytes { location, .. }
                | StorageStructField::Array { location, .. } => *location,
            };
            let field_slot = if location.slot.is_zero() {
                element_slot
            } else {
                let offset = builder.imm_u256(location.slot);
                builder.add(element_slot, offset)
            };
            let field_value = match *field {
                StorageStructField::Scalar { location } => {
                    storage.load_at_slot(&mut builder, location, field_slot)
                }
                StorageStructField::Enum { location, variants } => {
                    let value = storage.load_at_slot(&mut builder, location, field_slot);
                    let limit = builder.imm_u64(variants);
                    let valid = builder.lt(value, limit);
                    let invalid = builder.iszero(valid);
                    builder.panic_if(invalid, PanicCode::EnumConversion);
                    value
                }
                StorageStructField::Bytes { helper, .. } => builder.internal_call(
                    helper,
                    vec![field_slot],
                    MirType::MemoryObject(MemoryObjectKind::Bytes),
                    1,
                ),
                StorageStructField::Array { helper, .. } => builder.internal_call(
                    helper,
                    vec![field_slot],
                    MirType::MemoryObject(MemoryObjectKind::DynamicArray),
                    1,
                ),
            };
            builder.memory_object_store_field(value, field_layout, field_index as u64, field_value);
        }
        builder.memory_object_store_element(object, array_layout, index, value);
        let next_index = builder.add(index, one);
        let stride = builder.imm_u64(element_slots);
        let next_slot = builder.add(element_slot, stride);
        let backedge = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index, backedge, next_index);
        builder.add_phi_incoming(element_slot, backedge, next_slot);

        builder.switch_to_block(exit);
        builder.ret([object]);
    }
}

/// Adds the module-wide helper for clearing the data words of a storage bytes value.
pub(super) fn synthesize_storage_clear_helper(module: &mut Module) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::__clear_storage_words));
    function.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let slot = builder.add_param(MirType::uint256());
        let first_word = builder.add_param(MirType::uint256());
        let words = builder.add_param(MirType::uint256());
        let zero = builder.imm_u64(0);
        let data_slot = builder.storage_array_data_slot(slot);
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);
        builder.switch_to_block(header);
        let index = builder.phi(vec![(preheader, first_word)]);
        let condition = builder.lt(index, words);
        builder.branch(condition, body, exit);
        builder.switch_to_block(body);
        let element_slot = builder.add(data_slot, index);
        builder.sstore(element_slot, zero);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let backedge = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index, backedge, next);
        builder.switch_to_block(exit);
        builder.stop();
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
        let Some(id) = self.context.gcx.resolved_variable(lhs) else { return false };
        if !self.context.gcx.hir.variable(id).is_state_variable() {
            return false;
        }
        let Some(lhs_ty) = self.context.gcx.type_of_expr(lhs.id) else { return false };
        let Some(rhs_ty) = self.context.gcx.type_of_expr(rhs.id) else { return false };
        let target_ty = lhs_ty.peel_refs();
        if self.types.memory_layout(target_ty).is_none() || target_ty != rhs_ty.peel_refs() {
            return false;
        }
        match target_ty.kind {
            TyKind::Struct(_) => {
                matches!(rhs.peel_parens().kind, ExprKind::Call(..))
                    && self.is_constant_storage_value(rhs, target_ty)
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                matches!(rhs.peel_parens().kind, ExprKind::Lit(..))
                    && self.is_constant_storage_value(rhs, target_ty)
            }
            _ => false,
        }
    }

    fn is_constant_storage_value(&self, expr: &hir::Expr<'_>, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                matches!(self.context.gcx.try_eval_const_value(expr), Ok(ConstValue::String(_)))
            }
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return false;
                };
                let Some(hir::Res::Item(hir::ItemId::Struct(id))) =
                    self.context.gcx.resolved_expr(callee)
                else {
                    return false;
                };
                if id != struct_id {
                    return false;
                }
                let fields = self.context.gcx.hir.strukt(struct_id).fields;
                if args.len() != fields.len() {
                    return false;
                }
                let names =
                    self.context.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                fields.iter().enumerate().all(|(index, &field)| {
                    args.argument_for_parameter(index, Some(names.as_slice())).is_some_and(
                        |argument| {
                            self.is_constant_storage_value(
                                argument,
                                self.context.gcx.type_of_item(field.into()),
                            )
                        },
                    )
                })
            }
            TyKind::Array(..) | TyKind::DynArray(..) => false,
            TyKind::Elementary(_) => self.context.gcx.try_eval_const_value(expr).is_ok(),
            _ => false,
        }
    }

    pub(super) fn lower_constant_storage_assignment(
        &mut self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let ty = self.context.gcx.type_of_expr(lhs.id)?.peel_refs();
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
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let Ok(ConstValue::String(value)) = self.context.gcx.try_eval_const_value(expr)
                else {
                    return None;
                };
                self.store_constant_storage_bytes(
                    access.slot,
                    value.as_byte_str_in(self.context.gcx.sess),
                );
                Some(())
            }
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return None;
                };
                let hir::Res::Item(hir::ItemId::Struct(id)) =
                    self.context.gcx.resolved_expr(callee)?
                else {
                    return None;
                };
                if id != struct_id {
                    return None;
                }
                let fields = self.context.gcx.hir.strukt(struct_id).fields;
                let names =
                    self.context.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                for (index, &field) in fields.iter().enumerate() {
                    let argument = args.argument_for_parameter(index, Some(names.as_slice()))?;
                    let field_ty = self.context.gcx.type_of_item(field.into());
                    let location = self.context.storage.field_location(struct_id, index)?;
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
                let source_ty = self.context.gcx.type_of_expr(expr.id)?;
                let value = match self.context.gcx.try_eval_const_value(expr) {
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
                let id = self.context.gcx.resolved_variable(expr)?;
                if let Some(access) = self.storage_refs.get(&id).copied() {
                    return Some(access);
                }
                let var = self.context.gcx.hir.variable(id);
                if !var.is_state_variable() {
                    return None;
                }
                let location = self.context.storage.get(id)?;
                let slot = self.builder.imm_u256(location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Member(receiver, _) => {
                let id = self.context.gcx.resolved_variable(expr)?;
                let variable = self.context.gcx.hir.variable(id);
                let hir::ItemId::Struct(struct_id) = variable.parent? else { return None };
                let field = self
                    .context
                    .gcx
                    .hir
                    .strukt(struct_id)
                    .fields
                    .iter()
                    .position(|&field| field == id)?;
                let base = self.storage_access(receiver)?;
                let location = self.context.storage.field_location(struct_id, field)?;
                let slot = self.add_storage_offset(base.slot, location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Index(receiver, Some(index)) => {
                let base = self.storage_access(receiver)?;
                let ty = self.context.gcx.type_of_expr(receiver.id)?.peel_refs();
                let index_ty = self.context.gcx.type_of_expr(index.id)?;
                let index = self.lower_expr(index)?;
                if let TyKind::Mapping(_, value) = ty.kind {
                    let slot = self.mapping_slot(index, index_ty, base.slot);
                    if let Some((size, encoding)) = self.context.storage.packed_encoding(value) {
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
                self.builder.bounds_check(index, length);
                self.storage_array_element_access(base.slot, index, element, dynamic)
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                self.storage_access_ternary(condition, then_expr, else_expr)
            }
            ExprKind::Call(callee, arguments, _)
                if arguments.is_empty()
                    && self.context.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0) =>
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
        self.context.gcx.resolved_function(callee).is_some_and(|function_id| {
            self.context.gcx.hir.function(function_id).returns.first().is_some_and(|&ret| {
                self.context.gcx.type_of_item(ret.into()).is_ref_at(DataLocation::Storage)
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
        self.context
            .gcx
            .resolved_variable(expr)
            .is_some_and(|id| !self.context.gcx.hir.variable(id).is_state_variable())
            && self
                .context
                .gcx
                .type_of_expr(expr.id)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
    }

    pub(super) fn storage_array_push_access(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>, ValueId, ValueId)> {
        let base = self.storage_access(receiver)?;
        let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = receiver_ty.kind else { return None };
        let length = self.builder.sload(base.slot);
        let one = self.builder.imm_u64(1);
        let new_length = self.builder.checked_add(length, one);
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
            return report_unsupported(self.context.gcx, expr.span, "storage array push target");
        };
        let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            return self.lower_storage_bytes_push(expr, receiver, builtin, arguments);
        }
        let Some((base, element)) = self.storage_array_base(receiver) else {
            return report_unsupported(self.context.gcx, expr.span, "storage array push target");
        };
        let value = if builtin == Builtin::ArrayPush {
            let [argument] = arguments else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "storage array push arguments",
                );
            };
            let value = self.lower_typed_expr(argument, element)?;
            let value =
                self.coerce_value(value, self.context.gcx.type_of_expr(argument.id)?, element);
            if self.types.memory_layout(element).is_some() {
                self.materialize_memory_argument(element, value, argument.span)?
            } else {
                value
            }
        } else {
            if !arguments.is_empty() {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "storage array push arguments",
                );
            }
            self.default_value(element)
        };
        let length = self.builder.sload(base.slot);
        let one = self.builder.imm_u64(1);
        let new_length = self.builder.checked_add(length, one);
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
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "storage bytes push arguments",
                );
            };
            let source_ty = self.context.gcx.type_of_expr(argument.id)?;
            let value = self.lower_typed_expr(argument, self.gcx.types.fixed_bytes(1))?;
            let value = self.coerce_value(value, source_ty, self.context.gcx.types.fixed_bytes(1));
            let shift = self.builder.imm_u64(248);
            self.builder.shr(shift, value)
        } else {
            if !arguments.is_empty() {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "storage bytes push arguments",
                );
            }
            self.builder.imm_u64(0)
        };
        let old = self.load_storage_bytes(access.slot)?;
        let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
        let one = self.builder.imm_u64(1);
        let length = self.builder.checked_add(old_length, one);
        let size = self.builder.checked_padded_size(length);
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
            return report_unsupported(self.context.gcx, expr.span, "storage array pop target");
        };
        let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            let access = self.storage_access(receiver)?;
            let old = self.load_storage_bytes(access.slot)?;
            let old_length = self.builder.memory_object_len(old, MemoryObjectKind::Bytes);
            let zero = self.builder.imm_u64(0);
            let empty = self.builder.eq(old_length, zero);
            self.builder.panic_if(empty, PanicCode::EmptyArrayPop);
            let one = self.builder.imm_u64(1);
            let length = self.builder.sub(old_length, one);
            let size = self.builder.checked_padded_size(length);
            let layout = MemoryObjectLayout::Bytes;
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, length, layout.kind());
            self.builder.memory_object_copy(object, layout.kind(), old, layout.kind(), length);
            self.store_storage_bytes(access.slot, object)?;
            return Some(zero);
        }

        let Some((base, element)) = self.storage_array_base(receiver) else {
            return report_unsupported(self.context.gcx, expr.span, "storage array pop target");
        };
        let length = self.builder.sload(base.slot);
        let zero = self.builder.imm_u64(0);
        let empty = self.builder.eq(length, zero);
        self.builder.panic_if(empty, PanicCode::EmptyArrayPop);
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
        let ty = self.context.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = ty.kind else { return None };
        Some((base, element))
    }

    fn mapping_slot(&mut self, key: ValueId, key_ty: Ty<'gcx>, slot: ValueId) -> ValueId {
        let is_calldata = key_ty.data_stored_in(DataLocation::Calldata)
            || matches!(key_ty.kind, TyKind::Slice(inner) if inner.data_stored_in(DataLocation::Calldata));
        let is_dynamic = matches!(
            key_ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes)
                | TyKind::Slice(_)
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
        element: Ty<'gcx>,
        dynamic: bool,
    ) -> Option<StorageAccess> {
        if let Some((size, encoding)) = self.context.storage.packed_encoding(element)
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
        let element_slots = self.context.storage.element_slots(element);
        let base_slot =
            if dynamic { self.builder.storage_array_data_slot(base_slot) } else { base_slot };
        let slot = self.fixed_array_element_slot(base_slot, index, element_slots);
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
        let ty = self.context.gcx.type_of_expr(expr.id)?;
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
        let ty = self.context.gcx.type_of_expr(expr.id)?;
        self.store_storage_value(ty, access, value, expr.span)
    }

    pub(super) fn store_storage_access_with_source(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
        value: ValueId,
        source_ty: Ty<'gcx>,
    ) -> Option<()> {
        let ty = self.context.gcx.type_of_expr(expr.id)?;
        self.store_storage_value_with_source(ty, source_ty, access, value, expr.span)
    }

    pub(super) fn load_storage_value(
        &mut self,
        ty: Ty<'gcx>,
        access: StorageAccess,
        span: Span,
    ) -> Option<ValueId> {
        if self.types.memory_layout(ty).is_some() {
            return self.load_storage_object(ty, access.slot, span);
        }
        let value = if let Some(offset) = access.offset {
            self.context.storage.load_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
            )
        } else {
            self.context.storage.load_at_slot(&mut self.builder, access.location, access.slot)
        };
        if let TyKind::Enum(id) = ty.peel_refs().kind {
            let variants = self.context.gcx.hir.enumm(id).variants.len() as u64;
            self.builder.validate_enum_value(variants, value);
        }
        Some(value)
    }

    pub(super) fn store_storage_value(
        &mut self,
        ty: Ty<'gcx>,
        access: StorageAccess,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        self.store_storage_value_with_source(ty, ty, access, value, span)
    }

    fn store_storage_value_with_source(
        &mut self,
        ty: Ty<'gcx>,
        source_ty: Ty<'gcx>,
        access: StorageAccess,
        value: ValueId,
        span: Span,
    ) -> Option<()> {
        if self.types.memory_layout(ty).is_some() {
            return self.store_storage_object_with_source(ty, source_ty, access.slot, value, span);
        }
        if let TyKind::Enum(id) = ty.peel_refs().kind {
            let variants = self.context.gcx.hir.enumm(id).variants.len() as u64;
            self.builder.validate_enum_value(variants, value);
        }
        if let Some(offset) = access.offset {
            self.context.storage.store_packed_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.context.storage.store_at_slot(
                &mut self.builder,
                access.location,
                access.slot,
                value,
            );
        }
        Some(())
    }

    pub(super) fn load_storage_object(
        &mut self,
        ty: Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.load_storage_bytes(slot)
            }
            TyKind::Struct(struct_id) => {
                let fields = self.context.gcx.hir.strukt(struct_id).fields.len() as u64;
                let layout = MemoryObjectLayout::Struct { fields };
                let size = self.builder.imm_u64(fields.saturating_mul(32));
                let object = self.builder.alloc_object(
                    size,
                    layout,
                    AllocationSemantics::SOLIDITY_UNINITIALIZED,
                );
                for (index, &field) in
                    self.context.gcx.hir.strukt(struct_id).fields.iter().enumerate()
                {
                    let field_ty = self.context.gcx.type_of_item(field.into());
                    let location = self.context.storage.field_location(struct_id, index)?;
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
            TyKind::Array(element, len) => {
                let len = u64::try_from(len).ok()?;
                let element_words = self.types.element_words(element);
                let layout = MemoryObjectLayout::FixedArray { len, element_words };
                let size = self
                    .builder
                    .imm_u64(len.checked_mul(u64::from(element_words))?.saturating_mul(32));
                let object = self.builder.alloc_object(
                    size,
                    layout,
                    AllocationSemantics::SOLIDITY_UNINITIALIZED,
                );
                for index in 0..len {
                    let index_value = self.builder.imm_u64(index);
                    let access =
                        self.storage_array_element_access(slot, index_value, element, false)?;
                    let value = self.load_storage_value(element, access, span)?;
                    self.builder.memory_object_store_element(object, layout, index_value, value);
                }
                Some(object)
            }
            TyKind::DynArray(element) => self.load_dynamic_storage_object(element, slot, span),
            _ => report_unsupported(self.context.gcx, span, "storage object copy"),
        }
    }

    fn ensure_storage_array_helper(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
    ) -> Option<FunctionId> {
        if matches!(
            element.peel_refs().kind,
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            )
        ) {
            let bytes_helper = match *self.storage_bytes_helper {
                Some(helper) => helper,
                None => {
                    let helper = synthesize_storage_bytes_helper(self.module);
                    *self.storage_bytes_helper = Some(helper);
                    helper
                }
            };
            let helper = match *self.storage_bytes_array_helper {
                Some(helper) => helper,
                None => {
                    let helper = synthesize_storage_bytes_array_helper(self.module, bytes_helper);
                    *self.storage_bytes_array_helper = Some(helper);
                    helper
                }
            };
            return Some(helper);
        }
        if let solar_sema::ty::TyKind::Struct(struct_id) = element.peel_refs().kind {
            return self.ensure_storage_struct_array_helper(element, struct_id);
        }
        if let Some((size, encoding)) = self.storage.packed_encoding(element)
            && size.bits() < 256
            && self.types.memory_layout(element).is_none()
        {
            let encoding = match encoding {
                StorageEncoding::Unsigned => 0,
                StorageEncoding::Signed => 1,
                StorageEncoding::FixedBytes => 2,
            };
            let key = (size.bytes(), encoding);
            let helper = match self.packed_array_helpers.get(&key).copied() {
                Some(helper) => helper,
                None => {
                    let helper = synthesize_storage_packed_array_helper(self.module, key.0, key.1);
                    self.packed_array_helpers.insert(key, helper);
                    helper
                }
            };
            return Some(helper);
        }
        if self.types.element_words(element) == 1
            && self.types.memory_layout(element).is_none()
            && self.storage.packed_encoding(element).is_none_or(|(size, _)| size.bits() == 256)
        {
            let helper = match *self.storage_word_array_helper {
                Some(helper) => helper,
                None => {
                    let helper = synthesize_storage_word_array_helper(self.module);
                    *self.storage_word_array_helper = Some(helper);
                    helper
                }
            };
            return Some(helper);
        }
        None
    }

    fn ensure_storage_struct_array_helper(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        struct_id: solar_sema::hir::StructId,
    ) -> Option<FunctionId> {
        if let Some(&helper) = self.storage_struct_array_helpers.get(&struct_id) {
            return Some(helper);
        }

        let mut visiting = FxHashSet::default();
        if !self.can_lower_storage_struct_array(struct_id, &mut visiting) {
            return None;
        }

        let name = format!("__load_storage_struct_array_{}", self.module.functions.len());
        let mut function = Function::new(Ident::from_str(&name));
        function.attributes.no_inline = true;
        let helper = self.module.add_function(function);
        self.storage_struct_array_helpers.insert(struct_id, helper);

        let field_ids = self.gcx.hir.strukt(struct_id).fields.to_vec();
        let mut fields = Vec::with_capacity(field_ids.len());
        for (index, field_id) in field_ids.into_iter().enumerate() {
            let field_ty = self.gcx.type_of_item(field_id.into());
            let location = self.storage.field_location(struct_id, index)?;
            let field = match field_ty.peel_refs().kind {
                solar_sema::ty::TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                    | solar_sema::hir::ElementaryType::String,
                ) if self.share_storage_bytes => {
                    let helper = match *self.storage_bytes_helper {
                        Some(helper) => helper,
                        None => {
                            let helper = synthesize_storage_bytes_helper(self.module);
                            *self.storage_bytes_helper = Some(helper);
                            helper
                        }
                    };
                    StorageStructField::Bytes { location, helper }
                }
                solar_sema::ty::TyKind::DynArray(element) => {
                    let helper = self.ensure_storage_array_helper(element)?;
                    StorageStructField::Array { location, helper }
                }
                solar_sema::ty::TyKind::Enum(id) => StorageStructField::Enum {
                    location,
                    variants: self.gcx.hir.enumm(id).variants.len() as u64,
                },
                _ if self.storage.packed_encoding(field_ty).is_some() => {
                    StorageStructField::Scalar { location }
                }
                _ => return None,
            };
            fields.push(field);
        }

        synthesize_storage_struct_array_helper(
            self.module,
            helper,
            self.storage,
            &fields,
            self.storage.element_slots(element),
            self.gcx.hir.strukt(struct_id).fields.len() as u64,
        );
        Some(helper)
    }

    fn can_lower_storage_struct_array(
        &self,
        struct_id: solar_sema::hir::StructId,
        visiting: &mut FxHashSet<solar_sema::hir::StructId>,
    ) -> bool {
        if !visiting.insert(struct_id) {
            return true;
        }
        let supported =
            self.gcx.hir.strukt(struct_id).fields.iter().enumerate().all(|(index, &field_id)| {
                if self.storage.field_location(struct_id, index).is_none() {
                    return false;
                }
                let field_ty = self.gcx.type_of_item(field_id.into());
                match field_ty.peel_refs().kind {
                    solar_sema::ty::TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                    ) => self.share_storage_bytes,
                    solar_sema::ty::TyKind::DynArray(element) => {
                        self.can_lower_storage_array_element(element, visiting)
                    }
                    solar_sema::ty::TyKind::Enum(_) => true,
                    _ => self.storage.packed_encoding(field_ty).is_some(),
                }
            });
        visiting.remove(&struct_id);
        supported
    }

    fn can_lower_storage_array_element(
        &self,
        element: solar_sema::ty::Ty<'gcx>,
        visiting: &mut FxHashSet<solar_sema::hir::StructId>,
    ) -> bool {
        if matches!(
            element.peel_refs().kind,
            solar_sema::ty::TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            )
        ) {
            return true;
        }
        if let solar_sema::ty::TyKind::Struct(struct_id) = element.peel_refs().kind {
            return self.can_lower_storage_struct_array(struct_id, visiting);
        }
        if let Some((size, _)) = self.storage.packed_encoding(element)
            && size.bits() < 256
            && self.types.memory_layout(element).is_none()
        {
            return true;
        }
        self.types.element_words(element) == 1
            && self.types.memory_layout(element).is_none()
            && self.storage.packed_encoding(element).is_none_or(|(size, _)| size.bits() == 256)
    }

    fn load_dynamic_storage_object(
        &mut self,
        element: Ty<'gcx>,
        slot: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let element_words = self.types.element_words(element);
        if let Some(helper) = self.ensure_storage_array_helper(element) {
            let layout = MemoryObjectLayout::DynamicArray { element_words };
            return Some(self.builder.internal_call(
                helper,
                vec![slot],
                MirType::MemoryObject(layout.kind()),
                1,
            ));
        }
        let length = self.builder.sload(slot);
        let stride = self.builder.imm_u64(u64::from(element_words));
        let words = self.builder.checked_mul(length, stride);
        let one = self.builder.imm_u64(1);
        let words = self.builder.checked_add(words, one);
        let word_size = self.builder.imm_u64(32);
        let size = self.builder.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words };
        let object =
            self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
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
        ty: Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        self.store_storage_object_with_source(ty, ty, slot, object, span)
    }

    pub(super) fn store_storage_object_with_source(
        &mut self,
        ty: Ty<'gcx>,
        source_ty: Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        // MIR object values retain only their coarse kind; HIR types preserve
        // the nested shape needed when fixed arrays convert to storage arrays.
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.store_storage_bytes(slot, object)
            }
            TyKind::Struct(struct_id) => {
                let TyKind::Struct(source_struct_id) = source_ty.peel_refs().kind else {
                    return report_unsupported(self.context.gcx, span, "storage struct conversion");
                };
                let fields = self.context.gcx.hir.strukt(struct_id).fields.len() as u64;
                let source_fields =
                    self.context.gcx.hir.strukt(source_struct_id).fields.len() as u64;
                if fields != source_fields {
                    return report_unsupported(self.context.gcx, span, "storage struct conversion");
                }
                let layout = self.types.memory_layout(source_ty)?;
                for (index, &field) in
                    self.context.gcx.hir.strukt(struct_id).fields.iter().enumerate()
                {
                    let field_ty = self.context.gcx.type_of_item(field.into());
                    let location = self.context.storage.field_location(struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = self.builder.memory_object_load_field(object, layout, index as u64);
                    let source_field = self.context.gcx.hir.strukt(source_struct_id).fields[index];
                    let source_field_ty = self.context.gcx.type_of_item(source_field.into());
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
            TyKind::Array(element, len) => {
                let TyKind::Array(source_element, source_len) = source_ty.peel_refs().kind else {
                    return report_unsupported(self.context.gcx, span, "storage array conversion");
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
            TyKind::DynArray(element) => {
                self.store_dynamic_storage_object(element, source_ty, slot, object, span)
            }
            _ => report_unsupported(self.context.gcx, span, "storage object copy"),
        }
    }

    pub(super) fn load_storage_bytes(&mut self, slot: ValueId) -> Option<ValueId> {
        if !self.context.share_storage_bytes {
            return Some(lower_storage_bytes_inline(&mut self.builder, slot));
        }
        let helper = match *self.context.storage_bytes_helper {
            Some(helper) => helper,
            None => {
                let helper = synthesize_storage_bytes_helper(self.context.module);
                *self.context.storage_bytes_helper = Some(helper);
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
        self.builder.panic_if(invalid_encoding, PanicCode::StorageEncoding);
        (header, is_long, length)
    }

    pub(super) fn store_storage_bytes(&mut self, slot: ValueId, object: ValueId) -> Option<()> {
        let (_, old_is_long, old_length) = self.load_storage_bytes_header(slot);
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        let data_ptr = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let data = self.builder.make_slice(data_ptr, length, SliceLocation::Memory);
        let word_size = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let old_rounded = self.builder.checked_add(old_length, thirty_one);
        let old_words = self.builder.div(old_rounded, word_size);
        let rounded = self.builder.checked_add(length, thirty_one);
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
        self.clear_storage_words_with_helper(slot, new_words, old_words);
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

    fn clear_storage_words_with_helper(
        &mut self,
        slot: ValueId,
        first_word: ValueId,
        words: ValueId,
    ) {
        let helper = match *self.context.storage_clear_helper {
            Some(helper) => helper,
            None => {
                let helper = synthesize_storage_clear_helper(self.context.module);
                *self.context.storage_clear_helper = Some(helper);
                helper
            }
        };
        self.builder.internal_call_void(helper, vec![slot, first_word, words], 0);
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
        let old_rounded = self.builder.checked_add(old_length, thirty_one);
        let old_words = self.builder.div(old_rounded, word_size);
        let new_words = if bytes.len() < 32 {
            self.builder.imm_u64(0)
        } else {
            self.builder.imm_u64(bytes.len().div_ceil(32) as u64)
        };
        self.clear_storage_words_with_helper(slot, new_words, old_words);
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
        element: Ty<'gcx>,
        source_ty: Ty<'gcx>,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        let source_ty = source_ty.peel_refs();
        let source_layout = self.types.memory_layout(source_ty)?;
        let (source_element, length) = match source_ty.kind {
            TyKind::DynArray(source_element) | TyKind::Slice(source_element) => {
                (source_element, self.builder.memory_object_len(object, source_layout.kind()))
            }
            TyKind::Array(source_element, source_len) => {
                let source_len = self.builder.imm_u64(u64::try_from(source_len).ok()?);
                (source_element, source_len)
            }
            _ => return report_unsupported(self.context.gcx, span, "storage array conversion"),
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
        ty: Ty<'gcx>,
        access: StorageAccess,
    ) -> Option<()> {
        let zero = self.builder.imm_u256(U256::ZERO);
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.clear_storage_bytes(access.slot)
            }
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
                for (index, &field) in
                    self.context.gcx.hir.strukt(struct_id).fields.iter().enumerate()
                {
                    let field_ty = self.context.gcx.type_of_item(field.into());
                    let location = self.context.storage.field_location(struct_id, index)?;
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
                    self.context.storage.store_packed_at_slot(
                        &mut self.builder,
                        access.location,
                        access.slot,
                        offset,
                        zero,
                    );
                } else {
                    self.context.storage.store_at_slot(
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
        let rounded = self.builder.checked_add(length, thirty_one);
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
