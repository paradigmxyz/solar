use super::{Gcx, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{Never, bit_set::DenseBitSet};
use std::{collections::VecDeque, ops::ControlFlow};

pub(super) struct ReferencedItems {
    pub(super) functions: DenseBitSet<hir::FunctionId>,
    pub(super) events: DenseBitSet<hir::EventId>,
    pub(super) errors: DenseBitSet<hir::ErrorId>,
    pub(super) bytecode_dependencies: DenseBitSet<hir::ContractId>,
    internal_dispatch_targets: DenseBitSet<hir::FunctionId>,
}

pub(super) struct InterfaceItems {
    pub(super) creation: ReferencedItems,
    pub(super) deployed: ReferencedItems,
}

impl ReferencedItems {
    fn new(gcx: Gcx<'_>) -> Self {
        Self {
            functions: DenseBitSet::new_empty(gcx.hir.function_ids().count()),
            events: DenseBitSet::new_empty(gcx.hir.event_ids().count()),
            errors: DenseBitSet::new_empty(gcx.hir.error_ids().count()),
            bytecode_dependencies: DenseBitSet::new_empty(gcx.hir.contract_ids().count()),
            internal_dispatch_targets: DenseBitSet::new_empty(gcx.hir.function_ids().count()),
        }
    }
}

struct CallGraphBuilder<'gcx> {
    gcx: Gcx<'gcx>,
    contract: hir::ContractId,
    graph: ReferencedItems,
    worklist: VecDeque<hir::FunctionId>,
    visited_constants: DenseBitSet<hir::VariableId>,
    direct_callee: Option<hir::ExprId>,
}

impl<'gcx> CallGraphBuilder<'gcx> {
    fn new(gcx: Gcx<'gcx>, contract: hir::ContractId) -> Self {
        Self {
            gcx,
            contract,
            graph: ReferencedItems::new(gcx),
            worklist: VecDeque::new(),
            visited_constants: DenseBitSet::new_empty(gcx.hir.variable_ids().count()),
            direct_callee: None,
        }
    }

    fn build_creation(gcx: Gcx<'gcx>, contract: hir::ContractId) -> ReferencedItems {
        let mut this = Self::new(gcx, contract);
        for &base in gcx.hir.contract(contract).linearized_bases.iter().rev() {
            let base = gcx.hir.contract(base);
            for variable in base.variables() {
                let variable = gcx.hir.variable(variable);
                if variable.is_state_variable()
                    && !variable.is_constant()
                    && let Some(initializer) = variable.initializer
                {
                    let _ = this.visit_expr(initializer);
                }
            }
            if let Some(constructor) = base.ctor {
                this.enqueue(constructor);
            }
            for inheritance in base.bases_args {
                let _ = this.visit_modifier(inheritance);
            }
        }
        this.finish()
    }

    fn build_deployed(
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        creation: &ReferencedItems,
    ) -> ReferencedItems {
        let mut this = Self::new(gcx, contract);
        for function in gcx.interface_functions(contract) {
            this.enqueue(function.id);
        }
        let contract = gcx.hir.contract(contract);
        if let Some(fallback) = contract.fallback {
            this.enqueue(fallback);
        }
        if let Some(receive) = contract.receive {
            this.enqueue(receive);
        }
        for function in &creation.internal_dispatch_targets {
            this.add_internal_dispatch_target(function);
        }
        this.finish()
    }

    fn build_all(gcx: Gcx<'gcx>, contract: hir::ContractId) -> ReferencedItems {
        let mut this = Self::new(gcx, contract);
        let contract = gcx.hir.contract(contract);
        for modifier in contract.linearized_bases_args.iter().flatten() {
            let _ = this.visit_modifier(modifier);
        }
        for &base in contract.linearized_bases {
            let base = gcx.hir.contract(base);
            for variable in base.variables() {
                let variable = gcx.hir.variable(variable);
                if variable.is_state_variable()
                    && !variable.is_constant()
                    && let Some(initializer) = variable.initializer
                {
                    let _ = this.visit_expr(initializer);
                }
            }
            for function in base.all_functions() {
                this.enqueue(function);
            }
        }
        this.finish()
    }

    fn finish(mut self) -> ReferencedItems {
        while let Some(function) = self.worklist.pop_front() {
            let _ = self.visit_nested_function(function);
        }
        self.graph
    }

    fn enqueue(&mut self, function: hir::FunctionId) {
        if self.graph.functions.insert(function) {
            self.worklist.push_back(function);
        }
    }

    fn add_internal_dispatch_target(&mut self, function: hir::FunctionId) {
        self.graph.internal_dispatch_targets.insert(function);
        self.enqueue(function);
    }

