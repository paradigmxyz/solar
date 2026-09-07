//! Branch, loop, ternary, and state-merge lowering.

use super::{calls::ExternalReturnMode, *};

/// A `try` statement target resolved to the callee shape needed for lowering.
#[derive(Clone, Copy)]
enum TryCallee<'a> {
    Creation { ty: &'a hir::Type<'a>, contract_id: hir::ContractId },
    Member { receiver: &'a hir::Expr<'a>, selector: [u8; 4] },
    LinkedLibrary { address: U256, function: hir::FunctionId, receiver: Option<&'a hir::Expr<'a>> },
    FunctionPointer { address: ValueId, selector: ValueId },
}

/// The failed call's return data, which the catch clauses match on and bind.
///
/// Absent before Byzantium, where the only lowerable clause is a bare `catch { }` and there is no
/// `RETURNDATACOPY` to read the data with.
#[derive(Clone, Copy)]
struct TryCatchData {
    /// The `bytes` object holding the return data.
    object: ValueId,
    /// Pointer to the object's first data byte.
    data: ValueId,
    /// The data's length in bytes.
    len: ValueId,
    /// Whether the data is an `Error(string)` payload a `catch Error` clause can decode.
    error_matches: ValueId,
    /// Whether the data is a `Panic(uint256)` payload a `catch Panic` clause can decode.
    panic_matches: ValueId,
}

struct TryTarget<'a, 'gcx> {
    callee: TryCallee<'a>,
    /// ABI parameter types of the target call.
    parameter_types: Vec<Ty<'gcx>>,
    /// Return types of the target call.
    return_types: Vec<Ty<'gcx>>,
    /// Parameter names for named-argument resolution.
    parameter_names: Option<CallableParamNames>,
    /// Whether the call must use STATICCALL.
    static_call: bool,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn snapshot_loop_state(&self, block: BlockId) -> LoopState {
        LoopState { block, values: self.values.clone(), storage_refs: self.storage_refs.clone() }
    }

