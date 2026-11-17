//! The walk of the HIR that builds the coverage map.
//!
//! A decision is a tree of leaf conditions under `&&`, `||`, and `!`. The walk numbers the tree in
//! source order and assigns three kinds of counters:
//! - `V`, after each leaf condition is evaluated, before the branch on it;
//! - `R`, at the start of the right operand of each `&&` and `||`, which counts how often the left
//!   operand sends evaluation on;
//! - `T`, at the start of the path the decision takes when it holds, or after its value when no
//!   branch consumes it.
//!
//! Those counters give every leaf its true and false counts without a counter on a false edge. The
//! evaluations of a subtree complete `V(a) - R + V(b)` times for `a && b` and `a || b`, as the
//! evaluations of `a` that do not enter `b` and those of `b`. A leaf's true count comes from the
//! top down: the true count of a root is `T`, of the left operand of `&&` is `R`, of the right
//! operand of `&&` the true count of the `&&`, and of the right operand of `||` the true count of
//! the `||` minus that of its left operand, which is the completions of the left operand minus
//! `R`. The false count of a leaf is its completions minus its true count, so a leaf that reverts
//! takes neither side.

use super::{Count, CoverageMap, Record, RecordOwner, Region, RegionKind, Site, SourceCoverage};
use solar_ast::{BinOpKind, UnOpKind};
use solar_data_structures::{Never, index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, ExprKind, LoopSource, StmtKind, Visit},
};
use std::{cmp::Reverse, mem, ops::ControlFlow};

/// Counters of a dry walk, which never reach lowering, start here.
const VIRTUAL_COUNTER: u32 = 1 << 30;

pub(super) fn walk(gcx: Gcx<'_>) -> CoverageMap {
    let mut functions = gcx
        .hir
        .function_ids()
        .filter(|&id| {
            let function = gcx.hir.function(id);
            function.body.is_some() && !function.is_getter()
        })
        .collect::<Vec<_>>();
    functions.sort_by_key(|&id| {
        let function = gcx.hir.function(id);
        (function.source, function.span.lo())
    });
    let mut walker = Walker {
        gcx,
        sources: IndexVec::from_vec(
            (0..gcx.hir.source_ids().len()).map(|_| SourceCoverage::default()).collect(),
        ),
        sites: FxHashMap::default(),
        next: 1,
        dry: false,
        virtual_next: VIRTUAL_COUNTER,
        rec: Rec::default(),
    };
    for &id in &functions {
        walker.function(id);
    }
    let mut contracts = gcx.hir.contract_ids().collect::<Vec<_>>();
    contracts.sort_by_key(|&id| gcx.hir.contract(id).span.lo());
    for id in contracts {
        walker.init(id);
    }
    let mut map = CoverageMap { sources: walker.sources, sites: walker.sites };
    for source in map.sources.iter_mut() {
        source.records.sort_by_key(|record| record.regions.first().map(|region| region.span.lo()));
    }
    map
}

/// A record under construction.
#[derive(Default)]
struct Rec {
    files: Vec<hir::SourceId>,
    first: u32,
    regions: Vec<Region>,
    /// The counts of the code regions that enclose the current statement.
    enclosing: Vec<Count>,
}

/// The tree of a decision.
enum Node<'gcx> {
    /// A leaf condition and the counter after its evaluation.
    Leaf(&'gcx hir::Expr<'gcx>, Count),
    Not(Box<Node<'gcx>>),
    /// `lhs && rhs`, the right operand, and the counter at its start.
    And(Box<Node<'gcx>>, Box<Node<'gcx>>, &'gcx hir::Expr<'gcx>, Count),
    /// `lhs || rhs`, the right operand, and the counter at its start.
    Or(Box<Node<'gcx>>, Box<Node<'gcx>>, &'gcx hir::Expr<'gcx>, Count),
}

impl Node<'_> {
    /// Returns how often the evaluation of the tree completed.
    fn completed(&self) -> Count {
        match self {
            Self::Leaf(_, v) => v.clone(),
            Self::Not(node) => node.completed(),
            Self::And(lhs, rhs, _, r) | Self::Or(lhs, rhs, _, r) => {
                lhs.completed().sub(r.clone()).add(rhs.completed())
            }
        }
    }
}

/// What a decision leaves for its consumer: how often the evaluation of the condition completed,
/// and how often it held.
struct Outcome {
    completed: Count,
    taken: Count,
}

