//! Storage-reference access and aggregate materialization for one lowered function.

use super::*;

#[derive(Clone, Copy)]
enum StorageArrayElement {
    Bytes(FunctionId),
    Word,
    Packed { bytes: u8, encoding: StorageEncoding, enum_variants: Option<u64> },
}

/// Builds the helper for decoding one storage `bytes`/`string` slot.
fn build_storage_bytes_helper(function: &mut Function) {
    // load_storage_bytes(slot) -> bytes_object
    let mut builder = FunctionBuilder::new(function);
    let slot = builder.add_param(MirType::uint256());
    builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
    let object = lower_storage_bytes_inline(&mut builder, slot);
    builder.ret([object]);
}

fn build_storage_array_helper(function: &mut Function, element: StorageArrayElement) {
    // length = sload(slot)
    // array = alloc_dynamic_array(length); set_length(array, length)
    // data_slot = storage_array_data_slot(slot)
    // for i < length { array[i] = load/unpack(data_slot, i) }
    let mut builder = FunctionBuilder::new(function);
    let slot = builder.add_param(MirType::uint256());
    builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

    let length = builder.sload(slot);
    let (object, layout) =
        builder.alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);

    let data_slot = builder.storage_array_data_slot(slot);
    builder.counted_loop(length, |builder, index| {
        let value = match element {
            StorageArrayElement::Bytes(helper) => {
                let element_slot = builder.add(data_slot, index);
                builder.icall(
                    helper,
                    vec![element_slot],
                    MirType::MemoryObject(MemoryObjectKind::Bytes),
                    1,
                )
            }
            StorageArrayElement::Word => {
                let element_slot = builder.add(data_slot, index);
                builder.sload(element_slot)
            }
            StorageArrayElement::Packed { bytes, encoding, enum_variants } => {
                let value =
                    load_packed_storage_array_element(builder, data_slot, index, bytes, encoding);
                if let Some(variants) = enum_variants {
                    builder.validate_enum_value(variants, value);
                }
                value
            }
        };
        builder.memory_object_store_element(object, layout, index, value);
    });
    builder.ret([object]);
}

fn load_packed_storage_array_element(
    builder: &mut FunctionBuilder<'_>,
    data_slot: ValueId,
    index: ValueId,
    bytes: u8,
    encoding: StorageEncoding,
) -> ValueId {
    // per_slot = 32 / bytes
    // storage_slot = data_slot + index / per_slot
    // shift = (index % per_slot) * bytes * 8
    // value = decode(sload(storage_slot), shift, encoding)
    let (slot_index, index_in_slot) = packed_storage_array_position(builder, index, bytes);
    let storage_slot = builder.add(data_slot, slot_index);
    let word = builder.sload(storage_slot);
    let byte_shift = u64::from(bytes) * 8;
    let shift = if byte_shift.is_power_of_two() {
        let shift = builder.imm(u64::from(byte_shift.trailing_zeros()));
        builder.shl(shift, index_in_slot)
    } else {
        let byte_shift = builder.imm(byte_shift);
        builder.mul(index_in_slot, byte_shift)
    };
    let size = TypeSize::new_int_bits(u16::from(bytes) * 8);
    StorageLocation::packed_word(size, encoding).load_word(builder, word, Some(shift))
}

fn packed_storage_array_position(
    builder: &mut FunctionBuilder<'_>,
    index: ValueId,
    bytes: u8,
) -> (ValueId, ValueId) {
    let per_slot = u64::from(32 / bytes);
    if per_slot.is_power_of_two() {
        let slot_shift = builder.imm(u64::from(per_slot.trailing_zeros()));
        let slot_index = builder.shr(slot_shift, index);
        let slot_mask = builder.imm(per_slot - 1);
        let index_in_slot = builder.and(index, slot_mask);
        (slot_index, index_in_slot)
    } else {
        let per_slot_value = builder.imm(per_slot);
        let slot_index = builder.div(index, per_slot_value);
        let index_in_slot = builder.mod_(index, per_slot_value);
        (slot_index, index_in_slot)
    }
}

/// Builds the helper for clearing the data words of a storage bytes value.
fn build_storage_clear_helper(function: &mut Function) {
    // for i in first_word..words { sstore(data_slot + i, 0) }
    let mut builder = FunctionBuilder::new(function);
    let slot = builder.add_param(MirType::uint256());
    let first_word = builder.add_param(MirType::uint256());
    let words = builder.add_param(MirType::uint256());
    let zero = builder.imm(0);
    emit_clear_storage_words(&mut builder, slot, first_word, words, zero);
    builder.stop();
}

fn emit_clear_storage_words(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    first_word: ValueId,
    words: ValueId,
    zero: ValueId,
) {
    // data_slot = storage_array_data_slot(slot)
    // for i in first_word..words { sstore(data_slot + i, 0) }
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
    let next = builder.add_u64_offset(index, 1);
    let backedge = builder.current_block();
    builder.jump(header);
    builder.add_phi_incoming(index, backedge, next);
    builder.switch_to_block(exit);
}

