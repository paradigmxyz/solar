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
        match builtin {
            Builtin::AddressCall | Builtin::AddressStaticcall | Builtin::AddressDelegatecall => {
                // result = address_call(receiver, args, opts)
                let ExprKind::Member(receiver, _) = callee.kind else {
                    return self.cx.report_unsupported(callee.span, "address call");
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
                // result = payable_address_call(receiver, args)
                let ExprKind::Member(receiver, _) = callee.kind else {
                    return self.cx.report_unsupported(callee.span, "address call");
                };
                return self.lower_payable_address_call(receiver, builtin, args);
            }
            Builtin::ArrayPush => {
                // result = storage_array_push(receiver, value)
                let result = self.builtin_args::<1>(builtin, &args).and_then(|arguments| {
                    self.lower_storage_array_push(expr, callee, arguments.first())
                });
                return Some(result.unwrap_or_else(|| self.builder.imm(U256::ZERO)));
            }
            Builtin::ArrayPush0 => {
                // result = storage_array_push(receiver)
                let result = self
                    .builtin_args::<0>(builtin, &args)
                    .and_then(|_| self.lower_storage_array_push(expr, callee, None));
                return Some(result.unwrap_or_else(|| self.builder.imm(U256::ZERO)));
            }
            Builtin::ArrayPop => {
                // storage_array_pop(receiver)
                let result = self
                    .builtin_args::<0>(builtin, &args)
                    .and_then(|_| self.lower_storage_array_pop(expr, callee));
                return Some(result.unwrap_or_else(|| self.builder.imm(U256::ZERO)));
            }
            _ => {}
        }

        let (is_yul, is_void) = match builtin {
            builtin if builtin.is_yul() => {
                let Some(returns) = builtin.ty(self.cx.gcx).returns() else {
                    return report_error(
                        self.cx.gcx,
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

        // result = lower_builtin((yul | solidity), (void | value), args)
        match (is_yul, is_void) {
            (true, true) => {
                let _ = self.lower_yul_unit_builtin_call(builtin, args);
                Some(self.builder.imm(U256::ZERO))
            }
            (true, false) => Some(
                self.lower_yul_value_builtin_call(builtin, args)
                    .unwrap_or_else(|| self.builder.imm(U256::ZERO)),
            ),
            (false, true) => {
                let _ = self.lower_solidity_unit_builtin_call(builtin, args);
                Some(self.builder.imm(U256::ZERO))
            }
            (false, false) => Some(
                self.lower_solidity_value_builtin_call(expr, builtin, args)
                    .unwrap_or_else(|| self.builder.imm(U256::ZERO)),
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
        let data = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let data_span = data.span;
        let data_ty = self.cx.gcx.type_of_expr(data.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        if capture_returndata && !self.cx.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.cx.gcx,
                call_span,
                "codegen cannot bind low-level call returndata before Byzantium",
            );
        }
        let options =
            self.lower_call_options(call_opts, builtin == Builtin::AddressCall, "call option")?;
        let (value, zero) = (options.value, options.zero);
        // input = materialize_memory(arg)
        let data = self.lower_typed_expr(data, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_span)?;
        let input = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let input_size = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        // A bare call has no `extcodesize` guard, so before EIP-150 it also reserves the cost of
        // creating the callee's account, which is unknowable here; solc reserves it for all three
        // kinds too (`appendBareCall`).
        // gas = gas() | sub(gas(), reserve)
        let gas = self.call_gas(options.gas, options.value_set, true);
        // ok = call|staticcall|delegatecall(gas, to, value?, input, 0, 0)
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
        // data = capture ? returndata() : none
        let returndata = capture_returndata.then(|| self.materialize_returndata_bytes());
        Some((success, returndata))
    }

    fn lower_payable_address_call(
        &mut self,
        receiver: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let amount = &self.builtin_args::<1>(builtin, &args)?[0];
        let address = self.lower_expr(receiver)?;
        let amount = self.lower_typed_expr(amount, self.cx.gcx.types.uint(256))?;
        let zero = self.builder.imm(U256::ZERO);
        let stipend = self.builder.imm(2300);
        let amount_is_zero = self.builder.iszero(amount);
        // gas = amount == 0 ? 2300 : 0
        let gas = self.builder.select(amount_is_zero, stipend, zero);
        // ok = call(gas, to, amount, 0, 0, 0, 0)
        let success = self.builder.call(gas, address, amount, zero, zero, zero, zero);
        match builtin {
            Builtin::AddressPayableTransfer => {
                // if !ok { revert(0, returndatasize()) }
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
                match self.cx.gcx.resolved_builtin(callee) {
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
        let ExprKind::Call(callee, args, call_opts) = &expr.kind else { return None };
        let ExprKind::Member(receiver, _) = callee.kind else { return None };
        let capture_returndata = count > 1 || first_is_omitted;
        // ok, data? = low_level_call(...)
        let (success, returndata) = self.lower_address_call_result(
            callee.span,
            receiver,
            builtin,
            *args,
            *call_opts,
            capture_returndata,
        )?;
        // values = [ok] | [ok, data] | [data]
        match (count <= 1 && !first_is_omitted, count, returndata) {
            (true, _, _) => Some(vec![success]),
            (false, 2, Some(returndata)) => Some(vec![success, returndata]),
            (false, _, Some(returndata)) => Some(vec![returndata]),
            (false, _, None) => {
                self.cx.report_unsupported(expr.span, "low-level call return value list")
            }
        }
    }

    pub(super) fn lower_builtin_value(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
    ) -> Option<ValueId> {
        match builtin {
            Builtin::AddressBalance => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "address balance");
                };
                let receiver = self.lower_expr(receiver)?;
                Some(self.builder.balance(receiver))
            }
            Builtin::ArrayPop => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "array pop");
                };
                if self.storage_access(receiver).is_none() {
                    return self.cx.report_unsupported(receiver.span, "storage access");
                }
                Some(self.builder.imm(U256::ZERO))
            }
            Builtin::ContractCreationCode
            | Builtin::ContractRuntimeCode
            | Builtin::ContractName => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "environment builtin");
                };
                let TyKind::Meta(ty) = self.cx.gcx.type_of_expr(receiver.id)?.kind else {
                    return self.cx.report_unsupported(expr.span, "creation code target");
                };
                let TyKind::Contract(contract_id) = ty.peel_refs().kind else {
                    return self.cx.report_unsupported(expr.span, "creation code target");
                };
                match builtin {
                    Builtin::ContractName => {
                        // value = string(contract_name(C))
                        let name = self.cx.gcx.item_name(contract_id);
                        self.lower_bytes_literal(name.as_str().as_bytes())
                    }
                    Builtin::ContractCreationCode | Builtin::ContractRuntimeCode => {
                        // value = bytes(creation_bytecode(C) | runtime_bytecode(C))
                        let creation = builtin == Builtin::ContractCreationCode;
                        let bytecode =
                            self.cx.child_bytecodes.get(&contract_id).and_then(|bytecodes| {
                                if creation { bytecodes.deployment() } else { bytecodes.runtime() }
                            });
                        match bytecode {
                            Some(bytecode) => Self::build_bytes_literal(
                                self.cx.gcx,
                                self.cx.module,
                                &mut self.builder,
                                bytecode,
                                AllocationSemantics::INTERNAL,
                                Some(super::super::data::contract_bytecode_data_name(
                                    self.cx.gcx,
                                    contract_id,
                                    creation,
                                )),
                            ),
                            None => {
                                let (kind, name) = match builtin {
                                    Builtin::ContractCreationCode => ("creation", "creationCode"),
                                    Builtin::ContractRuntimeCode => ("runtime", "runtimeCode"),
                                    _ => unreachable!(),
                                };
                                self.cx
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
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "environment builtin");
                };
                let address = self.lower_expr(receiver)?;
                match builtin {
                    Builtin::AddressCodehash => {
                        // value = extcodehash(address)
                        Some(self.builder.extcodehash(address))
                    }
                    Builtin::AddressCode => {
                        // length = extcodesize(address)
                        // object = alloc_bytes(length)
                        // extcodecopy(address, object.data, 0, length)
                        let length = self.builder.extcodesize(address);
                        let object = self
                            .builder
                            .alloc_bytes_object(length, AllocationSemantics::SOLIDITY_ZEROED);
                        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
                        let zero = self.builder.imm(U256::ZERO);
                        self.builder.extcodecopy_heap(address, data, zero, length);
                        Some(object)
                    }
                    _ => unreachable!(),
                }
            }
            Builtin::FunctionAddress => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "function address");
                };
                match self.is_external_function_value(receiver) {
                    true => {
                        let value = self.lower_expr(receiver)?;
                        Some(self.external_function_address(value))
                    }
                    false => self.cx.report_unsupported(expr.span, "function address"),
                }
            }
            Builtin::FunctionSelector => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "function selector");
                };
                let item = [expr, receiver].into_iter().find_map(|expr| {
                    self.cx.gcx.resolved_expr(expr).and_then(|res| match res {
                        hir::Res::Item(
                            item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_)),
                        ) => Some(item),
                        _ => None,
                    })
                });
                match item {
                    Some(item) => {
                        // selector = selector(item) << 224
                        self.lower_selector_receiver_effects(receiver)?;
                        let selector = self.cx.gcx.function_selector(item).0;
                        Some(self.builder.imm(U256::from_be_slice(&selector) << 224))
                    }
                    None => match self.is_external_function_value(receiver) {
                        true => {
                            // selector = (function & 0xffffffff) << 224
                            let value = self.lower_expr(receiver)?;
                            let mask = self.builder.imm(u32::MAX);
                            let selector = self.builder.and(value, mask);
                            let shift = self.builder.imm(224);
                            Some(self.builder.shl(shift, selector))
                        }
                        false => self.cx.report_unsupported(expr.span, "function selector"),
                    },
                }
            }
            Builtin::EventSelector => {
                let event_id = match self.cx.gcx.resolved_expr(expr) {
                    Some(hir::Res::Item(hir::ItemId::Event(id))) => Some(id),
                    _ => match &expr.kind {
                        ExprKind::Member(receiver, _) => {
                            self.cx.gcx.resolved_expr(receiver).and_then(|res| match res {
                                hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                                _ => None,
                            })
                        }
                        _ => None,
                    },
                };
                match event_id {
                    Some(event_id) => {
                        Some(self.builder.imm(U256::from_be_slice(
                            self.cx.gcx.event_selector(event_id).as_slice(),
                        )))
                    }
                    None => self.cx.report_unsupported(expr.span, "event selector"),
                }
            }
            Builtin::FixedBytesLength => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "fixed-bytes length");
                };
                let TyKind::Elementary(ElementaryType::FixedBytes(size)) =
                    self.cx.gcx.type_of_expr(receiver.id)?.peel_refs().kind
                else {
                    return self.cx.report_unsupported(expr.span, "fixed-bytes length");
                };
                match receiver.peel_parens().kind {
                    ExprKind::Ident(_) => {}
                    _ => {
                        self.lower_expr(receiver)?;
                    }
                }
                Some(self.builder.imm(u64::from(size.bytes())))
            }
            Builtin::ArrayLength => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "array length");
                };
                match (&receiver.kind, self.cx.gcx.resolved_builtin(receiver)) {
                    (ExprKind::Member(address, _), Some(Builtin::AddressCode)) => {
                        let address = self.lower_expr(address)?;
                        Some(self.builder.extcodesize(address))
                    }
                    _ => {
                        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?;
                        self.lower_array_length(receiver, receiver_ty, expr.span, "array length")
                    }
                }
            }
            Builtin::TypeMin | Builtin::TypeMax | Builtin::InterfaceId => {
                let ExprKind::Member(receiver, _) = &expr.kind else {
                    return self.cx.report_unsupported(expr.span, "type member");
                };
                match builtin {
                    Builtin::InterfaceId => {
                        let TyKind::Meta(ty) = self.cx.gcx.type_of_expr(receiver.id)?.kind else {
                            return self.cx.report_unsupported(expr.span, "interface id");
                        };
                        let TyKind::Contract(id) = ty.peel_refs().kind else {
                            return self.cx.report_unsupported(expr.span, "interface id");
                        };
                        let value = U256::from_be_slice(&self.cx.gcx.interface_id(id).0) << 224;
                        Some(self.builder.imm(value))
                    }
                    Builtin::TypeMin | Builtin::TypeMax => {
                        let value = self.type_limit(
                            receiver,
                            expr.span,
                            matches!(builtin, Builtin::TypeMax),
                        )?;
                        Some(self.builder.imm(value))
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
                let offset = self.builder.imm(0);
                let value = self.builder.calldataload(offset);
                let mask = self.builder.imm(U256::MAX << 224);
                Some(self.builder.and(value, mask))
            }
            Builtin::MsgData => {
                let offset = self.builder.imm(0);
                let length = self.builder.calldatasize();
                Some(self.builder.make_slice(offset, length, SliceLocation::Calldata))
            }
            Builtin::TxOrigin => Some(self.builder.origin()),
            Builtin::TxGasPrice => Some(self.builder.gasprice()),
            _ => self.cx.report_unsupported(expr.span, "environment builtin"),
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
        let ty = match self.cx.gcx.type_of_expr(receiver.id)?.kind {
            TyKind::Meta(ty) => ty,
            _ => return self.cx.report_unsupported(span, "type limit"),
        };
        match ty.peel_refs().kind {
            TyKind::Enum(id) => {
                let max = self.cx.gcx.hir.enumm(id).variants.len().saturating_sub(1);
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
            _ => self.cx.report_unsupported(span, "type limit"),
        }
    }

    pub(super) fn external_function_address(&mut self, value: ValueId) -> ValueId {
        let shift = self.builder.imm(32);
        let address = self.builder.shr(shift, value);
        let mask = self.builder.imm(U256::MAX >> 96);
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
                // The message is evaluated regardless of the condition, like a function call
                // argument. With `--revert-strings strip`, only its side effects are kept.
                let message = match message {
                    Some(message) if self.strips_revert_string(message)? => None,
                    Some(message) => Some(self.prepare_revert_payload(message)?),
                    None => None,
                };
                let is_false = self.builder.iszero(condition);
                let Some(message) = message else {
                    self.builder.revert_if(is_false, RevertReason::Empty);
                    return Some(());
                };
                let revert_block = self.builder.create_block();
                let continue_block = self.builder.create_block();
                self.builder.branch(is_false, revert_block, continue_block);
                self.builder.switch_to_block(revert_block);
                self.emit_revert_payload(message);
                self.builder.switch_to_block(continue_block);
            }
            Builtin::Revert => {
                let _ = self.builtin_args::<0>(builtin, &args)?;
                // revert(0, 0)
                self.builder.revert_with(RevertReason::Empty);
            }
            Builtin::RevertMsg => {
                let message = &self.builtin_args::<1>(builtin, &args)?[0];
                if self.strips_revert_string(message)? {
                    // revert(0, 0)
                    self.builder.revert_with(RevertReason::Empty);
                } else {
                    self.lower_revert_payload(message)?;
                }
            }
            Builtin::Selfdestruct => {
                let address = &self.builtin_args::<1>(builtin, &args)?[0];
                let address = self.lower_expr(address)?;
                self.builder.selfdestruct(address);
            }
            _ => {
                return report_error(
                    self.cx.gcx,
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
        match builtin {
            Builtin::Keccak256 => {
                let value = &self.builtin_args::<1>(builtin, &args)?[0];
                if let ExprKind::Lit(lit) = self.peel_bytes_conversion(value).peel_parens().kind
                    && let LitKind::Str(_, bytes, _) = &lit.kind
                {
                    let hash = keccak256(bytes.as_byte_str());
                    return Some(self.builder.imm(U256::from_be_slice(hash.as_slice())));
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.cx.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncodePacked)
                    && let Some(hash) = self.lower_keccak_abi_encode_packed(*encode_args)
                {
                    return Some(hash);
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.cx.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncode)
                {
                    let exprs = self.variadic_builtin_args(Builtin::AbiEncode, encode_args)?;
                    let encoded = self.lower_abi_encode_scratch(exprs, None)?;
                    let pointer = self.builder.slice_ptr(encoded);
                    let length = self.builder.slice_len(encoded);
                    return Some(self.builder.keccak256(pointer, length));
                }
                let value_ty = self.cx.gcx.type_of_expr(value.id)?;
                let memory_ty = value_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
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
                let TyKind::Udvt(underlying, _) = self.cx.gcx.type_of_expr(expr.id)?.kind else {
                    return report_error(
                        self.cx.gcx,
                        expr.span,
                        "codegen expected UDVT wrap to return a user-defined value type",
                    );
                };
                self.lower_typed_expr(value, underlying)
            }
            Builtin::UdvtUnwrap => {
                let value = self.builtin_args::<1>(builtin, &args)?.first()?;
                let TyKind::Udvt(underlying, _) = self.cx.gcx.type_of_expr(value.id)?.kind else {
                    return report_error(
                        self.cx.gcx,
                        expr.span,
                        "codegen expected UDVT unwrap to receive a user-defined value type",
                    );
                };
                let value = self.lower_expr(value)?;
                Some(self.normalize_dirty_scalar(value, underlying))
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => report_error(
                self.cx.gcx,
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
            Some(bytes) => self.builder.imm(U256::from_be_slice(keccak256(bytes).as_slice())),
            None => {
                let argument_ty = self.cx.gcx.type_of_expr(argument.id)?;
                let memory_ty = argument_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                let value = self.lower_typed_expr(argument, memory_ty)?;
                let value = self.materialize_memory_argument(memory_ty, value, argument.span)?;
                self.builder.keccak256_bytes(value)
            }
        };
        let one = self.builder.imm(1);
        let inner = self.builder.sub(inner, one);
        let zero = self.builder.imm(U256::ZERO);
        let word_size = self.builder.imm(32);
        let size =
            self.builder.imm(EvmMemoryLayout::DYNAMIC_HEADER_SIZE + EvmMemoryLayout::WORD_SIZE);
        let object = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(object, word_size, MemoryObjectKind::Bytes);
        self.builder.memory_object_store_word(object, zero, inner);
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let outer = self.builder.keccak256(data, word_size);
        let mask = self.builder.imm(!U256::from(0xff));
        Some(self.builder.and(outer, mask))
    }

    fn lower_concat_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        enum Part {
            Literal(Vec<u8>),
            Dynamic { value: ValueId, length: ValueId },
            Fixed { value: ValueId, length: u64 },
        }

        let exprs = self.variadic_builtin_args(builtin, &args)?;
        let mut all_literals = Some(Vec::new());
        let mut total = self.builder.imm(0);
        let mut parts = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let ty = self.cx.gcx.type_of_expr(expr.id)?;
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
                            let length = self.builder.imm(bytes.len() as u64);
                            total = self.builder.add(total, length);
                        }
                        parts.push(Part::Literal(bytes));
                        continue;
                    }
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm(all_literals.len() as u64);
                        total = self.builder.add(total, length);
                    }
                    let memory_ty = ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                    let value = self.lower_typed_expr(expr, memory_ty)?;
                    let value = self.materialize_memory_argument(memory_ty, value, expr.span)?;
                    let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                    total = self.builder.add(total, length);
                    parts.push(Part::Dynamic { value, length });
                }
                TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm(all_literals.len() as u64);
                        total = self.builder.add(total, length);
                    }
                    let value = self.lower_expr(expr)?;
                    let length = u64::from(size.bytes());
                    let length_value = self.builder.imm(length);
                    total = self.builder.add(total, length_value);
                    parts.push(Part::Fixed { value, length });
                }
                _ => return self.cx.report_unsupported(expr.span, "concat argument"),
            }
        }

        if let Some(bytes) = all_literals {
            // output = bytes(concat(literals))
            return Self::build_bytes_literal(
                self.cx.gcx,
                self.cx.module,
                &mut self.builder,
                &bytes,
                AllocationSemantics::SOLIDITY_UNINITIALIZED,
                None,
            );
        }

        // output = alloc_bytes(total)
        let size = self.builder.padded_size(total);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        self.builder.set_memory_object_len(output, total, MemoryObjectKind::Bytes);

        let mut offset = self.builder.imm(0);
        // for part { copy(literal | dynamic | fixed, output, offset) }
        for part in parts {
            match part {
                Part::Literal(bytes) => {
                    for chunk in bytes.chunks(32) {
                        let value = self.lower_string_literal_word(chunk);
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm(chunk.len() as u64);
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
                    let length = self.builder.imm(length);
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
                self.cx.gcx,
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
                self.cx.gcx,
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
        self.cx
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
                self.cx
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
        let TyKind::Fn(function) = builtin.ty(self.cx.gcx).kind else {
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
        let parameters = match builtin.ty(self.cx.gcx).kind {
            TyKind::Fn(function) => function.parameters,
            _ => &[],
        };
        let values = self.lower_argument_exprs(
            CallArgumentParams { count: N, names: None, reverse: builtin.is_yul() },
            exprs.iter().enumerate(),
            |this, index, expr| {
                if builtin.is_yul() {
                    return this.lower_yul_word_expr(expr);
                }
                // Solidity builtins convert every argument to the declared parameter type, like
                // solc's `expressionAsType`. A narrow local dirtied by inline assembly is cleaned
                // here, before any semantic check such as the zero-modulus panic observes it.
                match parameters.get(index) {
                    Some(&parameter) => this.lower_typed_expr(expr, parameter),
                    None => this.lower_yul_word_expr(expr),
                }
            },
        )?;
        Some(values.try_into().expect("builtin argument count checked"))
    }

    fn unsupported_builtin<T>(&self, builtin: Builtin, span: Span) -> Option<T> {
        self.cx
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
        self.cx.gcx.dcx().err(message).span(span).help(help).emit();
        None
    }
}