struct Walker<'gcx> {
    gcx: Gcx<'gcx>,
    sources: IndexVec<hir::SourceId, SourceCoverage>,
    sites: FxHashMap<Site, u32>,
    next: u32,
    /// Whether to build the regions without counters, to learn their shape.
    dry: bool,
    virtual_next: u32,
    rec: Rec,
}

impl<'gcx> Walker<'gcx> {
    fn counter(&mut self, site: Site) -> Count {
        if self.dry {
            self.virtual_next += 1;
            return Count::Counter(self.virtual_next);
        }
        let id = self.next;
        self.next += 1;
        let previous = self.sites.insert(site, id);
        debug_assert!(previous.is_none(), "coverage site {site:?} has two counters");
        Count::Counter(id)
    }

    fn begin(&mut self, source: hir::SourceId) {
        self.rec = Rec { files: vec![source], first: self.next, ..Rec::default() };
    }

    fn end(&mut self, owner: RecordOwner, source: hir::SourceId) {
        let rec = mem::take(&mut self.rec);
        let mut regions = rec.regions;
        // A region comes before the regions nested in it.
        regions.sort_by_key(|region| (region.file, region.span.lo(), Reverse(region.span.hi())));
        let counters = [rec.first, self.next - rec.first];
        self.sources[source].records.push(Record { owner, files: rec.files, counters, regions });
    }

    fn region(&mut self, span: Span, kind: RegionKind) {
        self.rec.regions.push(Region { file: 0, span, kind });
    }

    /// Opens a code region unless an enclosing one has the same count.
    fn open(&mut self, span: Span, count: &Count) {
        if self.rec.enclosing.last() != Some(count) {
            self.region(span, RegionKind::Code(count.clone()));
        }
        self.rec.enclosing.push(count.clone());
    }

    fn close(&mut self) {
        self.rec.enclosing.pop();
    }

    fn function(&mut self, id: hir::FunctionId) {
        let function = self.gcx.hir.function(id);
        self.begin(function.source);
        let entry = self.counter(Site::Function(id));
        self.open(function.span, &entry);
        // Modifier and base constructor arguments run before the body.
        for modifier in function.modifiers {
            for arg in modifier.args.exprs() {
                self.expr(arg);
            }
        }
        if let Some(body) = function.body {
            // A constructor runs its state variable initializers first.
            let known = (function.modifiers.is_empty()
                && function.kind != hir::FunctionKind::Constructor)
                .then_some(entry);
            self.stmts(body.stmts, known);
        }
        self.close();
        self.end(RecordOwner::Function(id), function.source);
    }

    /// Walks the code of `id` that runs outside its functions: the initializers of its state
    /// variables, except constants, which lowering folds or inlines at each use, and the base
    /// constructor arguments of its inheritance specifiers.
    fn init(&mut self, id: hir::ContractId) {
        let contract = self.gcx.hir.contract(id);
        self.begin(contract.source);
        for base in contract.bases_args {
            if self.has_regions(|this| {
                for arg in base.args.exprs() {
                    this.expr(arg);
                }
            }) {
                for arg in base.args.exprs() {
                    self.expr(arg);
                }
            }
        }
        for var in contract.variables() {
            let var = self.gcx.hir.variable(var);
            let Some(initializer) = var.initializer else { continue };
            if !var.is_state_variable() || var.is_constant() {
                continue;
            }
            if self.has_regions(|this| this.expr(initializer)) {
                let count = self.counter(Site::Init(Site::key(var)));
                self.open(var.span, &count);
                self.expr(initializer);
                self.close();
            }
        }
        if self.rec.regions.is_empty() {
            self.rec = Rec::default();
        } else {
            self.end(RecordOwner::Init(id), contract.source);
        }
    }

    /// Returns whether `f` adds regions, without leaving any regions or counters behind.
    fn has_regions(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let len = self.rec.regions.len();
        let dry = mem::replace(&mut self.dry, true);
        f(self);
        self.dry = dry;
        let found = self.rec.regions.len() > len;
        self.rec.regions.truncate(len);
        found
    }

    fn stmt_count(&mut self, stmt: &'gcx hir::Stmt<'gcx>, known: Option<Count>) -> Count {
        known.unwrap_or_else(|| self.counter(Site::Stmt(Site::key(stmt))))
    }

