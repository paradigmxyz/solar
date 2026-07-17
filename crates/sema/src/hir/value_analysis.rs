//! Effective-body adapter for analyses built on [`ValueFlowState`].

use super::{
    Comparison, EffectiveBodyCx, EffectiveFlowAnalysis, Expr, ExprUse, InternalCallMode,
    JoinSemiLattice, Modifier, OperandOrder, Stmt, TryCatchClause, ValueFlowState, ValueSet,
    VariableId, apply_condition_facts,
};

/// Policy hooks for an effective-body analysis backed by [`ValueFlowState`].
///
/// The adapter owns assignment, declaration, modifier, call, return, and try-binding plumbing.
/// Implementations provide only their abstract-value policy and lint-specific observations. This
/// is analogous to rustc dataflow analyses separating the engine from transfer semantics.
pub trait ValueFlowAnalysis<'hir>: Sized {
    /// Abstract property attached to each reaching value.
    type Property: Clone + Eq;
    /// Complete analysis domain, which may layer facts beside the value flow.
    type Domain: Clone + Eq + JoinSemiLattice;

    /// Returns the value-flow component of `state`.
    fn flow(state: &Self::Domain) -> &ValueFlowState<Self::Property>;

    /// Returns mutable access to the value-flow component of `state`.
    fn flow_mut(state: &mut Self::Domain) -> &mut ValueFlowState<Self::Property>;

    /// Selects variables tracked by the reusable flow domain.
    fn tracks_variable(&self, _cx: EffectiveBodyCx<'hir>, _variable: VariableId) -> bool {
        true
    }

    /// Returns the reaching value represented by `expr`.
    fn value_of(
        &self,
        _cx: EffectiveBodyCx<'hir>,
        flow: &ValueFlowState<Self::Property>,
        expr: &'hir Expr<'hir>,
    ) -> ValueSet<Self::Property>;

    /// Property used for unknown, mutated, or uninitialized values.
    fn unknown_property(&self) -> Self::Property;

    /// Property used for values produced by `delete`.
    fn deleted_property(&self) -> Self::Property {
        self.unknown_property()
    }

    /// Selects source operand ordering for the effective-flow engine.
    fn operand_order(&self) -> OperandOrder {
        OperandOrder::Unspecified
    }

    /// Observes an expression after standard value transfer has run.
    fn inspect_expr_after(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
        _use_: ExprUse,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes an edge where `condition` has `value` after standard comparison decomposition.
    fn inspect_condition(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _condition: &'hir Expr<'hir>,
        _value: bool,
        _state: &mut Self::Domain,
    ) {
    }

    /// Refines lint-specific facts from a normalized comparison.
    fn apply_comparison_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _comparison: Comparison<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a statement after standard declaration/return binding has run.
    fn inspect_statement_after(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _statement: &'hir Stmt<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier activation after its parameters have been bound.
    fn inspect_modifier_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: super::FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier return after caller-owned value facts have been restored.
    ///
    /// `caller` is the state immediately before the modifier activation. Layered domains can use
    /// it to restore activation-scoped facts which are not part of [`ValueFlowState`].
    fn inspect_modifier_return(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: super::FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a resolved internal-call activation after its parameters have been bound.
    fn inspect_call_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a resolved internal-call return after caller-owned values and outputs are restored.
    fn inspect_call_return(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a try/catch clause entry after its return or error parameters have been bound.
    fn inspect_try_clause_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _try_expr: &'hir Expr<'hir>,
        _clause: &'hir TryCatchClause<'hir>,
        _is_success: bool,
        _state: &mut Self::Domain,
    ) {
    }

    /// Selects how a statically resolved internal call is traversed.
    fn internal_call_mode(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _state: &Self::Domain,
    ) -> InternalCallMode {
        InternalCallMode::Analyze
    }

    /// Applies an unresolved internal-call effect.
    fn apply_indirect_internal_call_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        state: &mut Self::Domain,
    ) {
        Self::flow_mut(state).forget_values();
    }
}

/// Adapts a [`ValueFlowAnalysis`] policy to the effective-flow engine.
pub struct ValueFlowAdapter<A> {
    analysis: A,
}

impl<A> ValueFlowAdapter<A> {
    /// Creates an adapter around `analysis`.
    pub const fn new(analysis: A) -> Self {
        Self { analysis }
    }

    /// Returns the wrapped analysis policy.
    pub const fn analysis(&self) -> &A {
        &self.analysis
    }

    /// Returns mutable access to the wrapped analysis policy.
    pub const fn analysis_mut(&mut self) -> &mut A {
        &mut self.analysis
    }

    /// Consumes the adapter and returns its analysis policy.
    pub fn into_analysis(self) -> A {
        self.analysis
    }
}

impl<'hir, A> EffectiveFlowAnalysis<'hir> for ValueFlowAdapter<A>
where
    A: ValueFlowAnalysis<'hir>,
{
    type Domain = A::Domain;

    fn operand_order(&self) -> OperandOrder {
        self.analysis.operand_order()
    }

    fn apply_expr_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        expr: &'hir Expr<'hir>,
        use_: ExprUse,
        state: &mut Self::Domain,
    ) {
        {
            let analysis = &self.analysis;
            let unknown = analysis.unknown_property();
            let deleted = analysis.deleted_property();
            let mut tracks = |variable| analysis.tracks_variable(cx, variable);
            let mut value_of =
                |flow: &ValueFlowState<A::Property>, value| analysis.value_of(cx, flow, value);
            A::flow_mut(state).apply_expr_with(
                cx.gcx(),
                expr,
                &mut tracks,
                &mut value_of,
                unknown,
                deleted,
            );
        }
        self.analysis.inspect_expr_after(cx, expr, use_, state);
    }

    fn apply_comparison_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        comparison: Comparison<'hir>,
        state: &mut Self::Domain,
    ) {
        self.analysis.apply_comparison_effect(cx, comparison, state);
    }

    fn apply_condition_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        condition: &'hir Expr<'hir>,
        value: bool,
        state: &mut Self::Domain,
    ) {
        apply_condition_facts(condition, value, state, &mut |comparison, state| {
            self.analysis.apply_comparison_effect(cx, comparison, state);
        });
        self.analysis.inspect_condition(cx, condition, value, state);
    }

    fn apply_modifier_entry_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        modifier: &'hir Modifier<'hir>,
        callee: super::FunctionId,
        state: &mut Self::Domain,
    ) {
        let analysis = &self.analysis;
        let unknown = analysis.unknown_property();
        let mut tracks = |variable| analysis.tracks_variable(cx, variable);
        let mut value_of =
            |flow: &ValueFlowState<A::Property>, value| analysis.value_of(cx, flow, value);
        A::flow_mut(state).bind_modifier_with(
            cx,
            modifier,
            callee,
            &mut tracks,
            &mut value_of,
            unknown,
        );
        self.analysis.inspect_modifier_entry(cx, modifier, callee, state);
    }

    fn apply_modifier_return_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        modifier: &'hir Modifier<'hir>,
        callee: super::FunctionId,
        caller: &Self::Domain,
        state: &mut Self::Domain,
    ) {
        A::flow_mut(state).return_from_modifier(cx, callee, A::flow(caller));
        self.analysis.inspect_modifier_return(cx, modifier, callee, caller, state);
    }

    fn apply_call_entry_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        state: &mut Self::Domain,
    ) {
        let analysis = &self.analysis;
        let unknown = analysis.unknown_property();
        let mut tracks = |variable| analysis.tracks_variable(cx, variable);
        let mut value_of =
            |flow: &ValueFlowState<A::Property>, value| analysis.value_of(cx, flow, value);
        A::flow_mut(state).bind_call_with(cx, call, callee, &mut tracks, &mut value_of, unknown);
        self.analysis.inspect_call_entry(cx, call, callee, state);
    }

    fn apply_call_return_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        caller: &Self::Domain,
        state: &mut Self::Domain,
    ) {
        A::flow_mut(state).return_from_call(cx, call, callee, A::flow(caller));
        self.analysis.inspect_call_return(cx, call, callee, caller, state);
    }

    fn apply_try_clause_entry_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        try_expr: &'hir Expr<'hir>,
        clause: &'hir TryCatchClause<'hir>,
        is_success: bool,
        state: &mut Self::Domain,
    ) {
        let analysis = &self.analysis;
        let unknown = analysis.unknown_property();
        let mut tracks = |variable| analysis.tracks_variable(cx, variable);
        A::flow_mut(state).bind_try_clause_with(try_expr, clause, is_success, &mut tracks, unknown);
        self.analysis.inspect_try_clause_entry(cx, try_expr, clause, is_success, state);
    }

    fn apply_statement_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
        state: &mut Self::Domain,
    ) {
        {
            let analysis = &self.analysis;
            let unknown = analysis.unknown_property();
            let mut tracks = |variable| analysis.tracks_variable(cx, variable);
            let mut value_of =
                |flow: &ValueFlowState<A::Property>, value| analysis.value_of(cx, flow, value);
            A::flow_mut(state).apply_statement_with(
                cx,
                statement,
                &mut tracks,
                &mut value_of,
                unknown,
            );
        }
        self.analysis.inspect_statement_after(cx, statement, state);
    }

    fn internal_call_mode(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        state: &Self::Domain,
    ) -> InternalCallMode {
        self.analysis.internal_call_mode(cx, call, callee, state)
    }

    fn apply_indirect_internal_call_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        state: &mut Self::Domain,
    ) {
        self.analysis.apply_indirect_internal_call_effect(cx, call, state);
    }
}
