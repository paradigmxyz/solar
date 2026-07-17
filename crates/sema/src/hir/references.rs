//! Cached reverse references to function declarations.

use super::{
    Expr, ExprId, ExprKind, FunctionId, Hir, Res, VarKind, VariableId, Visit, assignment_pairs,
};
use crate::ty::{CallKind, Gcx, TyKind};
use solar_data_structures::{Never, index::IndexVec, smallvec::SmallVec};
use solar_interface::Span;
use std::ops::ControlFlow;

/// How a source expression references a function declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionReferenceKind {
    /// A statically resolved internal call to the declaration.
    InternalCall,
    /// A statically resolved external call to the declaration.
    ExternalCall,
    /// The declaration is used as a function value.
    Value,
}

/// One reverse reference to a function declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionReference {
    /// The referencing expression.
    pub expr: ExprId,
    /// The expression's source span.
    pub span: Span,
    /// The lexically enclosing function, if any.
    pub owner: Option<FunctionId>,
    /// Whether the reference is a direct call or a function value.
    pub kind: FunctionReferenceKind,
}

/// Possible declarations denoted by an internal function value.
///
/// `may_be_unknown` distinguishes a complete finite target set from known targets joined with an
/// opaque value. Consumers must preserve an opaque-effect branch when it is true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionValueTargets {
    known: SmallVec<[FunctionValueTarget; 4]>,
    may_be_unknown: bool,
}

/// One statically known internal function-value target.
///
/// Bare references to virtual functions retain dynamic dispatch. Explicitly qualified references
/// such as `super.f` and `Base.f` are statically bound, matching Solidity call semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionValueTarget {
    function: FunctionId,
    virtual_dispatch: bool,
}

impl FunctionValueTarget {
    fn from_reference(gcx: Gcx<'_>, expr: &Expr<'_>, function: FunctionId) -> Self {
        let virtual_dispatch = gcx.hir.function(function).virtual_
            && matches!(expr.peel_parens().kind, ExprKind::Ident(_));
        Self { function, virtual_dispatch }
    }

    /// Returns the declaration denoted at the value's source site.
    pub const fn function(self) -> FunctionId {
        self.function
    }

    /// Returns whether the target must be resolved in the runtime dispatch contract.
    pub const fn requires_virtual_dispatch(self) -> bool {
        self.virtual_dispatch
    }
}

impl FunctionValueTargets {
    fn complete() -> Self {
        Self { known: SmallVec::new(), may_be_unknown: false }
    }

    pub(crate) fn unknown() -> Self {
        Self { known: SmallVec::new(), may_be_unknown: true }
    }

    fn singleton(target: FunctionValueTarget) -> Self {
        let mut targets = Self::complete();
        targets.known.push(target);
        targets
    }

    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for &target in &other.known {
            if !self.known.contains(&target) {
                self.known.push(target);
                changed = true;
            }
        }
        if other.may_be_unknown && !self.may_be_unknown {
            self.may_be_unknown = true;
            changed = true;
        }
        changed
    }

    /// Returns the known target declarations.
    pub fn known(&self) -> &[FunctionValueTarget] {
        &self.known
    }

    /// Returns whether the function value may also have an unresolved target.
    pub fn may_be_unknown(&self) -> bool {
        self.may_be_unknown
    }
}

/// Compilation-wide reverse-reference index for function declarations.
///
/// This is computed once per [`Gcx`] and is the HIR analogue of rustc's indexed use queries.
/// Direct calls use type-checked callee resolution; function-value references retain every HIR
/// resolution candidate so error recovery remains conservative.
#[derive(Debug)]
pub struct FunctionReferenceIndex<'hir> {
    references: IndexVec<FunctionId, &'hir [FunctionReference]>,
    value_targets: IndexVec<VariableId, FunctionValueTargets>,
}

