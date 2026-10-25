//! Cyclic constant detection.
//!
//! This ports solc's `ConstStateVarCircularReferenceChecker` from the `PostTypeChecker`. A
//! constant depends on every constant that an identifier or member access in its value resolves
//! to. Each constant, in declaration order, starts a depth-first search over these dependencies,
//! visited in declaration order. If the search reaches a cycle, error 6161 names the constant and
//! its dependency that the cycle was reached through. A search that gets 256 constants deep
//! reports error 7380 and ends the check, like solc's fatal error.
//!
//! solc starts each search afresh, which takes quadratic time when many constants share
//! dependencies. A search can only report an error if its constant reaches a cycle or a chain of
//! 256 constants, so one linear pass first finds the constants that do, and only those run solc's
//! search. A constant that no constant references cannot be reached from its dependencies, so
//! the search through its first dependency does not depend on it, and its result is shared with
//! the other such constants. Valid programs take linear time; the searches for other erroneous
//! constants can take quadratic time, as in solc.
//!
//! As in solc, this only runs when the type checker reported no errors, so type errors and the
//! inline assembly error for circular constants (3558) hide these errors. Errors that solc reports
//! in its `PostTypeChecker`, such as variables in interfaces (8274), do not.

use crate::{
    hir::{self, ExprKind, Visit},
    ty::Gcx,
};
use solar_data_structures::{
    Never, graph,
    index::IndexVec,
    map::{FxHashSet, FxIndexMap},
};
use solar_interface::error_code;
use std::{cell::LazyCell, ops::ControlFlow};

/// The depth at which solc gives up on a dependency chain.
pub(super) const MAX_DEPTH: usize = 256;

/// The referenced constants of each constant that references any, in declaration order.
type Graph = FxIndexMap<hir::VariableId, Vec<hir::VariableId>>;

pub(super) fn check(gcx: Gcx<'_>) {
    let mut graph = Graph::default();
    for (id, var) in gcx.hir.variables_enumerated() {
        if var.is_constant()
            && let Some(init) = var.initializer
        {
            let mut collector = ConstantReferences { gcx, references: Vec::new() };
            let _ = collector.visit_expr(init);
            if !collector.references.is_empty() {
                collector.references.sort_unstable();
                collector.references.dedup();
                graph.insert(id, collector.references);
            }
        }
    }

    let heights = heights(gcx, &graph);
    let referenced = LazyCell::new(|| graph.values().flatten().copied().collect::<FxHashSet<_>>());
    // The first dependencies of unreferenced constants that a cycle was reached through.
    let mut cyclic_firsts = FxHashSet::default();
    for (&id, dependencies) in &graph {
        if heights[id] < MAX_DEPTH {
            continue;
        }
        let first = dependencies[0];
        let unreferenced = !referenced.contains(&id);
        let result = if unreferenced && cyclic_firsts.contains(&first) {
            Found::Cycle(first)
        } else {
            Search::new(&graph).run(id, 1)
        };
        match result {
            Found::Nothing => {}
            Found::Cycle(via) => {
                if unreferenced && via == first {
                    cyclic_firsts.insert(first);
                }
                let msg = format!(
                    "the value of the constant `{}` has a cyclic dependency via `{}`",
                    gcx.item_name(id),
                    gcx.item_name(via),
                );
                gcx.dcx().err(msg).code(error_code!(6161)).span(gcx.hir.variable(id).span).emit();
            }
            Found::Exhausted(exhausted) => {
                gcx.dcx()
                    .err("variable definition exhausting cyclic dependency validator")
                    .code(error_code!(7380))
                    .span(gcx.hir.variable(exhausted).span)
                    .emit();
                return;
            }
        }
    }
}

/// Returns, for every variable, the number of constants on its longest dependency chain, or at
/// least [`MAX_DEPTH`] if it reaches a cycle.
fn heights(gcx: Gcx<'_>, graph: &Graph) -> IndexVec<hir::VariableId, usize> {
    let dependencies = |id| graph.get(&id).into_iter().flatten().copied();
    let len = gcx.hir.variable_ids().len();
    let mut heights = IndexVec::from_vec(vec![1; len]);
    // Components come in reverse topological order, so dependencies come first.
    for scc in graph::sccs(len, dependencies) {
        let id = scc[0];
        if scc.len() > 1 || dependencies(id).any(|dep| dep == id) {
            for &id in &scc {
                heights[id] = MAX_DEPTH;
            }
        } else {
            heights[id] = dependencies(id).map(|dep| heights[dep] + 1).max().unwrap_or(1);
        }
    }
    heights
}

/// What a search from one constant found.
enum Found {
    Nothing,
    /// A cycle, reached through this dependency of the starting constant.
    Cycle(hir::VariableId),
    /// This constant, [`MAX_DEPTH`] constants deep.
    Exhausted(hir::VariableId),
}

/// solc's `CycleDetector` search from one constant.
///
/// The recursion is bounded by [`MAX_DEPTH`].
struct Search<'a> {
    graph: &'a Graph,
    processing: FxHashSet<hir::VariableId>,
    processed: FxHashSet<hir::VariableId>,
}

impl<'a> Search<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self { graph, processing: FxHashSet::default(), processed: FxHashSet::default() }
    }

    /// Visits the dependencies of the constant `id`, which is `depth` constants deep, in
    /// declaration order.
    ///
    /// Like solc, a cycle found from the starting constant is reported through the dependency
    /// that reached it, and a constant [`MAX_DEPTH`] deep ends the search with itself.
    fn run(&mut self, id: hir::VariableId, depth: usize) -> Found {
        if self.processed.contains(&id) {
            return Found::Nothing;
        }
        if !self.processing.insert(id) {
            return Found::Cycle(id);
        }
        if depth >= MAX_DEPTH {
            return Found::Exhausted(id);
        }
        for &dep in self.graph.get(&id).into_iter().flatten() {
            match self.run(dep, depth + 1) {
                Found::Nothing => {}
                Found::Cycle(_) if depth == 1 => return Found::Cycle(dep),
                found => return found,
            }
        }
        self.processing.remove(&id);
        self.processed.insert(id);
        Found::Nothing
    }
}

/// Collects the constants referenced by an expression.
struct ConstantReferences<'gcx> {
    gcx: Gcx<'gcx>,
    references: Vec<hir::VariableId>,
}

impl<'gcx> Visit<'gcx> for ConstantReferences<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        if matches!(expr.kind, ExprKind::Ident(_) | ExprKind::Member(..))
            && let Some(id) = self.gcx.resolved_variable(expr)
            && self.gcx.hir.variable(id).is_constant()
        {
            self.references.push(id);
        }
        self.walk_expr(expr)
    }
}