    pub(super) fn lower_branches<T>(
        &mut self,
        condition: ValueId,
        then_first: bool,
        mut lower_then: impl FnMut(&mut Self) -> Option<T>,
        mut lower_else: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<(TernaryBranch<T>, TernaryBranch<T>)> {
        self.materialize_default_bindings();
        let first_block = self.builder.create_block();
        let second_block = self.builder.create_block();
        let (then_block, else_block) =
            if then_first { (first_block, second_block) } else { (second_block, first_block) };
        let merge_block = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        // branch(condition, then, else)
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        let then_value = lower_then(self)?;
        let then_terminated = self.is_terminated();
        let then_exit = self.builder.current_block();
        let then_values = std::mem::take(&mut self.values);
        let then_storage_refs = std::mem::take(&mut self.storage_refs);
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before_values.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(else_block);
        let else_value = lower_else(self)?;
        let else_terminated = self.is_terminated();
        let else_exit = self.builder.current_block();
        let else_values = std::mem::take(&mut self.values);
        let else_storage_refs = std::mem::take(&mut self.storage_refs);
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        // values = phi(then_values, else_values)
        // storage_refs = phi(then_storage_refs, else_storage_refs)
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

    pub(super) fn lower_if(
        &mut self,
        condition: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) -> Option<()> {
        let condition = self.lower_expr(condition)?;
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            true,
            |this| this.lower_stmt(then_stmt),
            |this| else_stmt.map_or(Some(()), |stmt| this.lower_stmt(stmt)),
        )?;
        if then_branch.terminated && else_branch.terminated {
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
        let mut default_block = None;

        for case in switch.cases {
            let block = self.builder.create_block();
            if let Some(constant) = case.constant {
                let value = self.lower_word_literal(constant)?;
                case_blocks.push((value, block));
            } else {
                default_block = Some(block);
            }
            body_blocks.push((case, block));
        }
        // switch(selector, default_or_merge, cases)
        self.builder.switch(selector, default_block.unwrap_or(merge_block), case_blocks);

        let mut states =
            Vec::with_capacity(body_blocks.len() + usize::from(default_block.is_none()));
        if default_block.is_none() {
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

        // values, storage_refs = phi(case_exits)
        self.builder.switch_to_block(merge_block);
        self.values = self.merge_loop_values(before_values, &states, &FxHashMap::default());
        self.storage_refs = self.merge_storage_ref_states(before_storage_refs, &states);
        Some(())
    }

    pub(super) fn lower_try(&mut self, try_stmt: &hir::StmtTry<'_>) -> Option<()> {
        // Parenthesizing the target changes nothing about the call it wraps, so peel the parens
        // here exactly as the checker does when it accepts the statement. solc rejects
        // `try (c.f()) { ... }` because its target must be a call syntactically, but the
        // statement has only one meaning and we compile it.
        let try_expr = try_stmt.expr.peel_parens();
        let ExprKind::Call(callee, args, call_opts) = &try_expr.kind else {
            return self.cx.report_unsupported(try_stmt.expr.span, "try expression");
        };
        let target = if let ExprKind::New(ty) = &callee.kind {
            let TyKind::Contract(contract_id) = self.cx.gcx.type_of_hir_ty(ty).kind else {
                return self.cx.report_unsupported(try_stmt.expr.span, "try target");
            };
            let contract = self.cx.gcx.hir.contract(contract_id);
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
            TryTarget {
                callee: TryCallee::Creation { ty, contract_id },
                parameter_types: parameters
                    .iter()
                    .map(|&parameter| self.cx.gcx.type_of_item(parameter.into()))
                    .collect(),
                return_types: Vec::new(),
                parameter_names: Some(parameter_names),
                static_call: false,
            }
        } else if let ExprKind::Member(receiver, _) = callee.kind {
            let Some(function_id) = self.cx.gcx.resolved_function(callee) else {
                return self.cx.report_unsupported(try_stmt.expr.span, "try target");
            };
            let function = self.cx.gcx.hir.function(function_id);
            let is_external_library = function.contract.is_some_and(|contract| {
                self.cx.gcx.hir.contract(contract).kind == hir::ContractKind::Library
            }) && matches!(
                function.visibility,
                hir::Visibility::Public | hir::Visibility::External
            );
            let (callee, parameter_types, parameter_names, static_call) = if is_external_library {
                let address = self.library_address(function_id);
                let attached =
                    self.cx.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
                (
                    TryCallee::LinkedLibrary {
                        address,
                        function: function_id,
                        receiver: attached.then_some(receiver),
                    },
                    function
                        .parameters
                        .iter()
                        .skip(usize::from(attached))
                        .map(|&parameter| self.cx.gcx.type_of_item(parameter.into()))
                        .collect(),
                    self.cx
                        .gcx
                        .call_param_source(callee)
                        .map(|source| self.cx.gcx.callable_param_names(source)),
                    false,
                )
            } else {
                (
                    TryCallee::Member {
                        receiver,
                        selector: self.cx.gcx.function_selector(function_id).0,
                    },
                    function
                        .parameters
                        .iter()
                        .map(|&parameter| self.cx.gcx.type_of_item(parameter.into()))
                        .collect(),
                    Some(self.cx.gcx.callable_param_names(CallableParamSource::Function {
                        id: function_id,
                        skips_receiver: false,
                    })),
                    self.uses_static_call(function.state_mutability),
                )
            };
            TryTarget {
                callee,
                parameter_types,
                return_types: function
                    .returns
                    .iter()
                    .map(|&return_id| self.cx.gcx.type_of_item(return_id.into()))
                    .collect(),
                parameter_names,
                static_call,
            }
        } else if let Some(TyKind::Fn(function)) =
            self.cx.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
            && function.is_external()
            && function.function_id.is_none()
        {
            let function_value = self.lower_expr(callee)?;
            let (address, selector) = self.split_external_function_pointer(function_value);
            TryTarget {
                callee: TryCallee::FunctionPointer { address, selector },
                parameter_types: function.parameters.to_vec(),
                return_types: function.returns.to_vec(),
                parameter_names: None,
                static_call: self.uses_static_call(function.state_mutability),
            }
        } else {
            return self.cx.report_unsupported(try_stmt.expr.span, "try target");
        };
        let Some((returns_clause, catch_clauses)) = try_stmt.clauses.split_first() else {
            return self.cx.report_unsupported(try_stmt.expr.span, "try/catch clause list");
        };
        if catch_clauses.is_empty() {
            return self.cx.report_unsupported(try_stmt.expr.span, "try/catch clause list");
        }
        let creation_binding = if matches!(target.callee, TryCallee::Creation { .. }) {
            if returns_clause.name.is_some() || returns_clause.args.len() > 1 {
                return self.cx.report_unsupported(returns_clause.span, "try return binding list");
            }
            returns_clause.args.first().copied()
        } else {
            if returns_clause.name.is_some()
                || (!returns_clause.args.is_empty()
                    && returns_clause.args.len() != target.return_types.len())
            {
                return self.cx.report_unsupported(returns_clause.span, "try return binding list");
            }
            None
        };
        for catch_clause in catch_clauses {
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            if catch_clause.name.is_some() && !catch_error && !catch_panic {
                return self.cx.report_unsupported(catch_clause.span, "try catch clause");
            }
            if catch_clause.args.len() > 1 {
                return self.cx.report_unsupported(catch_clause.span, "try catch clause");
            }
            if let Some(&binding) = catch_clause.args.first() {
                let ty = self.cx.gcx.type_of_item(binding.into());
                let expected = if catch_error {
                    TyKind::Elementary(ElementaryType::String)
                } else if catch_panic {
                    TyKind::Elementary(ElementaryType::UInt(TypeSize::new_int_bits(256)))
                } else {
                    TyKind::Elementary(ElementaryType::Bytes)
                };
                if ty.peel_refs().kind != expected {
                    return self.cx.report_unsupported(catch_clause.span, "try catch clause");
                }
            }
        }
        // Before Byzantium only a bare `catch { }` is lowerable: there is no return data for a
        // typed clause to match or bind. The type checker rejects one, so this reports rather
        // than bailing silently, which would leave the caller without a diagnostic.
        let supports_returndata = self.cx.gcx.sess.opts.evm_version.supports_returndata();
        if !supports_returndata
            && let Some(clause) =
                catch_clauses.iter().find(|clause| clause.name.is_some() || !clause.args.is_empty())
        {
            return self.cx.report_unsupported(clause.span, "typed catch clause");
        }
        if args.len() != target.parameter_types.len() {
            return self.cx.report_unsupported(args.span, "try argument list");
        }
        let return_types = self.external_return_types(&target.return_types);

        let (success, creation_value, ret_plan) = if let TryCallee::Creation { ty, contract_id } =
            target.callee
        {
            // address = create(...)
            // ok = address != 0
            let created = self.lower_create_contract(ty, contract_id, *args, *call_opts)?;
            let zero = self.builder.imm(U256::ZERO);
            let failed = self.builder.eq(created, zero);
            (self.builder.iszero(failed), Some(created), None)
        } else {
            let address = match target.callee {
                TryCallee::Member { receiver, .. } => self.lower_expr(receiver)?,
                TryCallee::LinkedLibrary { address, .. } => self.builder.imm(address),
                TryCallee::FunctionPointer { address, .. } => address,
                TryCallee::Creation { .. } => unreachable!(),
            };
            let options = self.lower_call_options(*call_opts, true, "try call option")?;
            let (call_value, zero) = (options.value, options.zero);
            let (mut values, mut types) =
                if let TryCallee::LinkedLibrary { function, receiver, .. } = target.callee {
                    let capacity = args.len() + usize::from(receiver.is_some());
                    let mut values = Vec::with_capacity(capacity);
                    let mut types = Vec::with_capacity(capacity);
                    if let Some(receiver) = receiver {
                        let function = self.cx.gcx.hir.function(function);
                        let Some(&parameter) = function.parameters.first() else {
                            return self
                                .cx
                                .report_unsupported(receiver.span, "attached library receiver");
                        };
                        let parameter_ty = self.cx.gcx.type_of_item(parameter.into());
                        let (value, ty) = self.lower_abi_receiver(receiver, parameter_ty)?;
                        values.push(value);
                        types.push(ty);
                    }
                    (values, types)
                } else {
                    (Vec::new(), Vec::new())
                };
            let (argument_values, argument_types) = self.lower_abi_call_arguments(
                *args,
                target.parameter_types.iter().copied(),
                target.parameter_names.as_ref(),
                args.span,
                "try argument",
                matches!(target.callee, TryCallee::LinkedLibrary { .. }),
            )?;
            values.extend(argument_values);
            types.extend(argument_types);
            let selector = match target.callee {
                TryCallee::Member { selector, .. } => {
                    self.builder.imm(U256::from_be_slice(&selector) << 224)
                }
                TryCallee::LinkedLibrary { function, .. } => {
                    let selector = self.cx.gcx.function_selector(function).0;
                    self.builder.imm(U256::from_be_slice(&selector) << 224)
                }
                TryCallee::FunctionPointer { selector, .. } => selector,
                TryCallee::Creation { .. } => unreachable!(),
            };
            // buffer = alloc_overlay_return_buffer(returns)
            // input = abi_encode(selector, args)
            let overlay_buffer = self.alloc_overlay_return_buffer(&return_types);
            // mstore(add(fmp(), ret_size), 0)
            self.touch_call_output_area(options.gas, &return_types, overlay_buffer.is_some());
            let layout = Arc::new(AbiLayout::new(types.into_boxed_slice()));
            let encoded =
                self.builder.abi_encode(layout, Some(selector), values.into_boxed_slice());
            let input = self.builder.slice_ptr(encoded);
            let input_size = self.builder.slice_len(encoded);
            // From Byzantium on the return values come out of the return data, which the catch
            // clauses need anyway; before it the call writes them into an output area overlaying
            // its input and the success path reads them back from there.
            // ret_offset, ret_size = plan_return_buffer(returns)
            let ret_plan = (!supports_returndata)
                .then(|| self.plan_return_buffer(input, zero, &return_types, overlay_buffer));
            let (ret_offset, ret_size) = match &ret_plan {
                Some(plan) => plan.output_area(),
                None => (zero, self.builder.imm(0)),
            };
            if self.needs_code_check(return_types.len()) {
                self.revert_if_no_code(address);
            }
            // The code check above is emitted at every version that needs the reserve, so the
            // call cannot create the callee's account.
            // gas = gas() | sub(gas(), reserve)
            let gas = self.call_gas(options.gas, options.value_set, false);
            // ok = delegatecall|staticcall|call(gas, address, input, ret_offset, ret_size)
            let success = match target.callee {
                TryCallee::LinkedLibrary { .. } => {
                    self.builder.delegatecall(gas, address, input, input_size, ret_offset, ret_size)
                }
                _ if target.static_call => {
                    self.builder.staticcall(gas, address, input, input_size, ret_offset, ret_size)
                }
                _ => self
                    .builder
                    .call(gas, address, call_value, input, input_size, ret_offset, ret_size),
            };
            (success, None, ret_plan)
        };

        let success_block = self.builder.create_block();
        let catch_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.materialize_default_bindings();
        let before = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        // branch(ok, success, catch)
        self.builder.branch(success, success_block, catch_block);

        self.builder.switch_to_block(success_block);
        if let Some(binding) = creation_binding {
            // success.address = address
            let Some(value) = creation_value else {
                return self.cx.report_unsupported(returns_clause.span, "try return binding list");
            };
            self.values.insert(binding, value);
        } else if !return_types.is_empty() {
            let values = if let Some(plan) = ret_plan {
                // success.returns = load_words(ret_offset) | abi_decode(buffer)
                self.finish_external_call(
                    plan,
                    &return_types,
                    returns_clause.span,
                    ExternalReturnMode::All,
                    "codegen cannot decode try/catch returndata before Byzantium",
                )?
            } else {
                // success.returns = abi_decode(returndata)
                let data = self.materialize_returndata_bytes();
                self.lower_abi_decode_values(data, &return_types, returns_clause.span)?
            };
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
        let mut states = Vec::with_capacity(catch_clauses.len() + 1);
        if !success_terminated {
            states.push(LoopState {
                block: success_exit,
                values: success_values,
                storage_refs: success_storage_refs,
            });
        }

        self.values = before.clone();
        self.storage_refs = before_storage_refs.clone();
        self.builder.switch_to_block(catch_block);
        // Only from Byzantium on; before it the bare clause matches unconditionally and there is
        // no data to bind or forward.
        // data = returndata()
        // selector = data.length >= 4 ? mload(data) >> 224 : 0
        // error_matches = selector == Error(string) && valid_error_payload(data)
        // panic_matches = selector == Panic(uint256) && data.length >= 36
        let catch_data = supports_returndata.then(|| {
            let object = self.materialize_returndata_bytes();
            let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
            let len = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
            let zero = self.builder.imm(U256::ZERO);
            let selector_slice = self.builder.make_slice(data, len, SliceLocation::Memory);
            let selector_word = self.builder.memory_slice_load_word(selector_slice, zero);
            let four = self.builder.imm(4);
            let selector_short = self.builder.lt(len, four);
            let has_selector = self.builder.iszero(selector_short);
            let selector_shift = self.builder.imm(224);
            let selector = self.builder.shr(selector_shift, selector_word);
            let error_selector =
                self.builder.imm(U256::from_be_slice(&keccak256("Error(string)")[..4]));
            let panic_selector = self.builder.imm(0x4e48_7b71_u64);
            let error_selector_matches = self.builder.eq(selector, error_selector);
            let error_matches = if catch_clauses
                .iter()
                .any(|clause| clause.name.is_some_and(|name| name.name == sym::Error))
            {
                self.lower_error_catch_match(data, len, error_selector_matches)
            } else {
                self.builder.and(has_selector, error_selector_matches)
            };
            let panic_size = self.builder.imm(36);
            let panic_short = self.builder.lt(len, panic_size);
            let panic_has_payload = self.builder.iszero(panic_short);
            let panic_selector_matches = self.builder.eq(selector, panic_selector);
            let panic_matches = self.builder.and(panic_has_payload, panic_selector_matches);
            TryCatchData { object, data, len, error_matches, panic_matches }
        });
        // Solidity matches the typed clauses before the low-level one, whichever order they are
        // written in: `catch Error(string)` first, then `catch Panic(uint256)`, and the bare or
        // `bytes memory` clause last as the fallback. The low-level clause matches
        // unconditionally, so testing the clauses in source order lets one written first shadow
        // the typed ones and run for a standard revert payload. The sort must stay stable so
        // that clauses of the same kind keep their source order.
        let mut ordered_clauses = catch_clauses.iter().collect::<Vec<_>>();
        ordered_clauses.sort_by_key(|clause| match clause.name.map(|name| name.name) {
            Some(sym::Error) => 0,
            Some(sym::Panic) => 1,
            _ => 2,
        });
        let mut next_catch = self.builder.current_block();
        for catch_clause in ordered_clauses {
            // if catch_matches(clause, data) { lower(clause) } else { next_catch }
            self.builder.switch_to_block(next_catch);
            let clause_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            let catch_error = catch_clause.name.is_some_and(|name| name.name == sym::Error);
            let catch_panic = catch_clause.name.is_some_and(|name| name.name == sym::Panic);
            // A typed clause is rejected before Byzantium, so its data is always there.
            let condition = if catch_error {
                catch_data?.error_matches
            } else if catch_panic {
                catch_data?.panic_matches
            } else {
                self.builder.imm_bool(true)
            };
            self.builder.branch(condition, clause_block, next_block);

            self.values = before.clone();
            self.storage_refs = before_storage_refs.clone();
            self.builder.switch_to_block(clause_block);
            if let Some(&binding) = catch_clause.args.first() {
                let object = catch_data?.object;
                let value = if catch_error {
                    self.lower_error_catch_string(object)?
                } else if catch_panic {
                    self.lower_panic_catch_word(object)
                } else {
                    object
                };
                self.values.insert(binding, value);
            }
            self.lower_block(catch_clause.block)?;
            let catch_terminated = self.is_terminated();
            let catch_exit = self.builder.current_block();
            if !catch_terminated {
                self.builder.jump(merge_block);
                states.push(LoopState {
                    block: catch_exit,
                    values: std::mem::take(&mut self.values),
                    storage_refs: std::mem::take(&mut self.storage_refs),
                });
            }
            next_catch = next_block;
        }
        self.builder.switch_to_block(next_catch);
        match catch_data {
            // revert(data.data, data.length)
            Some(data) => self.builder.revert(data.data, data.len),
            // Unreachable: a pre-Byzantium `try` only has a bare clause, which always matches.
            // revert(0, 0)
            None => {
                self.builder.revert_with(RevertReason::Empty);
            }
        }

        // values, storage_refs = phi(success, catches)
        self.builder.switch_to_block(merge_block);
        self.values = self.merge_many_values(before, &states);
        self.storage_refs = self.merge_storage_ref_states(before_storage_refs, &states);
        Some(())
    }

    pub(super) fn lower_word_literal(&mut self, lit: &hir::Lit<'_>) -> Option<ValueId> {
        if let LitKind::Str(_, bytes, _) = &lit.kind {
            let bytes = bytes.as_byte_str();
            if bytes.len() > 32 {
                return self.cx.report_unsupported(lit.span, "switch literal");
            }
            return Some(self.lower_string_literal_word(bytes));
        }
        if let LitKind::Bool(value) = lit.kind {
            return Some(self.builder.imm(if value { U256::ONE } else { U256::ZERO }));
        }
        if let LitKind::Address(value) = lit.kind {
            return Some(self.builder.imm(U256::from_be_slice(value.as_slice())));
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
        let ids = merged.keys().copied().collect::<Vec<_>>();
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

    pub(super) fn lower_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        // branch(condition, then, else)
        // value = then_value | else_value | phi(then_value, else_value)
        let condition = self.lower_expr(condition)?;
        let then_ty = self.cx.gcx.type_of_expr(then_expr.id)?;
        let else_ty = self.cx.gcx.type_of_expr(else_expr.id)?;
        let ty = then_ty.common_type(else_ty, self.cx.gcx)?;
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            true,
            |this| this.lower_ternary_value(then_expr, ty),
            |this| this.lower_ternary_value(else_expr, ty),
        )?;
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

    fn lower_ternary_value(&mut self, expr: &hir::Expr<'_>, ty: Ty<'gcx>) -> Option<ValueId> {
        let source_ty = self.cx.gcx.type_of_expr(expr.id)?;
        let value = self.lower_expr(expr)?;
        let value = if ty.is_ref_at(DataLocation::Memory) {
            self.materialize_memory_argument(ty, value, expr.span)?
        } else {
            value
        };
        Some(self.coerce_value(value, source_ty, ty))
    }

    pub(super) fn lower_logical(
        &mut self,
        lhs_expr: &hir::Expr<'_>,
        op: BinOpKind,
        rhs_expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        // if && { branch(lhs, rhs, false) }
        // if || { branch(lhs, true, rhs) }
        // result = phi(rhs_value, short_circuit_constant)
        let lhs = self.lower_expr(lhs_expr)?;
        let is_and = op == BinOpKind::And;
        let (then_branch, else_branch) = self.lower_branches(
            lhs,
            is_and,
            |this| {
                if is_and { this.lower_expr(rhs_expr) } else { Some(this.builder.imm_bool(true)) }
            },
            |this| {
                if is_and { Some(this.builder.imm_bool(false)) } else { this.lower_expr(rhs_expr) }
            },
        )?;
        let (rhs, short) =
            if is_and { (then_branch, else_branch) } else { (else_branch, then_branch) };
        if rhs.terminated || rhs.value == short.value {
            Some(short.value)
        } else {
            Some(self.builder.phi(vec![(rhs.block, rhs.value), (short.block, short.value)]))
        }
    }

    pub(super) fn lower_ternary_values(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> Option<Vec<ValueId>> {
        let condition = self.lower_expr(condition)?;
        // branch(condition, then, else)
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            true,
            |this| this.lower_values(then_expr),
            |this| this.lower_values(else_expr),
        )?;
        if !then_branch.terminated
            && !else_branch.terminated
            && then_branch.value.len() != else_branch.value.len()
        {
            return self.cx.report_unsupported(then_expr.span, "ternary value count");
        }
        // values = then_values | else_values | phi(then_i, else_i)
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
            LoopSource::For { update: Some(update) } if matches!(&update.kind, StmtKind::Block(block) if block.is_empty()) => {
                None
            }
            LoopSource::For { update } => update,
            LoopSource::While => None,
            LoopSource::DoWhile => {
                // body = block[..-1]
                // update = block[-1]
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
        // preheader -> header
        // header -> body -> update? -> header
        // break -> exit
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
            // body -> update
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
                // update.values = phi(body, continues)
                // update -> header
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
            // body -> header
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
        // exit.values = phi(breaks, header)
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
            let value = self
                .merge_incoming_values(incoming)
                .unwrap_or_else(|| header_phis.get(&id).copied().unwrap_or(before_value));
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

        // slot = first_slot | phi(incoming_slots)
        let slot = if incoming.iter().all(|&(_, access)| access.slot == first.slot) {
            first.slot
        } else {
            self.builder.phi(incoming.iter().map(|&(block, access)| (block, access.slot)).collect())
        };

        // offset = none | first_offset | phi(explicit_or_default_offsets)
        let offset = if incoming.iter().all(|&(_, access)| access.offset.is_none()) {
            None
        } else {
            let offsets = incoming
                .iter()
                .map(|&(_, access)| {
                    access
                        .offset
                        .unwrap_or_else(|| self.builder.imm(u64::from(access.location.offset)))
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
        let ids = values.keys().copied().collect::<Vec<_>>();
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
        let ids = before.keys().copied().collect::<Vec<_>>();
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
            if let Some(value) = self.merge_incoming_values(incoming) {
                before.insert(id, value);
            }
        }
        before
    }

    fn merge_incoming_values(&mut self, incoming: Vec<(BlockId, ValueId)>) -> Option<ValueId> {
        match incoming.as_slice() {
            [] => None,
            [(_, value)] => Some(*value),
            [(_, first), rest @ ..] if rest.iter().all(|(_, value)| value == first) => Some(*first),
            _ => Some(self.merge_value_phi(incoming)),
        }
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
        let ids = before.keys().copied().collect::<Vec<_>>();
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
