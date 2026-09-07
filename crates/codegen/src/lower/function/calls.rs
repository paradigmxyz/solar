//! Function calls, conversions, and call-target resolution.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ExternalReturnPlan {
    static_buffer: Option<(ValueId, ValueId, ValueId)>,
    offset: ValueId,
    size: ValueId,
    /// Whether the output area overlays the input area, as it does before Byzantium.
    overlays_input: bool,
    decode_returndata: bool,
}

impl ExternalReturnPlan {
    /// A plan for a call that declares no output area and decodes its return values from the
    /// return data, which only exists from Byzantium on.
    fn returndata(zero: ValueId) -> Self {
        Self {
            static_buffer: None,
            offset: zero,
            size: zero,
            overlays_input: false,
            decode_returndata: true,
        }
    }

    /// The `(offset, size)` output-area operands of the call the plan was built for.
    pub(super) fn output_area(&self) -> (ValueId, ValueId) {
        (self.offset, self.size)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ExternalReturnMode {
    First,
    All,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn uses_static_call(&self, state_mutability: hir::StateMutability) -> bool {
        matches!(state_mutability, hir::StateMutability::Pure | hir::StateMutability::View)
            && self.cx.gcx.sess.opts.evm_version.has_static_call()
    }

    /// Returns `true` if a call expecting `returns` return values must check that the callee has
    /// code.
    ///
    /// A call that expects return data needs no check from Byzantium on: a code-less callee
    /// returns nothing, so the return-data length check reverts anyway. Before Byzantium there is
    /// no `RETURNDATASIZE` and nothing subsumes the check, so every call needs it. This is solc's
    /// rule, `encodedHeadSize == 0 || !supportsReturndata()`, and it holds for every receiver:
    /// `this` is no exception, because the executing code and `address(this)` come apart under
    /// `DELEGATECALL`, where the account may well have no code of its own.
    ///
    /// With `--revert-strings debug` the check is always emitted, as in solc, so a code-less
    /// target reports "Target contract does not contain code" instead of a decoding failure.
    pub(super) fn needs_code_check(&self, returns: usize) -> bool {
        let opts = &self.cx.gcx.sess.opts;
        returns == 0 || !opts.evm_version.supports_returndata() || opts.revert_strings.is_debug()
    }

    /// Returns the `gas` operand of an external call, materializing the pre-EIP-150 reserve when
    /// [`LoweredCallOptions::gas`] left it to the call site.
    ///
    /// `may_create_account` says whether the call can create the callee's account, which a
    /// pre-EIP-150 `CALL` charges the caller for. Emit this immediately before the call: on such
    /// a target everything after the `GAS` runs on the withheld gas.
    pub(super) fn call_gas(
        &mut self,
        gas: Option<ValueId>,
        sends_value: bool,
        may_create_account: bool,
    ) -> ValueId {
        gas.unwrap_or_else(|| {
            crate::utils::pre_tangerine_call_gas(&mut self.builder, sends_value, may_create_account)
        })
    }

    pub(super) fn lower_user_operator(
        &mut self,
        span: Span,
        function_id: hir::FunctionId,
        values: &[ValueId],
    ) -> Option<ValueId> {
        let function = self.cx.gcx.hir.function(function_id);
        if function.parameters.len() != values.len() || function.returns.len() != 1 {
            return self.cx.report_unsupported(span, "user-defined operator signature");
        }
        let Some(&mir_id) = self.cx.function_ids.get(&function_id) else {
            return self.cx.report_unsupported(span, "user-defined operator function");
        };
        let result_ty = types::TypeLowerer::mir_return_type(
            self.cx.gcx.type_of_item(function.returns[0].into()),
        );
        let result = self.builder.icall(mir_id, values.to_vec(), result_ty, 1);
        self.dirty_values.insert(result);
        Some(result)
    }

    pub(super) fn lower_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        if let Some(struct_id) = self.cx.gcx.resolved_expr(callee).and_then(|res| match res {
            hir::Res::Item(item) => item.as_struct(),
            _ => None,
        }) {
            // result = lower_struct_ctor(callee, args)
            return self.lower_struct_constructor(expr, struct_id, args);
        }
        let is_type_conversion = matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_))
            || self.cx.gcx.resolved_expr(callee).is_some_and(|res| {
                matches!(res, hir::Res::Item(hir::ItemId::Contract(_) | hir::ItemId::Enum(_)))
            });
        if is_type_conversion {
            // result = convert(callee, args)
            if args.len() != 1 {
                return self.cx.report_unsupported(expr.span, "type conversion");
            }
            let Some(arg) = args.exprs().next() else {
                return self.cx.report_unsupported(expr.span, "type conversion");
            };
            let source_ty = self.cx.gcx.type_of_expr(arg.id)?;
            let target_ty = self.cx.gcx.type_of_expr(expr.id).or_else(|| {
                self.cx.gcx.resolved_expr(callee).and_then(|res| match res {
                    hir::Res::Item(id @ (hir::ItemId::Contract(_) | hir::ItemId::Enum(_))) => {
                        Some(self.cx.gcx.type_of_item(id))
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
                    return self.cx.report_unsupported(arg.span, "storage access");
                };
                self.load_storage_bytes(access.slot)
            } else {
                self.lower_expr(arg)?
            };
            return Some(self.coerce_value(value, source_ty, target_ty));
        }
        if let ExprKind::New(ty) = &callee.kind {
            if let TyKind::Contract(contract_id) = self.cx.gcx.type_of_hir_ty(ty).kind {
                // result = create_contract(callee, args, opts)
                return self.lower_new_contract(expr, ty, contract_id, args, call_opts);
            }
            if args.len() != 1 {
                return self.cx.report_unsupported(expr.span, "dynamic allocation");
            }
            let Some(arg) = args.exprs().next() else {
                return self.cx.report_unsupported(expr.span, "dynamic allocation");
            };
            let len = self.lower_typed_expr(arg, self.cx.gcx.types.uint(256))?;
            let ty = self.cx.gcx.type_of_expr(expr.id)?;
            let layout = self.types.memory_layout(ty)?;
            let size = match layout {
                MemoryObjectLayout::Bytes => self.builder.checked_padded_size(len),
                MemoryObjectLayout::DynamicArray { element_words } => {
                    let stride = self.builder.imm(u64::from(element_words));
                    let payload = self.builder.checked_mul(len, stride);
                    let one = self.builder.imm(1);
                    let words = self.builder.checked_add(payload, one);
                    let word_size = self.builder.imm(32);
                    self.builder.checked_mul(words, word_size)
                }
                _ => return self.cx.report_unsupported(expr.span, "allocation type"),
            };
            // result = alloc(size, zeroed)
            // result.length = length
            let object =
                self.builder.alloc_object(size, layout, AllocationSemantics::SOLIDITY_ZEROED);
            self.builder.set_memory_object_len(object, len, layout.kind());
            if let TyKind::DynArray(element) = ty.peel_refs().kind
                && self.types.memory_layout(element).is_some()
            {
                // for i in 0..length { object[i] = default(element) }
                self.counted_loop(len, |this, index| {
                    let value = this.default_binding_value(element);
                    this.builder.memory_object_store_element(object, layout, index, value);
                    Some(())
                })?;
            }
            return Some(object);
        }
        if let Some(builtin) = self.cx.gcx.resolved_builtin(callee) {
            // result = builtin(callee, args, opts)
            return self.lower_builtin_call(expr, callee, builtin, args, call_opts);
        }
        if let Some(TyKind::Fn(function)) = self.cx.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.function_id.is_none()
        {
            if function.is_external() {
                // result = external_function_pointer_call(callee, args, opts)
                return self
                    .lower_external_function_pointer_call(callee, function, args, call_opts);
            }
            if function.is_internal() {
                // result = internal_function_pointer_call(callee, args)
                return self.lower_internal_function_pointer_call(expr, callee, function, args);
            }
        }
        if let Some(function_id) = self.cx.gcx.resolved_function(callee) {
            // result = function_call(callee, args, opts)
            return self.lower_function_call(expr, callee, function_id, args, call_opts);
        }
        if self.cx.gcx.dcx().has_errors().is_err() {
            return Some(self.builder.imm(U256::ZERO));
        }
        self.cx.report_unsupported(expr.span, "function call")
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
        let contract = self.cx.gcx.hir.contract(contract_id);
        let bytecode = self
            .cx
            .child_bytecodes
            .get(&contract_id)
            .and_then(super::super::data::ContractBytecodes::deployment)
            .ok_or_else(|| {
                self.cx
                    .gcx
                    .dcx()
                    .err(format!(
                        "codegen is missing creation bytecode for `new {}`",
                        contract.name
                    ))
                    .span(ty.span)
                    .note("the deployed contract did not compile or was not lowered first")
                    .emit()
            });
        let Ok(bytecode) = bytecode else { return None };

        let mut call_value = self.builder.imm(U256::ZERO);
        let mut salt = None;
        if let Some(options) = call_opts {
            for option in options.args {
                match option.name.name {
                    sym::value => {
                        call_value =
                            self.lower_typed_expr(&option.value, self.cx.gcx.types.uint(256))?;
                    }
                    sym::salt => {
                        salt =
                            Some(self.lower_typed_expr(
                                &option.value,
                                self.cx.gcx.types.fixed_bytes(32),
                            )?);
                    }
                    _ => {
                        return self.cx.report_unsupported(option.name.span, "creation option");
                    }
                }
            }
        }

        let (parameters, parameter_names) = contract
            .ctor
            .map(|id| {
                let constructor = self.cx.gcx.hir.function(id);
                (
                    constructor.parameters,
                    self.cx.gcx.callable_param_names(CallableParamSource::Function {
                        id,
                        skips_receiver: false,
                    }),
                )
            })
            .unwrap_or((&[], Vec::new().into()));
        if args.len() != parameters.len() {
            return self.cx.report_unsupported(args.span, "constructor argument list");
        }

        let arguments = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: parameters.len(),
                names: Some(parameter_names.as_slice()),
                reverse: false,
            },
            args.span,
            "constructor argument",
            |this, index, argument| {
                let parameter_ty = this.cx.gcx.type_of_item(parameters[index].into());
                this.lower_abi_call_argument(argument, parameter_ty)
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = arguments.into_iter().unzip();
        // arguments = abi_encode(constructor_args)
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(Arc::clone(&layout), None, values.into_boxed_slice());
        let encoded_len = if layout.types.iter().any(AbiType::is_dynamic) {
            self.builder.slice_len(encoded)
        } else {
            self.builder.imm(layout.head_size())
        };

        let bytecode_len = u64::try_from(bytecode.len()).ok()?;
        let bytecode_len_value = self.builder.imm(bytecode_len);
        let total_len = self.builder.checked_add(bytecode_len_value, encoded_len);
        // CREATE consumes a raw byte range, so do not reserve a semantic bytes
        // header that no later operation can observe.
        let padding = self.builder.imm(31);
        let rounded_len = self.builder.checked_add(total_len, padding);
        let mask = self.builder.not(padding);
        let allocation_size = self.builder.and(rounded_len, mask);
        let data = self.builder.alloc_raw(allocation_size, AllocationSemantics::INTERNAL);

        super::super::data::copy_data_to_memory(
            self.cx.gcx,
            self.cx.module,
            &mut self.builder,
            data,
            bytecode,
            bytecode.len(),
            Some(super::super::data::contract_bytecode_data_name(self.cx.gcx, contract_id, true)),
        );
        let encoded_ptr = self.builder.slice_ptr(encoded);
        let copy_dest = self.builder.add(data, bytecode_len_value);
        // init = creation_bytecode ++ arguments
        self.builder.copy_slice_data(SliceLocation::Memory, copy_dest, encoded_ptr, encoded_len);
        // address = create|create2(value, init, init.length[, salt])
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
        Some(values.into_iter().next().unwrap_or_else(|| self.builder.imm(U256::ZERO)))
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
            return self.cx.report_unsupported(args.span, "external function argument list");
        }
        let function_value = self.lower_expr(callee)?;
        // address, selector = split_function_pointer(function)
        let (address, selector) = self.split_external_function_pointer(function_value);

        let options = self.lower_call_options(call_opts, true, "call option")?;

        let values_and_types = self.lower_argument_exprs(
            CallArgumentParams { count: arg_exprs.len(), names: None, reverse: false },
            arg_exprs.iter().enumerate(),
            |this, index, argument| {
                this.lower_abi_call_argument(argument, function.parameters[index])
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = values_and_types.into_iter().unzip();
        let return_tys = self.external_return_types(function.returns);
        let returns = return_tys.len();
        // buffer = alloc_overlay_return_buffer(returns)
        // input = abi_encode(selector, args)
        let overlay_buffer = self.alloc_overlay_return_buffer(&return_tys);
        // mstore(add(fmp(), ret_size), 0)
        self.touch_call_output_area(options.gas, &return_tys, overlay_buffer.is_some());
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        // ret_offset, ret_size, decode = plan_return_buffer(returns)
        let return_plan = self.plan_return_buffer(input, options.zero, &return_tys, overlay_buffer);
        if self.needs_code_check(returns) {
            self.revert_if_no_code(address);
        }
        // The code check above is emitted at every version that needs the reserve, so the call
        // cannot create the callee's account.
        // gas = gas() | sub(gas(), reserve)
        let gas = self.call_gas(options.gas, options.value_set, false);
        // ok = CALL|STATICCALL(gas, address, value, input, ret_offset, ret_size)
        let success = if self.uses_static_call(function.state_mutability) {
            self.builder.staticcall(
                gas,
                address,
                input,
                input_size,
                return_plan.offset,
                return_plan.size,
            )
        } else {
            self.builder.call(
                gas,
                address,
                options.value,
                input,
                input_size,
                return_plan.offset,
                return_plan.size,
            )
        };
        // if !ok { revert(0, returndatasize()) }
        self.revert_external_call(success);
        // results = decode_buffer | decode_returndata | load_words(ret_offset)
        self.finish_external_call(
            return_plan,
            &return_tys,
            callee.span,
            ExternalReturnMode::All,
            "codegen cannot decode external function-pointer returndata before Byzantium",
        )
    }

    pub(super) fn split_external_function_pointer(
        &mut self,
        function_value: ValueId,
    ) -> (ValueId, ValueId) {
        let selector_mask = self.builder.imm(u32::MAX);
        let selector = self.builder.and(function_value, selector_mask);
        let selector_shift = self.builder.imm(224);
        let selector = self.builder.shl(selector_shift, selector);
        let address_shift = self.builder.imm(32);
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
        if args.len() != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "internal function argument list");
        }
        let function_value = self.lower_expr(callee)?;
        let parameter_names = self
            .cx
            .gcx
            .call_param_source(callee)
            .map(|source| self.cx.gcx.callable_param_names(source));
        let mut values = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: function.parameters.len(),
                names: parameter_names.as_deref(),
                reverse: false,
            },
            expr.span,
            "named internal function argument",
            |this, index, argument| {
                let parameter = function.parameters[index];
                let value = this.lower_typed_expr(argument, parameter)?;
                this.materialize_call_argument(parameter, value, argument.span)
            },
        )?;
        values.insert(0, function_value);

