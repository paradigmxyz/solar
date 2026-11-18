//! The functions, events, errors and bytecode dependencies a contract reaches from its creation
//! and its interface.
//!
//! What a function body or a constant initializer reaches does not depend on the contract, except
//! for the overrides its calls dispatch to. [`record`] walks each body and initializer once and
//! keeps what the walk found, in walk order, as [`CallOp`]s, which [`Gcx`] caches. Each contract
//! then replays the operations of the bodies it reaches and resolves their dispatch, instead of
//! walking every body again.

use super::{CallDispatch, Gcx, TyKind};
use crate::hir::{self, Visit};
use solar_data_structures::{bit_set::DenseBitSet, map::FxIndexMap};
use std::{collections::VecDeque, ops::ControlFlow};

pub(super) struct ReferencedItems {
    pub(super) functions: DenseBitSet<hir::FunctionId>,
    pub(super) events: DenseBitSet<hir::EventId>,
    pub(super) errors: DenseBitSet<hir::ErrorId>,
    pub(super) bytecode_dependencies: DenseBitSet<hir::ContractId>,
    pub(super) internal_dispatch_targets: DenseBitSet<hir::FunctionId>,
    /// The functions that a call names in parentheses, which solc counts as dispatch targets but
    /// code generation does not need to. Only recorded when the ASTs are kept.
    pub(super) parenthesized_targets: Vec<hir::FunctionId>,
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
            parenthesized_targets: Vec::new(),
        }
    }
}

struct CallGraphBuilder<'gcx, 's> {
    gcx: Gcx<'gcx>,
    contract: hir::ContractId,
    graph: ReferencedItems,
    /// The functions to visit, with whether to visit only the targets their calls dispatch to.
    worklist: VecDeque<(hir::FunctionId, bool)>,
    visited_constants: DenseBitSet<hir::VariableId>,
    /// The constants whose initializers a stopped body walked, which reaches only what they
    /// dispatch to.
    stopped_constants: DenseBitSet<hir::VariableId>,
    /// Whether the traversal enters the body of a function, or the state variable initializers of
    /// a contract, only for the targets their calls dispatch to.
    stop: &'s dyn Fn(hir::ItemId) -> bool,
    /// The function whose body is being visited, if any.
    current: Option<hir::FunctionId>,
    /// Whether `current` is a function `stop` accepts.
    stopped: bool,
    /// How each function was first reached.
    parents: FxIndexMap<hir::FunctionId, Traced>,
}

/// How a traversal reached a function.
#[derive(Clone, Copy)]
pub(crate) struct Traced {
    /// The function that first reached it, or `None` for a root.
    pub(crate) parent: Option<hir::FunctionId>,
    /// Whether only the targets its calls dispatch to were visited, because only a body `stop`
    /// accepts, or a function reached only from one, calls it.
    pub(crate) dispatch_only: bool,
}

