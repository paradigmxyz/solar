//! ABI value and packed encoding helpers for one lowered function.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn lower_abi_encode_builtin(
        &mut self,
        args: hir::CallArgs<'_>,
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncode, &args)?;
        self.lower_abi_encode_builtin_args(exprs, selector)
    }

    pub(super) fn lower_abi_encode_builtin_args(
        &mut self,
        exprs: &[hir::Expr<'_>],
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let encoded = self.lower_abi_encode_slice(exprs, selector)?;
        Some(self.materialize_memory_slice(encoded))
    }

    pub(super) fn lower_abi_encode_slice(
        &mut self,
        exprs: &[hir::Expr<'_>],
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            let memory_ty = ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
            let mut value = self.lower_typed_expr(expr, memory_ty)?;
            let mut abi_type = if matches!(ty.peel_refs().kind, TyKind::StringLiteral(..)) {
                AbiType::Bytes(SliceLocation::Memory)
            } else {
                self.types.abi_type(ty)?
            };
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(ty, value, expr.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some(self.builder.abi_encode(layout, selector, values.into_boxed_slice()))
    }

    pub(super) fn lower_selector_word(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let value = self.lower_expr(expr)?;
        let fixed_bytes = self.gcx.type_of_expr(expr.id).is_some_and(|ty| {
            matches!(
                ty.peel_refs().kind,
                TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
            )
        });
        if !fixed_bytes
            && matches!(expr.peel_parens().kind, ExprKind::Lit(lit) if matches!(
                lit.kind,
                LitKind::Number(_) | LitKind::Rational(_)
            ))
        {
            let shift = self.builder.imm_u64(224);
            return Some(self.builder.shl(shift, value));
        }
        Some(value)
    }

    pub(super) fn lower_abi_encode_with_signature(
        &mut self,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let (signature, rest) =
            self.builtin_args_with_rest::<1>(Builtin::AbiEncodeWithSignature, &args)?;
        let selector = self.lower_signature_selector(&signature[0])?;
        self.lower_abi_encode_builtin_args(rest, Some(selector))
    }

    fn lower_signature_selector(&mut self, signature: &hir::Expr<'_>) -> Option<ValueId> {
        if let ExprKind::Lit(lit) = &signature.kind
            && let LitKind::Str(_, value, _) = &lit.kind
        {
            let hash = keccak256(value.as_byte_str());
            let selector = U256::from_be_slice(&hash[..4]) << 224;
            return Some(self.builder.imm_u256(selector));
        }

        if let ExprKind::Ternary(condition, then_expr, else_expr) = &signature.kind {
            let condition = self.lower_expr(condition)?;
            let then_selector = self.lower_signature_selector(then_expr)?;
            let else_selector = self.lower_signature_selector(else_expr)?;
            return Some(self.builder.select(condition, then_selector, else_selector));
        }

        let signature = self.lower_expr(signature)?;
        let signature = match self.builder.func().value_ty(signature) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(signature),
            _ => signature,
        };
        let hash = self.builder.keccak256_bytes(signature);
        let shift = self.builder.imm_u64(224);
        let selector = self.builder.shr(shift, hash);
        Some(self.builder.shl(shift, selector))
    }

    pub(super) fn lower_abi_encode_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiEncodeCall, &args)?;
        let function = &args[0];
        let tuple = &args[1];
        let (selector, parameter_types) =
            if let Some(function_id) = self.gcx.resolved_function(function) {
                let selector = self.gcx.function_selector(function_id).0;
                let parameter_types = self
                    .gcx
                    .hir
                    .function(function_id)
                    .parameters
                    .iter()
                    .map(|&parameter| self.gcx.type_of_item(parameter.into()))
                    .collect::<Vec<_>>();
                (self.builder.imm_u256(U256::from_be_slice(&selector) << 224), parameter_types)
            } else {
                let Some(TyKind::Fn(function_ty)) =
                    self.gcx.type_of_expr(function.id).map(|ty| ty.kind)
                else {
                    return report_unsupported(self.gcx, function.span, "abi.encodeCall function");
                };
                if !function_ty.is_external() {
                    return report_unsupported(self.gcx, function.span, "abi.encodeCall function");
                }
                let function_value = self.lower_expr(function)?;
                let mask = self.builder.imm_u256(U256::from(u32::MAX));
                let selector = self.builder.and(function_value, mask);
                let shift = self.builder.imm_u64(224);
                (self.builder.shl(shift, selector), function_ty.parameters.to_vec())
            };
        let exprs = match tuple.peel_parens().kind {
            ExprKind::Tuple(elements) => elements.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![tuple],
        };
        if exprs.len() != parameter_types.len() {
            return report_unsupported(self.gcx, tuple.span, "abi.encodeCall argument list");
        }
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for (index, expr) in exprs.into_iter().enumerate() {
            let ty = parameter_types[index];
            let from_ty = self.gcx.type_of_expr(expr.id)?;
            let memory_ty = ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
            let mut value = self.lower_typed_expr(expr, memory_ty)?;
            value = self.coerce_value(value, from_ty, ty);
            let mut abi_type = self.types.abi_type(ty)?;
            abi_type = self.abi_type_for_value(value, abi_type);
            if self.needs_calldata_materialization(value, &abi_type) {
                value = self.materialize_calldata_argument(ty, value, expr.span)?;
                abi_type = Self::memory_abi_type(abi_type);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        Some(self.materialize_memory_slice(encoded))
    }

    pub(super) fn lower_abi_decode(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiDecode, &args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => return report_unsupported(self.gcx, args[1].span, "abi.decode target type"),
        };
        if types.is_empty() {
            return report_unsupported(self.gcx, args[1].span, "abi.decode target type");
        }
        let mut decoded_types = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind) else {
                return report_unsupported(self.gcx, ty_expr.span, "abi.decode target type");
            };
            decoded_types.push(ty.with_loc_if_ref(self.gcx, DataLocation::Memory));
        }

        let data_expr = &args[0];
        let data_ty = self.gcx.type_of_expr(data_expr.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
        let data = self.lower_typed_expr(data_expr, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_expr.span)?;
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        Some(self.builder.abi_decode(layout, data))
    }

    fn lower_abi_decode_layout(
        &mut self,
        data: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<(ValueId, AbiParamLayout)> {
        let data = match self.builder.func().value_ty(data) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(data),
            _ => data,
        };
        let mut abi_types = Vec::with_capacity(types.len());
        for &ty in types {
            let Some(abi_type) = self.types.abi_param_type(ty) else {
                return report_unsupported(self.gcx, span, "abi.decode target type");
            };
            abi_types.push(abi_type);
        }
        Some((data, AbiParamLayout::new(abi_types.into_boxed_slice())))
    }

    pub(super) fn lower_abi_decode_values(
        &mut self,
        data: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<Vec<ValueId>> {
        let (data, layout) = self.lower_abi_decode_layout(data, types, span)?;
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let base = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        crate::transform::lower_abi::decode_memory_tuple(
            &mut self.builder,
            base,
            length,
            &layout,
            None,
        )
    }

    pub(super) fn revert_external_call(&mut self, success: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(success, continue_block, revert);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u256(U256::ZERO);
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            let size = self.builder.returndatasize();
            self.builder.returndatacopy(zero, zero, size);
            self.builder.revert(zero, size);
        } else {
            self.builder.revert(zero, zero);
        }
        self.builder.switch_to_block(continue_block);
    }

    pub(super) fn materialize_memory_slice(&mut self, slice: ValueId) -> ValueId {
        let length = self.builder.slice_len(slice);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, slice);
        object
    }

    pub(super) fn materialize_returndata_bytes(&mut self) -> ValueId {
        let length = self.builder.returndatasize();
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(length, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let total_words = self.checked_add(words, one);
        let size = self.checked_mul(total_words, word_size);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let source = self.builder.make_slice(zero, length, SliceLocation::Returndata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
    }

    pub(super) fn lower_error_catch_string(
        &mut self,
        data: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let data_len = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let four = self.builder.imm_u64(4);
        let payload_ptr = self.builder.add(data_ptr, four);
        let payload_len = self.builder.sub(data_len, four);
        let layout = AbiParamLayout::new(vec![AbiParamType::Bytes].into_boxed_slice());
        crate::transform::lower_abi::decode_memory_tuple(
            &mut self.builder,
            payload_ptr,
            payload_len,
            &layout,
            None,
        )
        .and_then(|mut values| values.pop())
        .or_else(|| report_unsupported(self.gcx, span, "Error catch payload"))
    }

    pub(super) fn lower_panic_catch_word(&mut self, data: ValueId) -> ValueId {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let four = self.builder.imm_u64(4);
        let payload_ptr = self.builder.add(data_ptr, four);
        let word_size = self.builder.imm_u64(32);
        let payload = self.builder.make_slice(payload_ptr, word_size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(payload, zero)
    }

    pub(super) fn lower_abi_encode_packed(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        let mut total = self.builder.imm_u64(0);
        let mut pieces = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.gcx.type_of_expr(expr.id)?;
            if let ExprKind::Lit(lit) = &expr.kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str().to_vec();
                let length = self.builder.imm_u64(bytes.len() as u64);
                total = self.checked_add(total, length);
                pieces.push(PackedPiece::Bytes(bytes));
                continue;
            }

            let memory_ty = ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
            let value = self.lower_typed_expr(expr, memory_ty)?;
            if self.is_calldata_dynamic_bytes_type(ty)
                || matches!(
                    ty.peel_refs().kind,
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
                )
            {
                let value_ty = self.builder.func().value_ty(value);
                let is_slice = matches!(
                    value_ty,
                    Some(MirType::Slice(SliceLocation::Calldata | SliceLocation::Memory))
                );
                let length = if is_slice {
                    self.builder.slice_len(value)
                } else {
                    self.builder.memory_object_len(value, MemoryObjectKind::Bytes)
                };
                total = self.checked_add(total, length);
                let source = if is_slice {
                    value
                } else {
                    let pointer = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                    self.builder.make_slice(pointer, length, SliceLocation::Memory)
                };
                pieces.push(PackedPiece::Dynamic { source, length });
                continue;
            }

            if let Some((length, source)) = self.packed_array_shape(ty, value) {
                let word = self.builder.imm_u64(32);
                let byte_length = self.checked_mul(length, word);
                total = self.checked_add(total, byte_length);
                pieces.push(PackedPiece::Array { value, length, source });
                continue;
            }

            let Some((length, fixed_bytes)) = self.packed_static_shape(ty) else {
                return report_unsupported(self.gcx, expr.span, "abi.encodePacked argument");
            };
            let length_value = self.builder.imm_u64(length);
            total = self.checked_add(total, length_value);
            pieces.push(PackedPiece::Static { value, length, fixed_bytes });
        }

        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.checked_add(total, thirty_one);
        let word_size = self.builder.imm_u64(32);
        let words = self.builder.div(rounded, word_size);
        let one = self.builder.imm_u64(1);
        let words = self.checked_add(words, one);
        let size = self.checked_mul(words, word_size);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);

        let mut offset = self.builder.imm_u64(0);
        let mut index = 0;
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm_u64(length);
                offset = self.checked_add(offset, length);
                index += consumed;
                continue;
            }

            match &pieces[index] {
                PackedPiece::Bytes(bytes) => {
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm_u256(U256::from_be_bytes(padded));
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm_u64(chunk.len() as u64);
                        offset = self.checked_add(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        *source,
                    );
                    offset = self.checked_add(offset, *length);
                }
                PackedPiece::Array { value, length, source } => {
                    offset = self.copy_packed_array(output, offset, *value, *length, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes } => {
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm_u64((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm_u64(*length);
                    offset = self.checked_add(offset, length);
                }
            }
            index += 1;
        }
        Some(output)
    }

    fn packed_array_shape(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(ValueId, PackedArraySource)> {
        let element = match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => element,
            TyKind::Slice(inner) => match inner.peel_refs().kind {
                TyKind::DynArray(element) | TyKind::Slice(element) | TyKind::Array(element, _) => {
                    element
                }
                _ => inner,
            },
            _ => return None,
        };
        if !matches!(self.types.abi_type(element)?, AbiType::Word) {
            return None;
        }

        let layout = self.types.memory_layout(ty)?;
        let source = match self.builder.func().value_ty(value) {
            Some(MirType::MemoryObject(
                MemoryObjectKind::DynamicArray | MemoryObjectKind::FixedArray,
            )) => PackedArraySource::Memory { layout },
            Some(MirType::Slice(location @ (SliceLocation::Memory | SliceLocation::Calldata))) => {
                PackedArraySource::Slice(location)
            }
            _ => return None,
        };
        let length = match source {
            PackedArraySource::Memory { layout: MemoryObjectLayout::DynamicArray { .. } } => {
                self.builder.memory_object_len(value, MemoryObjectKind::DynamicArray)
            }
            PackedArraySource::Memory { layout: MemoryObjectLayout::FixedArray { len, .. } } => {
                self.builder.imm_u64(len)
            }
            PackedArraySource::Memory { .. } => return None,
            PackedArraySource::Slice(_) => self.builder.slice_len(value),
        };
        Some((length, source))
    }

    pub(super) fn lower_packed_word_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        let (length, source) = self.packed_array_shape(ty, value)?;
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let size = self.checked_add(word, byte_length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, byte_length, MemoryObjectKind::Bytes);
        let offset = self.builder.imm_u64(0);
        self.copy_packed_array(output, offset, value, length, source);
        Some(output)
    }

    fn copy_packed_array(
        &mut self,
        output: ValueId,
        offset: ValueId,
        value: ValueId,
        length: ValueId,
        source: PackedArraySource,
    ) -> ValueId {
        let word = self.builder.imm_u64(32);
        let byte_length = self.checked_mul(length, word);
        let end_offset = self.checked_add(offset, byte_length);
        let base = match source {
            PackedArraySource::Memory { .. } => None,
            PackedArraySource::Slice(_) => Some(self.builder.slice_ptr(value)),
        };
        let memory_source = match source {
            PackedArraySource::Slice(SliceLocation::Memory) => Some(self.builder.make_slice(
                base.expect("slice base"),
                byte_length,
                SliceLocation::Memory,
            )),
            _ => None,
        };

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
        let element_offset = self.checked_mul(index, word);
        let destination = self.checked_add(offset, element_offset);
        let element = match source {
            PackedArraySource::Memory { layout } => {
                self.builder.memory_object_load_element(value, layout, index)
            }
            PackedArraySource::Slice(location) => match location {
                SliceLocation::Memory => self
                    .builder
                    .memory_slice_load_word(memory_source.expect("memory slice"), element_offset),
                SliceLocation::Calldata => {
                    self.builder.calldata_slice_load_word(value, element_offset)
                }
                SliceLocation::Returndata => unreachable!("returndata packed array"),
            },
        };
        self.builder.memory_object_store_word(output, destination, element);
        let one = self.builder.imm_u64(1);
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        end_offset
    }

    pub(super) fn is_calldata_dynamic_bytes_type(&self, ty: Ty<'gcx>) -> bool {
        match ty.kind {
            TyKind::Ref(inner, DataLocation::Calldata) => matches!(
                inner.kind,
                TyKind::Elementary(
                    solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                )
            ),
            TyKind::Slice(inner) => {
                inner.is_ref_at(DataLocation::Calldata)
                    && matches!(
                        inner.peel_refs().kind,
                        TyKind::Elementary(
                            solar_sema::hir::ElementaryType::Bytes
                                | solar_sema::hir::ElementaryType::String,
                        )
                    )
            }
            _ => false,
        }
    }

    fn try_write_packed_word(
        &mut self,
        output: ValueId,
        offset: ValueId,
        pieces: &[PackedPiece],
    ) -> Option<(usize, u64)> {
        let mut constant = U256::ZERO;
        let mut terms = Vec::new();
        let mut length = 0u64;
        let mut consumed = 0;

        for piece in pieces {
            match piece {
                PackedPiece::Bytes(bytes) => {
                    let piece_length = u64::try_from(bytes.len()).ok()?;
                    if piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - piece_length) * 8;
                    constant |= U256::from_be_slice(bytes) << shift;
                    length += piece_length;
                    consumed += 1;
                }
                PackedPiece::Static { value, length: piece_length, fixed_bytes: false }
                    if *piece_length < 32 =>
                {
                    if *piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(*piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - *piece_length) * 8;
                    terms.push((*value, shift));
                    length += *piece_length;
                    consumed += 1;
                }
                _ => break,
            }
        }

        if consumed < 2 || length == 0 || terms.is_empty() {
            return None;
        }

        let mut value = self.builder.imm_u256(constant);
        for (term, shift) in terms {
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm_u64(shift);
                self.builder.shl(shift, term)
            };
            value = self.builder.or(value, term);
        }
        self.builder.memory_object_store_word(output, offset, value);
        Some((consumed, length))
    }

    fn packed_static_shape(&self, ty: Ty<'gcx>) -> Option<(u64, bool)> {
        match ty.peel_refs().kind {
            TyKind::Elementary(elementary) => Some(match elementary {
                solar_sema::hir::ElementaryType::Bool => (1, false),
                solar_sema::hir::ElementaryType::Address(_) => (20, false),
                solar_sema::hir::ElementaryType::Int(size)
                | solar_sema::hir::ElementaryType::UInt(size)
                | solar_sema::hir::ElementaryType::Fixed(size, _)
                | solar_sema::hir::ElementaryType::UFixed(size, _) => {
                    (u64::from(size.bytes()), false)
                }
                solar_sema::hir::ElementaryType::FixedBytes(size) => {
                    (u64::from(size.bytes()), true)
                }
                _ => return None,
            }),
            TyKind::Contract(_) => Some((20, false)),
            TyKind::Enum(id) => {
                let variants = self.gcx.hir.enumm(id).variants.len().max(1);
                let bits = (usize::BITS - (variants - 1).leading_zeros()).max(1);
                Some((u64::from(bits.div_ceil(8)), false))
            }
            TyKind::Udvt(inner, _) => self.packed_static_shape(inner),
            TyKind::IntLiteral(..) => Some((32, false)),
            _ => None,
        }
    }

    pub(super) fn lower_hash_precompile_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let input = &self.builtin_args::<1>(builtin, &args)?[0];
        let input_ty = self.gcx.type_of_expr(input.id)?;
        if !matches!(self.types.memory_layout(input_ty)?, MemoryObjectLayout::Bytes) {
            return report_unsupported(self.gcx, input.span, "precompile input");
        }
        let span = input.span;
        let memory_ty = input_ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
        let input = self.lower_typed_expr(input, memory_ty)?;
        let input = self.materialize_memory_argument(memory_ty, input, span)?;
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        let input_len = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);

        let output_size = self.builder.imm_u64(64);
        let output = self.builder.alloc_object(
            output_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let output_len = self.builder.imm_u64(32);
        self.builder.set_memory_object_len(output, output_len, MemoryObjectKind::Bytes);
        let output_ptr = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);
        let address = self.builder.imm_u64(if builtin == Builtin::Sha256 { 2 } else { 3 });
        let output_size = self.builder.imm_u64(32);
        self.lower_precompile_call(address, input_ptr, input_len, output_ptr, output_size);
        let zero = self.builder.imm_u64(0);
        let output_slice = self.builder.make_slice(output_ptr, output_len, SliceLocation::Memory);
        let value = self.builder.memory_slice_load_word(output_slice, zero);
        Some(if builtin == Builtin::Ripemd160 {
            let scale = self.builder.imm_u256(U256::from(1) << 96);
            self.builder.mul(scale, value)
        } else {
            value
        })
    }

    pub(super) fn lower_ecrecover_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let values = self.builtin_args::<4>(Builtin::EcRecover, &args)?;
        let hash = &values[0];
        let v = &values[1];
        let r = &values[2];
        let s = &values[3];
        let hash = self.lower_expr(hash)?;
        let v = self.lower_expr(v)?;
        let r = self.lower_expr(r)?;
        let s = self.lower_expr(s)?;

        let input_size = self.builder.imm_u64(192);
        let input = self.builder.alloc_object(
            input_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let input_len = self.builder.imm_u64(160);
        self.builder.set_memory_object_len(input, input_len, MemoryObjectKind::Bytes);
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u64(0);
        self.builder.memory_object_store_word(input, zero, hash);
        for (offset, value) in [(32, v), (64, r), (96, s)] {
            let offset = self.builder.imm_u64(offset);
            self.builder.memory_object_store_word(input, offset, value);
        }
        let output_size = self.builder.imm_u64(64);
        let output = self.builder.alloc_object(
            output_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let output_len = self.builder.imm_u64(32);
        self.builder.set_memory_object_len(output, output_len, MemoryObjectKind::Bytes);
        let output_ptr = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);

        let address = self.builder.imm_u64(1);
        let input_size = self.builder.imm_u64(128);
        let output_size = self.builder.imm_u64(32);
        self.lower_precompile_call(address, input_ptr, input_size, output_ptr, output_size);
        let output_slice = self.builder.make_slice(output_ptr, output_len, SliceLocation::Memory);
        Some(self.builder.memory_slice_load_word(output_slice, zero))
    }

    fn lower_precompile_call(
        &mut self,
        address: ValueId,
        input_ptr: ValueId,
        input_size: ValueId,
        output_ptr: ValueId,
        output_size: ValueId,
    ) {
        let evm_version = self.gcx.sess.opts.evm_version;
        let gas = crate::utils::precompile_gas(&mut self.builder, evm_version);
        if evm_version.has_static_call() {
            self.builder.staticcall(gas, address, input_ptr, input_size, output_ptr, output_size);
        } else {
            let value = self.builder.imm_u256(U256::ZERO);
            self.builder.call(gas, address, value, input_ptr, input_size, output_ptr, output_size);
        }
    }
}
