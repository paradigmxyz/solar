//! Control flow analysis.
//!
//! This ports solc's control flow graph (`ControlFlowBuilder`), revert pruning
//! (`ControlFlowRevertPruner`), and analysis (`ControlFlowAnalyzer`), which report:
//! - error 3464: a local storage or calldata pointer that can be read, or a storage or calldata
//!   return variable that can be returned, before any assignment;
//! - warning 6321: an unnamed return variable that can be returned before any assignment;
//! - warning 5740: unreachable code.
//!
//! As in solc, every implemented function gets one flow per contract that inherits it, so
//! modifiers, virtual calls, and `super` calls resolve in that contract, and free functions get one
//! flow without a contract. Flows run in solc's order: free functions, then contracts, each with
//! the functions of its bases in declaration order.
//!
//! Each graph has an entry, an exit, a revert node, and a node for inline assembly `return` and
//! `stop`. Modifier bodies are inlined at their invocation, and `_` jumps to the next modifier or
//! to the function body. Loops come from the lowered HIR: a loop body made of a single
//! `if (cond) ... else break;` whose `break` has the span of the loop or of the condition is the
//! lowered loop condition, and other `for` loops have none. Like solc, every loop splits at its
//! condition, so the code after `for (;;) {}` is reachable. Nodes also keep the hull of the source
//! ranges solc attaches to them: statements, Solidity expressions in pre-order, Yul statements,
//! and the Yul functions between them, which the HIR drops from the statement list.
//!
//! A node that ends in an internal call to a function whose every path reverts, in the contract
//! the call resolves to, exits only to the revert node. Revert states come from solc's least fixed
//! point: a function has a non-reverting path if a search from its entry reaches its exit without
//! passing a call known to revert, and searches blocked on recursion count as reverting.
//!
//! The graph of a function only depends on its modifiers, so flows share a template per function
//! and resolved modifiers, and templates classify themselves as non-reverting if they reach their
//! exit without passing any call, or as reverting if they do not reach it at all. Each template is
//! analyzed once without pruning, and a flow reuses that analysis unless a reachable call in it
//! reverts in its contract.
//!
//! The uninitialized access check tracks only the variables that can be reported: return
//! variables of storage or calldata type or without a name, and local storage or calldata
//! variables declared without an initializer. An access is reported if the variable can be
//! unassigned on some path to it and its node can reach the exit; assignments, including those in
//! tuples and inline assembly, and `return` with a value assign. Unreachable code covers the nodes
//! not reachable from the entry that can reach the exit, the revert node, or the transaction
//! return node, with overlapping ranges merged.
//!
//! As in solc, a custom error `revert` does not read its arguments, `require` and `assert` do not
//! end a path, inline assembly functions are not analyzed, and 6321 is reported once per variable,
//! for the first flow. Solc also reports 3464 once per derived contract; we report each location
//! once.

use crate::{
    builtins::Builtin,
    hir::{self, BinOpKind, ExprKind, ItemId, LoopSource, Res, SourceId, StmtKind, VarKind},
    ty::{CallDispatch, Gcx, TyKind},
};
use rayon::prelude::*;
use solar_ast::DataLocation;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
    newtype_index,
    smallvec::SmallVec,
};
use solar_interface::{BytePos, Span, error_code};

newtype_index! {
    /// A node of a control flow graph.
    struct NodeId;

    /// A variable tracked in a template.
    struct LocalId;

    /// A control flow graph shared by the flows of a function with the same modifiers.
    struct TemplateId;
}

const ENTRY: NodeId = NodeId::new(0);
const EXIT: NodeId = NodeId::new(1);
const REVERT: NodeId = NodeId::new(2);
const TRANSACTION_RETURN: NodeId = NodeId::new(3);

/// The exits of a node that ends in a call to a reverting function.
const REVERT_EXITS: &[NodeId] = &[REVERT];

/// The most-derived contract and the function a flow is built for.
type FlowKey = (Option<hir::ContractId>, hir::FunctionId);

/// A function and the modifier definitions its invocations resolve to.
type TemplateKey = (hir::FunctionId, SmallVec<[Option<hir::FunctionId>; 2]>);

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

    let mut flows = gcx
        .hir
        .functions_enumerated()
        .filter(|(_, function)| function.is_free() && is_analyzed(function))
        .map(|(id, _)| (None, id))
        .collect::<Vec<_>>();
    for contract in gcx.hir.contract_ids() {
        let start = flows.len();
        // Bases usually declare their functions in reverse linearized order, so this is sorted.
        for &base in gcx.hir.contract(contract).linearized_bases.iter().rev() {
            flows.extend(
                gcx.hir
                    .contract(base)
                    .functions()
                    .filter(|&id| is_analyzed(gcx.hir.function(id)))
                    .map(|id| (Some(contract), id)),
            );
        }
        flows[start..].sort_unstable();
    }
    if flows.is_empty() {
        return;
    }

    let cx = BuildContext::new(gcx);
    let mut template_keys = IndexVec::<TemplateId, TemplateKey>::new();
    let function_templates = gcx
        .hir
        .functions_enumerated()
        .map(|(id, function)| {
            (is_analyzed(function) && function.modifiers.is_empty())
                .then(|| template_keys.push((id, SmallVec::new())))
        })
        .collect();
    let mut template_ids = FxHashMap::<TemplateKey, TemplateId>::default();
    let flow_templates = flows
        .iter()
        .filter(|&&(_, function)| !gcx.hir.function(function).modifiers.is_empty())
        .map(|&key| {
            let id = *template_ids
                .entry(template_key(gcx, key))
                .or_insert_with_key(|template_key| template_keys.push(template_key.clone()));
            (key, id)
        })
        .collect();
    let templates = template_keys
        .raw
        .par_iter()
        .map(|(function, modifiers)| Builder::build(&cx, *function, modifiers))
        .collect();
    let mut checker = Checker {
        gcx,
        reported_templates: DenseBitSet::new_empty(template_keys.len()),
        templates: IndexVec::from_vec(templates),
        function_templates,
        flow_templates,
        revert_states: FxHashMap::default(),
        reported: Reported::default(),
    };
    for key in flows {
        checker.check_flow(key);
    }
}

