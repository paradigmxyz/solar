//! Source coverage instrumentation, enabled with `-Zinstrument-coverage`.
//!
//! Like LLVM's source-based coverage, instrumentation places counters in the code and maps each
//! source region to an expression over them. A walk of the HIR, before any function is lowered,
//! assigns counters to every function, statement, and decision of every source, so the map also
//! covers functions that no bytecode contains. Lowering then emits a `coverage` MIR instruction for
//! a counter wherever control reaches its site; the backend emits it as `PUSHn <counter>;
//! COVERAGE`. Counters start at 1 and are unique within the compilation, so a probe names its
//! region without a table of program counters.
//!
//! A counter's sites are:
//! - the entry of a function, modifier, or Yul function body;
//! - the start of a statement written in the source, but not of a block or of a statement that
//!   lowering desugared, such as a loop's `if (cond) ... else break`;
//! - the point after a decision's condition is evaluated, right before it branches, and the start
//!   of the path taken when the condition holds. The other path of a two-way decision is the
//!   difference of the two, like an LLVM counter expression, which needs no counter on the edge and
//!   thus no extra jump. A `require` counts the path past its check;
//! - the start of each clause of a `try` statement and of each case of a `switch`.
//!
//! A counter placed after the condition, rather than the statement's counter, is the minuend so a
//! condition that reverts takes neither path.
//!
//! Decisions are `if` statements, loop conditions, `?:`, the right operands of `&&` and `||`,
//! `require` calls, `try` statements, and Yul `switch` statements. Lowering folds a constant
//! expression without evaluating its parts, so the walk skips constant expressions too.
//!
//! Sites are keyed by the address of their HIR node, which lowering reaches through the same
//! arena references. A counter whose site lowering never reaches stays at zero, as for code that
//! is never generated.

use solar_ast::BinOpKind;
use solar_data_structures::{Never, index::IndexVec, map::FxHashMap};
use solar_interface::Span;
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, ExprKind, LoopSource, StmtKind, Visit},
};
use std::ops::ControlFlow;

/// An execution count, given by counter hits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageCount {
    /// The hits of one counter.
    Counter(u32),
    /// The hits of the first counter minus the hits of the others.
    Difference(Box<[u32]>),
}

/// The coverage regions of one function, modifier, or Yul function.
#[derive(Debug)]
pub struct FunctionCoverage {
    /// The function.
    pub id: hir::FunctionId,
    /// How many times the function was entered.
    pub count: CoverageCount,
    /// The statements of the function body, in source order.
    pub statements: Vec<StatementCoverage>,
    /// The decisions of the function body, in source order.
    pub branches: Vec<BranchCoverage>,
}

/// The coverage region of one statement.
#[derive(Debug)]
pub struct StatementCoverage {
    /// The statement.
    pub span: Span,
    /// How many times the statement started executing.
    pub count: CoverageCount,
}

/// The coverage regions of one decision.
#[derive(Debug)]
pub struct BranchCoverage {
    /// The decision.
    pub span: Span,
    /// How many times each path was taken: for a two-way decision, the path taken when its
    /// condition holds first; for a `try` statement its clauses, and for a `switch` its cases, in
    /// source order, followed by the implicit default case if there is none.
    pub paths: Vec<CoverageCount>,
}

/// The coverage map of a compilation.
#[derive(Debug, Default)]
pub struct CoverageMap {
    sources: IndexVec<hir::SourceId, Vec<FunctionCoverage>>,
    sites: FxHashMap<Site, u32>,
}

impl CoverageMap {
    fn new(gcx: Gcx<'_>) -> Self {
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
            map: Self {
                sources: IndexVec::from_vec(
                    (0..gcx.hir.source_ids().len()).map(|_| Vec::new()).collect(),
                ),
                sites: FxHashMap::default(),
            },
            statements: Vec::new(),
            branches: Vec::new(),
        };
        for id in functions {
            walker.function(id);
        }
        walker.map
    }

    /// Returns the coverage regions of the functions defined in `source`, in source order.
    pub fn source(&self, source: hir::SourceId) -> &[FunctionCoverage] {
        &self.sources[source]
    }

    /// Returns the counter of `site`, if it has one.
    pub(crate) fn counter(&self, site: Site) -> Option<u32> {
        self.sites.get(&site).copied()
    }
}

