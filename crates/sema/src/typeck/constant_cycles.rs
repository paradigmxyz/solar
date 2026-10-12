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
    Never,
    cycle::{CycleDetector, CycleDetectorResult},
    map::{FxHashMap, FxHashSet, FxIndexMap},
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

    let heights = heights(&graph);
    let referenced = LazyCell::new(|| graph.values().flatten().copied().collect::<FxHashSet<_>>());
    // The first dependencies of unreferenced constants that a cycle was reached through.
    let mut cyclic_firsts = FxHashSet::default();
    for (&id, dependencies) in &graph {
        if heights[&id] < MAX_DEPTH {
            continue;
        }
        let first = dependencies[0];
        let unreferenced = !referenced.contains(&id);
        let result = if unreferenced && cyclic_firsts.contains(&first) {
            CycleDetectorResult::Cycle(first)
        } else {
            CycleDetector::detect(&graph, id, visit_dependencies)
        };
        match result {
            CycleDetectorResult::Continue => {}
            CycleDetectorResult::Cycle(via) => {
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
            CycleDetectorResult::Break(exhausted) => {
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

/// A constant on the search stack of [`heights`].
struct Frame<'a> {
    id: hir::VariableId,
    /// The dependencies left to search.
    dependencies: std::slice::Iter<'a, hir::VariableId>,
    height: usize,
}

/// Returns, for every constant in `graph`, the number of constants on its longest dependency
/// chain, or at least [`MAX_DEPTH`] if it reaches a cycle.
///
/// A cycle is a chain without end: a constant on the search stack counts as [`MAX_DEPTH`] high,
/// so the constants that depend on it come out higher. The search keeps its own stack, since
/// chains can be arbitrarily long.
fn heights(graph: &Graph) -> FxHashMap<hir::VariableId, usize> {
    let mut heights = FxHashMap::default();
    let mut stack = Vec::<Frame<'_>>::new();
    for (&root, dependencies) in graph {
        if heights.contains_key(&root) {
            continue;
        }
        heights.insert(root, MAX_DEPTH);
        stack.push(Frame { id: root, dependencies: dependencies.iter(), height: 1 });
        while let Some(frame) = stack.last_mut() {
            if let Some(&dep) = frame.dependencies.next() {
                if let Some(&height) = heights.get(&dep) {
                    frame.height = frame.height.max(height + 1);
                } else if let Some(dependencies) = graph.get(&dep) {
                    heights.insert(dep, MAX_DEPTH);
                    stack.push(Frame { id: dep, dependencies: dependencies.iter(), height: 1 });
                } else {
                    frame.height = frame.height.max(2);
                }
            } else {
                let Frame { id, height, .. } = stack.pop().unwrap();
                heights.insert(id, height);
                if let Some(parent) = stack.last_mut() {
                    parent.height = parent.height.max(height + 1);
                }
            }
        }
    }
    heights
}

/// Visits the dependencies of the constant `id`.
///
/// Like solc, a cycle found from the starting constant is reported through the dependency that
/// reached it, and a constant 256 deep breaks with itself.
fn visit_dependencies(
    graph: &Graph,
    cd: &mut CycleDetector<&Graph, hir::VariableId, hir::VariableId>,
    id: hir::VariableId,
) -> CycleDetectorResult<hir::VariableId, hir::VariableId> {
    // `run` returns without unwinding the depth, so read it first.
    let depth = cd.depth();
    if depth >= MAX_DEPTH {
        return CycleDetectorResult::Break(id);
    }
    let Some(dependencies) = graph.get(&id) else { return CycleDetectorResult::Continue };
    for &dep in dependencies {
        match cd.run(dep) {
            CycleDetectorResult::Continue => {}
            CycleDetectorResult::Cycle(_) if depth == 1 => return CycleDetectorResult::Cycle(dep),
            r => return r,
        }
    }
    CycleDetectorResult::Continue
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
