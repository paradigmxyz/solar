//! Statement-level HIR to MIR lowering.

use super::*;
use crate::mir::lower::data::stores_data_as_words;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_stmt(&mut self, stmt: &hir::Stmt<'_>) -> Option<()> {
        let previous = self.builder.replace_source_span(stmt.span);
        let previous_modifier_depth = self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_stmt_inner(stmt);
        self.builder.replace_modifier_depth(previous_modifier_depth);
        self.builder.replace_source_span(previous);
        result
    }

    fn lower_stmt_inner(&mut self, stmt: &hir::Stmt<'_>) -> Option<()> {
        match &stmt.kind {
            StmtKind::DeclSingle(id) => {
                let initializer = self.cx.gcx.hir.variable(*id).initializer;
                let ty = self.cx.gcx.type_of_item((*id).into());
                if ty.is_ref_at(DataLocation::Storage) {
                    let access = if let Some(initializer) = initializer {
                        self.require_storage_access(initializer)?
                    } else {
                        // An unassigned storage reference points at slot zero, like in solc.
                        // storage_ref = slot 0
                        StorageAccess {
                            slot: self.builder.imm(U256::ZERO),
                            location: StorageLocation::word(U256::ZERO),
                            offset: None,
                        }
                    };
                    self.storage_refs.insert(*id, access);
                    return Some(());
                }
                if initializer.is_none()
                    && let Some(layout) = self.types.memory_layout(ty)
                    && (!ty.is_ref_at(DataLocation::Memory)
                        || matches!(
                            layout,
                            MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. }
                        ))
                {
                    self.deferred_bindings.insert(*id);
                    return Some(());
                }
                let value = if !self.in_inline_assembly
                    && let Some(expr) = initializer
                    && let Ok(value) = self.cx.gcx.const_eval().eval_int(expr)
                    && let Some(value) = value.as_u256()
                {
                    let expr_ty = self.cx.gcx.type_of_expr(expr.id)?;
                    let value = self.builder.imm(value);
                    self.coerce_value(value, expr_ty, ty)
                } else if let Some(expr) = initializer {
                    if self.in_inline_assembly {
                        self.lower_yul_word_expr(expr)?
                    } else {
                        self.lower_typed_expr(expr, ty)?
                    }
                } else {
                    self.default_binding_value(ty)
                };
                let value = self.materialize_scalar_carrier(
                    *id,
                    value,
                    initializer.map_or(stmt.span, |expr| expr.span),
                )?;
                self.values.insert(*id, value);
            }
            StmtKind::DeclMulti(ids, expr) => {
                if ids.iter().flatten().any(|&id| {
                    // Memory declarations must also route through the copy
                    // path: the generic path would bind the callee's raw
                    // storage slot as if it were a memory pointer.
                    let ty = self.cx.gcx.type_of_item(id.into());
                    ty.is_ref_at(DataLocation::Storage) || ty.is_ref_at(DataLocation::Memory)
                }) && let Some(values) = self.lower_storage_reference_values(expr.peel_parens())
                {
                    if values.len() != ids.len() {
                        return self.cx.report_unsupported(expr.span, "storage reference tuple");
                    }
                    for (id, (value, source_ty, access)) in ids.iter().zip(values) {
                        let Some(id) = id else { continue };
                        let ty = self.cx.gcx.type_of_item((*id).into());
                        if let Some(access) = access {
                            if !ty.is_ref_at(DataLocation::Storage) {
                                // A storage-reference return declared into a
                                // non-storage local copies the referenced value;
                                // the reference itself only binds to a storage
                                // variable.
                                let span = self.cx.gcx.hir.variable(*id).span;
                                let value = self.load_storage_object(ty, value, span)?;
                                self.values.insert(*id, value);
                                continue;
                            }
                            self.storage_refs.insert(*id, access);
                        } else if ty.is_ref_at(DataLocation::Storage) {
                            return self.cx.report_unsupported(
                                self.cx.gcx.hir.variable(*id).span,
                                "mixed storage tuple",
                            );
                        } else {
                            let value =
                                self.convert_tuple_component(value, source_ty, ty, expr.span)?;
                            let value = self.materialize_raw_scalar(*id, value);
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
                                return self
                                    .cx
                                    .report_unsupported(expr.span, "tuple declaration value");
                            }
                            continue;
                        };
                        let Some(id) = id else {
                            // The declaration drops this component, so its value needs no read.
                            self.lower_discarded_expr(value)?;
                            continue;
                        };
                        let ty = self.cx.gcx.type_of_item((*id).into());
                        if ty.is_ref_at(DataLocation::Storage) {
                            let access = self.require_storage_access(value)?;
                            self.storage_refs.insert(*id, access);
                            continue;
                        }
                        let span = value.span;
                        let value = self.lower_typed_expr(value, ty)?;
                        let value = self.materialize_scalar_carrier(*id, value, span)?;
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                if let Some(builtin) = self.low_level_call_builtin(expr) {
                    let values = self.lower_low_level_call_values(
                        expr,
                        builtin,
                        ids.iter().flatten().count(),
                        ids.first().is_some_and(Option::is_none),
                    )?;
                    for (id, value) in ids.iter().flatten().zip(values) {
                        let value = self.materialize_raw_scalar(*id, value);
                        self.values.insert(*id, value);
                    }
                    return Some(());
                }
                let values = self.lower_values(expr)?;
                if values.len() != ids.len() {
                    return self.cx.report_unsupported(expr.span, "tuple declaration arity");
                }
                let source_ty = self.cx.gcx.type_of_expr(expr.id)?;
                let sources = match source_ty.kind {
                    TyKind::Tuple(sources) => sources,
                    _ => std::slice::from_ref(&source_ty),
                };
                for ((id, value), &source) in ids.iter().zip(values).zip(sources) {
                    if let Some(id) = id {
                        let target = self.cx.gcx.type_of_item((*id).into());
                        let value =
                            self.convert_tuple_component(value, source, target, expr.span)?;
                        let value = self.materialize_raw_scalar(*id, value);
                        self.values.insert(*id, value);
                    }
                }
            }
            StmtKind::Expr(expr) => {
                let expr = expr.peel_parens();
                if let ExprKind::Assign(lhs, None, rhs) = &expr.kind
                    && self.is_constant_storage_assignment(lhs, rhs)
                {
                    self.lower_constant_storage_assignment(lhs, rhs)?;
                    return Some(());
                }
                self.lower_discarded_expr(expr)?;
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
                    return self.cx.report_unsupported(stmt.span, "break outside loop");
                };
                let state = self.snapshot_loop_state(self.builder.current_block());
                self.loops.last_mut().expect("loop target exists").break_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Continue => {
                let Some(target) = self.loops.last().map(|targets| targets.continue_block) else {
                    return self.cx.report_unsupported(stmt.span, "continue outside loop");
                };
                let state = self.snapshot_loop_state(self.builder.current_block());
                self.loops.last_mut().expect("loop target exists").continue_states.push(state);
                self.builder.jump(target);
            }
            StmtKind::Return(expr) => {
                self.materialize_default_bindings();
                let values =
                    expr.map_or_else(|| Some(Vec::new()), |expr| self.lower_return_values(expr))?;
                if !values.is_empty() && values.len() != self.returns.len() {
                    return self.cx.report_unsupported(stmt.span, "return value count");
                }
                if let Some(target) = self.return_targets.last().map(|target| target.block) {
                    if !values.is_empty() {
                        let return_ids = self.returns.clone();
                        for (id, value) in return_ids.into_iter().zip(values) {
                            let ty = self.cx.gcx.type_of_item(id.into());
                            if ty.is_ref_at(DataLocation::Storage) {
                                self.set_storage_ref_slot(id, value);
                            } else {
                                let value =
                                    self.materialize_scalar_carrier(id, value, stmt.span)?;
                                self.values.insert(id, value);
                            }
                        }
                    }
                    self.record_return_state();
                    self.builder.jump(target);
                } else if !self.is_terminated() {
                    if values.is_empty() {
                        let returns = self.returns.clone();
                        self.finish(&returns)?;
                    } else {
                        let return_ids = self.returns.clone();
                        let values = return_ids
                            .into_iter()
                            .zip(values)
                            .map(|(id, value)| {
                                let ty = self.cx.gcx.type_of_item(id.into());
                                if ty.is_ref_at(DataLocation::Storage) {
                                    Some(value)
                                } else {
                                    self.materialize_scalar_carrier(id, value, stmt.span)
                                }
                            })
                            .collect::<Option<Vec<_>>>()?;
                        self.builder.ret(values);
                    }
                }
            }
            StmtKind::Revert(expr) => self.lower_revert_payload(expr)?,
            StmtKind::AssemblyBlock(block, memory_safe) => {
                let previous = std::mem::replace(&mut self.in_inline_assembly, true);
                let previous_writes = std::mem::take(&mut self.assembly_writes_memory);
                let result = self.lower_block(*block);
                // A block that writes no memory cannot break the promise.
                if !memory_safe && self.assembly_writes_memory {
                    self.cx.module.set_unsafe_assembly(true);
                }
                self.assembly_writes_memory = previous_writes;
                self.in_inline_assembly = previous;
                result?;
            }
            StmtKind::Placeholder => {
                self.lower_modifier_placeholder(stmt.span)?;
            }
            StmtKind::Emit(expr) => self.lower_emit(expr)?,
            StmtKind::Try(try_stmt) => self.lower_try(try_stmt)?,
            StmtKind::Err(_) => {
                return self.cx.report_unsupported(stmt.span, "statement");
            }
        }
        Some(())
    }

    /// Returns `true` if `--revert-strings strip` drops the reason string `expr`.
    ///
    /// Custom error payloads are never stripped. A reason that is not a compile-time constant
    /// string is still evaluated so its side effects and failures are kept; only the payload is
    /// dropped.
    pub(super) fn strips_revert_string(&mut self, expr: &hir::Expr<'_>) -> Option<bool> {
        if !self.cx.gcx.sess.opts.revert_strings.is_strip() {
            return Some(false);
        }
        if let ExprKind::Call(callee, ..) = &expr.peel_parens().kind
            && let Some(hir::Res::Item(hir::ItemId::Error(_))) = self.cx.gcx.resolved_expr(callee)
        {
            return Some(false);
        }
        if self.constant_bytes(expr).is_none() {
            self.lower_discarded_expr(expr)?;
        }
        Some(true)
    }

    /// Lowers an expression whose value is dropped, such as a tuple declaration or assignment
    /// hole, so that it can skip the reads only its value would need.
    pub(super) fn lower_discarded_expr(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let previous = self.builder.replace_source_span(expr.span);
        let previous_modifier_depth = self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_discarded_expr_inner(expr.peel_parens());
        self.builder.replace_modifier_depth(previous_modifier_depth);
        self.builder.replace_source_span(previous);
        result
    }

    fn lower_discarded_expr_inner(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        // Reading a constant with a value has no effects. One without a value runs its helper,
        // which can revert.
        let gcx = self.cx.gcx;
        if let Some(id) = gcx.resolved_variable(expr)
            && gcx.hir.variable(id).is_constant()
        {
            if gcx.const_eval().eval_var(id).is_err() {
                return self.call_constant_helper(id).map(drop);
            }
            return Some(());
        }
        match &expr.kind {
            // Names and `new T` have no effects to evaluate.
            ExprKind::Ident(_) | ExprKind::New(_) => Some(()),
            // Neither does literal arithmetic, whose fractions have no runtime value.
            _ if expr.is_numeric_literal() => Some(()),
            ExprKind::Call(callee, args) => {
                let (callee, options) = callee.split_call_options();
                self.lower_call(expr, callee, *args, options, false).map(drop)
            }
            ExprKind::Delete(value) => self.delete_lvalue(value),
            ExprKind::Assign(lhs, None, rhs) => {
                if let ExprKind::Tuple(elements) = &lhs.peel_parens().kind {
                    self.lower_tuple_assignment(elements, rhs)
                } else if !self.is_storage_reference_binding(lhs)
                    && self.type_of_expr_or_variable(lhs)?.is_ref_at(DataLocation::Storage)
                    && self.cx.gcx.type_of_expr(rhs.id)?.is_ref_at(DataLocation::Storage)
                {
                    self.storage_access(expr).map(drop)
                } else {
                    self.lower_expr(expr).map(drop)
                }
            }
            ExprKind::Tuple(values) => {
                for value in values.iter().flatten() {
                    self.lower_discarded_expr(value)?;
                }
                Some(())
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                let condition = self.lower_expr(condition)?;
                let ty = self.cx.gcx.type_of_expr(expr.id)?;
                let lower_branch = |this: &mut Self, branch| {
                    if ty.is_ref_at(DataLocation::Memory) {
                        this.lower_ternary_value(branch, ty).map(drop)
                    } else {
                        this.lower_discarded_expr(branch)
                    }
                };
                self.lower_branches(
                    condition,
                    true,
                    |this| lower_branch(this, then_expr),
                    |this| lower_branch(this, else_expr),
                )?;
                Some(())
            }
            ExprKind::CallOptions(callee, options) => {
                self.lower_discarded_expr(callee)?;
                for option in options.args {
                    self.lower_discarded_expr(&option.value)?;
                }
                Some(())
            }
            ExprKind::Member(receiver, _)
                if self.cx.gcx.resolved_builtin(expr) == Some(Builtin::ArrayPop) =>
            {
                self.require_storage_access(receiver).map(drop)
            }
            // A member function, such as `to.transfer`, or a member of an item only evaluates its
            // receiver. Function-typed fields are still read: calldata reads validate them.
            ExprKind::Member(receiver, _)
                if self.is_member_function(expr) || self.is_item_reference(receiver) =>
            {
                self.lower_discarded_expr(receiver)
            }
            // Types, modules, events, and errors have no effects to evaluate.
            _ if self.is_item_reference(expr) => Some(()),
            _ => self.lower_expr(expr).map(drop),
        }
    }

    /// Returns whether `expr` names a function or a builtin function, such as `this.f` or
    /// `to.transfer`, rather than a function-typed value.
    fn is_member_function(&self, expr: &hir::Expr<'_>) -> bool {
        let gcx = self.cx.gcx;
        matches!(
            gcx.resolved_expr(expr),
            Some(hir::Res::Item(hir::ItemId::Function(_)) | hir::Res::Builtin(_))
        ) && gcx.type_of_expr(expr.id).is_some_and(|ty| matches!(ty.kind, TyKind::Fn(_)))
    }

    /// Returns whether `expr` names a type, module, event, or error rather than a runtime value.
    fn is_item_reference(&self, expr: &hir::Expr<'_>) -> bool {
        self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| {
            matches!(
                ty.kind,
                TyKind::Type(_)
                    | TyKind::Meta(_)
                    | TyKind::Module(_)
                    | TyKind::BuiltinModule(_)
                    | TyKind::Event(..)
                    | TyKind::Error(..)
            )
        })
    }

    pub(super) fn lower_revert_payload(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let payload = self.prepare_revert_payload(expr)?;
        self.emit_revert_payload(payload);
        Some(())
    }

    pub(super) fn prepare_revert_payload(&mut self, expr: &hir::Expr<'_>) -> Option<RevertPayload> {
        if let Some((callee, args, _)) = expr.peel_parens().as_call()
            && let Some(hir::Res::Item(hir::ItemId::Error(error_id))) =
                self.cx.gcx.resolved_expr(callee)
        {
            return self.prepare_custom_error_payload(error_id, *args);
        }

        if let Some(symbol) = self.constant_bytes(expr) {
            let bytes = symbol.as_byte_str();
            return Some(match bytes.len() {
                0 => RevertPayload::EmptyString,
                1..=32 => {
                    let length = self.builder.imm(bytes.len() as u64);
                    let data = self.lower_string_literal_word(bytes);
                    RevertPayload::ShortString { length, data }
                }
                _ => RevertPayload::LongString(symbol),
            });
        }

        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        let memory_ty = ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        let value = self.lower_typed_expr(expr, memory_ty)?;
        let value = self.materialize_memory_argument(memory_ty, value, expr.span)?;
        Some(RevertPayload::ErrorString(value))
    }

    pub(super) fn emit_revert_payload(&mut self, payload: RevertPayload) {
        // require false, payload; unreachable
        let condition = self.builder.imm_bool(false);
        self.builder.require(condition, payload);
        self.builder.invalid();
    }

    fn prepare_custom_error_payload(
        &mut self,
        error_id: hir::ErrorId,
        args: hir::CallArgs<'_>,
    ) -> Option<RevertPayload> {
        let parameters = self.cx.gcx.item_parameters(hir::ItemId::Error(error_id));
        if args.len() != parameters.len() {
            return self.cx.report_unsupported(args.span, "error argument list");
        }
        let parameter_names =
            self.cx.gcx.callable_param_names(CallableParamSource::Error(error_id));
        let arguments = self.lower_call_arguments(
            args,
            CallArgumentParams {
                count: parameters.len(),
                names: Some(parameter_names.as_slice()),
                reverse: false,
            },
            args.span,
            "error argument",
            |this, index, argument| {
                let parameter_ty = this.cx.gcx.type_of_item(parameters[index].into());
                let (mut value, abi_type) = this.lower_abi_call_argument(argument, parameter_ty)?;
                if matches!(abi_type, AbiType::Word(_)) {
                    value = this.lower_word_value(parameter_ty, argument, value);
                }
                Some((value, abi_type))
            },
        )?;
        let (values, types): (Vec<_>, Vec<_>) = arguments.into_iter().unzip();
        let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
        let selector = self
            .builder
            .imm(U256::from_be_slice(&self.cx.gcx.function_selector(error_id).0) << 224);
        Some(RevertPayload::CustomError { selector, layout, values: values.into_boxed_slice() })
    }

    pub(super) fn lower_emit(&mut self, expr: &hir::Expr<'_>) -> Option<()> {
        let Some((callee, args, _)) = expr.peel_parens().as_call() else {
            return self.cx.report_unsupported(expr.span, "event emission");
        };
        let Some(hir::Res::Item(hir::ItemId::Event(event_id))) = self.cx.gcx.resolved_expr(callee)
        else {
            return self.cx.report_unsupported(expr.span, "event emission");
        };

        let event = self.cx.gcx.hir.event(event_id);
        let indexed_count =
            event.parameters.iter().filter(|&&id| self.cx.gcx.hir.variable(id).indexed).count();
        if args.len() != event.parameters.len() {
            return self.cx.report_unsupported(args.span, "event argument list");
        }

        let parameter_names =
            self.cx.gcx.callable_param_names(CallableParamSource::Event(event_id));
        // topics = anonymous ? [] : [event_selector]
        let mut topics = Vec::with_capacity(indexed_count + usize::from(!event.anonymous));
        if !event.anonymous {
            topics.push(
                self.builder
                    .imm(U256::from_be_slice(self.cx.gcx.event_selector(event_id).as_slice())),
            );
        }
        let arguments = self.lower_call_arguments(
            *args,
            CallArgumentParams {
                count: event.parameters.len(),
                names: Some(parameter_names.as_slice()),
                reverse: false,
            },
            args.span,
            "event argument",
            |this, index, argument| {
                let parameter = event.parameters[index];
                let parameter_ty = this.cx.gcx.type_of_item(parameter.into());
                // A literal stored as words in the event's data needs no object of its own: the
                // encoder writes its words.
                if !this.cx.gcx.hir.variable(parameter).indexed
                    && matches!(
                        parameter_ty.peel_refs().kind,
                        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                    )
                    && let ExprKind::Lit(lit) =
                        this.peel_bytes_conversion(argument).peel_parens().kind
                    && let LitKind::Str(_, bytes, _) = &lit.kind
                    && !bytes.as_byte_str().is_empty()
                    && stores_data_as_words(
                        this.cx.module.target_of(this.cx.gcx, this.builder.func()),
                        bytes.as_byte_str(),
                    )
                {
                    let value = Self::build_bytes_literal(
                        this.cx.gcx,
                        this.cx.module,
                        &mut this.builder,
                        bytes.as_byte_str(),
                        AllocationSemantics::INTERNAL,
                        false,
                    )?;
                    return Some((argument, value));
                }
                let value = this.lower_typed_expr(argument, parameter_ty)?;
                Some((argument, value))
            },
        )?;
        let mut data_values = Vec::new();
        let mut data_types = Vec::new();
        for (index, &parameter) in event.parameters.iter().enumerate() {
            let (argument, mut value) = arguments[index];
            let parameter_ty = self.cx.gcx.type_of_item(parameter.into());
            let variable = self.cx.gcx.hir.variable(parameter);
            if variable.indexed {
                // topics += encode_indexed(argument)
                match parameter_ty.peel_refs().kind {
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
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
                        self.validate_calldata_static_argument(value, parameter_ty);
                        let calldata_dynamic = self.builder.func().value_slice_location(value)
                            == Some(PtrLocation::Calldata)
                            && abi_type.is_dynamic();
                        if calldata_dynamic {
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
                            return self.cx.report_unsupported(
                                argument.span,
                                "indexed event aggregate encoding",
                            );
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
                // data_values += argument
                let mut abi_type = self.types.abi_type(parameter_ty)?;
                abi_type = self.abi_type_for_value(value, abi_type);
                self.check_calldata_array_size(value, parameter_ty, &abi_type);
                self.validate_calldata_static_argument(value, parameter_ty);
                // The dispatcher validates the elements of an argument that only ABI decoding
                // passes when an encoding takes it.
                if Self::is_calldata_word_array(&abi_type)
                    && self.needs_calldata_aggregate_validation(value, parameter_ty)
                    && !self.is_decoded_external_argument(value)
                {
                    self.validate_calldata_word_array(value, parameter_ty);
                }
                if matches!(abi_type, AbiType::Word(_)) {
                    value = self.lower_word_value(parameter_ty, argument, value);
                }
                data_values.push(value);
                data_types.push(abi_type);
            }
        }

        // data = abi_encode_scratch(non_indexed_arguments)
        let (data_ptr, data_size) = if data_types.is_empty() {
            let zero = self.builder.imm(U256::ZERO);
            (zero, zero)
        } else if data_types.len() <= 2
            && data_types.iter().all(|ty| matches!(ty, AbiType::Word(_)))
        {
            // mstore 0, a
            // mstore 32, b
            for (index, &value) in data_values.iter().enumerate() {
                let offset = self.builder.imm(32 * index as u64);
                self.builder.store(offset, value);
            }
            (self.builder.imm(0), self.builder.imm(32 * data_values.len() as u64))
        } else {
            // The log reads the data at once, so it stays unreserved past the free memory
            // pointer, as with solc.
            let layout = Arc::new(AbiLayout::new(data_types.into_boxed_slice()));
            let encoded =
                self.builder.abi_encode_scratch(layout, None, data_values.into_boxed_slice());
            (self.builder.slice_ptr(encoded), self.builder.slice_len(encoded))
        };
        // log data.data, data.length, topics...
        if topics.len() > LogTopics::MAX {
            return self.cx.report_unsupported(args.span, "event topic list");
        }
        self.builder.log(data_ptr, data_size, &topics);
        Some(())
    }

    pub(super) fn lower_block(&mut self, block: hir::Block<'_>) -> Option<()> {
        let mut stmts = block.stmts.iter().peekable();
        while let Some(stmt) = stmts.next() {
            if self.is_terminated() {
                break;
            }
            if let Some(args) = self.immediate_packed_hash_return_args(stmt, stmts.peek().copied())
            {
                let hash = self.lower_keccak_abi_encode_packed(args)?;
                self.builder.ret(vec![hash]);
                break;
            }
            self.lower_stmt(stmt)?;
        }
        Some(())
    }

    fn immediate_packed_hash_return_args(
        &self,
        stmt: &hir::Stmt<'_>,
        next: Option<&hir::Stmt<'_>>,
    ) -> Option<hir::CallArgs<'gcx>> {
        if !self.return_targets.is_empty() {
            return None;
        }
        let StmtKind::DeclSingle(id) = stmt.kind else { return None };
        let variable = self.cx.gcx.hir.variable(id);
        let ty = self.cx.gcx.type_of_item(id.into());
        if !ty.is_ref_at(DataLocation::Memory) || !self.is_dynamic_bytes_type(ty) {
            return None;
        }

        let ExprKind::Call(callee, args) = &variable.initializer?.peel_parens().kind else {
            return None;
        };
        if self.cx.gcx.resolved_builtin(callee) != Some(Builtin::AbiEncodePacked) {
            return None;
        }
        let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, args)?;
        if !exprs.iter().all(|expr| self.is_scratch_packed_expr(expr)) {
            return None;
        }

        let StmtKind::Return(Some(expr)) = &next?.kind else { return None };
        let (callee, hash_args, _) = expr.peel_parens().as_call()?;
        match (self.cx.gcx.resolved_builtin(callee), hash_args.kind, self.returns.as_slice()) {
            (Some(Builtin::Keccak256), hir::CallArgsKind::Unnamed([arg]), [_])
                if self.cx.gcx.resolved_variable(arg) == Some(id) =>
            {
                Some(*args)
            }
            _ => None,
        }
    }

    pub(super) fn lower_word_value(
        &mut self,
        ty: Ty<'gcx>,
        expr: &hir::Expr<'_>,
        value: ValueId,
    ) -> ValueId {
        let expr = expr.peel_parens();
        let value = self.normalize_dirty_scalar(value, ty);
        if let TyKind::Fn(function) = ty.peel_refs().kind
            && function.is_external()
        {
            let shift = self.builder.imm(64);
            return self.builder.shl_folded(value, shift);
        }
        if !matches!(ty.peel_refs().kind, TyKind::Elementary(ElementaryType::FixedBytes(_))) {
            let layout = types::TypeLowerer::value_layout(ty);
            return if matches!(layout, crate::mir::ValueLayout::Int(_)) {
                raw_scalars::cast_carrier(&mut self.builder, value, layout, MirType::I256)
            } else {
                value
            };
        }
        if let Some(value) = self.lower_fixed_bytes_literal(ty, expr) {
            return value;
        }
        if matches!(self.builder.func().value_ty(value), Some(MirType::MEMPTR)) {
            let zero = self.builder.imm(0);
            return self.builder.slice_load_word(value, zero);
        }
        value
    }
}
