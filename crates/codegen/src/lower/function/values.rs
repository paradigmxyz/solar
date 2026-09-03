//! Tuple, return, and multi-value lowering.

use super::*;

enum PreparedTupleAssignment<'gcx> {
    Value { place: LValuePlace<'gcx>, rhs: TupleAssignmentRhs<'gcx> },
    StorageReference { id: VariableId, access: StorageAccess },
}

enum TupleAssignmentRhs<'gcx> {
    Materialized { value: ValueId, source_ty: Option<Ty<'gcx>>, span: Span },
    // Capture the source slot during RHS evaluation, but copy its contents when
    // this assignment is committed.
    StorageCopy { access: StorageAccess, source_ty: Ty<'gcx>, span: Span },
    StorageReference { access: StorageAccess },
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        let expr = expr.peel_parens();
        if let ExprKind::Ternary(condition, then_expr, else_expr) = &expr.kind {
            return self.lower_ternary_values(condition, then_expr, else_expr);
        }
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            if let Some(builtin) = self.low_level_call_builtin(expr) {
                return self.lower_low_level_call_values(expr, builtin, 2, false);
            }
            let resolved_builtin = self.cx.gcx.resolved_builtin(callee);
            if resolved_builtin == Some(Builtin::AbiDecode)
                && let Some(types) = args.exprs().nth(1)
                && let ExprKind::Tuple(elements) = types.kind
                && elements.len() > 1
            {
                let first = self.lower_expr(expr)?;
                let base = self.multi_return_buffer_base();
                let gcx = self.cx.gcx;
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
            let function_ty = self.cx.gcx.type_of_expr(callee.id).and_then(|ty| match ty.kind {
                TyKind::Fn(function) => Some(function),
                _ => None,
            });
            let function_pointer = function_ty.filter(|function| function.function_id.is_none());
            let resolved_function = self.cx.gcx.resolved_function(callee);
            let returns = resolved_function
                .map(|function_id| self.cx.gcx.hir.function(function_id).returns.len())
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
                let gcx = self.cx.gcx;
                let return_types = if let Some(function_id) = resolved_function {
                    gcx.hir
                        .function(function_id)
                        .returns
                        .iter()
                        .map(|&id| gcx.type_of_item(id.into()))
                        .collect::<Vec<_>>()
                } else {
                    function_pointer?.returns.to_vec()
                };
                if function_ty.is_some_and(|function| !function.is_internal()) {
                    let base = self.multi_return_buffer_base();
                    let gcx = self.cx.gcx;
                    let return_types = return_types
                        .iter()
                        .skip(1)
                        .map(|ty| Some(ty.with_loc_if_ref(gcx, DataLocation::Memory)));
                    return Some(self.load_multi_return_values(first, base, returns, return_types));
                }
                return Some(self.load_internal_return_values(first, &return_types));
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
            let ty = self.cx.gcx.type_of_item(self.returns[0].into());
            if ty.is_ref_at(DataLocation::Storage) {
                let Some(access) = self.storage_access(expr) else {
                    return self.cx.report_unsupported(expr.span, "storage access");
                };
                return Some(vec![access.slot]);
            }
            let value = self.lower_typed_expr(expr, ty)?;
            let value = if ty.is_ref_at(DataLocation::Memory) {
                self.materialize_memory_argument(ty, value, expr.span)?
            } else {
                value
            };
            return Some(vec![value]);
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
                    let ty = self.cx.gcx.type_of_item(id.into());
                    if ty.is_ref_at(DataLocation::Storage) {
                        let Some(access) = self.storage_access(value) else {
                            return self.cx.report_unsupported(value.span, "storage access");
                        };
                        Some(access.slot)
                    } else {
                        let span = value.span;
                        let value = self.lower_typed_expr(value, ty)?;
                        if ty.is_ref_at(DataLocation::Memory) {
                            self.materialize_memory_argument(ty, value, span)
                        } else {
                            Some(value)
                        }
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
                return self.cx.report_unsupported(rhs.span, "storage reference tuple");
            }
            let mut assignments = Vec::with_capacity(elements.len());
            for (element, (value, source_ty, access)) in elements.iter().zip(values) {
                let Some(element) = element else { continue };
                if let Some(access) = access {
                    if self.is_storage_reference_binding(element) {
                        assignments
                            .push((*element, TupleAssignmentRhs::StorageReference { access }));
                        continue;
                    }
                    let value =
                        TupleAssignmentRhs::StorageCopy { access, source_ty, span: rhs.span };
                    let value = self.prepare_tuple_rhs(element, value)?;
                    assignments.push((*element, value));
                } else if self.is_storage_reference_binding(element) {
                    return self.cx.report_unsupported(element.span, "mixed storage tuple");
                } else {
                    let value = self.prepare_tuple_rhs(
                        element,
                        TupleAssignmentRhs::Materialized {
                            value,
                            source_ty: Some(source_ty),
                            span: rhs.span,
                        },
                    )?;
                    assignments.push((*element, value));
                }
            }
            return self.store_prepared_tuple_values(assignments);
        }
        if let Some(builtin) = self.low_level_call_builtin(rhs) {
            let values = self.lower_low_level_call_values(
                rhs,
                builtin,
                elements.iter().flatten().count(),
                elements.first().is_some_and(Option::is_none),
            )?;
            if values.len() != elements.iter().flatten().count() {
                return self.cx.report_unsupported(rhs.span, "tuple assignment arity");
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
            let values = values
                .into_iter()
                .map(|(element, rhs)| Some((element, self.prepare_tuple_rhs(element, rhs)?)))
                .collect::<Option<Vec<_>>>()?;
            return self.store_prepared_tuple_values(values);
        }
        let values = self.lower_values(rhs)?;
        if values.len() < elements.len() {
            return self.cx.report_unsupported(rhs.span, "tuple assignment arity");
        }
        self.store_tuple_values(
            elements
                .iter()
                .zip(values)
                .filter_map(|(element, value)| element.map(|element| (element, value, None))),
        )
    }

    fn prepare_tuple_rhs(
        &mut self,
        element: &hir::Expr<'_>,
        rhs: TupleAssignmentRhs<'gcx>,
    ) -> Option<TupleAssignmentRhs<'gcx>> {
        let target_ty = self.type_of_expr_or_variable(element)?;
        let (value, source_ty, span) = match rhs {
            rhs @ TupleAssignmentRhs::StorageCopy { .. } => return Some(rhs),
            TupleAssignmentRhs::Materialized { value, source_ty, span } => (value, source_ty, span),
            TupleAssignmentRhs::StorageReference { access } => {
                return Some(TupleAssignmentRhs::StorageReference { access });
            }
        };
        let source_ty = source_ty.unwrap_or(target_ty);
        let value = if target_ty.is_ref_at(DataLocation::Storage) {
            value
        } else {
            self.materialize_memory_argument(target_ty, value, span)?
        };
        let value = self.coerce_value(value, source_ty, target_ty);
        Some(TupleAssignmentRhs::Materialized { value, source_ty: Some(source_ty), span })
    }

    fn store_tuple_values<'hir>(
        &mut self,
        values: impl IntoIterator<Item = (&'hir hir::Expr<'hir>, ValueId, Option<Ty<'gcx>>)>,
    ) -> Option<()> {
        self.store_prepared_tuple_values(values.into_iter().map(|(element, value, source_ty)| {
            (element, TupleAssignmentRhs::Materialized { value, source_ty, span: element.span })
        }))
    }

    fn store_prepared_tuple_values<'hir>(
        &mut self,
        values: impl IntoIterator<Item = (&'hir hir::Expr<'hir>, TupleAssignmentRhs<'gcx>)>,
    ) -> Option<()> {
        // Solidity evaluates tuple targets left-to-right after the RHS, then
        // commits their writes right-to-left.
        let assignments = values
            .into_iter()
            .map(|(element, value)| {
                Some(match value {
                    TupleAssignmentRhs::StorageReference { access } => {
                        let Some(id) = self.cx.gcx.resolved_variable(element) else {
                            return self
                                .cx
                                .report_unsupported(element.span, "storage reference target");
                        };
                        PreparedTupleAssignment::StorageReference { id, access }
                    }
                    rhs => {
                        let place = self.resolve_lvalue_place(element)?;
                        PreparedTupleAssignment::Value { place, rhs }
                    }
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
                PreparedTupleAssignment::Value { place, rhs } => {
                    let (value, source_ty) = match rhs {
                        TupleAssignmentRhs::Materialized { value, source_ty, .. } => {
                            (value, source_ty)
                        }
                        TupleAssignmentRhs::StorageCopy { access, source_ty, span } => {
                            (self.load_storage_value(source_ty, access, span)?, Some(source_ty))
                        }
                        TupleAssignmentRhs::StorageReference { .. } => {
                            unreachable!("storage reference reached value assignment")
                        }
                    };
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
        values: &mut Vec<(&'hir hir::Expr<'hir>, TupleAssignmentRhs<'gcx>)>,
    ) -> Option<()> {
        if rhs_elements.len() < elements.len() {
            return self.cx.report_unsupported(span, "tuple assignment arity");
        }
        for (index, rhs) in rhs_elements.iter().enumerate() {
            let lhs = elements.get(index).copied().flatten();
            let Some(rhs) = rhs else {
                if lhs.is_some() {
                    return self.cx.report_unsupported(span, "tuple assignment value");
                }
                continue;
            };
            let ExprKind::Tuple(nested_rhs) = &rhs.peel_parens().kind else {
                if let Some(lhs) = lhs
                    && let ExprKind::Tuple(nested_lhs) = &lhs.peel_parens().kind
                {
                    let Some(TyKind::Tuple(rhs_types)) =
                        self.cx.gcx.type_of_expr(rhs.id).map(|ty| ty.kind)
                    else {
                        return self
                            .cx
                            .report_unsupported(rhs.span, "nested tuple assignment value");
                    };
                    let rhs_values = self.lower_values(rhs)?;
                    if rhs_values.len() != nested_lhs.len() || rhs_types.len() != nested_lhs.len() {
                        return self.cx.report_unsupported(rhs.span, "tuple assignment arity");
                    }
                    for ((lhs, value), &rhs_ty) in nested_lhs.iter().zip(rhs_values).zip(rhs_types)
                    {
                        let Some(lhs) = lhs else { continue };
                        if matches!(lhs.peel_parens().kind, ExprKind::Tuple(_)) {
                            return self
                                .cx
                                .report_unsupported(lhs.span, "nested tuple assignment target");
                        }
                        let value = if rhs_ty.is_ref_at(DataLocation::Storage) {
                            let access = StorageAccess {
                                slot: value,
                                location: StorageLocation::word(U256::ZERO),
                                offset: None,
                            };
                            if self.is_storage_reference_binding(lhs) {
                                TupleAssignmentRhs::StorageReference { access }
                            } else if self.types.memory_layout(rhs_ty).is_some() {
                                TupleAssignmentRhs::StorageCopy {
                                    access,
                                    source_ty: rhs_ty,
                                    span: rhs.span,
                                }
                            } else {
                                TupleAssignmentRhs::Materialized {
                                    value,
                                    source_ty: Some(rhs_ty),
                                    span: rhs.span,
                                }
                            }
                        } else {
                            TupleAssignmentRhs::Materialized {
                                value,
                                source_ty: Some(rhs_ty),
                                span: rhs.span,
                            }
                        };
                        values.push((lhs, value));
                    }
                } else {
                    if let Some(lhs) = lhs {
                        let source_ty = self.cx.gcx.type_of_expr(rhs.id);
                        let value = if self.is_storage_reference_binding(lhs)
                            && source_ty.is_some_and(|ty| ty.is_ref_at(DataLocation::Storage))
                        {
                            let Some(access) = self.storage_access(rhs) else {
                                return self.cx.report_unsupported(rhs.span, "storage access");
                            };
                            TupleAssignmentRhs::StorageReference { access }
                        } else if source_ty.is_some_and(|ty| {
                            ty.is_ref_at(DataLocation::Storage)
                                && self.types.memory_layout(ty).is_some()
                        }) {
                            let Some(access) = self.storage_access(rhs) else {
                                return self.cx.report_unsupported(rhs.span, "storage access");
                            };
                            TupleAssignmentRhs::StorageCopy {
                                access,
                                source_ty: source_ty?,
                                span: rhs.span,
                            }
                        } else {
                            TupleAssignmentRhs::Materialized {
                                value: self.lower_expr(rhs)?,
                                source_ty,
                                span: rhs.span,
                            }
                        };
                        values.push((lhs, value));
                    } else {
                        self.lower_expr(rhs)?;
                    }
                }
                continue;
            };
            let Some(lhs) = lhs else {
                self.lower_tuple_assignment_values(&[], nested_rhs, rhs.span, values)?;
                continue;
            };
            let ExprKind::Tuple(nested_lhs) = &lhs.peel_parens().kind else {
                return self.cx.report_unsupported(lhs.span, "tuple assignment target");
            };
            self.lower_tuple_assignment_values(nested_lhs, nested_rhs, rhs.span, values)?;
        }
        Some(())
    }

    pub(super) fn lower_storage_reference_call(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<Vec<(ValueId, Ty<'gcx>, Option<StorageAccess>)>> {
        let ExprKind::Call(callee, ..) = &expr.kind else { return None };
        let return_types = if let Some(function_id) = self.cx.gcx.resolved_function(callee) {
            self.cx
                .gcx
                .hir
                .function(function_id)
                .returns
                .iter()
                .map(|&id| self.cx.gcx.type_of_item(id.into()))
                .collect::<Vec<_>>()
        } else {
            let TyKind::Fn(function) = self.cx.gcx.type_of_expr(callee.id)?.kind else {
                return None;
            };
            function.returns.to_vec()
        };
        if return_types.is_empty() {
            return None;
        }
        if !return_types.iter().any(|ty| ty.is_ref_at(DataLocation::Storage)) {
            return None;
        }
        let values = self.lower_values(expr)?;
        (values.len() == return_types.len()).then(|| {
            values
                .into_iter()
                .zip(return_types)
                .map(|(value, ty)| {
                    let access = ty.is_ref_at(DataLocation::Storage).then(|| StorageAccess {
                        slot: value,
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    });
                    (value, ty, access)
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
        let offset = self.builder.imm(u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32));
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
        let MirType::MemoryObject(kind) = types::TypeLowerer::mir_return_type(ty) else {
            return self.load_multi_return_value(base, index, returns);
        };
        let index = self.builder.imm(u64::try_from(index).unwrap_or(u64::MAX));
        self.builder.memory_object_load_object(
            base,
            MemoryObjectLayout::word_fixed_array(u64::try_from(returns).unwrap_or(u64::MAX)),
            index,
            kind,
        )
    }

    pub(super) fn internal_return_words(ty: Ty<'gcx>) -> usize {
        if matches!(types::TypeLowerer::mir_return_type(ty), MirType::Slice(_)) { 2 } else { 1 }
    }

    pub(super) fn internal_returns_words(returns: impl IntoIterator<Item = Ty<'gcx>>) -> usize {
        returns.into_iter().map(Self::internal_return_words).sum()
    }

    pub(super) fn load_internal_return_values(
        &mut self,
        first: ValueId,
        return_types: &[Ty<'gcx>],
    ) -> Vec<ValueId> {
        let returns = Self::internal_returns_words(return_types.iter().copied());
        let base = self.multi_return_buffer_base();
        let mut index = Self::internal_return_words(return_types[0]);
        let mut values = Vec::with_capacity(return_types.len());
        let dirty = self.dirty_values.contains(&first);
        values.push(first);
        for &ty in &return_types[1..] {
            let return_ty = types::TypeLowerer::mir_return_type(ty);
            let value = match return_ty {
                MirType::Slice(location) => {
                    let pointer = self.load_multi_return_value(base, index, returns);
                    let length = self.load_multi_return_value(base, index + 1, returns);
                    self.builder.make_slice(pointer, length, location)
                }
                _ => self.load_multi_return_value_as(base, index, returns, ty),
            };
            if dirty || return_ty == MirType::Slice(SliceLocation::Calldata) {
                self.dirty_values.insert(value);
            }
            values.push(value);
            index += Self::internal_return_words(ty);
        }
        values
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
