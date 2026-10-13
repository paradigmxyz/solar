//! Tracks scalar bindings whose raw EVM bits can cross an assembly boundary.
//!
//! Copies and internal calls preserve raw bits, while Solidity operations and
//! conversions consume typed values. Propagate assembly exposure through the
//! copy graph before choosing signatures and loop phi types. These carriers
//! are explicitly i256; ordinary integer values retain their native width.
//!
//! Internal function pointers are scalars too. Calls through them dispatch
//! over the functions of the pointer's type, but assembly can retype a
//! pointer: `outFn := inFn` makes a function returning a `MemoryPointer` word
//! callable as one returning a `memory` struct. Taint the bindings assembly
//! reads or writes, closed over the copy graph, over copies that only change a
//! pointer's state mutability, over the edges between a reference and the
//! pointers or references loaded from or stored into its object, and from a
//! called pointer to the call's results. A function whose pointer flows into a
//! tainted binding is exposed to assembly, and a pointer call whose callee may
//! read a tainted binding dispatches over the exposed functions too,
//! reinterpreting one-word parameters and returns. Calls whose pointers never
//! touch assembly keep the per-type dispatch.
//!
//! A retyped call can also retype what it passes and returns: Seaport's
//! helpers map typed arrays through a generic function over `MemoryPointer`
//! words and hand it a typed callback. So the functions such a call passes are
//! exposed, and the parameters and returns of every exposed function are
//! tainted. These rules feed each other and iterate to a fixpoint.
//!
//! NOTE: Pointers reach this graph only through bindings. Assembly that writes
//! a pointer into memory or storage through an address it computed, rather
//! than through a reference binding, is not tracked.

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
    /// Edges that carry internal function pointers but not raw scalar bits.
    pointer_edges: FxHashMap<VariableId, Vec<VariableId>>,
    /// Each internal function with a binding its pointer flows into.
    pointer_values: Vec<(hir::FunctionId, VariableId)>,
    pointer_calls: Vec<PointerCall>,
    returns: &'gcx [VariableId],
    assembly: bool,
}

/// An internal function pointer call with the pointers it reads.
struct PointerCall {
    call: hir::ExprId,
    /// Bindings the callee may read.
    callee: SmallVec<[VariableId; 4]>,
    /// Bindings the arguments may copy or load pointers from.
    arguments: SmallVec<[VariableId; 4]>,
    /// Functions whose pointers the arguments take.
    functions: SmallVec<[hir::FunctionId; 1]>,
}