        let dispatcher = self.ensure_internal_function_pointer_dispatcher(function);
        if function.returns.is_empty() {
            // icall_void(dispatcher, function, args)
            // result = 0
            self.builder.icall_void(dispatcher, values, 0);
            return Some(self.builder.imm(U256::ZERO));
        }
        let first_ty = function.returns[0];
        let result_ty = types::TypeLowerer::mir_return_type(first_ty);
        // result = icall(dispatcher, function, args)
        let result = self.builder.icall(dispatcher, values, result_ty, function.returns.len());
        self.dirty_values.insert(result);
        Some(result)
    }

    pub(super) fn lower_internal_function_value(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let TyKind::Fn(function) = self.cx.gcx.type_of_expr(expr.id)?.kind else {
            return None;
        };
        if !function.is_internal() {
            return None;
        }
        let hir::Res::Item(hir::ItemId::Function(function_id)) = self.cx.gcx.resolved_expr(expr)?
        else {
            return None;
        };
        let function_id = self.resolve_call_target(expr, function_id);
        self.cx.state.pointer_registry.targets.insert(function_id);
        Some(self.builder.imm(internal_function_pointer_id(function_id)))
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
            let zero = self.builder.imm(0);
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
                // value = word & mask(min(length, fixed_width))
                let width = u64::from(size.bytes());
                let fixed_mask =
                    self.builder.imm(U256::MAX << (256 - usize::from(size.bytes()) * 8));
                let width_value = self.builder.imm(width);
                let short = self.builder.lt(length, width_value);
                let missing = self.builder.sub(width_value, length);
                let bits_per_byte = self.builder.imm(8);
                let shift = self.builder.mul(bits_per_byte, missing);
                let short_mask = self.builder.shl(shift, fixed_mask);
                let mask = self.builder.select(short, short_mask, fixed_mask);
                return self.builder.and(word, mask);
            }
        }
        if destination_size.is_some()
            && let Some(abi_type) = self.types.abi_type(from)
        {
            // validate_calldata_bytes(value)
            self.validate_calldata_bytes_argument(value, &abi_type);
        }
        // value = fixed_bytes_to_scalar(value)
        let value = if let Some(size) = source_size
            && destination_size.is_none()
            && u64::from(32 - size.bytes()) * 8 != 0
        {
            let shift = self.builder.imm(u64::from(32 - size.bytes()) * 8);
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
            // value = normalize_integer(value, to)
            return self.normalize_abi_scalar(value, to);
        }
        if let TyKind::Enum(id) = to.peel_refs().kind {
            if !matches!(from.peel_refs().kind, TyKind::Enum(from_id) if from_id == id) {
                // validate_enum(to, value)
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
            // value = mload(bytes.data)
            let zero = self.builder.imm(U256::ZERO);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        if let Some(source_size) = source_size {
            if source_size.bytes() > size.bytes() {
                // value = clean_fixed_bytes(value, to.width)
                return self.clean_fixed_bytes(value, size.bytes());
            }
            return value;
        }
        // value = value << (32 - to.width) * 8
        let shift = self.builder.imm(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn normalize_abi_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Enum(..) => {
                self.validate_enum(ty, value);
                value
            }
            TyKind::Fn(_) => value,
            TyKind::Elementary(ElementaryType::Bool) => {
                let zero = self.builder.imm(U256::ZERO);
                let is_zero = self.builder.eq(value, zero);
                self.builder.iszero(is_zero)
            }
            _ => AbiWordValidator::from_mir_type(types::TypeLowerer::mir_type(ty))
                .map_or(value, |validator| validator.cleanup(&mut self.builder, value)),
        }
    }

    pub(super) fn normalize_dirty_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        if self.in_inline_assembly || !self.dirty_values.contains(&value) {
            return value;
        }
        self.normalize_abi_scalar(value, ty)
    }

    // External functions are low-aligned as scalar MIR values, but Solidity memory stores their
    // 24-byte representation left-aligned. Keep the conversion at typed memory boundaries.
    pub(super) fn normalize_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if matches!(ty.peel_refs().kind, TyKind::Fn(function) if function.is_external()) {
            let shift = self.builder.imm(64);
            return self.builder.shr(shift, value);
        }
        self.normalize_abi_scalar(value, ty)
    }

    pub(super) fn decode_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm(64);
            return self.builder.shr(shift, value);
        }
        value
    }

    pub(super) fn encode_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm(64);
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
        let function = self.cx.gcx.hir.function(function_id);
        let attached = self.cx.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
        let delegate_call = self.cx.gcx.type_of_expr(callee.id).is_some_and(
            |ty| matches!(ty.kind, TyKind::Fn(function) if function.is_delegate_call()),
        );
        let attached_receiver = if attached {
            let ExprKind::Member(receiver, _) = callee.kind else {
                return self.cx.report_unsupported(expr.span, "attached function receiver");
            };
            Some(receiver)
        } else {
            None
        };
        if let ExprKind::Member(receiver, _) = callee.kind
            && self.cx.gcx.resolved_builtin(receiver) == Some(Builtin::This)
        {
            // result = external_abi_call(this, function, args, opts)
            let function_id = self.resolve_call_target(callee, function_id);
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        }
        if !attached
            && let ExprKind::Member(receiver, _) = callee.kind
            && self
                .cx
                .gcx
                .type_of_expr(receiver.id)
                .is_some_and(|ty| matches!(ty.peel_refs().kind, TyKind::Contract(_)))
            && !matches!(
                function.contract.map(|id| self.cx.gcx.hir.contract(id).kind),
                Some(hir::ContractKind::Library)
            )
        {
            // result = external_abi_call(receiver, function, args, opts)
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        }
        let function_id = self.resolve_call_target(callee, function_id);
        let function = self.cx.gcx.hir.function(function_id);
        if delegate_call {
            // result = delegatecall(library, function, args)
            let address = self.library_address(function_id);
            return self.lower_library_call(expr, function_id, attached_receiver, args, address);
        }
        // call_args = materialize(receiver, args)
        let receiver_count = usize::from(attached);
        if args.len() + receiver_count != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "function argument list");
        }
        let parameter_names = self
            .cx
            .gcx
            .call_param_source(callee)
            .map(|source| self.cx.gcx.callable_param_names(source));
        let mut values = Vec::with_capacity(function.parameters.len());
        if let Some(receiver) = attached_receiver {
            let parameter_ty = self.cx.gcx.type_of_item(function.parameters[0].into());
            let value = if Self::is_storage_parameter(parameter_ty) {
                let Some(access) = self.storage_access(receiver) else {
                    return self.cx.report_unsupported(receiver.span, "storage access");
                };
                access.slot
            } else {
                self.lower_typed_expr(receiver, parameter_ty)?
            };
            values.push(self.materialize_call_argument(parameter_ty, value, receiver.span)?);
        }
        let arguments = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: function.parameters.len() - receiver_count,
                names: parameter_names.as_deref(),
                reverse: function.is_yul,
            },
            expr.span,
            "named function argument",
            |this, argument_index, argument| {
                let parameter = function.parameters[argument_index + receiver_count];
                let parameter_ty = this.cx.gcx.type_of_item(parameter.into());
                let value = if Self::is_storage_parameter(parameter_ty) {
                    let Some(access) = this.storage_access(argument) else {
                        return this.cx.report_unsupported(argument.span, "storage access");
                    };
                    access.slot
                } else {
                    this.lower_typed_expr(argument, parameter_ty)?
                };
                this.materialize_call_argument(parameter_ty, value, argument.span)
            },
        )?;
        values.extend(arguments);
        let Some(&mir_id) = self.cx.function_ids.get(&function_id) else {
            return self.lower_external_function_call(expr, callee, function_id, args, call_opts);
        };
        if let Some(value) = self.lower_pure_struct_constructor(function, &values) {
            return Some(value);
        }
        if function.returns.is_empty() {
            // icall_void(function, call_args)
            // result = 0
            self.builder.icall_void(mir_id, values, 0);
            return Some(self.builder.imm(U256::ZERO));
        }
        let first_ty = self.cx.gcx.type_of_item((*function.returns.first()?).into());
        let result_ty = types::TypeLowerer::mir_return_type(first_ty);
        // result = icall(function, call_args)
        let result = self.builder.icall(mir_id, values, result_ty, function.returns.len());
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
        let return_ty = self.cx.gcx.type_of_item(function.returns[0].into());
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
        let Some(hir::Res::Item(item)) = self.cx.gcx.resolved_expr(constructor) else {
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
            return self.cx.report_unsupported(expr.span, "external function target");
        };
        let function = self.cx.gcx.hir.function(function_id);
        if args.len() != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "external function argument list");
        }
        let address = self.lower_expr(receiver)?;
        let options = self.lower_call_options(call_opts, true, "call option")?;
        let parameter_names = self.cx.gcx.callable_param_names(CallableParamSource::Function {
            id: function_id,
            skips_receiver: false,
        });
        let gcx = self.cx.gcx;
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
        let return_tys = function
            .returns
            .iter()
            .map(|&ret| self.cx.gcx.type_of_item(ret.into()))
            .collect::<Vec<_>>();
        let return_tys = self.external_return_types(&return_tys);
        let returns = return_tys.len();
        // buffer = alloc_overlay_return_buffer(returns)
        // input = abi_encode(selector, args)
        let overlay_buffer = self.alloc_overlay_return_buffer(&return_tys);
        // mstore(add(fmp(), ret_size), 0)
        self.touch_call_output_area(options.gas, &return_tys, overlay_buffer.is_some());
        let selector = self.cx.gcx.function_selector(function_id).0;
        let selector = self.builder.imm(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        // ret_offset, ret_size, decode = plan_return_buffer(returns)
        let return_plan = self.plan_return_buffer(input, options.zero, &return_tys, overlay_buffer);
        if self.needs_code_check(returns) {
            self.revert_if_no_code(address);
        }
        // The code check above is emitted at every version that needs the reserve, so the call
        // cannot create the callee's account.
        // gas = gas() | sub(gas(), reserve)
        let gas = self.call_gas(options.gas, options.value_set, false);
        // ok = CALL|STATICCALL(gas, address, value, input, ret_offset, ret_size)
        let success = if self.uses_static_call(function.state_mutability) {
            self.builder.staticcall(
                gas,
                address,
                input,
                input_size,
                return_plan.offset,
                return_plan.size,
            )
        } else {
            self.builder.call(
                gas,
                address,
                options.value,
                input,
                input_size,
                return_plan.offset,
                return_plan.size,
            )
        };
        // if !ok { revert(0, returndatasize()) }
        self.revert_external_call(success);
        // result = decode_buffer | decode_returndata | load_words(ret_offset)
        let values = self.finish_external_call(
            return_plan,
            &return_tys,
            expr.span,
            ExternalReturnMode::First,
            "codegen cannot decode external function returndata before Byzantium",
        )?;
        Some(values.into_iter().next().unwrap_or(options.zero))
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
        let parameter_types = parameter_types.collect::<Vec<_>>();
        let values_and_types = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: parameter_types.len(),
                names: parameter_names.map(CallableParamNames::as_slice),
                reverse: false,
            },
            span,
            error,
            |this, index, argument| {
                let parameter_ty = parameter_types[index];
                if storage_parameters && Self::is_storage_parameter(parameter_ty) {
                    let Some(access) = this.storage_access(argument) else {
                        return this.cx.report_unsupported(argument.span, "storage access");
                    };
                    Some((access.slot, AbiType::Word(None)))
                } else {
                    this.lower_abi_call_argument(argument, parameter_ty)
                }
            },
        )?;
        let (values, types) = values_and_types.into_iter().unzip();
        Some((values, types))
    }

    fn linked_library_address(&self, function_id: hir::FunctionId) -> Option<U256> {
        let contract_id = self
            .cx
            .gcx
            .hir
            .function(function_id)
            .contract
            .expect("library function must have a contract");
        let contract = self.cx.gcx.hir.contract(contract_id);
        assert_eq!(contract.kind, hir::ContractKind::Library);
        let source = self.cx.gcx.hir.source(contract.source).file.name.display().to_string();
        let libraries = &self.cx.gcx.sess.opts.libraries;
        let name = contract.name.as_str_in(self.cx.gcx.sess);
        libraries
            .iter()
            .find(|library| library.name == name && library.source.as_deref() == Some(&source))
            .or_else(|| {
                libraries.iter().find(|library| library.name == name && library.source.is_none())
            })
            .map(|library| U256::from_be_slice(library.address.as_slice()))
    }

    pub(super) fn library_address(&mut self, function_id: hir::FunctionId) -> U256 {
        if let Some(address) = self.linked_library_address(function_id) {
            return address;
        }

        let contract_id = self
            .cx
            .gcx
            .hir
            .function(function_id)
            .contract
            .expect("library function must have a contract");
        let contract = self.cx.gcx.hir.contract(contract_id);
        let source = self.cx.gcx.hir.source(contract.source).file.name.display().to_string();

        let name = contract.name.as_str_in(self.cx.gcx.sess).to_string();
        // The source map keeps absolute file names under `-Zui-testing` (the UI runner relies
        // on them), so hash only the file name there; otherwise the placeholder would change
        // with the checkout path and no blessed output could pin it.
        let hashed_source = if self.cx.gcx.sess.opts.unstable.ui_testing
            && let Some(file_name) = std::path::Path::new(&source).file_name()
        {
            file_name.to_string_lossy().into_owned()
        } else {
            source.clone()
        };
        let hash = keccak256(format!("{hashed_source}:{name}"));
        let mut placeholder = <[u8; 20]>::try_from(&hash[..20]).unwrap();
        placeholder[0] |= 0x80;
        self.cx.module.add_library_link(LibraryLink { source, name, placeholder });
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
        let function = self.cx.gcx.hir.function(function_id);
        let receiver_count = usize::from(receiver.is_some());
        if args.len() + receiver_count != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "library argument list");
        }
        let parameter_names = self.cx.gcx.callable_param_names(CallableParamSource::Function {
            id: function_id,
            skips_receiver: receiver.is_some(),
        });
        let gcx = self.cx.gcx;
        let parameter_types =
            function.parameters.iter().map(move |&parameter| gcx.type_of_item(parameter.into()));
        let mut parameter_types = parameter_types.skip(receiver_count);
        // receiver = lower_abi_receiver(receiver)
        let receiver = if let Some(receiver) = receiver {
            let parameter_ty = gcx.type_of_item(function.parameters[0].into());
            Some(self.lower_abi_receiver(receiver, parameter_ty)?)
        } else {
            None
        };
        let (mut values, mut types) = self.lower_abi_call_arguments(
            args,
            &mut parameter_types,
            Some(&parameter_names),
            expr.span,
            "library argument",
            true,
        )?;
        if let Some((value, ty)) = receiver {
            values.insert(0, value);
            types.insert(0, ty);
        }

        let evm_version = self.cx.gcx.sess.opts.evm_version;
        let return_types = function
            .returns
            .iter()
            .map(|&ret| self.cx.gcx.type_of_item(ret.into()))
            .collect::<Vec<_>>();
        let return_types = self.external_return_types(&return_types);
        // buffer = alloc_overlay_return_buffer(returns)
        // input = abi_encode(selector, args)
        let overlay_buffer = self.alloc_overlay_return_buffer(&return_types);
        // A library call takes no call options, so its gas operand is never already materialized.
        // mstore(add(fmp(), ret_size), 0)
        self.touch_call_output_area(None, &return_types, overlay_buffer.is_some());
        let selector = self.cx.gcx.function_selector(function_id).0;
        let selector = self.builder.imm(U256::from_be_slice(&selector) << 224);
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let input = self.builder.slice_ptr(encoded);
        let input_size = self.builder.slice_len(encoded);
        let zero = self.builder.imm(U256::ZERO);
        let address = self.builder.imm(address);
        let gas = evm_version.can_overcharge_gas_for_call().then(|| self.builder.gas());
        // From Byzantium on the return values come out of the return data; before it the
        // delegatecall writes them into an output area overlaying its input and the success path
        // reads them back from there, as solc's static output size does.
        // ret_offset, ret_size = plan_return_buffer(returns)
        let return_plan = if evm_version.supports_returndata() {
            ExternalReturnPlan::returndata(zero)
        } else {
            self.plan_return_buffer(input, zero, &return_types, overlay_buffer)
        };
        if self.needs_code_check(return_types.len()) {
            self.revert_if_no_code(address);
        }
        // A delegatecall transfers no value and creates no account, so the pre-EIP-150 reserve is
        // the call's base cost alone.
        // gas = gas() | sub(gas(), reserve)
        let gas = self.call_gas(gas, false, false);
        // ok = delegatecall(gas, library, input, ret_offset, ret_size)
        let success = self.builder.delegatecall(
            gas,
            address,
            input,
            input_size,
            return_plan.offset,
            return_plan.size,
        );
        // if !ok { revert(0, returndatasize()) }
        self.revert_external_call(success);
        if return_types.is_empty() {
            return Some(zero);
        }
        // result = load_words(ret_offset) | abi_decode(buffer) | abi_decode(returndata)
        let values = self.finish_external_call(
            return_plan,
            &return_types,
            expr.span,
            ExternalReturnMode::First,
            "codegen cannot decode linked library returndata before Byzantium",
        )?;
        Some(values.into_iter().next().unwrap_or(zero))
    }

    pub(super) fn lower_abi_receiver(
        &mut self,
        receiver: &hir::Expr<'_>,
        parameter_ty: Ty<'gcx>,
    ) -> Option<(ValueId, AbiType)> {
        if Self::is_storage_parameter(parameter_ty) {
            let Some(access) = self.storage_access(receiver) else {
                return self.cx.report_unsupported(receiver.span, "storage access");
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

    /// Replaces every dynamically encoded return type with a word before Byzantium.
    ///
    /// Without `RETURNDATASIZE` the size of such a value is unknowable, so solc types it as an
    /// inaccessible dynamic type whose decoding type is `uint256`
    /// (`FunctionType::returnParameterTypesWithoutDynamicTypes`): the call reserves a word for it
    /// in its output area and nothing decodes it, because the type checker rejects every use of
    /// the value. The remaining return values stay accessible, as in solc.
    pub(super) fn external_return_types(&mut self, return_tys: &[Ty<'gcx>]) -> Vec<Ty<'gcx>> {
        if self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            return return_tys.to_vec();
        }
        return_tys
            .iter()
            .map(|&ty| {
                if self.types.abi_return_type(ty).is_some_and(|abi| abi.is_dynamic()) {
                    self.cx.gcx.types.uint(256)
                } else {
                    ty
                }
            })
            .collect()
    }

    /// Allocates the buffer a pre-Byzantium call decodes its static aggregate returns from, which
    /// has to be in place before the arguments the output area overlays are encoded.
    ///
    /// Every allocation the encoding and the decoding make lands above the arguments, so it can
    /// land inside the output area as well. A buffer taken before them sits below the arguments
    /// instead, which keeps it disjoint from the area it is copied out of.
    pub(super) fn alloc_overlay_return_buffer(
        &mut self,
        return_tys: &[Ty<'gcx>],
    ) -> Option<(ValueId, ValueId, ValueId)> {
        if self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            return None;
        }
        // buffer = bytes(head_size(static))
        let layout = self.static_aggregate_return_layout(return_tys.iter().copied())?;
        self.alloc_static_return_buffer(&layout, true)
    }

    pub(super) fn plan_return_buffer(
        &mut self,
        input: ValueId,
        zero: ValueId,
        return_tys: &[Ty<'gcx>],
        overlay_buffer: Option<(ValueId, ValueId, ValueId)>,
    ) -> ExternalReturnPlan {
        let returns = return_tys.len();
        let static_return = self.static_aggregate_return_layout(return_tys.iter().copied());
        let decode_returndata = return_tys.iter().any(|&ty| {
            self.types.abi_return_type(ty).is_some_and(|ty| !matches!(ty, AbiType::Word(_)))
        });
        let words = (returns as u64).saturating_mul(32);
        if !self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            // Before Byzantium the output area overlays the input area, as solc's
            // `appendExternalFunctionCall` lays out an ordinary call: a code-bearing callee that
            // returns fewer bytes than it declares cannot be detected without `RETURNDATASIZE`,
            // so the decoding reads the selector and arguments the call left behind rather than
            // whatever untouched memory a buffer of its own would hold.
            // offset = input
            // size = head_size(static) | returns * 32
            let size_bytes = self.pre_byzantium_output_size(return_tys, overlay_buffer.is_some());
            let (offset, size, size_bytes) = match size_bytes {
                Some(size_bytes) => (input, self.builder.imm(size_bytes), size_bytes),
                None => (zero, zero, 0),
            };
            return ExternalReturnPlan {
                static_buffer: overlay_buffer.filter(|_| size_bytes != 0),
                offset,
                size,
                overlays_input: true,
                decode_returndata,
            };
        }
        // static = static_aggregate_layout(returns)
        // buffer = static ? alloc_static_buffer(static) : none
        // decode = any_return_is_nonword
        // offset = static.data ? static.data : (!decode && returns > 1 ? input : zero)
        // size = static.size ? static.size : (decode ? 0 : returns * 32)
        let static_return_buffer = static_return
            .as_ref()
            .and_then(|layout| self.alloc_static_return_buffer(layout, false));
        let (ret_offset, ret_size) = if let Some((_, data, size)) = static_return_buffer {
            (data, size)
        } else if decode_returndata {
            (zero, zero)
        } else {
            let offset = if returns > 1 { input } else { zero };
            (offset, self.builder.imm(words))
        };
        ExternalReturnPlan {
            static_buffer: static_return_buffer,
            offset: ret_offset,
            size: ret_size,
            overlays_input: false,
            decode_returndata,
        }
    }

    /// The size in bytes of the output area a pre-Byzantium call declares, which overlays the
    /// call's input area.
    ///
    /// This is solc's `ReturnInfo::estimatedReturnSize`: the head size of the static return
    /// values, and nothing for a dynamically encoded one, which `finish_external_call` reports as
    /// unsupported before Byzantium.
    fn pre_byzantium_output_size(
        &mut self,
        return_tys: &[Ty<'gcx>],
        has_overlay_buffer: bool,
    ) -> Option<u64> {
        let static_return = self.static_aggregate_return_layout(return_tys.iter().copied());
        match &static_return {
            Some(layout) => has_overlay_buffer.then(|| layout.checked_head_size()).flatten(),
            None => {
                let decode_returndata = return_tys.iter().any(|&ty| {
                    self.types.abi_return_type(ty).is_some_and(|ty| !matches!(ty, AbiType::Word(_)))
                });
                (!decode_returndata).then(|| (return_tys.len() as u64).saturating_mul(32))
            }
        }
    }

    /// Touches the word above a call's output area so that the memory the call needs is already
    /// expanded when its `gas` operand is computed.
    ///
    /// A pre-EIP-150 `CALL` is charged the expansion of its input and output areas out of the gas
    /// left before the forwarded gas is checked against the remainder, and the reserve
    /// [`crate::utils::pre_tangerine_call_gas`] withholds only leaves seven gas for it. solc
    /// touches the word above the area in `appendExternalFunctionCall`, and this is the same
    /// store: the area starts at the free-memory pointer, so writing at that pointer plus the
    /// output size covers every word the call can expand memory to, whatever the arguments
    /// encode to.
    ///
    /// The store has to precede the argument encoding, which is what keeps it from clobbering an
    /// argument the call still has to send. The word it writes is above the output area, so it is
    /// either overwritten by the arguments or free memory the call leaves alone.
    ///
    /// Nothing is emitted where the gas operand is already materialized: an explicit
    /// `{gas: ...}` is the caller's business, and from EIP-150 on the forwarded gas is capped
    /// anyway, so the call cannot be aborted by an overcharge.
    pub(super) fn touch_call_output_area(
        &mut self,
        gas: Option<ValueId>,
        return_tys: &[Ty<'gcx>],
        has_overlay_buffer: bool,
    ) {
        if gas.is_some() || self.cx.gcx.sess.opts.evm_version.can_overcharge_gas_for_call() {
            return;
        }
        let Some(size_bytes) = self.pre_byzantium_output_size(return_tys, has_overlay_buffer)
        else {
            return;
        };
        if size_bytes == 0 {
            return;
        }
        // mstore(add(fmp(), ret_size), 0)
        let area = self.builder.fmp();
        let size = self.builder.imm(size_bytes);
        let above = self.builder.add(area, size);
        let zero = self.builder.imm(U256::ZERO);
        self.builder.mstore(above, zero);
    }

    pub(super) fn finish_external_call(
        &mut self,
        plan: ExternalReturnPlan,
        return_tys: &[Ty<'gcx>],
        span: Span,
        mode: ExternalReturnMode,
        unsupported_returndata: &'static str,
    ) -> Option<Vec<ValueId>> {
        let ExternalReturnPlan { static_buffer, offset, decode_returndata, .. } = plan;
        let returns = return_tys.len();
        if returns == 0 {
            return Some(Vec::new());
        }
        let source = if let Some((object, data, size)) = static_buffer {
            self.revert_if_short_returndata(size);
            if plan.overlays_input {
                // The output area overlays the arguments, so the values move out of it before the
                // decoding allocates over them.
                // mcopy(data, ret_offset, ret_size)
                self.builder.mcopy(data, offset, size);
            }
            Some(object)
        } else if decode_returndata {
            if !self.cx.gcx.sess.opts.evm_version.supports_returndata() {
                return report_error(self.cx.gcx, span, unsupported_returndata);
            }
            Some(self.materialize_returndata_bytes())
        } else {
            None
        };
        if let Some(source) = source {
            return if mode == ExternalReturnMode::All {
                self.lower_abi_decode_values(source, return_tys, span)
            } else {
                self.lower_decoded_return_value(source, return_tys, span).map(|value| vec![value])
            };
        }
        self.validate_static_returndata(offset, return_tys);
        if returns > 1 {
            self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, offset);
        }
        let first = self.load_multi_return_value_as(offset, 0, returns, return_tys[0]);
        if mode == ExternalReturnMode::All && returns > 1 {
            return Some(self.load_multi_return_values(
                first,
                offset,
                returns,
                return_tys.iter().skip(1).copied().map(Some),
            ));
        }
        Some(vec![first])
    }

    fn lower_decoded_return_value(
        &mut self,
        data: ValueId,
        return_types: &[Ty<'gcx>],
        span: Span,
    ) -> Option<ValueId> {
        // values = lower_abi_decode_values(return_data, return_types)
        // multi_return_frame[1..] = values[1..]
        // return values[0]
        let values = self.lower_abi_decode_values(data, return_types, span)?;
        if values.len() > 1 {
            let (object, _, layout) = self.ensure_multi_return_buffer(values.len());
            for (index, value) in values.iter().copied().enumerate().skip(1) {
                let index = self.builder.imm(index as u64);
                self.builder.memory_object_store_element(object, layout, index, value);
            }
        }
        Some(values.into_iter().next().expect("external return list is not empty"))
    }

    /// Allocates the buffer a call decodes its static aggregate return values from.
    ///
    /// `as_bytes` allocates a bytes object, which the decoding reads in place; a single return
    /// value otherwise decodes out of a raw buffer, which the decoding copies into a bytes object
    /// of its own.
    fn alloc_static_return_buffer(
        &mut self,
        layout: &AbiParamLayout,
        as_bytes: bool,
    ) -> Option<(ValueId, ValueId, ValueId)> {
        // size = head_size(layout)
        // buffer = bytes(size) if multiple_returns else raw(size)
        // return (buffer, data, size)
        let size = layout.checked_head_size()?;
        if as_bytes || layout.types.len() != 1 {
            let object_size = self.builder.imm(size.checked_add(EvmMemoryLayout::WORD_SIZE)?);
            let object = self.builder.alloc_object(
                object_size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            let size = self.builder.imm(size);
            self.builder.set_memory_object_len(object, size, MemoryObjectKind::Bytes);
            let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
            return Some((object, data, size));
        }
        let size = self.builder.imm(size);
        let data = self.builder.alloc_raw(size, AllocationSemantics::INTERNAL);
        Some((data, data, size))
    }

    fn revert_if_short_returndata(&mut self, expected: ValueId) {
        // Before Byzantium the returned length is unobservable, so there is nothing to compare
        // against; `revert_if_no_code` guards the code-less callee instead.
        if !self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            return;
        }
        let actual = self.current_returndata_size();
        let short = self.builder.lt(actual, expected);
        self.builder.revert_if(short, RevertReason::TupleDataTooShort);
    }

    pub(super) fn revert_if_no_code(&mut self, address: ValueId) {
        let size = self.builder.extcodesize(address);
        let missing = self.builder.iszero(size);
        self.builder.revert_if(missing, RevertReason::TargetContractHasNoCode);
    }

    fn validate_static_returndata(&mut self, offset: ValueId, returns: &[Ty<'gcx>]) {
        // required = returns * 32
        // if returndatasize < required { revert(0, 0) }
        // for i {
        //     word = load_multi_return_value(offset, i, returns.len)
        //     if !valid(returns[i], word) { revert(0, 0) }
        // }
        let words = u64::try_from(returns.len()).unwrap_or(u64::MAX);
        let size = self.builder.imm(words.saturating_mul(32));
        self.revert_if_short_returndata(size);
        for (index, &ty) in returns.iter().enumerate() {
            let value = self.load_multi_return_value(offset, index, returns.len());
            self.validate_external_return_value(ty, value);
        }
    }

    fn validate_external_return_value(&mut self, ty: Ty<'gcx>, value: ValueId) {
        let validator = match ty.peel_refs().kind {
            TyKind::Enum(id) => {
                Some(AbiWordValidator::EnumRange(self.cx.gcx.hir.enumm(id).variants.len() as u64))
            }
            TyKind::Fn(_) => None,
            _ => AbiWordValidator::from_return_mir_type(types::TypeLowerer::mir_return_type(ty)),
        };
        let Some(validator) = validator else { return };
        let valid = validator.condition(&mut self.builder, value, false);

        let invalid = self.builder.iszero(valid);
        self.builder.revert_if(invalid, RevertReason::Empty);
    }

    pub(super) fn resolve_call_target(
        &self,
        callee: &hir::Expr<'_>,
        function: hir::FunctionId,
    ) -> hir::FunctionId {
        if let ExprKind::Member(base, _) = callee.kind
            && let Some(TyKind::Type(ty)) = self.cx.gcx.type_of_expr(base.id).map(|ty| ty.kind)
        {
            return match ty.kind {
                TyKind::Contract(_) => function,
                TyKind::Super(defining_contract) => self.cx.gcx.resolve_super_function(
                    self.cx.contract_id,
                    defining_contract,
                    function,
                ),
                _ => self.cx.gcx.resolve_virtual_function(self.cx.contract_id, function),
            };
        }
        self.cx.gcx.resolve_virtual_function(self.cx.contract_id, function)
    }
}

fn fixed_bytes_size(ty: Ty<'_>) -> Option<TypeSize> {
    match ty.peel_refs().kind {
        TyKind::Elementary(ElementaryType::FixedBytes(size)) => Some(size),
        _ => None,
    }
}