/// Returns the template key of a flow.
fn template_key(gcx: Gcx<'_>, (contract, function): FlowKey) -> TemplateKey {
    let modifiers = gcx
        .hir
        .function(function)
        .modifiers
        .iter()
        .map(|modifier| {
            gcx.resolve_modifier_target(contract?, modifier).filter(|&id| {
                let modifier = gcx.hir.function(id);
                modifier.kind.is_modifier() && modifier.body.is_some()
            })
        })
        .collect();
    (function, modifiers)
}

/// Returns the location of a variable reported with 3464.
fn pointer_location(var: &hir::Variable<'_>) -> Option<DataLocation> {
    var.data_location.filter(|&loc| matches!(loc, DataLocation::Storage | DataLocation::Calldata))
}

fn is_analyzed(function: &hir::Function<'_>) -> bool {
    function.body.is_some() && !function.kind.is_modifier() && !function.is_yul
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OccurrenceKind {
    /// The variable becomes unassigned.
    Declaration,
    Assignment,
    Access,
    /// The variable is returned at the end of the function.
    Return,
}

#[derive(Clone, Copy)]
struct Occurrence {
    var: LocalId,
    kind: OccurrenceKind,
    span: Span,
}

/// The internal call a node ends in, resolved in each flow's contract.
#[derive(Clone, Copy)]
struct Call {
    function: hir::FunctionId,
    dispatch: CallDispatch,
}

struct Node {
    exits: SmallVec<[NodeId; 2]>,
    /// The hull of the source ranges solc attaches to the node.
    span: Option<Span>,
    /// The source of `span`.
    source: SourceId,
    call: Option<Call>,
}

/// How the flows of a template revert, regardless of their contract.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RevertClass {
    /// The exit is reachable without passing any call.
    NonReverting,
    /// The exit is unreachable even through calls.
    Reverting,
    /// The revert state depends on the calls.
    Depends,
}

struct Template {
    function: hir::FunctionId,
    nodes: IndexVec<NodeId, Node>,
    /// The occurrences of each node, as a range of `occurrences`.
    occurrence_ranges: IndexVec<NodeId, (u32, u32)>,
    occurrences: Vec<Occurrence>,
    locals: IndexVec<LocalId, hir::VariableId>,
    class: RevertClass,
    /// The analysis without revert pruning.
    base: Analysis,
}

impl Template {
    fn occurrences(&self, node: NodeId) -> &[Occurrence] {
        let (start, end) = self.occurrence_ranges[node];
        &self.occurrences[start as usize..end as usize]
    }

    fn exits(&self, node: NodeId, pruned: &DenseBitSet<NodeId>) -> &[NodeId] {
        if pruned.contains(node) { REVERT_EXITS } else { &self.nodes[node].exits }
    }
}

#[derive(Default)]
struct Analysis {
    /// The call nodes reachable from the entry.
    reachable_calls: Vec<NodeId>,
    reaches_exit: bool,
    /// The uninitialized accesses that reach the exit, in solc's order.
    uninitialized: Vec<Occurrence>,
    /// The merged unreachable ranges, sorted.
    unreachable: Vec<Span>,
}

struct Checker<'gcx> {
    gcx: Gcx<'gcx>,
    templates: IndexVec<TemplateId, Template>,
    /// The template of each analyzed function without modifiers.
    function_templates: IndexVec<hir::FunctionId, Option<TemplateId>>,
    /// The template of each flow of a function with modifiers.
    flow_templates: FxHashMap<FlowKey, TemplateId>,
    /// Whether a flow has a non-reverting path.
    revert_states: FxHashMap<FlowKey, bool>,
    /// The templates whose analysis without pruning was reported, which reporting again would
    /// not change.
    reported_templates: DenseBitSet<TemplateId>,
    reported: Reported,
}

/// The diagnostics already emitted.
#[derive(Default)]
struct Reported {
    pointers: FxHashSet<(hir::VariableId, Span, bool)>,
    unnamed: FxHashSet<hir::VariableId>,
    unreachable: FxHashSet<Span>,
}

/// The data shared by template builders.
struct BuildContext<'gcx> {
    gcx: Gcx<'gcx>,
    tracked: DenseBitSet<hir::VariableId>,
    /// The functions overridden by another function. Virtual calls to other functions are static.
    overridden: DenseBitSet<hir::FunctionId>,
    /// The spans of all inline assembly functions, sorted.
    yul_functions: Vec<Span>,
}

impl<'gcx> BuildContext<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        let mut tracked = DenseBitSet::new_empty(gcx.hir.variable_ids().len());
        for (id, var) in gcx.hir.variables_enumerated() {
            let reported = match var.kind {
                VarKind::FunctionReturn => var.name.is_none() || pointer_location(var).is_some(),
                VarKind::Statement => var.initializer.is_none() && pointer_location(var).is_some(),
                _ => false,
            };
            if reported && matches!(var.parent, Some(ItemId::Function(_))) {
                tracked.insert(id);
            }
        }
        let mut overridden = DenseBitSet::new_empty(gcx.hir.function_ids().len());
        for (id, _) in gcx.hir.functions_enumerated().filter(|(_, function)| function.override_) {
            for &item in gcx.base_override_items(id.into()) {
                if let ItemId::Function(base) = item {
                    overridden.insert(base);
                }
            }
        }
        let mut yul_functions = gcx
            .hir
            .functions()
            .filter(|function| function.is_yul)
            .map(|function| function.span)
            .collect::<Vec<_>>();
        yul_functions.sort_unstable();
        Self { gcx, tracked, overridden, yul_functions }
    }
}

