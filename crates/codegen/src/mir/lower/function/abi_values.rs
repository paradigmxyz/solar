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
        let fields =
            decoded_types.iter().map(|&ty| types::TypeLowerer::mir_return_type(ty)).collect();
        let result_ty = self.cx.module.intern_return_type(fields)?;
        Some(self.builder.abi_decode(layout, data, result_ty))
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
        let decoded_types = types
            .iter()
            .copied()
            .map(|ty| types::TypeLowerer::return_encoding_ty(self.cx.gcx, ty))
            .collect::<Vec<_>>();
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
        let fields =
            decoded_types.iter().map(|&ty| types::TypeLowerer::mir_return_type(ty)).collect();
        let result_ty = self.cx.module.intern_return_type(fields)?;
        // result = abi_decode(layout, data)
        // fields = extract_value result, 0; ...
        let value = self.builder.abi_decode(layout, data, result_ty);
        Some(self.unpack_return_value(value))
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
        Some(self.builder.abi_decode(
            layout,
            payload,
            MirType::MemoryObject(MemoryObjectKind::Bytes),
        ))
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
        let valid = self.builder.icall(helper, vec![data_ptr, data_len], MirType::Bool);
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
            let mut builder = FunctionBuilder::new_semantic(function);
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
        let parts = self.lower_packed_parts(exprs)?;
        // output = abi_encode_packed(parts)
        Some(self.builder.emit_inst(
            InstKind::AbiEncodePacked { parts, hash: false },
            Some(MirType::MemoryObject(MemoryObjectKind::Bytes)),
        ))
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
            self.builder.emit_inst(
                InstKind::AbiEncodePacked { parts, hash: true },
                Some(MirType::bytes32()),
            ),
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
                parts.push(PackedPart::Literal(bytes.as_byte_str().to_vec().into()));
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
                    MirType::FixedBytes(size)
                } else if is_signed_packed_scalar(ty) {
                    MirType::Int(size)
                } else {
                    MirType::UInt(size)
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
            Some(MirType::MemoryObject(MemoryObjectKind::Bytes)),
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
        // result = sha256(input) / ripemd160(input)
        let kind = if builtin == Builtin::Sha256 {
            InstKind::Sha256(input)
        } else {
            InstKind::Ripemd160(input)
        };
        Some(self.builder.emit_inst(kind, Some(MirType::uint256())))
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
        Some(self.builder.emit_inst(InstKind::EcRecover(hash, v, r, s), Some(MirType::uint256())))
    }
}