/// Builds `store_storage_bytes(slot, object)`, shared by every `bytes`/`string` store into
/// storage like solc's `copy_byte_array_to_storage`: one body per contract instead of one per
/// assignment site.
fn build_storage_bytes_store_helper(function: &mut Function, clear_helper: FunctionId) {
    let mut builder = FunctionBuilder::new(function);
    let slot = builder.add_param(MirType::uint256());
    let object = builder.add_param(MirType::MemoryObject(MemoryObjectKind::Bytes));

    // old_is_long, old_length = decode_storage_bytes_header(slot)
    // length, data = bytes(object)
    let (_, old_is_long, old_length) = decode_storage_bytes_header(&mut builder, slot);
    let length = builder.memory_object_len(object, MemoryObjectKind::Bytes);
    let data_ptr = builder.memory_object_data(object, MemoryObjectKind::Bytes);
    let data = builder.make_slice(data_ptr, length, SliceLocation::Memory);
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let old_rounded = builder.add(old_length, thirty_one);
    let old_words = builder.div(old_rounded, word_size);
    let rounded = builder.checked_add(length, thirty_one);
    let words = builder.div(rounded, word_size);
    let zero = builder.imm(0);
    let short = builder.lt(length, word_size);
    let new_words = builder.select(short, zero, words);
    let shrunk = builder.gt(old_length, length);
    let needs_cleanup = builder.and(old_is_long, shrunk);
    let cleanup_block = builder.create_block();
    let write_block = builder.create_block();
    builder.branch(needs_cleanup, cleanup_block, write_block);

    // if old_is_long && old_length > length {
    //     clear_storage_words(slot, new_words, old_words)
    // }
    builder.switch_to_block(cleanup_block);
    builder.icall_void(clear_helper, vec![slot, new_words, old_words], 0);
    builder.jump(write_block);

    builder.switch_to_block(write_block);
    let short_block = builder.create_block();
    let long_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.branch(short, short_block, long_block);

    // header = mask(mload(data), length) | length * 2
    // sstore(slot, header)
    builder.switch_to_block(short_block);
    let data_word = builder.memory_slice_load_word(data, zero);
    let unused_bytes = builder.sub(word_size, length);
    let bits = builder.imm(8);
    let shift = builder.mul(unused_bytes, bits);
    let one = builder.imm(1);
    let high_bit = builder.shl(shift, one);
    let low_mask = builder.sub(high_bit, one);
    let data_mask = builder.not(low_mask);
    let data_word = builder.and(data_word, data_mask);
    let two = builder.imm(2);
    let tag = builder.mul(length, two);
    let header = builder.or(data_word, tag);
    builder.sstore(slot, header);
    builder.jump(merge_block);

    // sstore(slot, length << 1 | 1)
    // data_slot = storage_array_data_slot(slot)
    builder.switch_to_block(long_block);
    let one = builder.imm(1);
    let shifted = builder.shl(one, length);
    let tag = builder.or(shifted, one);
    builder.sstore(slot, tag);
    let data_slot = builder.storage_array_data_slot(slot);

    // for i in 0..length / 32 {
    //     sstore(data_slot + i, mload(data + i * 32))
    // }
    let full_words = builder.div(length, word_size);
    builder.counted_loop(full_words, |builder, index| {
        let byte_offset = builder.mul(index, word_size);
        let value = builder.memory_slice_load_word(data, byte_offset);
        let element_slot = builder.add(data_slot, index);
        builder.sstore(element_slot, value);
    });
    // The final memory word can contain dirty padding bytes, so mask it before storage,
    // matching solc's `copy_byte_array_to_storage`.
    let partial_block = builder.create_block();
    let remainder = builder.and(length, thirty_one);
    let has_partial = builder.iszero(remainder);
    builder.branch(has_partial, merge_block, partial_block);

    // if length % 32 != 0 {
    //     sstore(data_slot + full_words, mask(mload(data + full_words * 32), remainder))
    // }
    builder.switch_to_block(partial_block);
    let partial_offset = builder.mul(full_words, word_size);
    let partial_word = builder.memory_slice_load_word(data, partial_offset);
    let unused_bytes = builder.sub(word_size, remainder);
    let bits = builder.imm(8);
    let shift = builder.mul(unused_bytes, bits);
    let high_bit = builder.shl(shift, one);
    let low_mask = builder.sub(high_bit, one);
    let data_mask = builder.not(low_mask);
    let partial_word = builder.and(partial_word, data_mask);
    let partial_slot = builder.add(data_slot, full_words);
    builder.sstore(partial_slot, partial_word);
    builder.jump(merge_block);

    builder.switch_to_block(merge_block);
    builder.stop();
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn is_constant_storage_assignment(
        &self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
    ) -> bool {
        let ExprKind::Ident(_) = lhs.peel_parens().kind else { return false };
        let Some(id) = self.cx.gcx.resolved_variable(lhs) else { return false };
        if !self.cx.gcx.hir.variable(id).is_state_variable() {
            return false;
        }
        let Some(lhs_ty) = self.cx.gcx.type_of_expr(lhs.id) else { return false };
        let Some(rhs_ty) = self.cx.gcx.type_of_expr(rhs.id) else { return false };
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
                matches!(self.cx.gcx.try_eval_const_value(expr), Ok(ConstValue::String(_)))
            }
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return false;
                };
                let Some(hir::Res::Item(hir::ItemId::Struct(id))) =
                    self.cx.gcx.resolved_expr(callee)
                else {
                    return false;
                };
                if id != struct_id {
                    return false;
                }
                let fields = self.cx.gcx.hir.strukt(struct_id).fields;
                if args.len() != fields.len() {
                    return false;
                }
                let names =
                    self.cx.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                fields.iter().enumerate().all(|(index, &field)| {
                    args.argument_for_parameter(index, Some(names.as_slice())).is_some_and(
                        |argument| {
                            self.is_constant_storage_value(
                                argument,
                                self.cx.gcx.type_of_item(field.into()),
                            )
                        },
                    )
                })
            }
            TyKind::Array(..) | TyKind::DynArray(..) => false,
            TyKind::Elementary(_) => self.cx.gcx.try_eval_const_value(expr).is_ok(),
            _ => false,
        }
    }

    pub(super) fn lower_constant_storage_assignment(
        &mut self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let ty = self.cx.gcx.type_of_expr(lhs.id)?.peel_refs();
        let Some(access) = self.storage_access(lhs) else {
            return self.cx.report_unsupported(lhs.span, "storage access");
        };
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
                let Ok(ConstValue::String(value)) = self.cx.gcx.try_eval_const_value(expr) else {
                    return None;
                };
                self.store_constant_storage_bytes(
                    access.slot,
                    value.as_byte_str_in(self.cx.gcx.sess),
                );
                Some(())
            }
            TyKind::Struct(struct_id) => {
                let ExprKind::Call(callee, args, _) = &expr.peel_parens().kind else {
                    return None;
                };
                let hir::Res::Item(hir::ItemId::Struct(id)) = self.cx.gcx.resolved_expr(callee)?
                else {
                    return None;
                };
                if id != struct_id {
                    return None;
                }
                let fields = self.cx.gcx.hir.strukt(struct_id).fields;
                let names =
                    self.cx.gcx.callable_param_names(CallableParamSource::Struct(struct_id));
                for (index, &field) in fields.iter().enumerate() {
                    let argument = args.argument_for_parameter(index, Some(names.as_slice()))?;
                    let field_ty = self.cx.gcx.type_of_item(field.into());
                    let location = self.cx.storage.field_location(struct_id, index)?;
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
                let constant = match self.cx.gcx.try_eval_const_value(expr) {
                    Ok(ConstValue::Bool(value)) => Some(self.builder.imm_bool(*value)),
                    Ok(ConstValue::Integer(value)) => {
                        value.as_u256().map(|value| self.builder.imm(value))
                    }
                    _ => None,
                };
                let value = if let Some(value) = constant {
                    self.coerce_value(value, self.cx.gcx.type_of_expr(expr.id)?, ty)
                } else {
                    self.lower_typed_expr(expr, ty)?
                };
                self.store_storage_value(ty, access, value, expr.span)
            }
        }
    }

    pub(super) fn storage_access(&mut self, expr: &hir::Expr<'_>) -> Option<StorageAccess> {
        let expr = self.peel_bytes_conversion(expr);
        match &expr.kind {
            ExprKind::Ident(_) => {
                let id = self.cx.gcx.resolved_variable(expr)?;
                if let Some(access) = self.storage_refs.get(&id).copied() {
                    return Some(access);
                }
                let var = self.cx.gcx.hir.variable(id);
                if !var.is_state_variable() {
                    return None;
                }
                let location = self.cx.storage.get(id)?;
                let slot = self.builder.imm(location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Member(receiver, _) => {
                let id = self.cx.gcx.resolved_variable(expr)?;
                let variable = self.cx.gcx.hir.variable(id);
                let hir::ItemId::Struct(struct_id) = variable.parent? else { return None };
                let field = self
                    .cx
                    .gcx
                    .hir
                    .strukt(struct_id)
                    .fields
                    .iter()
                    .position(|&field| field == id)?;
                let base = self.storage_access(receiver)?;
                let location = self.cx.storage.field_location(struct_id, field)?;
                let slot = self.add_storage_offset(base.slot, location.slot);
                Some(StorageAccess { slot, location, offset: None })
            }
            ExprKind::Index(receiver, Some(index)) => {
                let base = self.storage_access(receiver)?;
                let ty = self.cx.gcx.type_of_expr(receiver.id)?.peel_refs();
                if let TyKind::Mapping(key, value) = ty.kind {
                    let index = self
                        .lower_fixed_bytes_literal(key, index)
                        .or_else(|| self.lower_typed_expr(index, key))?;
                    let index = self.normalize_abi_scalar(index, key);
                    let slot = self.mapping_slot(index, key, base.slot);
                    if let Some((size, encoding)) = self.cx.storage.packed_encoding(value) {
                        let location = StorageLocation::packed_word(size, encoding);
                        return Some(StorageAccess { slot, location, offset: None });
                    }
                    return Some(StorageAccess {
                        slot,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    });
                }
                // bounds_check(index, length)
                let index = self.lower_expr(index)?;
                let (element, dynamic, length) = match ty.kind {
                    TyKind::Array(element, len) => (element, false, self.builder.imm(len)),
                    TyKind::DynArray(element) => (element, true, self.builder.sload(base.slot)),
                    _ => return None,
                };
                if self.is_getter {
                    // if index >= length { revert(0, 0) }
                    let valid = self.builder.lt(index, length);
                    self.builder.revert_if_zero(valid, RevertReason::Empty);
                } else {
                    // bounds_check(index, length)
                    self.builder.bounds_check(index, length);
                }
                self.storage_array_element_access(base.slot, index, element, dynamic, expr.span)
            }
            ExprKind::Assign(lhs, None, rhs) if self.is_storage_reference_binding(lhs) => {
                let access = self.storage_access(rhs)?;
                let id = self.cx.gcx.resolved_variable(lhs)?;
                self.storage_refs.insert(id, access);
                Some(access)
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                self.storage_access_ternary(condition, then_expr, else_expr)
            }
            ExprKind::Call(callee, arguments, _)
                if arguments.is_empty()
                    && self.cx.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0) =>
            {
                let ExprKind::Member(receiver, _) = &callee.kind else { return None };
                let (base, element) = self.storage_array_base(receiver)?;
                let (access, new_length) =
                    self.storage_array_push_access(base, element, receiver.span)?;
                self.builder.sstore(base.slot, new_length);
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
        self.cx.gcx.resolved_function(callee).is_some_and(|function_id| {
            self.cx.gcx.hir.function(function_id).returns.first().is_some_and(|&ret| {
                self.cx.gcx.type_of_item(ret.into()).is_ref_at(DataLocation::Storage)
            })
        })
    }

    fn storage_access_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<StorageAccess> {
        let condition = self.lower_expr(condition)?;
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            true,
            |this| this.storage_access(then_expr),
            |this| this.storage_access(else_expr),
        )?;
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
        self.cx
            .gcx
            .resolved_variable(expr)
            .is_some_and(|id| !self.cx.gcx.hir.variable(id).is_state_variable())
            && self
                .cx
                .gcx
                .type_of_expr(expr.id)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
    }

    fn storage_array_push_access(
        &mut self,
        base: StorageAccess,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<(StorageAccess, ValueId)> {
        // old_length = sload(array_slot)
        // if old_length >= 2**64 { panic(MemoryAllocationOverflow) }
        // new_length = add old_length, 1
        // element_slot = storage_array_element_slot(array_slot, old_length)
        //
        // A length at or above 2**64 cannot be reached by growing the array, so
        // it is a forged one, and `keccak256(array_slot) + old_length` then
        // wraps onto unrelated storage. solc's `array_push` caps it the same
        // way, before the element slot is derived. The cap also bounds the new
        // length, so the increment cannot wrap and needs no overflow check.
        let length = self.builder.sload(base.slot);
        let max_length = self.builder.imm(U256::from(u64::MAX));
        let too_long = self.builder.gt(length, max_length);
        self.builder.panic_if(too_long, PanicCode::MemoryAllocationOverflow);
        let one = self.builder.imm(1);
        let new_length = self.builder.add(length, one);
        let access = self.storage_array_element_access(base.slot, length, element, true, span)?;
        Some((access, new_length))
    }

    /// Appends one zeroed byte to a storage `bytes`/`string` value in place and
    /// returns the access for the appended element, like solc's
    /// `array_push_zero` for byte arrays.
    ///
    /// The growth follows Solidity's short/long layout: a value that already is
    /// long only gets a new header, a value that crosses 32 bytes moves its 31
    /// data bytes into the data area, and a value that stays short keeps its
    /// data in the header. The appended byte is zeroed by the header rewrite in
    /// the short cases and by the data area already being clear in the long
    /// ones, so no extra store is needed.
    pub(super) fn grow_storage_bytes(&mut self, slot: ValueId) -> StorageAccess {
        // data = sload(slot)
        // old_length = extract_length(data)
        // new_length = old_length + 1
        // if new_length > 2**64 { panic(MemoryAllocationOverflow) }
        let (data, _, old_length) = decode_storage_bytes_header(&mut self.builder, slot);
        let one = self.builder.imm(1);
        let new_length = self.builder.add(old_length, one);
        let max_length = self.builder.imm(U256::from(1u64) << 64);
        let too_long = self.builder.gt(new_length, max_length);
        self.builder.panic_if(too_long, PanicCode::MemoryAllocationOverflow);

        let long_block = self.builder.create_block();
        let short_block = self.builder.create_block();
        let transition_block = self.builder.create_block();
        let packed_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let last_byte = self.builder.imm(31);
        let is_long = self.builder.gt(old_length, last_byte);
        self.builder.branch(is_long, long_block, short_block);

        // if old_length > 31 {
        //     sstore(slot, new_length << 1 | 1)
        //     word_slot = storage_array_data_slot(slot) + old_length / 32
        // }
        self.builder.switch_to_block(long_block);
        let header = long_storage_bytes_header(&mut self.builder, new_length);
        self.builder.sstore(slot, header);
        let long_slot = long_storage_bytes_byte_slot(&mut self.builder, slot, old_length);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(short_block);
        let at_boundary = self.builder.eq(old_length, last_byte);
        self.builder.branch(at_boundary, transition_block, packed_block);

        // if old_length == 31 {
        //     data_slot = storage_array_data_slot(slot)
        //     sstore(data_slot, data & not(0xff))
        //     sstore(slot, new_length << 1 | 1)
        // }
        self.builder.switch_to_block(transition_block);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let byte_mask = self.builder.imm(0xff);
        let keep_mask = self.builder.not(byte_mask);
        let moved = self.builder.and(data, keep_mask);
        self.builder.sstore(data_slot, moved);
        let header = long_storage_bytes_header(&mut self.builder, new_length);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        // if old_length < 31 { sstore(slot, mask(data, new_length) | new_length * 2) }
        self.builder.switch_to_block(packed_block);
        let header = short_storage_bytes_header(&mut self.builder, data, new_length);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        // word_slot = phi(long_slot, data_slot, slot)
        self.builder.switch_to_block(merge_block);
        let word_slot = self.builder.phi(vec![
            (long_block, long_slot),
            (transition_block, data_slot),
            (packed_block, slot),
        ]);
        storage_bytes_byte_access_at(&mut self.builder, word_slot, old_length)
    }

    pub(super) fn lower_storage_array_push(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        argument: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return self.cx.report_unsupported(expr.span, "storage array push target");
        };
        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            return self.lower_storage_bytes_push(receiver, argument);
        }
        let Some((base, element)) = self.storage_array_base(receiver) else {
            return self.cx.report_unsupported(expr.span, "storage array push target");
        };
        let value = if let Some(argument) = argument {
            // Type checking rejects `push(value)` when the element type contains a (nested)
            // mapping. Bail rather than append an element whose mapping entries the copy skips
            // and which therefore keeps whatever the slots it grew into already held.
            if element.has_mapping(self.cx.gcx) {
                return self
                    .cx
                    .report_unsupported(expr.span, "storage array push of a mapping element");
            }
            let (value, source_ty) = if self.types.memory_layout(element).is_some() {
                let memory_ty = element.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                // The copy into storage converts element-wise, so the source type has to be
                // the argument's own type: a shorter fixed array then copies only the
                // elements it has and the destination's remaining ones are zero-filled.
                let argument_ty = self.cx.gcx.type_of_expr(argument.id);
                let source_ty = argument_ty
                    .map(|ty| ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory))
                    .filter(|&ty| self.types.memory_layout(ty).is_some());
                // A storage source is loaded at its own type as well. Loading it as the
                // destination type reads the slots that follow a shorter source and decodes
                // a differently packed one at the wrong width, so the appended element ends
                // up holding unrelated state.
                let storage_source =
                    argument_ty.is_some_and(|ty| ty.is_ref_at(DataLocation::Storage));
                let load_ty = if storage_source {
                    // Unreachable today: a storage reference always has a memory layout, so
                    // the source type is always there. Kept as a fail-closed guard, because a
                    // type that ever loses one must bail rather than load at the destination
                    // type and append unrelated state.
                    let Some(source_ty) = source_ty else {
                        return self
                            .cx
                            .report_unsupported(argument.span, "storage array push source");
                    };
                    source_ty
                } else {
                    memory_ty
                };
                let source_ty = source_ty.unwrap_or(memory_ty);
                let value = self.lower_typed_expr(argument, load_ty)?;
                // A calldata argument is decoded at the type it is materialized with, so it
                // has to be the argument's own type as well: reading it at the destination
                // type would take the element count and the lengths from the destination.
                (self.materialize_memory_argument(source_ty, value, argument.span)?, source_ty)
            } else {
                let value = self.lower_typed_expr(argument, element)?;
                // `lower_typed_expr` already applies the destination type. Re-coercing
                // fixed-bytes literals shifts their already aligned value a second time.
                (value, element)
            };
            Some((value, source_ty))
        } else {
            None
        };
        // element_slot = storage_array_element_slot(array_slot, old_length)
        // store(element_slot, argument | default)
        // sstore(array_slot, old_length + 1)
        let (element_access, new_length) =
            self.storage_array_push_access(base, element, expr.span)?;
        if let Some((value, source_ty)) = value {
            self.store_storage_value_with_source(
                element,
                source_ty,
                element_access,
                value,
                expr.span,
            )?;
        }
        self.builder.sstore(base.slot, new_length);
        // A plain `push()` writes nothing, and its value is the appended element, which keeps
        // whatever the slot already held. Aggregate elements are reached as a reference through
        // `storage_access`, so only value-typed elements are read back here, and only where the
        // value is observed: a bare `a.push();` would just read the slot it grew into.
        //
        // r = load(element_slot)
        if argument.is_none()
            && !self.discarded_exprs.contains(&expr.id)
            && self.types.memory_layout(element).is_none()
        {
            return self.load_storage_value(element, element_access, expr.span);
        }
        Some(self.builder.imm(U256::ZERO))
    }

    /// Appends one byte to a storage `bytes`/`string` value in place, like
    /// solc's `array_push` for byte arrays.
    fn lower_storage_bytes_push(
        &mut self,
        receiver: &hir::Expr<'_>,
        argument: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        let Some(access) = self.storage_access(receiver) else {
            return self.cx.report_unsupported(receiver.span, "storage access");
        };
        let slot = access.slot;
        let byte_ty = self.cx.gcx.types.fixed_bytes(1);
        let Some(argument) = argument else {
            // The zero byte a plain `push()` appends is written by the growth itself, so the
            // appended element only has to be read back to produce the call's value.
            let element = self.grow_storage_bytes(slot);
            return self.load_storage_value(byte_ty, element, receiver.span);
        };
        let value = self.lower_typed_expr(argument, byte_ty)?;

        // data = sload(slot)
        // old_length = extract_length(data)
        // if !(old_length < 2**64) { panic(MemoryAllocationOverflow) }
        let (data, _, old_length) = decode_storage_bytes_header(&mut self.builder, slot);
        let max_length = self.builder.imm(U256::from(1u64) << 64);
        let in_range = self.builder.lt(old_length, max_length);
        let too_long = self.builder.iszero(in_range);
        self.builder.panic_if(too_long, PanicCode::MemoryAllocationOverflow);

        let long_block = self.builder.create_block();
        let short_block = self.builder.create_block();
        let transition_block = self.builder.create_block();
        let packed_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let last_byte = self.builder.imm(31);
        let is_long = self.builder.gt(old_length, last_byte);
        self.builder.branch(is_long, long_block, short_block);

        // if old_length > 31 {
        //     sstore(slot, data + 2)
        //     word_slot = storage_array_data_slot(slot) + old_length / 32
        //     store_byte(word_slot, 31 - old_length % 32, value)
        // }
        self.builder.switch_to_block(long_block);
        let two = self.builder.imm(2);
        let grown = self.builder.add(data, two);
        self.builder.sstore(slot, grown);
        let word_slot = long_storage_bytes_byte_slot(&mut self.builder, slot, old_length);
        let element = storage_bytes_byte_access_at(&mut self.builder, word_slot, old_length);
        self.store_storage_value(byte_ty, element, value, receiver.span)?;
        self.builder.jump(merge_block);

        self.builder.switch_to_block(short_block);
        let at_boundary = self.builder.eq(old_length, last_byte);
        self.builder.branch(at_boundary, transition_block, packed_block);

        // if old_length == 31 {
        //     sstore(storage_array_data_slot(slot), data & not(0xff) | byte(0, value))
        //     sstore(slot, 65)
        // }
        self.builder.switch_to_block(transition_block);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let byte_mask = self.builder.imm(0xff);
        let keep_mask = self.builder.not(byte_mask);
        let moved = self.builder.and(data, keep_mask);
        let zero = self.builder.imm(0);
        let byte = self.builder.byte(zero, value);
        let word = self.builder.or(moved, byte);
        self.builder.sstore(data_slot, word);
        let header = self.builder.imm(65);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        // if old_length < 31 {
        //     shift = 8 * (31 - old_length)
        //     sstore(slot, (data + 2) & not(0xff << shift) | byte(0, value) << shift)
        // }
        self.builder.switch_to_block(packed_block);
        let two = self.builder.imm(2);
        let grown = self.builder.add(data, two);
        let free_bytes = self.builder.sub(last_byte, old_length);
        let bits = self.builder.imm(8);
        let shift = self.builder.mul(free_bytes, bits);
        let byte_mask = self.builder.imm(0xff);
        let shifted_mask = self.builder.shl(shift, byte_mask);
        let keep_mask = self.builder.not(shifted_mask);
        let cleared = self.builder.and(grown, keep_mask);
        let zero = self.builder.imm(0);
        let byte = self.builder.byte(zero, value);
        let shifted = self.builder.shl(shift, byte);
        let header = self.builder.or(cleared, shifted);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        Some(self.builder.imm(U256::ZERO))
    }

    pub(super) fn lower_storage_array_pop(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let ExprKind::Member(receiver, _) = &callee.kind else {
            return self.cx.report_unsupported(expr.span, "storage array pop target");
        };
        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?.peel_refs();
        if matches!(
            receiver_ty.kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            let Some(access) = self.storage_access(receiver) else {
                return self.cx.report_unsupported(receiver.span, "storage access");
            };
            return self.lower_storage_bytes_pop(access.slot, expr.span);
        }

        let Some((base, element)) = self.storage_array_base(receiver) else {
            return self.cx.report_unsupported(expr.span, "storage array pop target");
        };

        // if length == 0 { panic(EmptyArrayPop) }
        // sstore(array_slot, length - 1)
        // clear(element[length - 1])
        let length = self.builder.sload(base.slot);
        let zero = self.builder.imm(0);
        let empty = self.builder.eq(length, zero);
        self.builder.panic_if(empty, PanicCode::EmptyArrayPop);
        let one = self.builder.imm(1);
        let last = self.builder.sub(length, one);
        self.builder.sstore(base.slot, last);
        let access =
            self.storage_array_element_access(base.slot, last, element, true, expr.span)?;
        self.clear_storage_access(element, access, expr.span)?;
        Some(zero)
    }

    /// Removes the last byte of a storage `bytes`/`string` value in place, like
    /// solc's `byte_array_pop`.
    ///
    /// A value that stays short or long only rewrites its header, plus the data
    /// word holding the removed byte when it is long; the long-to-short
    /// transition at 32 bytes moves the remaining 31 bytes back into the header
    /// and clears the data word.
    fn lower_storage_bytes_pop(&mut self, slot: ValueId, span: Span) -> Option<ValueId> {
        // data = sload(slot)
        // old_length = extract_length(data)
        // if old_length == 0 { panic(EmptyArrayPop) }
        let (data, _, old_length) = decode_storage_bytes_header(&mut self.builder, slot);
        let zero = self.builder.imm(0);
        let empty = self.builder.eq(old_length, zero);
        self.builder.panic_if(empty, PanicCode::EmptyArrayPop);

        let transition_block = self.builder.create_block();
        let resize_block = self.builder.create_block();
        let packed_block = self.builder.create_block();
        let long_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let word_size = self.builder.imm(32);
        let at_boundary = self.builder.eq(old_length, word_size);
        self.builder.branch(at_boundary, transition_block, resize_block);

        // if old_length == 32 {
        //     data_slot = storage_array_data_slot(slot)
        //     sstore(slot, mask(sload(data_slot), 31) | 62)
        //     sstore(data_slot, 0)
        // }
        self.builder.switch_to_block(transition_block);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let word = self.builder.sload(data_slot);
        let last_byte = self.builder.imm(31);
        let header = short_storage_bytes_header(&mut self.builder, word, last_byte);
        self.builder.sstore(slot, header);
        self.builder.sstore(data_slot, zero);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(resize_block);
        let one = self.builder.imm(1);
        let new_length = self.builder.sub(old_length, one);
        let is_short = self.builder.lt(old_length, word_size);
        self.builder.branch(is_short, packed_block, long_block);

        // if old_length < 32 { sstore(slot, mask(data, new_length) | new_length * 2) }
        self.builder.switch_to_block(packed_block);
        let header = short_storage_bytes_header(&mut self.builder, data, new_length);
        self.builder.sstore(slot, header);
        self.builder.jump(merge_block);

        // if old_length > 32 {
        //     word_slot = storage_array_data_slot(slot) + new_length / 32
        //     store_byte(word_slot, 31 - new_length % 32, 0)
        //     sstore(slot, data - 2)
        // }
        self.builder.switch_to_block(long_block);
        let word_slot = long_storage_bytes_byte_slot(&mut self.builder, slot, new_length);
        let element = storage_bytes_byte_access_at(&mut self.builder, word_slot, new_length);
        self.clear_storage_access(self.cx.gcx.types.fixed_bytes(1), element, span)?;
        let two = self.builder.imm(2);
        let shrunk = self.builder.sub(data, two);
        self.builder.sstore(slot, shrunk);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        Some(zero)
    }

    fn storage_array_base(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<(StorageAccess, Ty<'gcx>)> {
        let Some(base) = self.storage_access(receiver) else {
            return self.cx.report_unsupported(receiver.span, "storage access");
        };
        let ty = self.cx.gcx.type_of_expr(receiver.id)?.peel_refs();
        let TyKind::DynArray(element) = ty.kind else { return None };
        Some((base, element))
    }

    fn mapping_slot(&mut self, key: ValueId, key_ty: Ty<'gcx>, slot: ValueId) -> ValueId {
        let is_dynamic = matches!(
            key_ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes)
                | TyKind::Slice(_)
                | TyKind::StringLiteral(..)
        );
        if is_dynamic {
            if self.builder.func().value_slice_location(key) == Some(SliceLocation::Calldata) {
                self.builder.mapping_slot_calldata(key, slot)
            } else {
                self.builder.mapping_slot_memory(key, slot)
            }
        } else {
            let key = self.normalize_dirty_scalar(key, key_ty);
            self.builder.mapping_slot(key, slot)
        }
    }

    pub(super) fn storage_array_element_access(
        &mut self,
        base_slot: ValueId,
        index: ValueId,
        element: Ty<'gcx>,
        dynamic: bool,
        span: Span,
    ) -> Option<StorageAccess> {
        // slot_base = dynamic ? storage_array_data_slot(base_slot) : base_slot
        let slot_base =
            if dynamic { self.builder.storage_array_data_slot(base_slot) } else { base_slot };
        if let Some((size, encoding)) = self.cx.storage.packed_encoding(element)
            && size.bits() < 256
        {
            // slot = slot_base + index / elements_per_slot
            // offset = (index % elements_per_slot) * element_bytes
            let bytes = u64::from(size.bytes());
            let per_slot_value = self.builder.imm(32 / bytes);
            let slot_delta = self.builder.div(index, per_slot_value);
            let slot = self.builder.add(slot_base, slot_delta);
            let index_in_slot = self.builder.mod_(index, per_slot_value);
            let byte_size = self.builder.imm(bytes);
            let offset = self.builder.mul(index_in_slot, byte_size);
            let location = StorageLocation::packed_word(size, encoding);
            return Some(StorageAccess { slot, location, offset: Some(offset) });
        }
        // slot = slot_base + index * element_slots
        let element_slots = self.cx.storage.element_slots(element, span);
        let slot = self.fixed_array_element_slot(slot_base, index, element_slots);
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
            let stride = self.builder.imm(element_slots);
            let offset = self.builder.mul(index, stride);
            self.builder.add(base_slot, offset)
        }
    }

    pub(super) fn add_storage_offset(&mut self, slot: ValueId, offset: U256) -> ValueId {
        if offset.is_zero() {
            slot
        } else {
            let offset = self.builder.imm(offset);
            self.builder.add(slot, offset)
        }
    }

    pub(super) fn load_storage_access(
        &mut self,
        expr: &hir::Expr<'_>,
        access: StorageAccess,
    ) -> Option<ValueId> {
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        if ty.is_ref_at(DataLocation::Storage) {
            return Some(access.slot);
        }
        self.load_storage_value(ty, access, expr.span)
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
            self.cx.storage.load_at_offset(&mut self.builder, access.location, access.slot, offset)
        } else {
            self.cx.storage.load_at(&mut self.builder, access.location, access.slot)
        };
        self.validate_enum(ty, value);
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

    pub(super) fn store_storage_value_with_source(
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
        let dirty = !self.in_inline_assembly && self.dirty_values.contains(&value);
        let value = self.normalize_dirty_scalar(value, ty);
        if !dirty {
            self.validate_enum(ty, value);
        }
        if let Some(offset) = access.offset {
            self.cx.storage.store_at_offset(
                &mut self.builder,
                access.location,
                access.slot,
                offset,
                value,
            );
        } else {
            self.cx.storage.store_at(&mut self.builder, access.location, access.slot, value);
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
                // object = load_storage_bytes(slot)
                Some(self.load_storage_bytes(slot))
            }
            TyKind::Struct(struct_id) => {
                // object = alloc_struct(fields)
                // for field { object[field] = load_storage(field_slot) }
                let fields = self.cx.gcx.hir.strukt(struct_id).fields.len() as u64;
                let (object, layout) = self
                    .builder
                    .alloc_word_struct(fields, AllocationSemantics::SOLIDITY_UNINITIALIZED);
                for (index, &field) in self.cx.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
                    let field_ty = self.cx.gcx.type_of_item(field.into());
                    let location = self.cx.storage.field_location(struct_id, index)?;
                    let field_slot = self.add_storage_offset(slot, location.slot);
                    let value = self.load_storage_value(
                        field_ty,
                        StorageAccess { slot: field_slot, location, offset: None },
                        span,
                    )?;
                    let value = self.encode_memory_scalar(field_ty, value);
                    self.builder.memory_object_store_field(object, layout, index as u64, value);
                }
                Some(object)
            }
            TyKind::Array(element, len) => {
                // object = alloc_fixed_array(len)
                // for i in 0..len { object[i] = load_storage(element_slot(i)) }
                let len = u64::try_from(len).ok()?;
                let element_words = self.types.element_words(element);
                let layout = MemoryObjectLayout::FixedArray { len, element_words };
                let size =
                    self.builder.imm(len.checked_mul(u64::from(element_words))?.saturating_mul(32));
                let object = self.builder.alloc_object(
                    size,
                    layout,
                    AllocationSemantics::SOLIDITY_UNINITIALIZED,
                );
                let len = self.builder.imm(len);
                self.counted_loop(len, |this, index| {
                    let access =
                        this.storage_array_element_access(slot, index, element, false, span)?;
                    let value = this.load_storage_value(element, access, span)?;
                    let value = this.encode_memory_scalar(element, value);
                    this.builder.memory_object_store_element(object, layout, index, value);
                    Some(())
                })?;
                Some(object)
            }
            TyKind::DynArray(element) => self.load_dynamic_storage_object(element, slot, span),
            _ => self.cx.report_unsupported(span, "storage object copy"),
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
            let helper = self
                .lazy_helper(sym::load_storage_bytes_array, |this, function| {
                    let bytes_helper = this.ensure_storage_bytes_helper();
                    build_storage_array_helper(function, StorageArrayElement::Bytes(bytes_helper));
                    Some(())
                })
                .expect("storage bytes array helper construction cannot fail");
            return Some(helper);
        }
        if let solar_sema::ty::TyKind::Struct(struct_id) = element.peel_refs().kind {
            return self.ensure_storage_struct_array_helper(element, struct_id);
        }
        if matches!(element.peel_refs().kind, TyKind::Fn(function) if function.is_external()) {
            return None;
        }
        if let Some((size, encoding)) = self.cx.storage.packed_encoding(element)
            && size.bits() < 256
            && self.types.memory_layout(element).is_none()
        {
            let bytes = size.bytes();
            let enum_variants = match element.peel_refs().kind {
                TyKind::Enum(id) => Some(self.cx.gcx.hir.enumm(id).variants.len() as u64),
                _ => None,
            };
            let name = match enum_variants {
                Some(variants) => helper_name(
                    sym::load_storage_packed_array,
                    format!("{bytes}_{}_enum_{variants}", encoding as u8),
                ),
                None => helper_name(
                    sym::load_storage_packed_array,
                    format!("{bytes}_{}", encoding as u8),
                ),
            };
            let helper = self
                .lazy_helper(name, |_, function| {
                    build_storage_array_helper(
                        function,
                        StorageArrayElement::Packed { bytes, encoding, enum_variants },
                    );
                    Some(())
                })
                .expect("packed storage array helper construction cannot fail");
            return Some(helper);
        }
        if self.types.element_words(element) == 1
            && self.types.memory_layout(element).is_none()
            && self.cx.storage.packed_encoding(element).is_none_or(|(size, _)| size.bits() == 256)
        {
            let helper = self
                .lazy_helper(sym::load_storage_word_array, |_, function| {
                    build_storage_array_helper(function, StorageArrayElement::Word);
                    Some(())
                })
                .expect("storage word array helper construction cannot fail");
            return Some(helper);
        }
        None
    }

    fn ensure_storage_struct_array_helper(
        &mut self,
        element: solar_sema::ty::Ty<'gcx>,
        struct_id: solar_sema::hir::StructId,
    ) -> Option<FunctionId> {
        self.lazy_helper(
            helper_name(sym::load_storage_struct_array, struct_id.index()),
            |this, function| {
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                lowerer.lower_storage_struct_array_helper(element)
            },
        )
    }

    fn lower_storage_struct_array_helper(&mut self, element: Ty<'gcx>) -> Option<()> {
        // length = sload(slot)
        // array = alloc_dynamic_array(length); set_length(array, length)
        // element_slot = storage_array_data_slot(slot)
        // for i < length {
        //     array[i] = load_struct(element_slot)
        //     element_slot += element_slots
        // }
        let slot = self.builder.add_param(MirType::uint256());
        self.builder.add_return(MirType::MemoryObject(MemoryObjectKind::DynamicArray));

        let length = self.builder.sload(slot);
        let (object, layout) = self
            .builder
            .alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        let data_slot = self.builder.storage_array_data_slot(slot);
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let element_slot = self.builder.phi(vec![(preheader, data_slot)]);
        let condition = self.builder.lt(index, length);
        self.builder.branch(condition, body, exit);

        self.builder.switch_to_block(body);
        let value = self.load_storage_object(element, element_slot, Span::DUMMY)?;
        self.builder.memory_object_store_element(object, layout, index, value);
        let next_index = self.builder.add_u64_offset(index, 1);
        let element_slots = self.cx.storage.element_slots(element, Span::DUMMY);
        let next_slot = self.builder.add_u64_offset(element_slot, element_slots);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next_index);
        self.builder.add_phi_incoming(element_slot, backedge, next_slot);

        self.builder.switch_to_block(exit);
        self.builder.ret([object]);
        Some(())
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
            return Some(self.builder.icall(
                helper,
                vec![slot],
                MirType::MemoryObject(layout.kind()),
                1,
            ));
        }

        // length = sload(slot)
        // array = alloc_dynamic_array(length)
        // for i in 0..length { array[i] = load_storage(element_slot(i)) }
        let length = self.builder.sload(slot);
        let stride = self.builder.imm(u64::from(element_words));
        let words = self.builder.checked_mul(length, stride);
        let one = self.builder.imm(1);
        let words = self.builder.checked_add(words, one);
        let word_size = self.builder.imm(32);
        let size = self.builder.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::DynamicArray { element_words };
        let object =
            self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.set_memory_object_len(object, length, layout.kind());

        self.counted_loop(length, |this, index| {
            let access = this.storage_array_element_access(slot, index, element, true, span)?;
            let value = this.load_storage_value(element, access, span)?;
            let value = this.encode_memory_scalar(element, value);
            this.builder.memory_object_store_element(object, layout, index, value);
            Some(())
        })?;
        Some(object)
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
                // store_storage_bytes(slot, object)
                self.store_storage_bytes(slot, object)
            }
            TyKind::Struct(struct_id) => {
                // store_storage_struct(slot, object)
                let TyKind::Struct(source_struct_id) = source_ty.peel_refs().kind else {
                    return self.cx.report_unsupported(span, "storage struct conversion");
                };
                let fields = self.cx.gcx.hir.strukt(struct_id).fields.len() as u64;
                let source_fields = self.cx.gcx.hir.strukt(source_struct_id).fields.len() as u64;
                if fields != source_fields {
                    return self.cx.report_unsupported(span, "storage struct conversion");
                }
                if self.storage_struct_is_recursive(struct_id) {
                    let helper = self.ensure_recursive_storage_helper(
                        RecursiveStorageHelper::Store {
                            target: struct_id,
                            source: source_struct_id,
                        },
                        span,
                    )?;
                    self.builder.icall_void(helper, vec![slot, object], 0);
                    return Some(());
                }
                self.store_storage_struct_fields_with_source(
                    struct_id,
                    source_struct_id,
                    slot,
                    object,
                    span,
                )
            }
            TyKind::Array(element, len) => {
                // for i in 0..len { store_storage(element_slot(i), object[i]) }
                let TyKind::Array(source_element, source_len) = source_ty.peel_refs().kind else {
                    return self.cx.report_unsupported(span, "storage array conversion");
                };
                let len = u64::try_from(len).ok()?;
                let source_len = u64::try_from(source_len).ok()?;
                let layout = self.types.memory_layout(source_ty)?;
                let len = self.builder.imm(len);
                let source_len = self.builder.imm(source_len);
                self.counted_loop(len, |this, index| {
                    let access =
                        this.storage_array_element_access(slot, index, element, false, span)?;
                    let source = this.builder.create_block();
                    let default = this.builder.create_block();
                    let done = this.builder.create_block();
                    let has_source = this.builder.lt(index, source_len);
                    this.builder.branch(has_source, source, default);

                    this.builder.switch_to_block(source);
                    let value = this.builder.memory_object_load_element(object, layout, index);
                    let value = this.decode_memory_scalar(source_element, value);
                    this.store_storage_value_with_source(
                        element,
                        source_element,
                        access,
                        value,
                        span,
                    )?;
                    this.builder.jump(done);

                    this.builder.switch_to_block(default);
                    let value = this.default_value(element);
                    this.store_storage_value(element, access, value, span)?;
                    this.builder.jump(done);
                    this.builder.switch_to_block(done);
                    Some(())
                })?;
                Some(())
            }
            TyKind::DynArray(element) => {
                self.store_dynamic_storage_object(element, source_ty, slot, object, span)
            }
            _ => self.cx.report_unsupported(span, "storage object copy"),
        }
    }

    fn ensure_storage_bytes_helper(&mut self) -> FunctionId {
        self.lazy_helper(sym::load_storage_bytes, |_, function| {
            build_storage_bytes_helper(function);
            Some(())
        })
        .expect("storage bytes helper construction cannot fail")
    }

    pub(super) fn load_storage_bytes(&mut self, slot: ValueId) -> ValueId {
        if !self.cx.share_storage_bytes {
            return lower_storage_bytes_inline(&mut self.builder, slot);
        }
        let helper = self.ensure_storage_bytes_helper();
        self.builder.icall(helper, vec![slot], MirType::MemoryObject(MemoryObjectKind::Bytes), 1)
    }

    /// Reads the length of a storage `bytes`/`string` value from its header slot.
    pub(super) fn storage_bytes_length(&mut self, slot: ValueId) -> ValueId {
        // length = extract_length(sload(slot))
        let (_, _, length) = decode_storage_bytes_header(&mut self.builder, slot);
        length
    }

    /// Resolves one element of a storage `bytes`/`string` value to the single
    /// word that holds it, after bounds-checking the index.
    ///
    /// A short value keeps its data in the header slot, a long value keeps word
    /// `index / 32` at `storage_array_data_slot(slot) + index / 32`. The element
    /// is byte `index % 32` of that word, counted from the most significant
    /// byte, so the packed-word offset is `31 - index % 32`.
    pub(super) fn storage_bytes_byte_access(
        &mut self,
        slot: ValueId,
        index: ValueId,
    ) -> StorageAccess {
        // length = extract_length(sload(slot))
        // bounds_check(index, length)
        let (_, is_long, length) = decode_storage_bytes_header(&mut self.builder, slot);
        self.builder.bounds_check(index, length);

        // word_slot = is_long ? storage_array_data_slot(slot) + index / 32 : slot
        let long_block = self.builder.create_block();
        let short_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.branch(is_long, long_block, short_block);

        self.builder.switch_to_block(long_block);
        let long_slot = long_storage_bytes_byte_slot(&mut self.builder, slot, index);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(short_block);
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        let word_slot = self.builder.phi(vec![(long_block, long_slot), (short_block, slot)]);
        storage_bytes_byte_access_at(&mut self.builder, word_slot, index)
    }

    pub(super) fn store_storage_bytes(&mut self, slot: ValueId, object: ValueId) -> Option<()> {
        // store_storage_bytes(slot, object)
        let clear_helper = self.storage_clear_helper();
        let helper = self.lazy_helper(sym::store_storage_bytes, |_, function| {
            build_storage_bytes_store_helper(function, clear_helper);
            Some(())
        })?;
        self.builder.icall_void(helper, vec![slot, object], 0);
        Some(())
    }

    fn storage_clear_helper(&mut self) -> FunctionId {
        self.lazy_helper(sym::clear_storage_words, |_, function| {
            build_storage_clear_helper(function);
            Some(())
        })
        .expect("storage clear helper construction cannot fail")
    }

    fn clear_storage_words_with_helper(
        &mut self,
        slot: ValueId,
        first_word: ValueId,
        words: ValueId,
    ) {
        let helper = self.storage_clear_helper();
        self.builder.icall_void(helper, vec![slot, first_word, words], 0);
    }

    fn store_constant_storage_bytes(&mut self, slot: ValueId, bytes: &[u8]) {
        let (_, old_is_long, old_length) = decode_storage_bytes_header(&mut self.builder, slot);
        let length = self.builder.imm(bytes.len() as u64);
        let shrunk = self.builder.gt(old_length, length);
        let needs_cleanup = self.builder.and(old_is_long, shrunk);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(needs_cleanup, cleanup_block, write_block);

        // if old_is_long && old_length > length {
        //     clear_storage_words(slot, new_words, old_words)
        // }
        self.builder.switch_to_block(cleanup_block);
        let word_size = self.builder.imm(32);
        let thirty_one = self.builder.imm(31);
        let old_rounded = self.builder.add(old_length, thirty_one);
        let old_words = self.builder.div(old_rounded, word_size);
        let new_words = if bytes.len() < 32 {
            self.builder.imm(0)
        } else {
            self.builder.imm(bytes.len().div_ceil(32) as u64)
        };
        self.clear_storage_words_with_helper(slot, new_words, old_words);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        if bytes.len() < 32 {
            // sstore(slot, bytes_word | length * 2)
            let word = if bytes.is_empty() {
                U256::ZERO
            } else {
                U256::from_be_slice(bytes) << ((32 - bytes.len()) * 8)
            };
            let tag = U256::from((bytes.len() as u64) * 2);
            let value = self.builder.imm(word | tag);
            self.builder.sstore(slot, value);
        } else {
            // sstore(slot, length * 2 + 1)
            // for chunk, i { sstore(storage_array_data_slot(slot) + i, chunk) }
            let tag = U256::from((bytes.len() as u64) * 2 + 1);
            let value = self.builder.imm(tag);
            self.builder.sstore(slot, value);
            let data_slot = self.builder.storage_array_data_slot(slot);
            for (index, chunk) in bytes.chunks(32).enumerate() {
                let word = U256::from_be_slice(chunk) << ((32 - chunk.len()) * 8);
                let index = self.builder.imm(index as u64);
                let element_slot = self.builder.add(data_slot, index);
                let value = self.builder.imm(word);
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
        let (source_element, length, fixed_length) = match source_ty.kind {
            TyKind::DynArray(source_element) | TyKind::Slice(source_element) => {
                (source_element, self.builder.memory_object_len(object, source_layout.kind()), None)
            }
            TyKind::Array(source_element, source_len) => {
                let source_len = u64::try_from(source_len).ok()?;
                (source_element, self.builder.imm(source_len), Some(source_len))
            }
            _ => return self.cx.report_unsupported(span, "storage array conversion"),
        };

        let old_length = self.builder.sload(slot);
        let needs_cleanup = self.builder.gt(old_length, length);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(needs_cleanup, cleanup_block, write_block);

        // if old_length > length {
        //     for i in length..old_length { clear_storage(element_slot(i)) }
        // }
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
        let access = self.storage_array_element_access(slot, index, element, true, span)?;
        self.clear_storage_access(element, access, span)?;
        let next = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.switch_to_block(exit);
        self.builder.jump(write_block);

        self.builder.switch_to_block(write_block);
        // sstore(slot, length)
        // for i in 0..length { store_storage(element_slot(i), object[i]) }
        self.builder.sstore(slot, length);

        self.counted_loop(length, |this, index| {
            let value = this.builder.memory_object_load_element(object, source_layout, index);
            let value = if this.types.memory_layout(source_element).is_some() {
                this.materialize_array_element(object, source_layout, index, source_element, value)?
            } else {
                this.decode_memory_scalar(source_element, value)
            };
            let access = this.storage_array_element_access(slot, index, element, true, span)?;
            this.store_storage_value_with_source(element, source_element, access, value, span)
        })?;

        if let Some((size, _)) = self.cx.storage.packed_encoding(element)
            && 32 / size.bytes() > 1
        {
            // Packed element stores preserve neighboring bits, including stale values past the
            // logical length.
            let per_slot = u64::from(32 / size.bytes());
            if let Some(length) = fixed_length {
                let remainder = (length % per_slot) as u16;
                if remainder != 0 {
                    let slot_index = self.builder.imm(length / per_slot);
                    let used_bits = size.bits() * remainder;
                    let mask = (U256::from(1) << used_bits) - U256::from(1);
                    let mask = self.builder.imm(mask);
                    self.clear_unused_packed_array_elements(slot, slot_index, mask);
                }
            } else {
                let (slot_index, remainder) =
                    packed_storage_array_position(&mut self.builder, length, size.bytes());
                let no_partial_slot = self.builder.iszero(remainder);
                let cleanup_block = self.builder.create_block();
                let merge_block = self.builder.create_block();
                self.builder.branch(no_partial_slot, merge_block, cleanup_block);

                self.builder.switch_to_block(cleanup_block);
                let element_bits = self.builder.imm(u64::from(size.bits()));
                let used_bits = self.builder.mul(remainder, element_bits);
                let one = self.builder.imm(1);
                let high_bit = self.builder.shl(used_bits, one);
                let mask = self.builder.sub(high_bit, one);
                self.clear_unused_packed_array_elements(slot, slot_index, mask);
                self.builder.jump(merge_block);

                self.builder.switch_to_block(merge_block);
            }
        }
        Some(())
    }

    fn clear_unused_packed_array_elements(
        &mut self,
        slot: ValueId,
        slot_index: ValueId,
        mask: ValueId,
    ) {
        let data_slot = self.builder.storage_array_data_slot(slot);
        let partial_slot = self.builder.add(data_slot, slot_index);
        let word = self.builder.sload(partial_slot);
        let word = self.builder.and(word, mask);
        self.builder.sstore(partial_slot, word);
    }

    fn store_storage_struct_fields_with_source(
        &mut self,
        struct_id: solar_sema::hir::StructId,
        source_struct_id: solar_sema::hir::StructId,
        slot: ValueId,
        object: ValueId,
        span: Span,
    ) -> Option<()> {
        let source_ty = self.cx.gcx.mk_ty(TyKind::Struct(source_struct_id));
        let layout = self.types.memory_layout(source_ty)?;
        for (index, &field) in self.cx.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
            let field_ty = self.cx.gcx.type_of_item(field.into());
            let location = self.cx.storage.field_location(struct_id, index)?;
            let field_slot = self.add_storage_offset(slot, location.slot);
            let value = self.builder.memory_object_load_field(object, layout, index as u64);
            let source_field = self.cx.gcx.hir.strukt(source_struct_id).fields[index];
            let source_field_ty = self.cx.gcx.type_of_item(source_field.into());
            let value = self.decode_memory_scalar(source_field_ty, value);
            let access = StorageAccess { slot: field_slot, location, offset: None };
            self.store_storage_value_with_source(field_ty, source_field_ty, access, value, span)?;
        }
        Some(())
    }

    fn ensure_recursive_storage_helper(
        &mut self,
        helper: RecursiveStorageHelper,
        span: Span,
    ) -> Option<FunctionId> {
        let name = match helper {
            RecursiveStorageHelper::Store { .. } => sym::store_recursive_storage,
            RecursiveStorageHelper::Clear { .. } => sym::clear_recursive_storage,
        };
        let name = match helper {
            RecursiveStorageHelper::Store { target, source } => {
                helper_name(name, format!("{}_{}", target.index(), source.index()))
            }
            RecursiveStorageHelper::Clear { target } => helper_name(name, target.index()),
        };
        self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let slot = lowerer.builder.add_param(MirType::uint256());
            let lowered = match helper {
                RecursiveStorageHelper::Store { target, source } => {
                    let object =
                        lowerer.builder.add_param(MirType::MemoryObject(MemoryObjectKind::Struct));
                    lowerer
                        .store_storage_struct_fields_with_source(target, source, slot, object, span)
                        .is_some()
                }
                RecursiveStorageHelper::Clear { target } => {
                    lowerer.clear_storage_struct_fields(target, slot, span).is_some()
                }
            };
            if lowered {
                lowerer.builder.stop();
            }
            lowered.then_some(())
        })
    }

    pub(super) fn clear_storage_access(
        &mut self,
        ty: Ty<'gcx>,
        access: StorageAccess,
        span: Span,
    ) -> Option<()> {
        let zero = self.builder.imm(U256::ZERO);
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                // clear_storage_bytes(slot)
                self.clear_storage_bytes(access.slot)
            }
            TyKind::DynArray(element) => {
                // length = sload(slot)
                // sstore(slot, 0)
                // for i in 0..length { clear_storage(element_slot(i)) }
                let length = self.builder.sload(access.slot);
                self.builder.sstore(access.slot, zero);

                if let Some((size, _)) = self.cx.storage.packed_encoding(element)
                    && size.bits() < 256
                {
                    let elements_per_slot = self.builder.imm(32 / u64::from(size.bytes()));
                    let full_slots = self.builder.div(length, elements_per_slot);
                    let remainder = self.builder.mod_(length, elements_per_slot);
                    let remainder_is_zero = self.builder.iszero(remainder);
                    let has_partial_slot = self.builder.iszero(remainder_is_zero);
                    let slots = self.builder.add(full_slots, has_partial_slot);
                    emit_clear_storage_words(&mut self.builder, access.slot, zero, slots, zero);
                    return Some(());
                }

                self.counted_loop(length, |this, index| {
                    let element_access =
                        this.storage_array_element_access(access.slot, index, element, true, span)?;
                    this.clear_storage_access(element, element_access, span)
                })?;
            }
            TyKind::Struct(struct_id) => {
                // clear_storage_struct(slot)
                if self.storage_struct_is_recursive(struct_id) {
                    let helper = self.ensure_recursive_storage_helper(
                        RecursiveStorageHelper::Clear { target: struct_id },
                        span,
                    )?;
                    self.builder.icall_void(helper, vec![access.slot], 0);
                } else {
                    self.clear_storage_struct_fields(struct_id, access.slot, span)?;
                }
            }
            TyKind::Array(element, len) => {
                // for i in 0..len { clear_storage(element_slot(i)) }
                if let Some((size, _)) = self.cx.storage.packed_encoding(element)
                    && size.bits() < 256
                {
                    let slots = self.cx.storage.element_slots(ty, span);
                    let slots = self.builder.imm(slots);
                    self.counted_loop(slots, |this, index| {
                        let slot = this.builder.add(access.slot, index);
                        this.builder.sstore(slot, zero);
                    });
                    return Some(());
                }

                let len = u64::try_from(len).ok()?;
                let len = self.builder.imm(len);
                self.counted_loop(len, |this, index| {
                    let element_access = this.storage_array_element_access(
                        access.slot,
                        index,
                        element,
                        false,
                        span,
                    )?;
                    this.clear_storage_access(element, element_access, span)
                })?;
            }
            TyKind::Mapping(..) => {}
            _ => {
                if let Some(offset) = access.offset {
                    self.cx.storage.store_at_offset(
                        &mut self.builder,
                        access.location,
                        access.slot,
                        offset,
                        zero,
                    );
                } else {
                    self.cx.storage.store_at(&mut self.builder, access.location, access.slot, zero);
                }
            }
        }
        Some(())
    }

    fn clear_storage_struct_fields(
        &mut self,
        struct_id: solar_sema::hir::StructId,
        slot: ValueId,
        span: Span,
    ) -> Option<()> {
        let zero = self.builder.imm(U256::ZERO);
        let mut cleared_packed_slot = None;
        for (index, &field) in self.cx.gcx.hir.strukt(struct_id).fields.iter().enumerate() {
            let field_ty = self.cx.gcx.type_of_item(field.into());
            let location = self.cx.storage.field_location(struct_id, index)?;
            let field_slot = self.add_storage_offset(slot, location.slot);
            if location.size.bits() < 256 {
                if cleared_packed_slot != Some(location.slot) {
                    self.builder.sstore(field_slot, zero);
                    cleared_packed_slot = Some(location.slot);
                }
                continue;
            }
            self.clear_storage_access(
                field_ty,
                StorageAccess { slot: field_slot, location, offset: None },
                span,
            )?;
        }
        Some(())
    }

    fn storage_struct_is_recursive(&self, struct_id: solar_sema::hir::StructId) -> bool {
        let mut visiting = FxHashSet::default();
        self.cx.gcx.hir.strukt(struct_id).fields.iter().any(|&field| {
            self.storage_type_reaches_struct(
                self.cx.gcx.type_of_item(field.into()),
                struct_id,
                &mut visiting,
            )
        })
    }

    fn storage_type_reaches_struct(
        &self,
        ty: Ty<'gcx>,
        target: solar_sema::hir::StructId,
        visiting: &mut FxHashSet<solar_sema::hir::StructId>,
    ) -> bool {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                if struct_id == target {
                    return true;
                }
                if !visiting.insert(struct_id) {
                    return false;
                }
                let contains = self.cx.gcx.hir.strukt(struct_id).fields.iter().any(|&field| {
                    self.storage_type_reaches_struct(
                        self.cx.gcx.type_of_item(field.into()),
                        target,
                        visiting,
                    )
                });
                visiting.remove(&struct_id);
                contains
            }
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                self.storage_type_reaches_struct(element, target, visiting)
            }
            _ => false,
        }
    }

    fn clear_storage_bytes(&mut self, slot: ValueId) {
        let (_, is_long, length) = decode_storage_bytes_header(&mut self.builder, slot);
        let zero = self.builder.imm(0);
        let word_size = self.builder.imm(32);
        let thirty_one = self.builder.imm(31);
        let rounded = self.builder.add(length, thirty_one);
        let words = self.builder.div(rounded, word_size);
        let cleanup_block = self.builder.create_block();
        let write_block = self.builder.create_block();
        self.builder.branch(is_long, cleanup_block, write_block);

        // if is_long { clear_storage_words(slot, 0, words) }
        self.builder.switch_to_block(cleanup_block);
        emit_clear_storage_words(&mut self.builder, slot, zero, words, zero);
        self.builder.jump(write_block);

        // sstore(slot, 0)
        self.builder.switch_to_block(write_block);
        self.builder.sstore(slot, zero);
    }
}

