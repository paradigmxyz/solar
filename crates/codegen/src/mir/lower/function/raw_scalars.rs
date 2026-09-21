//! Tracks scalar bindings whose raw EVM bits can cross an assembly boundary.
//!
//! Copies and internal calls preserve raw bits, while Solidity operations and
//! conversions consume typed values. Propagate assembly exposure through the
//! copy graph before choosing signatures and loop phi types. These carriers
//! are explicitly i256; ordinary integer values retain their native width.

use super::*;
use smallvec::SmallVec;
use solar_data_structures::Never;
use solar_sema::hir::Visit;
use std::ops::ControlFlow;

struct Exposure<'gcx> {
    gcx: Gcx<'gcx>,
    raw: FxHashSet<VariableId>,
    contract: hir::ContractId,
    functions: Vec<hir::FunctionId>,
    indirect_callees: FxHashMap<Ty<'gcx>, SmallVec<[hir::FunctionId; 1]>>,
    visited: FxHashSet<hir::FunctionId>,
    edges: FxHashMap<VariableId, Vec<VariableId>>,
    returns: &'gcx [VariableId],
    assembly: bool,
}

impl<'gcx> Exposure<'gcx> {
    fn callees(&mut self, callee: &hir::Expr<'_>) -> SmallVec<[hir::FunctionId; 1]> {
        if let Some(id) = self.gcx.resolved_function(callee) {
            return smallvec::smallvec![super::resolve_call_target(
                self.gcx,
                self.contract,
                callee,
                id
            )];
        }
        let Some(ty) = self.gcx.type_of_expr(callee.id) else { return SmallVec::new() };
        let TyKind::Fn(function) = ty.kind else { return SmallVec::new() };
        if !function.is_internal() {
            return SmallVec::new();
        }
        self.indirect_callees
            .entry(ty)
            .or_insert_with(|| {
                let shape = InternalFunctionPointerShape::from_ty(function);
                self.functions
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let TyKind::Fn(function) = self.gcx.type_of_item(id.into()).kind else {
                            return false;
                        };
                        shape.is_assembly_cast_compatible_with(
                            &InternalFunctionPointerShape::from_ty(function),
                        )
                    })
                    .collect()
            })
            .clone()
    }

    fn sources(&mut self, expr: &hir::Expr<'_>, out: &mut SmallVec<[VariableId; 4]>) {
        if let Some(id) = self.gcx.user_operator(expr.id) {
            out.extend_from_slice(self.gcx.hir.function(id).returns);
            return;
        }
        match expr.kind {
            ExprKind::Ident(_) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(id))) =
                    self.gcx.resolved_expr(expr)
                {
                    out.push(id);
                }
            }
            ExprKind::Tuple(items) => {
                for expr in items.iter().flatten() {
                    self.sources(expr, out);
                }
            }
            ExprKind::Ternary(_, yes, no) => {
                self.sources(yes, out);
                self.sources(no, out);
            }
            ExprKind::Call(callee, _, _) => {
                for id in self.callees(callee) {
                    out.extend_from_slice(self.gcx.hir.function(id).returns);
                }
            }
            _ => {}
        }
    }

    fn connect(&mut self, destinations: &[VariableId], expr: &hir::Expr<'_>) {
        let mut sources = SmallVec::new();
        self.sources(expr, &mut sources);
        for &to in destinations {
            let ty = self.gcx.type_of_item(to.into());
            for &from in &sources {
                if ty == self.gcx.type_of_item(from.into()) {
                    self.edges.entry(from).or_default().push(to);
                    self.edges.entry(to).or_default().push(from);
                }
            }
        }
    }
}

