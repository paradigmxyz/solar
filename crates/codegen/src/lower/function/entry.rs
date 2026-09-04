//! Function signature, constructor, and body orchestration.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn bind_signature(&mut self, function: &hir::Function<'_>) {
        self.parameters.extend_from_slice(function.parameters);
        for &param in function.parameters {
            let ty = self.cx.gcx.type_of_item(param.into());
            let value = self.builder.add_param(types::TypeLowerer::mir_type(ty));
            if ty.is_ref_at(DataLocation::Storage) {
                self.storage_refs.insert(
                    param,
                    StorageAccess {
                        slot: value,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                self.values.insert(param, value);
                if ty.is_value_type() {
                    self.dirty_values.insert(value);
                }
            }
        }
        for &ret in function.returns {
            let ty = self.cx.gcx.type_of_item(ret.into());
            self.builder.add_return(types::TypeLowerer::mir_return_type(ty));
            if ty.is_ref_at(DataLocation::Storage) {
                let zero = self.builder.imm(U256::ZERO);
                self.storage_refs.insert(
                    ret,
                    StorageAccess {
                        slot: zero,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                self.default_bindings.insert(ret);
            }
        }
        self.returns.extend_from_slice(function.returns);
    }

    /// Lowers only one contract's own state initializers.
    pub(super) fn lower_state_initializers(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let contract = self.cx.gcx.hir.contract(contract_id);
        for id in contract.variables() {
            let variable = self.cx.gcx.hir.variable(id);
            if !variable.is_state_variable() || variable.is_constant() {
                continue;
            }
            let Some(initializer) = variable.initializer else { continue };
            let ty = self.cx.gcx.type_of_item(id.into());
            let source_ty = self.cx.gcx.type_of_expr(initializer.id)?;
            let value = self.lower_typed_expr(initializer, ty)?;
            if let Some(&immutable_id) = self.cx.immutable_ids.get(&id) {
                self.builder.store_immutable(immutable_id, value);
            } else {
                self.store_state_variable(id, value, source_ty, initializer.span)?;
            }
        }
        Some(())
    }

    pub(super) fn lower_implicit_base_constructors(
        &mut self,
        contract_id: hir::ContractId,
    ) -> Option<()> {
        self.prepare_base_constructor_arguments(contract_id)?;
        let mut lowered = FxHashSet::default();
        self.lower_implicit_base_constructors_inner(contract_id, &mut lowered)
    }

    /// Lowers every base constructor's argument list into the derived
    /// constructor's frame, before any base constructor body is lowered.
    ///
    /// The lists are evaluated in the order
    /// [`base_constructor_arg_lists`](Self::base_constructor_arg_lists)
    /// returns them, and the chosen list for each base is recorded for the
    /// body lowering that follows.
    ///
    /// The bindings stay live: an argument expression may assign to a
    /// constructor parameter, and, because every argument list is evaluated
    /// before the first base body runs, that assignment is visible to the
    /// body that owns the parameter. Restoring the pre-argument bindings here
    /// would lower each body from the parameter values the arguments started
    /// with instead.
    fn prepare_base_constructor_arguments(&mut self, contract_id: hir::ContractId) -> Option<()> {
        self.base_args = self.base_constructor_arg_lists(contract_id);
        // A base constructor without parameters is called with no arguments,
        // whether or not any contract wrote a list for it.
        for &base_id in self.cx.gcx.hir.contract(contract_id).linearized_bases.iter().skip(1) {
            if let Some(constructor_id) = self.cx.gcx.hir.contract(base_id).ctor
                && self.cx.gcx.hir.function(constructor_id).parameters.is_empty()
                && !self.base_args.iter().any(|&(id, _)| id == base_id)
            {
                self.base_args.push((base_id, hir::CallArgs::default()));
            }
        }
        for index in 0..self.base_args.len() {
            let (base_id, args) = self.base_args[index];
            let Some(constructor_id) = self.cx.gcx.hir.contract(base_id).ctor else {
                continue;
            };
            let constructor = self.cx.gcx.hir.function(constructor_id);
            if args.len() != constructor.parameters.len() {
                return self.cx.report_unsupported(constructor.span, "base constructor arguments");
            }
            let parameter_names = self.cx.gcx.callable_param_names(CallableParamSource::Function {
                id: constructor_id,
                skips_receiver: false,
            });
            let values = self.lower_call_arguments(
                args,
                CallArgumentParams {
                    count: constructor.parameters.len(),
                    names: Some(parameter_names.as_slice()),
                    reverse: false,
                },
                constructor.span,
                "named base constructor argument",
                |this, index, argument| {
                    let parameter_ty =
                        this.cx.gcx.type_of_item(constructor.parameters[index].into());
                    // A storage-reference argument passes the referenced slot,
                    // like a storage parameter of an internal call.
                    if Self::is_storage_parameter(parameter_ty) {
                        let Some(access) = this.storage_access(argument) else {
                            return this.cx.report_unsupported(argument.span, "storage access");
                        };
                        return Some(access.slot);
                    }
                    this.lower_typed_expr(argument, parameter_ty)
                },
            )?;
            // Later bases may name an earlier base's parameters in their own
            // argument list, and each body is lowered from these bindings, so
            // they stay live for the rest of the constructor.
            self.bind_constructor_parameters(constructor.parameters, &values);
            self.prepared_constructors.insert(constructor_id);
        }
        Some(())
    }

    /// Binds an inlined constructor's parameters to already lowered arguments.
    ///
    /// A storage-reference parameter is a slot number, so it binds as a
    /// storage reference like a lowered function's own parameter does.
    pub(super) fn bind_constructor_parameters(
        &mut self,
        parameters: &[VariableId],
        values: &[ValueId],
    ) {
        for (&parameter, &value) in parameters.iter().zip(values) {
            let ty = self.cx.gcx.type_of_item(parameter.into());
            if Self::is_storage_parameter(ty) {
                self.values.remove(&parameter);
                self.storage_refs.insert(
                    parameter,
                    StorageAccess {
                        slot: value,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                self.storage_refs.remove(&parameter);
                self.values.insert(parameter, value);
            }
        }
    }

    pub(super) fn lower_implicit_base_constructors_inner(
        &mut self,
        contract_id: hir::ContractId,
        lowered: &mut FxHashSet<hir::ContractId>,
    ) -> Option<()> {
        let bases = self.cx.gcx.hir.contract(contract_id).linearized_bases;
        for &base_id in bases.iter().skip(1).rev() {
            if !lowered.insert(base_id) {
                continue;
            }
            let Some(constructor_id) = self.cx.gcx.hir.contract(base_id).ctor else {
                self.lower_state_initializers(base_id)?;
                continue;
            };
            let constructor = self.cx.gcx.hir.function(constructor_id);
            let Some(args) = self.base_constructor_args(base_id) else {
                continue;
            };
            let modifier = hir::Modifier {
                span: constructor.span,
                name_span: constructor.span,
                id: hir::ItemId::Contract(base_id),
                args,
            };
            self.lower_base_constructor(&modifier, constructor_id, constructor)?;
        }
        Some(())
    }

    /// Returns the base constructor argument lists of `contract_id`'s
    /// hierarchy, in evaluation order.
    ///
    /// solc evaluates the lists grouped by the contract that wrote them, the
    /// most derived writer first, and inside one writer in that writer's own
    /// inheritance order; see `IRGenerator::evaluateConstructorArguments`,
    /// which evaluates a writer's lists at the top of that writer's
    /// constructor before the next constructor in the chain runs. Ordering the
    /// lists by the base they target instead runs their side effects in a
    /// different order.
    fn base_constructor_arg_lists(
        &self,
        contract_id: hir::ContractId,
    ) -> Vec<(hir::ContractId, hir::CallArgs<'gcx>)> {
        let hir = &self.cx.gcx.hir;
        let mut lists = Vec::new();
        for &writer_id in hir.contract(contract_id).linearized_bases {
            let writer = hir.contract(writer_id);
            for (index, &base_id) in writer.linearized_bases.iter().skip(1).enumerate() {
                // An empty list gives no arguments, so a list written for the
                // same base further up the hierarchy still applies.
                if let Some(modifier) = writer.linearized_bases_args.get(index).copied().flatten()
                    && !modifier.args.is_empty()
                    && !lists.iter().any(|&(id, _)| id == base_id)
                {
                    lists.push((base_id, modifier.args));
                }
            }
        }
        lists
    }

    pub(super) fn base_constructor_args(
        &self,
        base_id: hir::ContractId,
    ) -> Option<hir::CallArgs<'gcx>> {
        self.base_args.iter().find(|&&(id, _)| id == base_id).map(|&(_, args)| args)
    }

    pub(super) fn finish(&mut self, returns: &[VariableId]) -> Option<()> {
        if returns.is_empty() {
            self.builder.stop();
        } else {
            let mut values = Vec::with_capacity(returns.len());
            for &id in returns {
                let ty = self.cx.gcx.type_of_item(id.into());
                let value = if ty.is_ref_at(DataLocation::Storage) {
                    self.storage_refs.get(&id).copied()?.slot
                } else {
                    self.values.get(&id).copied().unwrap_or_else(|| self.default_binding_value(ty))
                };
                values.push(self.materialize_call_argument(
                    ty,
                    value,
                    self.cx.gcx.hir.variable(id).span,
                )?);
            }
            self.builder.ret(values);
        }
        Some(())
    }

    pub(super) fn is_terminated(&self) -> bool {
        self.builder.func().block(self.builder.current_block()).terminator.is_some()
    }

    pub(super) fn push_return_target(&mut self, block: BlockId) {
        self.return_targets.push(ReturnTarget { block, states: Vec::new() });
    }

    pub(super) fn materialize_default_bindings(&mut self) {
        let ids = self
            .default_bindings
            .iter()
            .chain(self.deferred_bindings.iter())
            .copied()
            .filter(|id| !self.values.contains_key(id))
            .collect::<Vec<_>>();
        for id in ids {
            let ty = self.cx.gcx.type_of_item(id.into());
            let value = self.default_binding_value(ty);
            self.values.insert(id, value);
        }
    }

    pub(super) fn record_return_state(&mut self) {
        let state = self.snapshot_loop_state(self.builder.current_block());
        if let Some(target) = self.return_targets.last_mut() {
            target.states.push(state);
        }
    }

    pub(super) fn finish_return_target(
        &mut self,
        before_values: FxHashMap<VariableId, ValueId>,
        before_storage_refs: FxHashMap<VariableId, StorageAccess>,
    ) {
        let target = self.return_targets.pop().expect("return target exists");
        self.builder.switch_to_block(target.block);
        self.values = self.merge_many_values(before_values, &target.states);
        self.storage_refs = self.merge_storage_ref_states(before_storage_refs, &target.states);
    }

    pub(super) fn lower_function_body(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        // return_values = lower_body(modifiers)
        if modifiers.is_empty() {
            self.lower_block(body)
        } else {
            self.materialize_default_bindings();
            let return_block = self.builder.create_block();
            let before_values = self.values.clone();
            let before_storage_refs = self.storage_refs.clone();
            self.push_return_target(return_block);
            let result = self.lower_modifier_chain(modifiers, body);
            result?;
            if !self.is_terminated() {
                self.record_return_state();
                self.builder.jump(return_block);
            }
            self.finish_return_target(before_values, before_storage_refs);
            Some(())
        }
    }
}