fn decode_storage_bytes_header(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
) -> (ValueId, ValueId, ValueId) {
    // header = sload(slot)
    // flag = header & 1; is_long = (flag == 1)
    // half = header >> 1
    // length = is_long ? half : (half & 0x7f)
    // if invalid_short_long_encoding { panic(StorageEncoding) }
    let header = builder.sload(slot);
    let one = builder.imm(1);
    let flag = builder.and(header, one);
    let is_long = builder.eq(flag, one);
    let shift = builder.imm(1);
    let half = builder.shr(shift, header);
    let short_mask = builder.imm(0x7f);
    let short_len = builder.and(half, short_mask);
    let length = builder.select(is_long, half, short_len);
    let thirty_two = builder.imm(32);
    let short_length = builder.lt(length, thirty_two);
    let invalid_encoding = builder.eq(is_long, short_length);
    builder.panic_if(invalid_encoding, PanicCode::StorageEncoding);
    (header, is_long, length)
}

/// Keeps only the first `length` bytes of a storage `bytes` header word, like
/// solc's `mask_bytes_dynamic`.
fn mask_storage_bytes_data(
    builder: &mut FunctionBuilder<'_>,
    data: ValueId,
    length: ValueId,
) -> ValueId {
    // mask = not((1 << (8 * (32 - length))) - 1)
    // masked = data & mask
    let word_size = builder.imm(32);
    let unused_bytes = builder.sub(word_size, length);
    let bits = builder.imm(8);
    let shift = builder.mul(unused_bytes, bits);
    let one = builder.imm(1);
    let high_bit = builder.shl(shift, one);
    let low_mask = builder.sub(high_bit, one);
    let data_mask = builder.not(low_mask);
    builder.and(data, data_mask)
}

