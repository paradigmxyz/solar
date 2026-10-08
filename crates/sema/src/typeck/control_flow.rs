//! Uninitialized storage and calldata pointer checks.
//!
//! This ports solc's control flow graph (`ControlFlowBuilder`), revert pruning
//! (`ControlFlowRevertPruner`), and uninitialized access analysis (`ControlFlowAnalyzer`) to report
//! error 3464: a local storage or calldata pointer that can be read, or a storage or calldata
//! return variable that can be returned, before any assignment.
//!
//! As in solc, every implemented function gets one graph per most-derived contract that inherits
//! it, so modifiers, virtual calls, and `super` calls resolve in that contract. Free functions get
//! one graph without a contract. Modifier bodies are inlined at their invocation, and `_` jumps to
//! the next modifier or to the function body. Each graph has an entry, an exit, a revert node, and
//! a node for inline assembly `return` and `stop`. Only paths that reach the exit count.
//!
//! A node that ends in an internal call to a function whose every path reverts, in the contract the
//! call resolves to, loses its exits. Revert states come from solc's least fixed point: a function
//! has a non-reverting path if a search from its entry reaches its exit without passing a call
//! known to revert, and searches blocked on recursion count as reverting. Pruning only removes
//! reports, so the callees are built and pruned only for graphs that report without it.
//!
//! The analysis then propagates sets of unassigned variables and of uninitialized accesses along
//! the graph until nothing changes, and reports the accesses that reach the exit. Declarations
//! without initializers make a variable unassigned; assignments, including those in tuples and
//! inline assembly, make it assigned; `return` with a value assigns all return variables.
//! Assignments read their left side before their right side in Solidity and after it in inline
//! assembly. Like solc, a custom error `revert` does not read its arguments, `require` and `assert`
//! do not end a path, and inline assembly functions are not analyzed.
//!
//! Only the variables that can be reported are tracked: return variables and local variables
//! declared without an initializer, so the graph does not keep other variables. This skips solc's
//! unreachable code warning (5740) and unnamed return warning (6321). Solc also reports a base
//! function once per derived contract; we report each location once.

use crate::{
    builtins::Builtin,
    hir::{self, BinOpKind, ExprKind, ItemId, LoopSource, StmtKind, VarKind},
    ty::{Gcx, Ty},
};
use solar_ast::DataLocation;
use solar_data_structures::{
    bit_set::{DenseBitSet, MixedBitSet},
    index::IndexVec,
    map::{FxHashMap, FxIndexSet, StdEntry},
    newtype_index,
};
use solar_interface::{Span, error_code};

newtype_index! {
    /// A node of a function's control flow graph.
    struct NodeId;

    /// An occurrence of a tracked variable in a control flow graph.
    struct OccurrenceId;

    /// An interned [`FlowKey`].
    struct FlowId;
}

const ENTRY: NodeId = NodeId::new(0);
const EXIT: NodeId = NodeId::new(1);
const REVERT: NodeId = NodeId::new(2);
const TRANSACTION_RETURN: NodeId = NodeId::new(3);

