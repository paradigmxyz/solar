//! Composable standard transfer state for source-level lint analyses.

use super::{
    Comparison, EffectiveBodyCx, EffectiveFlowAnalysis, Expr, ExprUse, InternalCallMode,
    JoinSemiLattice, Modifier, OperandOrder, Stmt, StorageAliasState, TryCatchClause,
    ValueFlowState, ValueSet, VariableId, apply_condition_facts,
};

/// Standard source-level facts shared by lints which need both values and storage provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintFlowState<P> {
    values: ValueFlowState<P>,
    storage: StorageAliasState,
}

impl<P: Clone + Eq> LintFlowState<P> {
    /// Creates standard lint state with `initial_property` for untouched values.
    pub fn new(initial_property: P) -> Self {
        Self {
            values: ValueFlowState::new(initial_property),
            storage: StorageAliasState::default(),
        }
    }

    /// Returns the value-flow component.
    pub fn values(&self) -> &ValueFlowState<P> {
        &self.values
    }

    /// Returns mutable access to the value-flow component.
    pub fn values_mut(&mut self) -> &mut ValueFlowState<P> {
        &mut self.values
    }

    /// Returns the storage-provenance component.
    pub fn storage(&self) -> &StorageAliasState {
        &self.storage
    }

    /// Returns mutable access to the storage-provenance component.
    pub fn storage_mut(&mut self) -> &mut StorageAliasState {
        &mut self.storage
    }

    /// Forgets facts invalidated by an opaque source operation.
    pub fn forget(&mut self) {
        self.values.forget_values();
        self.storage.forget();
    }
}

impl<P: Clone + Eq> JoinSemiLattice for LintFlowState<P> {
    fn join(&mut self, other: &Self) -> bool {
        self.values.join(&other.values) | self.storage.join(&other.storage)
    }
}

