//! Call and member-call lowering.

use super::{InternalFunctionPointerShape, Lowerer, checked_arith::PanicCode};
use crate::{
    memory::EvmMemoryLayout,
    mir::{
        Function, FunctionBuilder, FunctionId, MemoryObjectKind, MemoryObjectLayout, MirType,
        ValueId,
    },
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{DataLocation, LitKind, Span};
use solar_data_structures::{bit_set::GrowableBitSet, map::StdEntry};
use solar_interface::{
    Ident,
    diagnostics::{DiagMsg, ErrorGuaranteed},
    kw, sym,
};
use solar_sema::{
    builtins::Builtin,
    eval::erc7201_slot,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{CallableParamSource, Ty, TyFn, TyKind},
};

/// How a value travels across a linked-library delegatecall boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkedFieldKind {
    /// One head word holding the value itself.
    Value,
    /// One head word holding the args-relative offset of a
    /// `[len][elems...]` tail.
    DynArray,
    /// One head word holding the args-relative offset of a
    /// `[len][padded bytes]` tail.
    DynBytes,
}

impl LinkedFieldKind {
    pub(super) fn memory_object_layout(self) -> MemoryObjectLayout {
        match self {
            Self::DynBytes => MemoryObjectLayout::Bytes,
            Self::DynArray => MemoryObjectLayout::WORD_ARRAY,
            Self::Value => unreachable!(),
        }
    }

    fn memory_object_kind(self) -> MemoryObjectKind {
        self.memory_object_layout().kind()
    }

    pub(super) fn data_size(
        self,
        builder: &mut FunctionBuilder<'_>,
        len: ValueId,
        word: ValueId,
    ) -> ValueId {
        match self {
            Self::DynBytes => {
                let thirty_one = builder.imm_u64(31);
                let padded = builder.add(len, thirty_one);
                let mask = builder.imm_u256(U256::MAX - U256::from(31));
                builder.and(padded, mask)
            }
            Self::DynArray => builder.mul(len, word),
            Self::Value => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ExternalCallKind {
    Call,
    StaticCall,
    DelegateCall,
}

impl ExternalCallKind {
    fn from_low_level_builtin(builtin: Builtin) -> Option<Self> {
        match builtin {
            Builtin::AddressCall => Some(Self::Call),
            Builtin::AddressStaticcall => Some(Self::StaticCall),
            Builtin::AddressDelegatecall => Some(Self::DelegateCall),
            _ => None,
        }
    }

    fn from_state_mutability(
        state_mutability: hir::StateMutability,
        has_static_call: bool,
    ) -> Self {
        if has_static_call
            && matches!(state_mutability, hir::StateMutability::Pure | hir::StateMutability::View)
        {
            Self::StaticCall
        } else {
            Self::Call
        }
    }

    pub(super) fn accepts_value(self) -> bool {
        matches!(self, Self::Call)
    }
}

#[derive(Clone, Copy)]
enum BuiltinArgCount {
    Exact(usize),
    AtLeast(usize),
    Between(usize, usize),
}

impl BuiltinArgCount {
    fn description(self) -> String {
        match self {
            Self::Exact(count) => count.to_string(),
            Self::AtLeast(count) => format!("at least {count}"),
            Self::Between(min, max) => format!("{min} to {max}"),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum StorageArrayMethod<'hir> {
    PushDefault,
    Push(&'hir hir::Expr<'hir>),
    Pop,
}

impl<'hir> StorageArrayMethod<'hir> {
    pub(super) fn argument(&self) -> Option<&'hir hir::Expr<'hir>> {
        match self {
            Self::Push(arg) => Some(*arg),
            Self::PushDefault | Self::Pop => None,
        }
    }

    pub(super) fn is_push_default(&self) -> bool {
        matches!(self, Self::PushDefault)
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a function call.
    pub(super) fn lower_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> Option<ValueId> {
        if let Some(builtin) = self.gcx.resolved_builtin(callee) {
            // `T.wrap(x)` / `T.unwrap(v)` for a user-defined value type are identity
            // operations at the EVM level: a UDVT value is represented exactly as its
            // underlying type, so no wrapper is added or removed.
            if matches!(builtin, Builtin::UdvtWrap | Builtin::UdvtUnwrap) {
                return Some(match self.builtin_args(builtin, args) {
                    Ok([arg]) => self.lower_value_expr(builder, arg),
                    Err(guar) => builder.error_value(guar),
                });
            }

            if Self::builtin_uses_direct_call_lowering(builtin) {
                return self.lower_builtin_call(builder, builtin, args);
            }
        }

        if let Some(error_id) = self.custom_error_id_from_callee(callee) {
            self.emit_custom_error_revert(builder, error_id, args);
            return None;
        }

        if let Some(TyKind::Fn(function)) = self.get_expr_type(callee).map(|ty| ty.kind)
            && function.is_internal()
            && function.function_id.is_none()
            && self.gcx.resolved_function(callee).is_none()
        {
            return self.lower_internal_function_pointer_call(builder, callee, args, function);
        }

        if let Some(TyKind::Fn(function)) = self.get_expr_type(callee).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
            && self.gcx.resolved_function(callee).is_none()
        {
            return self
                .lower_external_function_pointer_call(builder, callee, args, call_opts, function);
        }

        if let ExprKind::Member(base, member) = &callee.kind {
            return self
                .lower_member_call_with_opts(builder, callee, base, *member, args, call_opts);
        }

        // Handle `new Contract(args)` - contract creation
        if let ExprKind::New(ty) = &callee.kind {
            if self.is_memory_array_new_type(ty) {
                return Some(self.lower_new_array(builder, ty, args));
            }
            return Some(self.lower_new_contract(builder, ty, args, call_opts));
        }

        // Handle internal function calls: func(args) where func is a function in the same contract
        if let Some(hir::Res::Item(item_id)) = self.gcx.resolved_expr(callee) {
            match item_id {
                hir::ItemId::Function(func_id) => {
                    return self.lower_internal_call(builder, func_id, args);
                }
                hir::ItemId::Contract(_) => {
                    let [arg] = match self.positional_args(args, "contract conversion") {
                        Ok(args) => args,
                        Err(guar) => return self.call_error_result(builder, callee, guar),
                    };
                    return Some(self.lower_value_expr(builder, arg));
                }
                hir::ItemId::Enum(enum_id) => {
                    let [arg] = match self.positional_args(args, "enum conversion") {
                        Ok(args) => args,
                        Err(guar) => return self.call_error_result(builder, callee, guar),
                    };
                    let value = self.lower_value_expr(builder, arg);
                    let variant_count = self.gcx.hir.enumm(enum_id).variants.len();
                    self.emit_enum_range_check(builder, value, variant_count);
                    return Some(value);
                }
                hir::ItemId::Struct(struct_id) => {
                    return Some(self.lower_struct_constructor(builder, struct_id, args));
                }
                _ => {}
            }
        }

        // Handle Type(expr) where callee is an explicit Type expression
        // e.g., uint256(x), address(y), bytes32(z)
        if let ExprKind::Type(ty) = &callee.kind {
            let [arg] = match self.positional_args(args, "type conversion") {
                Ok(args) => args,
                Err(guar) => return self.call_error_result(builder, callee, guar),
            };
            let value = self.lower_value_expr(builder, arg);
            return Some(self.lower_type_conversion(builder, ty, arg, value));
        }

        self.err_call_result(
            builder,
            callee,
            callee.span,
            "codegen does not support this call expression yet",
        )
    }

    fn err_call_result(
        &self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        span: Span,
        msg: impl Into<DiagMsg>,
    ) -> Option<ValueId> {
        let guar = self.gcx.dcx().err(msg).span(span).emit();
        self.call_error_result(builder, callee, guar)
    }

    fn call_error_result(
        &self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        guar: ErrorGuaranteed,
    ) -> Option<ValueId> {
        let returns_value = self
            .get_expr_type(callee)
            .and_then(|ty| ty.returns())
            .is_none_or(|returns| !returns.is_empty());
        returns_value.then(|| builder.error_value(guar))
    }

    fn emit_wrong_builtin_arg_count(
        &self,
        builtin: Builtin,
        span: Span,
        expected: BuiltinArgCount,
        actual: usize,
    ) -> ErrorGuaranteed {
        let expected = expected.description();
        let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
        self.gcx
            .dcx()
            .err(format!(
                "wrong number of arguments for {kind} `{}`: expected {expected}, found {}",
                builtin.name(),
                actual
            ))
            .span(span)
            .emit()
    }

    fn builtin_arg_exprs<'hir>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<&'hir [hir::Expr<'hir>], ErrorGuaranteed> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => Ok(exprs),
            hir::CallArgsKind::Named(_) => {
                let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
                Err(self
                    .gcx
                    .dcx()
                    .err(format!(
                        "named arguments are not supported for {kind} `{}` in codegen",
                        builtin.name()
                    ))
                    .span(args.span)
                    .emit())
            }
        }
    }

    fn positional_args<'hir, const N: usize>(
        &self,
        args: &CallArgs<'hir>,
        context: &str,
    ) -> Result<&'hir [hir::Expr<'hir>; N], ErrorGuaranteed> {
        let hir::CallArgsKind::Unnamed(exprs) = args.kind else {
            return Err(self
                .gcx
                .dcx()
                .err(format!("named arguments are not supported for {context} in codegen"))
                .span(args.span)
                .emit());
        };
        exprs.try_into().map_err(|_| {
            self.gcx
                .dcx()
                .err(format!(
                    "wrong number of arguments for {context} in codegen: expected {N}, found {}",
                    exprs.len()
                ))
                .span(args.span)
                .emit()
        })
    }

    pub(super) fn ordered_args_for<'hir>(
        &self,
        args: &CallArgs<'hir>,
        source: Option<CallableParamSource>,
    ) -> Result<Vec<&'hir hir::Expr<'hir>>, ErrorGuaranteed> {
        let parameter_names = match args.kind {
            hir::CallArgsKind::Unnamed(_) => None,
            hir::CallArgsKind::Named(_) => {
                let Some(source) = source else {
                    return Err(self
                        .gcx
                        .dcx()
                        .err("codegen cannot resolve this call's named parameters")
                        .span(args.span)
                        .emit());
                };
                Some(self.gcx.callable_param_names(source))
            }
        };
        (0..args.len())
            .map(|index| args.argument_for_parameter(index, parameter_names.as_deref()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                self.gcx
                    .dcx()
                    .err("codegen cannot order this call's arguments")
                    .span(args.span)
                    .emit()
            })
    }

    pub(super) fn ordered_call_args<'hir>(
        &self,
        callee: &hir::Expr<'hir>,
        args: &CallArgs<'hir>,
    ) -> Result<Vec<&'hir hir::Expr<'hir>>, ErrorGuaranteed> {
        self.ordered_args_for(args, self.gcx.call_param_source(callee))
    }

    fn ordered_function_args<'hir>(
        &self,
        func_id: hir::FunctionId,
        args: &CallArgs<'hir>,
        skips_receiver: bool,
    ) -> Result<Vec<&'hir hir::Expr<'hir>>, ErrorGuaranteed> {
        self.ordered_args_for(
            args,
            Some(CallableParamSource::Function { id: func_id, skips_receiver }),
        )
    }