impl FunctionReferenceIndex<'_> {
    /// Returns all references to `function`.
    pub fn references_to(&self, function: FunctionId) -> &[FunctionReference] {
        self.references[function]
    }

    /// Returns whether `function` is referenced anywhere in the compilation.
    pub fn is_referenced(&self, function: FunctionId) -> bool {
        !self.references_to(function).is_empty()
    }

    /// Returns whether `function` is used by internal execution.
    ///
    /// External calls do not count because they remain valid if a public declaration is narrowed
    /// to external visibility.
    pub fn is_internally_referenced(&self, function: FunctionId) -> bool {
        self.references_to(function).iter().any(|reference| {
            matches!(
                reference.kind,
                FunctionReferenceKind::InternalCall | FunctionReferenceKind::Value
            )
        })
    }

    /// Returns the declarations which `expr` may denote as an internal function value.
    ///
    /// This follows declaration initializers, assignments, ternaries, and arguments passed to
    /// statically resolved internal function parameters. The result is deliberately a may-set.
    pub fn possible_value_targets(&self, gcx: Gcx<'_>, expr: &Expr<'_>) -> FunctionValueTargets {
        collect_value_targets(gcx, expr, &self.value_targets)
    }
}

pub(crate) fn build_function_reference_index<'hir>(gcx: Gcx<'hir>) -> FunctionReferenceIndex<'hir> {
    let mut visitor = ReferenceCollector {
        gcx,
        references: vec![SmallVec::new(); gcx.hir.function_ids().len()],
        value_bindings: Vec::new(),
        owner: None,
        inside_resolved_callee: false,
    };
    for source in gcx.hir.source_ids() {
        let _ = visitor.visit_nested_source(source);
    }

    let mut references = IndexVec::<FunctionId, &'hir [FunctionReference]>::new();
    for entries in visitor.references {
        let entries: &'hir [FunctionReference] = gcx.bump().alloc_slice_copy(&entries);
        references.push(entries);
    }
    let mut targets = IndexVec::<VariableId, FunctionValueTargets>::new();
    for variable in gcx.hir.variable_ids() {
        let has_binding = visitor.value_bindings.iter().any(|(target, _)| *target == variable);
        let is_local = gcx.hir.variable(variable).kind == VarKind::Statement;
        targets.push(if has_binding && is_local {
            FunctionValueTargets::complete()
        } else {
            FunctionValueTargets::unknown()
        });
    }
    loop {
        let mut changed = false;
        for &(variable, value) in &visitor.value_bindings {
            let resolved = value.map_or_else(FunctionValueTargets::unknown, |value| {
                collect_value_targets(gcx, value, &targets)
            });
            changed |= targets[variable].join(&resolved);
        }
        if !changed {
            break;
        }
    }
    FunctionReferenceIndex { references, value_targets: targets }
}

struct ReferenceCollector<'hir> {
    gcx: Gcx<'hir>,
    references: Vec<SmallVec<[FunctionReference; 2]>>,
    value_bindings: Vec<(VariableId, Option<&'hir Expr<'hir>>)>,
    owner: Option<FunctionId>,
    inside_resolved_callee: bool,
}

impl ReferenceCollector<'_> {
    fn record(&mut self, function: FunctionId, expr: &Expr<'_>, kind: FunctionReferenceKind) {
        let reference =
            FunctionReference { expr: expr.id, span: expr.span, owner: self.owner, kind };
        let references = &mut self.references[function.index()];
        if !references.contains(&reference) {
            references.push(reference);
        }
    }
}

