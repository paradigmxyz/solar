use super::{Gcx, TyFnKind, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{Never, bit_set::DenseBitSet, index::IndexVec};
use std::{collections::VecDeque, ops::ControlFlow};

pub(super) struct CallGraph {
    nodes: IndexVec<hir::FunctionId, References>,
}

#[derive(Clone)]
struct References {
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

impl<'gcx> Gcx<'gcx> {
    pub(super) fn callgraph(self) -> &'gcx CallGraph {
        assert!(self.has_typeck_results(), "call graph requires type checking");
        self.callgraph.get_or_init(|| CallGraph::new(self))
    }
}

impl CallGraph {
    fn new(gcx: Gcx<'_>) -> Self {
        let mut nodes = IndexVec::with_capacity(gcx.hir.function_ids().count());
        for id in gcx.hir.function_ids() {
            let mut collector = ReferenceCollector::new(gcx);
            let _ = collector.visit_nested_function(id);
            nodes.push(collector.references);
        }
        Self { nodes }
    }

    pub(super) fn interface_items<'gcx>(
        &self,
        gcx: Gcx<'gcx>,
        id: hir::ContractId,
    ) -> InterfaceItems<'gcx> {
        let function_count = gcx.hir.function_ids().count();
        let event_count = gcx.hir.event_ids().count();
        let error_count = gcx.hir.error_ids().count();
        let mut references = References::new(function_count, event_count, error_count);
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
            references.union(&collector.references);
        }

        let mut worklist = VecDeque::from_iter(references.functions.iter());
        while let Some(function) = worklist.pop_front() {
            let node = &self.nodes[function];
            for callee in &node.functions {
                if references.functions.insert(callee) {
                    worklist.push_back(callee);
                }
            }
            for callee in &node.virtual_functions {
                let callee = self.resolve_virtual_call(gcx, id, callee);
                if references.functions.insert(callee) {
                    worklist.push_back(callee);
                }
            }
            references.events.union(&node.events);
            references.errors.union(&node.errors);
        }

        let events = references.events.iter().collect::<Vec<_>>();
        let errors = references.errors.iter().collect::<Vec<_>>();
        InterfaceItems {
            events: gcx.bump().alloc_slice_copy(&events),
            errors: gcx.bump().alloc_slice_copy(&errors),
        }
    }

    fn resolve_virtual_call(
        &self,
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
}

impl References {
    fn new(function_count: usize, event_count: usize, error_count: usize) -> Self {
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

struct ReferenceCollector<'gcx> {
    gcx: Gcx<'gcx>,
    references: References,
}

impl<'gcx> ReferenceCollector<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            references: References::new(
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
                    self.references.virtual_functions.insert(id);
                } else {
                    self.references.functions.insert(id);
                }
            }
            hir::Res::Item(hir::ItemId::Error(id)) => {
                self.references.errors.insert(id);
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
            self.references.functions.insert(function);
        }
        self.walk_expr(expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Emit(call) = stmt.kind
            && let hir::ExprKind::Call(callee, ..) = call.kind
            && let Some(hir::Res::Item(hir::ItemId::Event(id))) = self.gcx.resolved_expr(callee)
        {
            self.references.events.insert(id);
        }
        self.walk_stmt(stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(id) = modifier.id {
            if self.gcx.hir.function(id).virtual_ {
                self.references.virtual_functions.insert(id);
            } else {
                self.references.functions.insert(id);
            }
        }
        self.walk_modifier(modifier)
    }
}