    /// Walks statements, the first of which starts with count `known`, when the code before it
    /// counted.
    fn stmts(&mut self, stmts: &'gcx [hir::Stmt<'gcx>], mut known: Option<Count>) {
        for stmt in stmts {
            self.stmt(stmt, known.take());
        }
    }

    /// Walks the statement of a path whose count is `count`.
    fn stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>, known: Option<Count>) {
        match stmt.kind {
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                // A `for` loop with an initializer desugars to `{ <init>; loop { ... } }`, which
                // ends where the loop does.
                if let [init, loop_stmt] = block.stmts
                    && let StmtKind::Loop(_, LoopSource::For { .. }) = loop_stmt.kind
                    && loop_stmt.span.hi() == stmt.span.hi()
                {
                    let count = self.stmt_count(stmt, known);
                    self.open(stmt.span, &count);
                    self.stmt(init, Some(count));
                    self.loop_body(loop_stmt, stmt.span);
                    self.close();
                } else if let Some(count) = known {
                    self.open(stmt.span, &count);
                    self.stmts(block.stmts, Some(count));
                    self.close();
                } else {
                    self.stmts(block.stmts, None);
                }
            }
            StmtKind::AssemblyBlock(block, _) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.stmts(block.stmts, Some(count));
                self.close();
            }
            StmtKind::Loop(..) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.loop_body(stmt, stmt.span);
                self.close();
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                let Outcome { completed, taken } = self.decision(cond, Site::Arm(Site::key(stmt)));
                self.stmt(then_stmt, Some(taken.clone()));
                if let Some(else_stmt) = else_stmt {
                    self.stmt(else_stmt, Some(completed.sub(taken)));
                }
                self.close();
            }
            StmtKind::Switch(switch) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(switch.selector);
                let selected = self.counter(Site::Decision(Site::key(switch)));
                let mut cases = Vec::with_capacity(switch.cases.len());
                for case in switch.cases {
                    let taken = self.counter(Site::Arm(Site::key(case)));
                    let remaining = selected.clone().sub(taken.clone());
                    self.region(case.span, RegionKind::Branch(taken.clone(), remaining));
                    cases.push(taken);
                }
                for (case, taken) in switch.cases.iter().zip(cases) {
                    self.open(case.body.span, &taken);
                    self.stmts(case.body.stmts, Some(taken));
                    self.close();
                }
                self.close();
            }
            StmtKind::Try(try_stmt) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(&try_stmt.expr);
                let mut clauses = Vec::with_capacity(try_stmt.clauses.len());
                for clause in try_stmt.clauses {
                    let taken = self.counter(Site::Arm(Site::key(clause)));
                    let remaining = count.clone().sub(taken.clone());
                    self.region(clause.span, RegionKind::Branch(taken.clone(), remaining));
                    clauses.push(taken);
                }
                for (clause, taken) in try_stmt.clauses.iter().zip(clauses) {
                    self.open(clause.block.span, &taken);
                    self.stmts(clause.block.stmts, Some(taken));
                    self.close();
                }
                self.close();
            }
            StmtKind::DeclSingle(id) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                if let Some(initializer) = self.gcx.hir.variable(id).initializer {
                    self.expr(initializer);
                }
                self.close();
            }
            StmtKind::DeclMulti(_, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Emit(expr)
            | StmtKind::Revert(expr) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(expr);
                self.close();
            }
            StmtKind::Return(expr) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                if let Some(expr) = expr {
                    self.expr(expr);
                }
                self.close();
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Placeholder => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.close();
            }
            StmtKind::Err(_) => {}
        }
    }

    /// Walks the body of a desugared loop whose statement spans `span`.
    fn loop_body(&mut self, loop_stmt: &'gcx hir::Stmt<'gcx>, span: Span) {
        let StmtKind::Loop(block, source) = loop_stmt.kind else { unreachable!() };
        match (source, block.stmts) {
            // loop { if (<cond>) <body> else break; }
            (LoopSource::While | LoopSource::For { .. }, [check])
                if let StmtKind::If(cond, body, Some(exit)) = check.kind
                    && matches!(exit.kind, StmtKind::Break)
                    && (exit.span == span || exit.span == cond.span) =>
            {
                let Outcome { taken, .. } = self.decision(cond, Site::Arm(Site::key(check)));
                self.stmt(body, Some(taken));
            }
            // loop { <body>; if (<cond>) continue else break; }
            (LoopSource::DoWhile, [body, check]) if let StmtKind::If(cond, _, _) = check.kind => {
                self.stmt(body, None);
                let _ = self.decision(cond, Site::Arm(Site::key(check)));
            }
            (_, stmts) => self.stmts(stmts, None),
        }
        if let LoopSource::For { update: Some(update) } = source {
            self.stmt(update, None);
        }
    }

    fn expr(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let _ = Visit::visit_expr(self, expr);
    }

    /// Builds the tree of the decision `expr`.
    fn tree(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Node<'gcx> {
        let expr = expr.peel_parens();
        match expr.kind {
            ExprKind::Unary(op, inner)
                if op.kind == UnOpKind::Not && self.is_logical(inner.peel_parens()) =>
            {
                Node::Not(Box::new(self.tree(inner)))
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind.is_logical() => {
                let lhs = Box::new(self.tree(lhs));
                let r = self.counter(Site::Arm(Site::expr(rhs)));
                let rhs_expr = rhs;
                let rhs = Box::new(self.tree(rhs));
                if op.kind == BinOpKind::And {
                    Node::And(lhs, rhs, rhs_expr, r)
                } else {
                    Node::Or(lhs, rhs, rhs_expr, r)
                }
            }
            _ => self.leaf(expr),
        }
    }

    fn leaf(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Node<'gcx> {
        Node::Leaf(expr, self.counter(Site::Leaf(Site::key(expr))))
    }

    fn is_logical(&self, expr: &hir::Expr<'_>) -> bool {
        match expr.kind {
            ExprKind::Binary(_, op, _) => op.kind.is_logical(),
            ExprKind::Unary(op, inner) => {
                op.kind == UnOpKind::Not && self.is_logical(inner.peel_parens())
            }
            _ => false,
        }
    }

    /// Walks the decision `root`, whose consumer counts the paths it takes at `consumer`.
    fn decision(&mut self, root: &'gcx hir::Expr<'gcx>, consumer: Site) -> Outcome {
        let tree = self.tree(root);
        let taken = self.counter(consumer);
        let completed = tree.completed();
        self.branches(&tree, taken.clone());
        Outcome { completed, taken }
    }

    /// Adds the branch regions of the leaves of `node`, which holds `holds` times.
    fn branches(&mut self, node: &Node<'gcx>, holds: Count) {
        match node {
            Node::Leaf(expr, completed) => {
                self.region(
                    expr.span,
                    RegionKind::Branch(holds.clone(), completed.clone().sub(holds)),
                );
                self.expr(expr);
            }
            Node::Not(node) => self.branches(node, node.completed().sub(holds)),
            Node::And(lhs, rhs, rhs_expr, r) => {
                self.branches(lhs, r.clone());
                self.region(rhs_expr.span, RegionKind::Code(r.clone()));
                self.branches(rhs, holds);
            }
            Node::Or(lhs, rhs, rhs_expr, r) => {
                let lhs_holds = lhs.completed().sub(r.clone());
                self.branches(lhs, lhs_holds.clone());
                self.region(rhs_expr.span, RegionKind::Code(r.clone()));
                self.branches(rhs, holds.sub(lhs_holds));
            }
        }
    }

    fn ternary(
        &mut self,
        cond: &'gcx hir::Expr<'gcx>,
        then_expr: &'gcx hir::Expr<'gcx>,
        else_expr: &'gcx hir::Expr<'gcx>,
    ) {
        let Outcome { completed, taken } = self.decision(cond, Site::Arm(Site::expr(then_expr)));
        self.region(then_expr.span, RegionKind::Code(taken.clone()));
        self.region(else_expr.span, RegionKind::Code(completed.sub(taken)));
        self.expr(then_expr);
        self.expr(else_expr);
    }
}

impl<'gcx> Visit<'gcx> for Walker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // Lowering folds constant value expressions, so their decisions never run.
        if matches!(expr.kind, ExprKind::Binary(..) | ExprKind::Ternary(..) | ExprKind::Call(..))
            && self.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_value_type())
            && self.gcx.const_eval().eval_expr(expr).is_ok()
        {
            return ControlFlow::Continue(());
        }
        match expr.kind {
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.ternary(cond, then_expr, else_expr);
                ControlFlow::Continue(())
            }
            ExprKind::Binary(_, op, _) if op.kind.is_logical() => {
                let _ = self.decision(expr, Site::Value(Site::expr(expr)));
                ControlFlow::Continue(())
            }
            ExprKind::Call(callee, ref args)
                if self.gcx.resolved_builtin(callee) == Some(Builtin::Require)
                    && let Some(condition) = args.exprs().next() =>
            {
                self.visit_expr(callee)?;
                for arg in args.exprs().skip(1) {
                    self.visit_expr(arg)?;
                }
                let _ = self.decision(condition, Site::Arm(Site::expr(condition)));
                ControlFlow::Continue(())
            }
            _ => self.walk_expr(expr),
        }
    }
}
