//! Literal, member, environment, and shared expression lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_string_literal_word(&mut self, bytes: &[u8]) -> ValueId {
        let len = bytes.len().min(32);
        let mut padded = [0_u8; 32];
        padded[..len].copy_from_slice(&bytes[..len]);
        self.builder.imm_u256(U256::from_be_bytes(padded))
    }

    pub(super) fn lower_fixed_bytes_literal(
        &mut self,
        ty: Ty<'gcx>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            ty.peel_refs().kind
        else {
            return None;
        };
        let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind else {
            return None;
        };
        match &lit.kind {
            LitKind::Str(_, bytes, _) => Some(self.lower_string_literal_word(bytes.as_byte_str())),
            LitKind::Number(value) => {
                let shift = usize::from(32 - size.bytes()) * 8;
                Some(self.builder.imm_u256(*value << shift))
            }
            _ => None,
        }
    }

    pub(super) fn lower_literal(&mut self, kind: LitKind<'_>, span: Span) -> Option<ValueId> {
        match kind {
            LitKind::Str(_, value, _) => self.lower_bytes_literal(value.as_byte_str(), span),
            LitKind::Number(value) => Some(self.builder.imm_u256(value)),
            LitKind::Bool(value) => Some(self.builder.imm_bool(value)),
            LitKind::Address(value) => {
                Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())))
            }
            LitKind::Rational(value) if *value.denom() == U256::from(1) => {
                Some(self.builder.imm_u256(*value.numer()))
            }
            _ => report_unsupported(self.context.gcx, span, "literal"),
        }
    }

    pub(super) fn lower_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: Ident,
    ) -> Option<ValueId> {
        if let Some(builtin) = self.context.gcx.resolved_builtin(expr) {
            if builtin == Builtin::AddressBalance {
                let receiver = self.lower_expr(receiver)?;
                return Some(self.builder.balance(receiver));
            }
            return self.lower_environment_builtin(expr, builtin);
        }
        if let Some(value) = self.lower_internal_function_value(expr) {
            return Some(value);
        }
        if let Some(TyKind::Fn(function)) = self.context.gcx.type_of_expr(expr.id).map(|ty| ty.kind)
            && function.is_external()
            && let Some(function_id) = self.context.gcx.resolved_function(expr)
        {
            let address = self.lower_expr(receiver)?;
            let address_shift = self.builder.imm_u64(32);
            let address = self.builder.shl(address_shift, address);
            let selector = self.context.gcx.function_selector(function_id).0;
            let selector = self.builder.imm_u256(U256::from_be_slice(&selector));
            return Some(self.builder.or(address, selector));
        }
        if name.name == sym::offset
            && self
                .type_of_expr_or_variable(receiver)
                .is_some_and(|ty| ty.is_ref_at(DataLocation::Calldata))
        {
            return self.lower_yul_member(expr, receiver, name);
        }
        if let Some(access) = self.storage_access(expr) {
            return self.load_storage_access(expr, access);
        }
        if name.name == sym::length {
            let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?;
            if matches!(
                receiver_ty.peel_refs().kind,
                TyKind::StringLiteral(..)
                    | TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
            ) && let ExprKind::Lit(lit) = self.peel_bytes_conversion(receiver).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                return Some(self.builder.imm_u64(bytes.as_byte_str().len() as u64));
            }
            if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
                if !matches!(receiver.peel_parens().kind, ExprKind::Ident(_)) {
                    self.lower_expr(receiver)?;
                }
                return Some(self.builder.imm_u64(u64::try_from(len).ok()?));
            }
            if receiver_ty.is_ref_at(DataLocation::Storage) {
                if let Some(access) = self.storage_access(receiver) {
                    return match receiver_ty.peel_refs().kind {
                        TyKind::DynArray(_) => Some(self.builder.sload(access.slot)),
                        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                            let object = self.load_storage_bytes(access.slot)?;
                            Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                        }
                        _ => report_unsupported(self.context.gcx, expr.span, "length member"),
                    };
                }
                let object = self.lower_expr(receiver)?;
                return match self.builder.func().value_ty(object) {
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                        Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                    }
                    _ => report_unsupported(self.context.gcx, expr.span, "length member"),
                };
            }
            let object = self.lower_expr(receiver)?;
            let layout = self.types.memory_layout(receiver_ty)?;
            return match layout.kind() {
                MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                    Some(self.builder.memory_object_len(object, layout.kind()))
                }
                _ => report_unsupported(self.context.gcx, expr.span, "length member"),
            };
        }

        let id = self.context.gcx.resolved_variable(expr)?;
        let variable = self.context.gcx.hir.variable(id);
        if variable.is_constant() {
            return self.lower_constant(variable.initializer, expr.span);
        }
        if let Some(hir::ItemId::Enum(enum_id)) = variable.parent {
            let Some(index) = self
                .context
                .gcx
                .hir
                .enumm(enum_id)
                .variants
                .iter()
                .position(|&variant| variant == id)
            else {
                return report_unsupported(self.context.gcx, expr.span, "enum member");
            };
            return Some(self.builder.imm_u256(U256::from(index)));
        }
        if variable.is_state_variable() {
            return self.load_variable(id, expr.span);
        }
        let Some(hir::ItemId::Struct(struct_id)) = variable.parent else {
            return report_unsupported(self.context.gcx, expr.span, "member");
        };
        let Some(field) =
            self.context.gcx.hir.strukt(struct_id).fields.iter().position(|&field| field == id)
        else {
            return report_unsupported(self.context.gcx, expr.span, "struct field");
        };
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        let object = self.lower_expr(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && matches!(
                self.builder.func().value_ty(object),
                Some(MirType::Slice(SliceLocation::Calldata))
            )
        {
            let AbiType::Tuple(fields) = self.types.abi_type(receiver_ty)? else {
                return report_unsupported(self.context.gcx, expr.span, "calldata struct field");
            };
            let offset = fields[..field].iter().map(AbiType::head_size).sum();
            let offset = self.builder.imm_u64(offset);
            let base = self.builder.slice_ptr(object);
            let head = self.builder.add(base, offset);
            let field_ty = self
                .context
                .gcx
                .type_of_item(id.into())
                .with_loc_if_ref(self.context.gcx, DataLocation::Calldata);
            let validate_bounds = fields[field].is_dynamic();
            return self.materialize_calldata_value_at_inner(
                field_ty,
                head,
                base,
                expr.span,
                validate_bounds,
            );
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        let value = self.builder.memory_object_load_field(object, layout, field as u64);
        let field_ty = self.context.gcx.type_of_item(id.into());
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && let TyKind::Fn(function) = field_ty.peel_refs().kind
            && function.is_external()
        {
            let inst = match self.builder.func().value(value) {
                Value::Inst(inst) => Some(*inst),
                _ => None,
            };
            if let Some(inst) = inst {
                self.builder.func_mut().inst_mut(inst).metadata.set_abi_validation(true);
            }
        }
        Some(self.normalize_memory_scalar(field_ty, value))
    }

    pub(super) fn lower_yul_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: Ident,
    ) -> Option<ValueId> {
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata) {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                sym::offset => Some(self.builder.slice_ptr(value)),
                sym::length => Some(self.builder.slice_len(value)),
                _ => report_unsupported(self.context.gcx, expr.span, "Yul calldata member"),
            };
        }

        if let TyKind::Fn(function) = receiver_ty.peel_refs().kind
            && function.is_external()
        {
            let value = self.lower_expr(receiver)?;
            return match name.name {
                kw::Address => {
                    let shift = self.builder.imm_u64(32);
                    let address = self.builder.shr(shift, value);
                    let mask = self.builder.imm_u256(U256::MAX >> 96);
                    Some(self.builder.and(address, mask))
                }
                sym::selector => {
                    let mask = self.builder.imm_u256(U256::from(u32::MAX));
                    Some(self.builder.and(value, mask))
                }
                _ => report_unsupported(self.context.gcx, expr.span, "Yul function member"),
            };
        }

        let Some(access) = self.storage_access(receiver) else {
            return report_unsupported(self.context.gcx, expr.span, "Yul storage member");
        };
        match name.name {
            sym::slot => Some(access.slot),
            sym::offset => Some(
                access
                    .offset
                    .unwrap_or_else(|| self.builder.imm_u64(u64::from(access.location.offset))),
            ),
            _ => report_unsupported(self.context.gcx, expr.span, "Yul storage member"),
        }
    }

    pub(super) fn type_of_expr_or_variable(&self, expr: &hir::Expr<'_>) -> Option<Ty<'gcx>> {
        self.context.gcx.type_of_expr(expr.id).or_else(|| {
            self.context
                .gcx
                .resolved_variable(expr)
                .map(|id| self.context.gcx.type_of_item(id.into()))
        })
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

    pub(super) fn lower_environment_builtin(
        &mut self,
        expr: &hir::Expr<'_>,
        builtin: Builtin,
    ) -> Option<ValueId> {
        if matches!(builtin, Builtin::ContractCreationCode | Builtin::ContractRuntimeCode) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.context.gcx, expr.span, "environment builtin");
            };
            let TyKind::Meta(ty) = self.context.gcx.type_of_expr(receiver.id)?.kind else {
                return report_unsupported(self.context.gcx, expr.span, "creation code target");
            };
            let TyKind::Contract(contract_id) = ty.peel_refs().kind else {
                return report_unsupported(self.context.gcx, expr.span, "creation code target");
            };
            let bytecodes = if builtin == Builtin::ContractCreationCode {
                self.context.child_bytecodes
            } else {
                self.context.child_runtime_bytecodes
            };
            let Some(bytecode) = bytecodes.get(&contract_id) else {
                let (kind, name) = if builtin == Builtin::ContractCreationCode {
                    ("creation", "creationCode")
                } else {
                    ("runtime", "runtimeCode")
                };
                self.context
                    .gcx
                    .dcx()
                    .err(format!("codegen is missing {kind} bytecode for `{name}`"))
                    .span(expr.span)
                    .note("the referenced contract did not compile or was not lowered first")
                    .emit();
                return None;
            };
            return self.lower_bytes_literal(bytecode, expr.span);
        }
        if matches!(builtin, Builtin::AddressCode | Builtin::AddressCodehash) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.context.gcx, expr.span, "environment builtin");
            };
            let address = self.lower_expr(receiver)?;
            if builtin == Builtin::AddressCodehash {
                return Some(self.builder.extcodehash(address));
            }

            let length = self.builder.extcodesize(address);
            let size = self.builder.checked_padded_size(length);
            let object = self.builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::SOLIDITY_ZEROED,
            );
            self.builder.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
            let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
            let zero = self.builder.imm_u256(U256::ZERO);
            self.builder.extcodecopy(address, data, zero, length);
            return Some(object);
        }
        if builtin == Builtin::FunctionAddress {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.context.gcx, expr.span, "function address");
            };
            let Some(TyKind::Fn(function)) =
                self.type_of_expr_or_variable(receiver).map(|ty| ty.kind)
            else {
                return report_unsupported(self.context.gcx, expr.span, "function address");
            };
            if !function.is_external() {
                return report_unsupported(self.context.gcx, expr.span, "function address");
            }
            let value = self.lower_expr(receiver)?;
            let shift = self.builder.imm_u64(32);
            let address = self.builder.shr(shift, value);
            let mask = self.builder.imm_u256(U256::MAX >> 96);
            return Some(self.builder.and(address, mask));
        }
        if builtin == Builtin::FunctionSelector {
            let selector = match self.context.gcx.resolved_expr(expr).and_then(|res| match res {
                hir::Res::Item(item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_))) => {
                    Some(self.context.gcx.function_selector(item).0)
                }
                _ => None,
            }) {
                Some(selector) => {
                    let ExprKind::Member(receiver, _) = &expr.kind else {
                        return report_unsupported(
                            self.context.gcx,
                            expr.span,
                            "function selector",
                        );
                    };
                    self.lower_selector_receiver_effects(receiver)?;
                    selector
                }
                None => {
                    let hir::ExprKind::Member(receiver, _) = &expr.kind else {
                        return report_unsupported(
                            self.context.gcx,
                            expr.span,
                            "function selector",
                        );
                    };
                    if let Some(item) =
                        self.context.gcx.resolved_expr(receiver).and_then(|res| match res {
                            hir::Res::Item(
                                item @ (hir::ItemId::Function(_) | hir::ItemId::Error(_)),
                            ) => Some(item),
                            _ => None,
                        })
                    {
                        self.lower_selector_receiver_effects(receiver)?;
                        self.context.gcx.function_selector(item).0
                    } else {
                        let Some(TyKind::Fn(function)) =
                            self.type_of_expr_or_variable(receiver).map(|ty| ty.kind)
                        else {
                            return report_unsupported(
                                self.context.gcx,
                                expr.span,
                                "function selector",
                            );
                        };
                        if !function.is_external() {
                            return report_unsupported(
                                self.context.gcx,
                                expr.span,
                                "function selector",
                            );
                        }
                        let value = self.lower_expr(receiver)?;
                        let mask = self.builder.imm_u256(U256::from(u32::MAX));
                        let selector = self.builder.and(value, mask);
                        let shift = self.builder.imm_u64(224);
                        return Some(self.builder.shl(shift, selector));
                    }
                }
            };
            return Some(self.builder.imm_u256(U256::from_be_slice(&selector) << 224));
        }
        if builtin == Builtin::EventSelector {
            let event_id = self.context.gcx.resolved_expr(expr).and_then(|res| match res {
                hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                _ => None,
            });
            let event_id = event_id.or_else(|| {
                let ExprKind::Member(receiver, _) = &expr.kind else { return None };
                self.context.gcx.resolved_expr(receiver).and_then(|res| match res {
                    hir::Res::Item(hir::ItemId::Event(id)) => Some(id),
                    _ => None,
                })
            });
            let Some(event_id) = event_id else {
                return report_unsupported(self.context.gcx, expr.span, "event selector");
            };
            return Some(self.builder.imm_u256(U256::from_be_slice(
                self.context.gcx.event_selector(event_id).as_slice(),
            )));
        }
        if builtin == Builtin::ArrayLength {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.context.gcx, expr.span, "array length");
            };
            if self.context.gcx.resolved_builtin(receiver) == Some(Builtin::AddressCode)
                && let ExprKind::Member(address, _) = &receiver.kind
            {
                let address = self.lower_expr(address)?;
                return Some(self.builder.extcodesize(address));
            }
            let receiver_ty = self.context.gcx.type_of_expr(receiver.id)?;
            if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
                if !matches!(receiver.peel_parens().kind, ExprKind::Ident(_)) {
                    self.lower_expr(receiver)?;
                }
                return Some(self.builder.imm_u64(u64::try_from(len).ok()?));
            }
            if receiver_ty.is_ref_at(DataLocation::Storage) {
                if let Some(access) = self.storage_access(receiver) {
                    return match receiver_ty.peel_refs().kind {
                        TyKind::DynArray(_) => Some(self.builder.sload(access.slot)),
                        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                            let object = self.load_storage_bytes(access.slot)?;
                            Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                        }
                        _ => report_unsupported(self.context.gcx, expr.span, "array length"),
                    };
                }
                let object = self.lower_expr(receiver)?;
                return match self.builder.func().value_ty(object) {
                    Some(MirType::MemoryObject(MemoryObjectKind::Bytes)) => {
                        Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                    }
                    _ => report_unsupported(self.context.gcx, expr.span, "array length"),
                };
            }
            let object = self.lower_expr(receiver)?;
            if matches!(self.builder.func().value_ty(object), Some(MirType::Slice(_))) {
                return Some(self.builder.slice_len(object));
            }
            let layout = self.types.memory_layout(receiver_ty)?;
            return match layout.kind() {
                MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                    Some(self.builder.memory_object_len(object, layout.kind()))
                }
                _ => report_unsupported(self.context.gcx, expr.span, "array length"),
            };
        }
        if matches!(builtin, Builtin::TypeMin | Builtin::TypeMax | Builtin::InterfaceId) {
            let ExprKind::Member(receiver, _) = &expr.kind else {
                return report_unsupported(self.context.gcx, expr.span, "type member");
            };
            if builtin == Builtin::InterfaceId {
                let TyKind::Meta(ty) = self.context.gcx.type_of_expr(receiver.id)?.kind else {
                    return report_unsupported(self.context.gcx, expr.span, "interface id");
                };
                let TyKind::Contract(id) = ty.peel_refs().kind else {
                    return report_unsupported(self.context.gcx, expr.span, "interface id");
                };
                let value = self
                    .context
                    .gcx
                    .interface_functions(id)
                    .own()
                    .iter()
                    .fold(U256::ZERO, |value, function| {
                        value ^ U256::from_be_slice(function.selector.as_slice())
                    })
                    << 224;
                return Some(self.builder.imm_u256(value));
            }
            let value = self.type_limit(receiver, expr.span, builtin == Builtin::TypeMax)?;
            return Some(self.builder.imm_u256(value));
        }
        Some(match builtin {
            Builtin::This => self.builder.address(),
            Builtin::BlockCoinbase => self.builder.coinbase(),
            Builtin::BlockTimestamp => self.builder.timestamp(),
            Builtin::BlockDifficulty | Builtin::BlockPrevrandao => self.builder.prevrandao(),
            Builtin::BlockNumber => self.builder.number(),
            Builtin::BlockGaslimit => self.builder.gaslimit(),
            Builtin::BlockChainid => self.builder.chainid(),
            Builtin::BlockBasefee => self.builder.basefee(),
            Builtin::BlockBlobbasefee => self.builder.blobbasefee(),
            Builtin::MsgSender => self.builder.caller(),
            Builtin::MsgGas => self.builder.gas(),
            Builtin::MsgValue => self.builder.callvalue(),
            Builtin::MsgSig => {
                let offset = self.builder.imm_u64(0);
                let value = self.calldata_load_word(offset);
                let mask = self.builder.imm_u256(U256::MAX << 224);
                self.builder.and(value, mask)
            }
            Builtin::MsgData => {
                let offset = self.builder.imm_u64(0);
                let length = self.builder.calldatasize();
                self.builder.make_slice(offset, length, SliceLocation::Calldata)
            }
            Builtin::TxOrigin => self.builder.origin(),
            Builtin::TxGasPrice => self.builder.gasprice(),
            _ => return report_unsupported(self.context.gcx, expr.span, "environment builtin"),
        })
    }

    pub(super) fn type_limit(
        &self,
        receiver: &hir::Expr<'_>,
        span: Span,
        maximum: bool,
    ) -> Option<U256> {
        let TyKind::Meta(ty) = self.context.gcx.type_of_expr(receiver.id)?.kind else {
            return report_unsupported(self.context.gcx, span, "type limit");
        };
        match ty.peel_refs().kind {
            TyKind::Enum(id) => {
                let max = self.context.gcx.hir.enumm(id).variants.len().saturating_sub(1);
                Some(U256::from(if maximum { max } else { 0 }))
            }
            TyKind::Elementary(ElementaryType::UInt(size)) => {
                let max = (U256::from(1) << size.bits()) - U256::from(1);
                Some(if maximum { max } else { U256::ZERO })
            }
            TyKind::Elementary(ElementaryType::Int(size)) => {
                let magnitude = U256::from(1) << (size.bits() - 1);
                Some(if maximum {
                    magnitude - U256::from(1)
                } else {
                    U256::MAX - magnitude + U256::from(1)
                })
            }
            _ => report_unsupported(self.context.gcx, span, "type limit"),
        }
    }

    pub(super) fn normalize_byte_value(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let Some(ty) = self.context.gcx.type_of_expr(expr.id) else { return value };
        self.normalize_byte_type(ty, value)
    }

    pub(super) fn normalize_byte_type(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let TyKind::Elementary(ElementaryType::FixedBytes(size)) = ty.peel_refs().kind else {
            return value;
        };
        let shift = self.builder.imm_u64(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Type(ty) = &callee.kind
            && matches!(
                ty.kind,
                hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
            && let hir::CallArgsKind::Unnamed([inner]) = args.kind
        {
            return inner;
        }
        expr
    }

    pub(super) fn lower_constant(
        &mut self,
        initializer: Option<&hir::Expr<'_>>,
        span: Span,
    ) -> Option<ValueId> {
        let Some(initializer) = initializer else {
            return report_unsupported(self.context.gcx, span, "constant initializer");
        };
        if let Ok(value) = self.context.gcx.try_eval_const_value(initializer) {
            return match value {
                ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
                ConstValue::Integer(value) => Some(self.builder.imm_u256(value.as_u256()?)),
                ConstValue::String(value) => {
                    self.lower_bytes_literal(value.as_byte_str_in(self.context.gcx.sess), span)
                }
            };
        }
        self.lower_expr(initializer)
    }
}
