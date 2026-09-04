//! ABI value and packed encoding helpers for one lowered function.

use super::*;

enum PackedPiece<'gcx> {
    Bytes(Vec<u8>),
    Static {
        value: ValueId,
        length: u64,
        fixed_bytes: bool,
        signed: bool,
    },
    Dynamic {
        source: ValueId,
        length: ValueId,
    },
    Array {
        value: ValueId,
        length: ValueId,
        element: PackedArrayElement<'gcx>,
        source: PackedArraySource,
    },
}

struct PackedArrayElement<'gcx> {
    abi: AbiType,
    ty: Ty<'gcx>,
}

#[derive(Clone, Copy)]
enum PackedArraySource {
    Memory { layout: MemoryObjectLayout },
    Slice(SliceLocation),
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
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
        let (layout, values) = self.lower_abi_encode_arguments(exprs)?;
        Some(self.builder.abi_encode_bytes(layout, selector, values))
    }

    pub(super) fn lower_abi_encode_scratch(
        &mut self,
        exprs: &[hir::Expr<'_>],
        selector: Option<ValueId>,
    ) -> Option<ValueId> {
        let (layout, values) = self.lower_abi_encode_arguments(exprs)?;
        Some(self.builder.abi_encode_scratch(layout, selector, values))
    }

    fn lower_abi_encode_arguments(
        &mut self,
        exprs: &[hir::Expr<'_>],
    ) -> Option<(Arc<AbiLayout>, Box<[ValueId]>)> {
        // values = lower_typed(args)
        // values, types = prepare_abi_arguments(values)
        // layout = abi_layout(types)
        // return (layout, values)
        let values_and_types = self.lower_argument_exprs(
            CallArgumentParams { count: exprs.len(), names: None, reverse: false },
            exprs.iter().enumerate(),
            |this, _, expr| {
                let ty = this.cx.gcx.type_of_expr(expr.id)?;
                let memory_ty = ty.with_loc_if_ref(this.cx.gcx, DataLocation::Memory);
                let value = this.lower_typed_expr(expr, memory_ty)?;
                let abi_type = if matches!(ty.peel_refs().kind, TyKind::StringLiteral(..)) {
                    AbiType::Bytes(SliceLocation::Memory)
                } else {
                    this.types.abi_type(ty)?
                };
                this.prepare_abi_encode_argument(expr, ty, value, abi_type)
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = values_and_types.into_iter().unzip();
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some((layout, values.into_boxed_slice()))
    }

    pub(super) fn lower_selector_word(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let value = if let ExprKind::Lit(lit) = expr.peel_parens().kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            self.lower_string_literal_word(bytes.as_byte_str())
        } else {
            self.lower_expr(expr)?
        };
        let fixed_bytes = self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| {
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
            let shift = self.builder.imm(224);
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
        if let Some(selector) = Self::literal_signature_selector(signature) {
            // selector = keccak256(literal)[0..4] << 224
            return Some(self.builder.imm(selector));
        }

        if let ExprKind::Ternary(condition, then_expr, else_expr) = &signature.kind
            && let Some(then_selector) = Self::literal_signature_selector(then_expr)
            && let Some(else_selector) = Self::literal_signature_selector(else_expr)
        {
            // selector = select(condition, selector(then), selector(else))
            let condition = self.lower_expr(condition)?;
            let then_selector = self.builder.imm(then_selector);
            let else_selector = self.builder.imm(else_selector);
            return Some(self.builder.select(condition, then_selector, else_selector));
        }

        // selector = keccak256(materialize(signature))[0..4] << 224
        let signature_ty = self.cx.gcx.type_of_expr(signature.id);
        let signature = self.lower_expr(signature)?;
        if let Some(signature_ty) = signature_ty
            && let Some(abi_type) = self.types.abi_type(signature_ty)
        {
            self.validate_calldata_bytes_argument(signature, &abi_type);
        }
        let signature = match self.builder.func().value_ty(signature) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(signature),
            _ => signature,
        };
        let hash = self.builder.keccak256_bytes(signature);
        let shift = self.builder.imm(224);
        let selector = self.builder.shr(shift, hash);
        Some(self.builder.shl(shift, selector))
    }

    fn literal_signature_selector(signature: &hir::Expr<'_>) -> Option<U256> {
        let ExprKind::Lit(lit) = &signature.peel_parens().kind else { return None };
        let LitKind::Str(_, value, _) = &lit.kind else { return None };
        let hash = keccak256(value.as_byte_str());
        Some(U256::from_be_slice(&hash[..4]) << 224)
    }

    pub(super) fn lower_abi_encode_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        // data = abi_encode_bytes(parameter_layout, function.selector, values)
        let args = self.builtin_args::<2>(Builtin::AbiEncodeCall, &args)?;
        let function = &args[0];
        let tuple = &args[1];
        let (selector, parameter_types) =
            if let Some(function_id) = self.cx.gcx.resolved_function(function) {
                let selector = self.cx.gcx.function_selector(function_id).0;
                let parameter_types = self
                    .cx
                    .gcx
                    .hir
                    .function(function_id)
                    .parameters
                    .iter()
                    .map(|&parameter| self.cx.gcx.type_of_item(parameter.into()))
                    .collect::<Vec<_>>();
                (self.builder.imm(U256::from_be_slice(&selector) << 224), parameter_types)
            } else {
                let Some(TyKind::Fn(function_ty)) =
                    self.cx.gcx.type_of_expr(function.id).map(|ty| ty.kind)
                else {
                    return self.cx.report_unsupported(function.span, "abi.encodeCall function");
                };
                if !function_ty.is_external() {
                    return self.cx.report_unsupported(function.span, "abi.encodeCall function");
                }
                let function_value = self.lower_expr(function)?;
                let mask = self.builder.imm(u32::MAX);
                let selector = self.builder.and(function_value, mask);
                let shift = self.builder.imm(224);
                (self.builder.shl(shift, selector), function_ty.parameters.to_vec())
            };
        let exprs = match tuple.peel_parens().kind {
            ExprKind::Tuple(elements) => elements.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![tuple],
        };
        if exprs.len() != parameter_types.len() {
            return self.cx.report_unsupported(tuple.span, "abi.encodeCall argument list");
        }
        let values_and_types = self.lower_argument_exprs(
            CallArgumentParams { count: exprs.len(), names: None, reverse: false },
            exprs.into_iter().enumerate(),
            |this, index, expr| {
                let ty = parameter_types[index];
                let memory_ty = ty.with_loc_if_ref(this.cx.gcx, DataLocation::Memory);
                let value = this.lower_typed_expr(expr, memory_ty)?;
                let abi_type = this.types.abi_type(ty)?;
                this.prepare_abi_encode_argument(expr, ty, value, abi_type)
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = values_and_types.into_iter().unzip();
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some(self.builder.abi_encode_bytes(layout, Some(selector), values.into_boxed_slice()))
    }

    pub(super) fn canonicalize_abi_value(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let external_argument = self.is_external_abi_argument(value);
        let dirty = self.dirty_values.contains(&value);
        let external_only = external_argument
            && self.builder.func().attributes.visibility == solar_ast::Visibility::External;
        match ty.peel_refs().kind {
            // Aggregates are cleaned and validated word by word while encoding, like solc's
            // per-type encoders; copying them into a canonical object first would duplicate
            // the whole tree at every call site.
            TyKind::DynArray(_) | TyKind::Array(_, _) | TyKind::Struct(_) => value,
            _ if external_only && !dirty => value,
            _ => self.normalize_abi_scalar(value, ty),
        }
    }

    pub(super) fn lower_abi_decode(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        // data = materialize_memory_argument(input)
        // layout = intern_abi_layout(target_types)
        // value = abi_decode(layout, data)
        let args = self.builtin_args::<2>(Builtin::AbiDecode, &args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => {
                return self.cx.report_unsupported(args[1].span, "abi.decode target type");
            }
        };
        if types.is_empty() {
            return self.cx.report_unsupported(args[1].span, "abi.decode target type");
        }
        let mut decoded_types = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) = self.cx.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind)
            else {
                return self.cx.report_unsupported(ty_expr.span, "abi.decode target type");
            };
            decoded_types.push(ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory));
        }

        let data_expr = &args[0];
        let data_ty = self.cx.gcx.type_of_expr(data_expr.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        let data = self.lower_typed_expr(data_expr, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_expr.span)?;
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
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
                return self.cx.report_unsupported(span, "abi.decode target type");
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
        // data, layout = lower_decode_layout(data, types)
        // first = abi_decode(layout, data)
        // values = [first] | load_multi_return_values(first, ...)
        let decoded_types = types
            .iter()
            .copied()
            .map(|ty| types::TypeLowerer::return_encoding_ty(self.cx.gcx, ty))
            .collect::<Vec<_>>();
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
        let first = self.builder.abi_decode(layout, data);
        if decoded_types.len() == 1 {
            return Some(vec![first]);
        }
        let base = self.multi_return_buffer_base();
        Some(self.load_multi_return_values(
            first,
            base,
            decoded_types.len(),
            decoded_types.iter().skip(1).copied().map(Some),
        ))
    }

    pub(super) fn revert_external_call(&mut self, success: ValueId) {
        // if !success { revert(0, returndatasize()) }
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(success, continue_block, revert);
        self.builder.switch_to_block(revert);
        self.builder.revert_returndata();
        self.builder.switch_to_block(continue_block);
    }

    pub(super) fn materialize_memory_slice(&mut self, slice: ValueId) -> ValueId {
        // object = bytes(slice.len)
        // copy(slice, object.data)
        // return object
        let length = self.builder.slice_len(slice);
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, slice);
        object
    }

    pub(super) fn materialize_returndata_bytes(&mut self) -> ValueId {
        // object = bytes(returndatasize)
        // copy(returndata(0), object.data, returndatasize)
        let length = self.current_returndata_size();
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        let zero = self.builder.imm(U256::ZERO);
        let source = self.builder.make_slice(zero, length, SliceLocation::Returndata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
    }

    /// Returns the current call's returndata size, or zero before Byzantium.
    pub(super) fn current_returndata_size(&mut self) -> ValueId {
        if self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            self.builder.returndatasize()
        } else {
            self.builder.imm(U256::ZERO)
        }
    }

    pub(super) fn lower_error_catch_string(&mut self, data: ValueId) -> Option<ValueId> {
        // payload = bytes(data[4:])
        // message = abi_decode(bytes, payload)
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let data_len = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let four = self.builder.imm(4);
        let payload_ptr = self.builder.add_u64_offset(data_ptr, 4);
        let payload_len = self.builder.sub(data_len, four);
        let payload_slice =
            self.builder.make_slice(payload_ptr, payload_len, SliceLocation::Memory);
        let payload = self.materialize_memory_slice(payload_slice);
        let layout = self.cx.module.intern_abi_param_layout(AbiParamLayout::new(
            vec![AbiParamType::Bytes].into_boxed_slice(),
        ));
        Some(self.builder.abi_decode(layout, payload))
    }

    /// Checks whether an `Error(string)` payload can be decoded without reverting.
    pub(super) fn lower_error_catch_match(
        &mut self,
        data_ptr: ValueId,
        data_len: ValueId,
        selector_matches: ValueId,
    ) -> ValueId {
        // valid = selector_matches ? try_decode_error_message(data) : false
        let validate = self.builder.create_block();
        let no_match = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(selector_matches, validate, no_match);

        self.builder.switch_to_block(validate);
        let helper = self.ensure_error_catch_match_helper();
        let valid = self.builder.internal_call(helper, vec![data_ptr, data_len], MirType::Bool, 1);
        let valid_block = self.builder.current_block();
        self.builder.jump(done);

        self.builder.switch_to_block(no_match);
        let no_match_value = self.builder.imm_bool(false);
        let no_match_block = self.builder.current_block();
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        self.builder.phi(vec![(valid_block, valid), (no_match_block, no_match_value)])
    }

    /// Synthesizes the shared equivalent of Solc's `try_decode_error_message` helper.
    ///
    /// <https://github.com/ethereum/solidity/blob/develop/libsolidity/codegen/YulUtilFunctions.cpp#L4676-L4714>
    fn ensure_error_catch_match_helper(&mut self) -> FunctionId {
        // valid = len >= 68
        // offset = mload(data + 4)
        // valid &= offset <= u64::MAX && offset + 36 <= len
        // msg_len = mload(data + 4 + offset)
        // valid &= msg_len <= u64::MAX && msg_len <= len - (offset + 36)
        self.lazy_helper(sym::try_decode_error_message, |_, function| {
            let mut builder = FunctionBuilder::new(function);
            let data_ptr = builder.add_param(MirType::MemPtr);
            let data_len = builder.add_param(MirType::uint256());
            builder.add_return(MirType::Bool);

            let check_offset = builder.create_block();
            let check_length = builder.create_block();
            let no_match = builder.create_block();

            let min_size = builder.imm(68);
            let short = builder.lt(data_len, min_size);
            let has_head = builder.iszero(short);
            builder.branch(has_head, check_offset, no_match);

            builder.switch_to_block(check_offset);
            let payload_ptr = builder.add_u64_offset(data_ptr, 4);
            let offset = builder.mload(payload_ptr);
            let max_u64 = builder.imm(u64::MAX);
            let offset_too_large = builder.gt(offset, max_u64);
            let message_data_offset = builder.add_u64_offset(offset, 36);
            let head_out_of_range = builder.gt(message_data_offset, data_len);
            let invalid_offset = builder.or(offset_too_large, head_out_of_range);
            builder.branch(invalid_offset, no_match, check_length);

            builder.switch_to_block(check_length);
            let message_ptr = builder.add(payload_ptr, offset);
            let length = builder.mload(message_ptr);
            let length_too_large = builder.gt(length, max_u64);
            let remaining = builder.sub(data_len, message_data_offset);
            let data_out_of_range = builder.gt(length, remaining);
            let invalid_length = builder.or(length_too_large, data_out_of_range);
            let valid = builder.iszero(invalid_length);
            builder.ret([valid]);

            builder.switch_to_block(no_match);
            let no_match = builder.imm_bool(false);
            builder.ret([no_match]);
            Some(())
        })
        .expect("error catch match helper construction cannot fail")
    }

    pub(super) fn lower_panic_catch_word(&mut self, data: ValueId) -> ValueId {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm(U256::ZERO);
        let payload_ptr = self.builder.add_u64_offset(data_ptr, 4);
        let word_size = self.builder.imm(32);
        let payload = self.builder.make_slice(payload_ptr, word_size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(payload, zero)
    }

    pub(super) fn lower_abi_encode_packed(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        // pieces, total = lower_packed_pieces(args)
        let (pieces, total) = self.lower_packed_pieces(exprs, true)?;

        // output = bytes(total)
        let output = self.builder.alloc_bytes_object(total, AllocationSemantics::INTERNAL);

        let mut offset = self.builder.imm(0);
        let mut index = 0;
        // for piece { write_packed(output, piece) }
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm(length);
                offset = self.builder.checked_add(offset, length);
                index += consumed;
                continue;
            }

            match &pieces[index] {
                PackedPiece::Bytes(bytes) => {
                    // for chunk { mstore(output + offset, chunk) }
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm(U256::from_be_bytes(padded));
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm(chunk.len() as u64);
                        offset = self.builder.checked_add(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    // copy(source, output + offset)
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        *source,
                    );
                    offset = self.builder.checked_add(offset, *length);
                }
                PackedPiece::Array { value, length, element, source } => {
                    // offset = copy_packed_array(output, offset, value)
                    offset =
                        self.copy_packed_array(output, offset, *value, *length, element, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(output + offset, align(value, length))
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm(*length);
                    offset = self.builder.checked_add(offset, length);
                }
            }
            index += 1;
        }
        Some(output)
    }

    /// Hashes a statically packed input without allocating a bytes object.
    pub(super) fn lower_keccak_abi_encode_packed(
        &mut self,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        if !exprs.iter().all(|expr| self.is_scratch_packed_expr(expr)) {
            return None;
        }
        // pieces, total = lower_packed_pieces(args)
        let (pieces, total) = self.lower_packed_pieces(exprs, false)?;
        let has_dynamic = pieces.iter().any(|piece| matches!(piece, PackedPiece::Dynamic { .. }));
        // base = has_dynamic ? fmp : (scratch_needed ? fmp : 0)
        let base = if has_dynamic {
            Some(self.builder.fmp())
        } else {
            let _ = u64::try_from(self.builder.func().value_u256(total)?).ok()?;
            let mut max_write_end = 0u64;
            let mut offset = 0u64;
            for piece in &pieces {
                match piece {
                    PackedPiece::Bytes(bytes) => {
                        for chunk in bytes.chunks(32) {
                            max_write_end = max_write_end.max(offset.checked_add(32)?);
                            offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                        }
                    }
                    PackedPiece::Static { length, .. } => {
                        max_write_end = max_write_end.max(offset.checked_add(32)?);
                        offset = offset.checked_add(*length)?;
                    }
                    PackedPiece::Dynamic { .. } | PackedPiece::Array { .. } => return None,
                }
            }
            (max_write_end > EvmMemoryLayout::FMP_SLOT).then(|| self.builder.fmp())
        };

        let zero = base.unwrap_or_else(|| self.builder.imm(0));
        let mut offset = 0u64;
        let mut cursor = has_dynamic.then(|| base.expect("dynamic packed input has a base"));
        let mut index = 0;
        // write_packed(base, pieces)
        while index < pieces.len() {
            if let Some((consumed, length, value)) = self.try_pack_packed_word(&pieces[index..]) {
                let dest = self.packed_scratch_offset(cursor.or(base), offset);
                self.builder.mstore(dest, value);
                offset = offset.checked_add(length)?;
                index += consumed;
                continue;
            }

            let piece = &pieces[index];
            match piece {
                PackedPiece::Bytes(bytes) => {
                    // for chunk { mstore(base + offset, chunk) }
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm(U256::from_be_bytes(padded));
                        let dest = self.packed_scratch_offset(cursor.or(base), offset);
                        self.builder.mstore(dest, value);
                        offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    // copy(source, cursor + offset)
                    let dest = self.packed_scratch_offset(cursor, offset);
                    let location = self.builder.func().value_slice_location(*source)?;
                    let source_length = self.builder.slice_len(*source);
                    let pointer = self.builder.slice_ptr(*source);
                    self.builder.copy_slice_data(location, dest, pointer, source_length);
                    cursor = Some(self.builder.add(dest, *length));
                    offset = 0;
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(base + offset, align(value, length))
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    let dest = self.packed_scratch_offset(cursor.or(base), offset);
                    self.builder.mstore(dest, value);
                    offset = offset.checked_add(*length)?;
                }
                PackedPiece::Array { .. } => return None,
            }
            index += 1;
        }

        // size = has_dynamic ? end(base) - base : total
        // hash = keccak256(base, size)
        let size = if has_dynamic {
            let cursor = cursor.expect("dynamic packed input has a cursor");
            let end = self.builder.add_u64_offset(cursor, offset);
            self.builder.sub(end, zero)
        } else {
            total
        };
        Some(self.builder.keccak256(zero, size))
    }

    pub(super) fn is_scratch_packed_expr(&self, expr: &hir::Expr<'_>) -> bool {
        if matches!(
            self.peel_bytes_conversion(expr).peel_parens().kind,
            ExprKind::Lit(lit) if matches!(lit.kind, LitKind::Str(..))
        ) {
            return true;
        }
        let Some(ty) = self.cx.gcx.type_of_expr(expr.id) else { return false };
        self.packed_static_shape(ty).is_some() || self.is_dynamic_bytes_type(ty)
    }

    pub(super) fn is_dynamic_bytes_type(&self, ty: Ty<'gcx>) -> bool {
        matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String
            )
        ) || matches!(
            ty.kind,
            TyKind::Slice(inner)
                if matches!(
                    inner.peel_refs().kind,
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String
                    )
                )
        )
    }

    fn packed_scratch_offset(&mut self, base: Option<ValueId>, offset: u64) -> ValueId {
        match base {
            Some(base) => self.builder.add_u64_offset(base, offset),
            None => self.builder.imm(offset),
        }
    }

    fn lower_packed_pieces(
        &mut self,
        exprs: &[hir::Expr<'_>],
        checked: bool,
    ) -> Option<(Vec<PackedPiece<'gcx>>, ValueId)> {
        // pieces = encode_packed_shape(args)
        // total += piece.size
        let mut total = self.builder.imm(0);
        let mut pieces = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.cx.gcx.type_of_expr(expr.id)?;
            if let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str().to_vec();
                let length = self.builder.imm(bytes.len() as u64);
                total = self.add_packed_total(total, length, checked);
                pieces.push(PackedPiece::Bytes(bytes));
                continue;
            }

            let memory_ty = ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
            let mut value = self.lower_typed_expr(expr, memory_ty)?;
            if let Some(abi_type) = self.types.abi_type(ty) {
                self.validate_calldata_bytes_argument(value, &abi_type);
                self.validate_calldata_array_head(value, ty, &abi_type);
            }
            if self.needs_calldata_aggregate_validation(value, ty) {
                value = self.materialize_calldata_argument(ty, value, expr.span)?;
            }
            if self.is_dynamic_bytes_type(ty) {
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
                total = self.add_packed_total(total, length, checked);
                let source = if is_slice {
                    value
                } else {
                    let pointer = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                    self.builder.make_slice(pointer, length, SliceLocation::Memory)
                };
                pieces.push(PackedPiece::Dynamic { source, length });
                continue;
            }

            if let Some((length, element, element_bytes, source)) =
                self.packed_array_shape(ty, value)
            {
                let element_bytes_value = self.builder.imm(element_bytes);
                let byte_length = self.builder.checked_mul(length, element_bytes_value);
                total = self.add_packed_total(total, byte_length, checked);
                pieces.push(PackedPiece::Array {
                    value,
                    length,
                    element: PackedArrayElement { abi: element.abi, ty: element.ty },
                    source,
                });
                continue;
            }

            let Some((length, fixed_bytes)) = self.packed_static_shape(ty) else {
                return self.cx.report_unsupported(expr.span, "abi.encodePacked argument");
            };
            let value = self.normalize_abi_scalar(value, ty);
            let signed = is_signed_packed_scalar(ty);
            let length_value = self.builder.imm(length);
            total = self.add_packed_total(total, length_value, checked);
            pieces.push(PackedPiece::Static { value, length, fixed_bytes, signed });
        }
        Some((pieces, total))
    }

    fn add_packed_total(&mut self, lhs: ValueId, rhs: ValueId, checked: bool) -> ValueId {
        if let (Some(lhs), Some(rhs)) =
            (self.builder.func().value_u256(lhs), self.builder.func().value_u256(rhs))
            && let Some(result) = lhs.checked_add(rhs)
        {
            return self.builder.imm(result);
        }
        if checked { self.builder.checked_add(lhs, rhs) } else { self.builder.add(lhs, rhs) }
    }

    fn packed_array_shape(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(ValueId, PackedArrayElement<'gcx>, u64, PackedArraySource)> {
        let element_ty = self.array_element_type(ty)?;
        let array_abi = self.types.abi_type(ty)?;
        let element_abi = match array_abi {
            AbiType::DynamicArray { element, .. } | AbiType::FixedArray { element, .. } => *element,
            _ => return None,
        };
        let element_bytes = Self::packed_array_element_bytes(&element_abi)?;

        let layout = self.types.memory_layout(ty)?;
        let source = match self.builder.func().value_ty(value) {
            Some(MirType::MemoryObject(
                MemoryObjectKind::DynamicArray | MemoryObjectKind::FixedArray,
            )) => PackedArraySource::Memory { layout },
            Some(MirType::Slice(location @ (SliceLocation::Memory | SliceLocation::Calldata))) => {
                PackedArraySource::Slice(location)
            }
            Some(MirType::UInt(size))
                if matches!(
                    layout,
                    MemoryObjectLayout::DynamicArray { .. } | MemoryObjectLayout::FixedArray { .. }
                ) && size.bits() == 256 =>
            {
                PackedArraySource::Memory { layout }
            }
            _ => return None,
        };
        let length = match source {
            PackedArraySource::Memory { layout: MemoryObjectLayout::DynamicArray { .. } } => {
                self.builder.memory_object_len(value, MemoryObjectKind::DynamicArray)
            }
            PackedArraySource::Memory { layout: MemoryObjectLayout::FixedArray { len, .. } } => {
                self.builder.imm(len)
            }
            PackedArraySource::Memory { .. } => return None,
            PackedArraySource::Slice(_) => self.builder.slice_len(value),
        };
        Some((
            length,
            PackedArrayElement { abi: element_abi, ty: element_ty },
            element_bytes,
            source,
        ))
    }

    fn packed_array_element_bytes(element: &AbiType) -> Option<u64> {
        match element {
            AbiType::Word(_) | AbiType::Function => Some(32),
            AbiType::FixedArray { element, len } => {
                Self::packed_array_element_bytes(element)?.checked_mul(*len)
            }
            AbiType::DynamicArray { .. } | AbiType::Bytes(_) | AbiType::Tuple(_) => None,
        }
    }

    pub(super) fn lower_packed_word_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        // length, element, width, source = packed_array_shape(value)
        // output = bytes(length * width)
        // copy_packed_array(output, 0, value, length, element, source)
        let (length, element, element_bytes, source) = self.packed_array_shape(ty, value)?;
        let word = self.builder.imm(32);
        let element_bytes_value = self.builder.imm(element_bytes);
        let byte_length = self.builder.checked_mul(length, element_bytes_value);
        let size = self.builder.checked_add(word, byte_length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, byte_length, MemoryObjectKind::Bytes);
        let offset = self.builder.imm(0);
        self.copy_packed_array(output, offset, value, length, &element, source);
        Some(output)
    }

    pub(super) fn lower_inplace_dynamic_value(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        // words = count_inline(value)
        // output = bytes(words * 32)
        // copy_inplace_dynamic_value(ty, value, output, 0)
        let nullable_memory = matches!(self.builder.func().value(value), Value::Inst(inst) if matches!(
            &self.builder.func().inst(*inst).kind,
            InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
        ));
        if !self.inplace_dynamic_shape(ty)
            || (!matches!(self.builder.func().value_ty(value), Some(MirType::MemoryObject(_)))
                && !nullable_memory)
        {
            return None;
        }
        let words = self.count_inplace_dynamic_value(ty, value, nullable_memory)?;
        let word = self.builder.imm(32);
        let length = self.builder.checked_mul(words, word);
        let size = self.builder.checked_add(word, length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, length, MemoryObjectKind::Bytes);
        let zero = self.builder.imm(0);
        self.copy_inplace_dynamic_value(ty, value, output, zero, nullable_memory)?;
        Some(output)
    }

    fn inplace_dynamic_shape(&mut self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => {
                self.inplace_dynamic_shape(element)
            }
            TyKind::Struct(id) => {
                self.cx.gcx.hir.strukt(id).fields.iter().all(|&field| {
                    self.inplace_dynamic_shape(self.cx.gcx.type_of_item(field.into()))
                })
            }
            TyKind::Fn(function) => function.is_external(),
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => true,
            TyKind::Udvt(inner, _) => self.inplace_dynamic_shape(inner),
            TyKind::Tuple(_) => false,
            TyKind::Slice(_) => false,
            _ => matches!(self.types.abi_type(ty), Some(AbiType::Word(_))),
        }
    }

    /// A zeroed aggregate slot stores null for its default memory object. Masking descendants
    /// prevents that null from being followed into scratch memory.
    fn memory_non_null_mask(&mut self, value: ValueId, nullable_memory: bool) -> Option<ValueId> {
        if !nullable_memory {
            return None;
        }
        let is_null = self.builder.iszero(value);
        Some(self.builder.iszero(is_null))
    }

    fn inplace_memory_object_len(
        &mut self,
        value: ValueId,
        kind: MemoryObjectKind,
        nullable_memory: bool,
    ) -> ValueId {
        let length = self.builder.memory_object_len(value, kind);
        if let Some(non_null) = self.memory_non_null_mask(value, nullable_memory) {
            self.builder.mul(length, non_null)
        } else {
            length
        }
    }

    fn count_inplace_dynamic_value(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        nullable_memory: bool,
    ) -> Option<ValueId> {
        // total = sum(count(child))
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => {
                self.count_inplace_array(ty, value, nullable_memory)
            }
            TyKind::Struct(id) => {
                let gcx = self.cx.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let layout = MemoryObjectLayout::structure(fields.len() as u64);
                let non_null = self.memory_non_null_mask(value, nullable_memory);
                let mut total = self.builder.imm(0);
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let mut field_value =
                        self.builder.memory_object_load_field(value, layout, index as u64);
                    if let Some(non_null) = non_null {
                        field_value = self.builder.mul(field_value, non_null);
                    }
                    let field_words = self.count_inplace_dynamic_value(field, field_value, true)?;
                    total = self.builder.checked_add(total, field_words);
                }
                Some(total)
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.count_inplace_bytes(value, nullable_memory)),
            TyKind::Udvt(inner, _) => {
                self.count_inplace_dynamic_value(inner, value, nullable_memory)
            }
            TyKind::Tuple(_) | TyKind::Slice(_) => None,
            _ => Some(self.builder.imm(1)),
        }
    }

    fn count_inplace_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        nullable_memory: bool,
    ) -> Option<ValueId> {
        // total = 0
        // for i { total += count(element[i]) }
        let (element, length, layout, non_null) =
            self.inplace_array_info(ty, value, nullable_memory)?;
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let total = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let mut element_value = self.builder.memory_object_load_element(value, layout, index);
        if let Some(non_null) = non_null {
            element_value = self.builder.mul(element_value, non_null);
        }
        let element_words = self.count_inplace_dynamic_value(element, element_value, true)?;
        let next_total = self.builder.checked_add(total, element_words);
        let next_index = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next_index);
        self.builder.add_phi_incoming(total, backedge, next_total);

        self.builder.switch_to_block(exit);
        Some(total)
    }

    fn count_inplace_bytes(&mut self, value: ValueId, nullable_memory: bool) -> ValueId {
        // words = ceil(bytes.length / 32)
        let length =
            self.inplace_memory_object_len(value, MemoryObjectKind::Bytes, nullable_memory);
        let word = self.builder.imm(32);
        let thirty_one = self.builder.imm(31);
        let rounded = self.builder.checked_add(length, thirty_one);
        let mask = self.builder.not(thirty_one);
        let padded = self.builder.and(rounded, mask);
        self.builder.div(padded, word)
    }

    fn copy_inplace_dynamic_value(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        output: ValueId,
        offset: ValueId,
        nullable_memory: bool,
    ) -> Option<ValueId> {
        // for child { offset = copy(child, offset) }
        let ty = ty.peel_refs();
        let value = match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => {
                return self.copy_inplace_array(ty, value, output, offset, nullable_memory);
            }
            TyKind::Struct(id) => {
                let gcx = self.cx.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let layout = MemoryObjectLayout::structure(fields.len() as u64);
                let non_null = self.memory_non_null_mask(value, nullable_memory);
                let mut offset = offset;
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let mut field_value =
                        self.builder.memory_object_load_field(value, layout, index as u64);
                    if let Some(non_null) = non_null {
                        field_value = self.builder.mul(field_value, non_null);
                    }
                    offset =
                        self.copy_inplace_dynamic_value(field, field_value, output, offset, true)?;
                }
                return Some(offset);
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => return Some(self.copy_inplace_bytes(value, output, offset, nullable_memory)),
            TyKind::Udvt(inner, _) => {
                return self.copy_inplace_dynamic_value(
                    inner,
                    value,
                    output,
                    offset,
                    nullable_memory,
                );
            }
            TyKind::Tuple(_) | TyKind::Slice(_) => return None,
            TyKind::Fn(function) if function.is_external() => {
                AbiWordValidator::from_mir_type(MirType::Function)
                    .expect("function words always require cleanup")
                    .cleanup(&mut self.builder, value)
            }
            _ => self.normalize_abi_scalar(value, ty),
        };
        self.builder.memory_object_store_word(output, offset, value);
        let word = self.builder.imm(32);
        Some(self.builder.checked_add(offset, word))
    }

    fn inplace_array_info(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        nullable_memory: bool,
    ) -> Option<(Ty<'gcx>, ValueId, MemoryObjectLayout, Option<ValueId>)> {
        let ty = ty.peel_refs();
        let layout = self.types.memory_layout(ty)?;
        let non_null = self.memory_non_null_mask(value, nullable_memory);
        let (element, length) = match ty.kind {
            TyKind::DynArray(element) => {
                let mut length = self.builder.memory_object_len(value, layout.kind());
                if let Some(non_null) = non_null {
                    length = self.builder.mul(length, non_null);
                }
                (element, length)
            }
            TyKind::Array(element, length) => {
                let length = self.builder.imm(u64::try_from(length).ok()?);
                (element, length)
            }
            _ => return None,
        };
        Some((element, length, layout, non_null))
    }

    fn copy_inplace_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        output: ValueId,
        offset: ValueId,
        nullable_memory: bool,
    ) -> Option<ValueId> {
        // for i { offset = copy(element[i], offset) }
        let ty = ty.peel_refs();
        let (element, length, layout, non_null) =
            self.inplace_array_info(ty, value, nullable_memory)?;
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let current_offset = self.builder.phi(vec![(preheader, offset)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let mut element_value = self.builder.memory_object_load_element(value, layout, index);
        if let Some(non_null) = non_null {
            element_value = self.builder.mul(element_value, non_null);
        }
        let next_offset =
            self.copy_inplace_dynamic_value(element, element_value, output, current_offset, true)?;
        let next_index = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next_index);
        self.builder.add_phi_incoming(current_offset, backedge, next_offset);

        self.builder.switch_to_block(exit);
        Some(current_offset)
    }

    fn copy_inplace_bytes(
        &mut self,
        value: ValueId,
        output: ValueId,
        offset: ValueId,
        nullable_memory: bool,
    ) -> ValueId {
        let length =
            self.inplace_memory_object_len(value, MemoryObjectKind::Bytes, nullable_memory);
        let word = self.builder.imm(32);
        let thirty_one = self.builder.imm(31);
        let rounded = self.builder.checked_add(length, thirty_one);
        let mask = self.builder.not(thirty_one);
        let padded = self.builder.and(rounded, mask);
        let empty = self.builder.iszero(padded);
        let zero_block = self.builder.create_block();
        let copy_block = self.builder.create_block();
        self.builder.branch(empty, copy_block, zero_block);

        self.builder.switch_to_block(zero_block);
        // if padded_length != 0 { mstore(output + offset + padded_length - 32, 0) }
        let last_offset = self.builder.sub(padded, word);
        let last = self.builder.add(offset, last_offset);
        let zero = self.builder.imm(0);
        self.builder.memory_object_store_word(output, last, zero);
        self.builder.jump(copy_block);

        self.builder.switch_to_block(copy_block);
        // copy(value, output + offset)
        // return_offset = offset + padded_length
        let data = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
        let source = self.builder.make_slice(data, length, SliceLocation::Memory);
        self.builder.memory_object_copy_from_slice_at(
            output,
            MemoryObjectKind::Bytes,
            offset,
            source,
        );
        self.builder.add(offset, padded)
    }

    fn copy_packed_array(
        &mut self,
        output: ValueId,
        offset: ValueId,
        value: ValueId,
        length: ValueId,
        element: &PackedArrayElement<'gcx>,
        source: PackedArraySource,
    ) -> ValueId {
        let element_bytes =
            Self::packed_array_element_bytes(&element.abi).expect("packed array shape");
        let element_bytes_value = self.builder.imm(element_bytes);
        let byte_length = self.builder.checked_mul(length, element_bytes_value);
        let end_offset = self.builder.checked_add(offset, byte_length);
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

        // for i in 0..length {
        //     destination = output + offset + i * element_width
        // }
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let element_offset = self.builder.checked_mul(index, element_bytes_value);
        let destination = self.builder.checked_add(offset, element_offset);
        match &element.abi {
            AbiType::Word(_) | AbiType::Function => {
                // element = normalize(load_element(value, i))
                // mstore(destination, element)
                let element_value = match source {
                    PackedArraySource::Memory { layout } => {
                        self.builder.memory_object_load_element(value, layout, index)
                    }
                    PackedArraySource::Slice(location) => match location {
                        SliceLocation::Memory => self.builder.memory_slice_load_word(
                            memory_source.expect("memory slice"),
                            element_offset,
                        ),
                        SliceLocation::Calldata => {
                            self.builder.calldata_slice_load_word(value, element_offset)
                        }
                        SliceLocation::Returndata => unreachable!("returndata packed array"),
                    },
                };
                let element_value = self.normalize_abi_scalar(element_value, element.ty);
                let element_value = if matches!(&element.abi, AbiType::Function)
                    && matches!(source, PackedArraySource::Memory { .. })
                {
                    AbiWordValidator::from_mir_type(MirType::Function)
                        .expect("function words always require cleanup")
                        .cleanup(&mut self.builder, element_value)
                } else {
                    element_value
                };
                self.builder.memory_object_store_word(output, destination, element_value);
            }
            AbiType::FixedArray { element: nested, len } => {
                // copy_packed_array(output, destination, value[i])
                let nested_length = self.builder.imm(*len);
                let (nested_value, nested_source) = match source {
                    PackedArraySource::Memory { layout } => {
                        let nested_value =
                            self.builder.memory_object_load_element(value, layout, index);
                        let nested_layout = MemoryObjectLayout::word_fixed_array(*len);
                        (nested_value, PackedArraySource::Memory { layout: nested_layout })
                    }
                    PackedArraySource::Slice(location) => {
                        let base = self.builder.slice_ptr(value);
                        let pointer = self.builder.add(base, element_offset);
                        let nested_value =
                            self.builder.make_slice(pointer, nested_length, location);
                        (nested_value, PackedArraySource::Slice(location))
                    }
                };
                let nested_ty = match element.ty.peel_refs().kind {
                    TyKind::Array(nested_ty, _) => nested_ty,
                    _ => element.ty,
                };
                let nested_element = PackedArrayElement { abi: (**nested).clone(), ty: nested_ty };
                self.copy_packed_array(
                    output,
                    destination,
                    nested_value,
                    nested_length,
                    &nested_element,
                    nested_source,
                );
            }
            AbiType::DynamicArray { .. } | AbiType::Bytes(_) | AbiType::Tuple(_) => {
                unreachable!("packed array shape")
            }
        }
        let next = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        end_offset
    }

    fn try_pack_packed_word(
        &mut self,
        pieces: &[PackedPiece<'gcx>],
    ) -> Option<(usize, u64, ValueId)> {
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
                    constant |= U256::from_be_slice(bytes) << usize::try_from(shift).unwrap();
                    length += piece_length;
                    consumed += 1;
                }
                PackedPiece::Static { value, length: piece_length, fixed_bytes: false, signed }
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
                    terms.push((*value, shift, *piece_length, *signed));
                    length += *piece_length;
                    consumed += 1;
                }
                _ => break,
            }
        }

        if consumed < 2 || length == 0 || terms.is_empty() {
            return None;
        }

        let mut value = self.builder.imm(constant);
        for (term, shift, size, signed) in terms {
            let term = if signed { self.mask_to_bits(term, (size * 8) as u16) } else { term };
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm(shift);
                self.builder.shl(shift, term)
            };
            value = self.builder.or(value, term);
        }
        Some((consumed, length, value))
    }

    fn try_write_packed_word(
        &mut self,
        output: ValueId,
        offset: ValueId,
        pieces: &[PackedPiece<'gcx>],
    ) -> Option<(usize, u64)> {
        let (consumed, length, value) = self.try_pack_packed_word(pieces)?;
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
            TyKind::Fn(function) if function.is_external() => Some((24, false)),
            TyKind::Enum(id) => {
                let variants = self.cx.gcx.hir.enumm(id).variants.len().max(1);
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
        let span = input.span;
        let memory_ty = self.cx.gcx.types.bytes_ref.memory;
        // input = materialize(bytes)
        let input = self.lower_typed_expr(input, memory_ty)?;
        let input = self.materialize_memory_argument(memory_ty, input, span)?;
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        let input_len = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);

        // output = bytes(32)
        // precompile_call(sha256 ? 2 : 3, input, output)
        let (output_ptr, output_len) = self.alloc_precompile_output();
        let address = self.builder.imm(if builtin == Builtin::Sha256 { 2 } else { 3 });
        let output_size = self.builder.imm(32);
        self.lower_precompile_call(address, input_ptr, input_len, output_ptr, output_size);
        let zero = self.builder.imm(0);
        let output_slice = self.builder.make_slice(output_ptr, output_len, SliceLocation::Memory);
        // result = mload(output.data)
        let value = self.builder.memory_slice_load_word(output_slice, zero);
        Some(if builtin == Builtin::Ripemd160 {
            // result = result << 96
            let scale = self.builder.imm(1_u128 << 96);
            self.builder.mul(scale, value)
        } else {
            value
        })
    }

    pub(super) fn lower_ecrecover_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        // input = bytes(160)
        // store(input, hash, 0)
        // store(input, v, 32)
        // store(input, r, 64)
        // store(input, s, 96)
        // precompile_call(1, input.data, 128, output.data, 32)
        // result = load(output, 0)
        let values = self.builtin_args::<4>(Builtin::EcRecover, &args)?;
        let hash = &values[0];
        let v = &values[1];
        let r = &values[2];
        let s = &values[3];
        let hash = self.lower_expr(hash)?;
        let v = self.lower_expr(v)?;
        let r = self.lower_expr(r)?;
        let s = self.lower_expr(s)?;

        let input_size = self.builder.imm(192);
        let input = self.builder.alloc_object(
            input_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let input_len = self.builder.imm(160);
        self.builder.set_memory_object_len(input, input_len, MemoryObjectKind::Bytes);
        let input_ptr = self.builder.memory_object_data(input, MemoryObjectKind::Bytes);
        let zero = self.builder.imm(0);
        self.builder.memory_object_store_word(input, zero, hash);
        for (offset, value) in [(32, v), (64, r), (96, s)] {
            let offset = self.builder.imm(offset);
            self.builder.memory_object_store_word(input, offset, value);
        }
        let (output_ptr, output_len) = self.alloc_precompile_output();

        let address = self.builder.imm(1);
        let input_size = self.builder.imm(128);
        let output_size = self.builder.imm(32);
        self.lower_precompile_call(address, input_ptr, input_size, output_ptr, output_size);
        let output_slice = self.builder.make_slice(output_ptr, output_len, SliceLocation::Memory);
        Some(self.builder.memory_slice_load_word(output_slice, zero))
    }

    fn alloc_precompile_output(&mut self) -> (ValueId, ValueId) {
        let size = self.builder.imm(64);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
        );
        let length = self.builder.imm(32);
        self.builder.set_memory_object_len(output, length, MemoryObjectKind::Bytes);
        let pointer = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);
        (pointer, length)
    }

    fn lower_precompile_call(
        &mut self,
        address: ValueId,
        input_ptr: ValueId,
        input_size: ValueId,
        output_ptr: ValueId,
        output_size: ValueId,
    ) {
        let evm_version = self.cx.gcx.sess.opts.evm_version;
        let gas = crate::utils::precompile_gas(&mut self.builder, evm_version);
        if evm_version.has_static_call() {
            // staticcall(precompile_gas, address, input, output)
            self.builder.staticcall(gas, address, input_ptr, input_size, output_ptr, output_size);
        } else {
            // call(precompile_gas, address, 0, input, output)
            let value = self.builder.imm(U256::ZERO);
            self.builder.call(gas, address, value, input_ptr, input_size, output_ptr, output_size);
        }
    }
}
