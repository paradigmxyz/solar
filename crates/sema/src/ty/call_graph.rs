use super::{Gcx, TyFnKind, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{BumpExt, Never, bit_set::DenseBitSet};
use std::{collections::VecDeque, ops::ControlFlow};

pub(super) struct CallGraph {
    functions: DenseBitSet<hir::FunctionId>,
    virtual_functions: DenseBitSet<hir::FunctionId>,
    events: DenseBitSet<hir::EventId>,
    errors: DenseBitSet<hir::ErrorId>,
}

#[derive(Clone, Copy)]
pub(super) struct InterfaceItems<'gcx> {
    pub(super) events: &'gcx [hir::EventId],
    pub(super) errors: &'gcx [hir::ErrorId],
}

impl CallGraph {
    pub(super) fn new(gcx: Gcx<'_>, id: hir::FunctionId) -> Self {
        let mut collector = ReferenceCollector::new(gcx);
        let _ = collector.visit_nested_function(id);
        collector.callgraph
    }

    fn new_empty(function_count: usize, event_count: usize, error_count: usize) -> Self {
        Self {
            functions: DenseBitSet::new_empty(function_count),
            virtual_functions: DenseBitSet::new_empty(function_count),
            events: DenseBitSet::new_empty(event_count),
            errors: DenseBitSet::new_empty(error_count),
        }
    }

    fn union(&mut self, other: &Self) {
        self.functions.union(&other.functions);
        self.virtual_functions.union(&other.virtual_functions);
        self.events.union(&other.events);
        self.errors.union(&other.errors);
    }
}

pub(super) fn interface_items<'gcx>(gcx: Gcx<'gcx>, id: hir::ContractId) -> InterfaceItems<'gcx> {
    let function_count = gcx.hir.function_ids().count();
    let event_count = gcx.hir.event_ids().count();
    let error_count = gcx.hir.error_ids().count();
    let mut references = CallGraph::new_empty(function_count, event_count, error_count);
    let contract = gcx.hir.contract(id);

    for item in gcx.hir.contract_item_ids(id) {
        match item {
            hir::ItemId::Event(id) => {
                references.events.insert(id);
            }
            hir::ItemId::Error(id) => {
                references.errors.insert(id);
            }
            _ => {}
        }
    }

    for function in gcx.interface_functions(id) {
        references.functions.insert(function.id);
    }
    if let Some(fallback) = contract.fallback {
        references.functions.insert(fallback);
    }
    if let Some(receive) = contract.receive {
        references.functions.insert(receive);
    }

    for &base in contract.linearized_bases {
        let base = gcx.hir.contract(base);
        if let Some(constructor) = base.ctor {
            references.functions.insert(constructor);
        }

        let mut collector = ReferenceCollector::new(gcx);
        for variable in base.variables() {
            let _ = collector.visit_nested_var(variable);
        }
        for modifier in base.bases_args {
            let _ = collector.visit_modifier(modifier);
        }
        references.union(&collector.callgraph);
    }

    let mut worklist = VecDeque::from_iter(references.functions.iter());
    while let Some(function) = worklist.pop_front() {
        let callgraph = CallGraph::new(gcx, function);
        for callee in &callgraph.functions {
            if references.functions.insert(callee) {
                worklist.push_back(callee);
            }
        }
        for callee in &callgraph.virtual_functions {
            let callee = resolve_virtual_call(gcx, id, callee);
            if references.functions.insert(callee) {
                worklist.push_back(callee);
            }
        }
        references.events.union(&callgraph.events);
        references.errors.union(&callgraph.errors);
    }

    InterfaceItems {
        events: gcx.bump().alloc_from_iter(references.events.iter()),
        errors: gcx.bump().alloc_from_iter(references.errors.iter()),
    }
}

fn resolve_virtual_call(
    gcx: Gcx<'_>,
    contract: hir::ContractId,
    target: hir::FunctionId,
) -> hir::FunctionId {
    let target_function = gcx.hir.function(target);
    let Some(target_contract) = target_function.contract else { return target };
    let target_signature = gcx.item_signature(target.into());

    for &base in gcx.hir.contract(contract).linearized_bases {
        let base_contract = gcx.hir.contract(base);
        if !base_contract.linearized_bases.contains(&target_contract) {
            continue;
        }
        for candidate in base_contract.functions() {
            let candidate_function = gcx.hir.function(candidate);
            if candidate_function.kind == target_function.kind
                && candidate_function.name == target_function.name
                && gcx.item_signature(candidate.into()) == target_signature
            {
                return candidate;
            }
        }
    }
    target
}

struct ReferenceCollector<'gcx> {
    gcx: Gcx<'gcx>,
    callgraph: CallGraph,
}

impl<'gcx> ReferenceCollector<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            callgraph: CallGraph::new_empty(
                gcx.hir.function_ids().count(),
                gcx.hir.event_ids().count(),
                gcx.hir.error_ids().count(),
            ),
        }
    }

    fn collect_call(&mut self, call: &'gcx hir::Expr<'gcx>) {
        let hir::ExprKind::Call(callee, ..) = call.kind else { return };
        let Some(res) = self.gcx.resolved_expr(callee) else { return };
        match res {
            hir::Res::Item(hir::ItemId::Function(id))
                if self.gcx.type_of_expr(callee.id).is_some_and(
                    |ty| matches!(ty.kind, TyKind::Fn(function) if function.kind == TyFnKind::Internal),
                ) =>
            {
                let is_super_call = if let hir::ExprKind::Member(receiver, _) = callee.kind {
                    self.gcx.type_of_expr(receiver.id).is_some_and(
                        |ty| matches!(ty.kind, TyKind::Type(inner) if matches!(inner.kind, TyKind::Super(_))),
                    )
                } else {
                    false
                };
                if self.gcx.hir.function(id).virtual_ && !is_super_call {
                    self.callgraph.virtual_functions.insert(id);
                } else {
                    self.callgraph.functions.insert(id);
                }
            }
            hir::Res::Item(hir::ItemId::Error(id)) => {
                self.callgraph.errors.insert(id);
            }
            _ => {}
        }
    }
}

impl<'gcx> Visit<'gcx> for ReferenceCollector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.collect_call(expr);
        if let Some(function) = self.gcx.user_operator(expr.id) {
            self.callgraph.functions.insert(function);
        }
        self.walk_expr(expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Emit(call) = stmt.kind
            && let hir::ExprKind::Call(callee, ..) = call.kind
            && let Some(hir::Res::Item(hir::ItemId::Event(id))) = self.gcx.resolved_expr(callee)
        {
            self.callgraph.events.insert(id);
        }
        self.walk_stmt(stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(id) = modifier.id {
            if self.gcx.hir.function(id).virtual_ {
                self.callgraph.virtual_functions.insert(id);
            } else {
                self.callgraph.functions.insert(id);
            }
        }
        self.walk_modifier(modifier)
    }
}