impl<'gcx> Checker<'gcx> {
    fn check_flow(&mut self, key: FlowKey) {
        let id = self.template(key);
        let mut pruned = None;
        for i in 0..self.templates[id].base.reachable_calls.len() {
            let template = &self.templates[id];
            let node = template.base.reachable_calls[i];
            let call = template.nodes[node].call.unwrap();
            if let Some(callee) = self.callee(key.0, call)
                && !self.non_reverting(callee)
            {
                let len = self.templates[id].nodes.len();
                pruned.get_or_insert_with(|| DenseBitSet::new_empty(len)).insert(node);
            }
        }
        let template = &self.templates[id];
        match pruned {
            None if self.reported_templates.insert(id) => {
                self.reported.report(self.gcx, key, template, &template.base);
            }
            None => {}
            Some(pruned) => {
                self.reported.report(self.gcx, key, template, &analyze(template, &pruned));
            }
        }
    }

    /// Returns the template of a flow.
    fn template(&self, key: FlowKey) -> TemplateId {
        self.function_templates[key.1].unwrap_or_else(|| self.flow_templates[&key])
    }

    /// Returns the flow an internal call runs, if the callee is implemented.
    ///
    /// This is solc's `ASTNode::resolveFunctionCall` and `findScopeContract`.
    fn callee(&self, contract: Option<hir::ContractId>, call: Call) -> Option<FlowKey> {
        let id = match contract {
            Some(contract) => self.gcx.dispatch_call(contract, call.function, call.dispatch),
            None => call.function,
        };
        let function = self.gcx.hir.function(id);
        function.body.as_ref()?;
        let scope = function.contract.map(|base| {
            contract
                .filter(|&contract| {
                    self.gcx.hir.contract(contract).linearized_bases.contains(&base)
                })
                .unwrap_or(base)
        });
        Some((scope, id))
    }

    /// Returns `true` if a flow has a path from its entry to its exit that does not call a
    /// reverting function.
    fn non_reverting(&mut self, key: FlowKey) -> bool {
        if let Some(&state) = self.revert_states.get(&key) {
            return state;
        }
        let id = self.template(key);
        match self.templates[id].class {
            RevertClass::NonReverting => true,
            RevertClass::Reverting => false,
            RevertClass::Depends => {
                self.find_revert_states(key);
                self.revert_states[&key]
            }
        }
    }

    /// Computes the revert states of `key` and of the flows it can call.
    ///
    /// This is solc's `ControlFlowRevertPruner::findRevertStates`. Flows left unknown can only be
    /// blocked on recursion and count as reverting.
    fn find_revert_states(&mut self, key: FlowKey) {
        let mut pending = Vec::<(FlowKey, TemplateId)>::new();
        let mut indices = FxHashMap::<FlowKey, usize>::default();
        let mut stack = vec![key];
        while let Some(key) = stack.pop() {
            if self.revert_states.contains_key(&key) || indices.contains_key(&key) {
                continue;
            }
            let id = self.template(key);
            match self.templates[id].class {
                RevertClass::NonReverting => {
                    self.revert_states.insert(key, true);
                }
                RevertClass::Reverting => {
                    self.revert_states.insert(key, false);
                }
                RevertClass::Depends => {
                    indices.insert(key, pending.len());
                    pending.push((key, id));
                    let template = &self.templates[id];
                    stack.extend(
                        template
                            .base
                            .reachable_calls
                            .iter()
                            .filter_map(|&node| self.callee(key.0, template.nodes[node].call?)),
                    );
                }
            }
        }

        let mut wake_up = vec![Vec::new(); pending.len()];
        let mut queue = (0..pending.len()).rev().collect::<Vec<_>>();
        let mut visited = DenseBitSet::new_empty(0);
        let mut worklist = Vec::new();
        while let Some(item) = queue.pop() {
            let (key, id) = pending[item];
            if self.revert_states.contains_key(&key) {
                continue;
            }
            let template = &self.templates[id];
            let mut found_exit = false;
            let mut found_unknown = false;
            visited.clear_to(template.nodes.len());
            visited.insert(ENTRY);
            worklist.push(ENTRY);
            while let Some(node) = worklist.pop() {
                found_exit |= node == EXIT;
                if let Some(call) = template.nodes[node].call
                    && let Some(callee) = self.callee(key.0, call)
                {
                    match self.revert_states.get(&callee) {
                        Some(true) => {}
                        Some(false) => continue,
                        None => {
                            let waiting = &mut wake_up[indices[&callee]];
                            if waiting.last() != Some(&item) {
                                waiting.push(item);
                            }
                            found_unknown = true;
                            continue;
                        }
                    }
                }
                for &exit in &template.nodes[node].exits {
                    if visited.insert(exit) {
                        worklist.push(exit);
                    }
                }
            }
            if found_exit {
                self.revert_states.insert(key, true);
            } else if !found_unknown {
                self.revert_states.insert(key, false);
            } else {
                continue;
            }
            queue.append(&mut wake_up[item]);
        }
        for (key, _) in pending {
            self.revert_states.entry(key).or_insert(false);
        }
    }
}