/// Builds the header word of a short storage `bytes` value of `length` bytes,
/// like solc's `extract_used_part_and_set_length_of_short_byte_array`.
fn short_storage_bytes_header(
    builder: &mut FunctionBuilder<'_>,
    data: ValueId,
    length: ValueId,
) -> ValueId {
    // header = mask(data, length) | length * 2
    let masked = mask_storage_bytes_data(builder, data, length);
    let two = builder.imm(2);
    let tag = builder.mul(length, two);
    builder.or(masked, tag)
}

/// Builds the header word of a long storage `bytes` value of `length` bytes.
fn long_storage_bytes_header(builder: &mut FunctionBuilder<'_>, length: ValueId) -> ValueId {
    // header = length << 1 | 1
    let one = builder.imm(1);
    let shifted = builder.shl(one, length);
    builder.or(shifted, one)
}

/// The data-area slot holding byte `index` of a long storage `bytes` value,
/// like solc's `long_byte_array_index_access_no_checks`.
fn long_storage_bytes_byte_slot(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    index: ValueId,
) -> ValueId {
    // word_slot = storage_array_data_slot(slot) + index / 32
    let data_slot = builder.storage_array_data_slot(slot);
    let word_shift = builder.imm(5);
    let word_index = builder.shr(word_shift, index);
    builder.add(data_slot, word_index)
}

