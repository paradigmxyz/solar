//! Cyclic constant detection.
//!
//! This ports solc's `ConstStateVarCircularReferenceChecker` from the `PostTypeChecker`. A
//! constant depends on every constant that an identifier or member access in its value resolves
//! to. Each constant, in declaration order, starts a depth-first search over these dependencies,
//! visited in declaration order. If the search reaches a cycle, error 6161 names the constant and
//! the dependency the cycle was reached through, or the constant itself if it is on the cycle. A
//! search that gets 256 constants deep reports error 7380 and ends the check, like solc's fatal
//! error.
//!
//! solc starts each search afresh, which takes quadratic time when many constants share
//! dependencies. Here later searches skip the constants that earlier ones finished, so the whole
//! check takes linear time. A finished constant reaches no cycle, so this keeps every 6161 error.
//! To keep 7380 on long chains, each finished constant records the length of its longest
//! dependency chain, and a search that reaches it at a depth where that chain would get 256 deep
//! reports 7380 at the constant there. solc's searches only follow the chains that their
//! visiting order happens to explore, so we also report 7380 for some graphs that solc accepts.
//!
//! As in solc, this only runs when the type checker reported no errors, so type errors and the
//! inline assembly error for circular constants (3558) hide these errors.

use crate::{
    hir::{self, ExprKind, Visit},
    ty::Gcx,
};
use solar_data_structures::{
    Never,
    cycle::{CycleDetector, CycleDetectorResult},
    map::FxIndexMap,
};
use solar_interface::error_code;
use std::{cell::Cell, ops::ControlFlow};

/// The depth at which solc gives up on a dependency chain.
pub(super) const MAX_DEPTH: usize = 256;

type Graph = FxIndexMap<hir::VariableId, Constant>;

/// A constant that references other constants.
struct Constant {
    /// The referenced constants, in declaration order.
    dependencies: Vec<hir::VariableId>,
    /// The number of constants on the longest dependency chain from this one, set once its search
    /// finishes.
    height: Cell<usize>,
}

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

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
                let constant =
                    Constant { dependencies: collector.references, height: Cell::new(0) };
                graph.insert(id, constant);
            }
        }
    }

    let mut cd = CycleDetector::new(&graph, visit_dependencies);
    for &id in graph.keys() {
        match cd.run_root(id) {
            CycleDetectorResult::Continue => {}
            CycleDetectorResult::Cycle(via) => {
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

/// Visits the dependencies of the constant `id`.
///
/// Like solc, a cycle found from the starting constant is reported through the dependency that
/// reached it, and a constant 256 deep breaks with itself. A finished dependency breaks if its
/// longest chain reaches that depth.
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
    let Some(constant) = graph.get(&id) else { return CycleDetectorResult::Continue };
    let mut height = 1;
    for &dep in &constant.dependencies {
        match cd.run(dep) {
            CycleDetectorResult::Continue => {}
            CycleDetectorResult::Cycle(_) if depth == 1 => return CycleDetectorResult::Cycle(dep),
            r => return r,
        }
        let dep_height = height_of(graph, dep);
        if depth + dep_height >= MAX_DEPTH {
            return CycleDetectorResult::Break(exhausted_from(graph, dep, depth + 1));
        }
        height = height.max(dep_height + 1);
    }
    constant.height.set(height);
    CycleDetectorResult::Continue
}

/// Returns the number of constants on the longest dependency chain from the finished constant
/// `id`.
fn height_of(graph: &Graph, id: hir::VariableId) -> usize {
    graph.get(&id).map_or(1, |constant| constant.height.get())
}

/// Returns the first constant at [`MAX_DEPTH`] on the chains from the finished constant `id` at
/// `depth`.
fn exhausted_from(graph: &Graph, mut id: hir::VariableId, mut depth: usize) -> hir::VariableId {
    while depth < MAX_DEPTH {
        id = *graph[&id]
            .dependencies
            .iter()
            .find(|&&dep| depth + height_of(graph, dep) >= MAX_DEPTH)
            .unwrap();
        depth += 1;
    }
    id
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