impl Reported {
    fn report(&mut self, gcx: Gcx<'_>, key: FlowKey, template: &Template, analysis: &Analysis) {
        let function = gcx.hir.function(template.function);
        for &Occurrence { var, kind, span } in &analysis.uninitialized {
            let var = template.locals[var];
            let variable = gcx.hir.variable(var);
            let returned = kind == OccurrenceKind::Return;
            if let Some(location) = pointer_location(variable) {
                if !self.pointers.insert((var, span, returned)) {
                    continue;
                }
                let action = if returned { "returned" } else { "accessed" };
                let mut diag = gcx
                    .dcx()
                    .err(format!(
                        "this variable is of {location} pointer type and can be {action} without \
                         prior assignment, which would lead to undefined behaviour"
                    ))
                    .code(error_code!(3464))
                    .span(span);
                if !returned {
                    diag = diag.span_note(variable.span, "the variable was declared here");
                }
                diag.emit();
            } else if function.body.is_some_and(|body| !body.stmts.is_empty())
                && variable.name.is_none()
                && self.unnamed.insert(var)
            {
                let msg = match key.0.filter(|&contract| Some(contract) != function.contract) {
                    Some(contract) => format!(
                        "unnamed return variable can remain unassigned when the function is \
                         called when `{}` is the most derived contract",
                        gcx.hir.contract(contract).name
                    ),
                    None => "unnamed return variable can remain unassigned".to_string(),
                };
                gcx.dcx()
                    .warn(msg)
                    .code(error_code!(6321))
                    .span(span)
                    .help(
                        "add an explicit return with value to all non-reverting code paths or \
                         name the variable",
                    )
                    .emit();
            }
        }
        for &span in &analysis.unreachable {
            if self.unreachable.insert(span) {
                gcx.dcx().warn("unreachable code").code(error_code!(5740)).span(span).emit();
            }
        }
    }
}

struct Builder<'a, 'gcx> {
    cx: &'a BuildContext<'gcx>,
    function: &'gcx hir::Function<'gcx>,
    nodes: IndexVec<NodeId, Node>,
    occurrences: Vec<(NodeId, Occurrence)>,
    locals: IndexVec<LocalId, hir::VariableId>,
    current: NodeId,
    return_node: NodeId,
    loop_targets: Option<(NodeId, NodeId)>,
    placeholder: Option<(NodeId, NodeId)>,
    source: SourceId,
    in_assembly: bool,
}

impl<'a, 'gcx> Builder<'a, 'gcx> {
    fn build(
        cx: &'a BuildContext<'gcx>,
        function_id: hir::FunctionId,
        modifiers: &[Option<hir::FunctionId>],
    ) -> Template {
        let function = cx.gcx.hir.function(function_id);
        let mut this = Self {
            cx,
            function,
            nodes: IndexVec::new(),
            occurrences: Vec::new(),
            locals: IndexVec::new(),
            current: ENTRY,
            return_node: EXIT,
            loop_targets: None,
            placeholder: None,
            source: function.source,
            in_assembly: false,
        };
        for _ in [ENTRY, EXIT, REVERT, TRANSACTION_RETURN] {
            this.new_node();
        }
        for &ret in function.returns {
            let span = cx.gcx.hir.variable(ret).span;
            this.occur_at(ENTRY, ret, OccurrenceKind::Declaration, span);
            this.occur_at(EXIT, ret, OccurrenceKind::Return, span);
        }
        for (modifier, &target) in function.modifiers.iter().zip(modifiers) {
            for arg in modifier.args.exprs() {
                this.expr(arg);
            }
            if let Some(target) = target {
                this.modifier(target);
            }
        }
        if let Some(body) = &function.body {
            this.extend(body.span);
            this.stmts(body.stmts);
        }
        this.connect(this.current, this.return_node);
        this.finish(function_id)
    }

    fn finish(self, function: hir::FunctionId) -> Template {
        let Self { nodes, mut occurrences, locals, .. } = self;
        occurrences.sort_by_key(|&(node, _)| node);
        let mut occurrence_ranges =
            IndexVec::<NodeId, (u32, u32)>::from_vec(vec![(0, 0); nodes.len()]);
        for (i, &(node, _)) in occurrences.iter().enumerate() {
            let range = &mut occurrence_ranges[node];
            if range.0 == range.1 {
                range.0 = i as u32;
            }
            range.1 = i as u32 + 1;
        }
        let mut template = Template {
            function,
            nodes,
            occurrence_ranges,
            occurrences: occurrences.into_iter().map(|(_, occurrence)| occurrence).collect(),
            locals,
            class: RevertClass::Depends,
            base: Analysis::default(),
        };
        template.base = analyze(&template, &DenseBitSet::new_empty(template.nodes.len()));
        template.class = if !template.base.reaches_exit {
            RevertClass::Reverting
        } else if template.base.reachable_calls.is_empty() || reaches_exit_without_calls(&template)
        {
            RevertClass::NonReverting
        } else {
            RevertClass::Depends
        };
        template
    }

    fn modifier(&mut self, id: hir::FunctionId) {
        let modifier = self.cx.gcx.hir.function(id);
        let Some(body) = &modifier.body else { return };
        let entry = self.new_node();
        let exit = self.new_node();
        self.placeholder = Some((entry, exit));
        let source = std::mem::replace(&mut self.source, modifier.source);
        self.extend(modifier.span);
        self.stmts(body.stmts);
        self.source = source;
        self.connect(self.current, self.return_node);
        self.current = entry;
        self.return_node = exit;
        self.placeholder = None;
    }

    fn stmts(&mut self, stmts: &'gcx [hir::Stmt<'gcx>]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    /// Builds the statements of an inline assembly block, whose braces carry no range.
    fn yul_stmts(&mut self, block: &hir::Block<'gcx>) {
        let mut lo = block.span.lo();
        for stmt in block.stmts {
            self.yul_functions(lo, stmt.span.lo());
            self.stmt(stmt);
            lo = stmt.span.hi();
        }
        self.yul_functions(lo, block.span.hi());
    }

