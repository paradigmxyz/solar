//! Function signature, constructor, and body orchestration.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    pub(super) fn bind_signature(&mut self, function: &hir::Function<'_>) {
        for &param in function.parameters {
            let value = self
                .builder
                .add_param(types::TypeLowerer::mir_type(self.gcx.type_of_item(param.into())));
            let ty = self.gcx.type_of_item(param.into());
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
            }
        }
        for &ret in function.returns {
            self.builder
                .add_return(types::TypeLowerer::mir_return_type(self.gcx.type_of_item(ret.into())));
            let ty = self.gcx.type_of_item(ret.into());
            if ty.is_ref_at(DataLocation::Storage) {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.storage_refs.insert(
                    ret,
                    StorageAccess {
                        slot: zero,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    },
                );
            } else {
                let value = self.default_binding_value(ty);
                self.values.insert(ret, value);
            }
        }
        self.returns.extend_from_slice(function.returns);
    }

    /// Lowers only one contract's own state initializers.
    pub(super) fn lower_state_initializers(&mut self, contract_id: hir::ContractId) -> Option<()> {
        let contract = self.gcx.hir.contract(contract_id);
        for id in contract.variables() {
            let variable = self.gcx.hir.variable(id);
            if !variable.is_state_variable() || variable.is_constant() {
                continue;
            }
            let Some(initializer) = variable.initializer else { continue };
            let ty = self.gcx.type_of_item(id.into());
            let source_ty = self.gcx.type_of_expr(initializer.id)?;
            let value = self.lower_typed_expr(initializer, ty)?;
            let value = self.coerce_value(value, source_ty, ty);
            if let Some(&immutable_id) = self.immutable_ids.get(&id) {
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
        let mut lowered = FxHashSet::default();
        self.lower_implicit_base_constructors_inner(contract_id, &mut lowered)
    }

    pub(super) fn lower_implicit_base_constructors_inner(
        &mut self,
        contract_id: hir::ContractId,
        lowered: &mut FxHashSet<hir::ContractId>,
    ) -> Option<()> {
        let bases = self.gcx.hir.contract(contract_id).linearized_bases;
        for (index, &base_id) in bases.iter().skip(1).enumerate() {
            if lowered.contains(&base_id) {
                continue;
            }
            let Some(constructor_id) = self.gcx.hir.contract(base_id).ctor else {
                self.lower_implicit_base_constructors_inner(base_id, lowered)?;
                self.lower_state_initializers(base_id)?;
                lowered.insert(base_id);
                continue;
            };
            let constructor = self.gcx.hir.function(constructor_id);
            let Some(args) = self
                .base_constructor_args(contract_id, base_id, index)
                .or_else(|| constructor.parameters.is_empty().then(hir::CallArgs::default))
            else {
                continue;
            };
            let modifier = hir::Modifier {
                span: constructor.span,
                name_span: constructor.span,
                id: hir::ItemId::Contract(base_id),
                args,
            };
            lowered.insert(base_id);
            self.lower_base_constructor(&modifier, constructor_id, constructor, lowered)?;
        }
        Some(())
    }

    pub(super) fn base_constructor_args(
        &self,
        contract_id: hir::ContractId,
        base_id: hir::ContractId,
        index: usize,
    ) -> Option<hir::CallArgs<'gcx>> {
        let contract = self.gcx.hir.contract(contract_id);
        if let Some(modifier) = contract.linearized_bases_args.get(index).copied().flatten() {
            return Some(modifier.args);
        }

        for &ancestor_id in contract.linearized_bases.iter().skip(1) {
            let ancestor = self.gcx.hir.contract(ancestor_id);
            let Some(ancestor_index) =
                ancestor.linearized_bases.iter().skip(1).position(|&id| id == base_id)
            else {
                continue;
            };
            if let Some(modifier) =
                ancestor.linearized_bases_args.get(ancestor_index).copied().flatten()
            {
                return Some(modifier.args);
            }
        }
        None
    }

    pub(super) fn finish(&mut self, returns: &[VariableId]) -> Option<()> {
        if returns.is_empty() {
            self.builder.stop();
        } else {
            let mut values = Vec::with_capacity(returns.len());
            for &id in returns {
                let ty = self.gcx.type_of_item(id.into());
                let value = if ty.is_ref_at(DataLocation::Storage) {
                    self.storage_refs.get(&id).copied()?.slot
                } else {
                    self.values.get(&id).copied()?
                };
                values.push(self.materialize_memory_argument(
                    ty,
                    value,
                    self.gcx.hir.variable(id).span,
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

    pub(super) fn record_return_state(&mut self) {
        let state = LoopState {
            block: self.builder.current_block(),
            values: self.values.clone(),
            storage_refs: self.storage_refs.clone(),
        };
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
        self.storage_refs = self.merge_many_storage_refs(before_storage_refs, &target.states);
    }

    pub(super) fn lower_function_body(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        if modifiers.is_empty() {
            self.lower_block(body)
        } else {
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