    fn collect_call(&mut self, callee: &'gcx hir::Expr<'gcx>) -> bool {
        let Some(ty) = self.gcx.type_of_expr(callee.id) else { return false };
        match ty.kind {
            TyKind::Fn(function) if function.is_internal() => {
                if let Some(function) =
                    function.function_id.or_else(|| self.gcx.resolved_function(callee))
                {
                    let function = self.resolve_call_target(callee, function);
                    self.enqueue(function);
                    true
                } else {
                    false
                }
            }
            TyKind::Error(_, error) => {
                self.graph.errors.insert(error);
                false
            }
            _ => false,
        }
    }

    fn collect_function_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        if self.direct_callee == Some(expr.id) {
            return;
        }
        let Some(TyKind::Fn(function)) = self.gcx.type_of_expr(expr.id).map(|ty| ty.kind) else {
            return;
        };
        if !function.is_internal() {
            return;
        }
        let Some(function) = function.function_id.or_else(|| self.gcx.resolved_function(expr))
        else {
            return;
        };
        let function = self.resolve_call_target(expr, function);
        self.add_internal_dispatch_target(function);
    }

    fn collect_constant_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let Some(id) = self.gcx.resolved_variable(expr) else { return };
        let variable = self.gcx.hir.variable(id);
        if variable.is_constant()
            && self.visited_constants.insert(id)
            && let Some(initializer) = variable.initializer
        {
            let _ = self.visit_expr(initializer);
        }
    }

    fn collect_bytecode_dependency(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let ty = match &expr.kind {
            hir::ExprKind::New(ty) => Some(ty),
            hir::ExprKind::Member(base, member)
                if matches!(
                    member.name,
                    solar_interface::sym::creationCode | solar_interface::sym::runtimeCode
                ) =>
            {
                if let hir::ExprKind::TypeCall(ty) = &base.kind {
                    Some(ty)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(hir::Type { kind: hir::TypeKind::Custom(hir::ItemId::Contract(id)), .. }) = ty {
            self.graph.bytecode_dependencies.insert(*id);
        }
    }

    fn resolve_call_target(
        &self,
        callee: &hir::Expr<'_>,
        function: hir::FunctionId,
    ) -> hir::FunctionId {
        if let hir::ExprKind::Member(base, _) = callee.kind
            && let Some(TyKind::Type(ty)) = self.gcx.type_of_expr(base.id).map(|ty| ty.kind)
        {
            return match ty.kind {
                TyKind::Contract(_) => function,
                TyKind::Super(defining_contract) => {
                    self.gcx.resolve_super_function(self.contract, defining_contract, function)
                }
                _ => self.gcx.resolve_virtual_function(self.contract, function),
            };
        }
        self.gcx.resolve_virtual_function(self.contract, function)
    }
}

impl<'gcx> Visit<'gcx> for CallGraphBuilder<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.collect_bytecode_dependency(expr);
        self.collect_constant_reference(expr);
        self.collect_function_reference(expr);
        if let Some(function) = self.gcx.user_operator(expr.id) {
            self.enqueue(function);
        }

        if let hir::ExprKind::Call(callee, ref args, options) = expr.kind {
            let direct = self.collect_call(callee);
            let previous = self.direct_callee;
            if direct {
                self.direct_callee = Some(callee.id);
            }
            self.visit_expr(callee)?;
            self.direct_callee = previous;
            if let Some(options) = options {
                for option in options.args {
                    self.visit_expr(&option.value)?;
                }
            }
            return self.visit_call_args(args);
        }

        self.walk_expr(expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Emit(call) = stmt.kind
            && let hir::ExprKind::Call(callee, ..) = call.kind
            && let Some(TyKind::Event(_, event)) =
                self.gcx.type_of_expr(callee.id).map(|ty| ty.kind)
        {
            self.graph.events.insert(event);
        }
        self.walk_stmt(stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(function) = modifier.id {
            let function = self.gcx.resolve_virtual_function(self.contract, function);
            self.enqueue(function);
        }
        self.walk_modifier(modifier)
    }
}

pub(super) fn interface_items(gcx: Gcx<'_>, id: hir::ContractId) -> InterfaceItems {
    let creation = CallGraphBuilder::build_creation(gcx, id);
    let deployed = CallGraphBuilder::build_deployed(gcx, id, &creation);

    InterfaceItems { creation, deployed }
}

pub(super) fn all_bytecode_dependencies<'gcx>(
    gcx: Gcx<'gcx>,
    id: hir::ContractId,
) -> &'gcx DenseBitSet<hir::ContractId> {
    let graph = CallGraphBuilder::build_all(gcx, id);
    gcx.alloc(graph.bytecode_dependencies)
}