    /// Extends the current node with the inline assembly functions defined between `lo` and `hi`.
    fn yul_functions(&mut self, lo: BytePos, hi: BytePos) {
        let functions = &self.cx.yul_functions;
        let start = functions.partition_point(|span| span.lo() < lo);
        for i in start..functions.len() {
            let span = self.cx.yul_functions[i];
            if span.lo() >= hi {
                break;
            }
            if span.hi() <= hi {
                self.extend(span);
            }
        }
    }

    /// Returns the range solc attaches to a statement.
    fn stmt_span(&self, stmt: &hir::Stmt<'gcx>) -> Option<Span> {
        if self.in_assembly {
            return Some(stmt.span);
        }
        Some(match stmt.kind {
            StmtKind::DeclSingle(var) => {
                let var = self.cx.gcx.hir.variable(var);
                stmt.span.with_hi(var.initializer.map_or(var.span, |init| init.span).hi())
            }
            // The call of `emit` and `revert` statements has the span of the statement.
            StmtKind::Emit(expr) | StmtKind::Revert(expr) => match expr.kind {
                ExprKind::Call(_, ref args) => stmt.span.with_hi(args.span.hi()),
                _ => expr.span,
            },
            StmtKind::DeclMulti(_, expr) | StmtKind::Return(Some(expr)) => {
                stmt.span.with_hi(expr.span.hi())
            }
            StmtKind::Expr(expr) => expr.span,
            StmtKind::Break => stmt.span.with_hi(stmt.span.lo() + "break".len() as u32),
            StmtKind::Continue => stmt.span.with_hi(stmt.span.lo() + "continue".len() as u32),
            StmtKind::Return(None)
            | StmtKind::Block(_)
            | StmtKind::UncheckedBlock(_)
            | StmtKind::Loop(..)
            | StmtKind::If(..) => stmt.span,
            StmtKind::AssemblyBlock(_)
            | StmtKind::Switch(_)
            | StmtKind::Try(_)
            | StmtKind::Placeholder
            | StmtKind::Err(_) => return None,
        })
    }

