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

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn snapshot_bindings(&self, ids: &[hir::VariableId]) -> BindingSnapshot {
        ids.iter()
            .map(|&id| (id, self.values.get(&id).copied(), self.storage_refs.get(&id).copied()))
            .collect()
    }

    pub(super) fn restore_bindings(&mut self, snapshot: &BindingSnapshot) {
        for &(id, value, access) in snapshot {
            self.values.remove(&id);
            self.storage_refs.remove(&id);
            if let Some(value) = value {
                self.values.insert(id, value);
            }
            if let Some(access) = access {
                self.storage_refs.insert(id, access);
            }
        }
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
        let Some(modifier_id) = self.cx.gcx.resolve_modifier_target(self.cx.contract_id, modifier)
        else {
            return self.cx.report_unsupported(modifier.span, "base constructor modifier");
        };
        let modifier_function = self.cx.gcx.hir.function(modifier_id);
        if modifier_function.kind == hir::FunctionKind::Constructor {
            return self.lower_modifier_at(modifiers, body, index + 1);
        }
        if !modifier_function.kind.is_modifier() {
            return self.cx.report_unsupported(modifier.span, "modifier target");
        }
        let Some(modifier_body) = modifier_function.body else {
            return self.cx.report_unsupported(modifier.span, "modifier body");
        };
        if modifier.args.len() != modifier_function.parameters.len() {
            return self.cx.report_unsupported(modifier.span, "modifier argument list");
        }
        let incoming_returns = self.snapshot_bindings(&self.returns);
        let local_ids = self.modifier_local_ids(modifier_body);
        let saved_locals = self.snapshot_bindings(&local_ids);
        let parameter_names = match modifier.args.kind {
            hir::CallArgsKind::Named(_) => {
                Some(self.cx.gcx.callable_param_names(CallableParamSource::Function {
                    id: modifier_id,
                    skips_receiver: false,
                }))
            }
            hir::CallArgsKind::Unnamed(_) => None,
        };
        let saved_parameters = self.snapshot_bindings(modifier_function.parameters);
        self.lower_call_arguments(
            modifier.args,
            CallArgumentParams {
                count: modifier_function.parameters.len(),
                names: parameter_names.as_deref(),
                reverse: false,
            },
            modifier.span,
            "named modifier argument",
            |this, index, argument| {
                let parameter = modifier_function.parameters[index];
                let parameter_ty = this.cx.gcx.type_of_item(parameter.into());
                if Self::is_storage_parameter(parameter_ty) {
                    let Some(access) = this.storage_access(argument) else {
                        return this.cx.report_unsupported(argument.span, "storage access");
                    };
                    this.storage_refs.insert(parameter, access);
                } else {
                    let value = this.lower_typed_expr(argument, parameter_ty)?;
                    let value = this.normalize_dirty_scalar(value, parameter_ty);
                    let value =
                        this.materialize_call_argument(parameter_ty, value, argument.span)?;
                    this.values.insert(parameter, value);
                }
                Some(())
            },
        )?;

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
        self.restore_bindings(&saved_parameters);
        self.restore_bindings(&saved_locals);
        result
    }

    fn modifier_local_ids(&self, body: hir::Block<'gcx>) -> Vec<hir::VariableId> {
        let mut visitor = ModifierLocalIds { hir: &self.cx.gcx.hir, ids: FxHashSet::default() };
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
            return self.cx.report_unsupported(modifier.span, "base constructor body");
        };
        if modifier.args.len() != constructor.parameters.len() {
            return self.cx.report_unsupported(modifier.span, "base constructor arguments");
        }
        let contract_id = constructor.contract;
        // The parameters are already bound: `prepare_base_constructor_arguments`
        // lowered every base's argument list into this frame and left the
        // bindings live, so that an argument list which assigns to a parameter
        // is visible to the body that owns it. Rebinding from the initially
        // lowered arguments here would undo such an assignment.
        if !self.prepared_constructors.remove(&constructor_id) {
            return self.cx.report_unsupported(modifier.span, "base constructor arguments");
        }
        let saved_parameters = self.snapshot_bindings(constructor.parameters);

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
        self.restore_bindings(&saved_parameters);
        Some(())
    }

    pub(super) fn lower_modifier_placeholder(&mut self, span: Span) -> Option<()> {
        let Some(context) = self.modifiers.pop() else {
            return self.cx.report_unsupported(span, "modifier placeholder");
        };
        self.builder.replace_modifier_depth(self.modifier_depth);
        let continuation = self.builder.create_block();
        let before_values = self.values.clone();
        let before_storage_refs = self.storage_refs.clone();
        self.restore_bindings(&context.parameters);
        self.restore_bindings(&context.returns);
        self.push_return_target(continuation);
        self.modifier_depth = self.modifier_depth.saturating_add(1);
        self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_modifier_at(context.modifiers, context.body, context.next);
        self.modifier_depth = self.modifier_depth.saturating_sub(1);
        self.builder.replace_modifier_depth(self.modifier_depth);
        result?;
        if !self.is_terminated() {
            self.record_return_state();
            self.builder.jump(continuation);
        }
        self.finish_return_target(before_values, before_storage_refs);
        // Function parameters are inputs to every body expansion. Return
        // bindings, in contrast, carry the selected body's result onward.
        self.restore_bindings(&context.parameters);
        self.modifiers.push(context);
        self.builder.replace_modifier_depth(self.modifier_depth);
        Some(())
    }
}
