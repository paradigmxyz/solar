//! Statement-level HIR to MIR lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_stmt(&mut self, stmt: &hir::Stmt<'_>) -> Option<()> {
        match &stmt.kind {
            StmtKind::DeclSingle(id) => {
                let initializer = self.gcx.hir.variable(*id).initializer;
                let ty = self.gcx.type_of_item((*id).into());
                if ty.is_ref_at(DataLocation::Storage) {
                    let Some(initializer) = initializer else { return Some(()) };
                    let Some(access) = self.storage_access(initializer) else {
                        return report_unsupported(self.gcx, initializer.span, "storage reference");
                    };
                    self.storage_refs.insert(*id, access);
                    return Some(());
                }
                if initializer.is_none() && self.types.memory_layout(ty).is_some() {
                    self.deferred_bindings.insert(*id);
                    return Some(());
                }
                let value = if let Some(expr) = initializer {
                    let value = self.lower_typed_expr(expr, ty)?;
                    self.coerce_value(value, self.gcx.type_of_expr(expr.id)?, ty)
                } else if let Some(value) = self.default_object(ty) {
                    value
                } else {
                    self.builder.imm_u256(U256::ZERO)
                };
                let value = self.materialize_memory_argument(
                    ty,
                    value,
                    initializer.map_or(stmt.span, |expr| expr.span),
                )?;
                self.values.insert(*id, value);
            }
            StmtKind::DeclMulti(ids, expr) => {
                if ids
                    .iter()
                    .flatten()
                    .any(|&id| self.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage))
                    && let Some(values) = self.lower_storage_reference_call(expr.peel_parens())
                {
                    if values.len() != ids.len() {
                        return report_unsupported(self.gcx, expr.span, "storage reference tuple");
                    }
                    for (id, (value, access)) in ids.iter().zip(values) {
                        let Some(id) = id else { continue };
                        if let Some(access) = access {
                            if !self.gcx.type_of_item((*id).into()).is_ref_at(DataLocation::Storage)
                            {
                                return report_unsupported(
                                    self.gcx,
                                    self.gcx.hir.variable(*id).span,
                                    "mixed storage tuple",
                                );
                            }
                            self.storage_refs.insert(*id, access);
                        } else {
                            if self.gcx.type_of_item((*id).into()).is_ref_at(DataLocation::Storage)
                            {
                                return report_unsupported(
                                    self.gcx,
                                    self.gcx.hir.variable(*id).span,
                                    "mixed storage tuple",
                                );
                            }
                            self.values.insert(*id, value);
                        }
                    }
                    return Some(());
                }
                if let ExprKind::Tuple(values) = &expr.peel_parens().kind
                    && values.len() == ids.len()
                {
                    for (id, value) in ids.iter().zip(values.iter()) {
                        let Some(value) = value else {
                            if id.is_some() {
                                return report_unsupported(
                                    self.gcx,
                                    expr.span,
                                    "tuple declaration value",
                                );
                            }
                            continue;
                        };
                        let value = self.lower_expr(value)?;
                        let Some(id) = id else { continue };
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                if self.is_low_level_call_expr(expr) {
                    let values = self.lower_low_level_call_values(
                        expr,
                        ids.iter().flatten().count(),
                        ids.first().is_some_and(Option::is_none),
                    )?;
                    for (id, value) in ids.iter().flatten().zip(values) {
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                let values = self.lower_values(expr)?;
                if values.len() != ids.len() {
                    return report_unsupported(self.gcx, expr.span, "tuple declaration arity");
                }
                for (id, value) in ids.iter().zip(values) {
                    if let Some(id) = id {
                        self.values.insert(*id, value);
                    }
                }
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Assign(lhs, None, rhs) = &expr.peel_parens().kind
                    && self.is_constant_storage_assignment(lhs, rhs)
                {
                    self.lower_constant_storage_assignment(lhs, rhs)?;
                    return Some(());
                }
                self.lower_expr(expr)?;
            }
            StmtKind::Block(block) => self.lower_block(*block)?,
            StmtKind::UncheckedBlock(block) => {
                let previous = self.unchecked;
                self.unchecked = true;
                let result = self.lower_block(*block);
                self.unchecked = previous;
                result?;
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(cond, then_stmt, *else_stmt)?;
            }
            StmtKind::Switch(switch) => self.lower_switch(switch)?,
            StmtKind::Loop(block, source) => self.lower_loop(*block, *source)?,
            StmtKind::Break => {
                let Some(target) = self.loops.last().map(|targets| targets.break_block) else {
                    return report_unsupported(self.gcx, stmt.span, "break outside loop");
                };
                let state = LoopState {
                    block: self.builder.current_block(),
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                };
                self.loops.last_mut().expect("loop target exists").break_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Continue => {
                let Some(target) = self.loops.last().map(|targets| targets.continue_block) else {
                    return report_unsupported(self.gcx, stmt.span, "continue outside loop");
                };
                let state = LoopState {
                    block: self.builder.current_block(),
                    values: self.values.clone(),
                    storage_refs: self.storage_refs.clone(),
                };
                self.loops.last_mut().expect("loop target exists").continue_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Return(expr) => {
                let values = expr.map_or_else(
                    || Some(Vec::new()),
                    |expr| {
                        if self.returns.len() == 1 {
                            let ty = self.gcx.type_of_item(self.returns[0].into());
                            if ty.is_ref_at(DataLocation::Storage) {
                                return Some(vec![self.storage_access(expr)?.slot]);
                            }
                            if self
                                .gcx
                                .type_of_expr(expr.id)
                                .is_some_and(|source| source.is_ref_at(DataLocation::Storage))
                                && self.types.memory_layout(ty).is_some()
                            {
                                return Some(vec![self.lower_typed_expr(expr, ty)?]);
                            }
                            if let ExprKind::Lit(lit) =
                                self.peel_bytes_conversion(expr).peel_parens().kind
                                && matches!(lit.kind, LitKind::Str(..))
                                && matches!(
                                    ty.peel_refs().kind,
                                    TyKind::Elementary(
                                        solar_sema::hir::ElementaryType::Bytes
                                            | solar_sema::hir::ElementaryType::String,
                                    )
                                )
                            {
                                return Some(vec![self.lower_typed_expr(expr, ty)?]);
                            }
                        }
                        self.lower_return_values(expr)
                    },
                )?;
                if let Some(target) = self.return_targets.last().map(|target| target.block) {
                    if !values.is_empty() {
                        if values.len() != self.returns.len() {
                            return report_unsupported(self.gcx, stmt.span, "return value count");
                        }
                        let return_ids = self.returns.clone();
                        for (id, value) in return_ids.into_iter().zip(values) {
                            let ty = self.gcx.type_of_item(id.into());
                            if ty.is_ref_at(DataLocation::Storage) {
                                let access =
                                    self.storage_refs.get(&id).copied().unwrap_or(StorageAccess {
                                        slot: value,
                                        location: StorageLocation::word(U256::ZERO),
                                        offset: None,
                                    });
                                self.storage_refs
                                    .insert(id, StorageAccess { slot: value, ..access });
                            } else {
                                let value =
                                    self.materialize_memory_argument(ty, value, stmt.span)?;
                                self.values.insert(id, value);
                            }
                        }
                    }
                    self.record_return_state();
                    self.builder.jump(target);
                } else if !self.is_terminated() {
                    if values.is_empty() {
                        self.builder.stop();
                    } else {
                        if values.len() != self.returns.len() {
                            return report_unsupported(self.gcx, stmt.span, "return value count");
                        }
                        let return_ids = self.returns.clone();
                        let values = return_ids
                            .into_iter()
                            .zip(values)
                            .map(|(id, value)| {
                                let ty = self.gcx.type_of_item(id.into());
                                if ty.is_ref_at(DataLocation::Storage) {
                                    Some(value)
                                } else {
                                    self.materialize_memory_argument(ty, value, stmt.span)
                                }
                            })
                            .collect::<Option<Vec<_>>>()?;
                        self.builder.ret(values);
                    }
                }
            }
            StmtKind::Revert(expr) => self.lower_revert_payload(expr)?,
            StmtKind::AssemblyBlock(block) => self.lower_block(*block)?,
            StmtKind::Placeholder => {
                self.lower_modifier_placeholder(stmt.span)?;
            }
            StmtKind::Emit(expr) => self.lower_emit(expr)?,
            StmtKind::Try(try_stmt) => self.lower_try(try_stmt)?,
            StmtKind::Err(_) => return report_unsupported(self.gcx, stmt.span, "statement"),
        }
        Some(())
    }

    pub(super) fn lower_revert_payload(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Error(error_id))) =
                self.gcx.resolved_expr(callee)
        {
            return self.lower_custom_error_revert(error_id, *args);
        }

        if let Some(bytes) = self.constant_string_bytes(expr)
            && (1..=32).contains(&bytes.len())
        {
            let length = self.builder.imm_u64(bytes.len() as u64);
            let data = self.lower_string_literal_word(&bytes);
            let helper = self.ensure_revert_error_helper();
            self.builder.internal_call_void(helper, vec![length, data], 0);
            self.builder.invalid();
            return Some(());
        }

        let literal = expr.peel_parens();
        if let ExprKind::Lit(lit) = &literal.kind
            && let LitKind::Str(StrKind::Str | StrKind::Unicode | StrKind::Hex, bytes, _) =
                &lit.kind
            && bytes.as_byte_str().is_empty()
        {
            let selector = keccak256("Error(string)");
            let selector = self.builder.imm_u256(U256::from_be_slice(&selector[..4]) << 224);
            let zero = self.builder.imm_u64(0);
            self.builder.mstore(zero, selector);
            let offset = self.builder.imm_u64(4);
            let tuple_offset = self.builder.imm_u64(32);
            self.builder.mstore(offset, tuple_offset);
            let length = self.builder.imm_u64(36);
            let byte_len = self.builder.imm_u64(0);
            self.builder.mstore(length, byte_len);
            let size = self.builder.imm_u64(68);
            self.builder.revert(zero, size);
            return Some(());
        }

        let value = self.lower_expr(expr)?;
        let ty = self.gcx.type_of_expr(expr.id)?;
        let value =
            if self.needs_calldata_materialization(value, &AbiType::Bytes(SliceLocation::Memory)) {
                self.materialize_calldata_argument(ty, value, expr.span)?
            } else {
                value
            };
        let selector = keccak256("Error(string)");
        let selector = self.builder.imm_u256(U256::from_be_slice(&selector[..4]) << 224);
        let layout = Arc::new(AbiLayout::new(
            vec![AbiType::Bytes(SliceLocation::Memory)].into_boxed_slice(),
        ));
        let encoded =
            self.builder.abi_encode(layout, Some(selector), vec![value].into_boxed_slice());
        let pointer = self.builder.slice_ptr(encoded);
        let length = self.builder.slice_len(encoded);
        self.builder.revert(pointer, length);
        Some(())
    }

    fn constant_string_bytes(&self, expr: &hir::Expr<'_>) -> Option<Vec<u8>> {
        let mut expr = expr.peel_parens();
        for _ in 0..4 {
            match &expr.kind {
                ExprKind::Lit(lit) => {
                    let LitKind::Str(_, bytes, _) = &lit.kind else { return None };
                    return Some(bytes.as_byte_str().to_vec());
                }
                ExprKind::Ident(_) | ExprKind::Member(..) => {
                    let variable_id = self.gcx.resolved_variable(expr)?;
                    let variable = self.gcx.hir.variable(variable_id);
                    if !variable.is_constant() {
                        return None;
                    }
                    expr = variable.initializer?.peel_parens();
                }
                _ => return None,
            }
        }
        None
    }

    fn ensure_revert_error_helper(&mut self) -> FunctionId {
        if let Some(id) = *self.revert_error_helper {
            return id;
        }

        let mut function = Function::new(Ident::with_dummy_span(sym::__revert_error));
        function.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut function);
            let length = builder.add_param(MirType::uint256());
            let value = builder.add_param(MirType::uint256());
            let selector = keccak256("Error(string)");
            let selector = builder.imm_u256(U256::from_be_slice(&selector[..4]) << 224);
            let zero = builder.imm_u64(0);
            builder.mstore(zero, selector);
            let offset = builder.imm_u64(4);
            let tuple_offset = builder.imm_u64(32);
            builder.mstore(offset, tuple_offset);
            let length_offset = builder.imm_u64(36);
            builder.mstore(length_offset, length);
            let data_offset = builder.imm_u64(68);
            builder.mstore(data_offset, value);
            let size = builder.imm_u64(100);
            builder.revert(zero, size);
        }
        let id = self.module.add_function(function);
        *self.revert_error_helper = Some(id);
        id
    }

    pub(super) fn lower_custom_error_revert(
        &mut self,
        error_id: hir::ErrorId,
        args: hir::CallArgs<'_>,
    ) -> Option<()> {
        let parameters = self.gcx.item_parameters(hir::ItemId::Error(error_id));
        if args.len() != parameters.len() {
            return report_unsupported(self.gcx, args.span, "error arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Error(error_id));
        let mut values = Vec::with_capacity(parameters.len());
        let mut types = Vec::with_capacity(parameters.len());
        for (index, &parameter) in parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "error argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let (mut value, abi_type) = self.lower_abi_call_argument(argument, parameter_ty)?;
            if matches!(abi_type, AbiType::Word) {
                value = self.lower_word_value(parameter_ty, argument, value);
            }
            values.push(value);
            types.push(abi_type);
        }
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let selector = self
            .builder
            .imm_u256(U256::from_be_slice(&self.gcx.function_selector(error_id).0) << 224);
        let encoded = self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
        let pointer = self.builder.slice_ptr(encoded);
        let length = self.builder.slice_len(encoded);
        self.builder.revert(pointer, length);
        Some(())
    }

    pub(super) fn lower_emit(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return report_unsupported(self.gcx, expr.span, "event emission");
        };
        let Some(hir::Res::Item(hir::ItemId::Event(event_id))) = self.gcx.resolved_expr(callee)
        else {
            return report_unsupported(self.gcx, expr.span, "event emission");
        };

        let event = self.gcx.hir.event(event_id);
        let max_indexed = if event.anonymous { 4 } else { 3 };
        let indexed_count =
            event.parameters.iter().filter(|&&id| self.gcx.hir.variable(id).indexed).count();
        if indexed_count > max_indexed {
            if self.invalid_event_topics.insert(event_id) {
                self.gcx
                    .dcx()
                    .err(format!("event cannot have more than {max_indexed} indexed parameters"))
                    .span(event.span)
                    .emit();
            }
            return Some(());
        }
        if args.len() != event.parameters.len() {
            return report_unsupported(self.gcx, args.span, "event arguments");
        }

        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Event(event_id));
        let mut topics = Vec::with_capacity(indexed_count + usize::from(!event.anonymous));
        if !event.anonymous {
            topics.push(
                self.builder
                    .imm_u256(U256::from_be_slice(self.gcx.event_selector(event_id).as_slice())),
            );
        }
        let mut data_values = Vec::new();
        let mut data_types = Vec::new();
        for (index, &parameter) in event.parameters.iter().enumerate() {
            let Some(argument) =
                args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, args.span, "event argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let variable = self.gcx.hir.variable(parameter);
            let mut value = self.lower_typed_expr(argument, parameter_ty)?;
            if let Some(argument_ty) = self.gcx.type_of_expr(argument.id)
                && let Some(argument_abi_type) = self.types.abi_type(argument_ty)
            {
                self.validate_calldata_bytes_argument(value, &argument_abi_type);
            }
            if variable.indexed {
                match parameter_ty.peel_refs().kind {
                    TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                        | solar_sema::hir::ElementaryType::String,
                    ) => {
                        if matches!(self.builder.func().value_ty(value), Some(MirType::Slice(_))) {
                            value = self.materialize_memory_slice(value);
                        }
                        topics.push(self.builder.keccak256_bytes(value));
                    }
                    TyKind::Struct(_)
                    | TyKind::Array(..)
                    | TyKind::DynArray(_)
                    | TyKind::Slice(_)
                    | TyKind::Tuple(_) => {
                        let mut abi_type = self.types.abi_type(parameter_ty)?;
                        abi_type = self.abi_type_for_value(value, abi_type);
                        let validated_static =
                            self.validate_calldata_static_argument(value, parameter_ty);
                        let calldata_dynamic = matches!(
                            self.builder.func().value_ty(value),
                            Some(MirType::Slice(SliceLocation::Calldata))
                        ) && abi_type.is_dynamic();
                        if (calldata_dynamic
                            || self.needs_calldata_materialization(value, &abi_type))
                            && !validated_static
                        {
                            value = self.materialize_calldata_argument(
                                parameter_ty,
                                value,
                                argument.span,
                            )?;
                            abi_type = self.abi_type_for_value(value, abi_type);
                        }
                        if abi_type.is_dynamic() {
                            if let Some(packed) = self
                                .lower_packed_word_array(parameter_ty, value)
                                .or_else(|| self.lower_inplace_dynamic_value(parameter_ty, value))
                            {
                                topics.push(self.builder.keccak256_bytes(packed));
                                continue;
                            }
                            self.gcx
                                .dcx()
                                .err(
                                    "codegen does not support indexed event aggregate encoding yet",
                                )
                                .span(argument.span)
                                .emit();
                            return Some(());
                        }
                        let layout = Arc::new(AbiLayout::new(vec![abi_type].into_boxed_slice()));
                        let encoded = self.builder.abi_encode(layout, None, [value]);
                        let pointer = self.builder.slice_ptr(encoded);
                        let length = self.builder.slice_len(encoded);
                        topics.push(self.builder.keccak256(pointer, length));
                    }
                    _ => topics.push(self.lower_word_value(parameter_ty, argument, value)),
                }
            } else {
                let mut abi_type = self.types.abi_type(parameter_ty)?;
                abi_type = self.abi_type_for_value(value, abi_type);
                let validated_static = self.validate_calldata_static_argument(value, parameter_ty);
                if self.needs_calldata_materialization(value, &abi_type) && !validated_static {
                    value =
                        self.materialize_calldata_argument(parameter_ty, value, argument.span)?;
                    abi_type = Self::memory_abi_type(abi_type);
                }
                if matches!(abi_type, AbiType::Word) {
                    value = self.lower_word_value(parameter_ty, argument, value);
                }
                data_values.push(value);
                data_types.push(abi_type);
            }
        }

        let (data_ptr, data_size) = if data_types.is_empty() {
            let zero = self.builder.imm_u256(U256::ZERO);
            (zero, zero)
        } else if matches!(data_types.as_slice(), [AbiType::Word]) {
            let zero = self.builder.imm_u64(0);
            self.builder.mstore(zero, data_values[0]);
            (zero, self.builder.imm_u64(32))
        } else {
            let layout = Arc::new(AbiLayout::new(data_types.into_boxed_slice()));
            let encoded = self.builder.abi_encode(layout, None, data_values.into_boxed_slice());
            (self.builder.slice_ptr(encoded), self.builder.slice_len(encoded))
        };
        match topics.as_slice() {
            [] => self.builder.log0(data_ptr, data_size),
            &[topic] => self.builder.log1(data_ptr, data_size, topic),
            &[topic1, topic2] => self.builder.log2(data_ptr, data_size, topic1, topic2),
            &[topic1, topic2, topic3] => {
                self.builder.log3(data_ptr, data_size, topic1, topic2, topic3)
            }
            &[topic1, topic2, topic3, topic4] => {
                self.builder.log4(data_ptr, data_size, topic1, topic2, topic3, topic4)
            }
            _ => return report_unsupported(self.gcx, args.span, "event topics"),
        }
        Some(())
    }
    pub(super) fn lower_block(&mut self, block: hir::Block<'_>) -> Option<()> {
        for stmt in block.stmts {
            if self.is_terminated() {
                break;
            }
            self.lower_stmt(stmt)?;
        }
        Some(())
    }
    pub(super) fn lower_word_value(
        &mut self,
        ty: Ty<'gcx>,
        expr: &hir::Expr<'_>,
        value: ValueId,
    ) -> ValueId {
        let expr = expr.peel_parens();
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm_u64(64);
            return self.builder.shl(shift, value);
        }
        if !matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(_))
        ) {
            return value;
        }
        if let Some(value) = self.lower_fixed_bytes_literal(ty, expr) {
            return value;
        }
        if matches!(
            self.builder.func().value_ty(value),
            Some(MirType::MemoryObject(MemoryObjectKind::Bytes))
        ) {
            let zero = self.builder.imm_u64(0);
            return self.builder.memory_object_load_element(value, MemoryObjectLayout::Bytes, zero);
        }
        value
    }
}