    fn stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) {
        if let Some(span) = self.stmt_span(stmt) {
            self.extend(span);
        }
        match stmt.kind {
            StmtKind::DeclSingle(var) => {
                let variable = self.cx.gcx.hir.variable(var);
                self.occur(var, OccurrenceKind::Declaration, variable.span);
                if let Some(init) = variable.initializer {
                    self.expr(init);
                    self.occur(var, OccurrenceKind::Assignment, init.span);
                }
            }
            StmtKind::DeclMulti(vars, init) => {
                for &var in vars.iter().flatten() {
                    let span = self.cx.gcx.hir.variable(var).span;
                    self.occur(var, OccurrenceKind::Declaration, span);
                }
                self.expr(init);
                for &var in vars.iter().flatten() {
                    self.occur(var, OccurrenceKind::Assignment, init.span);
                }
            }
            StmtKind::Block(ref block) | StmtKind::UncheckedBlock(ref block) => {
                if self.in_assembly {
                    self.yul_stmts(block);
                } else {
                    self.stmts(block.stmts);
                }
            }
            StmtKind::AssemblyBlock(ref block) => {
                self.in_assembly = true;
                self.yul_stmts(block);
                self.in_assembly = false;
            }
            StmtKind::Emit(&hir::Expr { kind: ExprKind::Call(callee, ref args), .. }) => {
                self.expr(callee);
                for arg in args.exprs() {
                    self.expr(arg);
                }
            }
            StmtKind::Emit(expr) | StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::Revert(_) => self.jump(REVERT),
            StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.expr(expr);
                    for &ret in self.function.returns {
                        self.occur(ret, OccurrenceKind::Assignment, stmt.span);
                    }
                }
                self.jump(self.return_node);
            }
            StmtKind::Break => {
                if let Some((target, _)) = self.loop_targets {
                    self.jump(target);
                }
            }
            StmtKind::Continue => {
                if let Some((_, target)) = self.loop_targets {
                    self.jump(target);
                }
            }
            StmtKind::Loop(ref block, source) => self.loop_stmt(block, source),
            StmtKind::If(cond, then, else_) => {
                self.expr(cond);
                let [then_start, else_start] = self.split();
                let then_end = self.build_from(then_start, |this| this.stmt(then));
                match else_ {
                    Some(else_) => {
                        let else_end = self.build_from(else_start, |this| this.stmt(else_));
                        self.merge(&[then_end, else_end]);
                    }
                    None => {
                        self.connect(then_end, else_start);
                        self.current = else_start;
                    }
                }
            }
            StmtKind::Switch(switch) => {
                self.expr(switch.selector);
                let before = self.current;
                let ends = switch
                    .cases
                    .iter()
                    .map(|case| {
                        let start = self.new_node();
                        self.connect(before, start);
                        self.build_from(start, |this| this.yul_stmts(&case.body))
                    })
                    .collect::<SmallVec<[_; 4]>>();
                self.merge(&ends);
                if switch.cases.last().is_none_or(|case| case.constant.is_some()) {
                    self.connect(before, self.current);
                }
            }
            StmtKind::Try(try_) => {
                self.expr(&try_.expr);
                let before = self.current;
                let ends = try_
                    .clauses
                    .iter()
                    .map(|clause| {
                        let start = self.new_node();
                        self.connect(before, start);
                        self.build_from(start, |this| {
                            this.extend(clause.block.span);
                            this.stmts(clause.block.stmts);
                        })
                    })
                    .collect::<SmallVec<[_; 4]>>();
                self.merge(&ends);
            }
            StmtKind::Placeholder => {
                if let Some((entry, exit)) = self.placeholder {
                    self.connect(self.current, entry);
                    self.current = self.new_node();
                    self.connect(exit, self.current);
                }
            }
            StmtKind::Err(_) => {}
        }
    }

    /// Builds a lowered loop.
    fn loop_stmt(&mut self, block: &'gcx hir::Block<'gcx>, source: LoopSource<'gcx>) {
        // loop {
        //     <body>
        //     if (<cond>) continue else break;
        // }
        if let LoopSource::DoWhile = source {
            let [body @ .., check] = block.stmts else { return };
            let StmtKind::If(cond, _, _) = check.kind else { return };
            let after = self.new_node();
            self.next();
            let body_start = self.current;
            let condition = self.new_node();
            self.in_loop(after, condition, |this| this.stmts(body));
            self.connect(self.current, condition);
            self.current = condition;
            self.expr(cond);
            self.connect(self.current, body_start);
            self.connect(self.current, after);
            self.current = after;
            return;
        }

        // loop {
        //     if (<cond>) <body> else break;
        // }
        // <update>
        self.next();
        let condition = self.current;
        let body = match block.stmts {
            [hir::Stmt { kind: StmtKind::If(cond, then, Some(else_)), .. }]
                if matches!(else_.kind, StmtKind::Break)
                    && (else_.span == block.span || else_.span == cond.span) =>
            {
                self.expr(cond);
                std::slice::from_ref(*then)
            }
            stmts => stmts,
        };
        let [body_start, after] = self.split();
        let (update, post) = match source {
            LoopSource::For { update } => (update, self.new_node()),
            _ => (None, condition),
        };
        self.current = body_start;
        self.in_loop(after, post, |this| this.stmts(body));
        if post != condition {
            self.connect(self.current, post);
            self.current = post;
        }
        if let Some(update) = update {
            match update.kind {
                StmtKind::Block(ref step) if self.in_assembly => self.yul_stmts(step),
                _ => self.stmt(update),
            }
        }
        self.connect(self.current, condition);
        self.current = after;
    }

    fn expr(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        self.extend_expr(expr.span);
        match expr.kind {
            ExprKind::Ident(_) | ExprKind::YulMember(..) => {
                if let Some((var, span)) = self.var_ref(expr) {
                    self.occur(var, OccurrenceKind::Access, span);
                } else if let ExprKind::YulMember(base, _) = expr.kind {
                    self.expr(base);
                }
            }
            ExprKind::Assign(lhs, _, rhs) => {
                if self.in_assembly {
                    self.expr(rhs);
                    self.assign(lhs);
                } else {
                    self.assign(lhs);
                    self.expr(rhs);
                }
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.expr(lhs);
                if matches!(op.kind, BinOpKind::And | BinOpKind::Or) {
                    let [rhs_start, after] = self.split();
                    let rhs_end = self.build_from(rhs_start, |this| this.expr(rhs));
                    self.connect(rhs_end, after);
                    self.current = after;
                } else {
                    self.expr(rhs);
                    self.user_operator(expr);
                }
            }
            ExprKind::Unary(op, operand) => {
                if op.kind.has_side_effects() {
                    self.assign(operand);
                } else {
                    self.expr(operand);
                }
                self.user_operator(expr);
            }
            ExprKind::Ternary(cond, true_, false_) => {
                self.expr(cond);
                let [true_start, false_start] = self.split();
                let true_end = self.build_from(true_start, |this| this.expr(true_));
                let false_end = self.build_from(false_start, |this| this.expr(false_));
                self.merge(&[true_end, false_end]);
            }
            ExprKind::Call(callee, ref args) => {
                self.expr(callee);
                for arg in args.exprs() {
                    self.expr(arg);
                }
                self.call(callee);
            }
            ExprKind::CallOptions(callee, options) => {
                self.expr(callee);
                for arg in options.args {
                    self.expr(&arg.value);
                }
            }
            ExprKind::Delete(base) => self.assign(base),
            ExprKind::Member(base, _) | ExprKind::Payable(base) => self.expr(base),
            ExprKind::Index(base, index) => {
                self.expr(base);
                if let Some(index) = index {
                    self.expr(index);
                }
            }
            ExprKind::Slice(base, start, end) => {
                self.expr(base);
                for expr in [start, end].into_iter().flatten() {
                    self.expr(expr);
                }
            }
            ExprKind::Array(exprs) => {
                for expr in exprs {
                    self.expr(expr);
                }
            }
            ExprKind::Tuple(exprs) => {
                for expr in exprs.iter().flatten() {
                    self.expr(expr);
                }
            }
            ExprKind::Lit(_)
            | ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }
    }

    /// Visits an expression that is written to.
    fn assign(&mut self, lhs: &'gcx hir::Expr<'gcx>) {
        if let ExprKind::Tuple(exprs) = lhs.kind {
            self.extend_expr(lhs.span);
            for expr in exprs.iter().flatten() {
                self.assign(expr);
            }
        } else if let Some((var, span)) = self.var_ref(lhs) {
            self.extend_expr(lhs.span);
            self.occur(var, OccurrenceKind::Assignment, span);
        } else {
            self.expr(lhs);
        }
    }

    /// Returns the variable an identifier or inline assembly member access names, and the span
    /// solc reports for it.
    fn var_ref(&self, expr: &hir::Expr<'_>) -> Option<(hir::VariableId, Span)> {
        // Variables are never overloaded, so name resolution is final.
        let (var, span) = match expr.kind {
            ExprKind::Ident(&[Res::Item(ItemId::Variable(var))]) => (var, expr.span),
            ExprKind::YulMember(
                base @ &hir::Expr {
                    kind: ExprKind::Ident(&[Res::Item(ItemId::Variable(var))]), ..
                },
                _,
            ) => (var, base.span.to(expr.span)),
            _ => return None,
        };
        Some((var, span))
    }

    fn call(&mut self, callee: &'gcx hir::Expr<'gcx>) {
        let gcx = self.cx.gcx;
        let builtin = match callee.kind {
            ExprKind::Ident(res) if res.iter().any(|res| matches!(res, Res::Builtin(_))) => {
                gcx.resolved_builtin(callee)
            }
            _ => None,
        };
        if self.in_assembly {
            match builtin {
                Some(Builtin::YulReturn | Builtin::YulStop | Builtin::YulSelfdestruct) => {
                    self.jump(TRANSACTION_RETURN)
                }
                Some(Builtin::YulRevert | Builtin::YulInvalid) => self.jump(REVERT),
                _ => {}
            }
            return;
        }
        match builtin {
            Some(Builtin::Revert | Builtin::RevertMsg) => self.jump(REVERT),
            Some(Builtin::Require | Builtin::Assert) => {
                self.connect(self.current, REVERT);
                self.next();
            }
            _ => {
                let Some(TyKind::Fn(ty)) = gcx.type_of_expr(callee.id).map(|ty| ty.kind) else {
                    return;
                };
                if !ty.is_internal() {
                    return;
                }
                let function = matches!(callee.kind, ExprKind::Ident(_) | ExprKind::Member(..))
                    .then(|| ty.function_id.or_else(|| gcx.resolved_function(callee)))
                    .flatten();
                let call = function.and_then(|function| {
                    let dispatch = match gcx.call_dispatch(callee) {
                        CallDispatch::Virtual if !self.cx.overridden.contains(function) => {
                            CallDispatch::Static
                        }
                        dispatch => dispatch,
                    };
                    self.implemented_call(function, dispatch)
                });
                self.call_node(call);
            }
        }
    }

    /// Returns the call of `function`, unless it is static and has no body.
    fn implemented_call(&self, function: hir::FunctionId, dispatch: CallDispatch) -> Option<Call> {
        let implemented =
            dispatch != CallDispatch::Static || self.cx.gcx.hir.function(function).body.is_some();
        implemented.then_some(Call { function, dispatch })
    }

    fn user_operator(&mut self, expr: &hir::Expr<'_>) {
        if let Some(function) = self.cx.gcx.user_operator(expr.id) {
            self.call_node(self.implemented_call(function, CallDispatch::Static));
        }
    }

    /// Ends the current node in an internal call.
    fn call_node(&mut self, call: Option<Call>) {
        self.nodes[self.current].call = call;
        self.next();
    }

    fn occur(&mut self, var: hir::VariableId, kind: OccurrenceKind, span: Span) {
        self.occur_at(self.current, var, kind, span);
    }

    fn occur_at(&mut self, node: NodeId, var: hir::VariableId, kind: OccurrenceKind, span: Span) {
        if !self.cx.tracked.contains(var) {
            return;
        }
        let var = match self.locals.iter().position(|&local| local == var) {
            Some(i) => LocalId::new(i),
            None => self.locals.push(var),
        };
        self.occurrences.push((node, Occurrence { var, kind, span }));
    }

    /// Extends the range of the current node.
    fn extend(&mut self, span: Span) {
        let node = &mut self.nodes[self.current];
        match node.span {
            None => {
                node.span = Some(span);
                node.source = self.source;
            }
            Some(hull) if node.source == self.source => node.span = Some(hull.to(span)),
            // NOTE: solc merges ranges from different sources into an invalid one, which only
            // happens when a modifier from another source starts in the node.
            Some(_) => {}
        }
    }

    /// Extends the range of the current node with a Solidity expression.
    fn extend_expr(&mut self, span: Span) {
        if !self.in_assembly {
            self.extend(span);
        }
    }

    fn new_node(&mut self) -> NodeId {
        self.nodes.push(Node {
            exits: SmallVec::new(),
            span: None,
            source: self.source,
            call: None,
        })
    }

    fn connect(&mut self, from: NodeId, to: NodeId) {
        self.nodes[from].exits.push(to);
    }

    /// Continues in a new node after the current one.
    fn next(&mut self) {
        let next = self.new_node();
        self.connect(self.current, next);
        self.current = next;
    }

    /// Jumps to `target` and continues in an unreachable node.
    fn jump(&mut self, target: NodeId) {
        self.connect(self.current, target);
        self.current = self.new_node();
    }

    /// Splits the current node into two new nodes.
    fn split(&mut self) -> [NodeId; 2] {
        let nodes = [self.new_node(), self.new_node()];
        for node in nodes {
            self.connect(self.current, node);
        }
        nodes
    }

    /// Builds `f` from `start` and returns the node it ends in.
    fn build_from(&mut self, start: NodeId, f: impl FnOnce(&mut Self)) -> NodeId {
        self.current = start;
        f(self);
        self.current
    }

    /// Continues in a new node after all of `ends`.
    fn merge(&mut self, ends: &[NodeId]) {
        self.current = self.new_node();
        for &end in ends {
            self.connect(end, self.current);
        }
    }

    fn in_loop(
        &mut self,
        break_target: NodeId,
        continue_target: NodeId,
        f: impl FnOnce(&mut Self),
    ) {
        let targets = self.loop_targets.replace((break_target, continue_target));
        f(self);
        self.loop_targets = targets;
    }
}

