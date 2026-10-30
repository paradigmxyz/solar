//! ABI call argument and calldata materialization helpers.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn is_storage_parameter(ty: Ty<'gcx>) -> bool {
        ty.encodes_as_slot()
    }

    /// Whether `abi_type` is a calldata array of words, which an encoding copies from where it
    /// lies once [`Self::validate_calldata_word_array`] validated its elements.
    pub(super) fn is_calldata_word_array(abi_type: &AbiType) -> bool {
        matches!(
            abi_type,
            AbiType::DynamicArray { element, location: PtrLocation::Calldata }
                if matches!(element.as_ref(), AbiType::Word(_))
        )
    }

    /// Validates the elements of the calldata array of words `value` where they lie, before an
    /// encoding copies them: solc's IR pipeline validates each element it encodes, reading past
    /// the end of the calldata as zeros, and never checks the array's range.
    pub(super) fn validate_calldata_word_array(&mut self, value: ValueId, ty: Ty<'gcx>) {
        let Some(element) = self.array_element_type(ty) else { return };
        // for index in 0..slice_len(value):
        //   validate(element, slice_ptr(value) + index * 32)
        let length = self.builder.slice_len(value);
        let base = self.builder.slice_ptr(value);
        let stride = self.builder.imm(32);
        self.counted_loop(length, |this, index| {
            let offset = this.builder.mul(index, stride);
            let position = this.builder.add(base, offset);
            this.validate_calldata_static_value(element, position);
        });
    }

    /// Whether the calldata aggregate `value` has words to validate before an encoding reads
    /// them. Assembly can set a calldata variable to any range, whose words are validated as
    /// they are read like any others.
    pub(super) fn needs_calldata_aggregate_validation(&self, value: ValueId, ty: Ty<'gcx>) -> bool {
        self.builder.func().value_slice_location(value) == Some(PtrLocation::Calldata)
            && self.calldata_aggregate_requires_validation(ty)
    }

    /// Validates a static calldata aggregate in place so ABI encoding can read
    /// its words directly without copying the aggregate to memory.
    pub(super) fn validate_calldata_static_argument(&mut self, value: ValueId, ty: Ty<'gcx>) {
        let needs_validation = self.needs_calldata_aggregate_validation(value, ty);
        self.validate_calldata_static_argument_inner(value, ty, needs_validation);
    }

    fn validate_calldata_static_argument_inner(
        &mut self,
        value: ValueId,
        ty: Ty<'gcx>,
        needs_validation: bool,
    ) -> bool {
        // base = extract_value value, 0
        // validate_static(ty, base)
        if self.is_decoded_external_argument(value) || !needs_validation {
            return false;
        }
        let Some(abi_type) = self.types.abi_type(ty) else { return false };
        if abi_type.is_dynamic() {
            return false;
        }

        let base = self.builder.slice_ptr(value);
        self.validate_calldata_static_value(ty, base);
        true
    }

    /// Validates static calldata words in place, retaining zero-padding beyond the input.
    pub(super) fn validate_calldata_static_value(&mut self, ty: Ty<'gcx>, base: ValueId) {
        // validate_static(ty, base)
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
                let length = self.builder.imm(length);
                let element_size = self.builder.imm(element_size);
                self.counted_loop(length, |this, index| {
                    let offset = this.builder.mul(index, element_size);
                    let position = this.builder.add(base, offset);
                    this.validate_calldata_static_value(element, position);
                });
            }
            TyKind::Struct(id) => {
                let gcx = self.cx.gcx;
                let fields = gcx.hir.strukt(id).fields;
                self.validate_calldata_static_fields(
                    fields.iter().map(move |&field| gcx.type_of_item(field.into())),
                    base,
                );
            }
            TyKind::Tuple(fields) => {
                self.validate_calldata_static_fields(fields.iter().copied(), base)
            }
            _ => {
                if !Self::calldata_word_is_full_width(ty) {
                    let _ = self.decode_calldata_word(ty, base, false);
                }
            }
        }
    }

    fn validate_calldata_static_fields(
        &mut self,
        fields: impl IntoIterator<Item = Ty<'gcx>>,
        base: ValueId,
    ) {
        let mut offset = 0u64;
        for field_ty in fields {
            let position = self.builder.add_offset(base, offset);
            self.validate_calldata_static_value(field_ty, position);
            let Some(field_size) = self.types.abi_type(field_ty).map(|ty| ty.head_size()) else {
                return;
            };
            offset = offset.saturating_add(field_size);
        }
    }

    /// Panics when the element bytes of the calldata array `value` do not fit in a word, before an
    /// encoding copies them from where they lie. Only an array whose length assembly set can get
    /// there; its range is not otherwise checked against the calldata, as solc does not check it.
    pub(super) fn check_calldata_array_size(
        &mut self,
        value: ValueId,
        ty: Ty<'gcx>,
        abi_type: &AbiType,
    ) {
        if self.is_decoded_external_argument(value) {
            return;
        }
        if self.builder.func().value_slice_location(value) != Some(PtrLocation::Calldata) {
            return;
        }
        let AbiType::DynamicArray { element, location: PtrLocation::Calldata } = abi_type else {
            return;
        };
        // A slice of words is not checked: its copy reads from where it lies. A slice of
        // composite elements is encoded one element at a time, from a byte count that must not
        // wrap.
        let checked = match ty.peel_refs().kind {
            TyKind::DynArray(_) => true,
            TyKind::Slice(_) => !matches!(element.as_ref(), AbiType::Word(_)),
            _ => false,
        };
        if !checked {
            return;
        }
        // bytes = checked(length * element_head_size)
        let word = self.builder.imm(element.head_size());
        let length = self.builder.slice_len(value);
        self.builder.checked_mul(length, word);
    }

    pub(super) fn validate_calldata_bytes_argument(&mut self, value: ValueId, abi_type: &AbiType) {
        if self.is_decoded_external_argument(value) {
            return;
        }
        if self.builder.func().value_slice_location(value) == Some(PtrLocation::Calldata)
            && matches!(abi_type, AbiType::Bytes(PtrLocation::Calldata))
        {
            self.validate_calldata_bytes_slice(value);
        }
    }

    pub(super) fn is_external_abi_argument(&self, value: ValueId) -> bool {
        (self.builder.func().selector.is_some() || self.builder.func().attributes.is_fallback)
            && matches!(self.builder.func().value(value), Value::Arg(_))
    }

    /// Whether `value` is an external argument checked by ABI decoding, or the complete calldata
    /// passed to a fallback. A public function also takes its arguments from internal calls,
    /// which can pass a calldata slice that assembly set.
    pub(super) fn is_decoded_external_argument(&self, value: ValueId) -> bool {
        self.is_external_abi_argument(value)
            && self.builder.func().attributes.visibility == solar_ast::Visibility::External
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
                    ty.base_type(self.cx.gcx)
                        .is_some_and(|element| self.calldata_aggregate_requires_validation(element))
                }
            }
            TyKind::Struct(id) => self.cx.gcx.hir.strukt(id).fields.iter().any(|&field| {
                self.calldata_aggregate_requires_validation(self.cx.gcx.type_of_item(field.into()))
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
        types::TypeLowerer::value_layout(ty).is_full_abi_word()
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
            PtrLocation::Calldata => self.materialize_calldata_argument(ty, value, span),
            PtrLocation::Memory => match ty.peel_refs().kind {
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                    Some(self.materialize_memory_slice(value))
                }
                _ => self.cx.report_unsupported(span, "memory slice materialization"),
            },
            location => {
                self.cx.report_unsupported(span, &format!("{location} slice materialization"))
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
            return self.lower_shared_bytes_literal(*bytes);
        }
        if !ty.is_ref_at(DataLocation::Storage)
            && self.types.memory_layout(ty).is_some()
            && self
                .cx
                .gcx
                .type_of_expr(source_expr.id)
                .is_some_and(|source| source.is_ref_at(DataLocation::Storage))
        {
            let access = self.require_storage_access(source_expr)?;
            return self.load_storage_object(ty, access.slot, expr.span);
        }
        let value = self.lower_expr(expr)?;
        let source_ty = self.cx.gcx.type_of_expr(expr.id).or_else(|| {
            let ExprKind::Lit(lit) = &expr.kind else { return None };
            let LitKind::Str(_, bytes, _) = &lit.kind else { return None };
            Some(self.cx.gcx.mk_ty_string_literal(bytes.as_byte_str()))
        });
        let value = source_ty.map_or(value, |source_ty| self.coerce_value(value, source_ty, ty));
        Some(value)
    }

    pub(super) fn lower_abi_call_argument(
        &mut self,
        argument: &hir::Expr<'_>,
        parameter_ty: Ty<'gcx>,
    ) -> Option<(ValueId, AbiType)> {
        let value = self.lower_typed_expr(argument, parameter_ty)?;
        let abi_type = self.types.abi_type(parameter_ty)?;
        let abi_type = if self.cx.gcx.type_of_expr(argument.id).is_some_and(|ty| {
            ty.is_ref_at(DataLocation::Memory) || ty.is_ref_at(DataLocation::Storage)
        }) {
            Self::memory_abi_type(abi_type)
        } else {
            self.abi_type_for_value(value, abi_type)
        };
        self.prepare_abi_argument(argument, parameter_ty, value, abi_type)
    }

    pub(super) fn prepare_abi_encode_argument(
        &mut self,
        argument: &hir::Expr<'_>,
        ty: Ty<'gcx>,
        value: ValueId,
        abi_type: AbiType,
    ) -> Option<(ValueId, AbiType)> {
        let abi_type = self.abi_type_for_value(value, abi_type);
        self.prepare_abi_argument(argument, ty, value, abi_type)
    }

    pub(super) fn prepare_abi_argument(
        &mut self,
        argument: &hir::Expr<'_>,
        ty: Ty<'gcx>,
        mut value: ValueId,
        mut abi_type: AbiType,
    ) -> Option<(ValueId, AbiType)> {
        self.check_calldata_array_size(value, ty, &abi_type);
        let needs_validation = self.needs_calldata_aggregate_validation(value, ty);
        let validated_static =
            self.validate_calldata_static_argument_inner(value, ty, needs_validation);
        let word_array = needs_validation && Self::is_calldata_word_array(&abi_type);
        // The dispatcher validates the elements of an argument that only ABI decoding passes
        // when an encoding takes it.
        if word_array && !self.is_decoded_external_argument(value) {
            self.validate_calldata_word_array(value, ty);
        }
        // The encoder validates a dynamic calldata aggregate while it encodes it from where it
        // lies.
        let needs_materialization = needs_validation && !word_array && !abi_type.is_dynamic();
        if needs_materialization && !validated_static {
            value = self.materialize_calldata_argument(ty, value, argument.span)?;
            abi_type = Self::memory_abi_type(abi_type);
        } else {
            value = self.canonicalize_abi_value(ty, value);
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
            && self.builder.func().value_slice_location(value) == Some(PtrLocation::Calldata)
        {
            Some(value)
        } else {
            let value = self.materialize_memory_argument(ty, value, span)?;
            if self.builder.func().value_slice_location(value).is_none() {
                // argument = cast value to the declared parameter type
                Some(self.builder.cast(value, types::TypeLowerer::mir_type(ty)))
            } else {
                Some(value)
            }
        }
    }

    pub(super) fn memory_abi_type(ty: AbiType) -> AbiType {
        ty.at_location(PtrLocation::Memory)
    }

    pub(super) fn abi_type_for_value(&self, value: ValueId, ty: AbiType) -> AbiType {
        if let Some(location) = self.builder.func().value_slice_location(value) {
            match ty {
                AbiType::Bytes(_) | AbiType::DynamicArray { .. } => ty.at_location(location),
                ty => ty,
            }
        } else if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::MEMPTR) | Some(MirType::Slice(PtrLocation::Memory))
        ) || (matches!(ty, AbiType::Bytes(_) | AbiType::DynamicArray { .. })
            && self.builder.func().value_u64(value) == Some(EvmMemoryLayout::ZERO_SLOT))
            || self.builder.func().memory_object_load(value)
        {
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
            _ if self.is_dynamic_bytes_type(ty.peel_refs()) => {
                // if data is dirty {
                //     object = bytes(data.len)
                //     validate_calldata_bytes(data)
                //     copy(data, object.data)
                // } else {
                //     validate_calldata_bytes(data)
                //     object = materialize_calldata_bytes(data)
                // }
                if self.dirty_values.contains(&value) {
                    let length = self.builder.slice_len(value);
                    let object =
                        self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
                    self.validate_calldata_bytes_slice(value);
                    self.builder.memory_copy_from_slice(object, value);
                    Some(object)
                } else if self.is_decoded_external_argument(value) {
                    // ABI decoding already checked the range, and a length within
                    // calldata cannot overflow its padded allocation size.
                    // object = bytes(data.len) !unchecked size
                    // copy(data, object.data)
                    let length = self.builder.slice_len(value);
                    let object = self
                        .builder
                        .alloc_bounded_bytes_object(length, AllocationSemantics::INTERNAL);
                    self.builder.memory_copy_from_slice(object, value);
                    Some(object)
                } else {
                    self.validate_calldata_bytes_slice(value);
                    Some(self.materialize_memory_slice(value))
                }
            }
            TyKind::DynArray(_) | TyKind::Slice(_) => {
                // object = materialize_calldata_array(data)
                let element = self.array_element_type(ty).or_else(|| {
                    self.cx.report_unsupported(span, "calldata argument materialization")
                })?;
                let element_type = self.types.abi_type(element)?;
                let length = self.builder.slice_len(value);
                let data = self.builder.slice_ptr(value);
                let element_head_size = self.builder.imm(element_type.head_size());
                let head_size = self.builder.checked_mul(length, element_head_size);
                self.check_calldata_range(data, head_size);
                if matches!(element_type, AbiType::Word(_))
                    && Self::calldata_word_is_full_width(element)
                {
                    return Some(self.copy_calldata_word_array(data, length));
                }
                self.materialize_calldata_nested_array(element, data, length, span, true)
            }
            TyKind::Array(element, length) => {
                // object = materialize_calldata_fixed_array(data)
                let length = u64::try_from(length).ok()?;
                let data = self.builder.slice_ptr(value);
                self.materialize_calldata_fixed_array(element, length, data, span, true)
            }
            TyKind::Struct(id) => {
                // object = materialize_calldata_struct(data)
                let gcx = self.cx.gcx;
                let field_types =
                    gcx.hir.strukt(id).fields.iter().map(|&field| gcx.type_of_item(field.into()));
                let base = self.builder.slice_ptr(value);
                self.materialize_calldata_fields(field_types, base, span, true)
            }
            _ => self.cx.report_unsupported(span, "calldata argument materialization"),
        }
    }

    fn copy_calldata_word_array(&mut self, data: ValueId, length: ValueId) -> ValueId {
        // object = word_array(length)
        // copy(data, object.data, length * 32)
        let word = self.builder.imm(32);
        let byte_length = self.builder.checked_mul(length, word);
        let size = self.builder.checked_add(word, byte_length);
        let object = self.builder.alloc(size, AllocationSemantics::INTERNAL);
        self.builder.set_memory_len(object, length);
        let source = self.builder.make_slice(data, byte_length, PtrLocation::Calldata);
        self.builder.memory_copy_from_slice(object, source);
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
        // for i in 0..length { object[i] = decode(data + i * head_size) }
        let word = self.builder.imm(32);
        let element_abi = self.types.abi_type(element)?;
        let element_is_dynamic = element_abi.is_dynamic();
        let element_head_size = self.builder.imm(element_abi.head_size());
        let payload_size = self.builder.checked_mul(length, word);
        let size = self.builder.checked_add(word, payload_size);
        let layout = MemoryObjectLayout::WORD_ARRAY;
        let object = self.builder.alloc(size, AllocationSemantics::INTERNAL);
        self.builder.set_memory_len(object, length);

        self.counted_loop(length, |this, index| {
            // The checked payload size bounds offsets for one-word ABI heads.
            // offset = index * element_head_size
            let offset = if element_abi.head_size() == 32 {
                this.builder.mul(index, element_head_size)
            } else {
                this.builder.checked_mul(index, element_head_size)
            };
            let head = this.builder.add(data, offset);
            let value = this.materialize_calldata_value_at_inner(
                element,
                head,
                data,
                span,
                validate_bounds && element_is_dynamic,
            )?;
            let value = this.encode_memory_scalar(element, value);
            this.builder.memory_store_element(object, layout, index, value);
            Some(())
        })?;
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
        if let TyKind::Array(_, length) = base_ty.kind
            && self.types.abi_type(base_ty)?.is_dynamic()
        {
            let value_pos =
                self.calldata_value_position(base_ty, head, tuple_base, validate_bounds)?;
            let length = self.builder.imm(u64::try_from(length).ok()?);
            return Some(self.builder.make_slice(value_pos, length, PtrLocation::Calldata));
        }
        let decode_ty = if ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                base_ty.kind,
                TyKind::Array(..)
                    | TyKind::DynArray(_)
                    | TyKind::Slice(_)
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
        // position = calldata_value_position(ty, head, tuple_base)
        let value_pos = self.calldata_value_position(ty, head, tuple_base, validate_bounds)?;
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) if is_calldata => {
                // value = calldata_slice(position + 32, calldataload(position))
                let length = self.builder.load(value_pos, PtrLocation::Calldata);
                let data = self.calldata_tail_data(value_pos, length, validate_bounds.then_some(1));
                Some(self.builder.make_slice(data, length, PtrLocation::Calldata))
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                // value = materialize_calldata_bytes(position)
                Some(self.materialize_calldata_bytes_at(value_pos, validate_bounds))
            }
            TyKind::DynArray(element) | TyKind::Slice(element) => {
                let length = self.builder.load(value_pos, PtrLocation::Calldata);
                let element_type = self.types.abi_type(element)?;
                let stride = validate_bounds.then(|| element_type.head_size());
                let data = self.calldata_tail_data(value_pos, length, stride);
                if is_calldata {
                    // value = calldata_slice(position + 32, calldataload(position))
                    return Some(self.builder.make_slice(data, length, PtrLocation::Calldata));
                }
                // value = materialize_calldata_array(position)
                if matches!(element_type, AbiType::Word(_))
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
                    // value = calldata_slice(position, length)
                    let length = self.builder.imm(length);
                    return Some(self.builder.make_slice(value_pos, length, PtrLocation::Calldata));
                }
                // value = materialize_calldata_fixed_array(position)
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
                    // The tail offset check above already covers the whole head of an
                    // in-range struct; a wrapped pointer keeps solc's lazy semantics, where
                    // each selected field validates its own path and missing bytes read as
                    // zero.
                    let head_size = self.builder.imm(self.types.abi_type(ty)?.head_size());
                    return Some(self.builder.make_slice(
                        value_pos,
                        head_size,
                        PtrLocation::Calldata,
                    ));
                }
                // value = materialize_calldata_struct(position)
                let gcx = self.cx.gcx;
                let field_types =
                    gcx.hir.strukt(id).fields.iter().map(|&field| gcx.type_of_item(field.into()));
                self.materialize_calldata_fields(field_types, value_pos, span, validate_bounds)
            }
            TyKind::Tuple(fields) => {
                // value = materialize_calldata_tuple(position)
                self.materialize_calldata_fields(
                    fields.iter().copied(),
                    value_pos,
                    span,
                    validate_bounds,
                )
            }
            _ => {
                // value = decode_calldata_word(position)
                Some(self.decode_calldata_word(ty, value_pos, validate_bounds))
            }
        }
    }

    fn calldata_value_position(
        &mut self,
        ty: Ty<'gcx>,
        head: ValueId,
        tuple_base: ValueId,
        validate_bounds: bool,
    ) -> Option<ValueId> {
        let word = self.builder.imm(32);
        let abi_type = self.types.abi_type(ty)?;
        if !abi_type.is_dynamic() {
            // position = head
            return Some(head);
        }
        if validate_bounds {
            self.check_calldata_range(head, word);
        }
        // offset = calldataload(head)
        // position = tuple_base + offset
        let offset = self.builder.load(head, PtrLocation::Calldata);
        let value_pos = self.builder.add(tuple_base, offset);
        if validate_bounds {
            // check_calldata_tail_offset(tuple_base, offset)
            let calldata_size = self.builder.calldatasize();
            self.builder.check_calldata_tail_offset(
                CalldataTail::Access,
                tuple_base,
                offset,
                calldata_size,
                abi_type.tail_size(),
            );
        }
        Some(value_pos)
    }

    /// Reverts unless `size` bytes at `start` lie inside calldata.
    ///
    /// The bound is `slt(sub(calldatasize(), start), size)`: a pointer that wrapped through an
    /// accepted negative tail offset compares as in range and reads zero-filled calldata.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/YulUtilFunctions.cpp#L2550-L2557>
    fn check_calldata_range(&mut self, start: ValueId, size: ValueId) {
        let calldata_size = self.builder.calldatasize();
        let remaining = self.builder.sub(calldata_size, start);
        let invalid = self.builder.slt(remaining, size);
        self.builder.revert_if(invalid, RevertReason::CalldataTailTooShort);
    }

    /// Returns the data address of the dynamically sized value at `value_pos`, checking that
    /// `length` elements of `stride` bytes lie inside calldata when `stride` is given.
    fn calldata_tail_data(
        &mut self,
        value_pos: ValueId,
        length: ValueId,
        stride: Option<u64>,
    ) -> ValueId {
        let Some(stride) = stride else { return self.builder.add_offset(value_pos, 32) };
        // calldata_tail_data(value_pos + 32, length, stride)
        let calldata_size = self.builder.calldatasize();
        self.builder.calldata_tail_data(
            CalldataTail::Access,
            value_pos,
            32,
            length,
            calldata_size,
            stride,
        )
    }

    fn decode_calldata_word(
        &mut self,
        ty: Ty<'gcx>,
        position: ValueId,
        validate_bounds: bool,
    ) -> ValueId {
        let is_external_function =
            matches!(ty.kind, TyKind::Fn(function) if function.is_external());
        let word = self.builder.imm(32);
        if validate_bounds {
            self.check_calldata_range(position, word);
        }
        // value = calldataload(position)
        let value = self.builder.load(position, PtrLocation::Calldata);
        let validator = if is_external_function {
            AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)
                .expect("function words always validate")
        } else {
            match ty.kind {
                TyKind::Enum(id) => {
                    let variants = self.cx.gcx.hir.enumm(id).variants.len() as u64;
                    AbiWordValidator::EnumRange(variants)
                }
                _ => match AbiWordValidator::from_layout(types::TypeLowerer::value_layout(ty)) {
                    Some(validator) => validator,
                    None => return value,
                },
            }
        };
        // if !valid(value) { revert(0, 0) }
        let valid = validator.condition(&mut self.builder, value);
        let invalid = self.builder.eq_zero(valid);
        self.builder.revert_if(invalid, RevertReason::Empty);
        if is_external_function {
            // value = value >> 64
            let shift = self.builder.imm(64);
            self.builder.shr(value, shift)
        } else {
            value
        }
    }

    fn materialize_calldata_bytes_at(
        &mut self,
        position: ValueId,
        validate_bounds: bool,
    ) -> ValueId {
        // len = calldataload(position)
        // data = calldata_tail_data(position + 32, len, 1)
        // slice = calldata(data, len)
        // return materialize_memory_slice(slice)
        let length = self.builder.load(position, PtrLocation::Calldata);
        let data = self.calldata_tail_data(position, length, validate_bounds.then_some(1));
        let slice = self.builder.make_slice(data, length, PtrLocation::Calldata);
        self.materialize_memory_slice(slice)
    }

    fn validate_calldata_bytes_slice(&mut self, slice: ValueId) {
        // ABI decoding already checked an external argument's range.
        if self.is_decoded_external_argument(slice) {
            return;
        }
        // calldata_tail_data(pointer, length, 1)
        let pointer = self.builder.slice_ptr(slice);
        let length = self.builder.slice_len(slice);
        let calldata_size = self.builder.calldatasize();
        self.builder.calldata_tail_data(CalldataTail::Access, pointer, 0, length, calldata_size, 1);
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
            // check_range(base, length * element_head_size)
            let head_size = self.builder.imm(length.checked_mul(element_head_size)?);
            self.check_calldata_range(base, head_size);
        }
        // object = fixed_array(length)
        let byte_length = length.checked_mul(32)?;
        let (object, layout) = self.builder.alloc_word_array(length, AllocationSemantics::INTERNAL);
        if matches!(element_abi, AbiType::Word(_)) {
            if !Self::calldata_word_is_full_width(element) {
                let length = self.builder.imm(length);
                let word = self.builder.imm(32);
                self.counted_loop(length, |this, index| {
                    let offset = this.builder.mul(index, word);
                    let position = this.builder.add(base, offset);
                    this.validate_calldata_static_value(element, position);
                });
            }
            let byte_length = self.builder.imm(byte_length);
            // copy(calldata(base, byte_length), object)
            let source = self.builder.make_slice(base, byte_length, PtrLocation::Calldata);
            self.builder.memory_copy_to_object(object, source);
            return Some(object);
        }
        let nested_validate = validate_bounds && element_abi.is_dynamic();
        let length = self.builder.imm(length);
        let element_head_size = self.builder.imm(element_head_size);
        // for i in 0..length {
        //     head = base + i * element_head_size
        //     object[i] = decode_at(element, head, base)
        // }
        self.counted_loop(length, |this, index| {
            let head_offset = this.builder.mul(index, element_head_size);
            let head = this.builder.add(base, head_offset);
            let value = this.materialize_calldata_value_at_inner(
                element,
                head,
                base,
                span,
                nested_validate,
            )?;
            let value = this.encode_memory_scalar(element, value);
            this.builder.memory_store_element(object, layout, index, value);
            Some(())
        })?;
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
            // check_range(base, head_size)
            let head_size = self.builder.imm(head_size);
            self.check_calldata_range(base, head_size);
            false
        } else {
            validate_bounds
        };
        // object = struct(fields.length)
        let size = self.builder.imm(fields.len().checked_mul(32)? as u64);
        let object = self.builder.alloc(size, AllocationSemantics::INTERNAL);
        let mut offset = 0u64;
        // for field in fields {
        //     object[field] = decode_at(field, base + field.offset, base)
        // }
        for (index, field) in fields.iter().copied().enumerate() {
            let head = self.builder.add_offset(base, offset);
            let value =
                self.materialize_calldata_value_at_inner(field, head, base, span, nested_validate)?;
            let value = self.encode_memory_scalar(field, value);
            self.builder.memory_store_field(object, index as u64, value);
            offset = offset.checked_add(self.types.abi_type(field)?.head_size())?;
        }
        Some(object)
    }

    /// Copies bytes for consumers that use zero-padded calldata reads rather than source-range
    /// validation. Ordinary calldata-to-memory conversions retain their checked path.
    pub(super) fn copy_calldata_bytes_unchecked(&mut self, value: ValueId) -> Option<ValueId> {
        if self.builder.func().value_slice_location(value) != Some(PtrLocation::Calldata) {
            return Some(value);
        }
        // length = slice_len(value)
        // source = slice_ptr(value)
        // object = alloc_bytes(length)
        // copy(memory_data(object), source, length, calldata)
        let length = self.builder.slice_len(value);
        let source = self.builder.slice_ptr(value);
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        let dest = self.builder.memory_data(object);
        self.builder.copy_heap(dest, source, length, PtrLocation::Calldata);
        Some(object)
    }
}