impl<'gcx> Exposure<'gcx> {
    fn callees(&mut self, callee: &hir::Expr<'_>) -> SmallVec<[hir::FunctionId; 1]> {
        if let Some(id) = self.gcx.resolved_function(callee) {
            return smallvec::smallvec![self.gcx.resolve_call_target(self.contract, callee, id)];
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
            ExprKind::Call(callee, _) => {
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
        self.connect_pointers(destinations, expr);
    }

    /// Returns the internal function whose pointer an expression takes.
    fn function_reference(&self, expr: &hir::Expr<'_>) -> Option<hir::FunctionId> {
        if !matches!(expr.kind, ExprKind::Ident(_) | ExprKind::Member(..)) {
            return None;
        }
        let TyKind::Fn(function) = self.gcx.type_of_expr(expr.id)?.kind else { return None };
        if !function.is_internal() {
            return None;
        }
        let id = self.gcx.resolved_function(expr)?;
        Some(self.gcx.resolve_call_target(self.contract, expr, id))
    }

    /// Collects the bindings whose pointers an expression's value may copy or load, and the
    /// functions whose pointers it takes.
    fn pointer_sources(
        &mut self,
        expr: &hir::Expr<'_>,
        bindings: &mut SmallVec<[VariableId; 4]>,
        functions: &mut SmallVec<[hir::FunctionId; 1]>,
    ) {
        if let Some(function) = self.function_reference(expr) {
            functions.push(function);
            return;
        }
        if let Some(id) = self.gcx.user_operator(expr.id) {
            bindings.extend_from_slice(self.gcx.hir.function(id).returns);
            return;
        }
        match expr.kind {
            ExprKind::Ident(_) => bindings.extend(self.gcx.resolved_variable(expr)),
            ExprKind::Tuple(items) => {
                for item in items.iter().flatten() {
                    self.pointer_sources(item, bindings, functions);
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.pointer_sources(item, bindings, functions);
                }
            }
            ExprKind::Ternary(_, yes, no) => {
                self.pointer_sources(yes, bindings, functions);
                self.pointer_sources(no, bindings, functions);
            }
            // A value loaded from an object comes from the reference that reaches it.
            ExprKind::Member(base, _) | ExprKind::Index(base, _) | ExprKind::Slice(base, ..) => {
                self.pointer_sources(base, bindings, functions);
            }
            ExprKind::Call(callee, args) => {
                if self.gcx.resolved_expr(callee).is_some_and(
                    |res| matches!(res, hir::Res::Item(item) if item.as_struct().is_some()),
                ) {
                    for argument in args.exprs() {
                        self.pointer_sources(argument, bindings, functions);
                    }
                } else if let Some(builtin) = self.gcx.resolved_builtin(callee) {
                    // `array.push()` returns a reference into the array.
                    if builtin == Builtin::ArrayPush0
                        && let ExprKind::Member(base, _) = callee.kind
                    {
                        self.pointer_sources(base, bindings, functions);
                    }
                } else {
                    for id in self.callees(callee) {
                        bindings.extend_from_slice(self.gcx.hir.function(id).returns);
                    }
                    // A retyped callee can return values of other types.
                    if is_pointer_callee(self.gcx, callee) {
                        self.pointer_sources(callee, bindings, &mut SmallVec::new());
                    }
                }
            }
            _ => {}
        }
    }

    /// Records the internal function pointers an expression may carry into bindings.
    fn connect_pointers(&mut self, destinations: &[VariableId], expr: &hir::Expr<'_>) {
        let destinations = destinations
            .iter()
            .copied()
            .filter(|&id| can_hold_function_pointer(self.gcx.type_of_item(id.into())))
            .collect::<SmallVec<[_; 4]>>();
        if destinations.is_empty() {
            return;
        }
        let mut bindings = SmallVec::new();
        let mut functions = SmallVec::new();
        self.pointer_sources(expr, &mut bindings, &mut functions);
        bindings.retain(|&mut id| can_hold_function_pointer(self.gcx.type_of_item(id.into())));
        for &to in &destinations {
            self.pointer_values.extend(functions.iter().map(|&function| (function, to)));
            for &from in &bindings {
                if from != to {
                    self.pointer_edges.entry(from).or_default().push(to);
                    self.pointer_edges.entry(to).or_default().push(from);
                }
            }
        }
    }

    /// Records the references whose objects an assignment target stores into.
    fn store_roots(&mut self, target: &hir::Expr<'_>, roots: &mut SmallVec<[VariableId; 4]>) {
        match target.kind {
            ExprKind::Tuple(items) => {
                for item in items.iter().flatten() {
                    self.store_roots(item, roots);
                }
            }
            ExprKind::Member(base, _) | ExprKind::Index(base, _) => {
                self.pointer_sources(base, roots, &mut SmallVec::new());
            }
            ExprKind::Call(..) => self.pointer_sources(target, roots, &mut SmallVec::new()),
            _ => {}
        }
    }

    /// Records an internal function pointer call with the pointers it reads.
    fn record_pointer_call(
        &mut self,
        call: &hir::Expr<'_>,
        callee: &hir::Expr<'_>,
        args: hir::CallArgs<'_>,
    ) {
        if !is_pointer_callee(self.gcx, callee) {
            return;
        }
        let mut bindings = SmallVec::new();
        self.pointer_sources(callee, &mut bindings, &mut SmallVec::new());
        if bindings.is_empty() {
            return;
        }
        let mut arguments = SmallVec::new();
        let mut functions = SmallVec::new();
        for argument in args.exprs() {
            self.pointer_sources(argument, &mut arguments, &mut functions);
        }
        self.pointer_calls.push(PointerCall {
            call: call.id,
            callee: bindings,
            arguments,
            functions,
        });
    }
}

/// Returns whether a call through this callee goes through an internal function pointer.
fn is_pointer_callee(gcx: Gcx<'_>, callee: &hir::Expr<'_>) -> bool {
    gcx.resolved_builtin(callee).is_none()
        && gcx.type_of_expr(callee.id).is_some_and(|ty| {
            matches!(ty.kind, TyKind::Fn(function)
                if function.function_id.is_none() && function.is_internal())
        })
}

/// Returns whether a value of this type can hold an internal function pointer, directly or in
/// the object it references.
fn can_hold_function_pointer(ty: Ty<'_>) -> bool {
    match ty.kind {
        TyKind::Fn(function) => function.is_internal(),
        TyKind::Ref(..) | TyKind::Mapping(..) => true,
        _ => false,
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
                    let mut roots = SmallVec::new();
                    self.store_roots(lhs, &mut roots);
                    self.connect_pointers(&roots, rhs);
                }
                ExprKind::Call(callee, args) => {
                    self.record_pointer_call(expr, callee, args);
                    if self.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush)
                        && let ExprKind::Member(base, _) = callee.kind
                        && let Some(value) = args.exprs().next()
                    {
                        let mut roots = SmallVec::new();
                        self.pointer_sources(base, &mut roots, &mut SmallVec::new());
                        self.connect_pointers(&roots, value);
                    }
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
            pointer_edges: FxHashMap::default(),
            pointer_values: Vec::new(),
            pointer_calls: Vec::new(),
            returns: &[],
            assembly: false,
        };
        for &(id, _) in functions {
            let _ = exposure.visit_nested_function(id);
        }
        for &base in gcx.hir.contract(contract).linearized_bases {
            let base = gcx.hir.contract(base);
            if let Some(id) = base.ctor {
                let _ = exposure.visit_nested_function(id);
            }
            for id in base.variables() {
                if gcx.hir.variable(id).is_state_variable()
                    && let Some(initializer) = gcx.hir.variable(id).initializer
                {
                    exposure.connect_pointers(&[id], initializer);
                }
            }
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

        let mut tainted = exposure.raw.clone();
        let mut pending = tainted.iter().copied().collect::<Vec<_>>();
        let mut exposed = FxHashSet::default();
        let mut calls = FxHashSet::default();
        loop {
            // tainted = closure(tainted, copy edges + pointer edges)
            while let Some(id) = pending.pop() {
                let neighbors = exposure.edges.get(&id).into_iter().flatten();
                for &neighbor in
                    neighbors.chain(exposure.pointer_edges.get(&id).into_iter().flatten())
                {
                    if tainted.insert(neighbor) {
                        pending.push(neighbor);
                    }
                }
            }
            // A function whose pointer reaches a tainted binding is exposed to assembly.
            let mut newly_exposed = exposure
                .pointer_values
                .iter()
                .filter(|&&(function, binding)| {
                    tainted.contains(&binding) && exposed.insert(function)
                })
                .map(|&(function, _)| function)
                .collect::<Vec<_>>();
            // A tainted call can pass its arguments to an exposed function of another type.
            for call in &exposure.pointer_calls {
                if !calls.contains(&call.call)
                    && call.callee.iter().any(|binding| tainted.contains(binding))
                {
                    calls.insert(call.call);
                    pending.extend(call.arguments.iter().filter(|&&id| tainted.insert(id)));
                    newly_exposed.extend(
                        call.functions.iter().filter(|&&function| exposed.insert(function)),
                    );
                }
            }
            // An exposed function can run behind a retyped pointer, so its parameters and
            // returns can hold values of other types.
            for function in newly_exposed {
                let function = gcx.hir.function(function);
                pending.extend(
                    function
                        .parameters
                        .iter()
                        .chain(function.returns)
                        .filter(|&&id| tainted.insert(id)),
                );
            }
            if pending.is_empty() {
                break;
            }
        }
        self.pointer_registry.assembly_exposed = exposed;
        self.pointer_registry.assembly_calls = calls;

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
        let ty = types::TypeLowerer::mir_type(gcx.type_of_item(id.into()));
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
