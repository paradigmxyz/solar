//! Function calls, conversions, and call-target resolution.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
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
            let value = if let Some(value) = self.lower_fixed_bytes_literal(target_ty, arg) {
                value
            } else if source_ty.is_ref_at(DataLocation::Storage)
                && matches!(
                    source_ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
                && matches!(
                    target_ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
            {
                let access = self.storage_access(arg)?;
                self.load_storage_bytes(access.slot)?
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
            if matches!(
                builtin,
                Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall
            ) && let ExprKind::Member(receiver, _) = callee.kind
            {
                return self.lower_address_call(
                    callee.span,
                    receiver,
                    builtin,
                    args,
                    call_opts,
                    false,
                );
            }
            if matches!(builtin, Builtin::AddressPayableSend | Builtin::AddressPayableTransfer)
                && let ExprKind::Member(receiver, _) = callee.kind
            {
                return self.lower_payable_address_call(receiver, builtin, args);
            }
            return self.lower_builtin_call(expr, callee, builtin, args);
        }
        if let Some(TyKind::Fn(function)) =
            self.context.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            return self.lower_external_function_pointer_call(callee, function, args, call_opts);
        }
        if let Some(TyKind::Fn(function)) =
            self.context.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_internal()
            && function.function_id.is_none()
        {
            return self.lower_internal_function_pointer_call(expr, callee, function, args);
        }
        if let Some(function_id) = self.context.gcx.resolved_function(callee) {
            return self.lower_function_call(expr, callee, function_id, args, call_opts);
        }
        if self.context.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        report_unsupported(self.context.gcx, expr.span, "function call")
    }

    pub(super) fn lower_payable_address_call(
        &mut self,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let amount = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let amount = self.lower_expr(amount)?;
        let zero = self.builder.imm_u256(U256::ZERO);
        let stipend = self.builder.imm_u64(2300);
        let amount_is_zero = self.builder.iszero(amount);
        let gas = self.builder.select(amount_is_zero, stipend, zero);
        let success = self.builder.call(gas, address, amount, zero, zero, zero, zero);
        if builtin == Builtin::AddressPayableTransfer {
            self.revert_external_call(success);
            Some(zero)
        } else {
            Some(success)
        }
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
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    sym::value => call_value = value,
                    sym::salt => salt = Some(value),
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
        let size = self.builder.checked_padded_size(total_len);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, total_len, MemoryObjectKind::Bytes);

        for (index, chunk) in bytecode.chunks(32).enumerate() {
            let offset = self.builder.imm_u64(u64::try_from(index).ok()?.saturating_mul(32));
            let value = self.lower_string_literal_word(chunk);
            self.builder.memory_object_store_word(object, offset, value);
        }
        self.builder.memory_object_copy_from_slice_at(
            object,
            MemoryObjectKind::Bytes,
            bytecode_len_value,
            encoded,
        );

        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
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
        let arg_exprs = self.builtin_arg_exprs(Builtin::AbiEncode, &args)?;
        if arg_exprs.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, args.span, "external function arguments");
        }
        let function_value = self.lower_expr(callee)?;
        let selector_mask = self.builder.imm_u256(U256::from(u32::MAX));
        let selector = self.builder.and(function_value, selector_mask);
        let selector_shift = self.builder.imm_u64(224);
        let selector = self.builder.shl(selector_shift, selector);
        let address_shift = self.builder.imm_u64(32);
        let address = self.builder.shr(address_shift, function_value);

        let zero = self.builder.imm_u256(U256::ZERO);
        let mut gas = self.builder.gas();
        let mut call_value = zero;
        if let Some(options) = call_opts {
            for option in options.args {
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = value,
                    sym::value => call_value = value,
                    _ => {
                        return report_unsupported(
                            self.context.gcx,
                            option.name.span,
                            "call option",
                        );
                    }
                }
            }
        }

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
        let zero = self.builder.imm_u256(U256::ZERO);
        let returns = function.returns.len();
        let static_return = self.static_aggregate_return_layout(function.returns.iter().copied());
        let static_return_buffer =
            static_return.as_ref().and_then(|layout| self.alloc_static_return_buffer(layout));
        let decode_returndata = function.returns.iter().any(|&ret| {
            self.types.abi_return_type(ret).is_some_and(|ty| !matches!(ty, AbiType::Word))
        }) || self.context.gcx.sess.opts.evm_version.supports_returndata();
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
        let success = if matches!(
            function.state_mutability,
            hir::StateMutability::Pure | hir::StateMutability::View
        ) && self.context.gcx.sess.opts.evm_version.has_static_call()
        {
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
            let return_types = function
                .returns
                .iter()
                .copied()
                .map(|ty| ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory))
                .collect::<Vec<_>>();
            return self.lower_abi_decode_values(data, &return_types, callee.span);
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
            let return_types = function
                .returns
                .iter()
                .copied()
                .map(|ty| ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory))
                .collect::<Vec<_>>();
            return self.lower_abi_decode_values(data, &return_types, callee.span);
        }
        if returns > 1 {
            self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, ret_offset);
            let mut values = Vec::with_capacity(returns);
            for index in 0..returns {
                values.push(self.load_multi_return_value_as(
                    ret_offset,
                    index,
                    returns,
                    function.returns[index],
                ));
            }
            return Some(values);
        }
        Some(vec![self.load_multi_return_value_as(ret_offset, 0, returns, function.returns[0])])
    }

    pub(super) fn lower_internal_function_pointer_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function: &TyFn<'gcx>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
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
            let value = self.coerce_call_argument(argument, parameter, value);
            values.push(self.materialize_call_argument(parameter, value, argument.span)?);
        }
        values.insert(0, function_value);

        let dispatcher = self.ensure_internal_function_pointer_dispatcher(function);
        let returns = function.returns.len();
        if returns == 0 {
            self.builder.internal_call_void(dispatcher, values, 0);
            return Some(self.builder.imm_u256(U256::ZERO));
        }
        let result_ty = types::TypeLowerer::mir_return_type(function.returns[0]);
        Some(self.builder.internal_call(dispatcher, values, result_ty, returns))
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
        self.context.pointer_registry.targets.insert(function_id);
        Some(self.builder.imm_u64(internal_function_pointer_id(function_id)))
    }

    pub(super) fn ensure_internal_function_pointer_dispatcher(
        &mut self,
        function: &TyFn<'gcx>,
    ) -> FunctionId {
        let shape = InternalFunctionPointerShape {
            params: function
                .parameters
                .iter()
                .map(|&ty| types::TypeLowerer::mir_type(ty))
                .collect(),
            returns: function
                .returns
                .iter()
                .map(|&ty| types::TypeLowerer::mir_return_type(ty))
                .collect(),
        };
        if let Some(&dispatcher) = self.context.pointer_registry.dispatchers.get(&shape) {
            return dispatcher;
        }
        let index = self.context.pointer_registry.dispatchers.len();
        let dispatcher = self
            .context
            .module
            .add_function(Function::new(Ident::from_str(&format!("__internal_dispatch_{index}"))));
        self.context.pointer_registry.dispatchers.insert(shape, dispatcher);
        dispatcher
    }

    pub(super) fn coerce_value(&mut self, value: ValueId, from: Ty<'gcx>, to: Ty<'gcx>) -> ValueId {
        let source_size = match from.peel_refs().kind {
            TyKind::Elementary(ElementaryType::FixedBytes(size)) => Some(size),
            _ => None,
        };
        let destination_size = match to.peel_refs().kind {
            TyKind::Elementary(ElementaryType::FixedBytes(size)) => Some(size),
            _ => None,
        };
        if let TyKind::Slice(underlying) = from.peel_refs().kind
            && let Some(size) = destination_size
            && matches!(
                underlying.peel_refs().kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
            )
            && let Some(location) = self.builder.func().value_ty(value).and_then(|ty| match ty {
                MirType::Slice(location) => Some(location),
                _ => None,
            })
        {
            let zero = self.builder.imm_u64(0);
            let word = match location {
                SliceLocation::Calldata => self.builder.calldata_slice_load_word(value, zero),
                SliceLocation::Memory => self.builder.memory_slice_load_word(value, zero),
                SliceLocation::Returndata => return value,
            };
            let width = u64::from(size.bytes());
            let fixed_mask =
                self.builder.imm_u256(U256::MAX << (256 - usize::from(size.bytes()) * 8));
            let length = self.builder.slice_len(value);
            let width_value = self.builder.imm_u64(width);
            let short = self.builder.lt(length, width_value);
            let missing = self.builder.sub(width_value, length);
            let bits_per_byte = self.builder.imm_u64(8);
            let shift = self.builder.mul(bits_per_byte, missing);
            let short_mask = self.builder.shl(shift, fixed_mask);
            let mask = self.builder.select(short, short_mask, fixed_mask);
            return self.builder.and(word, mask);
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
                let limit = self.context.gcx.hir.enumm(id).variants.len() as u64;
                let limit = self.builder.imm_u64(limit);
                let valid = self.builder.lt(value, limit);
                let invalid = self.builder.iszero(valid);
                self.builder.panic_if(invalid, PanicCode::EnumConversion);
            }
            return value;
        }
        let Some(size) = destination_size else {
            return value;
        };
        if matches!(from.peel_refs().kind, TyKind::StringLiteral(..))
            && !matches!(
                self.builder.func().value_ty(value),
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
            )
        {
            return value;
        }
        if matches!(from.peel_refs().kind, TyKind::StringLiteral(..)) {
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if matches!(
            from.peel_refs().kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String,)
        ) {
            let value = match self.builder.func().value_ty(value) {
                Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => value,
                Some(MirType::Slice(_)) => self.materialize_memory_slice(value),
                _ => return value,
            };
            let zero = self.builder.imm_u256(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if let Some(source_size) = source_size {
            if source_size.bytes() > size.bytes() {
                let shift = u64::from(32 - size.bytes()) * 8;
                let mask = self.builder.imm_u256(U256::MAX << usize::try_from(shift).unwrap());
                return self.builder.and(value, mask);
            }
            return value;
        }
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn normalize_abi_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Udvt(inner, _) => self.normalize_abi_scalar(value, inner),
            TyKind::Elementary(ElementaryType::UInt(size)) if size.bits() < 256 => {
                let mask = U256::MAX >> (256 - usize::from(size.bits()));
                let mask = self.builder.imm_u256(mask);
                self.builder.and(value, mask)
            }
            TyKind::Elementary(ElementaryType::Int(size)) if size.bits() < 256 => {
                let byte = self.builder.imm_u64(u64::from(size.bits() / 8 - 1));
                self.builder.signextend(byte, value)
            }
            TyKind::Elementary(ElementaryType::Address(_)) => {
                let mask = U256::MAX >> 96;
                let mask = self.builder.imm_u256(mask);
                self.builder.and(value, mask)
            }
            TyKind::Contract(_) => {
                let mask = U256::MAX >> 96;
                let mask = self.builder.imm_u256(mask);
                self.builder.and(value, mask)
            }
            TyKind::Elementary(ElementaryType::FixedBytes(size)) if size.bytes() < 32 => {
                let mask = U256::MAX << (256 - usize::from(size.bytes()) * 8);
                let mask = self.builder.imm_u256(mask);
                self.builder.and(value, mask)
            }
            TyKind::Enum(id) => {
                let variants = self.context.gcx.hir.enumm(id).variants.len().max(1);
                let bits = (usize::BITS - (variants - 1).leading_zeros()).max(1);
                let mask = U256::MAX >> (256 - bits as usize);
                let mask = self.builder.imm_u256(mask);
                self.builder.and(value, mask)
            }
            TyKind::Elementary(ElementaryType::Bool) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                let is_zero = self.builder.eq(value, zero);
                self.builder.iszero(is_zero)
            }
            _ => value,
        }
    }

    pub(super) fn normalize_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let mask = self.builder.imm_u256(U256::MAX >> 64);
            return self.builder.and(value, mask);
        }
        self.normalize_abi_scalar(value, ty)
    }

    pub(super) fn coerce_call_argument(
        &mut self,
        argument: &hir::Expr<'_>,
        parameter_ty: Ty<'gcx>,
        value: ValueId,
    ) -> ValueId {
        let source_ty = self.context.gcx.type_of_expr(argument.id).or_else(|| {
            let ExprKind::Lit(lit) = &argument.kind else { return None };
            let LitKind::Str(_, bytes, _) = &lit.kind else { return None };
            Some(self.context.gcx.mk_ty_string_literal(bytes.as_byte_str()))
        });
        source_ty.map_or(value, |source_ty| self.coerce_value(value, source_ty, parameter_ty))
    }
    pub(super) fn lower_function_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        let function_id = self.resolve_call_target(callee, function_id);
        let function = self.context.gcx.hir.function(function_id);
        let attached =
            self.context.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
        if let ExprKind::Member(receiver, _) = callee.kind
            && self.context.gcx.resolved_builtin(receiver) == Some(Builtin::This)
        {
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
        if !attached
            && matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
            && let Some(address) = self.linked_library_address(function_id)
        {
            return self.lower_linked_library_call(expr, function_id, args, address);
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
        if attached {
            let ExprKind::Member(receiver, _) = callee.kind else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "attached function receiver",
                );
            };
            let parameter_ty = self.context.gcx.type_of_item(function.parameters[0].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                self.storage_access(receiver)?.slot
            } else {
                self.lower_typed_expr(receiver, parameter_ty)?
            };
            let value = self.coerce_call_argument(receiver, parameter_ty, value);
            values.push(self.materialize_call_argument(parameter_ty, value, receiver.span)?);
        }
        for index in receiver_count..function.parameters.len() {
            let Some(argument) =
                args.argument_for_parameter(index - receiver_count, parameter_names.as_deref())
            else {
                return report_unsupported(self.context.gcx, expr.span, "named function argument");
            };
            let parameter_ty = self.context.gcx.type_of_item(function.parameters[index].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                self.storage_access(argument)?.slot
            } else {
                self.lower_typed_expr(argument, parameter_ty)?
            };
            let value = self.coerce_call_argument(argument, parameter_ty, value);
            values.push(self.materialize_call_argument(parameter_ty, value, argument.span)?);
        }
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
        let result_ty = types::TypeLowerer::mir_return_type(
            self.context.gcx.type_of_item((*function.returns.first()?).into()),
        );
        Some(self.builder.internal_call(mir_id, values, result_ty, function.returns.len()))
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
        let ExprKind::Member(receiver, _) = callee.kind else {
            return report_unsupported(self.context.gcx, expr.span, "external function target");
        };
        let function = self.context.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "external function arguments");
        }
        let address = self.lower_expr(receiver)?;
        let zero = self.builder.imm_u256(U256::ZERO);
        let mut call_value = zero;
        let mut gas = self.builder.gas();
        if let Some(options) = call_opts {
            for option in options.args {
                let value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = value,
                    sym::value => call_value = value,
                    _ => {
                        return report_unsupported(
                            self.context.gcx,
                            option.name.span,
                            "call option",
                        );
                    }
                }
            }
        }
        let parameter_names =
            self.context.gcx.callable_param_names(CallableParamSource::Function {
                id: function_id,
                skips_receiver: false,
            });
        let mut values = Vec::with_capacity(function.parameters.len());
        let mut types = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "external function argument",
                );
            };
            let parameter_ty = self.context.gcx.type_of_item(parameter.into());
            let (value, abi_type) = self.lower_abi_call_argument(argument, parameter_ty)?;
            values.push(value);
            types.push(abi_type);
        }
        let selector = self.context.gcx.function_selector(function_id).0;
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let returns = function.returns.len();
        let return_tys = function
            .returns
            .iter()
            .map(|&ret| self.context.gcx.type_of_item(ret.into()))
            .collect::<Vec<_>>();
        let static_return = self.static_aggregate_return_layout(return_tys.iter().copied());
        let static_return_buffer =
            static_return.as_ref().and_then(|layout| self.alloc_static_return_buffer(layout));
        let decode_returndata = function.returns.iter().any(|&ret| {
            self.types
                .abi_return_type(self.context.gcx.type_of_item(ret.into()))
                .is_some_and(|ty| !matches!(ty, AbiType::Word))
        }) || self.context.gcx.sess.opts.evm_version.supports_returndata();
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
        let success = if matches!(
            function.state_mutability,
            hir::StateMutability::Pure | hir::StateMutability::View
        ) && self.context.gcx.sess.opts.evm_version.has_static_call()
        {
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
            let return_types = function
                .returns
                .iter()
                .map(|&ret| {
                    self.context
                        .gcx
                        .type_of_item(ret.into())
                        .with_loc_if_ref(self.context.gcx, DataLocation::Memory)
                })
                .collect::<Vec<_>>();
            let values = self.lower_abi_decode_values(data, &return_types, expr.span)?;
            if values.len() > 1 {
                let (object, _, layout) = self.ensure_multi_return_buffer(values.len());
                for (index, value) in values.iter().copied().enumerate().skip(1) {
                    let index = self.builder.imm_u64(index as u64);
                    self.builder.memory_object_store_element(object, layout, index, value);
                }
            }
            return values.into_iter().next().or(Some(zero));
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
            let return_types = function
                .returns
                .iter()
                .map(|&ret| {
                    self.context
                        .gcx
                        .type_of_item(ret.into())
                        .with_loc_if_ref(self.context.gcx, DataLocation::Memory)
                })
                .collect::<Vec<_>>();
            let values = self.lower_abi_decode_values(data, &return_types, expr.span)?;
            if values.len() > 1 {
                let (object, _, layout) = self.ensure_multi_return_buffer(values.len());
                for (index, value) in values.iter().copied().enumerate().skip(1) {
                    let index = self.builder.imm_u64(index as u64);
                    self.builder.memory_object_store_element(object, layout, index, value);
                }
            }
            return values.into_iter().next().or(Some(zero));
        }
        if self.context.gcx.sess.opts.evm_version.supports_returndata() {
            let size = self.builder.imm_u64((returns as u64).saturating_mul(32));
            self.revert_if_short_returndata(size);
            for (index, &ret) in function.returns.iter().enumerate() {
                let value = self.load_multi_return_value(ret_offset, index, returns);
                self.validate_external_return_value(
                    self.context.gcx.type_of_item(ret.into()),
                    value,
                );
            }
        }
        if returns > 1 {
            self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, ret_offset);
        }
        let ty = self.context.gcx.type_of_item(function.returns[0].into());
        Some(self.load_multi_return_value_as(ret_offset, 0, returns, ty))
    }

    pub(super) fn linked_library_address(&self, function_id: hir::FunctionId) -> Option<U256> {
        let contract_id = self.context.gcx.hir.function(function_id).contract?;
        let contract = self.context.gcx.hir.contract(contract_id);
        if contract.kind != hir::ContractKind::Library {
            return None;
        }
        let source = self.context.gcx.hir.source(contract.source).file.name.display().to_string();
        self.context
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
    }

    pub(super) fn lower_linked_library_call(
        &mut self,
        expr: &hir::Expr<'_>,
        function_id: hir::FunctionId,
        args: hir::CallArgs<'_>,
        address: U256,
    ) -> Option<ValueId> {
        let function = self.context.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return report_unsupported(self.context.gcx, expr.span, "linked library arguments");
        }
        let parameter_names =
            self.context.gcx.callable_param_names(CallableParamSource::Function {
                id: function_id,
                skips_receiver: false,
            });
        let mut values = Vec::with_capacity(function.parameters.len());
        let mut types = Vec::with_capacity(function.parameters.len());
        for (index, &parameter) in function.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.context.gcx, expr.span, "linked library argument");
            };
            let parameter_ty = self.context.gcx.type_of_item(parameter.into());
            let (value, abi_type) = if Self::is_storage_parameter(parameter_ty) {
                (self.storage_access(argument)?.slot, AbiType::Word)
            } else {
                self.lower_abi_call_argument(argument, parameter_ty)?
            };
            values.push(value);
            types.push(abi_type);
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
            .map(|&ret| {
                self.context
                    .gcx
                    .type_of_item(ret.into())
                    .with_loc_if_ref(self.context.gcx, DataLocation::Memory)
            })
            .collect::<Vec<_>>();
        let values = self.lower_abi_decode_values(data, &return_types, expr.span)?;
        if values.len() > 1 {
            let (object, _, layout) = self.ensure_multi_return_buffer(values.len());
            for (index, value) in values.iter().copied().enumerate().skip(1) {
                let index = self.builder.imm_u64(index as u64);
                self.builder.memory_object_store_element(object, layout, index, value);
            }
        }
        values.into_iter().next().or(Some(zero))
    }

    fn static_aggregate_return_layout(
        &mut self,
        returns: impl Iterator<Item = Ty<'gcx>>,
    ) -> Option<AbiParamLayout> {
        let types =
            returns.map(|ty| self.types.abi_return_param_type(ty)).collect::<Option<Vec<_>>>()?;
        (!types.is_empty()
            && types.iter().all(|ty| !ty.is_dynamic())
            && types
                .iter()
                .any(|ty| !matches!(ty, AbiParamType::Scalar(_) | AbiParamType::Enum { .. })))
        .then(|| AbiParamLayout::new(types.into_boxed_slice()))
    }

    fn alloc_static_return_buffer(
        &mut self,
        layout: &AbiParamLayout,
    ) -> Option<(ValueId, ValueId, ValueId)> {
        let size = layout.checked_head_size()?;
        if layout.types.len() != 1 {
            let object_size = size.checked_add(EvmMemoryLayout::WORD_SIZE)?;
            let object_size = self.builder.imm_u64(object_size);
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
        let actual = self.builder.returndata_size();
        let short = self.builder.lt(actual, expected);
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(short, revert, continue_block);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.revert(zero, zero);
        self.builder.switch_to_block(continue_block);
    }

    fn validate_external_return_value(&mut self, ty: Ty<'gcx>, value: ValueId) {
        let ty = ty.peel_refs();
        if let TyKind::Udvt(inner, _) = ty.kind {
            self.validate_external_return_value(inner, value);
            return;
        }

        let valid = match ty.kind {
            TyKind::Enum(id) => {
                let variants =
                    self.builder.imm_u64(self.context.gcx.hir.enumm(id).variants.len() as u64);
                self.builder.lt(value, variants)
            }
            TyKind::Elementary(elementary) => match elementary {
                solar_sema::hir::ElementaryType::UInt(size) if size.bits() < 256 => {
                    let mask = U256::MAX >> (256 - usize::from(size.bits()));
                    let mask = self.builder.imm_u256(mask);
                    let canonical = self.builder.and(value, mask);
                    self.builder.eq(value, canonical)
                }
                solar_sema::hir::ElementaryType::Int(size) if size.bits() < 256 => {
                    let byte = self.builder.imm_u64(u64::from(size.bits() / 8 - 1));
                    let canonical = self.builder.signextend(byte, value);
                    self.builder.eq(value, canonical)
                }
                solar_sema::hir::ElementaryType::Address(_) => {
                    let mask = self.builder.imm_u256(U256::MAX >> 96);
                    let canonical = self.builder.and(value, mask);
                    self.builder.eq(value, canonical)
                }
                solar_sema::hir::ElementaryType::FixedBytes(size) if size.bytes() < 32 => {
                    let mask = U256::MAX << (256 - usize::from(size.bytes()) * 8);
                    let mask = self.builder.imm_u256(mask);
                    let canonical = self.builder.and(value, mask);
                    self.builder.eq(value, canonical)
                }
                solar_sema::hir::ElementaryType::Bool => {
                    let zero = self.builder.iszero(value);
                    let canonical = self.builder.iszero(zero);
                    self.builder.eq(value, canonical)
                }
                _ => return,
            },
            TyKind::Contract(_) | TyKind::Super(_) => {
                let mask = self.builder.imm_u256(U256::MAX >> 96);
                let canonical = self.builder.and(value, mask);
                self.builder.eq(value, canonical)
            }
            _ => return,
        };

        let invalid = self.builder.iszero(valid);
        let revert = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder.branch(invalid, revert, continue_block);
        self.builder.switch_to_block(revert);
        let zero = self.builder.imm_u256(U256::ZERO);
        self.builder.revert(zero, zero);
        self.builder.switch_to_block(continue_block);
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
    pub(super) fn is_low_level_call_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let ExprKind::Call(callee, ..) = &expr.kind else { return false };
        matches!(
            self.context.gcx.resolved_builtin(callee),
            Some(Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall)
        ) && matches!(callee.kind, ExprKind::Member(..))
    }

    pub(super) fn lower_low_level_call_values(
        &mut self,
        expr: &hir::Expr<'_>,
        count: usize,
        first_is_omitted: bool,
    ) -> Option<Vec<ValueId>> {
        let ExprKind::Call(callee, args, call_opts) = &expr.kind else { return None };
        let builtin = self.context.gcx.resolved_builtin(callee)?;
        if !matches!(
            builtin,
            Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall
        ) {
            return None;
        }
        let ExprKind::Member(receiver, _) = callee.kind else { return None };
        let capture_returndata = count > 1 || first_is_omitted;
        let (success, returndata) = self.lower_address_call_result(
            callee.span,
            receiver,
            builtin,
            *args,
            *call_opts,
            capture_returndata,
        )?;
        if count <= 1 && !first_is_omitted {
            return Some(vec![success]);
        }
        if count != 2 {
            let Some(returndata) = returndata else {
                return report_unsupported(
                    self.context.gcx,
                    expr.span,
                    "low-level call return values",
                );
            };
            return Some(vec![returndata]);
        }
        let Some(returndata) = returndata else {
            return report_unsupported(self.context.gcx, expr.span, "low-level call return values");
        };
        Some(vec![success, returndata])
    }
}