/// The packed access for byte `index` of a storage `bytes` value inside
/// `word_slot`. Bytes are counted from the most significant one, so the
/// packed-word offset is `31 - index % 32`.
fn storage_bytes_byte_access_at(
    builder: &mut FunctionBuilder<'_>,
    word_slot: ValueId,
    index: ValueId,
) -> StorageAccess {
    // offset = 31 - index % 32
    let last_byte = builder.imm(31);
    let index_in_word = builder.and(index, last_byte);
    let offset = builder.sub(last_byte, index_in_word);
    let location =
        StorageLocation::packed_word(TypeSize::new_int_bits(8), StorageEncoding::FixedBytes);
    StorageAccess { slot: word_slot, location, offset: Some(offset) }
}

fn lower_storage_bytes_inline(builder: &mut FunctionBuilder<'_>, slot: ValueId) -> ValueId {
    let (header, is_long, length) = decode_storage_bytes_header(builder, slot);
    let thirty_two = builder.imm(32);
    let thirty_one = builder.imm(31);
    let rounded = builder.add(length, thirty_one);
    let words = builder.div(rounded, thirty_two);
    let object = builder.alloc_bytes_object(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);

    let short_block = builder.create_block();
    let long_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.branch(is_long, long_block, short_block);

    builder.switch_to_block(short_block);
    let zero = builder.imm(0);
    let short_mask = builder.imm(U256::MAX << 8);
    let short_data = builder.and(header, short_mask);
    builder.memory_object_store_word(object, zero, short_data);
    builder.jump(merge_block);

    builder.switch_to_block(long_block);
    let data_slot = builder.storage_array_data_slot(slot);
    builder.counted_loop(words, |builder, index| {
        let element_slot = builder.add(data_slot, index);
        let value = builder.sload(element_slot);
        let byte_offset = builder.mul(index, thirty_two);
        builder.memory_object_store_word(object, byte_offset, value);
    });
    builder.jump(merge_block);

    builder.switch_to_block(merge_block);
    object
}