/// The most-derived contract and the function a graph is built for.
type FlowKey = (Option<hir::ContractId>, hir::FunctionId);

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

    let mut tracked = DenseBitSet::new_empty(gcx.hir.variable_ids().len());
    let mut owners = DenseBitSet::new_empty(gcx.hir.function_ids().len());
    for (id, var) in gcx.hir.variables_enumerated() {
        let uninitialized = match var.kind {
            VarKind::FunctionReturn => true,
            VarKind::Statement => var.initializer.is_none(),
            _ => false,
        };
        if uninitialized
            && let Some(ItemId::Function(function)) = var.parent
            && is_pointer(gcx.type_of_item(id.into()))
        {
            tracked.insert(id);
            owners.insert(function);
        }
    }
    if tracked.is_empty() {
        return;
    }

    let owners = &owners;
    let contract_candidates = gcx.hir.contract_ids().flat_map(|contract| {
        gcx.hir.contract_item_ids(contract).filter_map(|item| item.as_function()).filter_map(
            move |function_id| {
                let function = gcx.hir.function(function_id);
                (is_analyzed(function)
                    && (owners.contains(function_id)
                        || function.modifiers.iter().any(|modifier| {
                            gcx.resolve_modifier_target(contract, modifier)
                                .is_some_and(|modifier| owners.contains(modifier))
                        })))
                .then_some((Some(contract), function_id))
            },
        )
    });
    let free_candidates = gcx.hir.functions_enumerated().filter_map(|(function_id, function)| {
        (function.is_free() && is_analyzed(function) && owners.contains(function_id))
            .then_some((None, function_id))
    });

    let mut keys = FxIndexSet::default();
    let mut scratch = Scratch::new(gcx.hir.variable_ids().len());
    let mut flows = FxHashMap::default();
    let mut reporting = Vec::new();
    for key in contract_candidates.chain(free_candidates) {
        let id = FlowId::new(keys.insert_full(key).0);
        let flow = Builder::build(gcx, &tracked, &mut keys, key);
        let mut reports = false;
        analyze(&flow, None, &mut scratch, |_| reports = true);
        if reports {
            flows.insert(id, flow);
            reporting.push(id);
        }
    }
    if reporting.is_empty() {
        return;
    }

    // Build every function the reporting flows can call, for revert pruning.
    let mut queue = reporting.clone();
    let mut callees = Vec::new();
    while let Some(id) = queue.pop() {
        callees.extend(flows[&id].nodes.iter().filter_map(|node| node.callee));
        for callee in callees.drain(..) {
            if let StdEntry::Vacant(entry) = flows.entry(callee) {
                let key = keys[callee.index()];
                entry.insert(Builder::build(gcx, &tracked, &mut keys, key));
                queue.push(callee);
            }
        }
    }
    let non_reverting = find_non_reverting(&flows, keys.len(), &mut scratch);

    let mut reports = Vec::new();
    for id in reporting {
        analyze(&flows[&id], Some(&non_reverting), &mut scratch, |report| reports.push(report));
    }
    reports.sort_by_key(|report| (report.span.lo(), report.span.hi(), report.returned));
    reports.dedup();
    for Report { var, span, returned } in reports {
        let location = if gcx.type_of_item(var.into()).data_stored_in(DataLocation::Calldata) {
            "calldata"
        } else {
            "storage"
        };
        let action = if returned { "returned" } else { "accessed" };
        let mut diag = gcx
            .dcx()
            .err(format!(
                "this variable is of {location} pointer type and can be {action} without prior \
                 assignment, which would lead to undefined behaviour"
            ))
            .code(error_code!(3464))
            .span(span);
        if !returned {
            diag = diag.span_note(gcx.hir.variable(var).span, "the variable was declared here");
        }
        diag.emit();
    }
}

/// Returns `true` if solc's analysis reports on variables of this type.
fn is_pointer(ty: Ty<'_>) -> bool {
    ty.data_stored_in(DataLocation::Storage) || ty.data_stored_in(DataLocation::Calldata)
}

fn is_analyzed(function: &hir::Function<'_>) -> bool {
    function.body.is_some() && !function.kind.is_modifier() && !function.is_yul
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    var: hir::VariableId,
    kind: OccurrenceKind,
    span: Span,
}

#[derive(Default)]
struct Node {
    exits: Vec<NodeId>,
    occurrences: Vec<OccurrenceId>,
    /// The internal call this node ends in.
    callee: Option<FlowId>,
}

#[derive(Default)]
struct Flow {
    nodes: IndexVec<NodeId, Node>,
    occurrences: IndexVec<OccurrenceId, Occurrence>,
}

struct Builder<'a, 'gcx> {
    gcx: Gcx<'gcx>,
    tracked: &'a DenseBitSet<hir::VariableId>,
    keys: &'a mut FxIndexSet<FlowKey>,
    contract: Option<hir::ContractId>,
    function: &'gcx hir::Function<'gcx>,
    flow: Flow,
    current: NodeId,
    return_node: NodeId,
    break_target: Option<NodeId>,
    continue_target: Option<NodeId>,
    placeholder: Option<(NodeId, NodeId)>,
    in_assembly: bool,
}

