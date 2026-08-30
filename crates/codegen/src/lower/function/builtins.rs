//! Builtin call and value lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_builtin_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
    ) -> Option<ValueId> {
        // if address_call/staticcall/delegatecall { lower_address_call(...) }
        // if send/transfer { lower_payable_address_call(...) }
        // if push/pop { lower_storage_array_{push,pop}(...) }
        // dispatch (yul|solidity) * (void|value)
        match builtin {
            Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall => {
                let ExprKind::Member(receiver, _) = callee.kind else {
                    return report_unsupported(self.context.gcx, callee.span, "address call");
                };
                return self.lower_address_call(
                    callee.span,
                    receiver,
                    builtin,
                    args,
                    call_opts,
                    false,
                );
            }
            Builtin::AddressPayableSend | Builtin::AddressPayableTransfer => {
                let ExprKind::Member(receiver, _) = callee.kind else {
                    return report_unsupported(self.context.gcx, callee.span, "address call");
                };
                return self.lower_payable_address_call(receiver, builtin, args);
            }
            Builtin::ArrayPush => {
                let result = self.builtin_args::<1>(builtin, &args).and_then(|arguments| {
                    self.lower_storage_array_push(expr, callee, arguments.first())
                });
                return Some(result.unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)));
            }
            Builtin::ArrayPush0 => {
                let result = self
                    .builtin_args::<0>(builtin, &args)
                    .and_then(|_| self.lower_storage_array_push(expr, callee, None));
                return Some(result.unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)));
            }
            Builtin::ArrayPop => {
                let result = self
                    .builtin_args::<0>(builtin, &args)
                    .and_then(|_| self.lower_storage_array_pop(expr, callee));
                return Some(result.unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)));
            }
            _ => {}
        }

        let (is_yul, is_void) = match builtin {
            builtin if builtin.is_yul() => {
                let Some(returns) = builtin.ty(self.context.gcx).returns() else {
                    return report_error(
                        self.context.gcx,
                        callee.span,
                        "codegen expected Yul builtin to have a function type",
                    );
                };
                (true, returns.is_empty())
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => (false, true),
            _ => (false, false),
        };

        match (is_yul, is_void) {
            (true, true) => {
                let _ = self.lower_yul_unit_builtin_call(builtin, args);
                Some(self.builder.imm_u256(U256::ZERO))
            }
            (true, false) => Some(
                self.lower_yul_value_builtin_call(builtin, args)
                    .unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)),
            ),
            (false, true) => {
                let _ = self.lower_solidity_unit_builtin_call(builtin, args);
                Some(self.builder.imm_u256(U256::ZERO))
            }
            (false, false) => Some(
                self.lower_solidity_value_builtin_call(expr, builtin, args)
                    .unwrap_or_else(|| self.builder.imm_u256(U256::ZERO)),
            ),
        }
    }

    pub(super) fn lower_address_call(
        &mut self,
        call_span: Span,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
        capture_returndata: bool,
    ) -> Option<ValueId> {
        self.lower_address_call_result(
            call_span,
            receiver,
            builtin,
            args,
            call_opts,
            capture_returndata,
        )
        .map(|(success, _)| success)
    }

    pub(super) fn lower_address_call_result(
        &mut self,
        call_span: Span,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
        call_opts: Option<&hir::CallOptions<'_>>,
        capture_returndata: bool,
    ) -> Option<(ValueId, Option<ValueId>)> {
        // input = materialize_memory(arg)
        // ok = CALL|STATICCALL|DELEGATECALL(gas, to, value?, input.ptr, input.len, 0, 0)
        // data = capture ? materialize_returndata_bytes() : none
        let data = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let data_span = data.span;
        let data_ty = self.context.gcx.type_of_expr(data.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
        if capture_returndata && !self.context.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.context.gcx,
                call_span,
                "codegen cannot bind low-level call returndata before Byzantium",
            );
        }
        let (gas, value, zero) =
            self.lower_call_options(call_opts, builtin == Builtin::AddressCall, "call option")?;
        let data = self.lower_typed_expr(data, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_span)?;
        let input = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let input_size = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let success = match builtin {
            Builtin::AddressCall => {
                self.builder.call(gas, address, value, input, input_size, zero, zero)
            }
            Builtin::AddressStaticcall => {
                self.builder.staticcall(gas, address, input, input_size, zero, zero)
            }
            Builtin::AddressDelegatecall => {
                self.builder.delegatecall(gas, address, input, input_size, zero, zero)
            }
            _ => unreachable!(),
        };
        let returndata = capture_returndata.then(|| self.materialize_returndata_bytes());
        Some((success, returndata))
    }

    fn lower_payable_address_call(
        &mut self,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // gas = amount == 0 ? 2300 : 0
        // ok = CALL(gas, to, value=amount, 0, 0, 0, 0)
        // if transfer && !ok { revert_returndata() }
        // if send { return ok }
        let amount = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let amount = self.lower_typed_expr(amount, self.context.gcx.types.uint(256))?;
        let zero = self.builder.imm_u256(U256::ZERO);
        let stipend = self.builder.imm_u64(2300);
        let amount_is_zero = self.builder.iszero(amount);
        let gas = self.builder.select(amount_is_zero, stipend, zero);
        let success = self.builder.call(gas, address, amount, zero, zero, zero, zero);
        match builtin {
            Builtin::AddressPayableTransfer => {
                self.revert_external_call(success);
                Some(zero)
            }
            Builtin::AddressPayableSend => Some(success),
            _ => unreachable!(),
        }
    }

    pub(super) fn low_level_call_builtin(&self, expr: &hir::Expr<'_>) -> Option<Builtin> {
        match &expr.kind {
            ExprKind::Call(callee, ..) if matches!(callee.kind, ExprKind::Member(..)) => {
                match self.context.gcx.resolved_builtin(callee) {
                    Some(
                        builtin @ (Builtin::AddressCall
                        | Builtin::AddressStaticcall
                        | Builtin::AddressDelegatecall),
                    ) => Some(builtin),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(super) fn lower_low_level_call_values(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
        count: usize,
        first_is_omitted: bool,
    ) -> Option<Vec<ValueId>> {
        // ok = low_level_call(...)
        // if capture { data = materialize_returndata_bytes() }
        // values = [ok] | [ok, data] | [data]
        let ExprKind::Call(callee, args, call_opts) = &expr.kind else { return None };
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
        match (count <= 1 && !first_is_omitted, count, returndata) {
            (true, _, _) => Some(vec![success]),
            (false, 2, Some(returndata)) => Some(vec![success, returndata]),
            (false, _, Some(returndata)) => Some(vec![returndata]),
            (false, _, None) => {
                report_unsupported(self.context.gcx, expr.span, "low-level call return values")
            }
        }
    }

    pub(super) fn lower_builtin_value(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
    ) -> Option<ValueId> {
        // value = lower_builtin(expr)
        match builtin {
            Builtin::AddressBalance => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "address balance");
                };
                let receiver = self.lower_expr(receiver)?;
                Some(self.builder.balance(receiver))
            }
            Builtin::ArrayPop => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "array pop");
                };
                if self.storage_access(receiver).is_none() {
                    return report_unsupported(self.context.gcx, receiver.span, "storage access");
                }
                Some(self.builder.imm_u256(U256::ZERO))
            }
            Builtin::ContractCreationCode
            | Builtin::ContractRuntimeCode
            | Builtin::ContractName => {
                // if type(C).creationCode { value = bytes(creation_bytecode(C)) }
                // if type(C).runtimeCode { value = bytes(runtime_bytecode(C)) }
                // if type(C).name { value = string(contract_name(C)) }
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "environment builtin");
                };
                let TyKind::Meta(ty) = self.context.gcx.type_of_expr(receiver.id)?.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "creation code target");
                };
                let TyKind::Contract(contract_id) = ty.peel_refs().kind else {
                    return report_unsupported(self.context.gcx, expr.span, "creation code target");
                };
                match builtin {
                    Builtin::ContractName => {
                        let name = self.context.gcx.item_name(contract_id);
                        self.lower_bytes_literal(name.as_str().as_bytes())
                    }
                    Builtin::ContractCreationCode | Builtin::ContractRuntimeCode => {
                        let bytecodes = match builtin {
                            Builtin::ContractCreationCode => self.context.child_bytecodes,
                            Builtin::ContractRuntimeCode => self.context.child_runtime_bytecodes,
                            _ => unreachable!(),
                        };
                        match bytecodes.get(&contract_id) {
                            Some(bytecode) => self.lower_bytes_literal(bytecode),
                            None => {
                                let (kind, name) = match builtin {
                                    Builtin::ContractCreationCode => ("creation", "creationCode"),
                                    Builtin::ContractRuntimeCode => ("runtime", "runtimeCode"),
                                    _ => unreachable!(),
                                };
                                self.context
                                    .gcx
                                    .dcx()
                                    .err(format!("codegen is missing {kind} bytecode for `{name}`"))
                                    .span(expr.span)
                                    .note("the referenced contract did not compile or was not lowered first")
                                    .emit();
                                None
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Builtin::AddressCode | Builtin::AddressCodehash => {
                // if codehash { hash = extcodehash(to) }
                // if code {
                //     len = extcodesize(to)
                //     object = alloc_bytes(len)
                //     extcodecopy(to, object.data, 0, len)
                // }
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "environment builtin");
                };
                let address = self.lower_expr(receiver)?;
                match builtin {
                    Builtin::AddressCodehash => Some(self.builder.extcodehash(address)),
                    Builtin::AddressCode => {
                        let length = self.builder.extcodesize(address);
                        let object = self
                            .builder
                            .alloc_bytes_object(length, AllocationSemantics::SOLIDITY_ZEROED);
                        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
                        let zero = self.builder.imm_u256(U256::ZERO);
                        self.builder.extcodecopy_heap(address, data, zero, length);
                        Some(object)
                    }
                    _ => unreachable!(),
                }
            }
            Builtin::FunctionAddress => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "function address");
                };
                match self.is_external_function_value(receiver) {
                    true => {
                        let value = self.lower_expr(receiver)?;
                        Some(self.external_function_address(value))
                    }
                    false => report_unsupported(self.context.gcx, expr.span, "function address"),
                }
            }
            Builtin::FunctionSelector => {
                // if resolved_item { selector = imm(selector) << 224 }
                // if external_function_value { selector = (value & 0xffffffff) << 224 }
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "function selector");
                };
                let item = [expr, receiver].into_iter().find_map(|expr| {
                    self.context.gcx.resolved_expr(expr).and_then(|res| match res {
                        hir::Res::Item(
                            item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_)),
                        ) => Some(item),
                        _ => None,
                    })
                });
                match item {
                    Some(item) => {
                        self.lower_selector_receiver_effects(receiver)?;
                        let selector = self.context.gcx.function_selector(item).0;
                        Some(self.builder.imm_u256(U256::from_be_slice(&selector) << 224))
                    }
                    None => match self.is_external_function_value(receiver) {
                        true => {
                            let value = self.lower_expr(receiver)?;
                            let mask = self.builder.imm_u256(U256::from(u32::MAX));
                            let selector = self.builder.and(value, mask);
                            let shift = self.builder.imm_u64(224);
                            Some(self.builder.shl(shift, selector))
                        }
                        false => {
                            report_unsupported(self.context.gcx, expr.span, "function selector")
                        }
                    },
                }
            }
            Builtin::EventSelector => {
                let event_id = match self.context.gcx.resolved_expr(expr) {
                    Some(hir::Res::Item(hir::ItemId::Event(id))) => Some(id),
                    _ => match &expr.kind {
                        ExprKind::Member(receiver, _) => {
                            self.context.gcx.resolved_expr(receiver).and_then(|res| match res {
                                hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                                _ => None,
                            })
                        }
                        _ => None,
                    },
                };
                match event_id {
                    Some(event_id) => Some(self.builder.imm_u256(U256::from_be_slice(
                        self.context.gcx.event_selector(event_id).as_slice(),
                    ))),
                    None => report_unsupported(self.context.gcx, expr.span, "event selector"),
                }
            }
            Builtin::FixedBytesLength => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "fixed-bytes length");
                };
                let TyKind::Elementary(ElementaryType::FixedBytes(size)) =
                    self.context.gcx.type_of_expr(receiver.id)?.peel_refs().kind
                else {
                    return report_unsupported(self.context.gcx, expr.span, "fixed-bytes length");
                };
                match receiver.peel_parens().kind {
                    ExprKind::Ident(_) => {}
                    _ => {
                        self.lower_expr(receiver)?;
                    }
                }
                Some(self.builder.imm_u64(u64::from(size.bytes())))
            }
            Builtin::ArrayLength => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "array length");
                };
                match (&receiver.kind, self.context.gcx.resolved_builtin(receiver)) {
                    (ExprKind::Member(address, _), Some(Builtin::AddressCode)) => {
                        let address = self.lower_expr(address)?;
                        Some(self.builder.extcodesize(address))
                    }
                    _ => {
                        let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?;
                        self.lower_array_length(receiver, receiver_ty, expr.span, "array length")
                    }
                }
            }
            Builtin::TypeMin | Builtin::TypeMax | Builtin::InterfaceId => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "type member");
                };
                match builtin {
                    Builtin::InterfaceId => {
                        let TyKind::Meta(ty) = self.context.gcx.type_of_expr(receiver.id)?.kind
                        else {
                            return report_unsupported(self.context.gcx, expr.span, "interface id");
                        };
                        let TyKind::Contract(id) = ty.peel_refs().kind else {
                            return report_unsupported(self.context.gcx, expr.span, "interface id");
                        };
                        let value = self.context.gcx.interface_functions(id).own().iter().fold(
                            U256::ZERO,
                            |value, function| {
                                value ^ U256::from_be_slice(function.selector.as_slice())
                            },
                        ) << 224;
                        Some(self.builder.imm_u256(value))
                    }
                    Builtin::TypeMin | Builtin::TypeMax => {
                        let value = self.type_limit(
                            receiver,
                            expr.span,
                            matches!(builtin, Builtin::TypeMax),
                        )?;
                        Some(self.builder.imm_u256(value))
                    }
                    _ => unreachable!(),
                }
            }
            Builtin::This => Some(self.builder.address()),
            Builtin::BlockCoinbase => Some(self.builder.coinbase()),
            Builtin::BlockTimestamp => Some(self.builder.timestamp()),
            Builtin::BlockDifficulty | Builtin::BlockPrevrandao => Some(self.builder.prevrandao()),
            Builtin::BlockNumber => Some(self.builder.number()),
            Builtin::BlockGaslimit => Some(self.builder.gaslimit()),
            Builtin::BlockSlotnum => Some(self.builder.slotnum()),
            Builtin::BlockChainid => Some(self.builder.chainid()),
            Builtin::BlockBasefee => Some(self.builder.basefee()),
            Builtin::BlockBlobbasefee => Some(self.builder.blobbasefee()),
            Builtin::MsgSender => Some(self.builder.caller()),
            Builtin::MsgGas => Some(self.builder.gas()),
            Builtin::MsgValue => Some(self.builder.callvalue()),
            Builtin::MsgSig => {
                let offset = self.builder.imm_u64(0);
                let value = self.builder.calldataload(offset);
                let mask = self.builder.imm_u256(U256::MAX << 224);
                Some(self.builder.and(value, mask))
            }
            Builtin::MsgData => {
                let offset = self.builder.imm_u64(0);
                let length = self.builder.calldatasize();
                Some(self.builder.make_slice(offset, length, SliceLocation::Calldata))
            }
            Builtin::TxOrigin => Some(self.builder.origin()),
            Builtin::TxGasPrice => Some(self.builder.gasprice()),
            _ => report_unsupported(self.context.gcx, expr.span, "environment builtin"),
        }
    }

    pub(super) fn lower_selector_receiver_effects(
        &mut self,
        receiver: &hir::Expr<'_>,
    ) -> Option<()> {
        let receiver = receiver.peel_parens();
        match receiver.kind {
            ExprKind::Ident(_) | ExprKind::Type(_) => Some(()),
            ExprKind::Member(base, _)
                if matches!(base.peel_parens().kind, ExprKind::Ident(_) | ExprKind::Type(_)) =>
            {
                Some(())
            }
            ExprKind::Member(base, _) => self.lower_expr(base).map(|_| ()),
            _ => self.lower_expr(receiver).map(|_| ()),
        }
    }

    pub(super) fn type_limit(
        &self,
        receiver: &hir::Expr<'_>,
        span: Span,
        maximum: bool,
    ) -> Option<U256> {
        let ty = match self.context.gcx.type_of_expr(receiver.id)?.kind {
            TyKind::Meta(ty) => ty,
            _ => return report_unsupported(self.context.gcx, span, "type limit"),
        };
        match ty.peel_refs().kind {
            TyKind::Enum(id) => {
                let max = self.context.gcx.hir.enumm(id).variants.len().saturating_sub(1);
                Some(U256::from(match maximum {
                    true => max,
                    false => 0,
                }))
            }
            TyKind::Elementary(ElementaryType::UInt(size)) => {
                let max = (U256::from(1) << size.bits()) - U256::from(1);
                Some(match maximum {
                    true => max,
                    false => U256::ZERO,
                })
            }
            TyKind::Elementary(ElementaryType::Int(size)) => {
                let magnitude = U256::from(1) << (size.bits() - 1);
                Some(match maximum {
                    true => magnitude - U256::from(1),
                    false => U256::MAX - magnitude + U256::from(1),
                })
            }
            _ => report_unsupported(self.context.gcx, span, "type limit"),
        }
    }

    pub(super) fn external_function_address(&mut self, value: ValueId) -> ValueId {
        let shift = self.builder.imm_u64(32);
        let address = self.builder.shr(shift, value);
        let mask = self.builder.imm_u256(U256::MAX >> 96);
        self.builder.and(address, mask)
    }

    pub(super) fn is_external_function_value(&self, expr: &hir::Expr<'_>) -> bool {
        matches!(
            self.type_of_expr_or_variable(expr).map(|ty| ty.kind),
            Some(TyKind::Fn(function)) if function.is_external()
        )
    }

    fn lower_solidity_unit_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<()> {
        // if assert && !condition { panic(Assert) }
        // if require && !condition { revert(payload_or_empty) }
        // if revert() { revert(0, 0) }
        // if revert(message) { revert(payload) }
        // selfdestruct(address)
        match builtin {
            Builtin::Assert => {
                let condition = &self.builtin_args::<1>(builtin, &args)?[0];
                let condition = self.lower_expr(condition)?;
                let invalid = self.builder.iszero(condition);
                self.builder.panic_if(invalid, PanicCode::Assert);
            }
            Builtin::Require => {
                let (required, message) = self.builtin_args_with_optional::<1>(builtin, &args)?;
                let condition = required.first()?;
                let condition = self.lower_expr(condition)?;
                let message = match message {
                    Some(message) => Some(self.prepare_revert_payload(message)?),
                    None => None,
                };
                let is_false = self.builder.iszero(condition);
                let revert_block = self.builder.create_block();
                let continue_block = self.builder.create_block();
                self.builder.branch(is_false, revert_block, continue_block);
                self.builder.switch_to_block(revert_block);
                match message {
                    Some(message) => self.emit_revert_payload(message),
                    None => {
                        let zero = self.builder.imm_u256(U256::ZERO);
                        self.builder.revert(zero, zero);
                    }
                }
                self.builder.switch_to_block(continue_block);
            }
            Builtin::Revert => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
            }
            Builtin::RevertMsg => {
                let message = &self.builtin_args::<1>(builtin, &args)?[0];
                self.lower_revert_payload(message)?;
            }
            Builtin::Selfdestruct => {
                let address = &self.builtin_args::<1>(builtin, &args)?[0];
                let address = self.lower_expr(address)?;
                self.builder.selfdestruct(address);
            }
            _ => {
                return report_error(
                    self.context.gcx,
                    args.span,
                    "codegen routed a value Solidity builtin through unit lowering",
                );
            }
        }
        Some(())
    }

    fn lower_solidity_value_builtin_call(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // value = builtin(args...)
        match builtin {
            Builtin::Keccak256 => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                if let ExprKind::Lit(lit) = self.peel_bytes_conversion(value).peel_parens().kind
                    && let LitKind::Str(_, bytes, _) = &lit.kind
                {
                    let hash = keccak256(bytes.as_byte_str());
                    return Some(self.builder.imm_u256(U256::from_be_slice(hash.as_slice())));
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.context.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncodePacked)
                    && let Some(hash) = self.lower_keccak_abi_encode_packed(*encode_args)
                {
                    return Some(hash);
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.context.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncode)
                {
                    let exprs = self.variadic_builtin_args(Builtin::AbiEncode, encode_args)?;
                    let encoded = self.lower_abi_encode_scratch(exprs, None)?;
                    let pointer = self.builder.slice_ptr(encoded);
                    let length = self.builder.slice_len(encoded);
                    return Some(self.builder.keccak256(pointer, length));
                }
                let value_ty = self.context.gcx.type_of_expr(value.id)?;
                let memory_ty = value_ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
                let span = value.span;
                let value = self.lower_typed_expr(value, memory_ty)?;
                let value = self.materialize_memory_argument(memory_ty, value, span)?;
                Some(self.builder.keccak256_bytes(value))
            }
            Builtin::Gasleft => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                Some(self.builder.gas())
            }
            Builtin::AbiEncode => self.lower_abi_encode_builtin(args, None),
            Builtin::AbiEncodeWithSelector => {
                let (selector, rest) = self.builtin_args_with_rest::<1>(builtin, &args)?;
                let selector = self.lower_selector_word(&selector[0])?;
                self.lower_abi_encode_builtin_args(rest, Some(selector))
            }
            Builtin::AbiEncodePacked => self.lower_abi_encode_packed(args),
            Builtin::AbiEncodeWithSignature => self.lower_abi_encode_with_signature(args),
            Builtin::AbiEncodeCall => self.lower_abi_encode_call(args),
            Builtin::AbiDecode => self.lower_abi_decode(args),
            Builtin::Blockhash | Builtin::Blobhash => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                let value = self.lower_expr(value)?;
                Some(match builtin {
                    Builtin::Blockhash => self.builder.blockhash(value),
                    Builtin::Blobhash => self.builder.blobhash(value),
                    _ => unreachable!(),
                })
            }
            Builtin::AddMod | Builtin::MulMod => {
                let [a, b, modulus] = self.lower_builtin_args(builtin, &args)?;
                self.builder.panic_if_zero(modulus, PanicCode::DivisionByZero);
                Some(match builtin {
                    Builtin::AddMod => self.builder.addmod(a, b, modulus),
                    Builtin::MulMod => self.builder.mulmod(a, b, modulus),
                    _ => unreachable!(),
                })
            }
            Builtin::Erc7201 => self.lower_erc7201(args),
            Builtin::Sha256 | Builtin::Ripemd160 => self.lower_hash_precompile_call(builtin, args),
            Builtin::EcRecover => self.lower_ecrecover_call(args),
            Builtin::StringConcat | Builtin::BytesConcat => {
                self.lower_concat_builtin_call(builtin, args)
            }
            Builtin::UdvtWrap => {
                let value = self.builtin_args::<1>(builtin, &args)?.first()?;
                let TyKind::Udvt(underlying, _) = self.context.gcx.type_of_expr(expr.id)?.kind
                else {
                    return report_error(
                        self.context.gcx,
                        expr.span,
                        "codegen expected UDVT wrap to return a user-defined value type",
                    );
                };
                self.lower_typed_expr(value, underlying)
            }
            Builtin::UdvtUnwrap => {
                let value = self.builtin_args::<1>(builtin, &args)?.first()?;
                self.lower_expr(value)
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => report_error(
                self.context.gcx,
                args.span,
                "codegen routed a unit builtin through value lowering",
            ),
            _ => {
                if self.validate_builtin_arity(builtin, &args) {
                    self.unsupported_builtin(builtin, args.span)
                } else {
                    None
                }
            }
        }
    }

    fn lower_erc7201(&mut self, args: hir::CallArgs<'_>) -> Option<ValueId> {
        // inner = keccak256(bytes(argument)) - 1
        // outer = keccak256(abi.encode(inner)) & ~0xff
        let argument = &self.builtin_args::<1>(Builtin::Erc7201, &args)?[0];
        let literal = match &argument.kind {
            ExprKind::Lit(lit) => match &lit.kind {
                LitKind::Str(_, bytes, _) => Some(bytes.as_byte_str()),
                _ => None,
            },
            _ => None,
        };
        let inner = match literal {
            Some(bytes) => self.builder.imm_u256(U256::from_be_slice(keccak256(bytes).as_slice())),
            None => {
                let argument_ty = self.context.gcx.type_of_expr(argument.id)?;
                let memory_ty = argument_ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
                let value = self.lower_typed_expr(argument, memory_ty)?;
                let value = self.materialize_memory_argument(memory_ty, value, argument.span)?;
                self.builder.keccak256_bytes(value)
            }
        };
        let one = self.builder.imm_u256(U256::from(1));
        let inner = self.builder.sub(inner, one);
        let zero = self.builder.imm_u256(U256::ZERO);
        let word_size = self.builder.imm_u64(32);
        let size =
            self.builder.imm_u64(EvmMemoryLayout::DYNAMIC_HEADER_SIZE + EvmMemoryLayout::WORD_SIZE);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, word_size, MemoryObjectKind::Bytes);
        self.builder.memory_object_store_word(object, zero, inner);
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let outer = self.builder.keccak256(data, word_size);
        let mask = self.builder.imm_u256(!U256::from(0xff));
        Some(self.builder.and(outer, mask))
    }

    fn lower_concat_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // if all_literals { output = bytes(concat(literals)) }
        // else { output = alloc_bytes(total) }
        // for part { copy(literal | dynamic | fixed, output, offset) }
        enum Part {
            Literal(Vec<u8>),
            Dynamic { value: ValueId, length: ValueId },
            Fixed { value: ValueId, length: u64 },
        }

        let exprs = self.variadic_builtin_args(builtin, &args)?;
        let mut all_literals = Some(Vec::new());
        let mut total = self.builder.imm_u64(0);
        let mut parts = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.context.gcx.type_of_expr(expr.id)?;
            match ty.peel_refs().kind {
                TyKind::StringLiteral(..)
                | TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes)
                | TyKind::Slice(_) => {
                    if let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind
                        && let LitKind::Str(_, bytes, _) = &lit.kind
                    {
                        let bytes = bytes.as_byte_str().to_vec();
                        if let Some(all_literals) = &mut all_literals {
                            all_literals.extend_from_slice(&bytes);
                        } else {
                            let length = self.builder.imm_u64(bytes.len() as u64);
                            total = self.builder.add(total, length);
                        }
                        parts.push(Part::Literal(bytes));
                        continue;
                    }
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm_u64(all_literals.len() as u64);
                        total = self.builder.add(total, length);
                    }
                    let memory_ty = ty.with_loc_if_ref(self.context.gcx, DataLocation::Memory);
                    let value = self.lower_typed_expr(expr, memory_ty)?;
                    let value = self.materialize_memory_argument(memory_ty, value, expr.span)?;
                    let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                    total = self.builder.add(total, length);
                    parts.push(Part::Dynamic { value, length });
                }
                TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm_u64(all_literals.len() as u64);
                        total = self.builder.add(total, length);
                    }
                    let value = self.lower_expr(expr)?;
                    let length = u64::from(size.bytes());
                    let length_value = self.builder.imm_u64(length);
                    total = self.builder.add(total, length_value);
                    parts.push(Part::Fixed { value, length });
                }
                _ => return report_unsupported(self.context.gcx, expr.span, "concat argument"),
            }
        }

        if let Some(bytes) = all_literals {
            return Self::build_bytes_literal(
                &mut self.builder,
                &bytes,
                AllocationSemantics::SOLIDITY_UNINITIALIZED,
            );
        }

        let size = self.builder.padded_size(total);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        self.builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);

        let mut offset = self.builder.imm_u64(0);
        for part in parts {
            match part {
                Part::Literal(bytes) => {
                    for chunk in bytes.chunks(32) {
                        let value = self.lower_string_literal_word(chunk);
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm_u64(chunk.len() as u64);
                        offset = self.builder.add(offset, length);
                    }
                }
                Part::Dynamic { value, length } => {
                    let source_ptr =
                        self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                    let source = self.builder.make_slice(source_ptr, length, SliceLocation::Memory);
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        source,
                    );
                    offset = self.builder.add(offset, length);
                }
                Part::Fixed { value, length } => {
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm_u64(length);
                    offset = self.builder.add(offset, length);
                }
            }
        }
        Some(output)
    }

    fn lower_yul_unit_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<()> {
        // mstore|mstore8|mcopy(...)
        // sstore|tstore(...)
        // calldatacopy|codecopy|extcodecopy|returndatacopy(...)
        // logN(...), N = 0..4
        // return|revert|stop|invalid|selfdestruct(...)
        // pop(args) = evaluate(args); discard
        macro_rules! lower {
            ($method:ident($($arg:ident),* $(,)?)) => {{
                let [$($arg),*] = self.lower_builtin_args(builtin, &args)?;
                self.builder.$method($($arg),*);
                Some(())
            }};
        }
        match builtin {
            Builtin::YulMstore => lower!(mstore(offset, value)),
            Builtin::YulMstore8 => lower!(mstore8(offset, value)),
            Builtin::YulMcopy => lower!(mcopy(dest, src, size)),
            Builtin::YulSstore => lower!(sstore(slot, value)),
            Builtin::YulTstore => lower!(tstore(slot, value)),
            Builtin::YulCalldatacopy => lower!(calldatacopy(dest, src, size)),
            Builtin::YulCodecopy => lower!(codecopy(dest, src, size)),
            Builtin::YulExtcodecopy => lower!(extcodecopy(address, dest, src, size)),
            Builtin::YulReturndatacopy => lower!(returndatacopy(dest, src, size)),
            Builtin::YulLog0 => lower!(log0(offset, size)),
            Builtin::YulLog1 => lower!(log1(offset, size, topic1)),
            Builtin::YulLog2 => lower!(log2(offset, size, topic1, topic2)),
            Builtin::YulLog3 => lower!(log3(offset, size, topic1, topic2, topic3)),
            Builtin::YulLog4 => lower!(log4(offset, size, topic1, topic2, topic3, topic4)),
            Builtin::YulRevert => lower!(revert(offset, size)),
            Builtin::YulReturn => lower!(ret_data(offset, size)),
            Builtin::YulStop => lower!(stop()),
            Builtin::YulInvalid => lower!(invalid()),
            Builtin::YulSelfdestruct => lower!(selfdestruct(address)),
            Builtin::YulPop => {
                let [_value] = self.lower_builtin_args(builtin, &args)?;
                Some(())
            }
            _ => report_error(
                self.context.gcx,
                args.span,
                "codegen routed a value Yul builtin through unit lowering",
            ),
        }
    }

    fn lower_yul_value_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        // value = lower_yul_expr(args...)
        macro_rules! lower {
            ($method:ident($($arg:ident),* $(,)?)) => {{
                let [$($arg),*] = self.lower_builtin_args(builtin, &args)?;
                Some(self.builder.$method($($arg),*))
            }};
        }
        match builtin {
            Builtin::YulAdd => lower!(add(lhs, rhs)),
            Builtin::YulSub => lower!(sub(lhs, rhs)),
            Builtin::YulMul => lower!(mul(lhs, rhs)),
            Builtin::YulDiv => lower!(div(lhs, rhs)),
            Builtin::YulSdiv => lower!(sdiv(lhs, rhs)),
            Builtin::YulMod => lower!(mod_(lhs, rhs)),
            Builtin::YulSmod => lower!(smod(lhs, rhs)),
            Builtin::YulExp => lower!(exp(base, exponent)),
            Builtin::YulSignextend => lower!(signextend(byte, value)),
            Builtin::YulEq => lower!(eq(lhs, rhs)),
            Builtin::YulLt => lower!(lt(lhs, rhs)),
            Builtin::YulGt => lower!(gt(lhs, rhs)),
            Builtin::YulSlt => lower!(slt(lhs, rhs)),
            Builtin::YulSgt => lower!(sgt(lhs, rhs)),
            Builtin::YulAnd => lower!(and(lhs, rhs)),
            Builtin::YulOr => lower!(or(lhs, rhs)),
            Builtin::YulXor => lower!(xor(lhs, rhs)),
            Builtin::YulNot => lower!(not(value)),
            Builtin::YulByte => lower!(byte(index, value)),
            Builtin::YulShl => lower!(shl(shift, value)),
            Builtin::YulShr => lower!(shr(shift, value)),
            Builtin::YulSar => lower!(sar(shift, value)),
            Builtin::YulIszero => lower!(iszero(value)),
            Builtin::YulAddmod => lower!(addmod(a, b, modulus)),
            Builtin::YulMulmod => lower!(mulmod(a, b, modulus)),
            Builtin::YulClz => lower!(clz(value)),
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
            Builtin::YulSlotnum => lower!(slotnum()),
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
            Builtin::YulExtcall | Builtin::YulExtdelegatecall | Builtin::YulExtstaticcall => self
                .unsupported_yul_version(
                    "codegen cannot emit EOF-only external calls in legacy bytecode",
                    "remove the EOF-only call or use a compiler that emits EOF containers",
                    args.span,
                ),
            _ => report_error(
                self.context.gcx,
                args.span,
                "codegen routed a unit Yul builtin through value lowering",
            ),
        }
    }

    fn emit_wrong_builtin_arg_count(
        &self,
        builtin: Builtin,
        span: Span,
        expected: BuiltinArgCount,
        actual: usize,
    ) {
        let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
        let expected = expected.description();
        self.context
            .gcx
            .dcx()
            .err(format!(
                "wrong number of arguments for {kind} `{}`: expected {expected}, found {actual}",
                builtin.name()
            ))
            .span(span)
            .emit();
    }

    pub(super) fn builtin_arg_exprs<'hir>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => Some(exprs),
            hir::CallArgsKind::Named(_) => {
                let kind = if builtin.is_yul() { "Yul builtin" } else { "builtin" };
                self.context
                    .gcx
                    .dcx()
                    .err(format!(
                        "named arguments are not supported for {kind} `{}` in codegen",
                        builtin.name()
                    ))
                    .span(args.span)
                    .emit();
                None
            }
        }
    }

    pub(super) fn builtin_args<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if exprs.len() == N {
            return Some(exprs);
        }
        self.emit_wrong_builtin_arg_count(
            builtin,
            args.span,
            BuiltinArgCount::Exact(N),
            exprs.len(),
        );
        None
    }

    pub(super) fn builtin_args_with_rest<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<(&'hir [hir::Expr<'hir>], &'hir [hir::Expr<'hir>])> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if exprs.len() < N {
            self.emit_wrong_builtin_arg_count(
                builtin,
                args.span,
                BuiltinArgCount::AtLeast(N),
                exprs.len(),
            );
            return None;
        }
        Some(exprs.split_at(N))
    }

    pub(super) fn builtin_args_with_optional<'hir, const N: usize>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<(&'hir [hir::Expr<'hir>], Option<&'hir hir::Expr<'hir>>)> {
        let exprs = self.builtin_arg_exprs(builtin, args)?;
        if (N..=N + 1).contains(&exprs.len()) {
            let (required, optional) = exprs.split_at(N);
            return Some((required, optional.first()));
        }
        self.emit_wrong_builtin_arg_count(
            builtin,
            args.span,
            BuiltinArgCount::Between(N, N + 1),
            exprs.len(),
        );
        None
    }

    pub(super) fn variadic_builtin_args<'hir>(
        &self,
        builtin: Builtin,
        args: &hir::CallArgs<'hir>,
    ) -> Option<&'hir [hir::Expr<'hir>]> {
        self.builtin_arg_exprs(builtin, args)
    }

    fn validate_builtin_arity(&self, builtin: Builtin, args: &hir::CallArgs<'_>) -> bool {
        let Some(exprs) = self.builtin_arg_exprs(builtin, args) else {
            return false;
        };
        let TyKind::Fn(function) = builtin.ty(self.context.gcx).kind else {
            return true;
        };
        let variadic =
            function.parameters.last().is_some_and(|ty| matches!(ty.kind, TyKind::Variadic));
        let (valid, expected) = if variadic {
            let minimum = function.parameters.len().saturating_sub(1);
            (exprs.len() >= minimum, BuiltinArgCount::AtLeast(minimum))
        } else {
            let expected = function.parameters.len();
            (exprs.len() == expected, BuiltinArgCount::Exact(expected))
        };
        if !valid {
            self.emit_wrong_builtin_arg_count(builtin, args.span, expected, exprs.len());
        }
        valid
    }

    pub(super) fn lower_builtin_args<const N: usize>(
        &mut self,
        builtin: Builtin,
        args: &hir::CallArgs<'_>,
    ) -> Option<[ValueId; N]> {
        let exprs = self.builtin_args::<N>(builtin, args)?;
        let mut values = [None; N];
        if builtin.is_yul() {
            for index in (0..N).rev() {
                values[index] = Some(self.lower_yul_word_expr(&exprs[index])?);
            }
        } else {
            // Solidity builtins convert every argument to the declared parameter type, like
            // solc's `expressionAsType`. A narrow local dirtied by inline assembly is cleaned
            // here, before any semantic check such as the zero-modulus panic observes it.
            let parameters = match builtin.ty(self.context.gcx).kind {
                TyKind::Fn(function) => function.parameters,
                _ => &[],
            };
            for index in 0..N {
                let expr = &exprs[index];
                values[index] = Some(match parameters.get(index) {
                    Some(&parameter) => self.lower_typed_expr(expr, parameter)?,
                    None => self.lower_yul_word_expr(expr)?,
                });
            }
        }
        Some(values.map(|value| value.expect("all builtin arguments lowered")))
    }

    fn unsupported_builtin<T>(&self, builtin: Builtin, span: Span) -> Option<T> {
        self.context
            .gcx
            .dcx()
            .err(format!("unsupported builtin call `{}`", builtin.name()))
            .span(span)
            .emit();
        None
    }

    fn unsupported_yul_version<T>(
        &self,
        message: &'static str,
        help: &'static str,
        span: Span,
    ) -> Option<T> {
        self.context.gcx.dcx().err(message).span(span).help(help).emit();
        None
    }
}
