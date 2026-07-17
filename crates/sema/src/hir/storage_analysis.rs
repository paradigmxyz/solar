//! Effective-body adapter for analyses built on [`StorageAliasState`].

use super::{
    Comparison, EffectiveBodyCx, EffectiveFlowAnalysis, Expr, ExprUse, InternalCallMode,
    JoinSemiLattice, Modifier, OperandOrder, Stmt, StorageAliasState, TryCatchClause,
    apply_condition_facts,
};

/// Policy hooks for an effective-body analysis backed by [`StorageAliasState`].
///
/// The adapter owns storage-reference and Yul-slot binding across declarations, modifiers, and
/// internal calls. Policies inspect accesses before the standard alias transfer is applied.
pub trait StorageFlowAnalysis<'hir>: Sized {
    /// Complete analysis domain, which may layer facts beside storage provenance.
    type Domain: Clone + Eq + JoinSemiLattice;

    /// Returns the storage-provenance component of `state`.
    fn storage(state: &Self::Domain) -> &StorageAliasState;

    /// Returns mutable access to the storage-provenance component of `state`.
    fn storage_mut(state: &mut Self::Domain) -> &mut StorageAliasState;

    /// Selects source operand ordering for the effective-flow engine.
    fn operand_order(&self) -> OperandOrder {
        OperandOrder::Unspecified
    }

    /// Observes an expression before standard alias transfer has run.
    fn inspect_expr_before(
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

    /// Refines layered analysis facts from a normalized comparison.
    fn apply_comparison_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _comparison: Comparison<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a statement before standard declaration/return alias binding has run.
    fn inspect_statement_before(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _statement: &'hir Stmt<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier activation after standard storage-parameter binding.
    fn inspect_modifier_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: super::FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier return after caller-owned storage provenance has been restored.
    fn inspect_modifier_return(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: super::FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a resolved internal-call activation after standard parameter binding.
    fn inspect_call_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a resolved internal-call return after caller-owned provenance has been restored.
    fn inspect_call_return(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a try/catch clause entry.
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
        Self::storage_mut(state).forget();
    }
}

/// Adapts a [`StorageFlowAnalysis`] policy to the effective-flow engine.
pub struct StorageFlowAdapter<A> {
    analysis: A,
}

impl<A> StorageFlowAdapter<A> {
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

impl<'hir, A> EffectiveFlowAnalysis<'hir> for StorageFlowAdapter<A>
where
    A: StorageFlowAnalysis<'hir>,
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
        self.analysis.inspect_expr_before(cx, expr, use_, state);
        A::storage_mut(state).apply_expr_effect(cx, expr);
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
        A::storage_mut(state).bind_modifier(cx, modifier, callee);
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
        A::storage_mut(state).return_from_modifier(cx, callee, A::storage(caller));
        self.analysis.inspect_modifier_return(cx, modifier, callee, caller, state);
    }

    fn apply_call_entry_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        callee: super::FunctionId,
        state: &mut Self::Domain,
    ) {
        A::storage_mut(state).bind_call(cx, call, callee);
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
        A::storage_mut(state).return_from_call(cx, call, callee, A::storage(caller));
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
        self.analysis.inspect_try_clause_entry(cx, try_expr, clause, is_success, state);
    }

    fn apply_statement_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
        state: &mut Self::Domain,
    ) {
        self.analysis.inspect_statement_before(cx, statement, state);
        A::storage_mut(state).apply_statement_effect(cx, statement);
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