    pub(super) fn builtin_args<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<&'hir [hir::Expr<'hir>; N], ErrorGuaranteed> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        exprs.try_into().map_err(|_| {
            self.emit_wrong_builtin_arg_count(
                builtin,
                args.span,
                BuiltinArgCount::Exact(N),
                exprs.len(),
            )
        })
    }

    pub(super) fn storage_array_method<'hir>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<StorageArrayMethod<'hir>, ErrorGuaranteed> {
        match builtin {
            Builtin::ArrayPush0 => {
                let [] = self.builtin_args(builtin, args)?;
                Ok(StorageArrayMethod::PushDefault)
            }
            Builtin::ArrayPush => {
                let [arg] = self.builtin_args(builtin, args)?;
                Ok(StorageArrayMethod::Push(arg))
            }
            Builtin::ArrayPop => {
                let [] = self.builtin_args(builtin, args)?;
                Ok(StorageArrayMethod::Pop)
            }
            _ => Err(self.recovery_error(
                Some(args.span),
                "codegen routed a non-array builtin through array method lowering",
            )),
        }
    }

    pub(super) fn builtin_args_with_rest<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<(&'hir [hir::Expr<'hir>; N], &'hir [hir::Expr<'hir>]), ErrorGuaranteed> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        let Some((prefix, rest)) = exprs.split_first_chunk() else {
            return Err(self.emit_wrong_builtin_arg_count(
                builtin,
                args.span,
                BuiltinArgCount::AtLeast(N),
                exprs.len(),
            ));
        };
        Ok((prefix, rest))
    }

    pub(super) fn variadic_builtin_args<'hir>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<&'hir [hir::Expr<'hir>], ErrorGuaranteed> {
        self.builtin_arg_exprs(builtin, args)
    }

    fn builtin_args_with_optional<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &CallArgs<'hir>,
    ) -> Result<(&'hir [hir::Expr<'hir>; N], Option<&'hir hir::Expr<'hir>>), ErrorGuaranteed> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if let Some((required, optional)) = exprs.split_first_chunk()
            && optional.len() <= 1
        {
            return Ok((required, optional.first()));
        }
        Err(self.emit_wrong_builtin_arg_count(
            builtin,
            args.span,
            BuiltinArgCount::Between(N, N + 1),
            exprs.len(),
        ))
    }

    fn lower_builtin_args<const N: usize>(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Result<[ValueId; N], ErrorGuaranteed> {
        let exprs = self.builtin_args(builtin, args)?;
        Ok(exprs.each_ref().map(|arg| self.lower_value_expr(builder, arg)))
    }

    fn lower_internal_function_pointer_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        function: &'gcx TyFn<'gcx>,
    ) -> Option<ValueId> {
        let function_value = self.lower_value_expr(builder, callee);
        let arg_exprs = match self.ordered_call_args(callee, args) {
            Ok(exprs) => exprs,
            Err(guar) => return self.call_error_result(builder, callee, guar),
        };
        let mut arg_values = arg_exprs
            .into_iter()
            .enumerate()
            .map(|(index, arg)| {
                let parameter = function.parameters.get(index).copied();
                if parameter.is_some_and(|ty| {
                    matches!(ty.kind, TyKind::Mapping(..) | TyKind::Ref(_, DataLocation::Storage))
                }) && let Some(slot) = self.lower_lvalue_slot(builder, arg)
                {
                    slot
                } else {
                    let value = self.lower_value_expr(builder, arg);
                    self.coerce_memory_slice_value(builder, value)
                }
            })
            .collect::<Vec<_>>();
        arg_values.insert(0, function_value);

        let dispatcher = self.ensure_internal_function_pointer_dispatcher(function);
        let returns = function.returns.len();
        let Some(&return_ty) = function.returns.first() else {
            builder.internal_call_void(dispatcher, arg_values, returns);
            return None;
        };
        Some(builder.internal_call(
            dispatcher,
            arg_values,
            self.lower_type_from_ty(return_ty),
            returns,
        ))
    }

    fn ensure_internal_function_pointer_dispatcher(
        &mut self,
        function: &'gcx TyFn<'gcx>,
    ) -> FunctionId {
        let shape = self.internal_function_pointer_shape(function);
        let index = self.internal_function_pointer_dispatchers.len();
        match self.internal_function_pointer_dispatchers.entry(shape) {
            StdEntry::Occupied(entry) => *entry.get(),
            StdEntry::Vacant(entry) => {
                let name = Ident::from_str(&format!("__internal_dispatch_{index}"));
                let dispatcher = self.module.add_function(Function::new(name));
                entry.insert(dispatcher);
                dispatcher
            }
        }
    }

    /// Lowers address-taken targets to a fixed point, then fills the reserved dispatchers.
    pub(super) fn generate_internal_function_pointer_dispatchers(&mut self) {
        if self.internal_function_pointer_dispatchers.is_empty() {
            return;
        }
        let mut lowered_targets = GrowableBitSet::new_empty();
        while let Some(function_id) = self
            .internal_function_pointer_targets
            .iter()
            .find(|&function_id| !lowered_targets.contains(function_id))
        {
            lowered_targets.insert(function_id);
            let function = self.gcx.hir.function(function_id);
            if function.kind != hir::FunctionKind::Function || function.body.is_none() {
                self.internal_function_pointer_targets.remove(function_id);
                continue;
            }
            if matches!(function.visibility, hir::Visibility::Internal | hir::Visibility::Private) {
                self.ensure_function_lowered(function_id);
            } else {
                self.ensure_internal_mir_function(function_id);
            }
        }

        let dispatchers = self
            .internal_function_pointer_dispatchers
            .iter()
            .map(|(shape, &function_id)| (shape.clone(), function_id))
            .collect::<Vec<_>>();
        for (shape, dispatcher) in dispatchers {
            self.generate_internal_function_pointer_dispatcher(shape, dispatcher);
        }
    }

    fn generate_internal_function_pointer_dispatcher(
        &mut self,
        shape: InternalFunctionPointerShape,
        dispatcher: FunctionId,
    ) {
        let candidates = self
            .internal_function_pointer_targets
            .iter()
            .filter(|&function_id| {
                let TyKind::Fn(candidate_ty) = self.gcx.type_of_item(function_id.into()).kind
                else {
                    return false;
                };
                self.internal_function_pointer_shape(candidate_ty) == shape
            })
            .collect::<Vec<_>>();

        let reserved = self.module.function(dispatcher);
        let name = Ident::new(reserved.name.symbol, reserved.name_span);
        let mut dispatcher_function = Function::new(name);
        dispatcher_function.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut dispatcher_function);
            let function_value = builder.add_param(MirType::Function);
            let arg_values = shape.0.iter().map(|&ty| builder.add_param(ty)).collect::<Vec<_>>();
            for &ty in &shape.1 {
                builder.add_return(ty);
            }

            for function_id in candidates {
                let case_block = builder.create_block();
                let next_block = builder.create_block();
                let id = builder.imm_u64(Self::internal_function_pointer_id(function_id));
                let is_match = builder.eq(function_value, id);
                builder.branch(is_match, case_block, next_block);

                builder.switch_to_block(case_block);
                if shape.1.is_empty() {
                    self.emit_internal_void_call(&mut builder, function_id, arg_values.clone());
                    builder.ret([]);
                } else {
                    let result =
                        self.emit_internal_call(&mut builder, function_id, arg_values.clone());
                    let mut return_values = Vec::with_capacity(shape.1.len());
                    return_values.push(result);
                    if shape.1.len() > 1 {
                        let base = self.multi_return_buffer_base(&mut builder);
                        for index in 1..shape.1.len() {
                            return_values.push(self.load_multi_return_value(
                                &mut builder,
                                base,
                                index,
                            ));
                        }
                    }
                    builder.ret(return_values);
                }
                builder.switch_to_block(next_block);
            }
            self.emit_panic_revert(&mut builder, PanicCode::InvalidInternalFunction);
        }
        dispatcher_function.name = self.module.function(dispatcher).name;
        *self.module.function_mut(dispatcher) = dispatcher_function;
    }

    fn internal_function_pointer_shape(
        &self,
        function: &TyFn<'gcx>,
    ) -> InternalFunctionPointerShape {
        (
            function.parameters.iter().map(|&ty| self.lower_type_from_ty(ty)).collect(),
            function.returns.iter().map(|&ty| self.lower_type_from_ty(ty)).collect(),
        )
    }

    fn lower_external_function_pointer_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
        function: &'gcx TyFn<'gcx>,
    ) -> Option<ValueId> {
        let (success, ret_offset) =
            self.emit_external_function_pointer_call(builder, callee, args, call_opts, function);
        self.emit_forwarding_revert_unless(builder, success);
        (!function.returns.is_empty()).then(|| builder.mload(ret_offset))
    }

    pub(super) fn emit_external_function_pointer_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
        function: &'gcx TyFn<'gcx>,
    ) -> (ValueId, ValueId) {
        let function_value = self.lower_value_expr(builder, callee);
        let selector_mask = builder.imm_u64(u32::MAX as u64);
        let selector = builder.and(function_value, selector_mask);
        let selector_shift = builder.imm_u64(224);
        let selector_word = builder.shl(selector_shift, selector);
        let address_shift = builder.imm_u64(32);
        let address = builder.shr(address_shift, function_value);

        let arg_exprs = match self.ordered_call_args(callee, args) {
            Ok(exprs) => exprs,
            Err(guar) => {
                let error = builder.error_value(guar);
                return (error, error);
            }
        };
        let (calldata_start, calldata_size) = match self.abi_encode_call_payload(
            builder,
            Some(selector_word),
            arg_exprs.iter().copied(),
        ) {
            Ok(payload) => payload,
            Err(guar) => {
                let error = builder.error_value(guar);
                return (error, error);
            }
        };

        let ret_offset =
            if function.returns.len() > 1 { calldata_start } else { builder.imm_u64(0) };
        let ret_size = builder.imm_u64((function.returns.len() * 32) as u64);
        let kind = ExternalCallKind::from_state_mutability(
            function.state_mutability,
            self.gcx.sess.opts.evm_version.has_static_call(),
        );
        let (gas, value) =
            self.lower_external_call_options(builder, call_opts, kind.accepts_value());
        let success = self.emit_external_call(
            builder,
            kind,
            gas,
            address,
            value,
            calldata_start,
            calldata_size,
            ret_offset,
            ret_size,
        );
        if function.returns.len() > 1 {
            let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            builder.mstore(ptr_slot, ret_offset);
        }
        (success, ret_offset)
    }

    pub(super) fn virtual_function_target(&self, function_id: hir::FunctionId) -> hir::FunctionId {
        let Some(contract_id) = self.contract_id else { return function_id };
        self.gcx.resolve_virtual_function(contract_id, function_id)
    }

    fn builtin_uses_direct_call_lowering(builtin: Builtin) -> bool {
        !matches!(
            builtin,
            Builtin::AddressCall
                | Builtin::AddressDelegatecall
                | Builtin::AddressStaticcall
                | Builtin::AddressPayableTransfer
                | Builtin::AddressPayableSend
                | Builtin::ArrayLength
                | Builtin::ArrayPush0
                | Builtin::ArrayPush
                | Builtin::ArrayPop
                | Builtin::UdvtWrap
                | Builtin::UdvtUnwrap
        )
    }

    fn custom_error_id_from_callee(&self, callee: &hir::Expr<'_>) -> Option<hir::ErrorId> {
        if let Some(hir::Res::Item(hir::ItemId::Error(error_id))) = self.gcx.resolved_expr(callee) {
            return Some(error_id);
        }

        if let Some(ty) = self.get_expr_type(callee)
            && let TyKind::Error(_, error_id) = ty.kind
        {
            return Some(error_id);
        }

        None
    }

    fn emit_revert_payload_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        if self.emit_custom_error_revert_from_expr(builder, expr) {
            return true;
        }
        self.emit_revert_error_string_from_expr(builder, expr)
    }

    fn emit_custom_error_revert_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        let ExprKind::Call(callee, args, _) = &expr.kind else { return false };
        let Some(error_id) = self.custom_error_id_from_callee(callee) else {
            return false;
        };
        self.emit_custom_error_revert(builder, error_id, args);
        true
    }

    fn emit_custom_error_revert(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        error_id: hir::ErrorId,
        args: &CallArgs<'_>,
    ) {
        let param_tys = self.gcx.item_parameter_types(hir::ItemId::Error(error_id));
        let arg_exprs =
            match self.ordered_args_for(args, Some(CallableParamSource::Error(error_id))) {
                Ok(exprs) => exprs,
                Err(_) => return,
            };
        let mut items = Vec::with_capacity(param_tys.len());
        for (&ty, arg) in param_tys.iter().zip(arg_exprs) {
            let value = self.lower_return_value_for_ty(builder, arg, ty);
            items.push((value, ty));
        }

        let selector = self.custom_error_selector(error_id);
        self.emit_abi_error_revert(builder, selector, &items);
    }

    fn custom_error_selector(&self, error_id: hir::ErrorId) -> [u8; 4] {
        let signature = self.gcx.item_signature(hir::ItemId::Error(error_id));
        let hash = keccak256(signature.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    /// Lowers a `new T[](len)` memory array expression.
    fn lower_new_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        if !self.is_memory_array_new_type(ty) {
            return self.err_value(
                builder,
                ty.span,
                "codegen expected a dynamic memory array type",
            );
        }

        let [len] = match self.positional_args(args, "dynamic memory array allocation") {
            Ok(args) => args,
            Err(guar) => return builder.error_value(guar),
        };
        let len = self.lower_value_expr(builder, len);

        let word_size = builder.imm_u64(EvmMemoryLayout::DYNAMIC_HEADER_SIZE);
        let data_size = if matches!(
            &ty.kind,
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            // `bytes`/`string`: the length counts bytes; the data area is the
            // length padded up to a word.
            let thirty_one = builder.imm_u64(31);
            let rounded = builder.add(len, thirty_one);
            let rounded_overflow = builder.lt(rounded, len);
            self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
            let mask = builder.not(thirty_one);
            builder.and(rounded, mask)
        } else {
            // Arrays: one word per element.
            let data_size = builder.mul(len, word_size);
            let checked_len = builder.div(data_size, word_size);
            let overflow = builder.eq(checked_len, len);
            self.emit_panic_if_zero(builder, overflow, PanicCode::MemoryAllocationOverflow);
            data_size
        };
        let total_size = builder.add(data_size, word_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);
        let object_layout = if matches!(
            &ty.kind,
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            crate::mir::MemoryObjectLayout::Bytes
        } else {
            crate::mir::MemoryObjectLayout::DynamicArray { element_words: 1 }
        };
        let ptr = builder.alloc_object(
            total_size,
            object_layout,
            crate::mir::AllocationSemantics::SOLIDITY_ZEROED,
        );
        builder.set_memory_object_len(ptr, len, object_layout.kind());

        ptr
    }

    fn is_memory_array_new_type(&self, ty: &hir::Type<'_>) -> bool {
        match &ty.kind {
            hir::TypeKind::Array(array) => array.size.is_none(),
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => true,
            _ => false,
        }
    }

    /// Lowers a `new Contract(args)` expression.
    /// Supports call options like `new Contract{salt: s, value: v}(args)`.
    fn lower_new_contract(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        // Extract ContractId from the type
        let contract_id = match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Contract(id)) => *id,
            _ => {
                return self.err_value(
                    builder,
                    ty.span,
                    "codegen expected a contract type for `new` expression",
                );
            }
        };
        let arg_exprs = match self.ordered_args_for(
            args,
            self.gcx
                .hir
                .contract(contract_id)
                .ctor
                .map(|id| CallableParamSource::Function { id, skips_receiver: false }),
        ) {
            Ok(exprs) => exprs,
            Err(guar) => return builder.error_value(guar),
        };

        // Look up pre-compiled bytecode
        let bytecode = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => {
                let guar = self
                    .gcx
                    .dcx()
                    .err(format!(
                        "codegen is missing creation bytecode for `new {}`",
                        self.gcx.hir.contract(contract_id).name
                    ))
                    .span(ty.span)
                    .note("the deployed contract did not compile or was not lowered first")
                    .emit();
                return builder.error_value(guar);
            }
        };

        let bytecode_len = bytecode.len();

        // Extract call options (salt, value)
        let mut salt_opt: Option<ValueId> = None;
        let mut value_opt: Option<ValueId> = None;

        if let Some(opts) = call_opts {
            for opt in opts {
                match opt.name.name {
                    sym::salt => {
                        salt_opt = Some(self.lower_value_expr(builder, &opt.value));
                    }
                    sym::value => {
                        value_opt = Some(self.lower_value_expr(builder, &opt.value));
                    }
                    _ => {
                        // gas option is not supported for contract creation
                    }
                }
            }
        }

        // Allocate memory for bytecode + constructor args from free memory pointer
        let mem_offset = builder.fmp();

        // Copy bytecode to memory using MSTORE
        // For each 32-byte chunk of bytecode, emit an MSTORE at (mem_offset + offset)
        for (i, chunk) in bytecode.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = U256::from_be_bytes(padded);
            let val_id = builder.imm_u256(value);
            let chunk_offset = builder.imm_u64((i as u64) * 32);
            let dest = builder.add(mem_offset, chunk_offset);
            builder.mstore(dest, val_id);
        }

        // Append constructor arguments after bytecode
        let mut args_offset = bytecode_len as u64;
        for arg in arg_exprs {
            let arg_val = self.lower_value_expr(builder, arg);
            let arg_offset_imm = builder.imm_u64(args_offset);
            let arg_dest = builder.add(mem_offset, arg_offset_imm);
            builder.mstore(arg_dest, arg_val);
            args_offset += 32; // Each arg is 32 bytes ABI encoded
        }

        // Total size = bytecode + args
        let total_size = builder.imm_u64(args_offset);

        // Update free memory pointer: new_free = mem_offset + ((total_size + 31) & ~31)
        let thirty_one = builder.imm_u64(31);
        let aligned_size = builder.add(total_size, thirty_one);
        let mask = builder.imm_u256(U256::from(!31u64));
        let aligned_size = builder.and(aligned_size, mask);
        let new_free = builder.add(mem_offset, aligned_size);
        builder.set_fmp(new_free);

        // Value to send with CREATE/CREATE2 (0 for non-payable, or from value option)
        let value = value_opt.unwrap_or_else(|| builder.imm_u64(0));

        let created = if let Some(salt) = salt_opt {
            builder.create2(value, mem_offset, total_size, salt)
        } else {
            builder.create(value, mem_offset, total_size)
        };
        self.emit_forwarding_revert_unless(builder, created);
        created
    }

    /// Lowers a builtin function call.
    fn lower_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Option<ValueId> {
        if builtin.is_yul() {
            let Some(returns) = builtin.ty(self.gcx).returns() else {
                let guar = self
                    .recovery_error(None, "codegen expected Yul builtin to have a function type");
                return Some(builder.error_value(guar));
            };
            if returns.is_empty() {
                let _ = self.lower_yul_unit_builtin_call(builder, builtin, args);
                return None;
            }
            debug_assert_eq!(returns.len(), 1);
            return Some(match self.lower_yul_value_builtin_call(builder, builtin, args) {
                Ok(value) => value,
                Err(guar) => builder.error_value(guar),
            });
        }

        match builtin {
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => {
                let _ = self.lower_unit_builtin_call(builder, builtin, args);
                None
            }
            _ => Some(match self.lower_builtin_value_call(builder, builtin, args) {
                Ok(value) => value,
                Err(guar) => builder.error_value(guar),
            }),
        }
    }

    fn lower_unit_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Result<(), ErrorGuaranteed> {
        match builtin {
            Builtin::Selfdestruct => {
                let [addr] = self.builtin_args(builtin, args)?;
                let addr = self.lower_value_expr(builder, addr);
                builder.selfdestruct(addr);
            }
            Builtin::Require | Builtin::Assert => {
                let (cond, message) = if builtin == Builtin::Require {
                    let ([cond], message) = self.builtin_args_with_optional(builtin, args)?;
                    (cond, message)
                } else {
                    let [cond] = self.builtin_args(builtin, args)?;
                    (cond, None)
                };
                let cond = self.lower_value_expr(builder, cond);
                let is_false = builder.iszero(cond);
                let revert_block = builder.create_block();
                let continue_block = builder.create_block();
                builder.branch(is_false, revert_block, continue_block);

                builder.switch_to_block(revert_block);
                if matches!(builtin, Builtin::Assert) {
                    self.emit_panic_revert(builder, PanicCode::Assert);
                } else if let Some(message) = message {
                    if !self.emit_revert_payload_from_expr(builder, message) {
                        let zero = builder.imm_u64(0);
                        builder.revert(zero, zero);
                    }
                } else {
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);
                }

                builder.switch_to_block(continue_block);
            }
            Builtin::Revert => {
                let [] = self.builtin_args(builtin, args)?;
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
            }
            Builtin::RevertMsg => {
                let [message] = self.builtin_args(builtin, args)?;
                let emitted = self.emit_revert_error_string_from_expr(builder, message);
                if !emitted {
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);
                }
            }
            _ => {
                return Err(self.recovery_error(
                    Some(args.span),
                    "codegen routed a value builtin through unit lowering",
                ));
            }
        }
        Ok(())
    }

    fn lower_builtin_value_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Result<ValueId, ErrorGuaranteed> {
        match builtin {
            Builtin::Keccak256 => {
                let [first] = self.builtin_args(builtin, args)?;
                if let Some(packed_args) = self.abi_encode_packed_call_args(first) {
                    let packed_args =
                        self.variadic_builtin_args(Builtin::AbiEncodePacked, packed_args)?;
                    return self.lower_keccak_abi_encode_packed(builder, packed_args);
                }
                if let Some(encode_args) = self.abi_encode_call_args(first) {
                    let arg_exprs = self.variadic_builtin_args(Builtin::AbiEncode, encode_args)?;
                    return self.lower_keccak_abi_encode(builder, arg_exprs);
                }

                // Dynamic `bytes`/`string` (incl. `bytes(s)` of a calldata
                // value): hash the raw data after materializing it to memory.
                if let Some(hash) = self.keccak_dynamic_bytes(builder, first) {
                    return Ok(hash);
                }
                let arg_val = self.lower_value_expr(builder, first);
                let ptr = builder.imm_u64(0);
                builder.mstore(ptr, arg_val);
                let size = builder.imm_u64(32);
                Ok(builder.keccak256(ptr, size))
            }
            Builtin::Erc7201 => {
                let [arg] = self.builtin_args(builtin, args)?;
                self.lower_erc7201_call(builder, arg)
            }
            Builtin::Gasleft => {
                let [] = self.builtin_args(builtin, args)?;
                Ok(builder.gas())
            }
            Builtin::Blockhash | Builtin::Blobhash => {
                let [value] = self.lower_builtin_args(builder, builtin, args)?;
                Ok(if builtin == Builtin::Blockhash {
                    builder.blockhash(value)
                } else {
                    builder.blobhash(value)
                })
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => Err(self.recovery_error(
                Some(args.span),
                "codegen routed a unit builtin through value lowering",
            )),
            Builtin::AddressBalance => {
                let [addr] = self.builtin_args(builtin, args)?;
                let addr = self.lower_value_expr(builder, addr);
                Ok(builder.balance(addr))
            }
            Builtin::AddMod | Builtin::MulMod => {
                let [a, b, modulus] = self.builtin_args(builtin, args)?;
                let a = self.lower_value_expr(builder, a);
                let b = self.lower_value_expr(builder, b);
                let modulus = self.lower_value_expr(builder, modulus);
                self.emit_panic_if_zero(builder, modulus, PanicCode::DivisionByZero);
                let value = if matches!(builtin, Builtin::AddMod) {
                    builder.addmod(a, b, modulus)
                } else {
                    builder.mulmod(a, b, modulus)
                };
                Ok(value)
            }
            Builtin::StringConcat | Builtin::BytesConcat => {
                let exprs = self.variadic_builtin_args(builtin, args)?;
                self.lower_abi_encode_packed(builder, exprs)
            }
            Builtin::Sha256 | Builtin::Ripemd160 => {
                self.lower_hash_precompile_call(builder, builtin, args)
            }
            Builtin::EcRecover => self.lower_ecrecover_call(builder, args),
            Builtin::AbiEncode => {
                // abi.encode: a fresh `bytes memory` allocation holding the
                // padded ABI tuple encoding of the arguments.
                let arg_exprs = self.variadic_builtin_args(builtin, args)?;
                self.lower_abi_encode_to_bytes(builder, arg_exprs)
            }
            Builtin::AbiEncodePacked => {
                // `abi.encodePacked`: pack values tightly based on their types.
                // Returns `bytes memory` (length + data).
                let exprs = self.variadic_builtin_args(builtin, args)?;
                self.lower_abi_encode_packed(builder, exprs)
            }
            Builtin::AbiEncodeWithSelector => {
                // A selector-prefixed payload adapted to a `bytes memory`
                // value: `[length][selector + ABI tuple encoding]`.
                let ([selector], exprs) = self.builtin_args_with_rest(builtin, args)?;
                let selector = self.lower_selector_word(builder, selector);
                let (data, len) =
                    self.abi_encode_call_payload(builder, Some(selector), exprs.iter())?;
                let slice = builder.make_slice(data, len, crate::mir::SliceLocation::Memory);
                Ok(self.materialize_memory_slice_bytes(builder, slice))
            }
            Builtin::AbiEncodeWithSignature => {
                let ([signature], exprs) = self.builtin_args_with_rest(builtin, args)?;
                let selector = self.lower_signature_selector(builder, signature);
                let (data, len) =
                    self.abi_encode_call_payload(builder, Some(selector), exprs.iter())?;
                let slice = builder.make_slice(data, len, crate::mir::SliceLocation::Memory);
                Ok(self.materialize_memory_slice_bytes(builder, slice))
            }
            Builtin::AbiEncodeCall => {
                // `abi.encodeCall(F, (args))` as a `bytes memory` value.
                let (data, len) = self.abi_encode_call_from_args(builder, args)?;
                let slice = builder.make_slice(data, len, crate::mir::SliceLocation::Memory);
                Ok(self.materialize_memory_slice_bytes(builder, slice))
            }
            Builtin::AbiDecode => {
                let [data, types] = self.builtin_args(builtin, args)?;
                self.lower_abi_decode(builder, data, types, args.span)
            }
            builtin if builtin.is_yul() => Err(self.recovery_error(
                Some(args.span),
                "codegen routed a Yul builtin through Solidity lowering",
            )),
            _ => Err(self
                .gcx
                .dcx()
                .err(format!("unsupported builtin call `{}`", builtin.name()))
                .span(args.span)
                .emit()),
        }
    }

    fn lower_erc7201_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        first: &hir::Expr<'_>,
    ) -> Result<ValueId, ErrorGuaranteed> {
        if let ExprKind::Lit(lit) = &first.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            return Ok(builder.imm_u256(erc7201_slot(bytes.as_byte_str()).into()));
        }

        let Some(inner_hash) = self.keccak_dynamic_bytes(builder, first) else {
            return Err(self
                .gcx
                .dcx()
                .err("codegen expected a string or bytes value for ERC-7201")
                .span(first.span)
                .emit());
        };
        let one = builder.imm_u64(1);
        let inner_hash_minus_one = builder.sub(inner_hash, one);
        let ptr = builder.imm_u64(0);
        builder.mstore(ptr, inner_hash_minus_one);
        let size = builder.imm_u64(32);
        let outer_hash = builder.keccak256(ptr, size);
        let mask = builder.imm_u256(!U256::from(0xff));
        Ok(builder.and(outer_hash, mask))
    }

    fn lower_yul_unit_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        call_args: &CallArgs<'_>,
    ) -> Result<(), ErrorGuaranteed> {
        macro_rules! lower {
            ($method:ident($($arg:ident),* $(,)?)) => {{
                let [$($arg),*] = self.lower_builtin_args(builder, builtin, call_args)?;
                builder.$method($($arg),*)
            }};
        }
        match builtin {
            Builtin::YulMstore => lower!(mstore(offset, value)),
            Builtin::YulMstore8 => lower!(mstore8(offset, value)),
            Builtin::YulMcopy => {
                // The Yul `mcopy` builtin is only available on Cancun-compatible
                // VMs; solc rejects it on older targets. Compiler-generated
                // copies stay as semantic MIR and the required `lower-mcopy`
                // pass uses the identity precompile on older targets, but an
                // explicit assembly `mcopy` keeps the diagnostic.
                if self.gcx.sess.opts.evm_version.has_mcopy() {
                    let [dst, src, size] = self.lower_builtin_args(builder, builtin, call_args)?;
                    builder.mcopy(dst, src, size);
                } else {
                    return Err(self
                        .gcx
                        .dcx()
                        .err("codegen requires Cancun-compatible EVM for memory copy")
                        .span(call_args.span)
                        .help("compile with `--evm-version cancun` or newer")
                        .emit());
                }
            }
            Builtin::YulSstore => lower!(sstore(slot, value)),
            Builtin::YulTstore => lower!(tstore(slot, value)),
            Builtin::YulCalldatacopy => lower!(calldatacopy(dst, src, size)),
            Builtin::YulCodecopy => lower!(codecopy(dst, src, size)),
            Builtin::YulExtcodecopy => lower!(extcodecopy(address, dst, src, size)),
            Builtin::YulReturndatacopy => lower!(returndatacopy(dst, src, size)),
            Builtin::YulLog0 => lower!(log0(offset, size)),
            Builtin::YulLog1 => lower!(log1(offset, size, topic1)),
            Builtin::YulLog2 => lower!(log2(offset, size, topic1, topic2)),
            Builtin::YulLog3 => lower!(log3(offset, size, topic1, topic2, topic3)),
            Builtin::YulLog4 => lower!(log4(offset, size, topic1, topic2, topic3, topic4)),
            Builtin::YulRevert => lower!(revert(offset, size)),
            Builtin::YulReturn => {
                // `return(offset, size)` halts and returns `size` bytes of memory.
                lower!(ret_data(offset, size));
            }
            Builtin::YulStop => lower!(stop()),
            Builtin::YulInvalid => lower!(invalid()),
            Builtin::YulSelfdestruct => lower!(selfdestruct(address)),
            Builtin::YulPop => {
                let [_value] = self.lower_builtin_args(builder, builtin, call_args)?;
            }
            _ => {
                return Err(self.recovery_error(
                    Some(call_args.span),
                    "codegen routed a value Yul builtin through unit lowering",
                ));
            }
        }
        Ok(())
    }

    fn lower_yul_value_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        call_args: &CallArgs<'_>,
    ) -> Result<ValueId, ErrorGuaranteed> {
        macro_rules! lower {
            ($method:ident($($arg:ident),* $(,)?)) => {{
                let [$($arg),*] = self.lower_builtin_args(builder, builtin, call_args)?;
                builder.$method($($arg),*)
            }};
        }
        let value = match builtin {
            Builtin::YulAdd => lower!(add(lhs, rhs)),
            Builtin::YulSub => lower!(sub(lhs, rhs)),
            Builtin::YulMul => lower!(mul(lhs, rhs)),
            Builtin::YulDiv => lower!(div(lhs, rhs)),
            Builtin::YulSdiv => lower!(sdiv(lhs, rhs)),
            Builtin::YulMod => lower!(mod_(lhs, rhs)),
            Builtin::YulSmod => lower!(smod(lhs, rhs)),
            Builtin::YulAddmod => lower!(addmod(a, b, modulus)),
            Builtin::YulMulmod => lower!(mulmod(a, b, modulus)),
            Builtin::YulExp => lower!(exp(base, exponent)),
            Builtin::YulSignextend => lower!(signextend(byte, value)),
            Builtin::YulAnd => lower!(and(lhs, rhs)),
            Builtin::YulOr => lower!(or(lhs, rhs)),
            Builtin::YulXor => lower!(xor(lhs, rhs)),
            Builtin::YulNot => lower!(not(value)),
            Builtin::YulByte => lower!(byte(index, value)),
            Builtin::YulShl => lower!(shl(shift, value)),
            Builtin::YulShr => lower!(shr(shift, value)),
            Builtin::YulSar => lower!(sar(shift, value)),
            Builtin::YulLt => lower!(lt(lhs, rhs)),
            Builtin::YulGt => lower!(gt(lhs, rhs)),
            Builtin::YulSlt => lower!(slt(lhs, rhs)),
            Builtin::YulSgt => lower!(sgt(lhs, rhs)),
            Builtin::YulEq => lower!(eq(lhs, rhs)),
            Builtin::YulIszero => lower!(iszero(value)),
            Builtin::YulClz => {
                let [value] = self.lower_builtin_args(builder, builtin, call_args)?;
                if self.gcx.sess.opts.evm_version.has_clz() {
                    builder.clz(value)
                } else {
                    return Err(self
                        .gcx
                        .dcx()
                        .err("codegen requires Osaka-compatible EVM for `clz`")
                        .span(call_args.span)
                        .help("compile with `--evm-version osaka` or newer")
                        .emit());
                }
            }
            Builtin::YulMload => lower!(mload(offset)),
            Builtin::YulMsize => lower!(msize()),
            Builtin::YulSload => lower!(sload(slot)),
            Builtin::YulTload => lower!(tload(slot)),
            Builtin::YulCalldataload => lower!(calldataload(offset)),
            Builtin::YulCalldatasize => lower!(calldatasize()),
            Builtin::YulCodesize => lower!(codesize()),
            Builtin::YulExtcodesize => lower!(extcodesize(address)),
            Builtin::YulExtcodehash => lower!(extcodehash(address)),
            Builtin::YulReturndatasize => lower!(returndatasize()),
            Builtin::YulAddress => lower!(address()),
            Builtin::YulBalance => lower!(balance(address)),
            Builtin::YulSelfbalance => lower!(selfbalance()),
            Builtin::YulCaller => lower!(caller()),
            Builtin::YulCallvalue => lower!(callvalue()),
            Builtin::YulOrigin => lower!(origin()),
            Builtin::YulGasprice => lower!(gasprice()),
            Builtin::YulBlockhash => lower!(blockhash(number)),
            Builtin::YulCoinbase => lower!(coinbase()),
            Builtin::YulTimestamp => lower!(timestamp()),
            Builtin::YulNumber => lower!(number()),
            Builtin::YulDifficulty | Builtin::YulPrevrandao => lower!(prevrandao()),
            Builtin::YulGaslimit => lower!(gaslimit()),
            Builtin::YulChainid => lower!(chainid()),
            Builtin::YulGas => lower!(gas()),
            Builtin::YulBasefee => lower!(basefee()),
            Builtin::YulBlobbasefee => lower!(blobbasefee()),
            Builtin::YulBlobhash => lower!(blobhash(index)),
            Builtin::YulKeccak256 => lower!(keccak256(offset, size)),
            Builtin::YulCall => {
                lower!(call(gas, address, value, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulCallcode => {
                lower!(callcode(gas, address, value, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulStaticcall => {
                lower!(staticcall(gas, address, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulDelegatecall => {
                lower!(delegatecall(gas, address, in_offset, in_size, out_offset, out_size))
            }
            Builtin::YulCreate => lower!(create(value, offset, size)),
            Builtin::YulCreate2 => lower!(create2(value, offset, size, salt)),
            Builtin::YulExtcall => {
                let [_address, _input, _value, _gas] =
                    self.lower_builtin_args(builder, builtin, call_args)?;
                return Err(self.unsupported_yul_builtin(builtin, call_args.span));
            }
            Builtin::YulExtdelegatecall | Builtin::YulExtstaticcall => {
                let [_address, _input, _gas] =
                    self.lower_builtin_args(builder, builtin, call_args)?;
                return Err(self.unsupported_yul_builtin(builtin, call_args.span));
            }
            _ => {
                return Err(self.recovery_error(
                    Some(call_args.span),
                    "codegen routed a unit Yul builtin through value lowering",
                ));
            }
        };
        Ok(value)
    }

    fn unsupported_yul_builtin(&self, builtin: Builtin, span: Span) -> ErrorGuaranteed {
        self.gcx
            .dcx()
            .err(format!("unsupported Yul builtin `{}`", builtin.name()))
            .span(span)
            .emit()
    }

    /// Lowers a member function call (e.g., counter.increment()).
    fn lower_member_call_with_opts(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> Option<ValueId> {
        let resolved = self.gcx.resolved_callee(callee.id);
        let builtin = self.gcx.resolved_builtin(callee);

        if let Some(builtin) = builtin
            && Self::builtin_uses_direct_call_lowering(builtin)
        {
            return self.lower_builtin_call(builder, builtin, args);
        }

        // Handle `Contract.StructType(args)`.
        if let Some(resolved) = resolved
            && let hir::Res::Item(hir::ItemId::Struct(struct_id)) = resolved.res
        {
            return Some(self.lower_struct_constructor(builder, struct_id, args));
        }

        // Handle enum conversion written as `Container.Enum(x)`.
        if let Some(resolved) = resolved
            && let hir::Res::Item(hir::ItemId::Enum(enum_id)) = resolved.res
        {
            let [arg] = match self.positional_args(args, "enum conversion") {
                Ok(args) => args,
                Err(guar) => return self.call_error_result(builder, callee, guar),
            };
            let value = self.lower_value_expr(builder, arg);
            let variant_count = self.gcx.hir.enumm(enum_id).variants.len();
            self.emit_enum_range_check(builder, value, variant_count);
            return Some(value);
        }

        // Handle library function calls: Library.func(args).
        if self.is_library_type_expr(base)
            && let Some(func_id) = self.resolved_function_callee(callee)
        {
            return self.lower_library_call(builder, func_id, args, None);
        }

        // `Base.f(...)` and `super.f(...)` are exact internal calls.
        if let Some(func_id) = self.resolved_exact_function_callee(base, callee) {
            return self.lower_resolved_internal_call(builder, func_id, func_id, args);
        }

        // Handle address payable transfer/send builtins
        if let Some(builtin @ (Builtin::AddressPayableTransfer | Builtin::AddressPayableSend)) =
            builtin
        {
            let [amount] = match self.builtin_args(builtin, args) {
                Ok(exprs) => exprs,
                Err(guar) => return self.call_error_result(builder, callee, guar),
            };
            let addr = self.lower_value_expr(builder, base);
            let amount = self.lower_value_expr(builder, amount);

            // CALL adds the 2300 stipend for a nonzero value. Supplying another
            // 2300 would expose 4600 gas to the recipient.
            let zero = builder.imm_u64(0);
            let stipend = builder.imm_u64(2300);
            let amount_is_zero = builder.iszero(amount);
            let gas = builder.select(amount_is_zero, stipend, zero);
            let success = builder.call(gas, addr, amount, zero, zero, zero, zero);

            if builtin == Builtin::AddressPayableTransfer {
                self.emit_forwarding_revert_unless(builder, success);
                return None;
            }
            // send returns success bool
            return Some(success);
        }

        // Handle low-level call/staticcall/delegatecall
        // addr.call{value: X}(data) returns (bool success, bytes memory returndata)
        // addr.staticcall(data) returns (bool success, bytes memory returndata)
        // addr.delegatecall(data) returns (bool success, bytes memory returndata)
        if let Some(builtin) = builtin
            && let Some(kind) = ExternalCallKind::from_low_level_builtin(builtin)
        {
            if builtin == Builtin::AddressStaticcall
                && !self.gcx.sess.opts.evm_version.has_static_call()
            {
                return self.err_call_result(
                    builder,
                    callee,
                    member.span,
                    "codegen cannot use `staticcall` before Byzantium".to_string(),
                );
            }
            let [data_arg] = match self.builtin_args(builtin, args) {
                Ok(exprs) => exprs,
                Err(guar) => return self.call_error_result(builder, callee, guar),
            };
            let addr = self.lower_value_expr(builder, base);

            // Get the calldata bytes argument.
            // Supported inputs are literals and ABI encode calls. Other bytes
            // expressions are diagnosed by `lower_bytes_arg_to_memory`.
            let (calldata_offset, calldata_size) =
                match self.lower_bytes_arg_to_memory(builder, data_arg) {
                    Ok(data) => data,
                    Err(guar) => return self.call_error_result(builder, callee, guar),
                };

            let (gas, value) =
                self.lower_external_call_options(builder, call_opts, kind.accepts_value());

            // The call itself yields the success flag. Tuple consumers copy
            // the second `bytes` result from returndata immediately afterward.
            let ret_offset = builder.imm_u64(0);
            let ret_size = builder.imm_u64(0);

            let success = self.emit_external_call(
                builder,
                kind,
                gas,
                addr,
                value,
                calldata_offset,
                calldata_size,
                ret_offset,
                ret_size,
            );

            // Low-level calls return `(bool, bytes)`, but this expression path
            // exposes only the first value. `lower_multi_var_decl` copies the
            // returndata bytes out of the return buffer when they are bound.
            return Some(success);
        }

        // Handle storage `bytes`/`string` methods before the generic member
        // call path. Their storage layout is Solidity's packed short/long
        // bytes form, not the generic dynamic-array layout. The receiver may be
        // a state variable, a storage-reference local, or a `bytes` field
        // reached through one (`state.part.push(b)`); `lower_lvalue_slot`
        // resolves the slot for all of these.
        if let Some(builtin @ (Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop)) =
            builtin
            && self.expr_is_storage_bytes_lvalue(base)
            && let Some(slot) = self.lower_lvalue_slot(builder, base)
        {
            return self.lower_storage_bytes_method_call(builder, slot, builtin, args);
        }

        // Resolve the receiver's slot at runtime so storage dynamic-array methods
        // work on references, nested arrays, mapping values, and struct fields.
        if let Some(builtin @ (Builtin::ArrayPush0 | Builtin::ArrayPush | Builtin::ArrayPop)) =
            builtin
            && let Some(array) = self.storage_dynamic_array_info(builder, base)
        {
            return self.lower_array_method_call(builder, array, builtin, args);
        }

        // Handle `using X for Y` library calls: x.method(args) -> Library.method(x, args)
        if let Some(resolved) = resolved
            && resolved.attached
            && let hir::Res::Item(hir::ItemId::Function(func_id)) = resolved.res
        {
            let bound_arg = if self
                .gcx
                .hir
                .function(func_id)
                .parameters
                .first()
                .is_some_and(|&param| self.param_is_storage_ref(param))
            {
                self.lower_lvalue_slot(builder, base).unwrap_or_else(|| {
                    self.err_value(
                        builder,
                        base.span,
                        "cannot resolve the storage slot of this attached library call receiver"
                            .to_string(),
                    )
                })
            } else {
                self.lower_value_expr(builder, base)
            };
            return self.lower_library_call(builder, func_id, args, Some(bound_arg));
        }

        // Look up the function being called to get its selector and return count.
        let resolved_func = self.resolved_function_callee(callee);
        if resolved_func.is_none() && self.gcx.has_typeck_results() {
            // The callee is unresolved: either a prior error left the receiver
            // untyped, or it is a member call on a receiver shape codegen does
            // not handle yet (e.g. `push`/`pop` on a nested or mapping-nested
            // array). Report it instead of asserting the typeck invariant.
            return self.err_call_result(
                builder,
                callee,
                member.span,
                format!("codegen does not support this `.{member}` member call yet"),
            );
        }
        let (selector, num_returns, struct_return_info) = if let Some(func_id) = resolved_func {
            (
                u32::from_be_bytes(self.gcx.function_selector(func_id).0),
                self.function_return_slot_count(func_id),
                self.function_struct_return(func_id),
            )
        } else {
            (
                self.compute_member_selector(base, member),
                self.get_member_function_return_count(base, member),
                None,
            )
        };
        // Use the recursive ABI encoder for every high-level call. The former
        // shallow struct loop copied nested memory pointers as calldata words
        // and treated dynamic bytes pointers as their encoded value.
        let arg_exprs = match self.ordered_call_args(callee, args) {
            Ok(exprs) => exprs,
            Err(guar) => return self.call_error_result(builder, callee, guar),
        };
        let selector_word = builder.imm_u256(U256::from(selector) << 224);
        let (calldata_start, calldata_size) =
            match self.abi_encode_call_payload(builder, Some(selector_word), arg_exprs.into_iter())
            {
                Ok(payload) => payload,
                Err(guar) => return self.call_error_result(builder, callee, guar),
            };

        let addr = self.lower_value_expr(builder, base);

        // Determine where to store return data and whether it's a struct
        let (ret_offset, ret_size, struct_ptr_opt) =
            if let Some((_struct_id, field_count)) = struct_return_info {
                // For struct returns, reserve a separate output allocation.
                let struct_size = (field_count as u64) * 32;
                let struct_size_val = builder.imm_u64(struct_size);
                let struct_ptr = builder.alloc_object(
                    struct_size_val,
                    crate::mir::MemoryObjectLayout::Struct { fields: field_count as u64 },
                    crate::mir::AllocationSemantics::INTERNAL,
                );

                let ret_size = builder.imm_u64(struct_size);
                (struct_ptr, ret_size, Some(struct_ptr))
            } else {
                // Reuse the unbumped calldata allocation for return data. CALL
                // has consumed the input before writing output.
                let ret_offset = if num_returns > 1 { calldata_start } else { builder.imm_u64(0) };
                let ret_size = builder.imm_u64((num_returns * 32) as u64);
                (ret_offset, ret_size, None)
            };

        let kind = self.external_function_call_kind(resolved_func);
        let (gas, value) =
            self.lower_external_call_options(builder, call_opts, kind.accepts_value());
        let success = self.emit_external_call(
            builder,
            kind,
            gas,
            addr,
            value,
            calldata_start,
            calldata_size,
            ret_offset,
            ret_size,
        );

        self.emit_forwarding_revert_unless(builder, success);

        if num_returns > 1 {
            let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            builder.mstore(ptr_slot, ret_offset);
        }

        if num_returns == 0 {
            return None;
        }

        // For struct returns, the data is already in the right place (at struct_ptr).
        // Just return the pointer.
        if let Some(struct_ptr) = struct_ptr_opt {
            return Some(struct_ptr);
        }

        // Load first return value from memory
        // Multi-return consumers snapshot additional words from the ephemeral
        // buffer at `ret_offset` before lowering any lvalues.
        Some(builder.mload(ret_offset))
    }

    pub(super) fn resolved_function_callee(
        &self,
        callee: &hir::Expr<'_>,
    ) -> Option<hir::FunctionId> {
        self.gcx.resolved_function(callee)
    }

    fn is_library_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.get_expr_type(expr) else { return false };
        let TyKind::Type(ty) = ty.kind else { return false };
        let TyKind::Contract(contract_id) = ty.kind else { return false };
        self.gcx.hir.contract(contract_id).kind.is_library()
    }

    fn function_return_slot_count(&self, func_id: hir::FunctionId) -> usize {
        self.return_slot_count(self.gcx.hir.function(func_id).returns)
    }

    fn return_slot_count(&self, returns: &[hir::VariableId]) -> usize {
        let mut total = 0;
        for &var_id in returns {
            let var = self.gcx.hir.variable(var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                total += self.gcx.hir.strukt(*struct_id).fields.len();
            } else {
                total += 1;
            }
        }
        total
    }

    fn function_struct_return(&self, func_id: hir::FunctionId) -> Option<(hir::StructId, usize)> {
        self.struct_return(self.gcx.hir.function(func_id).returns)
    }

    fn struct_return(&self, returns: &[hir::VariableId]) -> Option<(hir::StructId, usize)> {
        if returns.len() == 1 {
            let var = self.gcx.hir.variable(returns[0]);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                return Some((*struct_id, self.gcx.hir.strukt(*struct_id).fields.len()));
            }
        }
        None
    }

    /// Lowers explicit external-call options in source order and fills in the
    /// default gas and value operands.
    pub(super) fn lower_external_call_options(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
        accepts_value: bool,
    ) -> (ValueId, Option<ValueId>) {
        let mut gas = None;
        let mut value = None;
        if let Some(opts) = call_opts {
            for opt in opts {
                let option = self.lower_value_expr(builder, &opt.value);
                if opt.name.name == kw::Gas {
                    gas = Some(option);
                } else if opt.name.name == sym::value && accepts_value {
                    value = Some(option);
                }
            }
        }
        let gas = gas.unwrap_or_else(|| builder.gas());
        let value = accepts_value.then(|| value.unwrap_or_else(|| builder.imm_u64(0)));
        (gas, value)
    }

    pub(super) fn external_function_call_kind(
        &self,
        func_id: Option<hir::FunctionId>,
    ) -> ExternalCallKind {
        let state_mutability = func_id
            .map(|func_id| self.gcx.hir.function(func_id).state_mutability)
            .unwrap_or(hir::StateMutability::NonPayable);
        ExternalCallKind::from_state_mutability(
            state_mutability,
            self.gcx.sess.opts.evm_version.has_static_call(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_external_call(
        &self,
        builder: &mut FunctionBuilder<'_>,
        kind: ExternalCallKind,
        gas: ValueId,
        addr: ValueId,
        value: Option<ValueId>,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    ) -> ValueId {
        match kind {
            ExternalCallKind::Call => {
                let value = value.unwrap_or_else(|| {
                    let guar = self
                        .recovery_error(None, "codegen expected `CALL` to have a value operand");
                    builder.error_value(guar)
                });
                builder.call(gas, addr, value, args_offset, args_size, ret_offset, ret_size)
            }
            ExternalCallKind::StaticCall => {
                builder.staticcall(gas, addr, args_offset, args_size, ret_offset, ret_size)
            }
            ExternalCallKind::DelegateCall => {
                builder.delegatecall(gas, addr, args_offset, args_size, ret_offset, ret_size)
            }
        }
    }

    fn member_contract(&self, base: &hir::Expr<'_>) -> Option<hir::ContractId> {
        if let Some(var_id) = self.gcx.resolved_variable(base) {
            let ty = self.gcx.type_of_item(var_id.into());
            if let TyKind::Contract(contract_id) = ty.kind {
                return Some(contract_id);
            }
        }

        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
                self.gcx.resolved_expr(callee)
        {
            return Some(contract_id);
        }

        if let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
            self.gcx.resolved_expr(base)
        {
            return Some(contract_id);
        }

        if self.gcx.resolved_builtin(base) == Some(Builtin::This) {
            return self.current_contract_id;
        }

        None
    }

    fn find_member_function(&self, base: &hir::Expr<'_>, member: Ident) -> Option<hir::FunctionId> {
        let contract = self.gcx.hir.contract(self.member_contract(base)?);
        contract.linearized_bases.iter().find_map(|&base_id| {
            self.gcx.hir.contract(base_id).all_functions().find(|&func_id| {
                self.gcx.hir.function(func_id).name.is_some_and(|name| name.name == member.name)
            })
        })
    }

    /// Computes the function selector for a member call.
    pub(super) fn compute_member_selector(&self, base: &hir::Expr<'_>, member: Ident) -> u32 {
        if let Some(func_id) = self.find_member_function(base, member) {
            return u32::from_be_bytes(self.gcx.function_selector(func_id).0);
        }

        let sig = format!("{}()", member.name);
        let hash = alloy_primitives::keccak256(sig.as_bytes());
        u32::from_be_bytes(hash[..4].try_into().unwrap())
    }

    /// Gets the number of return values for a member function call.
    pub(super) fn get_member_function_return_count(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> usize {
        self.find_member_function(base, member)
            .map_or(1, |func_id| self.function_return_slot_count(func_id))
    }

    /// Whether a parameter is a storage reference — a `mapping` (always storage)
    /// or any type declared with the `storage` data location. Such parameters are
    /// passed by slot number rather than by value.
    pub(super) fn param_is_storage_ref(&self, param_id: hir::VariableId) -> bool {
        let var = self.gcx.hir.variable(param_id);
        matches!(var.ty.kind, hir::TypeKind::Mapping(_))
            || var.data_location == Some(solar_ast::DataLocation::Storage)
    }

    /// Lowers an internal function call.
    fn lower_internal_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
    ) -> Option<ValueId> {
        let argument_source = func_id;
        let func_id = self.virtual_function_target(func_id);
        self.lower_resolved_internal_call(builder, func_id, argument_source, args)
    }

    fn lower_resolved_internal_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        argument_source: hir::FunctionId,
        args: &CallArgs<'_>,
    ) -> Option<ValueId> {
        let func = self.gcx.hir.function(func_id);

        // A storage-reference parameter (a `mapping`, or an array/struct in
        // `storage`) is passed by slot number, so such an argument is lowered to
        // its storage slot rather than as a value — lowering it as a value would
        // `sload` the slot and pass the wrong thing.
        let params = func.parameters;
        let arg_exprs = match self.ordered_function_args(argument_source, args, false) {
            Ok(exprs) => exprs,
            Err(guar) => {
                return (!func.returns.is_empty()).then(|| builder.error_value(guar));
            }
        };
        let arg_vals: Vec<ValueId> = arg_exprs
            .into_iter()
            .enumerate()
            .map(|(i, arg)| {
                if params.get(i).is_some_and(|&p| self.param_is_storage_ref(p))
                    && let Some(slot) = self.lower_lvalue_slot(builder, arg)
                {
                    slot
                } else {
                    // A memory parameter receives one word; a logical slice
                    // materializes into the memory object the parameter expects.
                    let value = self.lower_value_expr(builder, arg);
                    match params.get(i) {
                        Some(&param_id) => self.coerce_arg_for_param(builder, param_id, arg, value),
                        None => self.coerce_memory_slice_value(builder, value),
                    }
                }
            })
            .collect();

        self.lower_internal_call_values(builder, func_id, arg_vals)
    }

    /// Lowers an internal call given already-lowered argument values. Used for
    /// operator expressions, whose operands are plain values rather than
    /// argument expressions.
    pub(super) fn lower_internal_call_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> Option<ValueId> {
        let func = self.gcx.hir.function(func_id);
        if func.body.is_none() {
            let guar = self
                .gcx
                .dcx()
                .err("codegen cannot lower an internal function without a body")
                .span(func.span)
                .emit();
            return (!func.returns.is_empty()).then(|| builder.error_value(guar));
        }
        let params = func.parameters;

        if func.returns.is_empty() {
            self.emit_internal_void_call(builder, func_id, arg_vals);
            return None;
        }

        // A `bytes`/`string` calldata slice return crosses the internal-call
        // boundary as an `(offset, length)` pair, which slice lowering does not
        // expand on the return side; a real `internal_call` would leave a slice
        // the backend cannot lower. Inline the callee instead so its named slice
        // return is reconstructed at the call site (where it folds away). This
        // is the `bytes calldata` helper idiom (`_emptyData`, `emptySignature`).
        if self.returns_calldata_slice(func) {
            let has_storage_ref_param = params.iter().any(|&p| self.param_is_storage_ref(p));
            let value = self.lower_calldata_slice_return_call(
                builder,
                func_id,
                arg_vals,
                has_storage_ref_param,
            );
            return Some(value);
        }

        Some(self.emit_internal_call(builder, func_id, arg_vals))
    }

    /// Whether any return of `func` is a `bytes`/`string`/array calldata
    /// slice. One such return is enough to force the inline path: a slice
    /// cannot cross a real `internal_call` boundary.
    fn returns_calldata_slice(&self, func: &hir::Function<'_>) -> bool {
        func.returns
            .iter()
            .any(|&id| Self::calldata_dynamic_var_kind(self.gcx.hir.variable(id)).is_some())
    }

    /// Whether `expr` is a direct call to a function with multiple returns of
    /// which at least one is a calldata slice — the shape whose inlining
    /// delivers its values through `pending_inline_returns` instead of the
    /// one-word-per-value multi-return buffer.
    pub(super) fn is_slice_multi_return_call(&self, expr: &hir::Expr<'_>) -> bool {
        let ExprKind::Call(callee, ..) = &expr.kind else { return false };
        let Some(func_id) = self.resolved_function_callee(callee) else { return false };
        let func = self.gcx.hir.function(func_id);
        func.returns.len() > 1 && self.returns_calldata_slice(func)
    }

    /// Lowers a call to an internal function that returns a calldata slice by
    /// inlining its body, so the returned slice is a `make_slice` at the call
    /// site that folds away. Full block lowering handles both straight-line and
    /// control-flow bodies through one inline exit block, with each return
    /// merging through its slot. Multi-return values are left pending for
    /// destructuring to consume, since a slice cannot ride the
    /// one-word-per-value multi-return buffer. A callee that cannot be inlined
    /// — a storage-reference parameter or recursion — is reported instead of
    /// lowered to a slice the backend cannot handle.
    fn lower_calldata_slice_return_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
        has_storage_ref_param: bool,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        if let Some(body) = func.body
            && !has_storage_ref_param
            && !self.function_is_recursive(func_id)
            && self.try_enter_inline(func_id)
        {
            let values = self.inline_slice_return_body(builder, func, &body, &arg_vals);
            self.exit_inline();
            let Some(first) = values.first().copied() else {
                return self.err_value(
                    builder,
                    func.span,
                    "codegen expected a calldata-slice return value",
                );
            };
            if values.len() > 1 {
                self.pending_inline_returns = Some(values);
            }
            return first;
        }
        let guar = self
            .gcx
            .dcx()
            .err("returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen")
            .span(func.span)
            .emit();
        builder.error_value(guar)
    }

    /// Inlines a calldata-slice-returning function through full block lowering.
    /// Every return variable is given a local slot up front — two words seeded
    /// with an empty slice for a calldata slice, one zeroed word otherwise — so
    /// a value assigned on only one branch merges through memory. An explicit
    /// `return` in the body stores its values into these slots and jumps to the
    /// inline exit block ([`Self::lower_inline_return`]); implicit named
    /// returns fall through to the same join. The exit block reads every slot
    /// back and returns the values in declaration order.
    fn inline_slice_return_body(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func: &hir::Function<'_>,
        body: &hir::Block<'_>,
        arg_vals: &[ValueId],
    ) -> Vec<ValueId> {
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_slice_slot_locals = std::mem::take(&mut self.slice_slot_locals);
        let saved_inline_returns = self.inline_returns.take();
        let saved_pending = self.pending_inline_returns.take();

        self.collect_assigned_vars_block(body);

        for &ret_id in func.returns {
            if Self::calldata_dynamic_var_kind(self.gcx.hir.variable(ret_id)).is_some() {
                let offset = self.alloc_local_slice_memory(ret_id);
                self.init_empty_slice_slot(builder, offset);
            } else {
                let offset = self.alloc_local_memory(ret_id);
                let addr = self.local_memory_addr(builder, offset);
                let zero = builder.imm_u64(0);
                builder.mstore(addr, zero);
            }
        }

        for (i, &param_id) in func.parameters.iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.bind_param_value(builder, param_id, arg_val);
            }
        }

        let exit_block = builder.create_block();
        self.inline_returns =
            Some(crate::lower::InlineReturnCtx { exit_block, return_vars: func.returns.to_vec() });

        let saved_in_unchecked_block = self.in_unchecked_block;
        self.in_unchecked_block = false;
        self.lower_block(builder, body);
        self.in_unchecked_block = saved_in_unchecked_block;

        // Implicit fallthrough joins the explicit returns at the exit block.
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(exit_block);
        }
        builder.switch_to_block(exit_block);

        // Read every return through its slot before caller state is restored;
        // the loaded values stay valid afterwards.
        let values: Vec<ValueId> = func
            .returns
            .iter()
            .map(|&ret_id| {
                let Some(offset) = self.get_local_memory_offset(&ret_id) else {
                    return self.err_value(
                        builder,
                        self.gcx.hir.variable(ret_id).span,
                        "codegen is missing an inline return slot",
                    );
                };
                if self.is_slice_slot_local(&ret_id) {
                    self.load_slice_slot(builder, offset, crate::mir::SliceLocation::Calldata)
                } else {
                    let addr = self.local_memory_addr(builder, offset);
                    builder.mload(addr)
                }
            })
            .collect();

        // Deliberately keep `next_local_memory_offset`: the body's slots —
        // above all the return slots the loaded values came from — stay part
        // of the enclosing function's frame. Rolling the offset back would let
        // later locals and, worse, the backend's cross-block spill area (which
        // starts at the frame's final high-water mark) reuse addresses whose
        // stored slices the call site still consumes.
        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.assigned_vars = saved_assigned_vars;
        self.slice_slot_locals = saved_slice_slot_locals;
        self.inline_returns = saved_inline_returns;
        self.pending_inline_returns = saved_pending;

        values
    }

    fn internal_call_target(&mut self, func_id: hir::FunctionId) -> crate::mir::FunctionId {
        let func = self.gcx.hir.function(func_id);
        if matches!(func.visibility, hir::Visibility::Internal | hir::Visibility::Private) {
            self.ensure_function_lowered(func_id)
        } else {
            self.ensure_internal_mir_function(func_id)
        }
    }

    fn emit_internal_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let Some(&ret_id) = func.returns.first() else {
            let guar = self.recovery_error(
                Some(func.span),
                "codegen expected internal call to return a value",
            );
            return builder.error_value(guar);
        };
        let result_ty = self.lower_type_from_var(ret_id);
        let mir_id = self.internal_call_target(func_id);
        builder.internal_call(mir_id, arg_vals, result_ty, func.returns.len())
    }

    fn emit_internal_void_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) {
        debug_assert!(self.gcx.hir.function(func_id).returns.is_empty());
        let mir_id = self.internal_call_target(func_id);
        builder.internal_call_void(mir_id, arg_vals, 0);
    }

    /// Lowers a base constructor call using already-resolved constructor arguments.
    pub(super) fn lower_base_constructor_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ctor_id: hir::FunctionId,
        arg_vals: &[ValueId],
    ) {
        self.lower_inline_constructor(builder, ctor_id, arg_vals)
    }

    /// Inlines a base constructor into the derived constructor.
    fn lower_inline_constructor(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: &[ValueId],
    ) {
        let func = self.gcx.hir.function(func_id);
        let parameters = func.parameters;
        let body = func.body;

        if !self.try_enter_inline(func_id) {
            {
                self.gcx
                    .dcx()
                    .err("codegen does not support this recursive call through inlining yet")
                    .emit();
                return;
            }
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_inline_returns = self.inline_returns.take();
        let saved_pending = self.pending_inline_returns.take();

        if let Some(body) = body {
            self.collect_assigned_vars_block(&body);
        }

        for (i, &param_id) in parameters.iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.bind_param_value(builder, param_id, arg_val);
            }
        }

        if let Some(body) = body {
            let exit_block = builder.create_block();
            self.inline_returns =
                Some(crate::lower::InlineReturnCtx { exit_block, return_vars: Vec::new() });
            let saved_in_unchecked_block = self.in_unchecked_block;
            self.in_unchecked_block = false;
            self.lower_block(builder, &body);
            self.in_unchecked_block = saved_in_unchecked_block;
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(exit_block);
            }
            builder.switch_to_block(exit_block);
        }

        // Keep `next_local_memory_offset`: the body's local slots stay part of
        // the enclosing function's frame. Rolling the offset back would place
        // the backend's cross-block spill area — which starts at the frame's
        // final high-water mark — inside this region, so a caller value
        // spilled across the inlined body would be clobbered by its locals.
        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.assigned_vars = saved_assigned_vars;
        self.inline_returns = saved_inline_returns;
        self.pending_inline_returns = saved_pending;
        self.exit_inline();
    }

    /// Lowers constructor arguments into the representation expected by the
    /// callee body. Memory `bytes`/`string` parameters receive Solidity's
    /// `[length][data...]` memory pointer, including literal base-constructor
    /// arguments such as `ERC20("Name", "SYM")`.
    pub(super) fn lower_constructor_arg(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg: &hir::Expr<'_>,
        param_ty: &hir::Type<'_>,
    ) -> ValueId {
        if matches!(
            param_ty.kind,
            hir::TypeKind::Elementary(hir::ElementaryType::String | hir::ElementaryType::Bytes)
        ) {
            return self.lower_expr_as_memory_bytes(builder, arg);
        }

        self.lower_value_expr(builder, arg)
    }

    /// Returns the linked address of the library that defines `func_id`, when
    /// one was supplied via `--libraries Name=0xADDRESS`.
    fn linked_library_address(&self, func_id: hir::FunctionId) -> Option<U256> {
        let libraries = &self.gcx.sess.opts.libraries;
        if libraries.is_empty() {
            return None;
        }
        let contract_id = self.gcx.hir.function(func_id).contract?;
        let contract = self.gcx.hir.contract(contract_id);
        if !contract.kind.is_library() {
            return None;
        }
        let name = contract.name.as_str();
        let source = self.gcx.hir.source(contract.source).file.name.display().to_string();
        let library = libraries
            .iter()
            .find(|library| {
                library.name == name && library.source.as_deref() == Some(source.as_str())
            })
            .or_else(|| {
                libraries.iter().find(|library| library.name == name && library.source.is_none())
            })?;
        Some(U256::from_be_slice(library.address.as_slice()))
    }

    /// How a struct field travels across a linked-library call boundary.
    pub(super) fn linked_field_kind(&self, ty: Ty<'gcx>) -> Option<LinkedFieldKind> {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(LinkedFieldKind::DynBytes)
            }
            TyKind::DynArray(elem) | TyKind::Slice(elem) => {
                let elem = elem.peel_refs();
                (!self.abi_is_dynamic(elem)
                    && !matches!(elem.kind, TyKind::Struct(_) | TyKind::Array(..)))
                .then_some(LinkedFieldKind::DynArray)
            }
            // Aggregates are memory pointers in the field-inlined layout and
            // cannot cross the delegatecall boundary as a word.
            TyKind::Struct(_) | TyKind::Array(..) | TyKind::Tuple(_) => None,
            _ => (!self.abi_is_dynamic(ty)).then_some(LinkedFieldKind::Value),
        }
    }

    /// Whether every parameter of `func_id` is encodable by the linked-library
    /// delegatecall convention: value types, storage references (passed by
    /// slot), and memory structs whose fields are values or one-level dynamic
    /// arrays/bytes (offset + tail). Anything else uses the unlinked internal
    /// body because a raw memory pointer would be meaningless to DELEGATECALL.
    fn linked_library_args_supported(&self, func_id: hir::FunctionId) -> bool {
        let func = self.gcx.hir.function(func_id);
        func.parameters.iter().all(|&param_id| {
            if self.param_is_storage_ref(param_id) {
                return true;
            }
            let ty = self.gcx.type_of_item(param_id.into());
            match ty.peel_refs().kind {
                TyKind::Struct(id) => self
                    .gcx
                    .struct_field_types(id)
                    .iter()
                    .all(|&field| self.linked_field_kind(field).is_some()),
                _ => self.linked_field_kind(ty) == Some(LinkedFieldKind::Value),
            }
        })
    }

    /// Lowers a call to a `public`/`external` function of a linked library as
    /// an ABI-encoded `DELEGATECALL` to the linked address, mirroring solc's
    /// library call convention: the library runs in the caller's storage and
    /// `msg` context, storage-reference arguments travel as their slot, and a
    /// failed call re-raises the callee's revert data.
    fn lower_linked_library_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &[&hir::Expr<'_>],
        lib_addr: U256,
    ) -> Option<ValueId> {
        let func = self.gcx.hir.function(func_id);
        let selector = u32::from_be_bytes(self.gcx.function_selector(func_id).0);
        let num_returns = self.function_return_slot_count(func_id);
        let struct_return_info = self.function_struct_return(func_id);

        // Evaluate arguments first: storage references lower to their slot,
        // memory structs to their pointer (field-inlined below), scalars to
        // their value.
        let params = func.parameters;
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut arg_slots = Vec::with_capacity(args.len());
        let mut arg_structs = Vec::with_capacity(args.len());
        for (i, &arg) in args.iter().enumerate() {
            let is_storage_ref = params.get(i).is_some_and(|&p| self.param_is_storage_ref(p));
            if is_storage_ref {
                let slot = self.lower_lvalue_slot(builder, arg).unwrap_or_else(|| {
                    self.err_value(
                        builder,
                        arg.span,
                        "cannot resolve the storage slot of this library call argument".to_string(),
                    )
                });
                arg_vals.push(slot);
                arg_slots.push(1usize);
                arg_structs.push(None);
            } else if let Some((struct_id, field_count)) = self.get_expr_struct_info(arg) {
                arg_vals.push(self.lower_value_expr(builder, arg));
                arg_slots.push(field_count);
                arg_structs.push(Some(struct_id));
            } else {
                arg_vals.push(self.lower_value_expr(builder, arg));
                arg_slots.push(1usize);
                arg_structs.push(None);
            }
        }
        let head_size_bytes = 4 + arg_slots.iter().sum::<usize>() * 32;

        // Build the calldata at the free pointer.
        let calldata_start = builder.fmp();

        let selector_val = builder.imm_u256(U256::from(selector) << 224);
        builder.mstore(calldata_start, selector_val);

        // Heads. A dynamic struct field reserves its head slot here and is
        // filled by the tail pass below with the tail's args-relative offset.
        let mut pending_tails: Vec<(u64, ValueId, LinkedFieldKind)> = Vec::new();
        let mut arg_offset = 4u64;
        for ((arg_val, slots), &struct_id) in arg_vals.iter().zip(&arg_slots).zip(&arg_structs) {
            if let Some(struct_id) = struct_id {
                let field_tys = self.gcx.struct_field_types(struct_id);
                let layout = crate::mir::MemoryObjectLayout::structure(*slots as u64);
                for field_idx in 0..*slots {
                    let field_addr =
                        builder.memory_object_field_addr(*arg_val, layout, field_idx as u64);
                    let field_val = builder.mload(field_addr);
                    match field_tys.get(field_idx).and_then(|&f| self.linked_field_kind(f)) {
                        Some(kind @ (LinkedFieldKind::DynArray | LinkedFieldKind::DynBytes)) => {
                            // `field_val` is the caller-memory pointer of the
                            // array/bytes; its contents travel in the tail.
                            pending_tails.push((arg_offset, field_val, kind));
                        }
                        _ => {
                            let offset_val = builder.imm_u64(arg_offset);
                            let write_addr = builder.add(calldata_start, offset_val);
                            builder.mstore(write_addr, field_val);
                        }
                    }
                    arg_offset += 32;
                }
            } else {
                let offset_val = builder.imm_u64(arg_offset);
                let write_addr = builder.add(calldata_start, offset_val);
                builder.mstore(write_addr, *arg_val);
                arg_offset += 32;
            }
        }

        // Tails: `[len][data...]` blobs appended after the heads; each head
        // slot holds its tail's offset relative to the args start (after the
        // selector), so the callee decodes with `calldataload(4 + offset)`.
        let mut tail_off = builder.imm_u64((head_size_bytes - 4) as u64);
        let word = builder.imm_u64(32);
        for (head_off, src, kind) in pending_tails {
            let object_kind = kind.memory_object_kind();
            let head_addr_off = builder.imm_u64(head_off);
            let head_addr = builder.add(calldata_start, head_addr_off);
            builder.mstore(head_addr, tail_off);

            let len = builder.memory_object_len(src, object_kind);
            let byte_len = kind.data_size(builder, len, word);

            let four = builder.imm_u64(4);
            let args_base = builder.add(calldata_start, four);
            let dst = builder.add(args_base, tail_off);
            builder.mstore(dst, len);
            let dst_data = builder.add(dst, word);
            let src_data = builder.memory_object_data(src, object_kind);
            builder.mcopy(dst_data, src_data, byte_len);

            let advanced = builder.add(word, byte_len);
            tail_off = builder.add(tail_off, advanced);
        }
        let four = builder.imm_u64(4);
        let total_size = builder.add(four, tail_off);

        // Return area: reuse the unbumped calldata allocation for value-type
        // returns, or append an allocation for struct returns.
        let (ret_offset, ret_size, struct_ptr_opt) =
            if let Some((_struct_id, field_count)) = struct_return_info {
                let struct_size = field_count as u64 * 32;
                let struct_ptr = builder.add(calldata_start, total_size);
                let struct_size_val = builder.imm_u64(struct_size);
                let new_free_ptr = builder.add(struct_ptr, struct_size_val);
                builder.set_fmp(new_free_ptr);
                (struct_ptr, builder.imm_u64(struct_size), Some(struct_ptr))
            } else {
                let ret_offset = if num_returns > 1 { calldata_start } else { builder.imm_u64(0) };
                (ret_offset, builder.imm_u64(num_returns as u64 * 32), None)
            };

        let calldata_size = total_size;
        let addr = builder.imm_u256(lib_addr);
        let gas = builder.gas();
        let success =
            builder.delegatecall(gas, addr, calldata_start, calldata_size, ret_offset, ret_size);

        self.emit_forwarding_revert_unless(builder, success);

        if num_returns > 1 {
            let ptr_slot = builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT);
            builder.mstore(ptr_slot, ret_offset);
        }

        if num_returns == 0 {
            return None;
        }
        if let Some(struct_ptr) = struct_ptr_opt {
            return Some(struct_ptr);
        }
        Some(builder.mload(ret_offset))
    }

    /// Lowers a library function call.
    fn lower_library_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
        bound_arg: Option<ValueId>,
    ) -> Option<ValueId> {
        let func = self.gcx.hir.function(func_id);
        let arg_exprs = match self.ordered_function_args(func_id, args, bound_arg.is_some()) {
            Ok(exprs) => exprs,
            Err(guar) => {
                return (!func.returns.is_empty()).then(|| builder.error_value(guar));
            }
        };

        // A `public`/`external` function of a library with a linked address
        // (`--libraries Name=0xADDR`) is called through DELEGATECALL, matching
        // solc's library model and keeping the body out of the caller's bytecode.
        if matches!(func.visibility, hir::Visibility::Public | hir::Visibility::External)
            && bound_arg.is_none()
            && self.linked_library_args_supported(func_id)
            && let Some(lib_addr) = self.linked_library_address(func_id)
        {
            return self.lower_linked_library_call(builder, func_id, &arg_exprs, lib_addr);
        }

        // An unlinked library body is emitted as an internal function in the
        // caller's bytecode. It runs in the caller's storage and `msg` context,
        // matching the delegatecall execution model without requiring a
        // separately deployed library.
        if func.body.is_some() {
            let mut arg_vals: Vec<ValueId> = Vec::new();

            let bound_offset = bound_arg.is_some() as usize;
            if let Some(bound_val) = bound_arg {
                arg_vals.push(bound_val);
            }

            // Lower all explicit arguments. A storage-reference parameter (a
            // `mapping`, or an array/struct in `storage`) is passed by slot, so
            // such an argument is lowered to its storage slot rather than as a
            // value — lowering it as a value would `sload` the slot and pass the
            // wrong thing.
            for (i, &arg) in arg_exprs.iter().enumerate() {
                let param_idx = i + bound_offset;
                if func.parameters.get(param_idx).is_some_and(|&p| self.param_is_storage_ref(p))
                    && let Some(slot) = self.lower_lvalue_slot(builder, arg)
                {
                    arg_vals.push(slot);
                } else {
                    let value = self.lower_value_expr(builder, arg);
                    arg_vals.push(match func.parameters.get(param_idx) {
                        Some(&param_id) => self.coerce_arg_for_param(builder, param_id, arg, value),
                        None => value,
                    });
                }
            }

            if func.returns.is_empty() {
                self.emit_internal_void_call(builder, func_id, arg_vals);
                return None;
            }

            // A library helper returning a calldata slice must inline for the
            // same reason an internal one does (the fallback would leave a slice
            // the backend cannot lower); non-inlinable shapes are reported.
            if self.returns_calldata_slice(func) {
                let has_storage_ref_param =
                    func.parameters.iter().any(|&p| self.param_is_storage_ref(p));
                let value = self.lower_calldata_slice_return_call(
                    builder,
                    func_id,
                    arg_vals,
                    has_storage_ref_param,
                );
                return Some(value);
            }

            Some(self.emit_internal_call(builder, func_id, arg_vals))
        } else {
            let guar =
                self.gcx.dcx().err("codegen does not support external library calls yet").emit();
            (!func.returns.is_empty()).then(|| builder.error_value(guar))
        }
    }

    /// Whether `func_id` directly or indirectly calls itself (cached).
    /// Calldata-slice return calls cannot use their required inline path when recursive.
    fn function_is_recursive(&mut self, func_id: hir::FunctionId) -> bool {
        if let Some(&cached) = self.recursive_functions.get(&func_id) {
            return cached;
        }
        let mut visiting = GrowableBitSet::new_empty();
        let result = self.function_reaches(func_id, func_id, &mut visiting);
        self.recursive_functions.insert(func_id, result);
        result
    }

    fn function_reaches(
        &self,
        current: hir::FunctionId,
        target: hir::FunctionId,
        visiting: &mut GrowableBitSet<hir::FunctionId>,
    ) -> bool {
        if !visiting.insert(current) {
            return false;
        }

        for callee in self.function_callees(current) {
            if callee == target || self.function_reaches(callee, target, visiting) {
                return true;
            }
        }

        false
    }

    fn function_callees(&self, func_id: hir::FunctionId) -> Vec<hir::FunctionId> {
        let mut callees = Vec::new();
        let func = self.gcx.hir.function(func_id);
        if let Some(body) = func.body {
            for stmt in body.stmts {
                self.stmt_collect_callees(stmt, &mut callees);
            }
        }
        callees
    }

    /// Collects calls contained recursively in a statement.
    fn stmt_collect_callees(&self, stmt: &hir::Stmt<'_>, callees: &mut Vec<hir::FunctionId>) {
        use hir::StmtKind;
        match &stmt.kind {
            StmtKind::Expr(e)
            | StmtKind::Return(Some(e))
            | StmtKind::Revert(e)
            | StmtKind::Emit(e) => self.expr_collect_callees(e, callees),
            StmtKind::Block(b) | StmtKind::UncheckedBlock(b) | StmtKind::AssemblyBlock(b) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::If(c, t, e) => {
                self.expr_collect_callees(c, callees);
                self.stmt_collect_callees(t, callees);
                if let Some(e) = e {
                    self.stmt_collect_callees(e, callees);
                }
            }
            StmtKind::Loop(b, _) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::Switch(sw) => {
                self.expr_collect_callees(sw.selector, callees);
                for case in sw.cases {
                    for stmt in case.body.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::Try(t) => {
                self.expr_collect_callees(&t.expr, callees);
                for clause in t.clauses {
                    for stmt in clause.block.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::DeclSingle(var_id) => {
                if let Some(init) = self.gcx.hir.variable(*var_id).initializer {
                    self.expr_collect_callees(init, callees);
                }
            }
            StmtKind::DeclMulti(_, init) => self.expr_collect_callees(init, callees),
            StmtKind::Return(None)
            | StmtKind::Continue
            | StmtKind::Break
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
    }

    /// Collects calls contained recursively in an expression.
    fn expr_collect_callees(&self, expr: &hir::Expr<'_>, callees: &mut Vec<hir::FunctionId>) {
        match &expr.kind {
            ExprKind::Call(callee, args, call_opts) => {
                if let Some(func_id) = self.resolved_function_callee(callee) {
                    callees.push(func_id);
                }
                self.expr_collect_callees(callee, callees);
                for arg in args.kind.exprs() {
                    self.expr_collect_callees(arg, callees);
                }
                if let Some(call_opts) = call_opts {
                    for option in call_opts.args {
                        self.expr_collect_callees(&option.value, callees);
                    }
                }
            }
            ExprKind::Binary(l, _, r) | ExprKind::Assign(l, _, r) => {
                self.expr_collect_callees(l, callees);
                self.expr_collect_callees(r, callees);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Member(e, _)
            | ExprKind::YulMember(e, _)
            | ExprKind::Payable(e)
            | ExprKind::Delete(e) => self.expr_collect_callees(e, callees),
            ExprKind::Ternary(c, t, f) => {
                self.expr_collect_callees(c, callees);
                self.expr_collect_callees(t, callees);
                self.expr_collect_callees(f, callees);
            }
            ExprKind::Index(b, i) => {
                self.expr_collect_callees(b, callees);
                if let Some(i) = i {
                    self.expr_collect_callees(i, callees);
                }
            }
            ExprKind::Slice(b, s, e) => {
                self.expr_collect_callees(b, callees);
                if let Some(s) = s {
                    self.expr_collect_callees(s, callees);
                }
                if let Some(e) = e {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Array(es) => {
                for e in *es {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Tuple(es) => {
                for e in es.iter().flatten() {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Ident(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }
    }

    fn lower_hash_precompile_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Result<ValueId, ErrorGuaranteed> {
        let [input] = self.builtin_args(builtin, args)?;
        let input = self.peel_bytes_conversion(input);
        let (input_ptr, input_len) = self.lower_precompile_bytes_input(builder, input)?;

        let address = builder.imm_u64(if builtin == Builtin::Sha256 { 2 } else { 3 });
        let output_ptr = builder.imm_u64(0);
        let output_size = builder.imm_u64(32);
        let gas = crate::utils::precompile_gas(builder, self.gcx.sess.opts.evm_version);
        let success = self.emit_precompile_call(
            builder,
            gas,
            address,
            input_ptr,
            input_len,
            output_ptr,
            output_size,
        );
        self.emit_forwarding_revert_unless(builder, success);

        let output = builder.mload(output_ptr);
        Ok(if builtin == Builtin::Ripemd160 {
            if self.gcx.sess.opts.evm_version.has_bitwise_shifting() {
                let shift = builder.imm_u64(96);
                builder.shl(shift, output)
            } else {
                let factor = builder.imm_u256(U256::from(1) << 96);
                builder.mul(output, factor)
            }
        } else {
            output
        })
    }

    fn lower_ecrecover_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> Result<ValueId, ErrorGuaranteed> {
        let [hash, v, r, s] = self.builtin_args(Builtin::EcRecover, args)?;
        let hash = self.lower_value_expr(builder, hash);
        let v = self.lower_value_expr(builder, v);
        let r = self.lower_value_expr(builder, r);
        let s = self.lower_value_expr(builder, s);

        let input_ptr = builder.fmp();
        builder.mstore(input_ptr, hash);
        for (offset, value) in [(32, v), (64, r), (96, s)] {
            let offset = builder.imm_u64(offset);
            let ptr = builder.add(input_ptr, offset);
            builder.mstore(ptr, value);
        }

        let output_offset = builder.imm_u64(128);
        let output_ptr = builder.add(input_ptr, output_offset);
        let zero = builder.imm_u64(0);
        builder.mstore(output_ptr, zero);

        let gas = crate::utils::precompile_gas(builder, self.gcx.sess.opts.evm_version);
        let address = builder.imm_u64(1);
        let input_size = builder.imm_u64(128);
        let output_size = builder.imm_u64(32);
        let success = self.emit_precompile_call(
            builder,
            gas,
            address,
            input_ptr,
            input_size,
            output_ptr,
            output_size,
        );
        self.emit_forwarding_revert_unless(builder, success);
        Ok(builder.mload(output_ptr))
    }

    fn lower_precompile_bytes_input(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        input: &hir::Expr<'_>,
    ) -> Result<(ValueId, ValueId), ErrorGuaranteed> {
        if let ExprKind::Lit(lit) = &input.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            let bytes = bytes.as_byte_str();
            if bytes.is_empty() {
                return Ok((builder.imm_u64(0), builder.imm_u64(0)));
            }

            let ptr = builder.fmp();
            for (i, chunk) in bytes.chunks(32).enumerate() {
                let mut padded = [0u8; 32];
                padded[..chunk.len()].copy_from_slice(chunk);
                let value = builder.imm_u256(U256::from_be_bytes(padded));
                let dest = if i == 0 {
                    ptr
                } else {
                    let offset = builder.imm_u64((i * 32) as u64);
                    builder.add(ptr, offset)
                };
                builder.mstore(dest, value);
            }
            return Ok((ptr, builder.imm_u64(bytes.len() as u64)));
        }

        self.lower_bytes_arg_to_memory(builder, input)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_precompile_call(
        &self,
        builder: &mut FunctionBuilder<'_>,
        gas: ValueId,
        address: ValueId,
        input_ptr: ValueId,
        input_size: ValueId,
        output_ptr: ValueId,
        output_size: ValueId,
    ) -> ValueId {
        if self.gcx.sess.opts.evm_version.has_static_call() {
            builder.staticcall(gas, address, input_ptr, input_size, output_ptr, output_size)
        } else {
            let value = builder.imm_u64(0);
            builder.call(gas, address, value, input_ptr, input_size, output_ptr, output_size)
        }
    }

    fn emit_forwarding_revert_unless(&self, builder: &mut FunctionBuilder<'_>, success: ValueId) {
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(success, continue_block, revert_block);

        builder.switch_to_block(revert_block);
        let zero = builder.imm_u64(0);
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            let size = builder.returndatasize();
            builder.returndatacopy(zero, zero, size);
            builder.revert(zero, size);
        } else {
            builder.revert(zero, zero);
        }

        builder.switch_to_block(continue_block);
    }

    pub(super) fn resolved_exact_function_callee(
        &self,
        base: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
    ) -> Option<hir::FunctionId> {
        let function_id = self.resolved_function_callee(callee)?;
        let TyKind::Type(ty) = self.get_expr_type(base)?.kind else { return None };
        match ty.kind {
            TyKind::Contract(_) => Some(function_id),
            TyKind::Super(contract_id) => {
                Some(self.resolve_super_function_target(contract_id, function_id))
            }
            _ => None,
        }
    }

    fn resolve_super_function_target(
        &self,
        defining_contract_id: hir::ContractId,
        function_id: hir::FunctionId,
    ) -> hir::FunctionId {
        let Some(contract_id) = self.contract_id else { return function_id };
        self.gcx.resolve_super_function(contract_id, defining_contract_id, function_id)
    }

    /// Checks if an expression has a contract value type.
    pub(super) fn is_contract_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr).is_some_and(|ty| matches!(ty.kind, TyKind::Contract(_)))
    }
}
