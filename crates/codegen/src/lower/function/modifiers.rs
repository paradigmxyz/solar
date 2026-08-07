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

    fn modifier_local_ids(&self, body: hir::Block<'_>) -> Vec<hir::VariableId> {
        let mut ids = FxHashSet::default();
        for stmt in body.stmts {
            Self::collect_modifier_stmt_ids(stmt, &mut ids);
        }
        ids.into_iter().collect()
    }

    fn collect_modifier_stmt_ids(stmt: &hir::Stmt<'_>, ids: &mut FxHashSet<hir::VariableId>) {
        match &stmt.kind {
            hir::StmtKind::DeclSingle(id) => {
                ids.insert(*id);
            }
            hir::StmtKind::DeclMulti(declarations, _) => {
                ids.extend(declarations.iter().flatten().copied());
            }
            hir::StmtKind::Block(block)
            | hir::StmtKind::UncheckedBlock(block)
            | hir::StmtKind::AssemblyBlock(block)
            | hir::StmtKind::Loop(block, _) => {
                for stmt in block.stmts {
                    Self::collect_modifier_stmt_ids(stmt, ids);
                }
            }
            hir::StmtKind::If(_, then_stmt, else_stmt) => {
                Self::collect_modifier_stmt_ids(then_stmt, ids);
                if let Some(else_stmt) = else_stmt {
                    Self::collect_modifier_stmt_ids(else_stmt, ids);
                }
            }
            hir::StmtKind::Switch(switch) => {
                for case in switch.cases {
                    for stmt in case.body.stmts {
                        Self::collect_modifier_stmt_ids(stmt, ids);
                    }
                }
            }
            hir::StmtKind::Try(try_stmt) => {
                for clause in try_stmt.clauses {
                    ids.extend(clause.args.iter().copied());
                    for stmt in clause.block.stmts {
                        Self::collect_modifier_stmt_ids(stmt, ids);
                    }
                }
            }
            hir::StmtKind::Emit(_)
            | hir::StmtKind::Revert(_)
            | hir::StmtKind::Return(_)
            | hir::StmtKind::Break
            | hir::StmtKind::Continue
            | hir::StmtKind::Expr(_)
            | hir::StmtKind::Placeholder
            | hir::StmtKind::Err(_) => {}
        }
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
        let contract_id = constructor.contract;
        // Root base initializers run before their constructor arguments; derived
        // initializers wait until ancestor constructors finish.
        let has_ancestors =
            contract_id.is_some_and(|id| self.gcx.hir.contract(id).linearized_bases.len() > 1);
        if !has_ancestors && let Some(contract_id) = contract_id {
            self.lower_state_initializers(contract_id)?;
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
            let value = self.coerce_call_argument(argument, parameter_ty, value);
            saved_parameters.push((parameter, self.values.insert(parameter, value)));
        }

        let continuation = self.builder.create_block();
        self.return_targets.push(continuation);
        if let Some(contract_id) = contract_id {
            self.lower_implicit_base_constructors_inner(contract_id, lowered)?;
            if has_ancestors {
                self.lower_state_initializers(contract_id)?;
            }
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
