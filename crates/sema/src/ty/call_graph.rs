use super::{Gcx, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxIndexSet},
};
use std::{collections::VecDeque, ops::ControlFlow};

/// Contract-level creation and deployed call graphs.
#[derive(Clone, Debug)]
pub struct ContractCallGraph {
    /// Calls and referenced interface items reachable during contract creation.
    pub creation: CallGraph,
    /// Calls and referenced interface items reachable after deployment.
    pub deployed: CallGraph,
}

impl ContractCallGraph {
    pub(crate) fn build(gcx: Gcx<'_>, id: hir::ContractId) -> Self {
        let creation = CallGraphBuilder::build_creation(gcx, id);
        let deployed = CallGraphBuilder::build_deployed(gcx, id, &creation);
        Self { creation, deployed }
    }
}

/// Calls and referenced interface items reachable from a set of roots.
#[derive(Clone, Debug, Default)]
pub struct CallGraph {
    /// Direct internal function call edges.
    pub edges: FxHashMap<hir::FunctionId, FxIndexSet<hir::FunctionId>>,
    /// Internal function references that may be called through dispatch.
    pub internal_dispatch: FxIndexSet<hir::FunctionId>,
    /// Events emitted from reachable functions.
    pub emitted_events: FxIndexSet<hir::EventId>,
    /// Custom errors used from reachable functions.
    pub used_errors: FxIndexSet<hir::ErrorId>,
}

struct CallGraphBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    graph: CallGraph,
    current_function: Option<hir::FunctionId>,
    function_queue: VecDeque<hir::FunctionId>,
    visited_functions: FxIndexSet<hir::FunctionId>,
}

impl<'gcx> CallGraphBuilder<'gcx> {
    fn build_creation(gcx: Gcx<'gcx>, contract: hir::ContractId) -> CallGraph {
        let mut builder = Self::new(gcx);
        for &base_id in gcx.hir.contract(contract).linearized_bases.iter().rev() {
            let base = gcx.hir.contract(base_id);
            for var_id in base.variables() {
                if !gcx.hir.variable(var_id).is_constant() {
                    let ControlFlow::Continue(()) = builder.visit_nested_var(var_id);
                }
            }
            if let Some(ctor) = base.ctor {
                builder.enqueue_function(ctor);
            }
        }
        for modifier in gcx.hir.contract(contract).linearized_bases_args.iter().flatten() {
            let ControlFlow::Continue(()) = builder.visit_modifier(modifier);
        }
        builder.drain_function_queue();
        builder.graph
    }

    fn build_deployed(
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        creation: &CallGraph,
    ) -> CallGraph {
        let mut builder = Self::new(gcx);
        for function in gcx.interface_functions(contract) {
            builder.enqueue_function(function.id);
        }
        if let Some(fallback) = gcx.hir.contract(contract).fallback {
            builder.enqueue_function(fallback);
        }
        if let Some(receive) = gcx.hir.contract(contract).receive {
            builder.enqueue_function(receive);
        }
        for &function in &creation.internal_dispatch {
            builder.enqueue_function(function);
        }
        builder.drain_function_queue();
        builder.graph
    }

    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            graph: CallGraph::default(),
            current_function: None,
            function_queue: VecDeque::new(),
            visited_functions: FxIndexSet::default(),
        }
    }

    fn enqueue_function(&mut self, id: hir::FunctionId) {
        if let Some(caller) = self.current_function {
            self.graph.edges.entry(caller).or_default().insert(id);
        }
        if !self.visited_functions.contains(&id) {
            self.function_queue.push_back(id);
        }
    }

    fn drain_function_queue(&mut self) {
        while let Some(id) = self.function_queue.pop_front() {
            if !self.visited_functions.insert(id) {
                continue;
            }
            let previous = self.current_function.replace(id);
            let ControlFlow::Continue(()) = self.visit_nested_function(id);
            self.current_function = previous;
        }
    }

    fn collect_call_target(&mut self, callee: &'gcx hir::Expr<'gcx>) {
        let Some(resolved) = self.gcx.resolved_callee(callee.id) else { return };
        let hir::Res::Item(item) = resolved.res else { return };
        match item {
            hir::ItemId::Function(id) => {
                if let Some(ty) = self.gcx.type_of_expr(callee.id)
                    && let TyKind::Fn(function) = ty.kind
                    && function.is_internal()
                {
                    self.enqueue_function(id);
                }
            }
            hir::ItemId::Error(id) => _ = self.graph.used_errors.insert(id),
            _ => {}
        }
    }

    fn collect_emit_target(&mut self, call: &'gcx hir::Expr<'gcx>) {
        let hir::ExprKind::Call(callee, ..) = call.kind else { return };
        let Some(resolved) = self.gcx.resolved_callee(callee.id) else { return };
        if let hir::Res::Item(hir::ItemId::Event(id)) = resolved.res {
            self.graph.emitted_events.insert(id);
        }
    }

    fn collect_function_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        if let Some(ty) = self.gcx.type_of_expr(expr.id)
            && let TyKind::Fn(function) = ty.kind
            && function.is_internal()
            && let Some(id) = function.function_id
        {
            self.graph.internal_dispatch.insert(id);
            self.enqueue_function(id);
        }
    }

    fn visit_call_callee(&mut self, callee: &'gcx hir::Expr<'gcx>) -> ControlFlow<Never> {
        match callee.kind {
            hir::ExprKind::Member(receiver, _) | hir::ExprKind::YulMember(receiver, _) => {
                self.visit_expr(receiver)
            }
            hir::ExprKind::Payable(expr)
            | hir::ExprKind::Unary(_, expr)
            | hir::ExprKind::Delete(expr) => self.visit_expr(expr),
            hir::ExprKind::Index(expr, index) => {
                self.visit_expr(expr)?;
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
                ControlFlow::Continue(())
            }
            hir::ExprKind::Slice(expr, start, end) => {
                self.visit_expr(expr)?;
                if let Some(start) = start {
                    self.visit_expr(start)?;
                }
                if let Some(end) = end {
                    self.visit_expr(end)?;
                }
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Continue(()),
        }
    }
}

impl<'gcx> Visit<'gcx> for CallGraphBuilder<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(id) = modifier.id {
            self.enqueue_function(id);
        }
        self.walk_modifier(modifier)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            hir::ExprKind::Binary(..) | hir::ExprKind::Unary(..) => {
                if let Some(function) = self.gcx.user_defined_operator(expr.id) {
                    self.enqueue_function(function);
                }
                self.walk_expr(expr)
            }
            hir::ExprKind::Call(callee, ref args, opts) => {
                self.collect_call_target(callee);
                self.visit_call_callee(callee)?;
                if let Some(opts) = opts {
                    for arg in opts.args {
                        self.visit_expr(&arg.value)?;
                    }
                }
                self.visit_call_args(args)
            }
            hir::ExprKind::Ident(_) | hir::ExprKind::Member(_, _) => {
                self.collect_function_reference(expr);
                self.walk_expr(expr)
            }
            _ => self.walk_expr(expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Emit(call) = stmt.kind {
            self.collect_emit_target(call);
        }
        self.walk_stmt(stmt)
    }
}
