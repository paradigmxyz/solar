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
            let ty = self.context.gcx.type_of_expr(expr.id)?;
            let memory_ty = ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
            let value = self.lower_typed_expr(expr, memory_ty)?;
            let abi_type = if matches!(ty.peel_refs().kind, TyKind::StringLiteral(..)) {
                AbiType::Bytes(SliceLocation::Memory)
            } else {
                self.types.abi_type(ty)?
            };
            let (value, abi_type) = self.prepare_abi_encode_argument(expr, ty, value, abi_type)?;
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        Some(self.builder.abi_encode(layout, selector, values.into_boxed_slice()))
    }

    pub(super) fn lower_selector_word(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let value = if let ExprKind::Lit(lit) = expr.peel_parens().kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            self.lower_string_literal_word(bytes.as_byte_str())
        } else {
            self.lower_expr(expr)?
        };
        let fixed_bytes = self.context.gcx.type_of_expr(expr.id).is_some_and(|ty| {
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

        let signature_ty = self.context.gcx.type_of_expr(signature.id);
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
        let shift = self.builder.imm_u64(224);
        let selector = self.builder.shr(shift, hash);
        Some(self.builder.shl(shift, selector))
    }

    pub(super) fn lower_abi_encode_call(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiEncodeCall, &args)?;
        let function = &args[0];
        let tuple = &args[1];
        let (selector, parameter_types) =
            if let Some(function_id) = self.context.gcx.resolved_function(function) {
                let selector = self.context.gcx.function_selector(function_id).0;
                let parameter_types = self
                    .context
                    .gcx
                    .hir
                    .function(function_id)
                    .parameters
                    .iter()
                    .map(|&parameter| self.context.gcx.type_of_item(parameter.into()))
                    .collect::<Vec<_>>();
                (self.builder.imm_u256(U256::from_be_slice(&selector) << 224), parameter_types)
            } else {
                let Some(TyKind::Fn(function_ty)) =
                    self.context.gcx.type_of_expr(function.id).map(|ty| ty.kind)
                else {
                    return report_unsupported(
                        self.context.gcx,
                        function.span,
                        "abi.encodeCall function",
                    );
                };
                if !function_ty.is_external() {
                    return report_unsupported(
                        self.context.gcx,
                        function.span,
                        "abi.encodeCall function",
                    );
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
            return report_unsupported(
                self.context.gcx,
                tuple.span,
                "abi.encodeCall argument list",
            );
        }
        let mut values = Vec::with_capacity(exprs.len());
        let mut types = Vec::with_capacity(exprs.len());
        for (index, expr) in exprs.into_iter().enumerate() {
            let ty = parameter_types[index];
            let memory_ty = ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
            let value = self.lower_typed_expr(expr, memory_ty)?;
            let abi_type = self.types.abi_type(ty)?;
            let (value, abi_type) = self.prepare_abi_encode_argument(expr, ty, value, abi_type)?;
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        Some(self.materialize_memory_slice(encoded))
    }

    pub(super) fn canonicalize_abi_value(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if !self.is_external_abi_argument(value) {
            self.validate_enum(ty, value);
        }
        match ty.peel_refs().kind {
            TyKind::DynArray(_) | TyKind::Array(_, _) => self.canonicalize_abi_array(ty, value),
            TyKind::Struct(_) => self.canonicalize_abi_struct(ty, value),
            _ => self.normalize_abi_scalar(value, ty),
        }
    }

    fn value_is_canonical(&self, value: ValueId, seen: &mut FxHashSet<ValueId>) -> bool {
        if !seen.insert(value) {
            return true;
        }
        let Value::Inst(inst) = self.builder.func().value(value) else { return false };
        let kind = &self.builder.func().inst(*inst).kind;
        match kind {
            InstKind::InternalCall { function, args, .. } => {
                // An aggregate argument may carry dirty ABI words into an
                // otherwise pure helper. Keep the cleanup in that case.
                if args.iter().any(|&arg| {
                    matches!(self.builder.func().value_ty(arg), Some(MirType::MemoryObject(_)))
                }) {
                    return false;
                }
                let function = self.context.module.function(*function);
                !function.instructions().any(|inst| {
                    matches!(
                        function.inst(inst).kind,
                        InstKind::MStore(..)
                            | InstKind::MStore8(..)
                            | InstKind::CalldataCopy(..)
                            | InstKind::CodeCopy(..)
                            | InstKind::ReturnDataCopy(..)
                            | InstKind::ExtCodeCopy(..)
                            | InstKind::MCopy(..)
                    )
                })
            }
            InstKind::MemoryObjectLoadField { object, .. }
            | InstKind::MemoryObjectLoadElement { object, .. } => {
                self.value_is_canonical(*object, seen)
            }
            InstKind::Phi(incoming) => {
                incoming.iter().all(|(_, value)| self.value_is_canonical(*value, seen))
            }
            _ => false,
        }
    }

    fn canonicalize_abi_array(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if self.is_external_abi_argument(value)
            && self.builder.func().attributes.visibility == solar_ast::Visibility::External
        {
            return value;
        }
        if self.value_is_canonical(value, &mut FxHashSet::default()) {
            return value;
        }
        let element_ty = match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => element,
            _ => return value,
        };
        if !self.abi_value_needs_normalization(element_ty) {
            return value;
        }

        let layout = match self.types.memory_layout(ty) {
            Some(layout @ MemoryObjectLayout::DynamicArray { element_words: 1 })
            | Some(layout @ MemoryObjectLayout::FixedArray { element_words: 1, .. }) => layout,
            _ => return value,
        };
        if !self.is_memory_object_value(value, layout.kind()) {
            return value;
        }
        let dynamic = matches!(layout, MemoryObjectLayout::DynamicArray { .. });
        let length = self.memory_object_length(value, layout);
        let words = if dynamic {
            let one = self.builder.imm_u64(1);
            self.builder.checked_add(length, one)
        } else {
            length
        };
        let word = self.builder.imm_u64(32);
        let size = self.builder.checked_mul(words, word);
        let output = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        if dynamic {
            self.builder.set_memory_object_len(output, length, layout.kind());
        }

        if !matches!(
            element_ty.peel_refs().kind,
            TyKind::DynArray(_) | TyKind::Array(_, _) | TyKind::Struct(_)
        ) {
            let source = self.builder.memory_object_data(value, layout.kind());
            let destination = self.builder.memory_object_data(output, layout.kind());
            let preheader = self.builder.current_block();
            let header = self.builder.create_block();
            let body = self.builder.create_block();
            let exit = self.builder.create_block();
            self.builder.jump(header);

            self.builder.switch_to_block(header);
            let zero = self.builder.imm_u64(0);
            let index = self.builder.phi(vec![(preheader, zero)]);
            let source = self.builder.phi(vec![(preheader, source)]);
            let destination = self.builder.phi(vec![(preheader, destination)]);
            let more = self.builder.lt(index, length);
            self.builder.branch(more, body, exit);

            self.builder.switch_to_block(body);
            let element_value = self.builder.mload(source);
            self.validate_enum(element_ty, element_value);
            let element_value = self.normalize_abi_scalar(element_value, element_ty);
            self.builder.mstore(destination, element_value);
            let word = self.builder.imm_u64(32);
            let next_source = self.builder.add(source, word);
            let next_destination = self.builder.add(destination, word);
            let next_index = self.builder.add_u64_offset(index, 1);
            let backedge = self.builder.current_block();
            self.builder.jump(header);
            self.builder.add_phi_incoming(index, backedge, next_index);
            self.builder.add_phi_incoming(source, backedge, next_source);
            self.builder.add_phi_incoming(destination, backedge, next_destination);

            self.builder.switch_to_block(exit);
            return output;
        }

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
        let element_value = self.builder.memory_object_load_element(value, layout, index);
        let element_value = self.canonicalize_abi_value(element_ty, element_value);
        self.builder.memory_object_store_element(output, layout, index, element_value);
        let next = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        output
    }

    fn canonicalize_abi_struct(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if self.value_is_canonical(value, &mut FxHashSet::default()) {
            return value;
        }
        if !self.abi_value_needs_normalization(ty) {
            return value;
        }
        let Some(layout @ MemoryObjectLayout::Struct { .. }) = self.types.memory_layout(ty) else {
            return value;
        };
        if !self.is_memory_object_value(value, MemoryObjectKind::Struct) {
            return value;
        }
        let TyKind::Struct(id) = ty.peel_refs().kind else { unreachable!("struct layout checked") };
        let gcx = self.context.gcx;
        let fields = gcx.hir.strukt(id).fields;
        let Some(size) = u64::try_from(fields.len()).ok().and_then(|len| len.checked_mul(32))
        else {
            return value;
        };
        let size = self.builder.imm_u64(size);
        let output = self.builder.alloc_object(size, layout, AllocationSemantics::INTERNAL);
        for (index, &field) in fields.iter().enumerate() {
            let field_ty = gcx.type_of_item(field.into());
            let field_value = self.builder.memory_object_load_field(value, layout, index as u64);
            let field_value = self.canonicalize_abi_value(field_ty, field_value);
            self.builder.memory_object_store_field(output, layout, index as u64, field_value);
        }
        output
    }

    fn is_memory_object_value(&self, value: ValueId, kind: MemoryObjectKind) -> bool {
        match self.builder.func().value_ty(value) {
            Some(MirType::MemoryObject(value_kind)) => value_kind == kind,
            Some(MirType::UInt(size)) => size.bits() == 256,
            _ => false,
        }
    }

    fn abi_value_needs_normalization(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => {
                self.abi_value_needs_normalization(element)
            }
            TyKind::Struct(id) => self.context.gcx.hir.strukt(id).fields.iter().any(|&field| {
                self.abi_value_needs_normalization(self.context.gcx.type_of_item(field.into()))
            }),
            TyKind::Udvt(inner, _) => self.abi_value_needs_normalization(inner),
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::UInt(size)
                | solar_sema::hir::ElementaryType::Int(size),
            ) => size.bits() < 256,
            TyKind::Elementary(solar_sema::hir::ElementaryType::Address(_))
            | TyKind::Contract(_)
            | TyKind::Enum(_)
            | TyKind::Elementary(solar_sema::hir::ElementaryType::Bool) => true,
            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) => {
                size.bytes() < 32
            }
            _ => false,
        }
    }

    pub(super) fn lower_abi_decode(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let args = self.builtin_args::<2>(Builtin::AbiDecode, &args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => {
                return report_unsupported(
                    self.context.gcx,
                    args[1].span,
                    "abi.decode target type",
                );
            }
        };
        if types.is_empty() {
            return report_unsupported(self.context.gcx, args[1].span, "abi.decode target type");
        }
        let mut decoded_types = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) =
                self.context.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind)
            else {
                return report_unsupported(
                    self.context.gcx,
                    ty_expr.span,
                    "abi.decode target type",
                );
            };
            decoded_types.push(ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory));
        }

        let data_expr = &args[0];
        let data_ty = self.context.gcx.type_of_expr(data_expr.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
        let data = self.lower_typed_expr(data_expr, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_expr.span)?;
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        let layout = self.context.module.intern_abi_param_layout(layout);
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
                return report_unsupported(self.context.gcx, span, "abi.decode target type");
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
        let memory_types = types
            .iter()
            .copied()
            .map(|ty| ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory))
            .collect::<Vec<_>>();
        let (data, layout) = self.lower_abi_decode_layout(data, &memory_types, span)?;
        if !layout.types.iter().any(AbiParamType::has_dynamic_child) {
            let layout = self.context.module.intern_abi_param_layout(layout);
            let first = self.builder.abi_decode(layout, data);
            if memory_types.len() == 1 {
                return Some(vec![first]);
            }
            let base = self.multi_return_buffer_base();
            return Some(self.load_multi_return_values(
                first,
                base,
                memory_types.len(),
                memory_types.iter().skip(1).copied().map(Some),
            ));
        }
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let base = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let mut counts = FxHashMap::default();
        for ty in &layout.types {
            Self::count_dynamic_tuple_types(ty, &mut counts);
        }
        let mut helpers = FxHashMap::default();
        for (ty, count) in counts {
            if count >= 2 {
                let helper = crate::transform::lower_abi::synthesize_memory_decode_helper(
                    self.context.module,
                    ty.clone(),
                    self.context.gcx.sess.opts.evm_version.has_bitwise_shifting(),
                );
                helpers.insert(ty, helper);
            }
        }
        crate::transform::lower_abi::decode_memory_tuple(
            &mut self.builder,
            base,
            length,
            &layout,
            false,
            (!helpers.is_empty()).then_some(&helpers),
            self.context.gcx.sess.opts.evm_version.has_bitwise_shifting(),
        )
    }

    fn count_dynamic_tuple_types(
        ty: &crate::mir::AbiParamType,
        counts: &mut FxHashMap<crate::mir::AbiParamType, usize>,
    ) {
        match ty {
            crate::mir::AbiParamType::FixedArray { element, .. }
            | crate::mir::AbiParamType::DynamicArray(element) => {
                Self::count_dynamic_tuple_types(element, counts)
            }
            crate::mir::AbiParamType::Tuple(fields) => {
                if ty.has_dynamic_child() {
                    *counts.entry(ty.clone()).or_default() += 1;
                }
                for field in fields {
                    Self::count_dynamic_tuple_types(field, counts);
                }
            }
            crate::mir::AbiParamType::Scalar(_)
            | crate::mir::AbiParamType::Enum { .. }
            | crate::mir::AbiParamType::Bytes => {}
        }
    }

    pub(super) fn revert_external_call(&mut self, success: ValueId) {
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(success, continue_block, revert);
        self.builder.switch_to_block(revert);
        self.builder.revert_returndata();
        self.builder.switch_to_block(continue_block);
    }

    pub(super) fn materialize_memory_slice(&mut self, slice: ValueId) -> ValueId {
        let length = self.builder.slice_len(slice);
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, slice);
        object
    }

    pub(super) fn materialize_returndata_bytes(&mut self) -> ValueId {
        let length = self.builder.returndata_size();
        let object = self.builder.alloc_bytes_object(length, AllocationSemantics::INTERNAL);
        let zero = self.builder.imm_u256(U256::ZERO);
        let source = self.builder.make_slice(zero, length, SliceLocation::Returndata);
        self.builder.memory_object_copy_from_slice(object, MemoryObjectKind::Bytes, source);
        object
    }

    pub(super) fn lower_error_catch_string(&mut self, data: ValueId) -> Option<ValueId> {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let data_len = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let four = self.builder.imm_u64(4);
        let payload_ptr = self.builder.add_u64_offset(data_ptr, 4);
        let payload_len = self.builder.sub(data_len, four);
        let payload_slice =
            self.builder.make_slice(payload_ptr, payload_len, SliceLocation::Memory);
        let payload = self.materialize_memory_slice(payload_slice);
        let layout = self.context.module.intern_abi_param_layout(AbiParamLayout::new(
            vec![AbiParamType::Bytes].into_boxed_slice(),
        ));
        Some(self.builder.abi_decode(layout, payload))
    }

    pub(super) fn lower_panic_catch_word(&mut self, data: ValueId) -> ValueId {
        let data_ptr = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let payload_ptr = self.builder.add_u64_offset(data_ptr, 4);
        let word_size = self.builder.imm_u64(32);
        let payload = self.builder.make_slice(payload_ptr, word_size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(payload, zero)
    }

    pub(super) fn lower_abi_encode_packed(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, &args)?;
        let (pieces, total) = self.lower_packed_pieces(exprs, true)?;

        let output = self.builder.alloc_bytes_object(total, AllocationSemantics::INTERNAL);

        let mut offset = self.builder.imm_u64(0);
        let mut index = 0;
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm_u64(length);
                offset = self.builder.checked_add(offset, length);
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
                        offset = self.builder.checked_add(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        *source,
                    );
                    offset = self.builder.checked_add(offset, *length);
                }
                PackedPiece::Array { value, length, element, source } => {
                    offset =
                        self.copy_packed_array(output, offset, *value, *length, element, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm_u64((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm_u64(*length);
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
        let (pieces, total) = self.lower_packed_pieces(exprs, false)?;
        let has_dynamic = pieces.iter().any(|piece| matches!(piece, PackedPiece::Dynamic { .. }));
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

        let zero = base.unwrap_or_else(|| self.builder.imm_u64(0));
        let mut offset = 0u64;
        let mut cursor = has_dynamic.then(|| base.expect("dynamic packed input has a base"));
        let mut index = 0;
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
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm_u256(U256::from_be_bytes(padded));
                        let dest = self.packed_scratch_offset(cursor.or(base), offset);
                        self.builder.mstore(dest, value);
                        offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    let dest = if let Some(cursor) = cursor {
                        self.packed_scratch_offset(Some(cursor), offset)
                    } else {
                        self.packed_scratch_offset(None, offset)
                    };
                    self.copy_packed_slice(dest, *source)?;
                    cursor = Some(self.builder.add(dest, *length));
                    offset = 0;
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm_u64((32 - *length) * 8);
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

        let size = if has_dynamic {
            let cursor = cursor.expect("dynamic packed input has a cursor");
            let end = self.builder.add_u64_offset(cursor, offset);
            self.builder.sub(end, zero)
        } else {
            total
        };
        Some(self.builder.keccak256(zero, size))
    }

    fn is_scratch_packed_expr(&self, expr: &hir::Expr<'_>) -> bool {
        if matches!(
            self.peel_bytes_conversion(expr).peel_parens().kind,
            ExprKind::Lit(lit) if matches!(lit.kind, LitKind::Str(..))
        ) {
            return true;
        }
        let Some(ty) = self.context.gcx.type_of_expr(expr.id) else { return false };
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
            None => self.builder.imm_u64(offset),
        }
    }

    fn copy_packed_slice(&mut self, destination: ValueId, source: ValueId) -> Option<()> {
        let location = match self.builder.func().value_ty(source)? {
            MirType::Slice(location) => location,
            _ => return None,
        };
        let length = self.builder.slice_len(source);
        let pointer = self.builder.slice_ptr(source);
        self.builder.copy_slice_data(location, destination, pointer, length);
        Some(())
    }

    fn lower_packed_pieces(
        &mut self,
        exprs: &[hir::Expr<'_>],
        checked: bool,
    ) -> Option<(Vec<PackedPiece<'gcx>>, ValueId)> {
        let mut total = self.builder.imm_u64(0);
        let mut pieces = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.context.gcx.type_of_expr(expr.id)?;
            if let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str().to_vec();
                let length = self.builder.imm_u64(bytes.len() as u64);
                total = self.add_packed_total(total, length, checked);
                pieces.push(PackedPiece::Bytes(bytes));
                continue;
            }

            let memory_ty = ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
            let mut value = self.lower_typed_expr(expr, memory_ty)?;
            self.validate_enum(ty, value);
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
                let element_bytes_value = self.builder.imm_u64(element_bytes);
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
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "abi.encodePacked argument",
                );
            };
            let value = self.normalize_abi_scalar(value, ty);
            let signed = is_signed_packed_scalar(ty);
            let length_value = self.builder.imm_u64(length);
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
            return self.builder.imm_u256(result);
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
                self.builder.imm_u64(len)
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
            AbiType::Word | AbiType::Function => Some(32),
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
        let (length, element, element_bytes, source) = self.packed_array_shape(ty, value)?;
        let word = self.builder.imm_u64(32);
        let element_bytes_value = self.builder.imm_u64(element_bytes);
        let byte_length = self.builder.checked_mul(length, element_bytes_value);
        let size = self.builder.checked_add(word, byte_length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, byte_length, MemoryObjectKind::Bytes);
        let offset = self.builder.imm_u64(0);
        self.copy_packed_array(output, offset, value, length, &element, source);
        Some(output)
    }

    pub(super) fn lower_inplace_dynamic_value(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<ValueId> {
        if !self.inplace_dynamic_shape(ty)
            || (!matches!(self.builder.func().value_ty(value), Some(MirType::MemoryObject(_)))
                && !matches!(self.builder.func().value(value), Value::Inst(inst) if matches!(
                    &self.builder.func().inst(*inst).kind,
                    InstKind::MemoryObjectLoadField { .. } | InstKind::MemoryObjectLoadElement { .. }
                )))
        {
            return None;
        }
        let words = self.count_inplace_dynamic_value(ty, value)?;
        let word = self.builder.imm_u64(32);
        let length = self.builder.checked_mul(words, word);
        let size = self.builder.checked_add(word, length);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(output, length, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u64(0);
        self.copy_inplace_dynamic_value(ty, value, output, zero)?;
        Some(output)
    }

    fn inplace_dynamic_shape(&mut self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => {
                self.inplace_dynamic_shape(element)
            }
            TyKind::Struct(id) => self.context.gcx.hir.strukt(id).fields.iter().all(|&field| {
                self.inplace_dynamic_shape(self.context.gcx.type_of_item(field.into()))
            }),
            TyKind::Fn(function) => function.is_external(),
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => true,
            TyKind::Udvt(inner, _) => self.inplace_dynamic_shape(inner),
            TyKind::Tuple(_) => false,
            TyKind::Slice(_) => false,
            _ => matches!(self.types.abi_type(ty), Some(AbiType::Word)),
        }
    }

    fn count_inplace_dynamic_value(&mut self, ty: Ty<'gcx>, value: ValueId) -> Option<ValueId> {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => self.count_inplace_array(ty, value),
            TyKind::Struct(id) => {
                let gcx = self.context.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let layout = MemoryObjectLayout::structure(fields.len() as u64);
                let mut total = self.builder.imm_u64(0);
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let field_value =
                        self.builder.memory_object_load_field(value, layout, index as u64);
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
            _ => Some(self.builder.imm_u64(1)),
        }
    }

    fn count_inplace_array(&mut self, ty: Ty<'gcx>, value: ValueId) -> Option<ValueId> {
        let (element, length, layout) = self.inplace_array_info(ty, value)?;
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let total = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let element_value = self.builder.memory_object_load_element(value, layout, index);
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
        let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
        let word = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
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
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::DynArray(_) | TyKind::Array(..) => {
                self.copy_inplace_array(ty, value, output, offset)
            }
            TyKind::Struct(id) => {
                let gcx = self.context.gcx;
                let fields = gcx.hir.strukt(id).fields;
                let layout = MemoryObjectLayout::structure(fields.len() as u64);
                let mut offset = offset;
                for (index, &field) in fields.iter().enumerate() {
                    let field = gcx.type_of_item(field.into());
                    let field_value =
                        self.builder.memory_object_load_field(value, layout, index as u64);
                    offset = self.copy_inplace_dynamic_value(field, field_value, output, offset)?;
                }
                Some(offset)
            }
            TyKind::Elementary(
                solar_sema::hir::ElementaryType::Bytes | solar_sema::hir::ElementaryType::String,
            ) => Some(self.copy_inplace_bytes(value, output, offset)),
            TyKind::Udvt(inner, _) => self.copy_inplace_dynamic_value(inner, value, output, offset),
            TyKind::Tuple(_) | TyKind::Slice(_) => None,
            TyKind::Fn(function) if function.is_external() => {
                let shift = self.builder.imm_u64(64);
                let value = self.builder.shl(shift, value);
                self.builder.memory_object_store_word(output, offset, value);
                let word = self.builder.imm_u64(32);
                Some(self.builder.checked_add(offset, word))
            }
            _ => {
                self.builder.memory_object_store_word(output, offset, value);
                let word = self.builder.imm_u64(32);
                Some(self.builder.checked_add(offset, word))
            }
        }
    }

    fn inplace_array_info(
        &mut self,
        ty: Ty<'gcx>,
        value: ValueId,
    ) -> Option<(Ty<'gcx>, ValueId, MemoryObjectLayout)> {
        let ty = ty.peel_refs();
        let layout = self.types.memory_layout(ty)?;
        let (element, length) = match ty.kind {
            TyKind::DynArray(element) => {
                let length = self.builder.memory_object_len(value, layout.kind());
                (element, length)
            }
            TyKind::Array(element, length) => {
                let length = self.builder.imm_u64(u64::try_from(length).ok()?);
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
        let ty = ty.peel_refs();
        let (element, length, layout) = self.inplace_array_info(ty, value)?;
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm_u64(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let current_offset = self.builder.phi(vec![(preheader, offset)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let element_value = self.builder.memory_object_load_element(value, layout, index);
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
        let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
        let word = self.builder.imm_u64(32);
        let thirty_one = self.builder.imm_u64(31);
        let rounded = self.builder.checked_add(length, thirty_one);
        let mask = self.builder.not(thirty_one);
        let padded = self.builder.and(rounded, mask);
        let empty = self.builder.iszero(padded);
        let zero_block = self.builder.create_block();
        let copy_block = self.builder.create_block();
        self.builder.branch(empty, copy_block, zero_block);

        self.builder.switch_to_block(zero_block);
        let last_offset = self.builder.sub(padded, word);
        let last = self.builder.add(offset, last_offset);
        let zero = self.builder.imm_u64(0);
        self.builder.memory_object_store_word(output, last, zero);
        self.builder.jump(copy_block);

        self.builder.switch_to_block(copy_block);
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
        let element_bytes_value = self.builder.imm_u64(element_bytes);
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
        let element_offset = self.builder.checked_mul(index, element_bytes_value);
        let destination = self.builder.checked_add(offset, element_offset);
        match &element.abi {
            AbiType::Word | AbiType::Function => {
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
                self.validate_enum(element.ty, element_value);
                let element_value = self.normalize_abi_scalar(element_value, element.ty);
                let element_value = if matches!(&element.abi, AbiType::Function)
                    && matches!(source, PackedArraySource::Memory { .. })
                {
                    let shift = self.builder.imm_u64(64);
                    self.builder.shl(shift, element_value)
                } else {
                    element_value
                };
                self.builder.memory_object_store_word(output, destination, element_value);
            }
            AbiType::FixedArray { element: nested, len } => {
                let nested_length = self.builder.imm_u64(*len);
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

        let mut value = self.builder.imm_u256(constant);
        for (term, shift, size, signed) in terms {
            let term = if signed { self.mask_to_bits(term, (size * 8) as u16) } else { term };
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm_u64(shift);
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
                let variants = self.context.gcx.hir.enumm(id).variants.len().max(1);
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
        let memory_ty = self.context.gcx.types.bytes_ref.memory;
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
        let evm_version = self.context.gcx.sess.opts.evm_version;
        let gas = crate::utils::precompile_gas(&mut self.builder, evm_version);
        if evm_version.has_static_call() {
            self.builder.staticcall(gas, address, input_ptr, input_size, output_ptr, output_size);
        } else {
            let value = self.builder.imm_u256(U256::ZERO);
            self.builder.call(gas, address, value, input_ptr, input_size, output_ptr, output_size);
        }
    }
}
