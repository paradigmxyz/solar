//! Tuple, return, and multi-value lowering.

use super::*;

enum PreparedTupleAssignment<'gcx> {
    Value { place: LValuePlace<'gcx>, value: ValueId, source_ty: Option<Ty<'gcx>> },
    StorageReference { id: VariableId, access: StorageAccess },
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        let expr = expr.peel_parens();
        if let ExprKind::Ternary(condition, then_expr, else_expr) = &expr.kind {
            return self.lower_ternary_values(condition, then_expr, else_expr);
        }
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            let resolved_builtin = self.context.gcx.resolved_builtin(callee);
            if let Some(builtin) = resolved_builtin
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
            if resolved_builtin == Some(Builtin::AbiDecode)
                && let Some(types) = args.exprs().nth(1)
                && let ExprKind::Tuple(elements) = types.kind
                && elements.len() > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let gcx = self.context.gcx;
                let return_types = elements.iter().skip(1).copied().map(move |element| {
                    element.and_then(|element| match gcx.type_of_expr(element.id)?.kind {
                        TyKind::Type(ty) => Some(ty),
                        _ => None,
                    })
                });
                return Some(self.load_multi_return_values(
                    first,
                    base,
                    elements.len(),
                    return_types,
                ));
            }
            let function_pointer =
                self.context.gcx.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                    TyKind::Fn(function)
                        if function.function_id.is_none()
                            && (function.is_internal() || function.is_external()) =>
                    {
                        Some(function)
                    }
                    _ => None,
                });
            let returns = self
                .context
                .gcx
                .resolved_function(callee)
                .map(|function_id| self.context.gcx.hir.function(function_id).returns.len())
                .or_else(|| function_pointer.map(|function| function.returns.len()));
            if let Some(function) = function_pointer
                && function.is_external()
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
                let gcx = self.context.gcx;
                let resolved_function = gcx.resolved_function(callee);
                let pointer_returns = function_pointer.map(|function| function.returns);
                let return_types = (1..returns).map(move |index| {
                    resolved_function
                        .and_then(|function_id| {
                            gcx.hir
                                .function(function_id)
                                .returns
                                .get(index)
                                .map(|&id| gcx.type_of_item(id.into()))
                        })
                        .or_else(|| pointer_returns.and_then(|returns| returns.get(index).copied()))
                });
                return Some(self.load_multi_return_values(first, base, returns, return_types));
            }
            let returns_empty = returns.is_some_and(|returns| returns == 0)
                || resolved_builtin.is_some_and(|builtin| {
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
        if self.returns.len() == 1 {
            let ty = self.context.gcx.type_of_item(self.returns[0].into());
            if ty.is_ref_at(DataLocation::Storage) {
                return Some(vec![self.storage_access(expr)?.slot]);
            }
            return Some(vec![self.lower_typed_expr(expr, ty)?]);
        }
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

    pub(super) fn lower_tuple_assignment<'hir>(
        &mut self,
        elements: &[Option<&'hir hir::Expr<'hir>>],
        rhs: &'hir hir::Expr<'hir>,
    ) -> Option<()> {
        let rhs = rhs.peel_parens();
        if elements.iter().flatten().any(|element| {
            // Memory-typed reference elements must also route through the copy
            // path: the generic path would store the callee's raw storage slot
            // as if it were a memory pointer.
            self.is_storage_reference_binding(element)
                || self.type_of_expr_or_variable(element).is_some_and(|ty| {
                    ty.is_ref_at(DataLocation::Storage) || ty.is_ref_at(DataLocation::Memory)
                })
        }) && let Some(values) = self.lower_storage_reference_call(rhs)
        {
            if values.len() != elements.len() {
                return report_unsupported(self.context.gcx, rhs.span, "storage reference tuple");
            }
            let mut assignment_values = Vec::with_capacity(elements.len());
            for (element, (mut value, access)) in elements.iter().zip(values) {
                let Some(element) = element else { continue };
                if let Some(access) = access {
                    if !self.is_storage_reference_binding(element) {
                        // A storage-reference return assigned to a plain
                        // storage lvalue copies the referenced value, matching
                        // solc: the reference itself only binds to a local
                        // storage variable.
                        let ty = self.type_of_expr_or_variable(element)?;
                        value = self.load_storage_object(ty, value, element.span)?;
                        assignment_values.push((*element, value, None));
                    } else {
                        assignment_values.push((*element, value, Some(access)));
                    }
                } else if self.is_storage_reference_binding(element) {
                    return report_unsupported(
                        self.context.gcx,
                        element.span,
                        "mixed storage tuple",
                    );
                } else {
                    assignment_values.push((*element, value, None));
                }
            }
            let mut assignments = Vec::with_capacity(assignment_values.len());
            for (element, value, access) in assignment_values {
                if let Some(access) = access {
                    let Some(id) = self.context.gcx.resolved_variable(element) else {
                        return report_unsupported(
                            self.context.gcx,
                            element.span,
                            "storage reference target",
                        );
                    };
                    assignments.push(PreparedTupleAssignment::StorageReference { id, access });
                } else {
                    assignments.push(PreparedTupleAssignment::Value {
                        place: self.resolve_lvalue_place(element)?,
                        value,
                        source_ty: None,
                    });
                }
            }
            return self.apply_tuple_assignments(assignments);
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind
            && rhs_elements.len() == elements.len()
            && elements.iter().flatten().any(|element| self.is_storage_reference_binding(element))
        {
            let tuple_span = rhs.span;
            let mut assignments = Vec::with_capacity(elements.len());
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
                assignments.push(PreparedTupleAssignment::StorageReference { id, access });
            }
            return self.apply_tuple_assignments(assignments);
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
            return self.store_tuple_values(
                elements
                    .iter()
                    .flatten()
                    .copied()
                    .zip(values)
                    .map(|(element, value)| (element, value, None)),
            );
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind {
            if rhs_elements.iter().filter(|element| element.is_some()).count() == 1
                && rhs_elements.iter().any(Option::is_none)
                && let Some(rhs) = rhs_elements.iter().flatten().next()
            {
                let values = self.lower_values(rhs)?;
                if values.len() >= elements.len() {
                    return self.store_tuple_values(elements.iter().zip(values).filter_map(
                        |(element, value)| element.map(|element| (element, value, None)),
                    ));
                }
            }
            let mut values = Vec::with_capacity(rhs_elements.len());
            self.lower_tuple_assignment_values(elements, rhs_elements, rhs.span, &mut values)?;
            let mut assignments = Vec::with_capacity(values.len());
            for (element, rhs, value) in values {
                let lhs_ty = self.type_of_expr_or_variable(element)?;
                let rhs_ty = self.context.gcx.type_of_expr(rhs.id).unwrap_or(lhs_ty);
                let value = if lhs_ty.is_ref_at(DataLocation::Storage) {
                    value
                } else {
                    self.materialize_memory_argument(lhs_ty, value, rhs.span)?
                };
                let value = self.coerce_value(value, rhs_ty, lhs_ty);
                assignments.push((element, value, Some(rhs_ty)));
            }
            return self.store_tuple_values(assignments);
        }
        let values = self.lower_values(rhs)?;
        if values.len() < elements.len() {
            return report_unsupported(self.context.gcx, rhs.span, "tuple assignment arity");
        }
        self.store_tuple_values(
            elements
                .iter()
                .zip(values)
                .filter_map(|(element, value)| element.map(|element| (element, value, None))),
        )
    }

    fn store_tuple_values<'hir>(
        &mut self,
        values: impl IntoIterator<Item = (&'hir hir::Expr<'hir>, ValueId, Option<Ty<'gcx>>)>,
    ) -> Option<()> {
        // Solidity evaluates tuple targets left-to-right after the RHS, then
        // commits their writes right-to-left.
        let assignments = values
            .into_iter()
            .map(|(element, value, source_ty)| {
                Some(PreparedTupleAssignment::Value {
                    place: self.resolve_lvalue_place(element)?,
                    value,
                    source_ty,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        self.apply_tuple_assignments(assignments)
    }

    fn apply_tuple_assignments(
        &mut self,
        assignments: Vec<PreparedTupleAssignment<'gcx>>,
    ) -> Option<()> {
        for assignment in assignments.into_iter().rev() {
            match assignment {
                PreparedTupleAssignment::Value { mut place, value, source_ty } => {
                    if let LValuePlace::StorageByte { slot, index, ty, .. } = place {
                        // Place resolution materializes storage bytes for its bounds check. Reload
                        // before each write so aliased byte targets do not restore a stale copy.
                        let object = self.load_storage_bytes(slot)?;
                        place = LValuePlace::StorageByte { slot, object, index, ty };
                    }
                    self.store_lvalue_place_with_source(&place, value, source_ty)?;
                }
                PreparedTupleAssignment::StorageReference { id, access } => {
                    self.storage_refs.insert(id, access);
                }
            }
        }
        Some(())
    }

    fn lower_tuple_assignment_values<'hir>(
        &mut self,
        elements: &[Option<&'hir hir::Expr<'hir>>],
        rhs_elements: &[Option<&'hir hir::Expr<'hir>>],
        span: Span,
        values: &mut Vec<(&'hir hir::Expr<'hir>, &'hir hir::Expr<'hir>, ValueId)>,
    ) -> Option<()> {
        if rhs_elements.len() < elements.len() {
            return report_unsupported(self.context.gcx, span, "tuple assignment arity");
        }
        for (index, rhs) in rhs_elements.iter().enumerate() {
            let lhs = elements.get(index).copied().flatten();
            let Some(rhs) = rhs else {
                if lhs.is_some() {
                    return report_unsupported(self.context.gcx, span, "tuple assignment value");
                }
                continue;
            };
            let ExprKind::Tuple(nested_rhs) = &rhs.peel_parens().kind else {
                let value = self.lower_expr(rhs)?;
                if let Some(lhs) = lhs {
                    values.push((lhs, rhs, value));
                }
                continue;
            };
            let Some(lhs) = lhs else {
                self.lower_tuple_assignment_values(&[], nested_rhs, rhs.span, values)?;
                continue;
            };
            let ExprKind::Tuple(nested_lhs) = &lhs.peel_parens().kind else {
                return report_unsupported(self.context.gcx, lhs.span, "tuple assignment target");
            };
            self.lower_tuple_assignment_values(nested_lhs, nested_rhs, rhs.span, values)?;
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
        let offset =
            self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32));
        debug_assert!(index < words);
        let position = self.builder.add(base, offset);
        self.builder.mload(position)
    }

    pub(super) fn load_multi_return_value_as(
        &mut self,
        base: ValueId,
        index: usize,
        returns: usize,
        ty: Ty<'gcx>,
    ) -> ValueId {
        let MirType::MemoryObject(kind) = types::TypeLowerer::mir_type(ty) else {
            return self.load_multi_return_value(base, index, returns);
        };
        let index = self.builder.imm_u64(u64::try_from(index).unwrap_or(u64::MAX));
        self.builder.memory_object_load_object(
            base,
            MemoryObjectLayout::word_fixed_array(u64::try_from(returns).unwrap_or(u64::MAX)),
            index,
            kind,
        )
    }

    pub(super) fn load_multi_return_values(
        &mut self,
        first: ValueId,
        base: ValueId,
        returns: usize,
        return_types: impl IntoIterator<Item = Option<Ty<'gcx>>>,
    ) -> Vec<ValueId> {
        let mut values = Vec::with_capacity(returns);
        values.push(first);
        for (index, ty) in return_types.into_iter().enumerate() {
            let index = index + 1;
            values.push(match ty {
                Some(ty) => self.load_multi_return_value_as(base, index, returns, ty),
                None => self.load_multi_return_value(base, index, returns),
            });
        }
        values
    }
}
