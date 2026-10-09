use super::{Gcx, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{Never, bit_set::DenseBitSet, map::FxIndexMap};
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

struct CallGraphBuilder<'gcx, 's> {
    gcx: Gcx<'gcx>,
    contract: hir::ContractId,
    graph: ReferencedItems,
    worklist: VecDeque<hir::FunctionId>,
    visited_constants: DenseBitSet<hir::VariableId>,
    direct_callee: Option<hir::ExprId>,
    /// Whether the traversal enters a function's body only for the targets its calls dispatch to.
    stop: &'s dyn Fn(hir::FunctionId) -> bool,
    /// The function whose body is being visited, if any.
    current: Option<hir::FunctionId>,
    /// Whether `current` is a function `stop` accepts.
    stopped: bool,
    /// The function that first reached each function, or `None` for a root.
    parents: FxIndexMap<hir::FunctionId, Option<hir::FunctionId>>,
    /// The first call through an internal function pointer, as the function it is in, or `None`
    /// for code outside any function.
    pointer_call: Option<Option<hir::FunctionId>>,
}

impl<'gcx, 's> CallGraphBuilder<'gcx, 's> {
    fn new(
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        stop: &'s dyn Fn(hir::FunctionId) -> bool,
    ) -> Self {
        Self {
            gcx,
            contract,
            graph: ReferencedItems::new(gcx),
            worklist: VecDeque::new(),
            visited_constants: DenseBitSet::new_empty(gcx.hir.variable_ids().count()),
            direct_callee: None,
            stop,
            current: None,
            stopped: false,
            parents: FxIndexMap::default(),
            pointer_call: None,
        }
    }

    fn build_creation(gcx: Gcx<'gcx>, contract: hir::ContractId) -> ReferencedItems {
        Self::new(gcx, contract, &|_| false).creation()
    }

    fn creation(mut self) -> ReferencedItems {
        self.enter_creation();
        self.finish()
    }

    fn enter_creation(&mut self) {
        let gcx = self.gcx;
        let contract = self.contract;
        let this = self;
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
    }

    fn build_deployed(
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        creation: &ReferencedItems,
    ) -> ReferencedItems {
        let mut this = Self::new(gcx, contract, &|_| false);
        this.enter_deployed(creation);
        this.finish()
    }

    fn enter_deployed(&mut self, creation: &ReferencedItems) {
        let gcx = self.gcx;
        for function in gcx.interface_functions(self.contract) {
            self.enqueue(function.id);
        }
        let contract = gcx.hir.contract(self.contract);
        if let Some(fallback) = contract.fallback {
            self.enqueue(fallback);
        }
        if let Some(receive) = contract.receive {
            self.enqueue(receive);
        }
        for function in &creation.internal_dispatch_targets {
            self.add_internal_dispatch_target(function, function);
        }
    }

    fn build_all(gcx: Gcx<'gcx>, contract: hir::ContractId) -> ReferencedItems {
        let mut this = Self::new(gcx, contract, &|_| false);
        this.enter_all();
        this.finish()
    }

    fn enter_all(&mut self) {
        let gcx = self.gcx;
        let this = self;
        let contract = gcx.hir.contract(this.contract);
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
    }

    fn finish(mut self) -> ReferencedItems {
        self.drain();
        self.graph
    }

    fn drain(&mut self) {
        while let Some(function) = self.worklist.pop_front() {
            self.stopped = (self.stop)(function);
            // A free or library function dispatches nowhere the contract chooses.
            if self.stopped
                && self
                    .gcx
                    .hir
                    .function(function)
                    .contract
                    .is_none_or(|contract| self.gcx.hir.contract(contract).kind.is_library())
            {
                continue;
            }
            self.current = Some(function);
            let _ = self.visit_nested_function(function);
            self.current = None;
            self.stopped = false;
        }
    }

    fn enqueue(&mut self, function: hir::FunctionId) {
        if self.graph.functions.insert(function) {
            self.parents.entry(function).or_insert(self.current);
            self.worklist.push_back(function);
        }
    }

    /// Reaches `resolved`, the target a call or reference to `named` dispatches to. A body `stop`
    /// accepts reaches only the targets the contract chooses, an override or a `super` target
    /// other than the function it names.
    fn enqueue_dispatched(&mut self, named: hir::FunctionId, resolved: hir::FunctionId) -> bool {
        let reached = !self.stopped || resolved != named;
        if reached {
            self.enqueue(resolved);
        }
        reached
    }

    fn add_internal_dispatch_target(&mut self, named: hir::FunctionId, function: hir::FunctionId) {
        if self.enqueue_dispatched(named, function) {
            self.graph.internal_dispatch_targets.insert(function);
        }
    }

    fn collect_call(&mut self, callee: &'gcx hir::Expr<'gcx>) -> bool {
        let Some(ty) = self.gcx.type_of_expr(callee.id) else { return false };
        match ty.kind {
            TyKind::Fn(function) => {
                let Some(function_id) =
                    function.function_id.or_else(|| self.gcx.resolved_function(callee))
                else {
                    if function.is_internal() && self.pointer_call.is_none() {
                        self.pointer_call = Some(self.current);
                    }
                    return false;
                };
                if !function.is_internal() {
                    return false;
                }
                let function = self.resolve_call_target(callee, function_id);
                self.enqueue_dispatched(function_id, function);
                true
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
        let resolved = self.resolve_call_target(expr, function);
        self.add_internal_dispatch_target(function, resolved);
    }

    fn collect_constant_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        // A stopped body does not reach what a constant's initializer names; leave it unvisited so
        // other readers can reach it.
        if self.stopped {
            return;
        }
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

impl<'gcx> Visit<'gcx> for CallGraphBuilder<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.collect_bytecode_dependency(expr);
        self.collect_constant_reference(expr);
        self.collect_function_reference(expr);
        if let Some(function) = self.gcx.user_operator(expr.id) {
            self.enqueue_dispatched(function, function);
        }

        if let Some((callee, args, options)) = expr.as_call() {
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
        if let Some(function) = self.gcx.resolve_modifier_target(self.contract, modifier)
            && let hir::ItemId::Function(named) = modifier.id
        {
            self.enqueue_dispatched(named, function);
        }
        self.walk_modifier(modifier)
    }
}

pub(super) fn interface_items(gcx: Gcx<'_>, id: hir::ContractId) -> InterfaceItems {
    let creation = CallGraphBuilder::build_creation(gcx, id);
    let deployed = CallGraphBuilder::build_deployed(gcx, id, &creation);

    InterfaceItems { creation, deployed }
}

pub(super) fn all_items(gcx: Gcx<'_>, id: hir::ContractId) -> ReferencedItems {
    CallGraphBuilder::build_all(gcx, id)
}

/// Where [`traced_from`] starts.
#[derive(Clone, Copy)]
pub(crate) enum TraceRoot {
    /// The contract's creation: its state variable initializers, constructors and base constructor
    /// arguments.
    Creation,
    /// A call to one function.
    Function(hir::FunctionId),
}

/// The functions a call to `root` in the contract `id` runs, with the function that first reached
/// each, or `None` for a root. A call through an internal function pointer can reach every
/// function whose value the contract takes anywhere, so it reaches them all.
pub(crate) fn traced_from(
    gcx: Gcx<'_>,
    id: hir::ContractId,
    root: TraceRoot,
) -> FxIndexMap<hir::FunctionId, Option<hir::FunctionId>> {
    let mut builder = CallGraphBuilder::new(gcx, id, &|_| false);
    match root {
        TraceRoot::Creation => builder.enter_creation(),
        TraceRoot::Function(function) => builder.enqueue(function),
    }
    builder.drain();
    if let Some(caller) = builder.pointer_call {
        let items = gcx.interface_items(id);
        builder.current = caller;
        for target in items
            .creation
            .internal_dispatch_targets
            .iter()
            .chain(items.deployed.internal_dispatch_targets.iter())
        {
            builder.enqueue(target);
        }
        builder.current = None;
        builder.drain();
    }
    builder.parents
}

/// The functions the contract `id` runs, found as [`interface_items`] finds them, from its
/// creation and its interface, or from every function of its bases when `all`, with the function
/// that first reached each, or `None` for a root. A function `stop` accepts is reached, and its
/// body reaches only the overrides and `super` targets its calls dispatch to in this contract,
/// so what else only it reaches is not.
pub(crate) fn traced_functions(
    gcx: Gcx<'_>,
    id: hir::ContractId,
    all: bool,
    stop: &dyn Fn(hir::FunctionId) -> bool,
) -> FxIndexMap<hir::FunctionId, Option<hir::FunctionId>> {
    let mut builder = CallGraphBuilder::new(gcx, id, stop);
    if all {
        builder.enter_all();
        builder.drain();
        return builder.parents;
    }
    builder.enter_creation();
    builder.drain();
    let mut deployed = CallGraphBuilder::new(gcx, id, stop);
    deployed.enter_deployed(&builder.graph);
    deployed.drain();
    let mut parents = builder.parents;
    for (function, parent) in deployed.parents {
        parents.entry(function).or_insert(parent);
    }
    parents
}
