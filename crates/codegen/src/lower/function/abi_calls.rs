//! ABI call argument and calldata materialization helpers.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn is_storage_parameter(ty: Ty<'gcx>) -> bool {
        ty.is_ref_at(DataLocation::Storage) || matches!(ty.peel_refs().kind, TyKind::Mapping(..))
    }

    pub(super) fn needs_calldata_materialization(&self, value: ValueId, ty: &AbiType) -> bool {
        if !matches!(
            self.builder.func().value_ty(value),
            Some(MirType::Slice(SliceLocation::Calldata))
        ) {
            return false;
        }
        match ty {
            AbiType::Bytes(SliceLocation::Memory)
            | AbiType::DynamicArray { location: SliceLocation::Memory, .. } => true,
            AbiType::DynamicArray { element, location: SliceLocation::Calldata } => {
                !matches!(element.as_ref(), AbiType::Word)
            }
            _ => false,
        }
    }

    pub(super) fn materialize_memory_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let Some(MirType::Slice(location)) = self.builder.func().value_ty(value) else {
            return Some(value);
        };
        match location {
            SliceLocation::Calldata => self.materialize_calldata_argument(ty, value, span),
            SliceLocation::Memory => match ty.peel_refs().kind {
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                    | solar_sema::hir::ElementaryType::String,
                ) => Some(self.materialize_memory_slice(value)),
                _ => report_unsupported(self.gcx, span, "memory slice materialization"),
            },
            SliceLocation::Returndata => {
                report_unsupported(self.gcx, span, "returndata slice materialization")
            }
        }
    }

    pub(super) fn lower_typed_expr(
        &mut self,
        expr: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<ValueId> {
        if !ty.is_ref_at(DataLocation::Storage)
            && self.types.memory_layout(ty).is_some()
            && self
                .gcx
                .type_of_expr(expr.id)
                .is_some_and(|source| source.is_ref_at(DataLocation::Storage))
        {
            let access = self.storage_access(expr)?;
            return self.load_storage_object(ty, access.slot, expr.span);
        }
        self.lower_expr(expr)
    }

    pub(super) fn lower_abi_call_argument(
        &mut self,
        argument: &hir::Expr<'_>,
        parameter_ty: Ty<'gcx>,
    ) -> Option<(ValueId, AbiType)> {
        let mut value = self.lower_typed_expr(argument, parameter_ty)?;
        let mut abi_type = self.types.abi_type(parameter_ty)?;
        abi_type = self.abi_type_for_value(value, abi_type);
        if self.needs_calldata_materialization(value, &abi_type) {
            value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
            abi_type = Self::memory_abi_type(abi_type);
        }
        Some((value, abi_type))
    }

    pub(super) fn materialize_call_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                self.builder.func().value_ty(value),
                Some(MirType::Slice(SliceLocation::Calldata))
            )
        {
            Some(value)
        } else {
            self.materialize_memory_argument(ty, value, span)
        }
    }

    pub(super) fn memory_abi_type(ty: AbiType) -> AbiType {
        match ty {
            AbiType::Word => AbiType::Word,
            AbiType::Bytes(_) => AbiType::Bytes(SliceLocation::Memory),
            AbiType::DynamicArray { element, .. } => AbiType::DynamicArray {
                element: Box::new(Self::memory_abi_type(*element)),
                location: SliceLocation::Memory,
            },
            AbiType::FixedArray { element, len } => {
                AbiType::FixedArray { element: Box::new(Self::memory_abi_type(*element)), len }
            }
            AbiType::Tuple(fields) => {
                AbiType::Tuple(fields.into_vec().into_iter().map(Self::memory_abi_type).collect())
            }
        }
    }

    pub(super) fn abi_type_for_value(&self, value: ValueId, ty: AbiType) -> AbiType {
        if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::MemoryObject(_)) | Some(MirType::Slice(SliceLocation::Memory))
        ) || matches!(self.builder.func().value(value), Value::Inst(inst) if matches!(
            &self.builder.func().inst(*inst).kind,
            InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
        )) {
            Self::memory_abi_type(ty)
        } else {
            ty
        }
    }

    pub(super) fn materialize_calldata_argument(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        match ty.peel_refs().kind {
            TyKind::Slice(element)
                if matches!(
                    element.peel_refs().kind,
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
                ) =>
            {
                Some(self.materialize_memory_slice(value))
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.materialize_memory_slice(value)),
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                let element_type = self.types.abi_type(element)?;
                let length = self.builder.slice_len(value);
                let data = self.builder.slice_ptr(value);
                if matches!(element_type, AbiType::Word) {
                    return Some(self.copy_calldata_word_array(data, length));
                }
                self.materialize_calldata_nested_array(element, data, length, span)
            }
            _ => report_unsupported(self.gcx, span, "calldata argument materialization"),
        }
    }

    fn copy_calldata_word_array(&mut self, data: ValueId, length: ValueId) -> ValueId {
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let size = self.checked_add(word, byte_length);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::DynamicArray);
        let source = self.builder.make_slice(data, byte_length, SliceLocation::Calldata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::DynamicArray, source);
        object
    }

    fn materialize_calldata_nested_array(
        &mut self,
        element: Ty<'gcx>,
        data: ValueId,
        length: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        let element_head_size = self.builder.imm_u64(self.types.abi_type(element)?.head_size());
        let payload_size = self.checked_mul(length, word);
        let size = self.checked_add(word, payload_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::DynamicArray);

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let offset = self.checked_mul(index, element_head_size);
        let head = self.builder.add(data, offset);
        let value = self.materialize_calldata_value_at_inner(element, head, data, span, false)?;
        self.builder.memory_object_store_element(
            object,
            MemoryObjectLayout::WORD_ARRAY,
            index,
            value,
        );
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        Some(object)
    }

    pub(super) fn materialize_calldata_index_value_at(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        self.materialize_calldata_value_at_inner(ty, head, tuple_base, span, validate_bounds)
    }

    fn materialize_calldata_value_at_inner(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            return self.materialize_calldata_value_at_inner(
                inner,
                head,
                tuple_base,
                span,
                validate_bounds,
            );
        }
        let word = self.builder.imm_u64(32);
        let value_pos = if self.types.abi_type(ty)?.is_dynamic() {
            if validate_bounds {
                self.check_calldata_range(head, word);
            }
            let offset = self.calldata_load_word(head);
            let value_pos = self.builder.add(tuple_base, offset);
            if validate_bounds {
                let overflow = self.builder.lt(value_pos, tuple_base);
                self.revert_if_calldata_invalid(overflow);
            }
            value_pos
        } else {
            head
        };
        match ty.kind {
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.materialize_calldata_bytes_at(value_pos)),
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                if validate_bounds {
                    self.check_calldata_range(value_pos, word);
                }
                let length = self.calldata_load_word(value_pos);
                let data = self.builder.add(value_pos, word);
                let element_type = self.types.abi_type(element)?;
                if matches!(element_type, AbiType::Word) {
                    if validate_bounds {
                        let element_head_size = self.builder.imm_u64(element_type.head_size());
                        let head_size = self.checked_mul(length, element_head_size);
                        self.check_calldata_range(data, head_size);
                    }
                    Some(self.copy_calldata_word_array(data, length))
                } else {
                    if validate_bounds {
                        let element_head_size = self.builder.imm_u64(element_type.head_size());
                        let head_size = self.checked_mul(length, element_head_size);
                        self.check_calldata_range(data, head_size);
                    }
                    self.materialize_calldata_nested_array(element, data, length, span)
                }
            }
            TyKind::Array(element, length) => {
                let length = u64::try_from(length).ok()?;
                self.materialize_calldata_fixed_array(
                    element,
                    length,
                    value_pos,
                    span,
                    validate_bounds,
                )
            }
            TyKind::Struct(id) => {
                let fields = self.gcx.hir.strukt(id).fields.to_vec();
                let field_types = fields
                    .iter()
                    .map(|&field| self.gcx.type_of_item(field.into()))
                    .collect::<Vec<_>>();
                self.materialize_calldata_fields(field_types, value_pos, span, validate_bounds)
            }
            TyKind::Tuple(fields) => self.materialize_calldata_fields(
                fields.iter().copied(),
                value_pos,
                span,
                validate_bounds,
            ),
            _ => Some(self.calldata_load_word(value_pos)),
        }
    }

    fn check_calldata_range(&mut self, start: ValueId, size: ValueId) {
        let end = self.builder.add(start, size);
        let overflow = self.builder.lt(end, start);
        let calldata_size = self.builder.calldatasize();
        let out_of_bounds = self.builder.gt(end, calldata_size);
        let invalid = self.builder.or(overflow, out_of_bounds);
        self.revert_if_calldata_invalid(invalid);
    }

    fn revert_if_calldata_invalid(&mut self, condition: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, revert, continue_block);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u64(0);
        self.builder.revert(zero, zero);
        self.builder.switch_to_block(continue_block);
    }

    pub(super) fn calldata_load_word(&mut self, pointer: ValueId) -> ValueId {
        let length = self.builder.imm_u64(32);
        let slice = self.builder.make_slice(pointer, length, SliceLocation::Calldata);
        let zero = self.builder.imm_u64(0);
        self.builder.calldata_slice_load_word(slice, zero)
    }

    fn materialize_calldata_bytes_at(&mut self, position: ValueId) -> ValueId {
        let length = self.calldata_load_word(position);
        let word = self.builder.imm_u64(32);
        let data = self.builder.add(position, word);
        let slice = self.builder.make_slice(data, length, SliceLocation::Calldata);
        self.materialize_memory_slice(slice)
    }

    fn materialize_calldata_fixed_array(
        &mut self,
        element: Ty<'gcx>,
        length: u64,
        base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        let length_value = self.builder.imm_u64(length);
        let size = self.checked_mul(length_value, word);
        let element_head_size = self.types.abi_type(element)?.head_size();
        let layout = MemoryObjectLayout::FixedArray { len: length, element_words: 1 };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        for index in 0..length {
            let index_value = self.builder.imm_u64(index);
            let element_head_size_value = self.builder.imm_u64(element_head_size);
            let head_offset = self.checked_mul(index_value, element_head_size_value);
            let head = self.builder.add(base, head_offset);
            let value = self.materialize_calldata_value_at_inner(
                element,
                head,
                base,
                span,
                validate_bounds,
            )?;
            self.builder.memory_object_store_element(object, layout, index_value, value);
        }
        Some(object)
    }

    fn materialize_calldata_fields(
        &mut self,
        fields: impl IntoIterator<Item = Ty<'gcx>>,
        base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let fields = fields.into_iter().collect::<Vec<_>>();
        let layout = MemoryObjectLayout::Struct { fields: fields.len() as u64 };
        let size = self.builder.imm_u64(fields.len().checked_mul(32)? as u64);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        let mut offset = 0u64;
        for (index, field) in fields.iter().copied().enumerate() {
            let field_offset = self.builder.imm_u64(offset);
            let head = self.builder.add(base, field_offset);
            let value =
                self.materialize_calldata_value_at_inner(field, head, base, span, validate_bounds)?;
            self.builder.memory_object_store_field(object, layout, index as u64, value);
            offset = offset.checked_add(self.types.abi_type(field)?.head_size())?;
        }
        Some(object)
    }
}
