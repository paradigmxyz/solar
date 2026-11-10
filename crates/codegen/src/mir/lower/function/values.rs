//! Tuple, return, and multi-value lowering.

use super::*;

enum PreparedTupleAssignment<'gcx> {
    Value { place: LValuePlace<'gcx>, target_ty: Ty<'gcx>, rhs: TupleAssignmentRhs<'gcx> },
    StorageReference { id: VariableId, access: StorageAccess },
}

enum TupleAssignmentRhs<'gcx> {
    /// A value of `source_ty`. A storage aggregate is its slot, which the assignment copies to
    /// memory when it is committed.
    Materialized {
        value: Word,
        source_ty: Option<Ty<'gcx>>,
        span: Span,
    },
    StorageReference {
        access: StorageAccess,
    },
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_values(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Option<Vec<Word>> {
        let expr = expr.peel_parens();
        if self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_unit()) {
            self.lower_discarded_expr(expr)?;
            return Some(Vec::new());
        }
        if let ExprKind::Ternary(condition, then_expr, else_expr) = &expr.kind {
            let ty = self.cx.gcx.type_of_expr(expr.id)?;
            return self.lower_ternary_values(condition, then_expr, else_expr, ty);
        }
        if let Some((callee, args, call_opts)) = expr.as_call() {
            if let Some(builtin) = self.low_level_call_builtin(expr) {
                let values = self.lower_low_level_call_values(expr, builtin, 2, false)?;
                return Some(values.into_iter().map(Word::from_clean).collect());
            }
            let resolved_builtin = self.cx.gcx.resolved_builtin(callee);
            if resolved_builtin == Some(Builtin::AbiDecode)
                && let Some(types) = args.exprs().nth(1)
                && let ExprKind::Tuple(elements) = types.kind
                && elements.len() > 1
            {
                let value = self.operand(expr)?.id();
                let values = self.unpack_return_value(value);
                return Some(values.into_iter().map(Word::from_clean).collect());
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
                let values = self.lower_external_function_pointer_call_values(
                    callee, function, *args, call_opts,
                )?;
                return Some(values.into_iter().map(Word::from_clean).collect());
            }
            if let Some(returns) = returns
                && returns > 1
            {
                // The fields are raw words where the callee's return variable is a raw carrier.
                let value = self.operand(expr)?.id();
                let fields = self.unpack_return_value(value);
                return Some(match (resolved_function, function_pointer) {
                    // An external call decodes its results at their native widths.
                    (Some(_), _)
                        if !function_ty.is_some_and(|function| {
                            function.is_internal() && !function.is_delegate_call()
                        }) =>
                    {
                        fields.into_iter().map(Word::from_clean).collect()
                    }
                    (Some(function_id), _) => {
                        let function_id = self.resolve_call_target(callee, function_id);
                        let returns = self.cx.gcx.hir.function(function_id).returns;
                        fields
                            .into_iter()
                            .zip(returns)
                            .map(|(field, &id)| self.binding_word(field, id))
                            .collect()
                    }
                    (None, Some(function)) => fields
                        .into_iter()
                        .zip(function.returns)
                        .map(|(field, &ty)| self.pointer_word(field, ty))
                        .collect(),
                    (None, None) => fields.into_iter().map(Word::from_clean).collect(),
                });
            }
        }
        match &expr.kind {
            ExprKind::Tuple(values) => {
                values.iter().flatten().map(|expr| self.lower_component(expr)).collect()
            }
            _ => Some(vec![self.lower_component(expr)?]),
        }
    }

    pub(super) fn lower_return_values(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Option<Vec<Word>> {
        if self.cx.gcx.type_of_expr(expr.id)?.is_unit() {
            self.lower_discarded_expr(expr)?;
            return Some(Vec::new());
        }
        if self.returns.len() == 1 {
            let ty = self.cx.gcx.type_of_item(self.returns[0].into());
            if ty.is_ref_at(DataLocation::Storage) {
                return Some(vec![Word::from_clean(self.require_storage_access(expr)?.slot)]);
            }
            let carried = self.lower_typed_expr(expr, ty)?;
            let carried = if ty.is_ref_at(DataLocation::Memory) {
                let value = carried.clean(self, ty);
                Word::from_clean(self.materialize_memory_argument(ty, value, expr.span)?)
            } else {
                carried
            };
            return Some(vec![carried]);
        }
        if self.returns.len() > 1
            && let ExprKind::Tuple(values) = &expr.peel_parens().kind
            && values.len() == self.returns.len()
        {
            let returns = self.returns.clone();
            let return_ty =
                |this: &Self, index: usize| this.cx.gcx.type_of_item(returns[index].into());
            let waits = self.operands_wait(values.iter().flatten().copied());
            return self.lower_converted_operands(
                &waits,
                values.len(),
                |this, index| {
                    let value = values[index]?;
                    this.lower_operand(value, return_ty(this, index))
                },
                |this, index, operand| {
                    let ty = return_ty(this, index);
                    let span = operand.span;
                    let carried = this.convert_operand(operand)?;
                    if ty.is_ref_at(DataLocation::Memory) {
                        let value = carried.clean(this, ty);
                        this.materialize_memory_argument(ty, value, span).map(Word::from_clean)
                    } else {
                        Some(carried)
                    }
                },
            );
        }
        if self.returns.len() > 1 {
            let returns = self.returns.clone();
            let source_types = self.multi_value_types(expr);
            let values = self.lower_values(expr)?;
            let Some(source_types) = source_types
                .filter(|types| types.len() == values.len() && values.len() == returns.len())
            else {
                // Without one source type per lowered value per declared return there is no way
                // to tell which value needs which conversion, and a storage reference among them
                // would travel on as a raw slot word.
                return self.cx.report_unsupported(expr.span, "return value arity");
            };
            return values
                .into_iter()
                .zip(source_types)
                .zip(returns)
                .map(|((value, source_ty), id)| {
                    let target_ty = self.cx.gcx.type_of_item(id.into());
                    self.convert_tuple_component(value, source_ty, target_ty, expr.span)
                })
                .collect();
        }

        self.lower_values(expr)
    }

    /// The component types of an expression that produces several values, such as a call to a
    /// function with several returns.
    fn multi_value_types(&self, expr: &'gcx hir::Expr<'gcx>) -> Option<Vec<Ty<'gcx>>> {
        let TyKind::Tuple(types) = self.cx.gcx.type_of_expr(expr.peel_parens().id)?.kind else {
            return None;
        };
        Some(types.to_vec())
    }

    /// Converts an already-lowered tuple component to its target type.
    pub(super) fn convert_tuple_component(
        &mut self,
        carried: Word,
        source_ty: Ty<'gcx>,
        target_ty: Ty<'gcx>,
        span: Span,
    ) -> Option<Word> {
        // A storage reference is its slot in both the source and the declared return, so the
        // value already has the right shape.
        if target_ty.is_ref_at(DataLocation::Storage) {
            return Some(carried);
        }
        if source_ty.is_ref_at(DataLocation::Storage) {
            // A target without a memory layout has nowhere to hold the copy, so there is no
            // conversion to emit and the slot word must not be handed on as a memory pointer.
            if self.types.memory_layout(target_ty).is_none() {
                return self.cx.report_unsupported(span, "storage reference return");
            }
            return self.convert_value(carried, source_ty, target_ty, span);
        }
        let carried = if target_ty.is_ref_at(DataLocation::Memory) {
            let value = carried.clean(self, source_ty);
            Word::from_clean(self.materialize_memory_argument(target_ty, value, span)?)
        } else {
            carried
        };
        // value = coerce(value, source_ty, target_ty)
        Some(if source_ty == target_ty {
            carried
        } else {
            let value = carried.clean(self, source_ty);
            Word::from_clean(self.coerce_value(value, source_ty, target_ty))
        })
    }

    /// Lowers one component of a multi-value expression in the shape `convert_tuple_component`
    /// expects.
    pub(super) fn lower_component(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Option<Word> {
        match self.cx.gcx.type_of_expr(expr.id) {
            Some(ty) if ty.is_value_type() => self.lower_typed_expr(expr, ty),
            _ => Some(Word::from_clean(self.operand(expr)?.id())),
        }
    }

    pub(super) fn lower_tuple_assignment(
        &mut self,
        elements: &[Option<&'gcx hir::Expr<'gcx>>],
        rhs: &'gcx hir::Expr<'gcx>,
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
        }) && let Some(values) = self.lower_storage_reference_values(rhs)
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
                    let value = TupleAssignmentRhs::Materialized {
                        value: Word::from_clean(access.slot),
                        source_ty: Some(source_ty),
                        span: rhs.span,
                    };
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
                    .map(|(element, value)| (element, Word::from_clean(value), None)),
            );
        }
        if let ExprKind::Tuple(rhs_elements) = &rhs.peel_parens().kind {
            if rhs_elements.iter().filter(|element| element.is_some()).count() == 1
                && rhs_elements.iter().any(Option::is_none)
                && let Some(rhs) = rhs_elements.iter().flatten().next()
            {
                let values = self.lower_values(rhs)?;
                if values.len() >= elements.len() {
                    let source_ty = self.cx.gcx.type_of_expr(rhs.id)?;
                    let sources = match source_ty.kind {
                        TyKind::Tuple(sources) => sources,
                        _ => std::slice::from_ref(&source_ty),
                    };
                    return self.store_tuple_values(
                        elements.iter().zip(values).zip(sources).filter_map(
                            |((element, value), &source)| {
                                element.map(|element| (element, value, Some(source)))
                            },
                        ),
                    );
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
        let source_ty = self.cx.gcx.type_of_expr(rhs.id)?;
        let source_types = match source_ty.kind {
            TyKind::Tuple(types) => types,
            _ => std::slice::from_ref(&source_ty),
        };
        self.store_tuple_values(elements.iter().zip(values).zip(source_types).filter_map(
            |((element, value), &source_ty)| {
                element.map(|element| (element, value, Some(source_ty)))
            },
        ))
    }

    fn prepare_tuple_rhs(
        &mut self,
        element: &'gcx hir::Expr<'gcx>,
        rhs: TupleAssignmentRhs<'gcx>,
    ) -> Option<TupleAssignmentRhs<'gcx>> {
        let target_ty = self.type_of_expr_or_variable(element)?;
        let (value, source_ty, span) = match rhs {
            TupleAssignmentRhs::Materialized { value, source_ty, span } => (value, source_ty, span),
            TupleAssignmentRhs::StorageReference { access } => {
                return Some(TupleAssignmentRhs::StorageReference { access });
            }
        };
        let source_ty = source_ty.unwrap_or(target_ty);
        if self.is_storage_aggregate(source_ty) {
            return Some(TupleAssignmentRhs::Materialized {
                value,
                source_ty: Some(source_ty),
                span,
            });
        }
        let value = if target_ty.is_ref_at(DataLocation::Memory) {
            let value = value.clean(self, source_ty);
            Word::from_clean(self.materialize_memory_argument(target_ty, value, span)?)
        } else {
            value
        };
        // An equal-type copy keeps its raw bits, like any copy edge.
        let value = if source_ty == target_ty {
            value
        } else {
            let value = value.clean(self, source_ty);
            Word::from_clean(self.coerce_value(value, source_ty, target_ty))
        };
        Some(TupleAssignmentRhs::Materialized { value, source_ty: Some(source_ty), span })
    }

    fn store_tuple_values(
        &mut self,
        values: impl IntoIterator<Item = (&'gcx hir::Expr<'gcx>, Word, Option<Ty<'gcx>>)>,
    ) -> Option<()> {
        let values = values
            .into_iter()
            .map(|(element, value, source_ty)| {
                let rhs = TupleAssignmentRhs::Materialized { value, source_ty, span: element.span };
                Some((element, self.prepare_tuple_rhs(element, rhs)?))
            })
            .collect::<Option<Vec<_>>>()?;
        self.store_prepared_tuple_values(values)
    }

    fn store_prepared_tuple_values(
        &mut self,
        values: impl IntoIterator<Item = (&'gcx hir::Expr<'gcx>, TupleAssignmentRhs<'gcx>)>,
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
                        let target_ty = self.type_of_expr_or_variable(element)?;
                        PreparedTupleAssignment::Value { place, target_ty, rhs }
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
                PreparedTupleAssignment::Value { place, target_ty, rhs } => {
                    let TupleAssignmentRhs::Materialized { value, source_ty, span } = rhs else {
                        unreachable!("storage reference reached value assignment")
                    };
                    // A storage aggregate is copied now, after every target ran.
                    let value = match source_ty {
                        Some(source_ty) if self.is_storage_aggregate(source_ty) => {
                            let memory_ty =
                                source_ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
                            self.convert_value(value, source_ty, memory_ty, span)?
                        }
                        _ => value,
                    };
                    self.store_word(&place, value, source_ty, target_ty)?;
                }
                PreparedTupleAssignment::StorageReference { id, access } => {
                    self.storage_refs.insert(id, access);
                }
            }
        }
        Some(())
    }

    fn lower_tuple_assignment_values(
        &mut self,
        elements: &[Option<&'gcx hir::Expr<'gcx>>],
        rhs_elements: &[Option<&'gcx hir::Expr<'gcx>>],
        span: Span,
        values: &mut Vec<(&'gcx hir::Expr<'gcx>, TupleAssignmentRhs<'gcx>)>,
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
                                slot: value.clean(self, rhs_ty),
                                location: StorageLocation::word(U256::ZERO),
                                offset: None,
                            };
                            if self.is_storage_reference_binding(lhs) {
                                TupleAssignmentRhs::StorageReference { access }
                            } else if self.types.memory_layout(rhs_ty).is_some() {
                                TupleAssignmentRhs::Materialized {
                                    value: Word::from_clean(access.slot),
                                    source_ty: Some(rhs_ty),
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
                            let access = self.require_storage_access(rhs)?;
                            TupleAssignmentRhs::StorageReference { access }
                        } else if source_ty.is_some_and(|ty| {
                            ty.is_ref_at(DataLocation::Storage)
                                && self.types.memory_layout(ty).is_some()
                        }) {
                            let access = self.require_storage_access(rhs)?;
                            TupleAssignmentRhs::Materialized {
                                value: Word::from_clean(access.slot),
                                source_ty,
                                span: rhs.span,
                            }
                        } else if let Some(target_ty) = self.type_of_expr_or_variable(lhs)
                            && let Some(value) = self.lower_fixed_bytes_literal(target_ty, rhs)
                        {
                            // A literal for fixed bytes is the target's word already, as in a
                            // single assignment, rather than a string object to convert.
                            TupleAssignmentRhs::Materialized {
                                value: Word::from_clean(value),
                                source_ty: Some(target_ty),
                                span: rhs.span,
                            }
                        } else {
                            TupleAssignmentRhs::Materialized {
                                value: self.lower_component(rhs)?,
                                source_ty,
                                span: rhs.span,
                            }
                        };
                        values.push((lhs, value));
                    } else {
                        // The assignment drops this component, so its value needs no read.
                        self.lower_discarded_expr(rhs)?;
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

    /// Lowers a call or conditional whose values include storage references, pairing each value
    /// with its source type and, for a storage reference, the slot it refers to.
    pub(super) fn lower_storage_reference_values(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
    ) -> Option<Vec<(Word, Ty<'gcx>, Option<StorageAccess>)>> {
        if expr.as_call().is_none() && !matches!(expr.kind, ExprKind::Ternary(..)) {
            return None;
        }
        let ty = self.cx.gcx.type_of_expr(expr.id)?;
        let types = match ty.kind {
            TyKind::Tuple(types) => types,
            _ => std::slice::from_ref(&ty),
        };
        if !types.iter().any(|ty| ty.is_ref_at(DataLocation::Storage)) {
            return None;
        }
        let values = self.lower_values(expr)?;
        (values.len() == types.len()).then(|| {
            values
                .into_iter()
                .zip(types)
                .map(|(value, &ty)| {
                    let access = ty.is_ref_at(DataLocation::Storage).then(|| StorageAccess {
                        slot: value.clean(self, ty),
                        location: StorageLocation::word(U256::ZERO),
                        offset: None,
                    });
                    (value, ty, access)
                })
                .collect()
        })
    }

    pub(super) fn pack_return_values(
        &mut self,
        values: Vec<ValueId>,
        types: &[Ty<'gcx>],
    ) -> ValueId {
        let fields = types.iter().map(|&ty| types::TypeLowerer::mir_type(ty)).collect();
        let ty = self.cx.module.intern_return_type(fields).expect("return values are not empty");
        if let MirType::Struct(id) = ty {
            // result = insert_value(undef, field0), ...
            let fields = self.cx.module.struct_types[id].fields.clone();
            let values = values
                .into_iter()
                .zip(fields)
                .map(|(value, ty)| self.builder.cast(value, ty))
                .collect::<Vec<_>>();
            self.builder.make_struct(id, values)
        } else {
            values[0]
        }
    }

    /// Splits the value that a call returns into its fields. The fields of an internal call are
    /// raw words where the return variable is a raw carrier, so the caller wraps them in `Word`.
    pub(super) fn unpack_return_value(&mut self, value: ValueId) -> Vec<ValueId> {
        let Some(MirType::Struct(id)) = self.builder.func().value_ty(value) else {
            return vec![value];
        };
        let fields = self.cx.module.struct_types[id].fields.clone();
        fields
            .iter()
            .enumerate()
            .map(|(index, &field)| {
                // field = extract_value result, index
                let result = self.builder.extract_value(id, value, index as u32, field);
                self.mark_unchecked_calldata_slice(result, field);
                result
            })
            .collect()
    }

    pub(super) fn load_static_abi_return_value(
        &mut self,
        base: ValueId,
        index: usize,
        words: usize,
    ) -> ValueId {
        debug_assert!(index < words);
        let offset = u64::try_from(index).unwrap_or(u64::MAX).saturating_mul(32);
        let position = self.builder.add_offset(base, offset);
        self.builder.load(position, PtrLocation::Memory)
    }

    pub(super) fn load_static_abi_return_value_as(
        &mut self,
        base: ValueId,
        index: usize,
        returns: usize,
        ty: Ty<'gcx>,
    ) -> ValueId {
        if types::TypeLowerer::mir_type(ty) != MirType::MEMPTR {
            return self.load_static_abi_return_value(base, index, returns);
        }
        self.builder.memory_load_field_as(
            base,
            u64::try_from(index).unwrap_or(u64::MAX),
            MirType::MEMPTR,
        )
    }
}
