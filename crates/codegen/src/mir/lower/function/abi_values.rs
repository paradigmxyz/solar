//! ABI value and packed encoding helpers for one lowered function.

use super::*;

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
                    AbiType::Bytes(PtrLocation::Memory)
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
            && matches!(expr.peel_parens().kind, ExprKind::Lit(lit) if lit.kind.is_number())
        {
            let shift = self.builder.imm(224);
            return Some(self.builder.shl(value, shift));
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
        let signature_ty = self.cx.gcx.type_of_expr(signature.id)?;
        let memory_ty = signature_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        let signature = self.lower_typed_expr(signature, memory_ty)?;
        if let Some(abi_type) = self.types.abi_type(signature_ty) {
            self.validate_calldata_bytes_argument(signature, &abi_type);
        }
        let signature = match self.builder.func().value_ty(signature) {
            Some(MirType::Slice(_)) => self.materialize_memory_slice(signature),
            _ => signature,
        };
        let hash = self.builder.keccak256_bytes(signature);
        let shift = self.builder.imm(224);
        let selector = self.builder.shr(hash, shift);
        Some(self.builder.shl(selector, shift))
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
        let Some(TyKind::Fn(function_ty)) = self.cx.gcx.type_of_expr(function.id).map(|ty| ty.kind)
        else {
            return self.cx.report_unsupported(function.span, "abi.encodeCall function");
        };
        let selector = if let Some(function_id) = self.cx.gcx.resolved_function(function) {
            self.lower_discarded_expr(function)?;
            let selector = self.cx.gcx.function_selector(function_id).0;
            self.builder.imm(U256::from_be_slice(&selector) << 224)
        } else {
            if !function_ty.is_external() {
                return self.cx.report_unsupported(function.span, "abi.encodeCall function");
            }
            let function_value = self.lower_expr(function)?;
            let mask = self.builder.imm(u32::MAX);
            let selector = self.builder.and(function_value, mask);
            let shift = self.builder.imm(224);
            self.builder.shl(selector, shift)
        };
        let parameter_types = function_ty.parameters;
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
                let abi_type = this.types.abi_type(memory_ty)?;
                this.prepare_abi_encode_argument(expr, memory_ty, value, abi_type)
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = values_and_types.into_iter().unzip();
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some(self.builder.abi_encode_bytes(layout, Some(selector), values.into_boxed_slice()))
    }

    pub(super) fn canonicalize_abi_value(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let external_only = self.is_decoded_external_argument(value);
        let dirty = self.dirty_values.contains(&value);
        match ty.peel_refs().kind {
            // Aggregates are cleaned and validated word by word while encoding; copying them
            // into a canonical object first would duplicate the whole tree at every call site.
            TyKind::DynArray(_) | TyKind::Array(_, _) | TyKind::Struct(_) => value,
            _ => {
                let value = if external_only && !dirty {
                    value
                } else {
                    self.normalize_abi_scalar(value, ty)
                };
                let layout = types::TypeLowerer::value_layout(ty);
                if matches!(layout, crate::mir::ValueLayout::Int(_)) {
                    raw_scalars::cast_carrier(&mut self.builder, value, layout, MirType::I256)
                } else {
                    value
                }
            }
        }
    }

    pub(super) fn lower_abi_decode(&mut self, args: hir::CallArgs<'_>) -> Option<CallResult> {
        // data = input (a calldata slice) | materialize_memory_argument(input)
        // layout = intern_abi_layout(target_types)
        // value = abi_decode(layout, data)
        let args = self.builtin_args::<2>(Builtin::AbiDecode, &args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => {
                return self.cx.report_unsupported(args[1].span, "abi.decode target type");
            }
        };
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
        // `abi_decode` reads a calldata slice in place, so calldata needs no copy.
        let data = if lies_in_calldata(data_ty) {
            self.lower_expr(data_expr)?
        } else {
            let data = self.lower_typed_expr(data_expr, memory_ty)?;
            self.materialize_memory_argument(memory_ty, data, data_expr.span)?
        };
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
        let fields = decoded_types.iter().map(|&ty| types::TypeLowerer::mir_type(ty)).collect();
        let Some(result_ty) = self.cx.module.intern_return_type(fields) else {
            self.builder.abi_decode_void(layout, data);
            return Some(CallResult::Void);
        };
        Some(CallResult::Value(self.builder.abi_decode(layout, data, result_ty)))
    }

    fn lower_abi_decode_layout(
        &mut self,
        data: ValueId,
        types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<(ValueId, AbiParamLayout)> {
        // `abi_decode` reads a memory or calldata slice in place; returndata needs a copy.
        let data = match self.builder.func().value_ty(data) {
            Some(MirType::Slice(PtrLocation::Returndata)) => self.materialize_memory_slice(data),
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
        let decoded_types = types
            .iter()
            .copied()
            .map(|ty| types::TypeLowerer::return_encoding_ty(self.cx.gcx, ty))
            .collect::<Vec<_>>();
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
        let fields = decoded_types.iter().map(|&ty| types::TypeLowerer::mir_type(ty)).collect();
        let result_ty = self.cx.module.intern_return_type(fields)?;
        // result = abi_decode(layout, data)
        // fields = extract_value result, 0; ...
        let value = self.builder.abi_decode(layout, data, result_ty);
        Some(self.unpack_return_value(value))
    }

    pub(super) fn materialize_memory_slice(&mut self, slice: ValueId) -> ValueId {
        // object = bytes(slice.len)
        // copy(slice, object.data)
        // return object
        let length = self.builder.slice_len(slice);
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        self.builder.memory_copy_from_slice(object, slice);
        object
    }

    pub(super) fn materialize_returndata_bytes(&mut self) -> ValueId {
        // object = returndata_bytes
        self.builder.returndata_bytes()
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
        // payload = data[4:]
        // message = abi_decode(bytes, payload)
        let data_len = self.builder.memory_len(data);
        let four = self.builder.imm(4);
        let payload_ptr = self.builder.memory_data_at(data, 4);
        let payload_len = self.builder.sub(data_len, four);
        let payload = self.builder.make_slice(payload_ptr, payload_len, PtrLocation::Memory);
        let layout = self.cx.module.intern_abi_param_layout(AbiParamLayout::new(
            vec![AbiParamType::Bytes].into_boxed_slice(),
        ));
        Some(self.builder.abi_decode(layout, payload, MirType::MEMPTR))
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
        // pointer = ptrtoint data to i256
        // valid = icall try_decode_error_message, pointer, length
        let data_ptr = self.builder.cast(data_ptr, MirType::I256);
        let valid = self.builder.icall(helper, vec![data_ptr, data_len], MirType::I1);
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
        // valid &= offset <= len - 36
        // msg_len = mload(data + 4 + offset)
        // valid &= msg_len <= u64::MAX && msg_len <= len - (offset + 36)
        self.lazy_helper(sym::try_decode_error_message, |this, function| {
            let target = this.cx.module.target_of(this.cx.gcx, function);
            let mut builder = FunctionBuilder::new_semantic(function).with_target(target);
            let data_ptr = builder.add_param(MirType::I256);
            let data_len = builder.add_param(MirType::I256);
            builder.set_return_type(MirType::I1);

            let check_offset = builder.create_block();
            let check_length = builder.create_block();
            let no_match = builder.create_block();

            let min_size = builder.imm(68);
            let short = builder.lt(data_len, min_size);
            let has_head = builder.eq_zero(short);
            builder.branch(has_head, check_offset, no_match);

            builder.switch_to_block(check_offset);
            let payload_ptr = builder.add_u64_offset(data_ptr, 4);
            let offset = builder.load(payload_ptr, PtrLocation::Memory);
            // `data_len >= 68` from the check above, so `data_len - 36` cannot wrap. It also
            // rejects offsets above `u64::MAX`, as returndata is far shorter.
            let head_size = builder.imm(36);
            let max_offset = builder.sub(data_len, head_size);
            let invalid_offset = builder.gt(offset, max_offset);
            builder.branch(invalid_offset, no_match, check_length);

            builder.switch_to_block(check_length);
            let message_ptr = builder.add(payload_ptr, offset);
            let length = builder.load(message_ptr, PtrLocation::Memory);
            let message_data_offset = builder.add_u64_offset(offset, 36);
            let length_too_large = builder.exceeds_bits(length, 64, ConditionUse::VALUE);
            let remaining = builder.sub(data_len, message_data_offset);
            let data_out_of_range = builder.gt(length, remaining);
            let invalid_length = builder.or(length_too_large, data_out_of_range);
            let valid = builder.eq_zero(invalid_length);
            builder.ret([valid]);

            builder.switch_to_block(no_match);
            let no_match = builder.imm_bool(false);
            builder.ret([no_match]);
            Some(())
        })
        .expect("error catch match helper construction cannot fail")
    }

    pub(super) fn lower_panic_catch_word(&mut self, data: ValueId) -> ValueId {
        let payload_ptr = self.builder.memory_data_at(data, 4);
        self.builder.load(payload_ptr, PtrLocation::Memory)
    }

    pub(super) fn lower_abi_encode_packed(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(builtin, &args)?;
        let parts = self.lower_packed_parts(exprs)?;
        // output = abi_encode_packed(parts)
        Some(
            self.builder
                .emit_inst(InstKind::AbiEncodePacked { parts, hash: false }, Some(MirType::MEMPTR)),
        )
    }

    pub(super) fn lower_keccak_abi_encode_packed(
        &mut self,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        if !exprs.iter().all(|expr| self.is_scratch_packed_expr(expr)) {
            return None;
        }
        let parts = self.lower_packed_parts(exprs)?;
        // hash = keccak256_packed(parts)
        Some(
            self.builder
                .emit_inst(InstKind::AbiEncodePacked { parts, hash: true }, Some(MirType::I256)),
        )
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

    fn lower_packed_parts(&mut self, exprs: &[hir::Expr<'_>]) -> Option<Box<[PackedPart]>> {
        let mut parts = Vec::with_capacity(exprs.len());
        // values = evaluate_arguments_in_order(args)
        // parts = describe_packed_shapes(values)
        for expr in exprs {
            let ty = self.cx.gcx.type_of_expr(expr.id)?;
            if let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str();
                // Adjacent literals pack into one.
                if let Some(PackedPart::Literal(last)) = parts.last_mut() {
                    *last = [&last[..], bytes].concat().into();
                } else {
                    parts.push(PackedPart::Literal(bytes.to_vec().into()));
                }
                continue;
            }
            let memory_ty = ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
            let mut value = self.lower_typed_expr(expr, memory_ty)?;
            let abi_type = self.types.abi_type(ty);
            if let Some(abi_type) = &abi_type {
                self.check_calldata_array_size(value, ty, abi_type);
            }
            if self.needs_calldata_aggregate_validation(value, ty) {
                // Packed encoding validates words even for ABI-decoded arguments, but static
                // arrays retain zero-padded calldata reads instead of checking the source range.
                if abi_type.as_ref().is_some_and(|abi_type| !abi_type.is_dynamic()) {
                    // validate_static(ty, slice_ptr(value))
                    let base = self.builder.slice_ptr(value);
                    self.validate_calldata_static_value(ty, base);
                } else if abi_type.as_ref().is_some_and(Self::is_calldata_word_array) {
                    self.validate_calldata_word_array(value, ty);
                } else {
                    value = self.materialize_calldata_argument(ty, value, expr.span)?;
                }
            }
            if self.is_dynamic_bytes_type(ty) {
                parts.push(PackedPart::Bytes(value));
            } else if let Some((element, source)) = self.packed_array_shape(ty, value) {
                parts.push(PackedPart::Array { value, element, source });
            } else {
                let Some((length, fixed_bytes)) = self.packed_static_shape(ty) else {
                    return self.cx.report_unsupported(expr.span, "abi.encodePacked argument");
                };
                let value = self.normalize_abi_scalar(value, ty);
                let size = TypeSize::new_int_bits((length * 8) as u16);
                let ty = if fixed_bytes {
                    crate::mir::ValueLayout::FixedBytes(size)
                } else if is_signed_packed_scalar(ty) {
                    crate::mir::ValueLayout::Int(size)
                } else {
                    crate::mir::ValueLayout::UInt(size)
                };
                parts.push(PackedPart::Scalar { value, ty });
            }
        }
        Some(parts.into_boxed_slice())
    }

    fn packed_array_shape(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(AbiType, PackedArraySource)> {
        let array_abi = self.types.abi_type(ty)?;
        let element_abi = match array_abi {
            AbiType::DynamicArray { element, .. } | AbiType::FixedArray { element, .. } => *element,
            _ => return None,
        };
        crate::mir::packed_element_bytes(&element_abi)?;

        let layout = self.types.memory_layout(ty)?;
        let source = match self.builder.func().value_ty(value) {
            Some(MirType::MEMPTR) => PackedArraySource::Memory { layout },
            Some(MirType::Slice(location @ (PtrLocation::Memory | PtrLocation::Calldata))) => {
                PackedArraySource::Slice(location)
            }
            Some(MirType::I256)
                if matches!(
                    layout,
                    MemoryObjectLayout::DynamicArray { .. } | MemoryObjectLayout::FixedArray { .. }
                ) =>
            {
                PackedArraySource::Memory { layout }
            }
            _ => return None,
        };
        Some((element_abi, source))
    }

    pub(super) fn lower_packed_word_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        let (element, source) = self.packed_array_shape(ty, value)?;
        // output = abi_encode_packed(array(value))
        Some(self.builder.emit_inst(
            InstKind::AbiEncodePacked {
                parts: Box::new([PackedPart::Array { value, element, source }]),
                hash: false,
            },
            Some(MirType::MEMPTR),
        ))
    }

    pub(super) fn lower_inplace_dynamic_value(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        // words = count_inline(value)
        // output = bytes(words * 32)
        // copy_inplace_dynamic_value(ty, value, output, 0)
        if !self.inplace_dynamic_shape(ty)
            || (!matches!(self.builder.func().value_ty(value), Some(MirType::MEMPTR))
                && !self.builder.func().memory_object_load(value))
        {
            return None;
        }
        let words = self.count_inplace_dynamic_value(ty, value)?;
        let word = self.builder.imm(32);
        let length = self.builder.checked_mul(words, word);
        let size = self.builder.checked_add(word, length);
        let output = self.builder.alloc(size, AllocationSemantics::INTERNAL);
        self.builder.set_memory_len(output, length);
        let zero = self.builder.imm(0);
        self.copy_inplace_dynamic_value(ty, value, output, zero)?;
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

    fn count_inplace_dynamic_value(&mut self, ty: Ty<'gcx>, value: ValueId) -> Option<ValueId> {
        // total = sum(count(child))
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => self.count_inplace_array(ty, value),
            TyKind::Struct(id) => {
                let gcx = self.cx.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let mut total = self.builder.imm(0);
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let field_value = self.builder.memory_load_field(value, index as u64);
                    let field_words = self.count_inplace_dynamic_value(field, field_value)?;
                    total = self.builder.checked_add(total, field_words);
                }
                Some(total)
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.count_inplace_bytes(value)),
            TyKind::Udvt(inner, _) => self.count_inplace_dynamic_value(inner, value),
            TyKind::Tuple(_) | TyKind::Slice(_) => None,
            _ => Some(self.builder.imm(1)),
        }
    }

    fn count_inplace_array(&mut self, ty: Ty<'gcx>, value: ValueId) -> Option<ValueId> {
        // total = 0
        // for i { total += count(element[i]) }
        let (element, length, layout) = self.inplace_array_info(ty, value)?;
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
        let element_value = self.builder.memory_load_element(value, layout, index);
        let element_words = self.count_inplace_dynamic_value(element, element_value)?;
        let next_total = self.builder.checked_add(total, element_words);
        let next_index = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next_index);
        self.builder.add_phi_incoming(total, backedge, next_total);

        self.builder.switch_to_block(exit);
        Some(total)
    }

    fn count_inplace_bytes(&mut self, value: ValueId) -> ValueId {
        // words = ceil(bytes.length / 32)
        let length = self.builder.memory_len(value);
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
    ) -> Option<ValueId> {
        // for child { offset = copy(child, offset) }
        let ty = ty.peel_refs();
        let value = match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => {
                return self.copy_inplace_array(ty, value, output, offset);
            }
            TyKind::Struct(id) => {
                let gcx = self.cx.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let mut offset = offset;
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let field_value = self.builder.memory_load_field(value, index as u64);
                    offset = self.copy_inplace_dynamic_value(field, field_value, output, offset)?;
                }
                return Some(offset);
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => return Some(self.copy_inplace_bytes(value, output, offset)),
            TyKind::Udvt(inner, _) => {
                return self.copy_inplace_dynamic_value(inner, value, output, offset);
            }
            TyKind::Tuple(_) | TyKind::Slice(_) => return None,
            TyKind::Fn(function) if function.is_external() => {
                AbiWordValidator::from_layout(crate::mir::ValueLayout::Function)
                    .expect("function words always require cleanup")
                    .cleanup(&mut self.builder, value)
            }
            _ => {
                let value = self.normalize_abi_scalar(value, ty);
                raw_scalars::cast_carrier(
                    &mut self.builder,
                    value,
                    types::TypeLowerer::value_layout(ty),
                    MirType::I256,
                )
            }
        };
        self.builder.memory_store_word(output, offset, value);
        let word = self.builder.imm(32);
        Some(self.builder.checked_add(offset, word))
    }

    fn inplace_array_info(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(Ty<'gcx>, ValueId, MemoryObjectLayout)> {
        let ty = ty.peel_refs();
        let layout = self.types.memory_layout(ty)?;
        let (element, length) = match ty.kind {
            TyKind::DynArray(element) => (element, self.builder.memory_len(value)),
            TyKind::Array(element, length) => {
                let length = self.builder.imm(u64::try_from(length).ok()?);
                (element, length)
            }
            _ => return None,
        };
        Some((element, length, layout))
    }

    fn copy_inplace_array(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
        output: ValueId,
        offset: ValueId,
    ) -> Option<ValueId> {
        // for i { offset = copy(element[i], offset) }
        let ty = ty.peel_refs();
        let (element, length, layout) = self.inplace_array_info(ty, value)?;
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
        let element_value = self.builder.memory_load_element(value, layout, index);
        let next_offset =
            self.copy_inplace_dynamic_value(element, element_value, output, current_offset)?;
        let next_index = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next_index);
        self.builder.add_phi_incoming(current_offset, backedge, next_offset);

        self.builder.switch_to_block(exit);
        Some(current_offset)
    }

    fn copy_inplace_bytes(&mut self, value: ValueId, output: ValueId, offset: ValueId) -> ValueId {
        let length = self.builder.memory_len(value);
        let word = self.builder.imm(32);
        let thirty_one = self.builder.imm(31);
        let rounded = self.builder.checked_add(length, thirty_one);
        let mask = self.builder.not(thirty_one);
        let padded = self.builder.and(rounded, mask);
        let empty = self.builder.eq_zero(padded);
        let zero_block = self.builder.create_block();
        let copy_block = self.builder.create_block();
        self.builder.branch(empty, copy_block, zero_block);

        self.builder.switch_to_block(zero_block);
        // if padded_length != 0 { mstore(output + offset + padded_length - 32, 0) }
        let last_offset = self.builder.sub(padded, word);
        let last = self.builder.add(offset, last_offset);
        let zero = self.builder.imm(0);
        self.builder.memory_store_word(output, last, zero);
        self.builder.jump(copy_block);

        self.builder.switch_to_block(copy_block);
        // copy(value, output + offset)
        // return_offset = offset + padded_length
        let data = self.builder.memory_data(value);
        let source = self.builder.make_slice(data, length, PtrLocation::Memory);
        self.builder.memory_copy_from_slice_at(output, offset, source);
        self.builder.add(offset, padded)
    }

    fn packed_static_shape(&self, ty: Ty<'gcx>) -> Option<(u64, bool)> {
        match ty.peel_refs().kind {
            TyKind::Elementary(elementary) => Some(match elementary {
                solar_sema::hir::ElementaryType::Bool => (1, false),
                solar_sema::hir::ElementaryType::Address(_) => (20, false),
                solar_sema::hir::ElementaryType::Int(size)
                | solar_sema::hir::ElementaryType::UInt(size) => (u64::from(size.bytes()), false),
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
        let input_ty = self.cx.gcx.type_of_expr(input.id)?;
        let input = if lies_in_calldata(input_ty) {
            let input = self.lower_expr(input)?;
            self.copy_calldata_bytes_unchecked(input)?
        } else {
            let input = self.lower_typed_expr(input, memory_ty)?;
            self.materialize_memory_argument(memory_ty, input, span)?
        };
        // result = sha256(input) / ripemd160(input)
        let kind = if builtin == Builtin::Sha256 {
            InstKind::builtin(crate::mir::Builtin::Sha256, [input])
        } else {
            InstKind::builtin(crate::mir::Builtin::Ripemd160, [input])
        };
        Some(self.builder.emit_inst(kind, Some(MirType::I256)))
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

        // result = ecrecover(hash, v, r, s)
        Some(self.builder.emit_inst(
            InstKind::builtin(crate::mir::Builtin::EcRecover, [hash, v, r, s]),
            Some(MirType::I256),
        ))
    }

    /// `keccak256(abi.encode(..))` of one or two value-type words, hashed in scratch space like
    /// the packed form. Function values and untyped literals keep the general encoder.
    pub(super) fn lower_keccak_abi_encode_words(
        &mut self,
        exprs: &[hir::Expr<'_>],
    ) -> Option<ValueId> {
        if !(1..=2).contains(&exprs.len()) {
            return None;
        }
        let tys = exprs
            .iter()
            .map(|expr| self.cx.gcx.type_of_expr(expr.id))
            .collect::<Option<Vec<_>>>()?;
        if !tys
            .iter()
            .all(|ty| ty.is_value_type() && !matches!(ty.kind, TyKind::Fn(_) | TyKind::Super(_)))
        {
            return None;
        }
        // words = clean(evaluate_arguments_in_order(args))
        let parts = exprs
            .iter()
            .zip(tys)
            .map(|(expr, ty)| {
                let value = self.lower_typed_expr(expr, ty)?;
                let value = self.canonicalize_abi_value(ty, value);
                let value = self.builder.cast_word(value);
                Some(PackedPart::Scalar { value, ty: crate::mir::ValueLayout::uint256() })
            })
            .collect::<Option<Box<[_]>>>()?;
        // hash = keccak256_packed(words)
        Some(
            self.builder
                .emit_inst(InstKind::AbiEncodePacked { parts, hash: true }, Some(MirType::I256)),
        )
    }
}

/// Whether the type is a calldata reference or a slice of one.
fn lies_in_calldata(ty: Ty<'_>) -> bool {
    match ty.kind {
        TyKind::Slice(inner) => inner.is_ref_at(DataLocation::Calldata),
        _ => ty.is_ref_at(DataLocation::Calldata),
    }
}
