//! Modifier and base-constructor expansion.

use super::*;
use solar_data_structures::Never;
use solar_sema::hir::Visit;
use std::ops::ControlFlow;

struct ModifierLocalIds<'hir> {
    hir: &'hir hir::Hir<'hir>,
    ids: FxHashSet<hir::VariableId>,
}

impl<'hir> hir::Visit<'hir> for ModifierLocalIds<'hir> {
    type BreakValue = Never;

    fn hir(&self) -> &'hir hir::Hir<'hir> {
        self.hir
    }

    fn visit_nested_var(&mut self, id: hir::VariableId) -> ControlFlow<Self::BreakValue> {
        if matches!(self.hir.variable(id).kind, hir::VarKind::Statement | hir::VarKind::TryCatch) {
            self.ids.insert(id);
        }
        ControlFlow::Continue(())
    }
}

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
    fn snapshot_bindings(&self, ids: &[hir::VariableId]) -> BindingSnapshot {
        BindingSnapshot {
            ids: ids.to_vec(),
            values: ids
                .iter()
                .filter_map(|&id| self.values.get(&id).copied().map(|value| (id, value)))
                .collect(),
            storage_refs: ids
                .iter()
                .filter_map(|&id| self.storage_refs.get(&id).copied().map(|access| (id, access)))
                .collect(),
        }
    }

    fn restore_bindings(&mut self, snapshot: &BindingSnapshot) {
        for &id in &snapshot.ids {
            self.values.remove(&id);
            self.storage_refs.remove(&id);
        }
        self.values.extend(snapshot.values.iter().map(|(&id, &value)| (id, value)));
        self.storage_refs.extend(snapshot.storage_refs.iter().map(|(&id, &access)| (id, access)));
    }

    pub(super) fn lower_modifier_chain(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
    ) -> Option<()> {
        self.lower_modifier_at(modifiers, body, 0)
    }

    pub(super) fn lower_modifier_at(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
        index: usize,
    ) -> Option<()> {
        let Some(modifier) = modifiers.get(index) else {
            return self.lower_block(body);
        };
        if modifier.id.as_contract().is_some() {
            return self.lower_modifier_at(modifiers, body, index + 1);
        }
        let hir::ItemId::Function(modifier_id) = modifier.id else {
            return report_unsupported(self.gcx, modifier.span, "base constructor modifier");
        };
        let modifier_id = self.gcx.resolve_virtual_function(self.contract_id, modifier_id);
        let modifier_function = self.gcx.hir.function(modifier_id);
        if modifier_function.kind == hir::FunctionKind::Constructor {
            return self.lower_modifier_at(modifiers, body, index + 1);
        }
        if !modifier_function.kind.is_modifier() {
            return report_unsupported(self.gcx, modifier.span, "modifier target");
        }
        let Some(modifier_body) = modifier_function.body else {
            return report_unsupported(self.gcx, modifier.span, "modifier body");
        };
        if modifier.args.len() != modifier_function.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "modifier argument list");
        }
        let incoming_returns = self.snapshot_bindings(&self.returns);
        let local_ids = self.modifier_local_ids(modifier_body);
        let mut saved_locals = Vec::with_capacity(local_ids.len());
        let mut saved_storage_locals = Vec::new();
        for id in local_ids {
            if Self::is_storage_parameter(self.gcx.type_of_item(id.into())) {
                saved_storage_locals.push((id, self.storage_refs.get(&id).copied()));
            } else {
                saved_locals.push((id, self.values.get(&id).copied()));
            }
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: modifier_id,
            skips_receiver: false,
        });
        let mut saved_parameters = Vec::with_capacity(modifier_function.parameters.len());
        let mut saved_storage_parameters = Vec::new();
        for (index, &parameter) in modifier_function.parameters.iter().enumerate() {
            let Some(argument) =
                modifier.args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(self.gcx, modifier.span, "named modifier argument");
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            if Self::is_storage_parameter(parameter_ty) {
                let access = self.storage_access(argument)?;
                saved_storage_parameters
                    .push((parameter, self.storage_refs.insert(parameter, access)));
            } else {
                let value = self.lower_typed_expr(argument, parameter_ty)?;
                let value = self.coerce_call_argument(argument, parameter_ty, value);
                let value = self.materialize_call_argument(parameter_ty, value, argument.span)?;
                saved_parameters.push((parameter, self.values.insert(parameter, value)));
            }
        }

        let context = ModifierContext {
            modifiers,
            body,
            next: index + 1,
            parameters: self.snapshot_bindings(&self.parameters),
            returns: self.snapshot_bindings(&self.returns),
            incoming_returns,
        };
        // A modifier's output starts with the incoming return bindings. Its
        // argument expressions still form the input frame used by each `_`.
        self.restore_bindings(&context.incoming_returns);
        self.modifiers.push(context);
        let result = self.lower_block(modifier_body);
        self.modifiers.pop();
        for (parameter, previous) in saved_parameters {
            if let Some(value) = previous {
                self.values.insert(parameter, value);
            } else {
                self.values.remove(&parameter);
            }
        }
        for (parameter, previous) in saved_storage_parameters {
            if let Some(access) = previous {
                self.storage_refs.insert(parameter, access);
            } else {
                self.storage_refs.remove(&parameter);
            }
        }
        for (local, previous) in saved_locals {
            if let Some(value) = previous {
                self.values.insert(local, value);
            } else {
                self.values.remove(&local);
            }
        }
        for (local, previous) in saved_storage_locals {
            if let Some(access) = previous {
                self.storage_refs.insert(local, access);
            } else {
                self.storage_refs.remove(&local);
            }
        }
        result
    }

    fn modifier_local_ids(&self, body: hir::Block<'gcx>) -> Vec<hir::VariableId> {
        let mut visitor = ModifierLocalIds { hir: &self.gcx.hir, ids: FxHashSet::default() };
        for stmt in body.stmts {
            let _ = visitor.visit_stmt(stmt);
        }
        visitor.ids.into_iter().collect()
    }

    pub(super) fn lower_base_constructor(
        &mut self,
        modifier: &hir::Modifier<'_>,
        constructor_id: hir::FunctionId,
        constructor: &'gcx hir::Function<'gcx>,
    ) -> Option<()> {
        let Some(body) = constructor.body else {
            return report_unsupported(self.gcx, modifier.span, "base constructor body");
        };
        if modifier.args.len() != constructor.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "base constructor arguments");
        }
        let contract_id = constructor.contract;
        let Some(values) = self.constructor_arguments.remove(&constructor_id) else {
            return report_unsupported(self.gcx, modifier.span, "base constructor arguments");
        };
        if values.len() != constructor.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "base constructor arguments");
        }
        let mut saved_parameters = Vec::with_capacity(constructor.parameters.len());
        for (&parameter, value) in constructor.parameters.iter().zip(values) {
            saved_parameters.push((parameter, self.values.insert(parameter, value)));
        }

        let continuation = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.push_return_target(continuation);
        if let Some(contract_id) = contract_id {
            self.lower_state_initializers(contract_id)?;
        }
        let parameter_start = self.parameters.len();
        self.parameters.extend_from_slice(constructor.parameters);
        let result = self.lower_function_body(constructor.modifiers, body);
        self.parameters.truncate(parameter_start);
        result?;
        if !self.is_terminated() {
            self.record_return_state();
            self.builder.jump(continuation);
        }
        self.finish_return_target(before_values, before_storage_refs);
        for (parameter, previous) in saved_parameters {
            if let Some(value) = previous {
                self.values.insert(parameter, value);
            } else {
                self.values.remove(&parameter);
            }
        }
        Some(())
    }

    pub(super) fn lower_modifier_placeholder(&mut self, span: Span) -> Option<()> {
        let Some(context) = self.modifiers.last().cloned() else {
            return report_unsupported(self.gcx, span, "modifier placeholder");
        };
        let continuation = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.restore_bindings(&context.parameters);
        self.restore_bindings(&context.returns);
        self.push_return_target(continuation);
        let result = self.lower_modifier_at(context.modifiers, context.body, context.next);
        result?;
        if !self.is_terminated() {
            self.record_return_state();
            self.builder.jump(continuation);
        }
        self.finish_return_target(before_values, before_storage_refs);
        // Function parameters are inputs to every body expansion. Return
        // bindings, in contrast, carry the selected body's result onward.
        self.restore_bindings(&context.parameters);
        Some(())
    }
}