/// Returns `true` if the exit of a template is reachable without passing a call.
fn reaches_exit_without_calls(template: &Template) -> bool {
    let mut visited = DenseBitSet::new_empty(template.nodes.len());
    let mut worklist = vec![ENTRY];
    visited.insert(ENTRY);
    while let Some(node) = worklist.pop() {
        if node == EXIT {
            return true;
        }
        if template.nodes[node].call.is_some() {
            continue;
        }
        for &exit in &template.nodes[node].exits {
            if visited.insert(exit) {
                worklist.push(exit);
            }
        }
    }
    false
}

/// Returns the predecessors of each node.
fn predecessors(
    template: &Template,
    pruned: &DenseBitSet<NodeId>,
) -> IndexVec<NodeId, SmallVec<[NodeId; 2]>> {
    let mut entries = IndexVec::<NodeId, SmallVec<[NodeId; 2]>>::from_vec(vec![
        SmallVec::new();
        template.nodes.len()
    ]);
    for node in template.nodes.indices() {
        for &exit in template.exits(node, pruned) {
            entries[exit].push(node);
        }
    }
    entries
}

/// Returns the nodes that can reach one of `roots`.
fn reaching(
    entries: &IndexVec<NodeId, SmallVec<[NodeId; 2]>>,
    roots: &[NodeId],
) -> DenseBitSet<NodeId> {
    let mut visited = DenseBitSet::new_empty(entries.len());
    let mut worklist = roots.to_vec();
    for &root in roots {
        visited.insert(root);
    }
    while let Some(node) = worklist.pop() {
        for &entry in &entries[node] {
            if visited.insert(entry) {
                worklist.push(entry);
            }
        }
    }
    visited
}