/// Returns the coverage map of the compilation when `-Zinstrument-coverage` is set.
pub fn coverage_map(gcx: Gcx<'_>) -> Option<&CoverageMap> {
    gcx.sess.opts.unstable.instrument_coverage.then(|| {
        gcx.codegen_state::<crate::backend::evm::CodegenState>()
            .coverage
            .get_or_init(|| CoverageMap::new(gcx))
    })
}

/// A place where lowering emits a counter, keyed by the address of its HIR node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Site {
    /// The entry of a function body.
    Function(hir::FunctionId),
    /// The start of a statement.
    Stmt(usize),
    /// After the condition of a decision, keyed by the `if` statement, the `switch` statement, the
    /// true arm of a `?:`, the right operand of `&&` or `||`, or the condition of a `require`.
    Decision(usize),
    /// The start of the path a decision takes, keyed like [`Site::Decision`], or a `try` clause or
    /// a `switch` case.
    Taken(usize),
}

impl Site {
    /// Returns the address key of `node`.
    pub(crate) fn key<T>(node: &T) -> usize {
        std::ptr::from_ref(node) as usize
    }
}

struct Walker<'gcx> {
    gcx: Gcx<'gcx>,
    map: CoverageMap,
    statements: Vec<StatementCoverage>,
    branches: Vec<BranchCoverage>,
}

impl<'gcx> Walker<'gcx> {
    fn counter(&mut self, site: Site) -> u32 {
        let counter = self.map.sites.len() as u32 + 1;
        let previous = self.map.sites.insert(site, counter);
        debug_assert!(previous.is_none(), "coverage site {site:?} has two counters");
        counter
    }

    fn function(&mut self, id: hir::FunctionId) {
        let function = self.gcx.hir.function(id);
        let count = CoverageCount::Counter(self.counter(Site::Function(id)));
        if let Some(body) = function.body {
            self.stmts(body.stmts);
        }
        let mut statements = std::mem::take(&mut self.statements);
        let mut branches = std::mem::take(&mut self.branches);
        statements.sort_by_key(|statement| statement.span.lo());
        branches.sort_by_key(|branch| branch.span.lo());
        self.map.sources[function.source].push(FunctionCoverage {
            id,
            count,
            statements,
            branches,
        });
    }

    fn statement(&mut self, stmt: &'gcx hir::Stmt<'gcx>) {
        let count = CoverageCount::Counter(self.counter(Site::Stmt(Site::key(stmt))));
        self.statements.push(StatementCoverage { span: stmt.span, count });
    }

    /// Adds a two-way decision keyed by `node`, whose first path starts at the taken site.
    fn decision<T>(&mut self, node: &T, span: Span, taken_first: bool) {
        let key = Site::key(node);
        let decision = self.counter(Site::Decision(key));
        let taken = self.counter(Site::Taken(key));
        let mut paths =
            [CoverageCount::Counter(taken), CoverageCount::Difference(Box::new([decision, taken]))];
        if !taken_first {
            paths.reverse();
        }
        self.branches.push(BranchCoverage { span, paths: paths.into() });
    }