impl<'a, 'gcx> Builder<'a, 'gcx> {
    fn build(
        gcx: Gcx<'gcx>,
        tracked: &'a DenseBitSet<hir::VariableId>,
        keys: &'a mut FxIndexSet<FlowKey>,
        key: FlowKey,
    ) -> Flow {
        let (contract, function_id) = key;
        let function = gcx.hir.function(function_id);
        let mut this = Self {
            gcx,
            tracked,
            keys,
            contract,
            function,
            flow: Flow::default(),
            current: ENTRY,
            return_node: EXIT,
            break_target: None,
            continue_target: None,
            placeholder: None,
            in_assembly: false,
        };
        for _ in [ENTRY, EXIT, REVERT, TRANSACTION_RETURN] {
            this.new_node();
        }
        for &ret in function.returns {
            let span = gcx.hir.variable(ret).span;
            this.occur_at(ENTRY, ret, OccurrenceKind::Declaration, span);
            this.occur_at(EXIT, ret, OccurrenceKind::Return, span);
        }
        for modifier in function.modifiers {
            this.modifier(modifier);
        }
        if let Some(body) = &function.body {
            this.stmts(body.stmts);
        }
        this.connect(this.current, this.return_node);
        this.flow
    }

    fn modifier(&mut self, modifier: &'gcx hir::Modifier<'gcx>) {
        for arg in modifier.args.exprs() {
            self.expr(arg);
        }
        let Some(contract) = self.contract else { return };
        let Some(id) = self.gcx.resolve_modifier_target(contract, modifier) else { return };
        let modifier = self.gcx.hir.function(id);
        if !modifier.kind.is_modifier() {
            return;
        }
        let Some(body) = &modifier.body else { return };
        let entry = self.new_node();
        let exit = self.new_node();
        self.placeholder = Some((entry, exit));
        self.stmts(body.stmts);
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

    fn stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) {
        match stmt.kind {
            StmtKind::DeclSingle(var) => {
                let variable = self.gcx.hir.variable(var);
                self.occur(var, OccurrenceKind::Declaration, variable.span);
                if let Some(init) = variable.initializer {
                    self.expr(init);
                    self.occur(var, OccurrenceKind::Assignment, init.span);
                }
            }
            StmtKind::DeclMulti(vars, init) => {
                for &var in vars.iter().flatten() {
                    self.occur(var, OccurrenceKind::Declaration, self.gcx.hir.variable(var).span);
                }
                self.expr(init);
                for &var in vars.iter().flatten() {
                    self.occur(var, OccurrenceKind::Assignment, init.span);
                }
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => self.stmts(block.stmts),
            StmtKind::AssemblyBlock(block) => {
                let in_assembly = std::mem::replace(&mut self.in_assembly, true);
                self.stmts(block.stmts);
                self.in_assembly = in_assembly;
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
                if let Some(target) = self.break_target {
                    self.jump(target);
                }
            }
            StmtKind::Continue => {
                if let Some(target) = self.continue_target {
                    self.jump(target);
                }
            }
            StmtKind::Loop(block, source) => self.loop_stmt(block, source),
            StmtKind::If(cond, then, else_) => {
                self.expr(cond);
                let before = self.current;
                let then_end = self.branch(before, |this| this.stmt(then));
                let else_end = match else_ {
                    Some(else_) => self.branch(before, |this| this.stmt(else_)),
                    None => before,
                };
                self.merge(&[then_end, else_end]);
            }
            StmtKind::Switch(switch) => {
                self.expr(switch.selector);
                let before = self.current;
                let mut ends = switch
                    .cases
                    .iter()
                    .map(|case| self.branch(before, |this| this.stmts(case.body.stmts)))
                    .collect::<Vec<_>>();
                if switch.cases.last().is_none_or(|case| case.constant.is_some()) {
                    ends.push(before);
                }
                self.merge(&ends);
            }
            StmtKind::Try(try_) => {
                self.expr(&try_.expr);
                let before = self.current;
                let ends = try_
                    .clauses
                    .iter()
                    .map(|clause| self.branch(before, |this| this.stmts(clause.block.stmts)))
                    .collect::<Vec<_>>();
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

    /// Builds a desugared loop.
    ///
    /// Loop conditions are lowered to `if (cond) ... else break;`, which gives the same graph as
    /// solc's condition split.
    fn loop_stmt(&mut self, block: hir::Block<'gcx>, source: LoopSource<'gcx>) {
        let header = self.new_node();
        self.connect(self.current, header);
        self.current = header;
        let after = self.new_node();
        match source {
            LoopSource::For { update, has_cond } => {
                // NOTE: solc splits a `for` loop at its condition even without one, so the code
                // after `for (;;) {}` is reachable.
                if !has_cond {
                    self.current = self.new_node();
                    self.connect(header, self.current);
                    self.connect(header, after);
                }
                let post = self.new_node();
                self.in_loop(after, post, |this| this.stmts(block.stmts));
                self.connect(self.current, post);
                self.current = post;
                if let Some(update) = update {
                    self.stmt(update);
                }
            }
            LoopSource::While => self.in_loop(after, header, |this| this.stmts(block.stmts)),
            // loop {
            //     { <body> }
            //     if (<cond>) continue else break;
            // }
            LoopSource::DoWhile => {
                let Some((check, body)) = block.stmts.split_last() else { return };
                let cond = self.new_node();
                self.in_loop(after, cond, |this| this.stmts(body));
                self.connect(self.current, cond);
                self.current = cond;
                self.in_loop(after, header, |this| this.stmt(check));
            }
        }
        self.connect(self.current, header);
        self.current = after;
    }

    fn expr(&mut self, expr: &'gcx hir::Expr<'gcx>) {
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
                    let before = self.current;
                    let rhs_end = self.branch(before, |this| this.expr(rhs));
                    self.merge(&[rhs_end, before]);
                } else {
                    self.expr(rhs);
                    self.user_operator(expr);
                }
            }
            ExprKind::Unary(_, operand) => {
                self.expr(operand);
                self.user_operator(expr);
            }
            ExprKind::Ternary(cond, true_, false_) => {
                self.expr(cond);
                let before = self.current;
                let true_end = self.branch(before, |this| this.expr(true_));
                let false_end = self.branch(before, |this| this.expr(false_));
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
            ExprKind::Delete(base) | ExprKind::Member(base, _) | ExprKind::Payable(base) => {
                self.expr(base)
            }
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

    /// Visits the target of an assignment.
    fn assign(&mut self, lhs: &'gcx hir::Expr<'gcx>) {
        if let ExprKind::Tuple(exprs) = lhs.kind {
            for expr in exprs.iter().flatten() {
                self.assign(expr);
            }
        } else if let Some((var, span)) = self.var_ref(lhs) {
            self.occur(var, OccurrenceKind::Assignment, span);
        } else {
            self.expr(lhs);
        }
    }

    /// Returns the variable an identifier or inline assembly member access names, and the span
    /// solc reports for it.
    fn var_ref(&self, expr: &hir::Expr<'_>) -> Option<(hir::VariableId, Span)> {
        let (ident, span) = match expr.kind {
            ExprKind::Ident(_) => (expr, expr.span),
            ExprKind::YulMember(base, _) if matches!(base.kind, ExprKind::Ident(_)) => {
                (base, base.span.to(expr.span))
            }
            _ => return None,
        };
        Some((self.gcx.resolved_variable(ident)?, span))
    }

    fn call(&mut self, callee: &'gcx hir::Expr<'gcx>) {
        match self.gcx.resolved_builtin(callee) {
            Some(Builtin::YulReturn | Builtin::YulStop | Builtin::YulSelfdestruct)
                if self.in_assembly =>
            {
                self.jump(TRANSACTION_RETURN)
            }
            Some(Builtin::YulRevert | Builtin::YulInvalid) if self.in_assembly => self.jump(REVERT),
            Some(Builtin::Revert | Builtin::RevertMsg) if !self.in_assembly => self.jump(REVERT),
            _ if !self.in_assembly
                && matches!(callee.kind, ExprKind::Ident(_) | ExprKind::Member(..))
                && let Some(function) = self.gcx.internal_call_target(self.contract, callee) =>
            {
                self.call_node(function)
            }
            _ => {}
        }
    }

    fn user_operator(&mut self, expr: &hir::Expr<'_>) {
        if let Some(function) = self.gcx.user_operator(expr.id) {
            self.call_node(function);
        }
    }

    /// Ends the current node in an internal call.
    fn call_node(&mut self, function: hir::FunctionId) {
        let f = self.gcx.hir.function(function);
        if f.body.is_some() {
            // Like solc's `findScopeContract`.
            let scope = f.contract.map(|base| {
                self.contract
                    .filter(|&contract| {
                        self.gcx.hir.contract(contract).linearized_bases.contains(&base)
                    })
                    .unwrap_or(base)
            });
            let callee = FlowId::new(self.keys.insert_full((scope, function)).0);
            self.flow.nodes[self.current].callee = Some(callee);
        }
        let next = self.new_node();
        self.connect(self.current, next);
        self.current = next;
    }

    fn occur(&mut self, var: hir::VariableId, kind: OccurrenceKind, span: Span) {
        self.occur_at(self.current, var, kind, span);
    }

    fn occur_at(&mut self, node: NodeId, var: hir::VariableId, kind: OccurrenceKind, span: Span) {
        if self.tracked.contains(var) {
            let id = self.flow.occurrences.push(Occurrence { var, kind, span });
            self.flow.nodes[node].occurrences.push(id);
        }
    }

    fn new_node(&mut self) -> NodeId {
        self.flow.nodes.push(Node::default())
    }

    fn connect(&mut self, from: NodeId, to: NodeId) {
        self.flow.nodes[from].exits.push(to);
    }

    /// Jumps to `target` and continues in an unreachable node.
    fn jump(&mut self, target: NodeId) {
        self.connect(self.current, target);
        self.current = self.new_node();
    }

    /// Builds `f` in a new node reached from `from` and returns the node it ends in.
    fn branch(&mut self, from: NodeId, f: impl FnOnce(&mut Self)) -> NodeId {
        self.current = self.new_node();
        self.connect(from, self.current);
        f(self);
        self.current
    }

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
        let break_target = self.break_target.replace(break_target);
        let continue_target = self.continue_target.replace(continue_target);
        f(self);
        self.break_target = break_target;
        self.continue_target = continue_target;
    }
}

/// Buffers reused across flows.
struct Scratch {
    nodes: DenseBitSet<NodeId>,
    worklist: Vec<NodeId>,
    unassigned: MixedBitSet<hir::VariableId>,
    accesses: DenseBitSet<OccurrenceId>,
}

impl Scratch {
    fn new(variables: usize) -> Self {
        Self {
            nodes: DenseBitSet::new_empty(0),
            worklist: Vec::new(),
            unassigned: MixedBitSet::new_empty(variables),
            accesses: DenseBitSet::new_empty(0),
        }
    }
}

/// Returns the flows with a path from entry to exit that does not call a reverting function.
///
/// This is solc's `ControlFlowRevertPruner::findRevertStates`. Flows left unknown can only be
/// blocked on recursion and count as reverting.
fn find_non_reverting(
    flows: &FxHashMap<FlowId, Flow>,
    num_keys: usize,
    scratch: &mut Scratch,
) -> DenseBitSet<FlowId> {
    let mut known = DenseBitSet::new_empty(num_keys);
    let mut non_reverting = DenseBitSet::new_empty(num_keys);
    let mut wake_up = IndexVec::<FlowId, Vec<FlowId>>::from_vec(vec![Vec::new(); num_keys]);
    let mut pending = flows.keys().copied().collect::<Vec<_>>();
    pending.sort_unstable_by(|a, b| b.cmp(a));
    while let Some(item) = pending.pop() {
        if known.contains(item) {
            continue;
        }
        let flow = &flows[&item];
        let mut found_exit = false;
        let mut found_unknown = false;
        let Scratch { nodes: visited, worklist: queue, .. } = scratch;
        visited.clear_to(flow.nodes.len());
        visited.insert(ENTRY);
        queue.push(ENTRY);
        while let Some(node) = queue.pop() {
            found_exit |= node == EXIT;
            if let Some(callee) = flow.nodes[node].callee {
                if !known.contains(callee) {
                    wake_up[callee].push(item);
                    found_unknown = true;
                    continue;
                }
                if !non_reverting.contains(callee) {
                    continue;
                }
            }
            for &exit in &flow.nodes[node].exits {
                if visited.insert(exit) {
                    queue.push(exit);
                }
            }
        }
        if found_exit {
            non_reverting.insert(item);
        } else if found_unknown {
            continue;
        }
        known.insert(item);
        pending.append(&mut wake_up[item]);
    }
    non_reverting
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Report {
    var: hir::VariableId,
    span: Span,
    returned: bool,
}

/// The state at the entry of a node. Accesses also include the node's own.
struct NodeInfo {
    unassigned: MixedBitSet<hir::VariableId>,
    accesses: DenseBitSet<OccurrenceId>,
}

/// Reports the uninitialized accesses that reach the exit of a flow, without revert pruning if
/// `non_reverting` is `None`.
///
/// This is solc's `ControlFlowAnalyzer::checkUninitializedAccess`.
fn analyze(
    flow: &Flow,
    non_reverting: Option<&DenseBitSet<FlowId>>,
    scratch: &mut Scratch,
    mut report: impl FnMut(Report),
) {
    let num_vars = scratch.unassigned.domain_size();
    let new_info = || NodeInfo {
        unassigned: MixedBitSet::new_empty(num_vars),
        accesses: DenseBitSet::new_empty(flow.occurrences.len()),
    };
    let mut infos = IndexVec::<NodeId, Option<NodeInfo>>::from_vec(
        std::iter::repeat_with(|| None).take(flow.nodes.len()).collect(),
    );
    infos[ENTRY] = Some(new_info());
    let Scratch { nodes: queued, worklist, unassigned, accesses } = scratch;
    queued.clear_to(flow.nodes.len());
    queued.insert(ENTRY);
    worklist.push(ENTRY);
    while let Some(node) = worklist.pop() {
        queued.remove(node);
        let info = infos[node].as_mut().unwrap();
        unassigned.clone_from(&info.unassigned);
        for &id in &flow.nodes[node].occurrences {
            let occurrence = flow.occurrences[id];
            match occurrence.kind {
                OccurrenceKind::Declaration => {
                    unassigned.insert(occurrence.var);
                }
                OccurrenceKind::Assignment => {
                    unassigned.remove(occurrence.var);
                }
                OccurrenceKind::Access | OccurrenceKind::Return => {
                    if unassigned.contains(occurrence.var) {
                        info.accesses.insert(id);
                    }
                }
            }
        }

        // A call that always reverts only exits to the revert node, which is never analyzed.
        if let Some(callee) = flow.nodes[node].callee
            && non_reverting.is_some_and(|non_reverting| !non_reverting.contains(callee))
        {
            continue;
        }
        accesses.clone_from(&info.accesses);
        for &exit in &flow.nodes[node].exits {
            let existed = infos[exit].is_some();
            let next = infos[exit].get_or_insert_with(new_info);
            let changed = next.unassigned.union(&*unassigned) | next.accesses.union(&*accesses);
            if (changed || !existed) && queued.insert(exit) {
                worklist.push(exit);
            }
        }
    }

    let Some(exit) = &infos[EXIT] else { return };
    for id in exit.accesses.iter() {
        let occurrence = flow.occurrences[id];
        report(Report {
            var: occurrence.var,
            span: occurrence.span,
            returned: occurrence.kind == OccurrenceKind::Return,
        });
    }
}
