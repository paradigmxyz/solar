//! Cyclic constant detection.
//!
//! This ports solc's `ConstStateVarCircularReferenceChecker` from the `PostTypeChecker`. A
//! constant depends on every constant that an identifier or member access in its value resolves
//! to. One memoized depth-first search over these dependencies records, for each constant, whether
//! it reaches a cycle and the length of the longest chain that starts at it, so the check takes
//! linear time.
//!
//! For each constant in declaration order, error 6161 names the constant and its first dependency
//! that reaches a cycle, which is the dependency solc's fresh search from that constant reports.
//! A constant that depends on itself directly names itself. The first constant whose longest chain
//! has [`MAX_DEPTH`] constants reports error 7380 and ends the check.
//!
//! As in solc, this only runs when the type checker reported no errors, so type errors hide these
//! errors.

use crate::{
    hir::{self, ExprKind, Visit},
    ty::Gcx,
};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxIndexMap},
};
use solar_interface::error_code;
use std::ops::ControlFlow;

/// The length at which the search gives up on a dependency chain.
///
/// Codegen recurses once per constant in the longest chain. The bound keeps codegen in a debug
/// build within the default compiler thread stack, and also bounds the search's recursion.
const MAX_DEPTH: usize = 2048;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

    let mut dependencies = FxIndexMap::default();
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

    let mut search = Search { dependencies: &dependencies, reaches: FxHashMap::default() };
    for (&id, deps) in &dependencies {
        let Some(reach) = search.visit(id, 1) else {
            gcx.dcx()
                .err("variable definition exhausting cyclic dependency validator")
                .code(error_code!(7380))
                .span(gcx.hir.variable(id).span)
                .emit();
            return;
        };
        // Every dependency has been searched by now.
        if reach.cycle
            && let Some(&via) =
                deps.iter().find(|&dep| search.reaches[dep].is_some_and(|reach| reach.cycle))
        {
            let msg = format!(
                "the value of the constant `{}` has a cyclic dependency via `{}`",
                gcx.item_name(id),
                gcx.item_name(via),
            );
            gcx.dcx().err(msg).code(error_code!(6161)).span(gcx.hir.variable(id).span).emit();
        }
    }
}

/// What the search found reachable from a constant.
#[derive(Clone, Copy)]
struct Reach {
    /// Whether a cycle is reachable from the constant.
    cycle: bool,
    /// The number of constants in the longest chain that starts at the constant.
    chain: usize,
}

/// A memoized depth-first search over constant dependencies.
struct Search<'a> {
    dependencies: &'a FxIndexMap<hir::VariableId, Vec<hir::VariableId>>,
    /// The result for each searched constant, or `None` while the constant is on the stack.
    reaches: FxHashMap<hir::VariableId, Option<Reach>>,
}

impl Search<'_> {
    /// Searches from `id`, which is `depth` constants deep. Returns `None` once the search gets
    /// [`MAX_DEPTH`] constants deep or finds a chain of [`MAX_DEPTH`] constants.
    fn visit(&mut self, id: hir::VariableId, depth: usize) -> Option<Reach> {
        match self.reaches.get(&id) {
            // A dependency on the stack closes a cycle.
            Some(None) => return Some(Reach { cycle: true, chain: 0 }),
            Some(&Some(reach)) => return Some(reach),
            None if depth >= MAX_DEPTH => return None,
            None => {}
        }
        self.reaches.insert(id, None);
        let mut reach = Reach { cycle: false, chain: 0 };
        for &dep in self.dependencies.get(&id).into_iter().flatten() {
            let dep_reach = self.visit(dep, depth + 1)?;
            reach.cycle |= dep_reach.cycle;
            reach.chain = reach.chain.max(dep_reach.chain);
        }
        reach.chain += 1;
        if reach.chain >= MAX_DEPTH {
            return None;
        }
        self.reaches.insert(id, Some(reach));
        Some(reach)
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
