//! Tuple, return, and multi-value lowering.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        let expr = expr.peel_parens();
        if let ExprKind::Ternary(condition, then_expr, else_expr) = &expr.kind {
            return self.lower_ternary_values(condition, then_expr, else_expr);
        }
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            if let Some(builtin) = self.context.gcx.resolved_builtin(callee)
                && matches!(
                    builtin,
                    Builtin::AddressCall
                        | Builtin::AddressStaticcall
                        | Builtin::AddressDelegatecall
                )
                && let ExprKind::Member(receiver, _) = callee.kind
            {
                let (success, returndata) = self.lower_address_call_result(
                    expr.span, receiver, builtin, *args, *call_opts, true,
                )?;
                let returndata = returndata?;
                return Some(vec![success, returndata]);
            }
            if self.context.gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode)
                && let ExprKind::Call(_, args, _) = &expr.kind
                && let Some(types) = args.exprs().nth(1)
                && let ExprKind::Tuple(elements) = types.kind
                && elements.len() > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let mut values = Vec::with_capacity(elements.len());
                values.push(first);
                for index in 1..elements.len() {
                    let ty = elements[index].and_then(|element| {
                        let TyKind::Type(ty) = self.context.gcx.type_of_expr(element.id)?.kind
                        else {
                            return None;
                        };
                        Some(ty)
                    });
                    let value = match ty {
                        Some(ty) => {
                            self.load_multi_return_value_as(base, index, elements.len(), ty)
                        }
                        None => self.load_multi_return_value(base, index, elements.len()),
                    };
                    values.push(value);
                }
                return Some(values);
            }
            let returns = self
                .context
                .gcx
                .resolved_function(callee)
                .map(|function_id| self.context.gcx.hir.function(function_id).returns.len())
                .or_else(|| {
                    self.context.gcx.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                        TyKind::Fn(function)
                            if function.function_id.is_none()
                                && (function.is_internal() || function.is_external()) =>
                        {
                            Some(function.returns.len())
                        }
                        _ => None,
                    })
                });
            if let Some(TyKind::Fn(function)) =
                self.context.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
                && function.is_external()
                && function.function_id.is_none()
            {
                return self.lower_external_function_pointer_call_values(
                    callee, function, *args, *call_opts,
                );
            }
            if let Some(returns) = returns
                && returns > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let mut values = Vec::with_capacity(returns);
                values.push(first);
                for index in 1..returns {
                    let ty = self
                        .context
                        .gcx
                        .resolved_function(callee)
                        .and_then(|function_id| {
                            self.context
                                .gcx
                                .hir
                                .function(function_id)
                                .returns
                                .get(index)
                                .map(|&id| self.context.gcx.type_of_item(id.into()))
                        })
                        .or_else(|| {
                            self.context.gcx.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                                TyKind::Fn(function) => function.returns.get(index).copied(),
                                _ => None,
                            })
                        });
                    let value = match ty {
                        Some(ty) => self.load_multi_return_value_as(base, index, returns, ty),
                        None => self.load_multi_return_value(base, index, returns),
                    };
                    values.push(value);
                }
                return Some(values);
            }
            let returns_empty = returns.is_some_and(|returns| returns == 0)
                || self.context.gcx.resolved_builtin(callee).is_some_and(|builtin| {
                    matches!(builtin, Builtin::Assert | Builtin::Revert | Builtin::RevertMsg)
                });
            if returns_empty {
                self.lower_expr(expr)?;
                return Some(Vec::new());
            }
        }
        match &expr.kind {
            ExprKind::Tuple(values) => {
                values.iter().flatten().map(|expr| self.lower_expr(expr)).collect()
            }
            _ => Some(vec![self.lower_expr(expr)?]),
        }
    }

    pub(super) fn lower_return_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        if self.returns.len() > 1
            && let ExprKind::Tuple(values) = &expr.peel_parens().kind
            && values.len() == self.returns.len()
        {
            let returns = self.returns.clone();
            return values
                .iter()
                .zip(returns)
                .map(|(value, id)| {
                    let value = (*value)?;
                    let ty = self.context.gcx.type_of_item(id.into());
                    if ty.is_ref_at(DataLocation::Storage) {
                        self.storage_access(value).map(|access| access.slot)
                    } else {
                        self.lower_typed_expr(value, ty)
                    }
                })
                .collect();
        }

        self.lower_values(expr)
    }

    pub(super) fn lower_tuple_assignment(
        &mut self,
        elements: &[Option<&hir::Expr<'_>>],
        rhs: &hir::Expr<'_>,
    ) -> Option<()> {
        let rhs = rhs.peel_parens();
        if elements.iter().flatten().any(|element| self.is_storage_reference_binding(element))
            && let Some(values) = self.lower_storage_reference_call(rhs)
        {
            if values.len() != elements.len() {
                return report_unsupported(self.context.gcx, rhs.span, "storage reference tuple");
            }
            for (element, (value, access)) in elements.iter().zip(values) {
                let Some(element) = element else { continue };
                if let Some(access) = access {
                    if !self.is_storage_reference_binding(element) {
                        return report_unsupported(
                            self.context.gcx,
                            element.span,
                            "mixed storage tuple",
                        );
                    }
                    let Some(id) = self.context.gcx.resolved_variable(element) else {
                        return report_unsupported(
                            self.context.gcx,
                            element.span,
                            "storage reference target",
                        );
                    };
                    self.storage_refs.insert(id, access);
                } else {
                    if self.is_storage_reference_binding(element) {
                        return report_unsupported(
                            self.context.gcx,
                            element.span,
                            "mixed storage tuple",
                        );
                    }
                    self.store_lvalue(element, value)?;
                }
            }
            return Some(());
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind
            && rhs_elements.len() == elements.len()
            && elements.iter().flatten().any(|element| self.is_storage_reference_binding(element))
        {
            let tuple_span = rhs.span;
            let mut bindings = Vec::with_capacity(elements.len());
            for (lhs, rhs) in elements.iter().zip(rhs_elements.iter()) {
                let Some(rhs) = rhs else {
                    if lhs.is_some() {
                        return report_unsupported(
                            self.context.gcx,
                            tuple_span,
                            "storage reference tuple",
                        );
                    }
                    continue;
                };
                let Some(lhs) = lhs else {
                    self.lower_expr(rhs)?;
                    continue;
                };
                if !self.is_storage_reference_binding(lhs) {
                    return report_unsupported(self.context.gcx, lhs.span, "mixed storage tuple");
                }
                let access = self.storage_access(rhs)?;
                let Some(id) = self.context.gcx.resolved_variable(lhs) else {
                    return report_unsupported(
                        self.context.gcx,
                        lhs.span,
                        "storage reference target",
                    );
                };
                bindings.push((id, access));
            }
            for (id, access) in bindings {
                self.storage_refs.insert(id, access);
            }
            return Some(());
        }
        if self.is_low_level_call_expr(rhs) {
            let values = self.lower_low_level_call_values(
                rhs,
                elements.iter().flatten().count(),
                elements.first().is_some_and(Option::is_none),
            )?;
            if values.len() != elements.iter().flatten().count() {
                return report_unsupported(self.context.gcx, rhs.span, "tuple assignment arity");
            }
            for (element, value) in elements.iter().flatten().zip(values) {
                self.store_lvalue(element, value)?;
            }
            return Some(());
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind {
            if rhs_elements.iter().filter(|element| element.is_some()).count() == 1
                && rhs_elements.iter().any(Option::is_none)
                && let Some(rhs) = rhs_elements.iter().flatten().next()
            {
                let values = self.lower_values(rhs)?;
                if values.len() >= elements.len() {
                    for (element, value) in elements.iter().zip(values) {
                        if let Some(element) = element {
                            self.store_lvalue(element, value)?;
                        }
                    }
                    return Some(());
                }
            }
            if rhs_elements.len() < elements.len() {
                return report_unsupported(self.context.gcx, rhs.span, "tuple assignment arity");
            }
            let mut values = Vec::with_capacity(rhs_elements.len());
            for (index, value) in rhs_elements.iter().enumerate() {
                let Some(value) = value else {
                    if elements.get(index).is_some_and(Option::is_some) {
                        return report_unsupported(
                            self.context.gcx,
                            rhs.span,
                            "tuple assignment value",
                        );
                    }
                    continue;
                };
                values.push((index, value, self.lower_expr(value)?));
            }
            for (index, rhs, value) in values {
                let Some(Some(element)) = elements.get(index) else { continue };
                let lhs_ty = self.type_of_expr_or_variable(element)?;
                let rhs_ty = self.context.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let value = if lhs_ty.is_ref_at(DataLocation::Storage) {
                    value
                } else {
                    self.materialize_memory_argument(lhs_ty, value, rhs.span)?
                };
                let value = self.coerce_value(value, rhs_ty, lhs_ty);
                self.store_lvalue_with_source(element, value, Some(rhs_ty))?;
            }
            return Some(());
        }
        let values = self.lower_values(rhs)?;
        if values.len() < elements.len() {
            return report_unsupported(self.context.gcx, rhs.span, "tuple assignment arity");
        }
        for (element, value) in elements.iter().zip(values) {
            if let Some(element) = element {
                self.store_lvalue(element, value)?;
            }
        }
        Some(())
    }

    pub(super) fn lower_storage_reference_call(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<Vec<(ValueId, Option<StorageAccess>)>> {
        let ExprKind::Call(callee, ..) = &expr.kind else { return None };
        let function_id = self.context.gcx.resolved_function(callee)?;
        let returns = self.context.gcx.hir.function(function_id).returns;
        if returns.is_empty() {
            return None;
        }
        let has_storage_return = returns
            .iter()
            .any(|&id| self.context.gcx.type_of_item(id.into()).is_ref_at(DataLocation::Storage));
        if !has_storage_return {
            return None;
        }
        let values = self.lower_values(expr)?;
        (values.len() == returns.len()).then(|| {
            values
                .into_iter()
                .zip(returns)
                .map(|(value, id)| {
                    let access = self
                        .context
                        .gcx
                        .type_of_item((*id).into())
                        .is_ref_at(DataLocation::Storage)
                        .then(|| StorageAccess {
                            slot: value,
                            location: StorageLocation::word(U256::ZERO),
                            offset: None,
                        });
                    (value, access)
                })
                .collect()
        })
    }

    pub(super) fn multi_return_buffer_base(&mut self) -> ValueId {
        self.builder.frame_load(0, FrameMode::MultiReturn, FrameSlotKind::Word)
    }

    pub(super) fn ensure_multi_return_buffer(
        &mut self,
        words: usize,
    ) -> (ValueId, ValueId, MemoryObjectLayout) {
        debug_assert!(words > 1);
        // The published pointer has no capacity, so each producer gets a fresh object.
        let words = u64::try_from(words).unwrap_or(u64::MAX);
        let (object, layout) = self.builder.alloc_word_array(words, AllocationSemantics::INTERNAL);
        let base = self.builder.memory_object_data(object, MemoryObjectKind::FixedArray);
        self.builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, base);
        (object, base, layout)
    }

    pub(super) fn load_multi_return_value(
        &mut self,
        base: ValueId,
        index: usize,
        words: usize,
    ) -> ValueId {
        let offset = self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX) * 32);
        let size =
            self.builder.imm_u64(u64::try_from(words).unwrap_or(u64::MAX).saturating_mul(32));
        let slice = self.builder.make_slice(base, size, SliceLocation::Memory);
        self.builder.memory_slice_load_word(slice, offset)
    }

    pub(super) fn load_multi_return_value_as(
        &mut self,
        base: ValueId,
        index: usize,
        returns: usize,
        ty: Ty<'gcx>,
    ) -> ValueId {
        let kind = match types::TypeLowerer::mir_type(ty) {
            MirType::MemoryObject(kind) => kind,
            _ => return self.load_multi_return_value(base, index, returns),
        };
        let index = self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX));
        self.builder.memory_object_load_object(
            base,
            MemoryObjectLayout::word_fixed_array(u64::try_from(returns).unwrap_or(u64::MAX)),
            index,
            kind,
        )
    }
}
