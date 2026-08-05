//! Modifier and base-constructor expansion.

use super::*;

impl<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
    FunctionLowerer<'gcx, 'mir, 'ids, 'bytes, 'events, 'module, 'pointers>
{
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
                saved_parameters.push((parameter, self.values.insert(parameter, value)));
            }
        }

        self.modifiers.push(ModifierContext { modifiers, body, next: index + 1 });
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
        result
    }

    pub(super) fn lower_base_constructor(
        &mut self,
        modifier: &hir::Modifier<'_>,
        constructor_id: hir::FunctionId,
        constructor: &'gcx hir::Function<'gcx>,
        lowered: &mut FxHashSet<hir::ContractId>,
    ) -> Option<()> {
        let Some(body) = constructor.body else {
            return report_unsupported(self.gcx, modifier.span, "base constructor body");
        };
        if modifier.args.len() != constructor.parameters.len() {
            return report_unsupported(self.gcx, modifier.span, "base constructor arguments");
        }
        let parameter_names = self.gcx.callable_param_names(CallableParamSource::Function {
            id: constructor_id,
            skips_receiver: false,
        });
        let mut saved_parameters = Vec::with_capacity(constructor.parameters.len());
        for (index, &parameter) in constructor.parameters.iter().enumerate() {
            let Some(argument) =
                modifier.args.argument_for_parameter(index, Some(parameter_names.as_slice()))
            else {
                return report_unsupported(
                    self.gcx,
                    modifier.span,
                    "named base constructor argument",
                );
            };
            let parameter_ty = self.gcx.type_of_item(parameter.into());
            let value = self.lower_typed_expr(argument, parameter_ty)?;
            saved_parameters.push((parameter, self.values.insert(parameter, value)));
        }

        let continuation = self.builder.create_block();
        self.return_targets.push(continuation);
        if let Some(contract_id) = constructor.contract {
            self.lower_implicit_base_constructors_inner(contract_id, lowered)?;
        }
        let result = self.lower_function_body(constructor.modifiers, body);
        self.return_targets.pop();
        result?;
        if !self.is_terminated() {
            self.builder.jump(continuation);
        }
        self.builder.switch_to_block(continuation);
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
        let Some(context) = self.modifiers.last().copied() else {
            return report_unsupported(self.gcx, span, "modifier placeholder");
        };
        let continuation = self.builder.create_block();
        self.return_targets.push(continuation);
        let result = self.lower_modifier_at(context.modifiers, context.body, context.next);
        self.return_targets.pop();
        result?;
        if !self.is_terminated() {
            self.builder.jump(continuation);
        }
        self.builder.switch_to_block(continuation);
        Some(())
    }
}