/// Analyzes a template where the nodes in `pruned` exit only to the revert node.
///
/// This is solc's `ControlFlowAnalyzer::checkUninitializedAccess` and `checkUnreachable`.
fn analyze(template: &Template, pruned: &DenseBitSet<NodeId>) -> Analysis {
    let num_nodes = template.nodes.len();
    let mut reachable = DenseBitSet::new_empty(num_nodes);
    let mut worklist = vec![ENTRY];
    reachable.insert(ENTRY);
    while let Some(node) = worklist.pop() {
        for &exit in template.exits(node, pruned) {
            if reachable.insert(exit) {
                worklist.push(exit);
            }
        }
    }
    let reachable_calls =
        reachable.iter().filter(|&node| template.nodes[node].call.is_some()).collect();
    let reaches_exit = reachable.contains(EXIT);

    // The predecessors of each node, built on first use.
    let mut entries = None;

    let mut uninitialized = Vec::new();
    if reaches_exit && !template.locals.is_empty() {
        let transfer =
            |state: &mut DenseBitSet<LocalId>, occurrence: &Occurrence| match occurrence.kind {
                OccurrenceKind::Declaration => _ = state.insert(occurrence.var),
                OccurrenceKind::Assignment => _ = state.remove(occurrence.var),
                OccurrenceKind::Access | OccurrenceKind::Return => {}
            };
        let mut unassigned = IndexVec::<NodeId, DenseBitSet<LocalId>>::from_vec(vec![
            DenseBitSet::new_empty(template.locals.len());
            num_nodes
        ]);
        let mut queued = DenseBitSet::new_empty(num_nodes);
        let mut state = DenseBitSet::new_empty(template.locals.len());
        worklist.push(ENTRY);
        queued.insert(ENTRY);
        while let Some(node) = worklist.pop() {
            queued.remove(node);
            state.clone_from(&unassigned[node]);
            for occurrence in template.occurrences(node) {
                transfer(&mut state, occurrence);
            }
            for &exit in template.exits(node, pruned) {
                if unassigned[exit].union(&state) && queued.insert(exit) {
                    worklist.push(exit);
                }
            }
        }

        let mut reaches_exit = None;
        for node in reachable.iter() {
            state.clone_from(&unassigned[node]);
            for occurrence in template.occurrences(node) {
                transfer(&mut state, occurrence);
                if matches!(occurrence.kind, OccurrenceKind::Access | OccurrenceKind::Return)
                    && state.contains(occurrence.var)
                    && (node == EXIT
                        || reaches_exit
                            .get_or_insert_with(|| {
                                reaching(
                                    entries.get_or_insert_with(|| predecessors(template, pruned)),
                                    &[EXIT],
                                )
                            })
                            .contains(node))
                {
                    uninitialized.push(*occurrence);
                }
            }
        }
        // Like solc, returns have no location of their own and come first.
        uninitialized.sort_by_key(|occurrence| {
            let returned = occurrence.kind == OccurrenceKind::Return;
            let span = (!returned).then_some((occurrence.span.lo(), occurrence.span.hi()));
            (span, occurrence.var, occurrence.kind)
        });
    }

    let mut unreachable = Vec::<Span>::new();
    if template
        .nodes
        .iter_enumerated()
        .any(|(id, node)| node.span.is_some() && !reachable.contains(id))
    {
        let live = reaching(
            entries.get_or_insert_with(|| predecessors(template, pruned)),
            &[EXIT, REVERT, TRANSACTION_RETURN],
        );
        unreachable.extend(template.nodes.iter_enumerated().filter_map(|(id, node)| {
            node.span.filter(|_| !reachable.contains(id) && live.contains(id))
        }));
        unreachable.sort_unstable_by_key(|span| (span.lo(), span.hi()));
        unreachable.dedup_by(|next, merged| {
            let overlaps = next.lo() <= merged.hi();
            if overlaps {
                *merged = merged.to(*next);
            }
            overlaps
        });
    }

    Analysis { reachable_calls, reaches_exit, uninitialized, unreachable }
}
