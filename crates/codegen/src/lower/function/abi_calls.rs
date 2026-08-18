//! ABI call argument and calldata materialization helpers.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
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
            | AbiType::DynamicArray { location: SliceLocation::Memory, .. }
            | AbiType::FixedArray { .. }
            | AbiType::Tuple(_) => false,
            AbiType::DynamicArray { element, location: SliceLocation::Calldata } => {
                !matches!(element.as_ref(), AbiType::Word | AbiType::Function | AbiType::Bytes(_))
            }
            _ => false,
        }
    }

    pub(super) fn needs_validated_calldata_materialization(
        &self,
        value: ValueId,
        abi_type: &AbiType,
        ty: Ty<'gcx>,
    ) -> bool {
        self.needs_calldata_materialization(value, abi_type)
            || (self.needs_calldata_aggregate_validation(value, ty)
                && !self.can_defer_calldata_validation(value, abi_type))
    }

    fn can_defer_calldata_validation(&self, value: ValueId, abi_type: &AbiType) -> bool {
        self.is_external_abi_argument(value)
            && matches!(
                abi_type,
                AbiType::DynamicArray {
                    element,
                    location: SliceLocation::Calldata,
                } if matches!(element.as_ref(), AbiType::Word | AbiType::Bytes(_))
            )
    }

    pub(super) fn needs_calldata_aggregate_validation(&self, value: ValueId, ty: Ty<'gcx>) -> bool {
        matches!(self.builder.func().value_ty(value), Some(MirType::Slice(SliceLocation::Calldata)))
            && self.calldata_aggregate_requires_validation(ty)
    }

    /// Validates a static calldata aggregate in place so ABI encoding can read
    /// its words directly without copying the aggregate to memory.
    pub(super) fn validate_calldata_static_argument(
        &mut self,
        value: ValueId,
        ty: Ty<'gcx>,
    ) -> bool {
        if !self.needs_calldata_aggregate_validation(value, ty) {
            return false;
        }
        let Some(abi_type) = self.types.abi_type(ty) else { return false };
        if abi_type.is_dynamic() {
            return false;
        }

        let base = self.builder.slice_ptr(value);
        let length = self.builder.slice_len(value);
        let size = self.builder.imm_u64(abi_type.head_size());
        let too_short = self.builder.gt(size, length);
        self.revert_if_calldata_invalid(too_short);
        self.check_calldata_range(base, size);
        self.validate_calldata_static_value(ty, base);
        true
    }

    fn validate_calldata_static_value(&mut self, ty: Ty<'gcx>, base: ValueId) {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            self.validate_calldata_static_value(inner, base);
            return;
        }

        match ty.kind {
            TyKind::Array(element, length) => {
                let Ok(length) = u64::try_from(length) else { return };
                let Some(element_size) = self.types.abi_type(element).map(|ty| ty.head_size())
                else {
                    return;
                };
                for index in 0..length {
                    let offset = self.builder.imm_u64(index.saturating_mul(element_size));
                    let position = self.builder.add(base, offset);
                    self.validate_calldata_static_value(element, position);
                }
            }
            TyKind::Struct(id) => {
                let mut offset = 0;
                for &field in self.gcx.hir.strukt(id).fields {
                    let field_ty = self.gcx.type_of_item(field.into());
                    let position = if offset == 0 {
                        base
                    } else {
                        let offset_value = self.builder.imm_u64(offset);
                        self.builder.add(base, offset_value)
                    };
                    self.validate_calldata_static_value(field_ty, position);
                    let Some(field_size) = self.types.abi_type(field_ty).map(|ty| ty.head_size())
                    else {
                        return;
                    };
                    offset = offset.saturating_add(field_size);
                }
            }
            TyKind::Tuple(fields) => {
                let mut offset = 0;
                for &field_ty in fields {
                    let position = if offset == 0 {
                        base
                    } else {
                        let offset_value = self.builder.imm_u64(offset);
                        self.builder.add(base, offset_value)
                    };
                    self.validate_calldata_static_value(field_ty, position);
                    let Some(field_size) = self.types.abi_type(field_ty).map(|ty| ty.head_size())
                    else {
                        return;
                    };
                    offset = offset.saturating_add(field_size);
                }
            }
            _ => {
                if !Self::calldata_word_is_full_width(ty) {
                    let _ = self.decode_calldata_word(ty, base, false);
                }
            }
        }
    }

    pub(super) fn validate_calldata_array_head(
        &mut self,
        value: ValueId,
        ty: Ty<'gcx>,
        abi_type: &AbiType,
    ) {
        if self.is_external_abi_argument(value) {
            return;
        }
        if !matches!(
            self.builder.func().value_ty(value),
            Some(MirType::Slice(SliceLocation::Calldata))
        ) || !matches!(ty.peel_refs().kind, TyKind::DynArray(_))
        {
            return;
        }
        let AbiType::DynamicArray { element, location: SliceLocation::Calldata } = abi_type else {
            return;
        };
        let word = self.builder.imm_u64(element.head_size());
        let length = self.builder.slice_len(value);
        let data = self.builder.slice_ptr(value);
        let byte_length = self.builder.checked_mul(length, word);
        self.check_calldata_range(data, byte_length);
    }

    pub(super) fn validate_calldata_bytes_argument(&mut self, value: ValueId, abi_type: &AbiType) {
        if self.is_external_abi_argument(value) {
            return;
        }
        if matches!(
            (self.builder.func().value_ty(value), abi_type),
            (
                Some(MirType::Slice(SliceLocation::Calldata)),
                AbiType::Bytes(SliceLocation::Calldata),
            )
        ) {
            self.validate_calldata_bytes_slice(value);
        }
    }

    pub(super) fn is_external_abi_argument(&self, value: ValueId) -> bool {
        self.builder.func().abi_args_lazy
            && self.builder.func().selector.is_some()
            && matches!(self.builder.func().value(value), Value::Arg(_))
    }

    pub(super) fn calldata_aggregate_requires_validation(&self, ty: Ty<'gcx>) -> bool {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => {
                self.calldata_aggregate_requires_validation(element)
            }
            TyKind::Slice(underlying) => {
                if matches!(
                    underlying.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
                ) {
                    false
                } else {
                    ty.base_type(self.context.gcx)
                        .is_some_and(|element| self.calldata_aggregate_requires_validation(element))
                }
            }
            TyKind::Struct(id) => self.context.gcx.hir.strukt(id).fields.iter().any(|&field| {
                self.calldata_aggregate_requires_validation(
                    self.context.gcx.type_of_item(field.into()),
                )
            }),
            TyKind::Tuple(fields) => {
                fields.iter().any(|&field| self.calldata_aggregate_requires_validation(field))
            }
            TyKind::Udvt(inner, _) => self.calldata_aggregate_requires_validation(inner),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => false,
            _ => !Self::calldata_word_is_full_width(ty),
        }
    }

    fn calldata_word_is_full_width(ty: Ty<'gcx>) -> bool {
        match types::TypeLowerer::mir_type(ty) {
            MirType::UInt(size) | MirType::Int(size) => size.bits() == 256,
            MirType::FixedBytes(size) => size.bytes() == 32,
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
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                    Some(self.materialize_memory_slice(value))
                }
                _ => report_unsupported(self.context.gcx, span, "memory slice materialization"),
            },
            SliceLocation::Returndata => {
                report_unsupported(self.context.gcx, span, "returndata slice materialization")
            }
        }
    }

    pub(super) fn lower_typed_expr(
        &mut self,
        expr: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<ValueId> {
        if let Some(value) = self.lower_fixed_bytes_literal(ty, expr) {
            return Some(value);
        }
        let source_expr = self.peel_bytes_conversion(expr);
        if let ExprKind::Lit(lit) = source_expr.peel_parens().kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
            && matches!(
                ty.peel_refs().kind,
                TyKind::StringLiteral(..)
                    | TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
            )
        {
            return self.lower_bytes_literal(bytes.as_byte_str(), expr.span);
        }
        if !ty.is_ref_at(DataLocation::Storage)
            && self.types.memory_layout(ty).is_some()
            && self
                .context
                .gcx
                .type_of_expr(source_expr.id)
                .is_some_and(|source| source.is_ref_at(DataLocation::Storage))
        {
            let access = self.storage_access(source_expr)?;
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
        value = self.coerce_call_argument(argument, parameter_ty, value);
        let mut abi_type = self.types.abi_type(parameter_ty)?;
        abi_type = self.abi_type_for_value(value, abi_type);
        self.validate_calldata_bytes_argument(value, &abi_type);
        let validated_static = self.validate_calldata_static_argument(value, parameter_ty);
        if self.needs_validated_calldata_materialization(value, &abi_type, parameter_ty)
            && !validated_static
        {
            value = self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
            abi_type = Self::memory_abi_type(abi_type);
        } else {
            value = self.canonicalize_abi_value(parameter_ty, value);
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
            AbiType::Function => AbiType::Function,
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
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
                ) =>
            {
                self.validate_calldata_bytes_slice(value);
                Some(self.materialize_memory_slice(value))
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.validate_calldata_bytes_slice(value);
                Some(self.materialize_memory_slice(value))
            }
            TyKind::DynArray(_) | TyKind::Slice(_) => {
                let element = self.array_element_type(ty).or_else(|| {
                    report_unsupported(self.context.gcx, span, "calldata argument materialization")
                })?;
                let element_type = self.types.abi_type(element)?;
                let length = self.builder.slice_len(value);
                let data = self.builder.slice_ptr(value);
                if matches!(element_type, AbiType::Word)
                    && Self::calldata_word_is_full_width(element)
                {
                    return Some(self.copy_calldata_word_array(data, length));
                }
                self.materialize_calldata_nested_array(element, data, length, span, true)
            }
            TyKind::Array(element, length) => {
                let length = u64::try_from(length).ok()?;
                let data = self.builder.slice_ptr(value);
                self.materialize_calldata_fixed_array(element, length, data, span, true)
            }
            TyKind::Struct(id) => {
                let fields = self.context.gcx.hir.strukt(id).fields.to_vec();
                let field_types = fields
                    .iter()
                    .map(|&field| self.context.gcx.type_of_item(field.into()))
                    .collect::<Vec<_>>();
                let base = self.builder.slice_ptr(value);
                self.materialize_calldata_fields(field_types, base, span, true)
            }
            _ => report_unsupported(self.context.gcx, span, "calldata argument materialization"),
        }
    }

    fn copy_calldata_word_array(&mut self, data: ValueId, length: ValueId) -> ValueId {
        let word = self.builder.imm_u64(32);
        let byte_length = self.builder.checked_mul(length, word);
        let size = self.builder.checked_add(word, byte_length);
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
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        let element_abi = self.types.abi_type(element)?;
        let element_is_dynamic = element_abi.is_dynamic();
        let element_head_size = self.builder.imm_u64(element_abi.head_size());
        let payload_size = self.builder.checked_mul(length, word);
        let size = self.builder.checked_add(word, payload_size);
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
        let offset = self.builder.checked_mul(index, element_head_size);
        let head = self.builder.add(data, offset);
        let value = self.materialize_calldata_value_at_inner(
            element,
            head,
            data,
            span,
            validate_bounds && element_is_dynamic,
        )?;
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
        let base_ty = ty.peel_refs();
        if let TyKind::Array(element, length) = base_ty.kind
            && self.types.abi_type(base_ty)?.is_dynamic()
        {
            let value_pos =
                self.calldata_value_position(base_ty, head, tuple_base, validate_bounds)?;
            let length = u64::try_from(length).ok()?;
            if validate_bounds {
                let element_head_size = self.types.abi_type(element)?.head_size();
                let size = self.builder.imm_u64(element_head_size.checked_mul(length)?);
                self.check_calldata_tail_range(value_pos, size);
            }
            let length = self.builder.imm_u64(length);
            return Some(self.builder.make_slice(value_pos, length, SliceLocation::Calldata));
        }
        let decode_ty = if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                base_ty.kind,
                TyKind::Array(..)
                    | TyKind::Struct(_)
                    | TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
            ) {
            ty
        } else {
            base_ty
        };
        self.materialize_calldata_value_at_inner(decode_ty, head, tuple_base, span, validate_bounds)
    }

    pub(super) fn materialize_calldata_value_at_inner(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let is_calldata = ty.is_ref_at(DataLocation::Calldata);
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
        let value_pos = self.calldata_value_position(ty, head, tuple_base, validate_bounds)?;
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) if is_calldata => {
                let length = self.calldata_load_word(value_pos);
                if validate_bounds {
                    let byte_stride = self.builder.imm_u64(1);
                    self.validate_calldata_dynamic_tail(value_pos, length, byte_stride);
                }
                let data = self.builder.add(value_pos, word);
                Some(self.builder.make_slice(data, length, SliceLocation::Calldata))
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(self.materialize_calldata_bytes_at(value_pos, validate_bounds))
            }
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                if is_calldata {
                    let length = self.calldata_load_word(value_pos);
                    if validate_bounds {
                        let element_head_size =
                            self.builder.imm_u64(self.types.abi_type(element)?.head_size());
                        self.validate_calldata_dynamic_tail(value_pos, length, element_head_size);
                    }
                    let data = self.builder.add(value_pos, word);
                    return Some(self.builder.make_slice(data, length, SliceLocation::Calldata));
                }
                let length = self.calldata_load_word(value_pos);
                let element_type = self.types.abi_type(element)?;
                if validate_bounds {
                    let element_head_size = self.builder.imm_u64(element_type.head_size());
                    self.validate_calldata_dynamic_tail(value_pos, length, element_head_size);
                }
                let data = self.builder.add(value_pos, word);
                if matches!(element_type, AbiType::Word)
                    && Self::calldata_word_is_full_width(element)
                {
                    Some(self.copy_calldata_word_array(data, length))
                } else {
                    self.materialize_calldata_nested_array(
                        element,
                        data,
                        length,
                        span,
                        validate_bounds,
                    )
                }
            }
            TyKind::Array(element, length) => {
                let length = u64::try_from(length).ok()?;
                if is_calldata {
                    let element_head_size = self.types.abi_type(element)?.head_size();
                    let head_size = length.checked_mul(element_head_size)?;
                    if validate_bounds {
                        let head_size = self.builder.imm_u64(head_size);
                        self.check_calldata_tail_range(value_pos, head_size);
                    }
                    let length = self.builder.imm_u64(length);
                    return Some(self.builder.make_slice(
                        value_pos,
                        length,
                        SliceLocation::Calldata,
                    ));
                }
                self.materialize_calldata_fixed_array(
                    element,
                    length,
                    value_pos,
                    span,
                    validate_bounds,
                )
            }
            TyKind::Struct(id) => {
                if is_calldata {
                    let head_size = self.builder.imm_u64(self.types.abi_type(ty)?.head_size());
                    if validate_bounds {
                        self.check_calldata_tail_range(value_pos, head_size);
                    }
                    return Some(self.builder.make_slice(
                        value_pos,
                        head_size,
                        SliceLocation::Calldata,
                    ));
                }
                let fields = self.context.gcx.hir.strukt(id).fields.to_vec();
                let field_types = fields
                    .iter()
                    .map(|&field| self.context.gcx.type_of_item(field.into()))
                    .collect::<Vec<_>>();
                self.materialize_calldata_fields(field_types, value_pos, span, validate_bounds)
            }
            TyKind::Tuple(fields) => self.materialize_calldata_fields(
                fields.iter().copied(),
                value_pos,
                span,
                validate_bounds,
            ),
            TyKind::Fn(function) if function.is_external() => {
                Some(self.decode_calldata_function_pointer_with_bounds(value_pos, validate_bounds))
            }
            _ => Some(self.decode_calldata_word(ty, value_pos, validate_bounds)),
        }
    }

    fn calldata_value_position(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let word = self.builder.imm_u64(32);
        if !self.types.abi_type(ty)?.is_dynamic() {
            return Some(head);
        }
        if validate_bounds {
            self.check_calldata_range(head, word);
        }
        let offset = self.calldata_load_word(head);
        let value_pos = self.builder.add(tuple_base, offset);
        if validate_bounds {
            // Solidity's calldata tail helper uses a signed offset bound. Negative ABI
            // offsets may therefore wrap to a valid EVM calldata load, whose missing bytes
            // read as zero; the tail checks below decide whether the resulting value is valid.
            let calldata_size = self.builder.calldatasize();
            let thirty_one = self.builder.imm_u64(31);
            let available = self.builder.sub(calldata_size, tuple_base);
            let bound = self.builder.sub(available, thirty_one);
            let valid = self.builder.slt(offset, bound);
            let invalid = self.builder.iszero(valid);
            self.revert_if_calldata_invalid(invalid);
        }
        Some(value_pos)
    }

    fn check_calldata_range(&mut self, start: ValueId, size: ValueId) {
        let end = self.builder.add(start, size);
        let overflow = self.builder.lt(end, start);
        let calldata_size = self.builder.calldatasize();
        let out_of_bounds = self.builder.gt(end, calldata_size);
        let invalid = self.builder.or(overflow, out_of_bounds);
        self.revert_if_calldata_invalid(invalid);
    }

    fn check_calldata_tail_range(&mut self, start: ValueId, size: ValueId) {
        let end = self.builder.add(start, size);
        let calldata_size = self.builder.calldatasize();
        let out_of_bounds = self.builder.gt(end, calldata_size);
        self.revert_if_calldata_invalid(out_of_bounds);
    }

    fn validate_calldata_dynamic_tail(
        &mut self,
        value_pos: ValueId,
        length: ValueId,
        stride: ValueId,
    ) {
        let max_length = self.builder.imm_u64(u64::MAX);
        let too_large = self.builder.gt(length, max_length);
        self.revert_if_calldata_invalid(too_large);

        let size = self.builder.mul(length, stride);
        let word = self.builder.imm_u64(32);
        let data = self.builder.add(value_pos, word);
        let calldata_size = self.builder.calldatasize();
        let limit = self.builder.sub(calldata_size, size);
        let out_of_bounds = self.builder.sgt(data, limit);
        self.revert_if_calldata_invalid(out_of_bounds);
    }

    pub(super) fn revert_if_calldata_invalid(&mut self, condition: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(condition, revert, continue_block);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u64(0);
        self.builder.revert(zero, zero);
        self.builder.switch_to_block(continue_block);
    }

    pub(super) fn calldata_load_word(&mut self, pointer: ValueId) -> ValueId {
        self.builder.calldataload(pointer)
    }

    fn decode_calldata_function_pointer_with_bounds(
        &mut self,
        position: ValueId,
        validate_bounds: bool,
    ) -> ValueId {
        let word = self.builder.imm_u64(32);
        if validate_bounds {
            self.check_calldata_range(position, word);
        }
        let value = self.calldata_load_word(position);
        let valid = AbiWordValidator::from_mir_type(MirType::Function)
            .expect("function words always validate")
            .condition(
                &mut self.builder,
                value,
                self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
            );
        let invalid = self.builder.iszero(valid);
        self.revert_if_calldata_invalid(invalid);
        let shift = self.builder.imm_u64(64);
        self.builder.shr(shift, value)
    }

    fn decode_calldata_word(
        &mut self,
        ty: Ty<'gcx>,
        position: ValueId,
        validate_bounds: bool,
    ) -> ValueId {
        if let TyKind::Fn(function) = ty.kind
            && function.is_external()
        {
            return self.decode_calldata_function_pointer_with_bounds(position, validate_bounds);
        }

        let word = self.builder.imm_u64(32);
        if validate_bounds {
            self.check_calldata_range(position, word);
        }
        let value = self.calldata_load_word(position);
        let validator = match ty.kind {
            TyKind::Enum(id) => {
                let variants = self.context.gcx.hir.enumm(id).variants.len() as u64;
                AbiWordValidator::EnumRange(variants)
            }
            _ => match AbiWordValidator::from_mir_type(types::TypeLowerer::mir_type(ty)) {
                Some(validator) => validator,
                None => return value,
            },
        };
        let valid = validator.condition(
            &mut self.builder,
            value,
            self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
        );
        let invalid = self.builder.iszero(valid);
        self.revert_if_calldata_invalid(invalid);
        value
    }

    fn materialize_calldata_bytes_at(
        &mut self,
        position: ValueId,
        validate_bounds: bool,
    ) -> ValueId {
        let word = self.builder.imm_u64(32);
        let length = self.calldata_load_word(position);
        if validate_bounds {
            let byte_stride = self.builder.imm_u64(1);
            self.validate_calldata_dynamic_tail(position, length, byte_stride);
        }
        let data = self.builder.add(position, word);
        let slice = self.builder.make_slice(data, length, SliceLocation::Calldata);
        self.materialize_memory_slice(slice)
    }

    fn validate_calldata_bytes_slice(&mut self, slice: ValueId) {
        let pointer = self.builder.slice_ptr(slice);
        let length = self.builder.slice_len(slice);
        let max_length = self.builder.imm_u64(u64::MAX);
        let too_large = self.builder.gt(length, max_length);
        self.revert_if_calldata_invalid(too_large);
        let calldata_size = self.builder.calldatasize();
        let limit = self.builder.sub(calldata_size, length);
        let out_of_bounds = self.builder.sgt(pointer, limit);
        self.revert_if_calldata_invalid(out_of_bounds);
    }

    fn materialize_calldata_fixed_array(
        &mut self,
        element: Ty<'gcx>,
        length: u64,
        base: ValueId,
        span: Span,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let element_abi = self.types.abi_type(element)?;
        let element_head_size = element_abi.head_size();
        if validate_bounds {
            let head_size = length.checked_mul(element_head_size)?;
            let head_size = self.builder.imm_u64(head_size);
            self.check_calldata_tail_range(base, head_size);
        }
        let size = self.builder.imm_u64(length.checked_mul(32)?);
        let layout = MemoryObjectLayout::FixedArray { len: length, element_words: 1 };
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        for index in 0..length {
            let index_value = self.builder.imm_u64(index);
            let head_offset = self.builder.imm_u64(index.checked_mul(element_head_size)?);
            let head = self.builder.add(base, head_offset);
            let value = self.materialize_calldata_value_at_inner(
                element,
                head,
                base,
                span,
                validate_bounds && element_abi.is_dynamic(),
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
        let mut head_size = 0_u64;
        let all_static = fields.iter().all(|&field| {
            let Some(abi) = self.types.abi_type(field) else { return false };
            head_size = head_size.saturating_add(abi.head_size());
            !abi.is_dynamic()
        });
        let nested_validate = if validate_bounds && all_static {
            let head_size = self.builder.imm_u64(head_size);
            self.check_calldata_tail_range(base, head_size);
            false
        } else {
            validate_bounds
        };
        let layout = MemoryObjectLayout::Struct { fields: fields.len() as u64 };
        let size = self.builder.imm_u64(fields.len().checked_mul(32)? as u64);
        let object = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        let mut offset = 0u64;
        for (index, field) in fields.iter().copied().enumerate() {
            let field_offset = self.builder.imm_u64(offset);
            let head = self.builder.add(base, field_offset);
            let value =
                self.materialize_calldata_value_at_inner(field, head, base, span, nested_validate)?;
            self.builder.memory_object_store_field(object, layout, index as u64, value);
            offset = offset.checked_add(self.types.abi_type(field)?.head_size())?;
        }
        Some(object)
    }
}
