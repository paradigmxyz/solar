//! Cyclic constant detection.
//!
//! This ports solc's `ConstStateVarCircularReferenceChecker` from the `PostTypeChecker`. A
//! constant depends on every constant that an identifier or member access in its value resolves
//! to. Each constant, in declaration order, gets its own depth-first search over these
//! dependencies, visited in declaration order. If the search reaches a cycle, error 6161 names the
//! constant and the dependency the cycle was reached through, or the constant itself if it is on
//! the cycle. A search that gets 256 constants deep reports error 7380 and ends the check, like
//! solc's fatal error.
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
use std::ops::ControlFlow;

/// The depth at which solc gives up on a dependency chain.
const MAX_DEPTH: usize = 256;

type Dependencies = FxIndexMap<hir::VariableId, Vec<hir::VariableId>>;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

    let mut dependencies = Dependencies::default();
    for (id, var) in gcx.hir.variables_enumerated() {
        if var.is_constant()
            && let Some(init) = var.initializer
        {
            let mut collector = ConstantReferences { gcx, references: Vec::new() };
            let _ = collector.visit_expr(init);
            if !collector.references.is_empty() {
                collector.references.sort_unstable();
                collector.references.dedup();
                dependencies.insert(id, collector.references);
            }
        }
    }

    for &id in dependencies.keys() {
        match CycleDetector::detect(&dependencies, id, visit_dependencies) {
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
/// reached it, and a constant 256 deep breaks with itself.
fn visit_dependencies(
    dependencies: &Dependencies,
    cd: &mut CycleDetector<&Dependencies, hir::VariableId, hir::VariableId>,
    id: hir::VariableId,
) -> CycleDetectorResult<hir::VariableId, hir::VariableId> {
    // `run` returns without unwinding the depth, so read it first.
    let depth = cd.depth();
    if depth >= MAX_DEPTH {
        return CycleDetectorResult::Break(id);
    }
    for &dep in dependencies.get(&id).into_iter().flatten() {
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