impl<'hir> Visit<'hir> for ReferenceCollector<'hir> {
    type BreakValue = Never;

    fn hir(&self) -> &'hir Hir<'hir> {
        &self.gcx.hir
    }

    fn visit_nested_function(&mut self, function: FunctionId) -> ControlFlow<Self::BreakValue> {
        let previous = self.owner.replace(function);
        let result = self.visit_function(self.gcx.hir.function(function));
        self.owner = previous;
        result
    }

    fn visit_nested_var(&mut self, variable: VariableId) -> ControlFlow<Self::BreakValue> {
        if let Some(initializer) = self.gcx.hir.variable(variable).initializer {
            self.value_bindings.push((variable, Some(initializer)));
        }
        self.visit_var(self.gcx.hir.variable(variable))
    }

    fn visit_expr(&mut self, expr: &'hir Expr<'hir>) -> ControlFlow<Self::BreakValue> {
        match &expr.kind {
            ExprKind::Call(callee, args, options) => {
                let info = self.gcx.call_info(expr);
                let target = info.and_then(|info| info.function());
                if let Some(function) = target {
                    let kind = if info.is_some_and(|info| info.kind() == CallKind::Internal) {
                        FunctionReferenceKind::InternalCall
                    } else {
                        FunctionReferenceKind::ExternalCall
                    };
                    self.record(function, expr, kind);
                    if kind == FunctionReferenceKind::InternalCall {
                        for (index, &parameter) in
                            self.gcx.hir.function(function).parameters.iter().enumerate()
                        {
                            if let Some(argument) = self.gcx.call_arg_for_param(expr, index) {
                                self.value_bindings.push((parameter, Some(argument)));
                            }
                        }
                    }
                }

                let previous = self.inside_resolved_callee;
                self.inside_resolved_callee = target.is_some();
                self.visit_expr(callee)?;
                self.inside_resolved_callee = previous;
                if let Some(options) = options {
                    for option in options.args {
                        self.visit_expr(&option.value)?;
                    }
                }
                self.visit_call_args(args)
            }
            ExprKind::Ident(resolutions) => {
                if !self.inside_resolved_callee {
                    if let Some(function) =
                        self.gcx.type_of_expr(expr.id).and_then(|ty| match ty.kind {
                            TyKind::Fn(function) => function.function_id,
                            _ => None,
                        })
                    {
                        self.record(function, expr, FunctionReferenceKind::Value);
                    } else {
                        for function in resolutions.iter().filter_map(Res::as_function) {
                            self.record(function, expr, FunctionReferenceKind::Value);
                        }
                    }
                }
                ControlFlow::Continue(())
            }
            ExprKind::Member(..) if !self.inside_resolved_callee => {
                if let Some(function) =
                    self.gcx.type_of_expr(expr.id).and_then(|ty| match ty.kind {
                        TyKind::Fn(function) => function.function_id,
                        _ => None,
                    })
                {
                    self.record(function, expr, FunctionReferenceKind::Value);
                }
                self.walk_expr(expr)
            }
            ExprKind::Assign(target, None, value) => {
                for assignment in assignment_pairs(target, Some(value)) {
                    if let Some(variable) = assignment.target.as_variable() {
                        self.value_bindings.push((variable, assignment.value));
                    }
                }
                self.walk_expr(expr)
            }
            ExprKind::Delete(target) => {
                if let Some(variable) = target.as_variable() {
                    self.value_bindings.push((variable, None));
                }
                self.walk_expr(expr)
            }
            _ => self.walk_expr(expr),
        }
    }
}

fn collect_value_targets(
    gcx: Gcx<'_>,
    expr: &Expr<'_>,
    variables: &IndexVec<VariableId, FunctionValueTargets>,
) -> FunctionValueTargets {
    let expr = expr.peel_parens();
    if let Some(variable) = expr.as_variable() {
        return variables[variable].clone();
    }
    if matches!(expr.kind, ExprKind::Ident(_) | ExprKind::Member(..))
        && let Some(function) = gcx.type_of_expr(expr.id).and_then(|ty| match ty.kind {
            TyKind::Fn(function) => function.function_id,
            _ => None,
        })
    {
        return FunctionValueTargets::singleton(FunctionValueTarget::from_reference(
            gcx, expr, function,
        ));
    }

    match &expr.kind {
        ExprKind::Ident(resolutions) => {
            let mut targets = FunctionValueTargets::unknown();
            for function in resolutions.iter().filter_map(Res::as_function) {
                let target = FunctionValueTarget::from_reference(gcx, expr, function);
                if !targets.known.contains(&target) {
                    targets.known.push(target);
                }
            }
            targets
        }
        ExprKind::Assign(_, None, value) => collect_value_targets(gcx, value, variables),
        ExprKind::Ternary(_, then_expr, else_expr) => {
            let mut targets = collect_value_targets(gcx, then_expr, variables);
            _ = targets.join(&collect_value_targets(gcx, else_expr, variables));
            targets
        }
        ExprKind::Tuple(values) => {
            let mut targets = FunctionValueTargets::complete();
            for value in values.iter().flatten() {
                _ = targets.join(&collect_value_targets(gcx, value, variables));
            }
            targets
        }
        _ => FunctionValueTargets::unknown(),
    }
}