impl<'gcx, 's> CallGraphBuilder<'gcx, 's> {
    fn new(
        gcx: Gcx<'gcx>,
        contract: hir::ContractId,
        stop: &'s dyn Fn(hir::ItemId) -> bool,
    ) -> Self {
        Self {
            gcx,
            contract,
            graph: ReferencedItems::new(gcx),
            worklist: VecDeque::new(),
            visited_constants: DenseBitSet::new_empty(gcx.hir.variable_ids().count()),
            stopped_constants: DenseBitSet::new_empty(gcx.hir.variable_ids().count()),
            stop,
            current: None,
            stopped: false,
            parents: FxIndexMap::default(),
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
        for &base_id in gcx.hir.contract(contract).linearized_bases.iter().rev() {
            let base = gcx.hir.contract(base_id);
            this.stopped = (this.stop)(hir::ItemId::Contract(base_id));
            for id in base.variables() {
                let variable = gcx.hir.variable(id);
                if variable.is_state_variable()
                    && !variable.is_constant()
                    && variable.initializer.is_some()
                {
                    this.replay(gcx.initializer_call_ops(id));
                }
            }
            if let Some(constructor) = base.ctor {
                this.enqueue(constructor);
            }
            for inheritance in base.bases_args {
                this.replay(&record(gcx, |recorder| recorder.visit_modifier(inheritance)));
            }
            this.stopped = false;
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
            self.add_internal_dispatch_target(function);
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
            this.replay(&record(gcx, |recorder| recorder.visit_modifier(modifier)));
        }
        for &base_id in contract.linearized_bases {
            let base = gcx.hir.contract(base_id);
            this.stopped = (this.stop)(hir::ItemId::Contract(base_id));
            for id in base.variables() {
                let variable = gcx.hir.variable(id);
                if variable.is_state_variable()
                    && !variable.is_constant()
                    && variable.initializer.is_some()
                {
                    this.replay(gcx.initializer_call_ops(id));
                }
            }
            this.stopped = false;
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
        while let Some((function, dispatch_only)) = self.worklist.pop_front() {
            self.stopped = dispatch_only || (self.stop)(hir::ItemId::Function(function));
            self.current = Some(function);
            self.replay(self.gcx.function_call_ops(function));
            self.current = None;
            self.stopped = false;
        }
    }

    fn enqueue(&mut self, function: hir::FunctionId) {
        if self.graph.functions.insert(function) {
            let traced = Traced { parent: self.current, dispatch_only: false };
            // A function first reached through a trusted body takes the path of the body that
            // reaches it in full.
            let first = self.parents.entry(function).or_insert(traced);
            if first.dispatch_only {
                *first = traced;
            }
            self.worklist.push_back((function, false));
        }
    }

    /// Visits `function`, which a body `stop` accepts calls directly, only for the targets its
    /// own calls dispatch to: the contract chose those, not the review that trusted the caller.
    fn enqueue_dispatch_only(&mut self, function: hir::FunctionId) {
        if !self.graph.functions.contains(function) && !self.parents.contains_key(&function) {
            self.parents.insert(function, Traced { parent: self.current, dispatch_only: true });
            self.worklist.push_back((function, true));
        }
    }

    /// Reaches `resolved`, the target a call or reference to `named` dispatches to. A body `stop`
    /// accepts reaches the targets the contract chooses, an override or a `super` target other
    /// than the function it names, and visits the function it names only for what its own calls
    /// dispatch to.
    fn enqueue_dispatched(&mut self, named: hir::FunctionId, resolved: hir::FunctionId) {
        if self.stopped && resolved == named {
            self.enqueue_dispatch_only(resolved);
        } else {
            self.enqueue(resolved);
        }
    }

    fn add_internal_dispatch_target(&mut self, function: hir::FunctionId) {
        // A function value can escape reviewed code and be invoked by an untrusted caller.
        // Direct calls stay behind the trusted boundary; escaped targets do not.
        self.enqueue(function);
        self.graph.internal_dispatch_targets.insert(function);
    }

    /// Replays what a walk found, in the order it found it, in the context of the contract.
    fn replay(&mut self, ops: &[CallOp]) {
        let gcx = self.gcx;
        for &op in ops {
            match op {
                CallOp::Call { named, dispatch, parenthesized } => {
                    let function = gcx.dispatch_call(self.contract, named, dispatch);
                    self.enqueue_dispatched(named, function);
                    if parenthesized {
                        self.graph.parenthesized_targets.push(function);
                    }
                }
                CallOp::Reference { named, dispatch } => {
                    let function = gcx.dispatch_call(self.contract, named, dispatch);
                    self.add_internal_dispatch_target(function);
                }
                CallOp::Constant(id) => {
                    // A stopped body reaches only what a constant's initializer dispatches to, so
                    // its walk leaves the constant to a full walk by another reader.
                    let first = if self.stopped {
                        !self.visited_constants.contains(id) && self.stopped_constants.insert(id)
                    } else {
                        self.visited_constants.insert(id)
                    };
                    if first {
                        self.replay(gcx.initializer_call_ops(id));
                    }
                }
                CallOp::Event(event) => {
                    self.graph.events.insert(event);
                }
                CallOp::Error(error) => {
                    self.graph.errors.insert(error);
                }
                CallOp::BytecodeDependency(contract) => {
                    self.graph.bytecode_dependencies.insert(contract);
                }
            }
        }
    }
}

/// What a walk of a function or an initializer reaches, before a contract resolves the overrides
/// its calls dispatch to.
#[derive(Clone, Copy, Debug)]
pub(super) enum CallOp {
    /// A call, a modifier invocation or a user-defined operator that runs `named` as `dispatch`
    /// resolves it. `parenthesized` records that the call names its callee in parentheses, which
    /// solc counts as a dispatch target, when the ASTs are kept.
    Call { named: hir::FunctionId, dispatch: CallDispatch, parenthesized: bool },
    /// A function value that is not called directly, which becomes an internal dispatch target.
    Reference { named: hir::FunctionId, dispatch: CallDispatch },
    /// A reference to a constant with an initializer, which reaches what the initializer reaches.
    Constant(hir::VariableId),
    /// An emitted event.
    Event(hir::EventId),
    /// A call of an error.
    Error(hir::ErrorId),
    /// A contract whose bytecode the code needs.
    BytecodeDependency(hir::ContractId),
}

/// Returns what the walk `f` reaches, in walk order.
fn record<'gcx>(
    gcx: Gcx<'gcx>,
    f: impl FnOnce(&mut Recorder<'gcx>) -> ControlFlow<!>,
) -> Vec<CallOp> {
    let mut recorder = Recorder { gcx, ops: Vec::new(), direct_callee: None };
    let _ = f(&mut recorder);
    recorder.ops
}

/// Returns what the body of `function` reaches.
pub(super) fn record_function(gcx: Gcx<'_>, function: hir::FunctionId) -> Vec<CallOp> {
    record(gcx, |recorder| recorder.visit_nested_function(function))
}

/// Returns what the initializer of the variable `id` reaches.
pub(super) fn record_initializer(gcx: Gcx<'_>, id: hir::VariableId) -> Vec<CallOp> {
    let Some(initializer) = gcx.hir.variable(id).initializer else { return Vec::new() };
    record(gcx, |recorder| recorder.visit_expr(initializer))
}

/// Walks a function or an initializer and records what it reaches as [`CallOp`]s.
struct Recorder<'gcx> {
    gcx: Gcx<'gcx>,
    ops: Vec<CallOp>,
    direct_callee: Option<hir::ExprId>,
}

impl<'gcx> Recorder<'gcx> {
    /// Records what a call of `callee` reaches, and returns the function it calls directly.
    fn collect_call(
        &mut self,
        expr: &'gcx hir::Expr<'gcx>,
        callee: &'gcx hir::Expr<'gcx>,
    ) -> Option<hir::FunctionId> {
        if let Some(named) = self.gcx.internal_call_target(None, callee) {
            // Lowering drops the parentheses of a callee, which leaves it starting after its call.
            let parenthesized =
                self.gcx.sess.opts.unstable.keep_asts && callee.span.lo() > expr.span.lo();
            let dispatch = self.gcx.call_dispatch(callee);
            self.ops.push(CallOp::Call { named, dispatch, parenthesized });
            return Some(named);
        }
        if let Some(TyKind::Error(_, error)) = self.gcx.type_of_expr(callee.id).map(|ty| ty.kind) {
            self.ops.push(CallOp::Error(error));
        }
        None
    }

