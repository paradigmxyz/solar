//! Solidity, Yul, and address builtin lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_builtin_call(
        &mut self,
        expr: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        builtin: Builtin,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        if builtin.is_yul() {
            let Some(returns) = builtin.ty(self.gcx).returns() else {
                return report_error(
                    self.gcx,
                    callee.span,
                    "codegen expected Yul builtin to have a function type",
                );
            };
            return if returns.is_empty() {
                let _ = self.lower_yul_unit_builtin_call(builtin, args);
                Some(self.builder.imm_u256(U256::ZERO))
            } else {
                self.lower_yul_value_builtin_call(builtin, args)
                    .or_else(|| Some(self.builder.imm_u256(U256::ZERO)))
            };
        }

        if matches!(builtin, Builtin::ArrayPush | Builtin::ArrayPush0 | Builtin::ArrayPop) {
            let result = match builtin {
                Builtin::ArrayPush => {
                    let Some(arguments) = self.builtin_args::<1>(builtin, &args) else {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    };
                    self.lower_storage_array_push(expr, callee, builtin, arguments)
                }
                Builtin::ArrayPush0 => {
                    let Some(arguments) = self.builtin_args::<0>(builtin, &args) else {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    };
                    self.lower_storage_array_push(expr, callee, builtin, arguments)
                }
                Builtin::ArrayPop => {
                    if self.builtin_args::<0>(builtin, &args).is_none() {
                        return Some(self.builder.imm_u256(U256::ZERO));
                    }
                    self.lower_storage_array_pop(expr, callee)
                }
                _ => unreachable!(),
            };
            return result.or_else(|| Some(self.builder.imm_u256(U256::ZERO)));
        }

        match builtin {
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => {
                let _ = self.lower_unit_builtin_call(builtin, args);
                Some(self.builder.imm_u256(U256::ZERO))
            }
            _ => self
                .lower_solidity_value_builtin_call(builtin, args)
                .or_else(|| Some(self.builder.imm_u256(U256::ZERO))),
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
        let data_ty = self.gcx.type_of_expr(data.id)?;
        let memory_ty = data_ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
        let zero = self.builder.imm_u256(U256::ZERO);
        if capture_returndata && !self.gcx.sess.opts.evm_version.supports_returndata() {
            return report_error(
                self.gcx,
                call_span,
                "codegen cannot bind low-level call returndata before Byzantium",
            );
        }
        let mut gas = self.builder.gas();
        let mut value = zero;
        if let Some(options) = call_opts {
            for option in options.args {
                let option_value = self.lower_expr(&option.value)?;
                match option.name.name {
                    kw::Gas => gas = option_value,
                    sym::value if builtin == Builtin::AddressCall => value = option_value,
                    _ => return report_unsupported(self.gcx, option.name.span, "call option"),
                }
            }
        }
        let data = self.lower_typed_expr(data, memory_ty)?;
        let data = self.materialize_memory_argument(memory_ty, data, data_span)?;
        let input = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        let input_size = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let success = match builtin {
            Builtin::AddressCall => {
                self.builder.call(gas, address, value, input, input_size, zero, zero)
            }
            Builtin::AddressStaticcall => {
                if !self.gcx.sess.opts.evm_version.has_static_call() {
                    return report_error(
                        self.gcx,
                        call_span,
                        "codegen cannot use `staticcall` before Byzantium",
                    );
                }
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

    fn lower_unit_builtin_call(&mut self, builtin: Builtin, args: hir::CallArgs<'_>) -> Option<()> {
        match builtin {
            Builtin::Assert => {
                let condition = &self.builtin_args::<1>(builtin, &args)?[0];
                let condition = self.lower_expr(condition)?;
                let invalid = self.builder.iszero(condition);
                self.panic_if(invalid, PanicCode::Assert);
            }
            Builtin::Require => {
                let (required, message) = self.builtin_args_with_optional::<1>(builtin, &args)?;
                let condition = required.first()?;
                let condition = self.lower_expr(condition)?;
                let is_false = self.builder.iszero(condition);
                let revert_block = self.builder.create_block();
                let continue_block = self.builder.create_block();
                self.builder.branch(is_false, revert_block, continue_block);
                self.builder.switch_to_block(revert_block);
                if let Some(message) = message {
                    self.lower_revert_payload(message)?;
                } else {
                    let zero = self.builder.imm_u256(U256::ZERO);
                    self.builder.revert(zero, zero);
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
                    self.gcx,
                    args.span,
                    "codegen routed a value builtin through unit lowering",
                );
            }
        }
        Some(())
    }

    fn lower_solidity_value_builtin_call(
        &mut self,
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
                    return Some(self.builder.imm_u256(U256::from_be_slice(hash.as_slice())));
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncodePacked)
                    && let Some(hash) = self.lower_keccak_abi_encode_packed(*encode_args)
                {
                    return Some(hash);
                }
                if let ExprKind::Call(callee, encode_args, _) = &value.kind
                    && self.gcx.resolved_builtin(callee) == Some(Builtin::AbiEncode)
                {
                    let exprs = self.variadic_builtin_args(Builtin::AbiEncode, encode_args)?;
                    let encoded = self.lower_abi_encode_slice(exprs, None)?;
                    let pointer = self.builder.slice_ptr(encoded);
                    let length = self.builder.slice_len(encoded);
                    return Some(self.builder.keccak256(pointer, length));
                }
                let value_ty = self.gcx.type_of_expr(value.id);
                let value = self.lower_expr(value)?;
                if let Some(value_ty) = value_ty
                    && let Some(abi_type) = self.types.abi_type(value_ty)
                {
                    self.validate_calldata_bytes_argument(value, &abi_type);
                }
                let value = match self.builder.func().value_ty(value) {
                    Some(MirType::Slice(_)) => self.materialize_memory_slice(value),
                    _ => value,
                };
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
                Some(if builtin == Builtin::Blockhash {
                    self.builder.blockhash(value)
                } else {
                    self.builder.blobhash(value)
                })
            }
            Builtin::AddMod | Builtin::MulMod => {
                let [a, b, modulus] = self.lower_builtin_args(builtin, &args)?;
                self.panic_if_zero(modulus, PanicCode::DivisionByZero);
                Some(if builtin == Builtin::AddMod {
                    self.builder.addmod(a, b, modulus)
                } else {
                    self.builder.mulmod(a, b, modulus)
                })
            }
            Builtin::Erc7201 => self.lower_erc7201(args),
            Builtin::Sha256 | Builtin::Ripemd160 => self.lower_hash_precompile_call(builtin, args),
            Builtin::EcRecover => self.lower_ecrecover_call(args),
            Builtin::StringConcat | Builtin::BytesConcat => {
                self.lower_concat_builtin_call(builtin, args)
            }
            Builtin::UdvtWrap | Builtin::UdvtUnwrap => {
                let value = self.builtin_args::<1>(builtin, &args)?.first()?;
                self.lower_expr(value)
            }
            Builtin::Selfdestruct
            | Builtin::Require
            | Builtin::Assert
            | Builtin::Revert
            | Builtin::RevertMsg => report_error(
                self.gcx,
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
        let argument = &self.builtin_args::<1>(Builtin::Erc7201, &args)?[0];
        let inner = if let ExprKind::Lit(lit) = &argument.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            self.builder.imm_u256(U256::from_be_slice(keccak256(bytes.as_byte_str()).as_slice()))
        } else {
            let value = self.lower_expr(argument)?;
            if let Some(value_ty) = self.gcx.type_of_expr(argument.id)
                && let Some(abi_type) = self.types.abi_type(value_ty)
            {
                self.validate_calldata_bytes_argument(value, &abi_type);
            }
            let value = if matches!(self.builder.func().value_ty(value), Some(MirType::Slice(_))) {
                self.materialize_memory_slice(value)
            } else {
                value
            };
            self.builder.keccak256_bytes(value)
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
            let ty = self.gcx.type_of_expr(expr.id)?;
            match ty.peel_refs().kind {
                TyKind::StringLiteral(..)
                | TyKind::Elementary(
                    solar_sema::hir::ElementaryType::String
                    | solar_sema::hir::ElementaryType::Bytes,
                ) => {
                    if let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind
                        && let LitKind::Str(_, bytes, _) = &lit.kind
                    {
                        let bytes = bytes.as_byte_str().to_vec();
                        if let Some(all_literals) = &mut all_literals {
                            all_literals.extend_from_slice(&bytes);
                        } else {
                            let length = self.builder.imm_u64(bytes.len() as u64);
                            total = self.checked_add(total, length);
                        }
                        parts.push(Part::Literal(bytes));
                        continue;
                    }
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm_u64(all_literals.len() as u64);
                        total = self.checked_add(total, length);
                    }
                    let memory_ty = ty.with_loc_if_ref(self.gcx, DataLocation::Memory);
                    let value = self.lower_typed_expr(expr, memory_ty)?;
                    let value = self.materialize_memory_argument(memory_ty, value, expr.span)?;
                    let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                    total = self.checked_add(total, length);
                    parts.push(Part::Dynamic { value, length });
                }
                TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) => {
                    if let Some(all_literals) = all_literals.take() {
                        let length = self.builder.imm_u64(all_literals.len() as u64);
                        total = self.checked_add(total, length);
                    }
                    let value = self.lower_expr(expr)?;
                    let length = u64::from(size.bytes());
                    let length_value = self.builder.imm_u64(length);
                    total = self.checked_add(total, length_value);
                    parts.push(Part::Fixed { value, length });
                }
                _ => return report_unsupported(self.gcx, expr.span, "concat argument"),
            }
        }

        if let Some(bytes) = all_literals {
            return Self::build_bytes_literal(
                &mut self.builder,
                &bytes,
                AllocationSemantics::SOLIDITY_UNINITIALIZED,
            );
        }

        let size = self.checked_padded_size(total);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_ZEROED,
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
                        offset = self.checked_add(offset, length);
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
                    offset = self.checked_add(offset, length);
                }
                Part::Fixed { value, length } => {
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm_u64(length);
                    offset = self.checked_add(offset, length);
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
        match builtin {
            Builtin::YulMstore => {
                let [offset, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mstore(offset, value);
            }
            Builtin::YulMstore8 => {
                let [offset, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mstore8(offset, value);
            }
            Builtin::YulMcopy => {
                if !self.gcx.sess.opts.evm_version.has_mcopy() {
                    return self.unsupported_yul_version(
                        "codegen requires Cancun-compatible EVM for memory copy",
                        "compile with `--evm-version cancun` or newer",
                        args.span,
                    );
                }
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.mcopy(dest, src, size);
            }
            Builtin::YulSstore => {
                let [slot, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.sstore(slot, value);
            }
            Builtin::YulTstore => {
                let [slot, value] = self.lower_builtin_args(builtin, &args)?;
                self.builder.tstore(slot, value);
            }
            Builtin::YulCalldatacopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.calldatacopy(dest, src, size);
            }
            Builtin::YulCodecopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.codecopy(dest, src, size);
            }
            Builtin::YulExtcodecopy => {
                let [address, dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.extcodecopy(address, dest, src, size);
            }
            Builtin::YulReturndatacopy => {
                let [dest, src, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.returndatacopy(dest, src, size);
            }
            Builtin::YulLog0 => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log0(offset, size);
            }
            Builtin::YulLog1 => {
                let [offset, size, topic1] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log1(offset, size, topic1);
            }
            Builtin::YulLog2 => {
                let [offset, size, topic1, topic2] = self.lower_builtin_args(builtin, &args)?;
                self.builder.log2(offset, size, topic1, topic2);
            }
            Builtin::YulLog3 => {
                let [offset, size, topic1, topic2, topic3] =
                    self.lower_builtin_args(builtin, &args)?;
                self.builder.log3(offset, size, topic1, topic2, topic3);
            }
            Builtin::YulLog4 => {
                let [offset, size, topic1, topic2, topic3, topic4] =
                    self.lower_builtin_args(builtin, &args)?;
                self.builder.log4(offset, size, topic1, topic2, topic3, topic4);
            }
            Builtin::YulRevert => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.revert(offset, size);
            }
            Builtin::YulReturn => {
                let [offset, size] = self.lower_builtin_args(builtin, &args)?;
                self.builder.ret_data(offset, size);
            }
            Builtin::YulStop => {
                let [] = self.lower_builtin_args(builtin, &args)?;
                self.builder.stop();
            }
            Builtin::YulInvalid => {
                let [] = self.lower_builtin_args(builtin, &args)?;
                self.builder.invalid();
            }
            Builtin::YulSelfdestruct => {
                let [address] = self.lower_builtin_args(builtin, &args)?;
                self.builder.selfdestruct(address);
            }
            Builtin::YulPop => {
                let [_value] = self.lower_builtin_args(builtin, &args)?;
            }
            _ => {
                return report_error(
                    self.gcx,
                    args.span,
                    "codegen routed a value Yul builtin through unit lowering",
                );
            }
        }
        Some(())
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
            Builtin::YulClz => {
                let [value] = self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_clz() {
                    return self.unsupported_yul_version(
                        "codegen requires Osaka-compatible EVM for `clz`",
                        "compile with `--evm-version osaka` or newer",
                        args.span,
                    );
                }
                Some(self.builder.clz(value))
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
                let [address, input_offset, input_size, value] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extcall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extcall(address, input_offset, input_size, value))
            }
            Builtin::YulExtdelegatecall => {
                let [address, input_offset, input_size] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extdelegatecall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extdelegatecall(address, input_offset, input_size))
            }
            Builtin::YulExtstaticcall => {
                let [address, input_offset, input_size] =
                    self.lower_builtin_args(builtin, &args)?;
                if !self.gcx.sess.opts.evm_version.has_ext_call() {
                    return self.unsupported_yul_version(
                        "codegen requires Prague-compatible EVM for `extstaticcall`",
                        "compile with `--evm-version prague` or newer",
                        args.span,
                    );
                }
                Some(self.builder.extstaticcall(address, input_offset, input_size))
            }
            _ => report_error(
                self.gcx,
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
        self.gcx
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
                self.gcx
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
        let TyKind::Fn(function) = builtin.ty(self.gcx).kind else {
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
        let values = exprs.iter().map(|arg| self.lower_expr(arg)).collect::<Option<Vec<_>>>()?;
        values.try_into().ok()
    }

    fn unsupported_builtin<T>(&self, builtin: Builtin, span: Span) -> Option<T> {
        self.gcx
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
        self.gcx.dcx().err(message).span(span).help(help).emit();
        None
    }
}
