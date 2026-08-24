//! Branch, loop, ternary, and state-merge lowering.

use super::*;

/// A `try` statement target resolved to the callee shape needed for lowering.
struct TryTarget<'a, 'gcx> {
    /// Creation metadata when the target is `new Contract(...)`.
    creation: Option<(&'a hir::Type<'a>, hir::ContractId)>,
    /// ABI parameter types of the target call.
    parameter_types: Vec<Ty<'gcx>>,
    /// Return types of the target call.
    return_types: Vec<Ty<'gcx>>,
    /// Parameter names for named-argument resolution.
    parameter_names: Option<CallableParamNames>,
    /// Computed selector value for external function pointer targets.
    selector: Option<ValueId>,
    /// Raw four-byte selector for member-call targets.
    selector_bytes: Option<[u8; 4]>,
    /// Computed address value for external function pointer targets.
    address: Option<ValueId>,
    /// Receiver expression for member-call targets.
    receiver: Option<&'a hir::Expr<'a>>,
    /// Whether the call must use STATICCALL.
    static_call: bool,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn snapshot_loop_state(&self, block: BlockId) -> LoopState {
        LoopState { block, values: self.values.clone(), storage_refs: self.storage_refs.clone() }
    }

    pub(super) fn lower_if(
        &mut self,
        condition: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) -> Option<()> {
        let condition = self.lower_expr(condition)?;
        self.materialize_default_bindings();
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        self.lower_stmt(then_stmt)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = std::mem::take(&mut self.values);
        let then_storage_refs = std::mem::take(&mut self.storage_refs);
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(else_block);
        if let Some(stmt) = else_stmt {
            self.lower_stmt(stmt)?;
        }
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = std::mem::take(&mut self.values);
        let else_storage_refs = std::mem::take(&mut self.storage_refs);
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before,
            MergeBranch { block: then_exit, values: then_values, terminated: then_terminated },
            MergeBranch { block: else_exit, values: else_values, terminated: else_terminated },
        );
        self.storage_refs = self.merge_storage_refs(
            before_storage_refs,
            MergeBranch {
                block: then_exit,
                values: then_storage_refs,
                terminated: then_terminated,
            },
            MergeBranch {
                block: else_exit,
                values: else_storage_refs,
                terminated: else_terminated,
            },
        );
        if then_terminated && else_terminated {
            self.builder.invalid();
        }
        Some(())
    }

    pub(super) fn lower_switch(&mut self, switch: &hir::StmtSwitch<'_>) -> Option<()> {
        let selector = self.lower_yul_word_expr(switch.selector)?;
        self.materialize_default_bindings();
        let switch_block = self.builder.current_block();
        let merge_block = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        let mut case_blocks = Vec::new();
        let mut body_blocks = Vec::new();
        let mut default_block = merge_block;
        let mut has_default = false;

        for case in switch.cases {
            let block = self.builder.create_block();
            if let Some(constant) = case.constant {
                let value = self.lower_word_literal(constant)?;
                case_blocks.push((value, block));
            } else {
                default_block = block;
                has_default = true;
            }
            body_blocks.push((case, block));
        }
        self.builder.switch(selector, default_block, case_blocks);

        let mut states = Vec::with_capacity(body_blocks.len() + usize::from(!has_default));
        if !has_default {
            states.push(LoopState {
                block: switch_block,
                values: before_values.clone(),
                storage_refs: before_storage_refs.clone(),
            });
        }
        for (case, block) in body_blocks {
            self.values = before_values.clone();
            self.storage_refs = before_storage_refs.clone();
            self.builder.switch_to_block(block);
            self.lower_block(case.body)?;
            let terminated = self.is_terminated();
            let exit = self.builder.current_block();
            if !terminated {
                self.builder.jump(merge_block);
                states.push(self.snapshot_loop_state(exit));
            }
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_loop_values(before_values, &states, &FxHashMap::default());
        self.storage_refs = self.merge_storage_ref_states(before_storage_refs, &states);
        Some(())
    }

    pub(super) fn lower_try(&mut self, try_stmt: &hir::StmtTry<'_>) -> Option<()> {
        let ExprKind::Call(callee, args, call_opts) = &try_stmt.expr.kind else {
            return report_unsupported(self.context.gcx, try_stmt.expr.span, "try expression");
        };
        let target = if let ExprKind::New(ty) = &callee.kind {
            let TyKind::Contract(contract_id) = self.context.gcx.type_of_hir_ty(ty).kind else {
                return report_unsupported(self.context.gcx, try_stmt.expr.span, "try target");
            };
            let contract = self.context.gcx.hir.contract(contract_id);
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
            TryTarget {
                creation: Some((ty, contract_id)),
                parameter_types: parameters
                    .iter()
                    .map(|&parameter| self.context.gcx.type_of_item(parameter.into()))
                    .collect(),
                return_types: Vec::new(),
                parameter_names: Some(parameter_names),
                selector: None,
                selector_bytes: None,
                address: None,
                receiver: None,
                static_call: false,
            }
        } else if let ExprKind::Member(receiver, _) = callee.kind {
            let Some(function_id) = self.context.gcx.resolved_function(callee) else {
                return report_unsupported(self.context.gcx, try_stmt.expr.span, "try target");
            };
            let function = self.context.gcx.hir.function(function_id);
            TryTarget {
                creation: None,
                parameter_types: function
                    .parameters
                    .iter()
                    .map(|&parameter| self.context.gcx.type_of_item(parameter.into()))
                    .collect(),
                return_types: function
                    .returns
                    .iter()
                    .map(|&return_id| self.context.gcx.type_of_item(return_id.into()))
                    .collect(),
                parameter_names: Some(self.context.gcx.callable_param_names(
                    CallableParamSource::Function { id: function_id, skips_receiver: false },
                )),
                selector: None,
                selector_bytes: Some(self.context.gcx.function_selector(function_id).0),
                address: None,
                receiver: Some(receiver),
                static_call: matches!(
                    function.state_mutability,
                    hir::StateMutability::Pure | hir::StateMutability::View
                ) && self.context.gcx.sess.opts.evm_version.has_static_call(),
            }
        } else if let Some(TyKind::Fn(function)) =
            self.context.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            let function_value = self.lower_expr(callee)?;
            let (address, selector) = self.split_external_function_pointer(function_value);
            TryTarget {
                creation: None,
                parameter_types: function.parameters.to_vec(),
                return_types: function.returns.to_vec(),
                parameter_names: None,
                selector: Some(selector),
                selector_bytes: None,
                address: Some(address),
                receiver: None,
                static_call: matches!(
                    function.state_mutability,
                    hir::StateMutability::Pure | hir::StateMutability::View
                ) && self.context.gcx.sess.opts.evm_version.has_static_call(),
            }
        } else {
            return report_unsupported(self.context.gcx, try_stmt.expr.span, "try target");
        };
        let Some((returns_clause, catch_clauses)) = try_stmt.clauses.split_first() else {
            return report_unsupported(self.context.gcx, try_stmt.expr.span, "try/catch clauses");
        };
        if catch_clauses.is_empty() {
            return report_unsupported(self.context.gcx, try_stmt.expr.span, "try/catch clauses");
        }
        let creation_binding = if target.creation.is_some() {
            if returns_clause.name.is_some() || returns_clause.args.len() > 1 {
                return report_unsupported(
                    self.context.gcx,
                    returns_clause.span,
                    "try return bindings",
                );
            }
            returns_clause.args.first().copied()
        } else {
            if returns_clause.name.is_some()
                || (!returns_clause.args.is_empty()
                    && returns_clause.args.len() != target.return_types.len())
            {
                return report_unsupported(
                    self.context.gcx,
                    returns_clause.span,
                    "try return bindings",
                );
            }
            None
        };
        for catch_clause in catch_clauses {
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            if catch_clause.name.is_some() && !catch_error && !catch_panic {
                return report_unsupported(self.context.gcx, catch_clause.span, "try catch clause");
            }
            if catch_clause.args.len() > 1 {
                return report_unsupported(self.context.gcx, catch_clause.span, "try catch clause");
            }
            if let Some(&binding) = catch_clause.args.first() {
                let ty = self.context.gcx.type_of_item(binding.into());
                let expected = if catch_error {
                    TyKind::Elementary(ElementaryType::String)
                } else if catch_panic {
                    TyKind::Elementary(ElementaryType::UInt(TypeSize::new_int_bits(256)))
                } else {
                    TyKind::Elementary(ElementaryType::Bytes)
                };
                if ty.peel_refs().kind != expected {
                    return report_unsupported(
                        self.context.gcx,
                        catch_clause.span,
                        "try catch clause",
                    );
                }
                if !self.context.gcx.sess.opts.evm_version.supports_returndata() {
                    return report_error(
                        self.context.gcx,
                        try_stmt.expr.span,
                        "codegen cannot bind try/catch returndata before Byzantium",
                    );
                }
            }
        }
        if args.len() != target.parameter_types.len() {
            return report_unsupported(self.context.gcx, args.span, "try arguments");
        }

        let (success, creation_value) = if let Some((ty, contract_id)) = target.creation {
            let created = self.lower_create_contract(ty, contract_id, *args, *call_opts)?;
            let zero = self.builder.imm_u256(U256::ZERO);
            let failed = self.builder.eq(created, zero);
            (self.builder.iszero(failed), Some(created))
        } else {
            let address = if let Some(address) = target.address {
                address
            } else {
                let Some(receiver) = target.receiver else {
                    return report_unsupported(
                        self.context.gcx,
                        try_stmt.expr.span,
                        "try receiver",
                    );
                };
                self.lower_expr(receiver)?
            };
            let (gas, call_value, zero) =
                self.lower_call_options(*call_opts, true, "try call option")?;
            let mut values = Vec::with_capacity(target.parameter_types.len());
            let mut types = Vec::with_capacity(target.parameter_types.len());
            for (index, parameter_ty) in target.parameter_types.iter().copied().enumerate() {
                let Some(argument) =
                    args.argument_for_parameter(index, target.parameter_names.as_deref())
                else {
                    return report_unsupported(self.context.gcx, args.span, "try argument");
                };
                let (value, abi_type) = self.lower_abi_call_argument(argument, parameter_ty)?;
                values.push(value);
                types.push(abi_type);
            }
            let selector = if let Some(selector) = target.selector {
                selector
            } else {
                let Some(selector_bytes) = target.selector_bytes else {
                    return report_unsupported(
                        self.context.gcx,
                        try_stmt.expr.span,
                        "try selector",
                    );
                };
                self.builder.imm_u256(U256::from_be_slice(&selector_bytes) << 224)
            };
            let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
            let encoded =
                self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
            let input = self.builder.slice_ptr(encoded);
            let input_size = self.builder.slice_len(encoded);
            let ret_offset = zero;
            let ret_size = self.builder.imm_u64(0);
            let success = if target.static_call {
                self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
            } else {
                self.builder.call(gas, address, call_value, input, input_size, ret_offset, ret_size)
            };
            (success, None)
        };

        let success_block = self.builder.create_block();
        let catch_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.materialize_default_bindings();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.builder.branch(success, success_block, catch_block);

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(success_block);
        if let Some(binding) = creation_binding {
            let Some(value) = creation_value else {
                return report_unsupported(
                    self.context.gcx,
                    returns_clause.span,
                    "try return bindings",
                );
            };
            self.values.insert(binding, value);
        } else if !target.return_types.is_empty() && !returns_clause.args.is_empty() {
            let data = self.materialize_returndata_bytes();
            let values =
                self.lower_abi_decode_values(data, &target.return_types, returns_clause.span)?;
            for (&binding, value) in returns_clause.args.iter().zip(values) {
                self.values.insert(binding, value);
            }
        }
        self.lower_block(returns_clause.block)?;
        let success_terminated = self.is_terminated();
        let success_exit = self.builder.current_block();
        let success_values = std::mem::take(&mut self.values);
        let success_storage_refs = std::mem::take(&mut self.storage_refs);
        if !success_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(catch_block);
        let catch_data = self.materialize_returndata_bytes();
        let catch_data_ptr = self.builder.memory_object_data(catch_data, MemoryObjectKind::Bytes);
        let catch_data_len = self.builder.memory_object_len(catch_data, MemoryObjectKind::Bytes);
        let zero = self.builder.imm_u256(U256::ZERO);
        let selector_slice =
            self.builder.make_slice(catch_data_ptr, catch_data_len, SliceLocation::Memory);
        let selector_word = self.builder.memory_slice_load_word(selector_slice, zero);
        let four = self.builder.imm_u64(4);
        let selector_short = self.builder.lt(catch_data_len, four);
        let has_selector = self.builder.iszero(selector_short);
        let selector_shift = self.builder.imm_u64(224);
        let selector = self.builder.shr(selector_shift, selector_word);
        let error_selector =
            self.builder.imm_u256(U256::from_be_slice(&keccak256("Error(string)")[..4]));
        let panic_selector = self.builder.imm_u256(U256::from(0x4e48_7b71_u64));
        let error_selector_matches = self.builder.eq(selector, error_selector);
        let error_matches = self.builder.and(has_selector, error_selector_matches);
        let panic_size = self.builder.imm_u64(36);
        let panic_short = self.builder.lt(catch_data_len, panic_size);
        let panic_has_payload = self.builder.iszero(panic_short);
        let panic_selector_matches = self.builder.eq(selector, panic_selector);
        let panic_matches = self.builder.and(panic_has_payload, panic_selector_matches);
        let mut catch_states = Vec::with_capacity(catch_clauses.len());
        let mut next_catch = self.builder.current_block();
        for catch_clause in catch_clauses {
            self.builder.switch_to_block(next_catch);
            let clause_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            let condition = if catch_error {
                error_matches
            } else if catch_panic {
                panic_matches
            } else {
                self.builder.imm_bool(true)
            };
            self.builder.branch(condition, clause_block, next_block);

            self.values = before.clone();
            self.storage_refs = before_storage_refs.clone();
            self.builder.switch_to_block(clause_block);
            if let Some(&binding) = catch_clause.args.first() {
                let value = if catch_error {
                    self.lower_error_catch_string(catch_data)?
                } else if catch_panic {
                    self.lower_panic_catch_word(catch_data)
                } else {
                    catch_data
                };
                self.values.insert(binding, value);
            }
            self.lower_block(catch_clause.block)?;
            let catch_terminated = self.is_terminated();
            let catch_exit = self.builder.current_block();
            if !catch_terminated {
                self.builder.jump(merge_block);
                catch_states.push(LoopState {
                    block: catch_exit,
                    values: std::mem::take(&mut self.values),
                    storage_refs: std::mem::take(&mut self.storage_refs),
                });
            }
            next_catch = next_block;
        }
        self.builder.switch_to_block(next_catch);
        self.builder.revert(catch_data_ptr, catch_data_len);

        let mut states = Vec::with_capacity(catch_states.len() + 1);
        if !success_terminated {
            states.push(LoopState {
                block: success_exit,
                values: success_values,
                storage_refs: success_storage_refs,
            });
        }
        states.extend(catch_states);
        self.builder.switch_to_block(merge_block);
        self.values = self.merge_many_values(before, &states);
        self.storage_refs = self.merge_storage_ref_states(before_storage_refs, &states);
        Some(())
    }

    pub(super) fn lower_word_literal(&mut self, lit: &hir::Lit<'_>) -> Option<ValueId> {
        if let LitKind::Str(_, bytes, _) = &lit.kind {
            let bytes = bytes.as_byte_str();
            if bytes.len() > 32 {
                return report_unsupported(self.context.gcx, lit.span, "switch literal");
            }
            return Some(self.lower_string_literal_word(bytes));
        }
        if let LitKind::Bool(value) = lit.kind {
            return Some(self.builder.imm_u256(if value { U256::ONE } else { U256::ZERO }));
        }
        if let LitKind::Address(value) = lit.kind {
            return Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())));
        }
        self.lower_literal(lit.kind, lit.span)
    }

    pub(super) fn lower_yul_word_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        if let ExprKind::Lit(lit) = expr.peel_parens().kind {
            return self.lower_word_literal(lit);
        }
        self.lower_expr(expr)
    }

    pub(super) fn merge_storage_refs(
        &mut self,
        before: FxHashMap<VariableId, StorageAccess>,
        then_branch: MergeBranch<StorageAccess>,
        else_branch: MergeBranch<StorageAccess>,
    ) -> FxHashMap<VariableId, StorageAccess> {
        let mut merged = before;
        let ids = then_branch
            .values
            .keys()
            .chain(else_branch.values.keys())
            .copied()
            .collect::<solar_data_structures::map::FxHashSet<_>>();
        for id in ids {
            let then = then_branch.values.get(&id).copied().or_else(|| merged.get(&id).copied());
            let else_ = else_branch.values.get(&id).copied().or_else(|| merged.get(&id).copied());
            let mut incoming = Vec::with_capacity(2);
            if !then_branch.terminated
                && let Some(access) = then
            {
                incoming.push((then_branch.block, access));
            }
            if !else_branch.terminated
                && let Some(access) = else_
            {
                incoming.push((else_branch.block, access));
            }
            let access = self.merge_storage_accesses(incoming).or(then.or(else_));
            if let Some(access) = access {
                merged.insert(id, access);
            }
        }
        merged
    }

    pub(super) fn lower_ternary_branches<T>(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
        mut lower: impl FnMut(&mut Self, &hir::Expr<'_>) -> Option<T>,
    ) -> Option<(TernaryBranch<T>, TernaryBranch<T>)> {
        let condition = self.lower_expr(condition)?;
        self.materialize_default_bindings();
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.builder.branch(condition, then_block, else_block);

        self.values = before_values.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(then_block);
        let then_value = lower(self, then_expr)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = self.values.clone();
        let then_storage_refs = self.storage_refs.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before_values.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(else_block);
        let else_value = lower(self, else_expr)?;
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = std::mem::take(&mut self.values);
        let else_storage_refs = std::mem::take(&mut self.storage_refs);
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before_values,
            MergeBranch { block: then_exit, values: then_values, terminated: then_terminated },
            MergeBranch { block: else_exit, values: else_values, terminated: else_terminated },
        );
        self.storage_refs = self.merge_storage_refs(
            before_storage_refs,
            MergeBranch {
                block: then_exit,
                values: then_storage_refs,
                terminated: then_terminated,
            },
            MergeBranch {
                block: else_exit,
                values: else_storage_refs,
                terminated: else_terminated,
            },
        );
        Some((
            TernaryBranch { block: then_exit, value: then_value, terminated: then_terminated },
            TernaryBranch { block: else_exit, value: else_value, terminated: else_terminated },
        ))
    }

    pub(super) fn lower_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let (then_branch, else_branch) =
            self.lower_ternary_branches(condition, then_expr, else_expr, |this, expr| {
                this.lower_expr(expr)
            })?;
        match (then_branch.terminated, else_branch.terminated) {
            (true, false) => Some(else_branch.value),
            (false, true) => Some(then_branch.value),
            _ if then_branch.value == else_branch.value => Some(then_branch.value),
            _ => Some(self.merge_value_phi(vec![
                (then_branch.block, then_branch.value),
                (else_branch.block, else_branch.value),
            ])),
        }
    }

    pub(super) fn lower_logical(
        &mut self,
        lhs_expr: &hir::Expr<'_>,
        op: BinOpKind,
        rhs_expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let lhs = self.lower_expr(lhs_expr)?;
        self.materialize_default_bindings();
        let rhs_block = self.builder.create_block();
        let short_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        let is_and = op == BinOpKind::And;
        if is_and {
            self.builder.branch(lhs, rhs_block, short_block);
        } else {
            self.builder.branch(lhs, short_block, rhs_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(rhs_block);
        let rhs = self.lower_expr(rhs_expr)?;
        let rhs_terminated = self.is_terminated();
        let rhs_exit = self.builder.current_block();
        let rhs_values = self.values.clone();
        let rhs_storage_refs = self.storage_refs.clone();
        if !rhs_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(short_block);
        let short = self.builder.imm_bool(!is_and);
        let short_exit = self.builder.current_block();
        let short_values = self.values.clone();
        let short_storage_refs = self.storage_refs.clone();
        self.builder.jump(merge_block);

        self.builder.switch_to_block(merge_block);
        self.values = self.merge_values(
            before,
            MergeBranch { block: rhs_exit, values: rhs_values, terminated: rhs_terminated },
            MergeBranch { block: short_exit, values: short_values, terminated: false },
        );
        self.storage_refs = self.merge_storage_refs(
            before_storage_refs,
            MergeBranch { block: rhs_exit, values: rhs_storage_refs, terminated: rhs_terminated },
            MergeBranch { block: short_exit, values: short_storage_refs, terminated: false },
        );
        if rhs_terminated {
            return Some(short);
        }
        if rhs == short {
            Some(short)
        } else {
            Some(self.builder.phi(vec![(rhs_exit, rhs), (short_exit, short)]))
        }
    }

    pub(super) fn lower_ternary_values(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<Vec<ValueId>> {
        let (then_branch, else_branch) =
            self.lower_ternary_branches(condition, then_expr, else_expr, |this, expr| {
                this.lower_values(expr)
            })?;
        if !then_branch.terminated
            && !else_branch.terminated
            && then_branch.value.len() != else_branch.value.len()
        {
            return report_unsupported(self.context.gcx, then_expr.span, "ternary value count");
        }
        let values = match (then_branch.terminated, else_branch.terminated) {
            (true, false) => else_branch.value,
            (false, true) => then_branch.value,
            (true, true) => Vec::new(),
            (false, false) => then_branch
                .value
                .into_iter()
                .zip(else_branch.value)
                .map(|(then, else_)| {
                    if then == else_ {
                        then
                    } else {
                        self.merge_value_phi(vec![
                            (then_branch.block, then),
                            (else_branch.block, else_),
                        ])
                    }
                })
                .collect(),
        };
        Some(values)
    }

    pub(super) fn lower_loop(
        &mut self,
        mut block: hir::Block<'_>,
        source: LoopSource<'_>,
    ) -> Option<()> {
        self.materialize_default_bindings();
        let update_stmt = match source {
            LoopSource::For { update } => update,
            LoopSource::While => None,
            LoopSource::DoWhile => {
                let (condition, body) =
                    block.stmts.split_last().expect("do while loop has a condition");
                block.stmts = body;
                Some(condition)
            }
        };
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let exit = self.builder.create_block();
        let update = update_stmt.map(|_| self.builder.create_block());
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        let mut header_values = before_values.clone();
        let mut header_phis = FxHashMap::default();
        for (&id, &value) in &before_values {
            let phi = self.merge_value_phi(vec![(preheader, value)]);
            header_values.insert(id, phi);
            header_phis.insert(id, phi);
        }
        self.values = header_values.clone();
        let mut header_storage_refs = before_storage_refs.clone();
        for (&id, &access) in &before_storage_refs {
            let slot = self.builder.phi(vec![(preheader, access.slot)]);
            let offset = access.offset.map(|offset| self.builder.phi(vec![(preheader, offset)]));
            header_storage_refs.insert(id, StorageAccess { slot, offset, ..access });
        }
        self.storage_refs = header_storage_refs.clone();
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: update.unwrap_or(header),
            break_states: Vec::new(),
            continue_states: Vec::new(),
        });
        let update_state = if let Some(update_stmt) = update_stmt {
            self.lower_block(block)?;
            let normal_state = (!self.is_terminated())
                .then(|| self.snapshot_loop_state(self.builder.current_block()));
            if normal_state.is_some() {
                self.builder.jump(update.expect("for loop update block"));
            }

            let mut update_states = Vec::with_capacity(
                usize::from(normal_state.is_some())
                    + self.loops.last().expect("loop target exists").continue_states.len(),
            );
            if let Some(state) = normal_state {
                update_states.push(state);
            }
            update_states.extend(std::mem::take(
                &mut self.loops.last_mut().expect("loop target exists").continue_states,
            ));

            if update_states.is_empty() {
                let update = update.expect("loop update block exists");
                self.builder.switch_to_block(update);
                self.builder.invalid();
                None
            } else {
                self.builder.switch_to_block(update.expect("loop update block exists"));
                self.values = self.merge_loop_values(
                    header_values.clone(),
                    &update_states,
                    &FxHashMap::default(),
                );
                self.storage_refs =
                    self.merge_storage_ref_states(header_storage_refs.clone(), &update_states);
                if matches!(source, LoopSource::DoWhile) {
                    self.loops.last_mut().expect("loop target exists").continue_block = header;
                }
                self.lower_stmt(update_stmt)?;
                let update_state = (!self.is_terminated())
                    .then(|| self.snapshot_loop_state(self.builder.current_block()));
                if update_state.is_some() {
                    self.builder.jump(header);
                }
                update_state
            }
        } else {
            self.lower_block(block)?;
            let normal_state = (!self.is_terminated())
                .then(|| self.snapshot_loop_state(self.builder.current_block()));
            if normal_state.is_some() {
                self.builder.jump(header);
            }
            normal_state
        };
        let loop_targets = self.loops.pop().expect("loop target exists");
        if let Some(state) = &update_state {
            self.add_loop_phi_incoming(&header_phis, state);
            self.add_loop_storage_phi_incoming(&header_storage_refs, state);
        }
        if update_stmt.is_none() || matches!(source, LoopSource::DoWhile) {
            for state in &loop_targets.continue_states {
                self.add_loop_phi_incoming(&header_phis, state);
                self.add_loop_storage_phi_incoming(&header_storage_refs, state);
            }
        }
        self.builder.switch_to_block(exit);
        self.values =
            self.merge_loop_values(before_values, &loop_targets.break_states, &header_phis);
        self.storage_refs =
            self.merge_storage_ref_states(header_storage_refs, &loop_targets.break_states);
        Some(())
    }

    pub(super) fn add_loop_phi_incoming(
        &mut self,
        header_phis: &FxHashMap<VariableId, ValueId>,
        state: &LoopState,
    ) {
        for (&id, &phi) in header_phis {
            let value = state.values.get(&id).copied().unwrap_or(phi);
            self.builder.add_phi_incoming(phi, state.block, value);
            if !self.dirty_values.is_empty() && self.dirty_values.contains(&value) {
                self.dirty_values.insert(phi);
            }
        }
    }

    pub(super) fn add_loop_storage_phi_incoming(
        &mut self,
        header_refs: &FxHashMap<VariableId, StorageAccess>,
        state: &LoopState,
    ) {
        for (&id, &header) in header_refs {
            let access = state.storage_refs.get(&id).copied().unwrap_or(header);
            self.builder.add_phi_incoming(header.slot, state.block, access.slot);
            if let Some(offset) = header.offset {
                self.builder.add_phi_incoming(offset, state.block, access.offset.unwrap_or(offset));
            }
        }
    }

    pub(super) fn merge_loop_values(
        &mut self,
        before: FxHashMap<VariableId, ValueId>,
        exits: &[LoopState],
        header_phis: &FxHashMap<VariableId, ValueId>,
    ) -> FxHashMap<VariableId, ValueId> {
        let ids = before.keys().copied().collect::<Vec<_>>();
        let mut merged = before;
        for id in ids {
            let before_value = merged[&id];
            let incoming = exits
                .iter()
                .filter_map(|state| {
                    state.values.get(&id).copied().map(|value| (state.block, value))
                })
                .collect::<Vec<_>>();
            let value = match incoming.as_slice() {
                [] => header_phis.get(&id).copied().unwrap_or(before_value),
                [(_, value)] => *value,
                [(_, first), rest @ ..] if rest.iter().all(|(_, value)| value == first) => *first,
                _ => self.merge_value_phi(incoming),
            };
            merged.insert(id, value);
        }
        merged
    }

    pub(super) fn merge_storage_accesses(
        &mut self,
        incoming: Vec<(BlockId, StorageAccess)>,
    ) -> Option<StorageAccess> {
        let first = incoming.first().map(|&(_, access)| access)?;
        if incoming.iter().all(|&(_, access)| access == first) {
            return Some(first);
        }

        let slot = if incoming.iter().all(|&(_, access)| access.slot == first.slot) {
            first.slot
        } else {
            self.builder.phi(incoming.iter().map(|&(block, access)| (block, access.slot)).collect())
        };

        let offset = if incoming.iter().all(|&(_, access)| access.offset.is_none()) {
            None
        } else {
            let offsets = incoming
                .iter()
                .map(|&(_, access)| {
                    access
                        .offset
                        .unwrap_or_else(|| self.builder.imm_u64(u64::from(access.location.offset)))
                })
                .collect::<Vec<_>>();
            let first_offset = offsets[0];
            if offsets.iter().all(|&offset| offset == first_offset) {
                Some(first_offset)
            } else {
                Some(
                    self.builder.phi(
                        incoming
                            .iter()
                            .zip(offsets)
                            .map(|(&(block, _), offset)| (block, offset))
                            .collect(),
                    ),
                )
            }
        };
        Some(StorageAccess { slot, location: first.location, offset })
    }

    pub(super) fn merge_values(
        &mut self,
        before: FxHashMap<VariableId, ValueId>,
        then_branch: MergeBranch<ValueId>,
        else_branch: MergeBranch<ValueId>,
    ) -> FxHashMap<VariableId, ValueId> {
        let mut values = before;
        let mut ids = Vec::new();
        ids.extend(then_branch.values.keys().copied());
        ids.extend(else_branch.values.keys().copied());
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let then_value = then_branch.values.get(&id).copied();
            let else_value = else_branch.values.get(&id).copied();
            let value =
                match (then_branch.terminated, else_branch.terminated, then_value, else_value) {
                    (true, false, _, value) | (false, true, value, _) => value,
                    (_, _, Some(lhs), Some(rhs)) if lhs == rhs => Some(lhs),
                    (false, false, Some(lhs), Some(rhs)) => {
                        Some(self.merge_value_phi(vec![
                            (then_branch.block, lhs),
                            (else_branch.block, rhs),
                        ]))
                    }
                    _ => then_value.or(else_value),
                };
            if let Some(value) = value {
                values.insert(id, value);
            }
        }
        values
    }

    pub(super) fn merge_many_values(
        &mut self,
        mut before: FxHashMap<VariableId, ValueId>,
        states: &[LoopState],
    ) -> FxHashMap<VariableId, ValueId> {
        let mut ids =
            states.iter().flat_map(|state| state.values.keys().copied()).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let incoming = states
                .iter()
                .filter_map(|state| {
                    state
                        .values
                        .get(&id)
                        .copied()
                        .or_else(|| before.get(&id).copied())
                        .map(|value| (state.block, value))
                })
                .collect::<Vec<_>>();
            let value = match incoming.as_slice() {
                [] => None,
                [(_, value)] => Some(*value),
                [(_, first), rest @ ..] if rest.iter().all(|(_, value)| value == first) => {
                    Some(*first)
                }
                _ => Some(self.merge_value_phi(incoming)),
            };
            if let Some(value) = value {
                before.insert(id, value);
            }
        }
        before
    }

    fn merge_value_phi(&mut self, incoming: Vec<(BlockId, ValueId)>) -> ValueId {
        let dirty = !self.dirty_values.is_empty()
            && incoming.iter().any(|(_, value)| self.dirty_values.contains(value));
        let value = self.builder.phi(incoming);
        if dirty {
            self.dirty_values.insert(value);
        }
        value
    }

    pub(super) fn merge_storage_ref_states(
        &mut self,
        mut before: FxHashMap<VariableId, StorageAccess>,
        states: &[LoopState],
    ) -> FxHashMap<VariableId, StorageAccess> {
        let ids = states
            .iter()
            .flat_map(|state| state.storage_refs.keys())
            .copied()
            .collect::<solar_data_structures::map::FxHashSet<_>>();
        for id in ids {
            let fallback = before.get(&id).copied();
            let incoming = states
                .iter()
                .filter_map(|state| {
                    state
                        .storage_refs
                        .get(&id)
                        .copied()
                        .or(fallback)
                        .map(|access| (state.block, access))
                })
                .collect::<Vec<_>>();
            if let Some(access) = self.merge_storage_accesses(incoming).or(fallback) {
                before.insert(id, access);
            }
        }
        before
    }
}