    fn collect_function_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        if self.direct_callee == Some(expr.id) {
            return;
        }
        if let Some(named) = self.gcx.internal_call_target(None, expr) {
            let dispatch = self.gcx.call_dispatch(expr);
            self.ops.push(CallOp::Reference { named, dispatch });
        }
    }

    fn collect_constant_reference(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let Some(id) = self.gcx.resolved_variable(expr) else { return };
        let variable = self.gcx.hir.variable(id);
        if variable.is_constant() && variable.initializer.is_some() {
            self.ops.push(CallOp::Constant(id));
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
            self.ops.push(CallOp::BytecodeDependency(*id));
        }
    }
}

impl<'gcx> Visit<'gcx> for Recorder<'gcx> {
    type BreakValue = !;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.collect_bytecode_dependency(expr);
        self.collect_constant_reference(expr);
        self.collect_function_reference(expr);
        if let Some(named) = self.gcx.user_operator(expr.id) {
            self.ops.push(CallOp::Call {
                named,
                dispatch: CallDispatch::Static,
                parenthesized: false,
            });
        }

        if let Some((callee, args, options)) = expr.as_call() {
            let direct = self.collect_call(expr, callee).is_some();
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
            self.ops.push(CallOp::Event(event));
        }
        self.walk_stmt(stmt)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let hir::ItemId::Function(named) = modifier.id {
            let dispatch =
                if modifier.qualified { CallDispatch::Static } else { CallDispatch::Virtual };
            self.ops.push(CallOp::Call { named, dispatch, parenthesized: false });
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

/// The functions the contract `id` runs, found as [`interface_items`] finds them, from its
/// creation and its interface, or from every function of its bases when `all`, with the function
/// that first reached each, or `None` for a root. A function `stop` accepts is reached, and its
/// body reaches the overrides and `super` targets its calls dispatch to in this contract. The
/// functions it calls directly are visited only for what their own calls dispatch to, and are
/// marked [`Traced::dispatch_only`].
pub(crate) fn traced_functions(
    gcx: Gcx<'_>,
    id: hir::ContractId,
    all: bool,
    stop: &dyn Fn(hir::ItemId) -> bool,
) -> FxIndexMap<hir::FunctionId, Traced> {
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
    for (function, traced) in deployed.parents {
        let first = parents.entry(function).or_insert(traced);
        if first.dispatch_only && !traced.dispatch_only {
            *first = traced;
        }
    }
    parents
}