    fn stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) {
        match stmt.kind {
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                // A `for` loop with an initializer desugars to `{ <init>; loop { ... } }`, which
                // ends where the loop does.
                if let [init, loop_stmt] = block.stmts
                    && let StmtKind::Loop(_, LoopSource::For { .. }) = loop_stmt.kind
                    && loop_stmt.span.hi() == stmt.span.hi()
                {
                    self.statement(stmt);
                    self.stmt(init);
                    self.loop_body(loop_stmt, stmt.span);
                } else {
                    self.stmts(block.stmts);
                }
            }
            StmtKind::AssemblyBlock(block, _) => {
                self.statement(stmt);
                self.stmts(block.stmts);
            }
            StmtKind::Loop(..) => {
                self.statement(stmt);
                self.loop_body(stmt, stmt.span);
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.statement(stmt);
                self.expr(cond);
                self.decision(stmt, stmt.span, true);
                self.stmt(then_stmt);
                if let Some(else_stmt) = else_stmt {
                    self.stmt(else_stmt);
                }
            }
            StmtKind::Switch(switch) => {
                self.statement(stmt);
                self.expr(switch.selector);
                let decision = self.counter(Site::Decision(Site::key(switch)));
                let mut paths = Vec::with_capacity(switch.cases.len() + 1);
                let mut cases = vec![decision];
                for case in switch.cases {
                    let counter = self.counter(Site::Taken(Site::key(case)));
                    paths.push(CoverageCount::Counter(counter));
                    cases.push(counter);
                }
                if switch.cases.last().is_none_or(|case| case.constant.is_some()) {
                    paths.push(CoverageCount::Difference(cases.into()));
                }
                self.branches.push(BranchCoverage { span: stmt.span, paths });
                for case in switch.cases {
                    self.stmts(case.body.stmts);
                }
            }
            StmtKind::Try(try_stmt) => {
                self.statement(stmt);
                self.expr(&try_stmt.expr);
                let paths = try_stmt
                    .clauses
                    .iter()
                    .map(|clause| {
                        CoverageCount::Counter(self.counter(Site::Taken(Site::key(clause))))
                    })
                    .collect();
                self.branches.push(BranchCoverage { span: stmt.span, paths });
                for clause in try_stmt.clauses {
                    self.stmts(clause.block.stmts);
                }
            }
            StmtKind::DeclSingle(id) => {
                self.statement(stmt);
                if let Some(initializer) = self.gcx.hir.variable(id).initializer {
                    self.expr(initializer);
                }
            }
            StmtKind::DeclMulti(_, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Emit(expr)
            | StmtKind::Revert(expr) => {
                self.statement(stmt);
                self.expr(expr);
            }
            StmtKind::Return(expr) => {
                self.statement(stmt);
                if let Some(expr) = expr {
                    self.expr(expr);
                }
            }
            StmtKind::Break | StmtKind::Continue => self.statement(stmt),
            StmtKind::Placeholder | StmtKind::Err(_) => {}
        }
    }

    fn stmts(&mut self, stmts: &'gcx [hir::Stmt<'gcx>]) {
        for stmt in stmts {
            self.stmt(stmt);
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
                self.expr(cond);
                self.decision(check, span, true);
                self.stmt(body);
            }
            // loop { <body>; if (<cond>) continue else break; }
            (LoopSource::DoWhile, [body, check]) if let StmtKind::If(cond, _, _) = check.kind => {
                self.stmt(body);
                self.expr(cond);
                self.decision(check, span, true);
            }
            (_, stmts) => self.stmts(stmts),
        }
        if let LoopSource::For { update: Some(update) } = source {
            match update.kind {
                StmtKind::Expr(expr) => {
                    self.statement(update);
                    self.expr(expr);
                }
                _ => self.stmt(update),
            }
        }
    }

    fn expr(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let _ = Visit::visit_expr(self, expr);
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
                self.visit_expr(cond)?;
                self.decision(then_expr, expr.span, true);
                self.visit_expr(then_expr)?;
                self.visit_expr(else_expr)
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind.is_logical() => {
                self.visit_expr(lhs)?;
                // `a && b` evaluates `b` when `a` holds, and `a || b` when it does not.
                self.decision(rhs, expr.span, op.kind == BinOpKind::And);
                self.visit_expr(rhs)
            }
            ExprKind::Call(callee, ref args)
                if self.gcx.resolved_builtin(callee) == Some(Builtin::Require)
                    && let Some(condition) = args.exprs().next() =>
            {
                self.walk_expr(expr)?;
                self.decision(condition, expr.span, true);
                ControlFlow::Continue(())
            }
            _ => self.walk_expr(expr),
        }
    }
}