/// Policy hooks for an effective-body analysis backed by [`LintFlowState`].
///
/// This is the full rustc-style standard transfer adapter. Use [`super::ValueFlowAdapter`] or
/// [`super::StorageFlowAdapter`] when only one component is needed; use this policy when a lint
/// correlates values or call results with storage reads and writes.
pub trait LintFlowAnalysis<'hir>: Sized {
    /// Abstract property attached to each reaching value.
    type Property: Clone + Eq;
    /// Complete analysis domain, which may layer lint-specific facts beside standard flow.
    type Domain: Clone + Eq + JoinSemiLattice;

    /// Returns the standard flow component of `state`.
    fn flow(state: &Self::Domain) -> &LintFlowState<Self::Property>;

    /// Returns mutable access to the standard flow component of `state`.
    fn flow_mut(state: &mut Self::Domain) -> &mut LintFlowState<Self::Property>;

    /// Selects variables tracked by value flow.
    fn tracks_variable(&self, _cx: EffectiveBodyCx<'hir>, _variable: VariableId) -> bool {
        true
    }

    /// Returns the reaching value represented by `expr`.
    fn value_of(
        &self,
        _cx: EffectiveBodyCx<'hir>,
        values: &ValueFlowState<Self::Property>,
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

    /// Observes an expression before standard value and storage transfer.
    fn inspect_expr_before(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
        _use_: &ExprUse,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes an expression after standard value and storage transfer.
    fn inspect_expr_after(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
        _use_: &ExprUse,
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

    /// Observes a statement before standard declaration and return transfer.
    fn inspect_statement_before(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _statement: &'hir Stmt<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a statement after standard declaration and return transfer.
    fn inspect_statement_after(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _statement: &'hir Stmt<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier activation after standard value and storage parameter binding.
    fn inspect_modifier_entry(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: super::FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a modifier return after caller-owned standard facts have been restored.
    ///
    /// `caller` is the state immediately before activation. Policies should restore any layered
    /// activation-scoped facts here, mirroring rustc's call-return transfer edge.
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

    /// Observes a resolved internal-call return after caller-owned standard facts are restored.
    fn inspect_call_return(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: super::FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Observes a try/catch clause entry after standard value binding.
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
        Self::flow_mut(state).forget();
    }
}

/// Adapts a [`LintFlowAnalysis`] policy to the effective-flow engine.
pub struct LintFlowAdapter<A> {
    analysis: A,
}

impl<A> LintFlowAdapter<A> {
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

impl<'hir, A> EffectiveFlowAnalysis<'hir> for LintFlowAdapter<A>
where
    A: LintFlowAnalysis<'hir>,
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
        self.analysis.inspect_expr_before(cx, expr, &use_, state);
        {
            let analysis = &self.analysis;
            let unknown = analysis.unknown_property();
            let deleted = analysis.deleted_property();
            let mut tracks = |variable| analysis.tracks_variable(cx, variable);
            let mut value_of =
                |values: &ValueFlowState<A::Property>, value| analysis.value_of(cx, values, value);
            let flow = A::flow_mut(state);
            flow.values.apply_expr_with(
                cx.gcx(),
                expr,
                &mut tracks,
                &mut value_of,
                unknown,
                deleted,
            );
            flow.storage.apply_expr_effect(cx, expr);
        }
        self.analysis.inspect_expr_after(cx, expr, &use_, state);
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
            |values: &ValueFlowState<A::Property>, value| analysis.value_of(cx, values, value);
        let flow = A::flow_mut(state);
        flow.values.bind_modifier_with(cx, modifier, callee, &mut tracks, &mut value_of, unknown);
        flow.storage.bind_modifier(cx, modifier, callee);
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
        let caller_flow = A::flow(caller);
        let flow = A::flow_mut(state);
        flow.values.return_from_modifier(cx, callee, &caller_flow.values);
        flow.storage.return_from_modifier(cx, callee, &caller_flow.storage);
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
            |values: &ValueFlowState<A::Property>, value| analysis.value_of(cx, values, value);
        let flow = A::flow_mut(state);
        flow.values.bind_call_with(cx, call, callee, &mut tracks, &mut value_of, unknown);
        flow.storage.bind_call(cx, call, callee);
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
        let caller_flow = A::flow(caller);
        let flow = A::flow_mut(state);
        flow.values.return_from_call(cx, call, callee, &caller_flow.values);
        flow.storage.return_from_call(cx, call, callee, &caller_flow.storage);
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
        A::flow_mut(state).values.bind_try_clause_with(
            try_expr,
            clause,
            is_success,
            &mut tracks,
            unknown,
        );
        self.analysis.inspect_try_clause_entry(cx, try_expr, clause, is_success, state);
    }

    fn apply_statement_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        statement: &'hir Stmt<'hir>,
        state: &mut Self::Domain,
    ) {
        self.analysis.inspect_statement_before(cx, statement, state);
        {
            let analysis = &self.analysis;
            let unknown = analysis.unknown_property();
            let mut tracks = |variable| analysis.tracks_variable(cx, variable);
            let mut value_of =
                |values: &ValueFlowState<A::Property>, value| analysis.value_of(cx, values, value);
            let flow = A::flow_mut(state);
            flow.values.apply_statement_with(cx, statement, &mut tracks, &mut value_of, unknown);
            flow.storage.apply_statement_effect(cx, statement);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Compiler,
        hir::{ExprKind, FunctionId},
        ty::Gcx,
    };
    use solar_interface::{Session, config::CompileOpts};
    use std::{ops::ControlFlow, path::PathBuf};

    const SOURCE: &str = r#"
contract C {
    uint256 balance;

    modifier protected() {
        _;
        balance = 3;
    }

    function entry(address payable recipient) external {
        uint256 amount = balance;
        bool ok = recipient.send(amount);
        require(ok);
        writeBalance();
    }

    function writeBalance() internal {
        balance = 0;
    }

    function protectedEntry() external protected {
        balance = 1;
        writeBalance();
    }

    function loopEntry(address payable recipient) external {
        while (balance != 0) {
            if (recipient.send(balance)) {
                balance = 0;
            } else {
                break;
            }
        }
    }
}
"#;

    struct Analysis<'hir> {
        gcx: Gcx<'hir>,
        ok: VariableId,
        balance: VariableId,
        observed_successful_write: bool,
        observed_repeated_call_condition: bool,
    }

    impl<'hir> LintFlowAnalysis<'hir> for Analysis<'hir> {
        type Property = bool;
        type Domain = LintFlowState<bool>;

        fn flow(state: &Self::Domain) -> &LintFlowState<Self::Property> {
            state
        }

        fn flow_mut(state: &mut Self::Domain) -> &mut LintFlowState<Self::Property> {
            state
        }

        fn value_of(
            &self,
            _cx: EffectiveBodyCx<'hir>,
            values: &ValueFlowState<Self::Property>,
            expr: &'hir Expr<'hir>,
        ) -> ValueSet<Self::Property> {
            values.expr(self.gcx, expr, false)
        }

        fn unknown_property(&self) -> Self::Property {
            false
        }

        fn operand_order(&self) -> OperandOrder {
            OperandOrder::Unspecified
        }

        fn inspect_condition(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            condition: &'hir Expr<'hir>,
            value: bool,
            state: &mut Self::Domain,
        ) {
            if value {
                state.values_mut().refine_expr(self.gcx, condition, false, |success| {
                    *success = true;
                });
                if self.gcx.call_info(condition).is_some_and(|info| {
                    info.builtin() == Some(crate::builtins::Builtin::AddressPayableSend)
                }) && !state.values().evaluated_sites().is_correlatable(condition.id)
                    && state
                        .values()
                        .call_result(condition.id, 0)
                        .is_some_and(|result| result.is_proven(|value| *value.property()))
                {
                    self.observed_repeated_call_condition = true;
                }
            }
        }

        fn inspect_expr_before(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            _use_: &ExprUse,
            state: &mut Self::Domain,
        ) {
            if let ExprKind::Assign(target, ..) = &expr.kind
                && cx.reports_enabled()
                && state.storage().write_roots(self.gcx, target).contains(self.balance)
                && state.values().variable(self.ok).is_proven(|value| *value.property())
            {
                self.observed_successful_write = true;
            }
        }
    }

    #[test]
    fn composes_value_conditions_with_storage_provenance() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);
        compiler.enter_mut(|compiler| {
            let mut parser = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("lint_flow.sol"), SOURCE)
                .unwrap();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|compiler| {
            let gcx = compiler.gcx();
            let variable = |name| {
                gcx.hir
                    .variable_ids()
                    .find(|&variable| {
                        gcx.hir
                            .variable(variable)
                            .name
                            .is_some_and(|ident| ident.name.as_str() == name)
                    })
                    .unwrap()
            };
            let entry = gcx
                .hir
                .function_ids()
                .find(|&function| gcx.item_canonical_name(function).to_string() == "C.entry")
                .unwrap();
            let analysis = Analysis {
                gcx,
                ok: variable("ok"),
                balance: variable("balance"),
                observed_successful_write: false,
                observed_repeated_call_condition: false,
            };
            let mut analysis = LintFlowAdapter::new(analysis);
            let _ = super::super::analyze_effective_body_flow(
                gcx,
                entry,
                LintFlowState::new(false),
                &mut analysis,
            );
            assert!(analysis.analysis().observed_successful_write);

            let loop_entry = gcx
                .hir
                .function_ids()
                .find(|&function| gcx.item_canonical_name(function).to_string() == "C.loopEntry")
                .unwrap();
            let analysis = Analysis {
                gcx,
                ok: variable("ok"),
                balance: variable("balance"),
                observed_successful_write: false,
                observed_repeated_call_condition: false,
            };
            let mut analysis = LintFlowAdapter::new(analysis);
            let _ = super::super::analyze_effective_body_flow(
                gcx,
                loop_entry,
                LintFlowState::new(false),
                &mut analysis,
            );
            assert!(analysis.analysis().observed_repeated_call_condition);
        });
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LifecycleState {
        flow: LintFlowState<bool>,
        protected: bool,
    }

    impl JoinSemiLattice for LifecycleState {
        fn join(&mut self, other: &Self) -> bool {
            let changed = self.flow.join(&other.flow);
            let protected = self.protected && other.protected;
            let changed = changed || protected != self.protected;
            self.protected = protected;
            changed
        }
    }

    struct LifecycleAnalysis<'hir> {
        gcx: Gcx<'hir>,
        modifier: FunctionId,
        helper: FunctionId,
        balance: VariableId,
        protected_writes: Vec<(FunctionId, bool)>,
        helper_entry_was_protected: bool,
        helper_return_was_protected: bool,
        modifier_return_saw_active: bool,
        modifier_return_restored_caller: bool,
    }

    impl<'hir> LintFlowAnalysis<'hir> for LifecycleAnalysis<'hir> {
        type Property = bool;
        type Domain = LifecycleState;

        fn flow(state: &Self::Domain) -> &LintFlowState<Self::Property> {
            &state.flow
        }

        fn flow_mut(state: &mut Self::Domain) -> &mut LintFlowState<Self::Property> {
            &mut state.flow
        }

        fn value_of(
            &self,
            _cx: EffectiveBodyCx<'hir>,
            values: &ValueFlowState<Self::Property>,
            expr: &'hir Expr<'hir>,
        ) -> ValueSet<Self::Property> {
            values.expr(self.gcx, expr, false)
        }

        fn unknown_property(&self) -> Self::Property {
            false
        }

        fn inspect_modifier_entry(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _modifier: &'hir Modifier<'hir>,
            callee: FunctionId,
            state: &mut Self::Domain,
        ) {
            if callee == self.modifier {
                state.protected = true;
            }
        }

        fn inspect_modifier_return(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _modifier: &'hir Modifier<'hir>,
            callee: FunctionId,
            caller: &Self::Domain,
            state: &mut Self::Domain,
        ) {
            if callee == self.modifier {
                self.modifier_return_saw_active = state.protected;
                state.protected = caller.protected;
                self.modifier_return_restored_caller = !state.protected;
            }
        }

        fn inspect_call_entry(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            callee: FunctionId,
            state: &mut Self::Domain,
        ) {
            if callee == self.helper {
                self.helper_entry_was_protected = state.protected;
            }
        }

        fn inspect_call_return(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            callee: FunctionId,
            caller: &Self::Domain,
            state: &mut Self::Domain,
        ) {
            if callee == self.helper {
                self.helper_return_was_protected = state.protected;
                state.protected = caller.protected;
            }
        }

        fn inspect_expr_before(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            _use_: &ExprUse,
            state: &mut Self::Domain,
        ) {
            if cx.reports_enabled()
                && let ExprKind::Assign(target, ..) = &expr.kind
                && state.flow.storage().write_roots(self.gcx, target).contains(self.balance)
            {
                self.protected_writes.push((cx.function(), state.protected));
            }
        }
    }

    #[test]
    fn exposes_activation_lifecycle_after_standard_transfer() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);
        compiler.enter_mut(|compiler| {
            let mut parser = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("lint_lifecycle.sol"), SOURCE)
                .unwrap();
            parser.add_file(file);
            parser.parse();
            assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|compiler| {
            let gcx = compiler.gcx();
            let function = |name| {
                gcx.hir
                    .function_ids()
                    .find(|&function| gcx.item_canonical_name(function).to_string() == name)
                    .unwrap()
            };
            let balance = gcx
                .hir
                .variable_ids()
                .find(|&variable| {
                    gcx.hir
                        .variable(variable)
                        .name
                        .is_some_and(|name| name.name.as_str() == "balance")
                })
                .unwrap();
            let entry = function("C.protectedEntry");
            let modifier = function("C.protected");
            let helper = function("C.writeBalance");
            let analysis = LifecycleAnalysis {
                gcx,
                modifier,
                helper,
                balance,
                protected_writes: Vec::new(),
                helper_entry_was_protected: false,
                helper_return_was_protected: false,
                modifier_return_saw_active: false,
                modifier_return_restored_caller: false,
            };
            let mut analysis = LintFlowAdapter::new(analysis);
            let result = super::super::analyze_effective_body_flow(
                gcx,
                entry,
                LifecycleState { flow: LintFlowState::new(false), protected: false },
                &mut analysis,
            );

            assert_eq!(
                analysis.analysis().protected_writes,
                [(entry, true), (helper, true), (modifier, true)]
            );
            assert!(analysis.analysis().helper_entry_was_protected);
            assert!(analysis.analysis().helper_return_was_protected);
            assert!(analysis.analysis().modifier_return_saw_active);
            assert!(analysis.analysis().modifier_return_restored_caller);
            assert!(!result.normal_exit().unwrap().protected);
        });
    }
}