impl<'gcx> Visit<'gcx> for Exposure<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_function(&mut self, id: hir::FunctionId) -> ControlFlow<Never> {
        if !self.visited.insert(id) {
            return ControlFlow::Continue(());
        }
        let previous = self.returns;
        self.returns = self.gcx.hir.function(id).returns;
        let result = self.walk_nested_function(id);
        self.returns = previous;
        result
    }

    fn visit_modifier(&mut self, modifier: &'gcx hir::Modifier<'gcx>) -> ControlFlow<Never> {
        if let Some(id) = self.gcx.resolve_modifier_target(self.contract, modifier) {
            let function = self.gcx.hir.function(id);
            for (&parameter, argument) in function.parameters.iter().zip(modifier.args.exprs()) {
                self.connect(&[parameter], argument);
            }
            if self.visited.insert(id) {
                self.walk_function(function)?;
            }
        }
        self.walk_modifier(modifier)
    }

    fn visit_nested_var(&mut self, id: VariableId) -> ControlFlow<Never> {
        if let Some(expr) = self.gcx.hir.variable(id).initializer {
            self.connect(&[id], expr);
        }
        self.walk_nested_var(id)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Never> {
        let previous = self.assembly;
        match stmt.kind {
            StmtKind::AssemblyBlock(_) => self.assembly = true,
            StmtKind::Return(Some(expr)) => self.connect(self.returns, expr),
            StmtKind::DeclMulti(ids, expr) => {
                self.connect(&ids.iter().flatten().copied().collect::<SmallVec<[_; 4]>>(), expr);
            }
            _ => {}
        }
        let result = self.walk_stmt(stmt);
        self.assembly = previous;
        result
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Never> {
        if self.assembly {
            if matches!(expr.kind, ExprKind::Ident(_))
                && let Some(hir::Res::Item(hir::ItemId::Variable(id))) =
                    self.gcx.resolved_expr(expr)
            {
                self.raw.insert(id);
            }
        } else if let Some(id) = self.gcx.user_operator(expr.id) {
            let parameters = self.gcx.hir.function(id).parameters;
            match expr.kind {
                ExprKind::Unary(_, value) => self.connect(&parameters[..1], value),
                ExprKind::Binary(lhs, _, rhs) => {
                    self.connect(&parameters[..1], lhs);
                    self.connect(&parameters[1..], rhs);
                }
                _ => {}
            }
        } else {
            match expr.kind {
                ExprKind::Assign(lhs, None, rhs) => {
                    let mut ids = SmallVec::new();
                    self.sources(lhs, &mut ids);
                    self.connect(&ids, rhs);
                }
                ExprKind::Call(callee, args, _) => {
                    let callees = self.callees(callee);
                    if callees.is_empty() {
                        return self.walk_expr(expr);
                    }
                    let names = matches!(args.kind, hir::CallArgsKind::Named(_))
                        .then(|| self.gcx.call_param_source(callee))
                        .flatten()
                        .map(|source| self.gcx.callable_param_names(source));
                    let attached =
                        self.gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached);
                    for id in callees {
                        let function = self.gcx.hir.function(id);
                        for (index, &parameter) in function.parameters.iter().enumerate() {
                            if attached && index == 0 {
                                if let ExprKind::Member(receiver, _) = callee.kind {
                                    self.connect(&[parameter], receiver);
                                }
                            } else if let Some(argument) = args.argument_for_parameter(
                                index - usize::from(attached),
                                names.as_deref(),
                            ) {
                                self.connect(&[parameter], argument);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.walk_expr(expr)
    }
}

impl LoweringState {
    pub(in crate::mir::lower) fn analyze_raw_scalars<'gcx>(
        &mut self,
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        functions: &[(hir::FunctionId, bool)],
    ) {
        let mut exposure = Exposure {
            gcx,
            contract,
            functions: functions.iter().map(|&(id, _)| id).collect(),
            indirect_callees: FxHashMap::default(),
            visited: FxHashSet::default(),
            raw: FxHashSet::default(),
            edges: FxHashMap::default(),
            returns: &[],
            assembly: false,
        };
        for &(id, _) in functions {
            let _ = exposure.visit_nested_function(id);
        }
        let mut pending = exposure.raw.iter().copied().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if let Some(neighbors) = exposure.edges.get(&id) {
                for &neighbor in neighbors {
                    if exposure.raw.insert(neighbor) {
                        pending.push(neighbor);
                    }
                }
            }
        }
        self.raw_pointer_types = exposure
            .raw
            .iter()
            .map(|&id| types::TypeLowerer::mir_type(gcx.type_of_item(id.into())))
            .filter(|ty| ty.integer_bits().is_some())
            .collect();
        self.raw_scalars = exposure.raw;
    }

    pub(super) fn pointer_carrier(&self, ty: MirType) -> MirType {
        if self.raw_pointer_types.contains(&ty) { MirType::I256 } else { ty }
    }

    pub(in crate::mir::lower) fn scalar_carrier(&self, gcx: Gcx<'_>, id: VariableId) -> MirType {
        let ty = types::TypeLowerer::mir_signature_type(gcx.type_of_item(id.into()));
        if ty.integer_bits().is_some() && self.raw_scalars.contains(&id) {
            MirType::I256
        } else {
            ty
        }
    }
}

impl FunctionLowerer<'_, '_> {
    pub(super) fn materialize_raw_scalar(&mut self, id: VariableId, value: ValueId) -> ValueId {
        if !self.cx.state.raw_scalars.contains(&id) {
            return value;
        }
        let layout = types::TypeLowerer::value_layout(self.cx.gcx.type_of_item(id.into()));
        if layout.mir_type().integer_bits().is_some() {
            cast_carrier(&mut self.builder, value, layout, MirType::I256)
        } else {
            value
        }
    }

    pub(super) fn materialize_scalar_carrier(
        &mut self,
        id: VariableId,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let ty = self.cx.state.scalar_carrier(self.cx.gcx, id);
        if ty.integer_bits().is_some() {
            Some(cast_carrier(
                &mut self.builder,
                value,
                types::TypeLowerer::value_layout(self.cx.gcx.type_of_item(id.into())),
                ty,
            ))
        } else {
            self.materialize_call_argument(self.cx.gcx.type_of_item(id.into()), value, span)
        }
    }
}

pub(super) fn cast_carrier(
    builder: &mut FunctionBuilder<'_>,
    value: ValueId,
    source: crate::mir::ValueLayout,
    target: MirType,
) -> ValueId {
    if target == MirType::I256
        && matches!(source, crate::mir::ValueLayout::Int(_))
        && let Some(bits) = builder.func().value_ty(value).and_then(MirType::integer_bits)
        && bits < 256
    {
        builder.emit_inst(InstKind::Sext(value, bits, 256), Some(target))
    } else {
        builder.cast(value, target)
    }
}
