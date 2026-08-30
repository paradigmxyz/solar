//! Function calls, conversions, and call-target resolution.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn uses_static_call(&self, state_mutability: hir::StateMutability) -> bool {
        matches!(state_mutability, hir::StateMutability::Pure | hir::StateMutability::View)
            && self.context.gcx.sess.opts.evm_version.has_static_call()
    }

    pub(super) fn lower_user_operator(
        &mut self,
        span: Span,
        function_id: hir::FunctionId,
        values: &[ValueId],
    ) -> Option<ValueId> {
        let function = self.context.gcx.hir.function(function_id);
        if function.parameters.len() != values.len() || function.returns.len() != 1 {
            return report_unsupported(self.context.gcx, span, "user-defined operator signature");
        }
        let Some(&mir_id) = self.context.function_ids.get(&function_id) else {
            return report_unsupported(self.context.gcx, span, "user-defined operator function");
        };
        let result_ty = types::TypeLowerer::mir_return_type(
            self.context.gcx.type_of_item(function.returns[0].into()),
        );
        Some(self.builder.internal_call(mir_id, values.to_vec(), result_ty, 1))
    }

    pub(super) fn lower_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        // result = lower_struct_ctor(callee, args)
        // result = convert(callee, args)
        // result = new(callee, args, opts)
        // result = builtin(callee, args, opts)
        // result = function_pointer_call(callee, args, opts)
        // result = function_call(callee, args, opts)
        if let Some(struct_id) = self.context.gcx.resolved_expr(callee).and_then(|res| match res {
            hir::Res::Item(item) => item.as_struct(),
            _ => None,
        }) {
            return self.lower_struct_constructor(expr, struct_id, args);
        }
        let is_type_conversion = matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_))
            || self.context.gcx.resolved_expr(callee).is_some_and(|res| {
                matches!(res, hir::Res::Item(hir::ItemId::Contract(_) | hir::ItemId::Enum(_)))
            });
        if is_type_conversion {
            if args.len() != 1 {
                return report_unsupported(self.context.gcx, expr.span, "type conversion");
            }
            let Some(arg) = args.exprs().next() else {
                return report_unsupported(self.context.gcx, expr.span, "type conversion");
            };
            let source_ty = self.context.gcx.type_of_expr(arg.id)?;
            let target_ty = self.context.gcx.type_of_expr(expr.id).or_else(|| {
                self.context.gcx.resolved_expr(callee).and_then(|res| match res {
                    hir::Res::Item(id @ (hir::ItemId::Contract(_) | hir::ItemId::Enum(_))) => {
                        Some(self.context.gcx.type_of_item(id))
                    }
                    _ => None,
                })
            })?;
            if let Some(value) = self.lower_fixed_bytes_literal(target_ty, arg) {
                return Some(value);
            }
            let value = if source_ty.is_ref_at(DataLocation::Storage)
                && matches!(
                    source_ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
                && matches!(
                    target_ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                ) {
                let Some(access) = self.storage_access(arg) else {
                    return report_unsupported(self.context.gcx, arg.span, "storage access");
                };
                self.load_storage_bytes(access.slot)
            } else {
                self.lower_expr(arg)?
            };
            return Some(self.coerce_value(value, source_ty, target_ty));
        }
        if let ExprKind::New(ty) = &callee.kind {
            if let TyKind::Contract(contract_id) = self.context.gcx.type_of_hir_ty(ty).kind {
                return self.lower_new_contract(expr, ty, contract_id, args, call_opts);
            }
            if args.len() != 1 {
                return report_unsupported(self.context.gcx, expr.span, "dynamic allocation");
            }
            let Some(arg) = args.exprs().next() else {
                return report_unsupported(self.context.gcx, expr.span, "dynamic allocation");
            };
            let len = self.lower_expr(arg)?;
            let ty = self.context.gcx.type_of_expr(expr.id)?;
            let layout = self.types.memory_layout(ty)?;
            let size = match layout {
                MemoryObjectLayout::Bytes => self.builder.checked_padded_size(len),
                MemoryObjectLayout::DynamicArray { element_words } => {
                    let stride = self.builder.imm_u64(u64::from(element_words));
                    let payload = self.builder.checked_mul(len, stride);
                    let one = self.builder.imm_u64(1);
                    let words = self.builder.checked_add(payload, one);
                    let word_size = self.builder.imm_u64(32);
                    self.builder.checked_mul(words, word_size)
                }
                _ => return report_unsupported(self.context.gcx, expr.span, "allocation type"),
            };
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, len, layout.kind());
            return Some(object);
        }
        if let Some(builtin) = self.context.gcx.resolved_builtin(callee) {
            return self.lower_builtin_call(expr, callee, builtin, args, call_opts);
        }
        if let Some(TyKind::Fn(function)) =
            self.context.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.function_id.is_none()
        {
            if function.is_external() {
                return self
                    .lower_external_function_pointer_call(callee, function, args, call_opts);
            }
            if function.is_internal() {
                return self.lower_internal_function_pointer_call(expr, callee, function, args);
            }
        }
        if let Some(function_id) = self.context.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args, call_opts);
        }
        if self.context.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        report_unsupported(self.context.gcx, expr.span, "function call")
    }

    pub(super) fn lower_new_contract(
        &mut self,
        _expr: &hir::Expr<'_>,
        ty: &hir::Type<'_>,
        contract_id: hir::ContractId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let created = self.lower_create_contract(ty, contract_id, args, call_opts)?;
        self.revert_external_call(created);
        Some(created)
    }

    pub(super) fn lower_create_contract(
        &mut self,
        ty: &hir::Type<'_>,
        contract_id: hir::ContractId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        // init = creation_bytecode ++ abi_encode(constructor_args)
        // address = create|create2(value, init.data, init.len[, salt])
        let contract = self.context.gcx.hir.contract(contract_id);
        let bytecode = self.context.child_bytecodes.get(&contract_id).ok_or_else(|| {
            self.context
                .gcx
                .dcx()
                .err(format!("codegen is missing creation bytecode for `new {}`", contract.name))
                .span(ty.span)
                .note("the deployed contract did not compile or was not lowered first")
                .emit()
        });
        let Ok(bytecode) = bytecode else { return None };

        let mut call_value = self.builder.imm_u256(U256::ZERO);
        let mut salt = None;
        if let Some(options) = call_opts {
            for option in options.args {
                match option.name.name {
                    sym::value => {
                        call_value =
                            self.lower_typed_expr(&option.value, self.context.gcx.types.uint(256))?;
                    }
                    sym::salt => {
                        salt = Some(self.lower_typed_expr(
                            &option.value,
                            self.context.gcx.types.fixed_bytes(32),
                        )?);
                    }
                    _ => {
                        return report_unsupported(
                            self.context.gcx,
                            option.name.span,
                            "creation option",
                        );
                    }
                }
            }
        }

        let (parameters, parameter_names) = contract
            .ctor
            .map(|id| {
                let constructor = self.context.gcx.hir.function(id);
                (
                    constructor.parameters,
                    self.context.gcx.callable_param_names(CallableParamSource::Function {
                        id,
                        skips_receiver: false,
                    }),
                )
            })
            .unwrap_or((&[], Vec::new().into()));
        if args.len() != parameters.len() {
            return report_unsupported(self.context.gcx, args.span, "constructor arguments");
        }

        let mut values = Vec::with_capacity(parameters.len());
        let mut types = Vec::with_capacity(parameters.len());
        for (index, &parameter) in parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.context.gcx, args.span, "constructor argument");
            };
            let parameter_ty = self.context.gcx.type_of_item(parameter.into());
            let (value, abi_type) = self.lower_abi_call_argument(argument, parameter_ty)?;
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(Arc::clone(&layout), None, values.into_boxed_slice());
        let encoded_len = if layout.types.iter().any(AbiType::is_dynamic) {
            self.builder.slice_len(encoded)
        } else {
            self.builder.imm_u64(layout.head_size())
        };

        let bytecode_len = u64::try_from(bytecode.len()).ok()?;
        let bytecode_len_value = self.builder.imm_u64(bytecode_len);
        let total_len = self.builder.checked_add(bytecode_len_value, encoded_len);
        // CREATE consumes a raw byte range, so do not reserve a semantic bytes
        // header that no later operation can observe.
        let padding = self.builder.imm_u64(31);
        let rounded_len = self.builder.checked_add(total_len, padding);
        let mask = self.builder.not(padding);
        let allocation_size = self.builder.and(rounded_len, mask);
        let data = self.builder.alloc_raw(allocation_size, AllocationSemantics::INTERNAL);

        for (index, chunk) in bytecode.chunks(32).enumerate() {
            let offset = u64::try_from(index).ok()?.saturating_mul(32);
            let address = self.builder.add_u64_offset(data, offset);
            let value = self.lower_string_literal_word(chunk);
            self.builder.mstore(address, value);
        }
        let encoded_ptr = self.builder.slice_ptr(encoded);
        let copy_dest = self.builder.add(data, bytecode_len_value);
        self.builder.copy_slice_data(SliceLocation::Memory, copy_dest, encoded_ptr, encoded_len);
        let created = if let Some(salt) = salt {
            self.builder.create2(call_value, data, total_len, salt)
        } else {
            self.builder.create(call_value, data, total_len)
        };
        Some(created)
    }

    pub(super) fn lower_external_function_pointer_call(
        &mut self,
        callee: &hir::Expr<'_>,
        function: &TyFn<'gcx>,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let values =
            self.lower_external_function_pointer_call_values(callee, function, args, call_opts)?;
        Some(values.into_iter().next().unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)))
    }

    pub(super) fn lower_external_function_pointer_call_values(
        &mut self,
        callee: &hir::Expr<'_>,
        function: &TyFn<'gcx>,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<Vec<ValueId>> {
        // address, selector = function_pointer
        // input = abi_encode(selector, args)
        // buffer, ret_offset, ret_size, decode = plan_return_buffer(returns)
        // ok = CALL/STATICCALL(gas, address, value, input, ret_offset, ret_size)
        // if !ok { revert_returndata() }
        // result = decode_buffer | decode_returndata | load_words(ret_offset)
        let arg_exprs = self.builtin_arg_exprs(Builtin::AbiEncode, &args)?;
        if arg_exprs.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, args.span, "external function arguments");
        }
        let function_value = self.lower_expr(callee)?;
        let (address, selector) = self.split_external_function_pointer(function_value);

        let (gas, call_value, zero) = self.lower_call_options(call_opts, true, "call option")?;

        let mut values = Vec::with_capacity(arg_exprs.len());
        let mut types = Vec::with_capacity(arg_exprs.len());
        for (argument, &parameter) in arg_exprs.iter().zip(function.parameters) {
            let (value, abi_type) = self.lower_abi_call_argument(argument, parameter)?;
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let return_tys = function.returns;
        let returns = return_tys.len();
        let (static_return_buffer, ret_offset, ret_size, decode_returndata) =
            self.plan_return_buffer(input, zero, return_tys);
        if returns == 0 {
            self.revert_if_no_code(address);
        }
        let success = if self.uses_static_call(function.state_mutability) {
            self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
        } else {
            self.builder.call(gas, address, call_value, input, input_size, ret_offset, ret_size)
        };
        self.revert_external_call(success);
        if returns == 0 {
            return Some(Vec::new());
        }
        if let Some((data, _, size)) = static_return_buffer {
            self.revert_if_short_returndata(size);
            return self.lower_abi_decode_values(data, return_tys, callee.span);
        }
        if decode_returndata {
            if !self.context.gcx.sess.opts.evm_version.supports_returndata() {
                return report_error(
                    self.context.gcx,
                    callee.span,
                    "codegen cannot decode external function-pointer returndata before Byzantium",
                );
            }
            let data = self.materialize_returndata_bytes();
            return self.lower_abi_decode_values(data, return_tys, callee.span);
        }
        if self.context.gcx.sess.opts.evm_version.supports_returndata() {
            self.validate_static_returndata(ret_offset, function.returns);
        }
        if returns > 1 {
            self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, ret_offset);
            let first =
                self.load_multi_return_value_as(ret_offset, 0, returns, function.returns[0]);
            return Some(self.load_multi_return_values(
                first,
                ret_offset,
                returns,
                function.returns.iter().skip(1).copied().map(Some),
            ));
        }
        Some(vec![self.load_multi_return_value_as(ret_offset, 0, returns, function.returns[0])])
    }

    pub(super) fn split_external_function_pointer(
        &mut self,
        function_value: ValueId,
    ) -> (ValueId, ValueId) {
        let selector_mask = self.builder.imm_u256(U256::from(u32::MAX));
        let selector = self.builder.and(function_value, selector_mask);
        let selector_shift = self.builder.imm_u64(224);
        let selector = self.builder.shl(selector_shift, selector);
        let address_shift = self.builder.imm_u64(32);
        let address = self.builder.shr(address_shift, function_value);
        (address, selector)
    }

    pub(super) fn lower_internal_function_pointer_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function: &TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // ptr = lower(function_ptr)
        // args = materialize_args(...)
        // if returns == 0 {
        //     internal_call_void(dispatcher, [ptr, args])
        //     result = 0
        // } else {
        //     result = internal_call(dispatcher, [ptr, args])
        // }
        if args.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "internal function arguments");
        }
        let function_value = self.lower_expr(callee)?;
        let parameter_names = self
            .context
            .gcx
            .call_param_source(callee)
            .map(|source| self.context.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) = args.argument_for_parameter(index, parameter_names.as_deref())
            else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "named internal function argument",
                );
            };
            let value = self.lower_typed_expr(argument, parameter)?;
            let value = self.normalize_internal_call_argument(value, parameter);
            values.push(self.materialize_call_argument(parameter, value, argument.span)?);
        }
        values.insert(0, function_value);

        let dispatcher = self.ensure_internal_function_pointer_dispatcher(function);
        if function.returns.is_empty() {
            self.builder.internal_call_void(dispatcher, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let first_ty = function.returns[0];
        let result_ty = types::TypeLowerer::mir_return_type(first_ty);
        let returns = Self::internal_returns_words(function.returns.iter().copied());
        let result =
            self.builder.internal_call(dispatcher, values, result_ty, function.returns.len());
        Some(self.rebuild_first_internal_return(result, first_ty, returns))
    }

    pub(super) fn lower_internal_function_value(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let TyKind::Fn(function) = self.context.gcx.type_of_expr(expr.id)?.kind else {
            return None;
        };
        if !function.is_internal() {
            return None;
        }
        let hir::Res::Item(hir::ItemId::Function(function_id)) =
            self.context.gcx.resolved_expr(expr)?
        else {
            return None;
        };
        let function_id = self.resolve_call_target(expr, function_id);
        self.context.state.pointer_registry.targets.insert(function_id);
        Some(self.builder.imm_u64(internal_function_pointer_id(function_id)))
    }

    pub(super) fn ensure_internal_function_pointer_dispatcher(
        &mut self,
        function: &TyFn<'gcx>,
    ) -> FunctionId {
        // dispatch(function_ptr, params...) -> returns...
        let shape = InternalFunctionPointerShape::from_ty(function);
        let name = shape.helper_name();
        let InternalFunctionPointerShape { params, returns } = shape;
        self.lazy_helper(name, |_, function| {
            function.attributes.is_function_pointer_dispatcher = true;
            let mut builder = FunctionBuilder::new(function);
            builder.add_param(MirType::Function);
            for ty in params {
                builder.add_param(ty);
            }
            for ty in returns {
                builder.add_return(ty);
            }
            Some(())
        })
        .expect("internal dispatcher helper construction cannot fail")
    }

    pub(super) fn coerce_value(&mut self, value: ValueId, from: Ty<'gcx>, to: Ty<'gcx>) -> ValueId {
        // value = from != to ? normalize_dirty(value, from) : value
        // if dynamic_bytes_to_fixed { value = mask(load_word(value), source_length) }
        // if fixed_bytes_target && dynamic_bytes_calldata_source {
        //     validate_calldata_bytes(value)
        // }
        // if narrowing_integer { value = normalize_integer(value, to) }
        // if enum_target && from != to { validate_enum(to, value) }
        // if fixed_bytes_target { value = clean_or_extend(value, to) }
        let value = if from.peel_refs() != to.peel_refs() {
            self.normalize_dirty_scalar(value, from)
        } else {
            value
        };
        let source_size = fixed_bytes_size(from);
        let destination_size = fixed_bytes_size(to);
        if let Some(size) = destination_size
            && (self.is_dynamic_bytes_type(from)
                || matches!(from.peel_refs().kind, TyKind::StringLiteral(..)))
        {
            let zero = self.builder.imm_u64(0);
            let word_and_length = match self.builder.func().value_ty(value) {
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                    let word = self.builder.memory_object_load_element(
                        value,
                        MemoryObjectLayout::Bytes,
                        zero,
                    );
                    let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                    Some((word, length))
                }
                Some(MirType::Slice(SliceLocation::Calldata)) => {
                    let word = self.builder.calldata_slice_load_word(value, zero);
                    let length = self.builder.slice_len(value);
                    Some((word, length))
                }
                Some(MirType::Slice(SliceLocation::Memory)) => {
                    let word = self.builder.memory_slice_load_word(value, zero);
                    let length = self.builder.slice_len(value);
                    Some((word, length))
                }
                _ => None,
            };
            if let Some((word, length)) = word_and_length {
                let width = u64::from(size.bytes());
                let fixed_mask =
                    self.builder.imm_u256(U256::MAX << (256 - usize::from(size.bytes()) * 8));
                let width_value = self.builder.imm_u64(width);
                let short = self.builder.lt(length, width_value);
                let missing = self.builder.sub(width_value, length);
                let bits_per_byte = self.builder.imm_u64(8);
                let shift = self.builder.mul(bits_per_byte, missing);
                let short_mask = self.builder.shl(shift, fixed_mask);
                let mask = self.builder.select(short, short_mask, fixed_mask);
                return self.builder.and(word, mask);
            }
        }
        if destination_size.is_some()
            && let Some(abi_type) = self.types.abi_type(from)
        {
            self.validate_calldata_bytes_argument(value, &abi_type);
        }
        let value = if let Some(size) = source_size
            && destination_size.is_none()
            && u64::from(32 - size.bytes()) * 8 != 0
        {
            let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
            self.builder.shr(shift, value)
        } else {
            value
        };
        let integer_conversion_needs_cleanup = match (from.peel_refs().kind, to.peel_refs().kind) {
            (
                TyKind::Elementary(ElementaryType::UInt(from_size)),
                TyKind::Elementary(ElementaryType::UInt(to_size)),
            )
            | (
                TyKind::Elementary(ElementaryType::Int(from_size)),
                TyKind::Elementary(ElementaryType::Int(to_size)),
            ) => to_size.bits() < from_size.bits(),
            (
                TyKind::Elementary(ElementaryType::UInt(from_size)),
                TyKind::Elementary(ElementaryType::Int(to_size)),
            )
            | (
                TyKind::Elementary(ElementaryType::Int(from_size)),
                TyKind::Elementary(ElementaryType::UInt(to_size)),
            ) => to_size.bits() <= from_size.bits(),
            _ => false,
        };
        if integer_conversion_needs_cleanup {
            return self.normalize_abi_scalar(value, to);
        }
        if let TyKind::Enum(id) = to.peel_refs().kind {
            if !matches!(from.peel_refs().kind, TyKind::Enum(from_id) if from_id == id) {
                self.validate_enum(to, value);
            }
            return value;
        }
        let Some(size) = destination_size else {
            return value;
        };
        let byte_value = match from.peel_refs().kind {
            TyKind::StringLiteral(..) => match self.builder.func().value_ty(value) {
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => Some(value),
                _ => return value,
            },
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                match self.builder.func().value_ty(value) {
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => Some(value),
                    Some(MirType::Slice(_)) => Some(self.materialize_memory_slice(value)),
                    _ => return value,
                }
            }
            _ => None,
        };
        if let Some(value) = byte_value {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if let Some(source_size) = source_size {
            if source_size.bytes() > size.bytes() {
                return self.clean_fixed_bytes(value, size.bytes());
            }
            return value;
        }
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn normalize_abi_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        let peeled = ty.peel_refs();
        if matches!(peeled.kind, TyKind::Elementary(ElementaryType::Bool)) {
            let zero = self.builder.imm_u256(U256::ZERO);
            let is_zero = self.builder.eq(value, zero);
            return self.builder.iszero(is_zero);
        }
        let validator = match peeled.kind {
            TyKind::Enum(id) => Some(AbiWordValidator::EnumRange(
                self.context.gcx.hir.enumm(id).variants.len() as u64,
            )),
            TyKind::Fn(_) => None,
            _ => AbiWordValidator::from_mir_type(types::TypeLowerer::mir_type(ty)),
        };
        validator.map_or(value, |validator| validator.cleanup(&mut self.builder, value))
    }

    pub(super) fn normalize_dirty_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        if self.in_inline_assembly || !self.dirty_values.contains(&value) {
            return value;
        }
        self.validate_enum(ty, value);
        self.normalize_abi_scalar(value, ty)
    }

    fn normalize_internal_call_argument(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        if matches!(ty.peel_refs().kind, TyKind::Udvt(..)) {
            value
        } else {
            self.normalize_dirty_scalar(value, ty)
        }
    }

    // External functions are low-aligned as scalar MIR values, but Solidity memory stores their
    // 24-byte representation left-aligned. Keep the conversion at typed memory boundaries.
    pub(super) fn normalize_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if matches!(ty.peel_refs().kind, TyKind::Fn(function) if function.is_external()) {
            let shift = self.builder.imm_u64(64);
            return self.builder.shr(shift, value);
        }
        self.normalize_abi_scalar(value, ty)
    }

    pub(super) fn decode_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm_u64(64);
            return self.builder.shr(shift, value);
        }
        value
    }

    pub(super) fn encode_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm_u64(64);
            return self.builder.shl(shift, value);
        }
        self.normalize_abi_scalar(value, ty)
    }

    pub(super) fn lower_function_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        // args = resolve_receiver_and_storage_args(...)
        // result = internal_call[_void](function, args)
        // result = external_abi_call(function, args, opts)
        // result = delegatecall(library, args)
        let function = self.context.gcx.hir.function(function_id);
        let attached =
            self.context.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
        let delegate_call = self.context.gcx.type_of_expr(callee.id).is_some_and(
            |ty| matches!(ty.kind, TyKind::Fn(function) if function.is_delegate_call()),
        );
        let attached_receiver = if attached {
            let ExprKind::Member(receiver, _) = callee.kind else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "attached function receiver",
                );
            };
            Some(receiver)
        } else {
            None
        };
        if let ExprKind::Member(receiver, _) = callee.kind
            && self.context.gcx.resolved_builtin(receiver) == Some(Builtin::This)
        {
            let function_id = self.resolve_call_target(callee, function_id);
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        }
        if !attached
            && let ExprKind::Member(receiver, _) = callee.kind
            && self
                .context
                .gcx
                .type_of_expr(receiver.id)
                .is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::Contract(_)))
            && !matches!(
                function.contract.map(|id| self.context.gcx.hir.contract(id).kind),
                Some(hir::ContractKind::Library)
            )
        {
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        }
        let function_id = self.resolve_call_target(callee, function_id);
        let function = self.context.gcx.hir.function(function_id);
        if delegate_call {
            let address = self.library_address(function_id);
            return self.lower_library_call(expr, function_id, attached_receiver, args, address);
        }
        let receiver_count = usize::from(attached);
        if args.len() + receiver_count != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "function argument list");
        }
        let parameter_names = self
            .context
            .gcx
            .call_param_source(callee)
            .map(|source| self.context.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        if let Some(receiver) = attached_receiver {
            let parameter_ty = self.context.gcx.type_of_item(function.parameters[0].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                let Some(access) = self.storage_access(receiver) else {
                    return report_unsupported(self.context.gcx, receiver.span, "storage access");
                };
                access.slot
            } else {
                self.lower_typed_expr(receiver, parameter_ty)?
            };
            let value = self.normalize_internal_call_argument(value, parameter_ty);
            values.push(self.materialize_call_argument(parameter_ty, value, receiver.span)?);
        }
        let mut parameter_indices = (receiver_count..function.parameters.len()).collect::<Vec<_>>();
        if function.is_yul {
            parameter_indices.reverse();
        }
        let mut arguments = vec![None; function.parameters.len() - receiver_count];
        for index in parameter_indices {
            let parameter = function.parameters[index];
            let Some(argument) =
                args.argument_for_parameter(index - receiver_count, parameter_names.as_deref())
            else {
                return report_unsupported(self.context.gcx, expr.span, "named function argument");
            };
            let parameter_ty = self.context.gcx.type_of_item(parameter.into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                let Some(access) = self.storage_access(argument) else {
                    return report_unsupported(self.context.gcx, argument.span, "storage access");
                };
                access.slot
            } else {
                self.lower_typed_expr(argument, parameter_ty)?
            };
            let value = self.normalize_internal_call_argument(value, parameter_ty);
            arguments[index - receiver_count] =
                Some(self.materialize_call_argument(parameter_ty, value, argument.span)?);
        }
        values.extend(arguments.into_iter().map(|value| value.expect("argument lowered")));
        let Some(&mir_id) = self.context.function_ids.get(&function_id) else {
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        };
        if let Some(value) = self.lower_pure_struct_constructor(function, &values) {
            return Some(value);
        }
        if function.returns.is_empty() {
            self.builder.internal_call_void(mir_id, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let first_ty = self.context.gcx.type_of_item((*function.returns.first()?).into());
        let result_ty = types::TypeLowerer::mir_return_type(first_ty);
        let returns = Self::internal_returns_words(
            function.returns.iter().map(|&id| self.context.gcx.type_of_item(id.into())),
        );
        let result = self.builder.internal_call(mir_id, values, result_ty, function.returns.len());
        let result = self.rebuild_first_internal_return(result, first_ty, returns);
        self.dirty_values.insert(result);
        Some(result)
    }

    fn lower_pure_struct_constructor(
        &mut self,
        function: &hir::Function<'_>,
        values: &[ValueId],
    ) -> Option<ValueId> {
        if function.state_mutability != StateMutability::Pure
            || !function.modifiers.is_empty()
            || function.parameters.len() != values.len()
            || function.returns.len() != 1
        {
            return None;
        }
        let return_ty = self.context.gcx.type_of_item(function.returns[0].into());
        if !return_ty.is_ref_at(DataLocation::Memory) {
            return None;
        }
        let TyKind::Struct(return_struct) = return_ty.peel_refs().kind else { return None };
        let body = function.body?;
        let [stmt] = body.stmts else { return None };
        let StmtKind::Return(Some(return_expr)) = stmt.kind else { return None };
        let ExprKind::Call(constructor, args, None) = return_expr.peel_parens().kind else {
            return None;
        };
        let Some(hir::Res::Item(item)) = self.context.gcx.resolved_expr(constructor) else {
            return None;
        };
        let struct_id = item.as_struct()?;
        if struct_id != return_struct {
            return None;
        }

        let saved = self.snapshot_bindings(function.parameters);
        for (&id, &value) in function.parameters.iter().zip(values) {
            self.values.insert(id, value);
        }
        let result = self.lower_struct_constructor(return_expr, struct_id, args);
        self.restore_bindings(&saved);
        result
    }

    pub(super) fn lower_external_function_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        // input = abi_encode(selector, args)
        // buffer, ret_offset, ret_size, decode = plan_return_buffer(returns)
        // ok = CALL/STATICCALL(gas, address, value, input, ret_offset, ret_size)
        // if !ok { revert_returndata() }
        // result = decode_buffer | decode_returndata | load_words(ret_offset)
        let ExprKind::Member(receiver, _) = callee.kind else {
            return report_unsupported(self.context.gcx, expr.span, "external function target");
        };
        let function = self.context.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "external function arguments");
        }
        let address = self.lower_expr(receiver)?;
        let (gas, call_value, zero) = self.lower_call_options(call_opts, true, "call option")?;
        let parameter_names =
            self.context.gcx.callable_param_names(CallableParamSource::Function {
                id: function_id,
                skips_receiver: false,
            });
        let gcx = self.context.gcx;
        let parameter_types =
            function.parameters.iter().map(move |&parameter| gcx.type_of_item(parameter.into()));
        let (values, types) = self.lower_abi_call_arguments(
            args,
            parameter_types,
            Some(&parameter_names),
            expr.span,
            "external function argument",
            false,
        )?;
        let selector = self.context.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let return_tys = function
            .returns
            .iter()
            .map(|&ret| self.context.gcx.type_of_item(ret.into()))
            .collect::<Vec<_>>();
        let returns = return_tys.len();
        let (static_return_buffer, ret_offset, ret_size, decode_returndata) =
            self.plan_return_buffer(input, zero, &return_tys);
        if returns == 0
            && (self.builder.func().attributes.is_constructor
                || self.context.gcx.resolved_builtin(receiver) != Some(Builtin::This))
        {
            self.revert_if_no_code(address);
        }
        let success = if self.uses_static_call(function.state_mutability) {
            self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
        } else {
            self.builder.call(gas, address, call_value, input, input_size, ret_offset, ret_size)
        };
        self.revert_external_call(success);
        if returns == 0 {
            return Some(zero);
        }
        if let Some((data, _, size)) = static_return_buffer {
            self.revert_if_short_returndata(size);
            return self.lower_decoded_return_value(data, &return_tys, expr.span, zero);
        }
        if decode_returndata {
            if !self.context.gcx.sess.opts.evm_version.supports_returndata() {
                return report_error(
                    self.context.gcx,
                    expr.span,
                    "codegen cannot decode external function returndata before Byzantium",
                );
            }
            let data = self.materialize_returndata_bytes();
            return self.lower_decoded_return_value(data, &return_tys, expr.span, zero);
        }
        if self.context.gcx.sess.opts.evm_version.supports_returndata() {
            self.validate_static_returndata(ret_offset, &return_tys);
        }
        if returns > 1 {
            self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, ret_offset);
        }
        let ty = self.context.gcx.type_of_item(function.returns[0].into());
        Some(self.load_multi_return_value_as(ret_offset, 0, returns, ty))
    }

    pub(super) fn lower_abi_call_arguments(
        &mut self,
        args: hir::CallArgs<'_>,
        parameter_types: impl ExactSizeIterator<Item = Ty<'gcx>>,
        parameter_names: Option<&CallableParamNames>,
        span: Span,
        error: &'static str,
        storage_parameters: bool,
    ) -> Option<(Vec<ValueId>, Vec<AbiType>)> {
        let parameter_names = parameter_names.map(|names| names.as_slice());
        let values_and_types = parameter_types
            .enumerate()
            .map(|(index, parameter_ty)| {
                let argument = args
                    .argument_for_parameter(index, parameter_names)
                    .or_else(|| report_unsupported(self.context.gcx, span, error))?;
                if storage_parameters && Self::is_storage_parameter(parameter_ty) {
                    let Some(access) = self.storage_access(argument) else {
                        return report_unsupported(
                            self.context.gcx,
                            argument.span,
                            "storage access",
                        );
                    };
                    Some((access.slot, AbiType::Word(None)))
                } else {
                    Some(self.lower_abi_call_argument(argument, parameter_ty)?)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        let (values, types) = values_and_types.into_iter().unzip();
        Some((values, types))
    }

    pub(super) fn library_address(&mut self, function_id: hir::FunctionId) -> U256 {
        let contract_id = self
            .context
            .gcx
            .hir
            .function(function_id)
            .contract
            .expect("library function must have a contract");
        let contract = self.context.gcx.hir.contract(contract_id);
        assert_eq!(contract.kind, hir::ContractKind::Library);
        let source = self.context.gcx.hir.source(contract.source).file.name.display().to_string();
        if let Some(address) = self
            .context
            .gcx
            .sess
            .opts
            .libraries
            .iter()
            .find(|library| {
                library.name == contract.name.as_str_in(self.context.gcx.sess)
                    && library.source.as_ref().is_none_or(|path| source.ends_with(path))
            })
            .map(|library| U256::from_be_slice(library.address.as_slice()))
        {
            return address;
        }

        let name = contract.name.as_str_in(self.context.gcx.sess).to_string();
        let hash = keccak256(format!("{source}:{name}"));
        let mut placeholder = <[u8; 20]>::try_from(&hash[..20]).unwrap();
        placeholder[0] |= 0x80;
        self.context.module.add_library_link(LibraryLink { source, name, placeholder });
        U256::from_be_slice(&placeholder)
    }

    pub(super) fn lower_library_call(
        &mut self,
        expr: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        receiver: Option<&hir::Expr<'_>>,
        args: hir::CallArgs<'_>,
        address: U256,
    ) -> Option<ValueId> {
        // input = abi_encode(selector, args)
        // ok = DELEGATECALL(gas, library, input, 0, 0)
        // if !ok { revert_returndata() }
        // result = abi_decode(returndata) | 0
        let function = self.context.gcx.hir.function(function_id);
        let receiver_count = usize::from(receiver.is_some());
        if args.len() + receiver_count != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "library arguments");
        }
        let parameter_names =
            self.context.gcx.callable_param_names(CallableParamSource::Function {
                id: function_id,
                skips_receiver: receiver.is_some(),
            });
        let gcx = self.context.gcx;
        let parameter_types =
            function.parameters.iter().map(move |&parameter| gcx.type_of_item(parameter.into()));
        let mut parameter_types = parameter_types.skip(receiver_count);
        let (mut values, mut types) = self.lower_abi_call_arguments(
            args,
            &mut parameter_types,
            Some(&parameter_names),
            expr.span,
            "library argument",
            true,
        )?;
        if let Some(receiver) = receiver {
            let parameter_ty = gcx.type_of_item(function.parameters[0].into());
            let (value, ty) = self.lower_abi_receiver(receiver, parameter_ty)?;
            values.insert(0, value);
            types.insert(0, ty);
        }

        let selector = self.context.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let zero = self.builder.imm_u256(U256::ZERO);
        let address = self.builder.imm_u256(address);
        let gas = self.builder.gas();
        if function.returns.is_empty() {
            self.revert_if_no_code(address);
        }
        let success = self.builder.delegatecall(gas, address, input, input_size, zero, zero);
        self.revert_external_call(success);
        if function.returns.is_empty() {
            return Some(zero);
        }
        if !self.context.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.context.gcx,
                expr.span,
                "codegen cannot decode linked library returndata before Byzantium",
            );
        }
        let data = self.materialize_returndata_bytes();
        let return_types = function
            .returns
            .iter()
            .map(|&ret| self.context.gcx.type_of_item(ret.into()))
            .collect::<Vec<_>>();
        self.lower_decoded_return_value(data, &return_types, expr.span, zero)
    }

    pub(super) fn lower_abi_receiver(
        &mut self,
        receiver: &hir::Expr<'_>,
        parameter_ty: Ty<'gcx>,
    ) -> Option<(ValueId, AbiType)> {
        if Self::is_storage_parameter(parameter_ty) {
            let Some(access) = self.storage_access(receiver) else {
                return report_unsupported(self.context.gcx, receiver.span, "storage access");
            };
            Some((access.slot, AbiType::Word(None)))
        } else {
            self.lower_abi_call_argument(receiver, parameter_ty)
        }
    }

    fn static_aggregate_return_layout(
        &mut self,
        returns: impl Iterator<Item = Ty<'gcx>>,
    ) -> Option<AbiParamLayout> {
        let types =
            returns.map(|ty| self.types.abi_return_param_type(ty)).collect::<Option<Vec<_>>>()?;
        (!types.is_empty()
            && types.iter().all(|ty| !ty.is_dynamic())
            && types.iter().any(|ty| !ty.is_scalar_word()))
        .then(|| AbiParamLayout::new(types.into_boxed_slice()))
    }

    fn plan_return_buffer(
        &mut self,
        input: ValueId,
        zero: ValueId,
        return_tys: &[Ty<'gcx>],
    ) -> (Option<(ValueId, ValueId, ValueId)>, ValueId, ValueId, bool) {
        // static = static_aggregate_layout(returns)
        // buffer = static ? alloc_static_buffer(static) : none
        // decode = any_return_is_nonword
        // offset = static.data ? static.data : (!decode && returns > 1 ? input : zero)
        // size = static.size ? static.size : (decode ? 0 : returns * 32)
        let returns = return_tys.len();
        let static_return = self.static_aggregate_return_layout(return_tys.iter().copied());
        let static_return_buffer =
            static_return.as_ref().and_then(|layout| self.alloc_static_return_buffer(layout));
        let decode_returndata = return_tys.iter().any(|&ty| {
            self.types.abi_return_type(ty).is_some_and(|ty| !matches!(ty, AbiType::Word(_)))
        });
        let ret_offset = static_return_buffer.as_ref().map_or_else(
            || if !decode_returndata && returns > 1 { input } else { zero },
            |(_, data, _)| *data,
        );
        let ret_size = if let Some((_, _, size)) = static_return_buffer.as_ref() {
            *size
        } else if decode_returndata {
            zero
        } else {
            self.builder.imm_u64((returns as u64).saturating_mul(32))
        };
        (static_return_buffer, ret_offset, ret_size, decode_returndata)
    }

    fn lower_decoded_return_value(
        &mut self,
        data: ValueId,
        return_types: &[Ty<'gcx>],
        span: Span,
        zero: ValueId,
    ) -> Option<ValueId> {
        // values = lower_abi_decode_values(return_data, return_types)
        // multi_return_frame[1..] = values[1..]
        // return values[0] or zero
        let values = self.lower_abi_decode_values(data, return_types, span)?;
        if values.len() > 1 {
            let (object, _, layout) = self.ensure_multi_return_buffer(values.len());
            for (index, value) in values.iter().copied().enumerate().skip(1) {
                let index = self.builder.imm_u64(index as u64);
                self.builder.memory_object_store_element(object, layout, index, value);
            }
        }
        values.into_iter().next().or(Some(zero))
    }

    fn alloc_static_return_buffer(
        &mut self,
        layout: &AbiParamLayout,
    ) -> Option<(ValueId, ValueId, ValueId)> {
        // size = head_size(layout)
        // buffer = bytes(size) if multiple_returns else raw(size)
        // return (buffer, data, size)
        let size = layout.checked_head_size()?;
        if layout.types.len() != 1 {
            let object_size = self.builder.imm_u64(size.checked_add(EvmMemoryLayout::WORD_SIZE)?);
            let object = self.builder.alloc_object(
                object_size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            let size = self.builder.imm_u64(size);
            self.builder.set_memory_object_len(object, size, MemoryObjectKind::Bytes);
            let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
            return Some((object, data, size));
        }
        let size = self.builder.imm_u64(size);
        let data = self.builder.alloc_raw(size, AllocationSemantics::INTERNAL);
        Some((data, data, size))
    }

    fn revert_if_short_returndata(&mut self, expected: ValueId) {
        let actual = self.current_returndata_size();
        let short = self.builder.lt(actual, expected);
        self.revert_if_invalid(short);
    }

    pub(super) fn revert_if_no_code(&mut self, address: ValueId) {
        let size = self.builder.extcodesize(address);
        let missing = self.builder.iszero(size);
        self.revert_if_invalid(missing);
    }

    fn validate_static_returndata(&mut self, offset: ValueId, returns: &[Ty<'gcx>]) {
        // required = returns * 32
        // if returndatasize < required { revert(0, 0) }
        // for i {
        //     word = load_multi_return_value(offset, i, returns.len)
        //     if !valid(returns[i], word) { revert(0, 0) }
        // }
        let words = u64::try_from(returns.len()).unwrap_or(u64::MAX);
        let size = self.builder.imm_u64(words.saturating_mul(32));
        self.revert_if_short_returndata(size);
        for (index, &ty) in returns.iter().enumerate() {
            let value = self.load_multi_return_value(offset, index, returns.len());
            self.validate_external_return_value(ty, value);
        }
    }

    fn validate_external_return_value(&mut self, ty: Ty<'gcx>, value: ValueId) {
        let validator = match ty.peel_refs().kind {
            TyKind::Enum(id) => Some(AbiWordValidator::EnumRange(
                self.context.gcx.hir.enumm(id).variants.len() as u64,
            )),
            TyKind::Fn(_) => None,
            _ => AbiWordValidator::from_return_mir_type(types::TypeLowerer::mir_return_type(ty)),
        };
        let Some(validator) = validator else { return };
        let valid = validator.condition(&mut self.builder, value, false);

        let invalid = self.builder.iszero(valid);
        self.revert_if_invalid(invalid);
    }

    pub(super) fn resolve_call_target(
        &self,
        callee: &hir::Expr<'_>,
        function: hir::FunctionId,
    ) -> hir::FunctionId {
        if let ExprKind::Member(base, _) = callee.kind
            && let Some(TyKind::Type(ty)) = self.context.gcx.type_of_expr(base.id).map(|ty| ty.kind)
        {
            return match ty.kind {
                TyKind::Contract(_) => function,
                TyKind::Super(defining_contract) => self.context.gcx.resolve_super_function(
                    self.context.contract_id,
                    defining_contract,
                    function,
                ),
                _ => self.context.gcx.resolve_virtual_function(self.context.contract_id, function),
            };
        }
        self.context.gcx.resolve_virtual_function(self.context.contract_id, function)
    }
}

fn fixed_bytes_size(ty: Ty<'_>) -> Option<TypeSize> {
    match ty.peel_refs().kind {
        TyKind::Elementary(ElementaryType::FixedBytes(size)) => Some(size),
        _ => None,
    }
}
