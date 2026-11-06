//! Modifier and base-constructor expansion.

use super::*;

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
            return self.lower_nested_modifier(modifiers, body, index + 1);
        }
        let Some(modifier_id) = self.cx.gcx.resolve_modifier_target(self.cx.contract_id, modifier)
        else {
            return self.cx.report_unsupported(modifier.span, "base constructor modifier");
        };
        let modifier_function = self.cx.gcx.hir.function(modifier_id);
        if modifier_function.kind == hir::FunctionKind::Constructor {
            return self.lower_nested_modifier(modifiers, body, index + 1);
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
        // Named arguments name the parameters of the modifier the invocation
        // statically refers to, which is the declaration the type checker
        // validated them against. An override's signature does not carry
        // parameter names, so resolving them against the virtually selected
        // implementation instead would reorder same-typed arguments whenever
        // the override renames its parameters.
        let parameter_names = match modifier.args.kind {
            hir::CallArgsKind::Named(_) => {
                // Unreachable today: a modifier invocation always resolves to a function,
                // unlike a base constructor call, which is lowered elsewhere. Kept as a
                // fail-closed guard, because binding named arguments without the declared
                // parameter names would have to guess an order.
                let Some(declared_id) = modifier.id.as_function() else {
                    return self.cx.report_unsupported(modifier.span, "modifier target");
                };
                Some(self.cx.gcx.callable_param_names(CallableParamSource::Function {
                    id: declared_id,
                    skips_receiver: false,
                }))
            }
            hir::CallArgsKind::Unnamed(_) => None,
        };
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
                    let access = this.require_storage_access(argument)?;
                    this.storage_refs.insert(parameter, access);
                } else {
                    let carried = this.lower_typed_expr(argument, parameter_ty)?;
                    let value = this.bind_word(parameter, carried, argument.span)?;
                    this.values.insert(parameter, value);
                }
                Some(())
            },
        )?;

        let context = ModifierContext {
            modifiers,
            body,
            next: index + 1,
            returns: self.snapshot_bindings(&self.returns),
            incoming_returns,
        };
        // A modifier's output starts with the incoming return bindings. Its
        // argument expressions still form the input frame used by each `_`.
        self.restore_bindings(&context.incoming_returns);
        self.modifiers.push(context);
        let result = self.lower_block(modifier_body);
        self.modifiers.pop();
        result
    }

    /// Lowers the chain from `next` one modifier level deeper.
    ///
    /// Like solc's `appendModifierOrFunctionCode`, which counts every entry of the list, a base
    /// constructor call in the list adds a level too.
    fn lower_nested_modifier(
        &mut self,
        modifiers: &'gcx [hir::Modifier<'gcx>],
        body: hir::Block<'gcx>,
        next: usize,
    ) -> Option<()> {
        self.modifier_depth = self.modifier_depth.saturating_add(1);
        self.builder.replace_modifier_depth(self.modifier_depth);
        let result = self.lower_modifier_at(modifiers, body, next);
        self.modifier_depth = self.modifier_depth.saturating_sub(1);
        self.builder.replace_modifier_depth(self.modifier_depth);
        result
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
            return self.cx.report_unsupported(modifier.span, "base constructor argument list");
        }
        let contract_id = constructor.contract;
        // The parameters are already bound: `prepare_base_constructor_arguments`
        // lowered every base's argument list into this frame and left the
        // bindings live, so that an argument list which assigns to a parameter
        // is visible to the body that owns it. Rebinding from the initially
        // lowered arguments here would undo such an assignment.
        if !self.prepared_constructors.remove(&constructor_id) {
            return self.cx.report_unsupported(modifier.span, "base constructor argument list");
        }

        // A base constructor has no returns, so nothing flows out of its frame.
        self.lower_return_frame(constructor.span, |this| {
            if let Some(contract_id) = contract_id {
                this.lower_state_initializers(contract_id)?;
            }
            this.lower_function_body(constructor.modifiers, body, constructor.span)
        })
    }

    pub(super) fn lower_modifier_placeholder(&mut self, span: Span) -> Option<()> {
        let Some(context) = self.modifiers.pop() else {
            return self.cx.report_unsupported(span, "modifier placeholder");
        };
        self.lower_return_frame(span, |this| {
            this.restore_bindings(&context.returns);
            this.lower_nested_modifier(context.modifiers, context.body, context.next)
        })?;
        self.modifiers.push(context);
        Some(())
    }
}
