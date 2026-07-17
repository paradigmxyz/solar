//! Runtime-aware forward dataflow over HIR function bodies.

use super::{
    Block, Comparison, EffectiveBodyCx, Expr, ExprKind, Function, FunctionId, Modifier, Place,
    Stmt, StmtKind, VariableId, apply_condition_facts,
    effective::effective_body_dispatch_contracts,
};
use crate::{
    builtins::Builtin,
    ty::{CallKind, CallTermination, Gcx},
};
use solar_data_structures::smallvec::SmallVec;
use std::ptr;

/// Describes how an expression's value is consumed by its parent.
///
/// This is the HIR counterpart of rustc's expression-use and place contexts. Analyses can use it
/// to distinguish a read from an assignment target or a value which is merely copied into a local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprUse {
    /// The expression is evaluated as an ordinary value.
    Value,
    /// The expression is the callee of a call.
    Callee,
    /// The expression is evaluated only to project a member, index, or slice.
    ///
    /// Like rustc's non-mutating projection place context, this does not by itself read or escape
    /// every value contained by an aggregate base. The resulting projection carries the actual
    /// read, store, or place context.
    Projection,
    /// The expression denotes an assignment or mutation target.
    Place,
    /// The expression's value is stored into the given place.
    ///
    /// This is an observation context; analyses should apply the actual write when the parent
    /// assignment or declaration callback runs. `None` represents a destructuring or computed
    /// target without one unambiguous source-level place.
    Store(Option<Place>),
    /// The expression is evaluated as a statement and its result is discarded.
    Discard,
}

/// A finite join-semilattice used as a forward dataflow domain.
///
/// This follows the shape of `rustc_mir_dataflow::JoinSemiLattice`: `join` mutates `self` and
/// returns whether the value changed. Domains must have finite height (or otherwise guarantee
/// convergence), and analysis transfer functions must be monotone: loop and recursive-call
/// fixpoints rely on both properties.
pub trait JoinSemiLattice {
    /// Joins `other` into `self` and returns whether `self` changed.
    fn join(&mut self, other: &Self) -> bool;
}

impl JoinSemiLattice for bool {
    fn join(&mut self, other: &Self) -> bool {
        let old = *self;
        *self |= *other;
        old != *self
    }
}

/// Controls how an internal call participates in an effective-body analysis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InternalCallMode {
    /// Do not analyze the callee.
    Skip,
    /// Analyze the callee and enable observations such as lint diagnostics.
    #[default]
    Analyze,
    /// Analyze the callee with [`EffectiveBodyCx::reports_enabled`] set to `false`.
    ///
    /// Transfer functions still run. This mode is useful when every function is analyzed as a
    /// root and an inlined clean call would otherwise produce duplicate diagnostics.
    AnalyzeWithoutReports,
}

/// Controls how sibling expression operands are evaluated.
///
/// Solidity intentionally leaves most sibling operand order unspecified. Analyses whose result
/// depends on whether one sibling runs before another should select [`OperandOrder::Unspecified`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OperandOrder {
    /// Visit operands once in their source order.
    ///
    /// This is sufficient for order-insensitive domains and avoids replaying diagnostics.
    #[default]
    Source,
    /// Join every operand order.
    ///
    /// The driver explores small groups exactly and uses a conservative fixpoint for unusually
    /// large groups. Transfer functions can consequently be replayed and should keep all
    /// path-sensitive facts in their domain; diagnostic collectors should deduplicate spans.
    Unspecified,
}

/// Transfer functions for runtime-aware forward dataflow.
///
/// The driver owns evaluation order, control-flow joins, loop fixpoints, modifier expansion,
/// internal-call traversal, aborting paths, virtual dispatch, and recursive call summaries.
/// Implementations only describe the domain-specific effects of expressions and statements.
/// Semantic transfer must not depend on exact [`EffectiveBodyCx::call_depth`] or
/// [`EffectiveBodyCx::loop_depth`], because recursive summaries intentionally abstract dynamic
/// activation depth. The finite `in_loop`/`in_enclosing_loop` context is preserved; exact depths
/// may still classify or deduplicate observations.
pub trait EffectiveFlowAnalysis<'hir> {
    /// The forward dataflow state.
    type Domain: Clone + Eq + JoinSemiLattice;

    /// Selects how Solidity sibling operands participate in the analysis.
    ///
    /// Implementations must choose explicitly so an order-sensitive analysis cannot silently use
    /// source order for Solidity expressions whose evaluation order is unspecified.
    fn operand_order(&self) -> OperandOrder;

    /// Applies the local effect of an expression after its child expressions are evaluated.
    fn apply_expr_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _expr: &'hir Expr<'hir>,
        _use_: ExprUse,
        _state: &mut Self::Domain,
    ) {
    }

    /// Refines `state` for an edge on which `condition` has the given boolean value.
    ///
    /// The driver invokes this for `if` and ternary arms, short-circuit operands, loop conditions,
    /// and the successful continuation of `require` and `assert`.
    fn apply_condition_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        condition: &'hir Expr<'hir>,
        value: bool,
        state: &mut Self::Domain,
    ) {
        apply_condition_facts(condition, value, state, &mut |comparison, state| {
            self.apply_comparison_effect(cx, comparison, state);
        });
    }

    /// Refines `state` with one normalized comparison known to hold on the current edge.
    fn apply_comparison_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _comparison: Comparison<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Binds analysis state for an applied modifier before its body is entered.
    ///
    /// Consumers which track parameter values can pair `modifier` arguments with the declaration
    /// through [`Gcx::modifier_arg`].
    fn apply_modifier_entry_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Restores caller-local state after one applied modifier activation returns normally.
    fn apply_modifier_return_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _modifier: &'hir Modifier<'hir>,
        _callee: FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Binds analysis state for an internal call before its effective body is entered.
    ///
    /// Consumers which track parameter values can use [`Gcx::call_arg_for_param`]. The ordinary
    /// [`EffectiveFlowAnalysis::apply_expr_effect`] callback runs after the callee returns, so it
    /// can consume return-variable state as the value of `call`.
    fn apply_call_entry_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: FunctionId,
        _state: &mut Self::Domain,
    ) {
    }

    /// Transfers callee return state back to the call expression after a normal internal return.
    ///
    /// The callback runs in the caller's [`EffectiveBodyCx`] after all normal callee exits have
    /// been joined and before [`EffectiveFlowAnalysis::apply_expr_effect`] observes `call`. This
    /// is the appropriate hook for analyses which map named return variables to a call result.
    /// `caller` is the state before callee parameters were bound, allowing local-value analyses
    /// to restore the caller activation after direct or mutual recursion.
    fn apply_call_return_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: FunctionId,
        _caller: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }

    /// Binds values introduced by one `try` success or catch clause.
    fn apply_try_clause_entry_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _try_expr: &'hir Expr<'hir>,
        _clause: &'hir super::TryCatchClause<'hir>,
        _is_success: bool,
        _state: &mut Self::Domain,
    ) {
    }

    /// Applies the local effect of a statement after its immediate expressions are evaluated.
    fn apply_statement_effect(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _stmt: &'hir Stmt<'hir>,
        _state: &mut Self::Domain,
    ) {
    }

    /// Selects how an internal call is analyzed.
    fn internal_call_mode(
        &mut self,
        _cx: EffectiveBodyCx<'hir>,
        _call: &'hir Expr<'hir>,
        _callee: FunctionId,
        _state: &Self::Domain,
    ) -> InternalCallMode {
        InternalCallMode::Analyze
    }

    /// Applies a conservative effect for the opaque branch of an internal function-pointer call.
    ///
    /// There is deliberately no default because preserving state silently under-approximates
    /// most may analyses. Implementations must explicitly invalidate or join their top-like fact,
    /// or choose identity when the domain is genuinely unaffected by unknown internal execution.
    fn apply_indirect_internal_call_effect(
        &mut self,
        cx: EffectiveBodyCx<'hir>,
        call: &'hir Expr<'hir>,
        state: &mut Self::Domain,
    );
}

/// The states reaching normal exits from an effective function body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveFlowResult<D> {
    fallthrough: Option<D>,
    return_: Option<D>,
    halt_: Option<D>,
}

impl<D> EffectiveFlowResult<D> {
    /// Returns the state which reaches the end of the function body.
    pub fn fallthrough(&self) -> Option<&D> {
        self.fallthrough.as_ref()
    }

    /// Returns the joined state of explicit `return` exits.
    pub fn returned(&self) -> Option<&D> {
        self.return_.as_ref()
    }

    /// Returns the joined state of successful EVM halts such as `stop` and `selfdestruct`.
    pub fn halted(&self) -> Option<&D> {
        self.halt_.as_ref()
    }
}

impl<D> EffectiveFlowResult<D>
where
    D: Clone + JoinSemiLattice,
{
    /// Returns the state joined across all successful exits.
    pub fn normal_exit(&self) -> Option<D> {
        let mut state = self.fallthrough.clone();
        merge_opt(&mut state, self.return_.clone());
        merge_opt(&mut state, self.halt_.clone());
        state
    }
}

/// Runs a forward analysis over the runtime-effective body of `function`.
///
/// Applied modifiers are expanded at placeholders. Internal calls use the root contract's virtual
/// dispatch context and are traversed according to [`EffectiveFlowAnalysis::internal_call_mode`].
/// Recursive call summaries are solved to a least fixpoint over the analysis domain.
pub fn analyze_effective_body_flow<'hir, A>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    entry: A::Domain,
    analysis: &mut A,
) -> EffectiveFlowResult<A::Domain>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    analyze_effective_body_flow_with_dispatch(
        gcx,
        function,
        gcx.hir.function(function).contract,
        entry,
        analysis,
    )
}

/// Runs a forward analysis over `function` as inherited by `dispatch_contract`.
pub fn analyze_effective_body_flow_in_contract<'hir, A>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    dispatch_contract: super::ContractId,
    entry: A::Domain,
    analysis: &mut A,
) -> EffectiveFlowResult<A::Domain>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    analyze_effective_body_flow_with_dispatch(
        gcx,
        function,
        Some(dispatch_contract),
        entry,
        analysis,
    )
}

/// Runs a forward analysis in every contract dispatch context which can execute `function`.
///
/// The same `analysis` receives callbacks for every context. Consumers which collect diagnostics
/// should deduplicate source spans. Each context starts from a fresh clone of `entry`, and the
/// returned results follow contract declaration order.
pub fn analyze_effective_body_flow_dispatches<'hir, A>(
    gcx: Gcx<'hir>,
    function: FunctionId,
    entry: A::Domain,
    analysis: &mut A,
) -> SmallVec<[EffectiveFlowResult<A::Domain>; 4]>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    effective_body_dispatch_contracts(gcx, function)
        .into_iter()
        .map(|dispatch_contract| {
            analyze_effective_body_flow_with_dispatch(
                gcx,
                function,
                dispatch_contract,
                entry.clone(),
                analysis,
            )
        })
        .collect()
}

fn analyze_effective_body_flow_with_dispatch<'hir, A>(
    gcx: Gcx<'hir>,
    root_function: FunctionId,
    dispatch_contract: Option<super::ContractId>,
    entry: A::Domain,
    analysis: &mut A,
) -> EffectiveFlowResult<A::Domain>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    let function = gcx.hir.function(root_function);
    let Some(body) = function.body else {
        return EffectiveFlowResult { fallthrough: Some(entry), return_: None, halt_: None };
    };
    let exits = EffectiveFlowEngine {
        gcx,
        analysis,
        root_function,
        dispatch_contract,
        current_function: None,
        loop_depth: 0,
        call_depth: 0,
        call_entry_loop_depths: Vec::new(),
        reports_enabled: true,
        call_summaries: Vec::new(),
        expr_aborted: false,
        expr_halted: None,
    }
    .analyze_callable(function, body, None, entry);
    EffectiveFlowResult {
        fallthrough: exits.fallthrough,
        return_: exits.return_,
        halt_: exits.halt_,
    }
}

type Placeholder<'hir> = Option<(&'hir [Modifier<'hir>], usize, Block<'hir>, Option<FunctionId>)>;

#[derive(Clone)]
enum Operand<'hir> {
    Expr(&'hir Expr<'hir>, ExprUse),
    AssignmentValue(&'hir Expr<'hir>, &'hir Expr<'hir>),
    ConditionTrue(&'hir Expr<'hir>),
}

#[derive(Clone, Debug, Default)]
struct Exits<D> {
    fallthrough: Option<D>,
    return_: Option<D>,
    halt_: Option<D>,
    break_: Option<D>,
    continue_: Option<D>,
}

impl<D> Exits<D>
where
    D: JoinSemiLattice,
{
    fn fallthrough(state: D) -> Self {
        Self { fallthrough: Some(state), return_: None, halt_: None, break_: None, continue_: None }
    }

    fn break_(state: D) -> Self {
        Self { fallthrough: None, return_: None, halt_: None, break_: Some(state), continue_: None }
    }

    fn continue_(state: D) -> Self {
        Self { fallthrough: None, return_: None, halt_: None, break_: None, continue_: Some(state) }
    }

    fn abort() -> Self {
        Self { fallthrough: None, return_: None, halt_: None, break_: None, continue_: None }
    }

    fn merge(&mut self, other: Self) {
        merge_opt(&mut self.fallthrough, other.fallthrough);
        merge_opt(&mut self.return_, other.return_);
        merge_opt(&mut self.halt_, other.halt_);
        merge_opt(&mut self.break_, other.break_);
        merge_opt(&mut self.continue_, other.continue_);
    }
}

fn merge_opt<D>(dst: &mut Option<D>, src: Option<D>)
where
    D: JoinSemiLattice,
{
    match (dst.as_mut(), src) {
        (None, src) => *dst = src,
        (Some(_), None) => {}
        (Some(dst), Some(src)) => _ = dst.join(&src),
    }
}

struct EffectiveFlowEngine<'a, 'hir, A>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    gcx: Gcx<'hir>,
    analysis: &'a mut A,
    root_function: FunctionId,
    dispatch_contract: Option<super::ContractId>,
    current_function: Option<FunctionId>,
    loop_depth: usize,
    call_depth: usize,
    call_entry_loop_depths: Vec<usize>,
    reports_enabled: bool,
    call_summaries: Vec<CallSummary<A::Domain>>,
    expr_aborted: bool,
    expr_halted: Option<A::Domain>,
}

#[derive(Clone, Debug)]
struct CallSummary<D> {
    function: FunctionId,
    entry: D,
    in_loop: bool,
    in_enclosing_loop: bool,
    normal: Option<D>,
    halt_: Option<D>,
}

impl<'hir, A> EffectiveFlowEngine<'_, 'hir, A>
where
    A: EffectiveFlowAnalysis<'hir>,
{
    fn cx(&self) -> EffectiveBodyCx<'hir> {
        EffectiveBodyCx {
            gcx: self.gcx,
            dispatch_contract: self.dispatch_contract,
            root_function: self.root_function,
            function: self.current_function,
            loop_depth: self.loop_depth,
            call_depth: self.call_depth,
            call_entry_loop_depth: self.call_entry_loop_depths.last().copied(),
            reports_enabled: self.reports_enabled,
        }
    }

    fn analyze_callable(
        &mut self,
        function: &'hir Function<'hir>,
        body: Block<'hir>,
        function_id: Option<FunctionId>,
        entry: A::Domain,
    ) -> Exits<A::Domain> {
        self.analyze_modifier_chain(function.modifiers, 0, body, function_id, entry)
    }

    fn analyze_modifier_chain(
        &mut self,
        modifiers: &'hir [Modifier<'hir>],
        index: usize,
        body: Block<'hir>,
        body_function: Option<FunctionId>,
        mut entry: A::Domain,
    ) -> Exits<A::Domain> {
        let Some(modifier) = modifiers.get(index) else {
            let previous_function = std::mem::replace(&mut self.current_function, body_function);
            let exits = self.analyze_block(body, None, entry);
            self.current_function = previous_function;
            return exits;
        };

        let operands: SmallVec<[Operand<'hir>; 4]> =
            modifier.args.exprs().map(|arg| Operand::Expr(arg, ExprUse::Value)).collect();
        self.expr_aborted = false;
        self.analyze_operands(&operands, &mut entry);
        let halted = self.expr_halted.take();
        if self.expr_aborted {
            return Exits { halt_: halted, ..Exits::abort() };
        }

        let Some(mut modifier_id) = modifier.id.as_function() else {
            let mut exits =
                self.analyze_modifier_chain(modifiers, index + 1, body, body_function, entry);
            merge_opt(&mut exits.halt_, halted);
            return exits;
        };
        if let Some(dispatch_contract) = self.dispatch_contract {
            modifier_id = self.gcx.modifier_in_contract(modifier_id, dispatch_contract);
        }
        let modifier_function = self.gcx.hir.function(modifier_id);
        let Some(modifier_body) = modifier_function.body else {
            let mut exits =
                self.analyze_modifier_chain(modifiers, index + 1, body, body_function, entry);
            merge_opt(&mut exits.halt_, halted);
            return exits;
        };

        let caller = entry.clone();
        self.analysis.apply_modifier_entry_effect(self.cx(), modifier, modifier_id, &mut entry);

        let previous_function = self.current_function.replace(modifier_id);
        let mut exits = self.analyze_block(
            modifier_body,
            Some((modifiers, index + 1, body, body_function)),
            entry,
        );
        self.current_function = previous_function;
        if let Some(state) = &mut exits.fallthrough {
            self.analysis.apply_modifier_return_effect(
                self.cx(),
                modifier,
                modifier_id,
                &caller,
                state,
            );
        }
        if let Some(state) = &mut exits.return_ {
            self.analysis.apply_modifier_return_effect(
                self.cx(),
                modifier,
                modifier_id,
                &caller,
                state,
            );
        }
        merge_opt(&mut exits.halt_, halted);
        exits
    }

    fn analyze_block(
        &mut self,
        block: Block<'hir>,
        placeholder: Placeholder<'hir>,
        mut entry: A::Domain,
    ) -> Exits<A::Domain> {
        let mut exits = Exits::abort();
        for stmt in block.stmts {
            let stmt_exits = self.analyze_stmt(stmt, placeholder, entry);
            merge_opt(&mut exits.return_, stmt_exits.return_);
            merge_opt(&mut exits.halt_, stmt_exits.halt_);
            merge_opt(&mut exits.break_, stmt_exits.break_);
            merge_opt(&mut exits.continue_, stmt_exits.continue_);
            let Some(next) = stmt_exits.fallthrough else { return exits };
            entry = next;
        }
        exits.fallthrough = Some(entry);
        exits
    }

    fn analyze_stmt(
        &mut self,
        stmt: &'hir Stmt<'hir>,
        placeholder: Placeholder<'hir>,
        mut entry: A::Domain,
    ) -> Exits<A::Domain> {
        debug_assert!(self.expr_halted.is_none());
        self.expr_aborted = false;
        match stmt.kind {
            StmtKind::DeclSingle(variable) => {
                if let Some(initializer) = self.gcx.hir.variable(variable).initializer {
                    self.analyze_expr(
                        initializer,
                        ExprUse::Store(Some(Place::from_local(variable))),
                        &mut entry,
                    );
                }
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                exits.fallthrough = Some(entry);
                exits
            }
            StmtKind::DeclMulti(variables, expr) => {
                self.analyze_multi_decl_value(variables, expr, &mut entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                exits.fallthrough = Some(entry);
                exits
            }
            StmtKind::Expr(expr) => {
                self.analyze_expr(expr, ExprUse::Discard, &mut entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                exits.fallthrough = Some(entry);
                exits
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                self.analyze_block(block, placeholder, entry)
            }
            StmtKind::Emit(expr) => {
                self.analyze_expr(expr, ExprUse::Value, &mut entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                exits.fallthrough = Some(entry);
                exits
            }
            StmtKind::Revert(expr) => {
                self.analyze_expr(expr, ExprUse::Value, &mut entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if !self.expr_aborted {
                    self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                }
                exits
            }
            StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.analyze_expr(expr, ExprUse::Value, &mut entry);
                }
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                exits.return_ = Some(entry);
                exits
            }
            StmtKind::Break => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                Exits::break_(entry)
            }
            StmtKind::Continue => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                Exits::continue_(entry)
            }
            StmtKind::Loop(block, source) => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                let mut loop_entry = entry.clone();
                self.loop_depth += 1;
                let body = loop {
                    let mut body = self.analyze_block(block, placeholder, loop_entry.clone());
                    let mut continue_fallthrough = None;
                    if let Some(continue_state) = body.continue_.take() {
                        match self.loop_continue_epilogue(block, source) {
                            Some(epilogue) => {
                                let epilogue =
                                    self.analyze_stmt(epilogue, placeholder, continue_state);
                                merge_opt(&mut body.return_, epilogue.return_);
                                merge_opt(&mut body.halt_, epilogue.halt_);
                                merge_opt(&mut body.break_, epilogue.break_);
                                continue_fallthrough = epilogue.fallthrough;
                                merge_opt(&mut continue_fallthrough, epilogue.continue_);
                            }
                            None => continue_fallthrough = Some(continue_state),
                        }
                    }
                    let mut back_edge = entry.clone();
                    if let Some(state) = &body.fallthrough {
                        _ = back_edge.join(state);
                    }
                    if let Some(state) = &continue_fallthrough {
                        _ = back_edge.join(state);
                    }
                    if !loop_entry.join(&back_edge) {
                        break body;
                    }
                };
                self.loop_depth -= 1;

                Exits {
                    // Loop fallthrough and continue edges are backedges. Only an executed `break`
                    // reaches the statement after the loop; the HIR lowering inserts that break
                    // for false `while` and `for` conditions.
                    fallthrough: body.break_,
                    return_: body.return_,
                    halt_: body.halt_,
                    break_: None,
                    continue_: None,
                }
            }
            StmtKind::If(condition, then_stmt, else_stmt) => {
                let (then_entry, else_entry) = self.analyze_condition(condition, entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if let Some(mut then_entry) = then_entry {
                    self.analysis.apply_statement_effect(self.cx(), stmt, &mut then_entry);
                    exits.merge(self.analyze_stmt(then_stmt, placeholder, then_entry));
                }
                if let Some(mut else_entry) = else_entry {
                    self.analysis.apply_statement_effect(self.cx(), stmt, &mut else_entry);
                    exits.merge(match else_stmt {
                        Some(else_stmt) => self.analyze_stmt(else_stmt, placeholder, else_entry),
                        None => Exits::fallthrough(else_entry),
                    });
                }
                exits
            }
            StmtKind::Try(try_stmt) => {
                let ExprKind::Call(..) = &try_stmt.expr.kind else {
                    unreachable!("try expression must be a call")
                };
                self.analyze_call_operands(&try_stmt.expr, ExprUse::Value, &mut entry);
                let mut exits = Exits::abort();
                exits.halt_ = self.expr_halted.take();
                if self.expr_aborted {
                    return exits;
                }

                // Argument and call-option effects happen in the caller and survive a caught
                // failure. The call's own effect only reaches the successful `returns` clause.
                let catch_entry = entry.clone();
                self.apply_expr_effects(&try_stmt.expr, ExprUse::Value, &mut entry);
                merge_opt(&mut exits.halt_, self.expr_halted.take());
                if !self.expr_aborted {
                    self.analysis.apply_try_clause_entry_effect(
                        self.cx(),
                        &try_stmt.expr,
                        &try_stmt.clauses[0],
                        true,
                        &mut entry,
                    );
                    self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                    exits.merge(self.analyze_block(try_stmt.clauses[0].block, placeholder, entry));
                }
                self.expr_aborted = false;
                for clause in &try_stmt.clauses[1..] {
                    let mut clause_entry = catch_entry.clone();
                    self.analysis.apply_try_clause_entry_effect(
                        self.cx(),
                        &try_stmt.expr,
                        clause,
                        false,
                        &mut clause_entry,
                    );
                    self.analysis.apply_statement_effect(self.cx(), stmt, &mut clause_entry);
                    exits.merge(self.analyze_block(clause.block, placeholder, clause_entry));
                }
                exits
            }
            StmtKind::Placeholder => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                match placeholder {
                    Some((modifiers, index, body, body_function)) => {
                        let mut exits = self.analyze_modifier_chain(
                            modifiers,
                            index,
                            body,
                            body_function,
                            entry,
                        );
                        // An explicit return only leaves the substituted function body or inner
                        // modifier. Execution resumes after `_` in the enclosing modifier.
                        let returned = exits.return_.take();
                        merge_opt(&mut exits.fallthrough, returned);
                        exits
                    }
                    None => Exits::fallthrough(entry),
                }
            }
            StmtKind::AssemblyBlock(block) => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                self.analyze_block(block, None, entry)
            }
            StmtKind::Switch(switch) => {
                self.analyze_expr(switch.selector, ExprUse::Value, &mut entry);
                let halted = self.expr_halted.take();
                if self.expr_aborted {
                    return Exits { halt_: halted, ..Exits::abort() };
                }
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);

                let has_default = switch.cases.last().is_some_and(|case| case.constant.is_none());
                let mut exits =
                    if has_default { Exits::abort() } else { Exits::fallthrough(entry.clone()) };
                exits.halt_ = halted;
                for case in switch.cases {
                    exits.merge(self.analyze_block(case.body, None, entry.clone()));
                }
                exits
            }
            StmtKind::Err(_) => {
                self.analysis.apply_statement_effect(self.cx(), stmt, &mut entry);
                Exits::fallthrough(entry)
            }
        }
    }

    fn loop_continue_epilogue(
        &self,
        block: Block<'hir>,
        source: super::LoopSource,
    ) -> Option<&'hir Stmt<'hir>> {
        match source {
            super::LoopSource::While => None,
            super::LoopSource::DoWhile => block.stmts.last(),
            super::LoopSource::For => {
                let [stmt] = block.stmts else { return None };
                let body = match stmt.kind {
                    StmtKind::If(_, then_stmt, _) => {
                        let StmtKind::Block(body) = then_stmt.kind else { return None };
                        body
                    }
                    // A conditionless `for` with an update lowers directly to the synthetic
                    // `{ body; update; }` block.
                    StmtKind::Block(body) => body,
                    _ => return None,
                };
                (body.span == block.span).then(|| body.stmts.last()).flatten()
            }
        }
    }

    fn analyze_call_operands(
        &mut self,
        call: &'hir Expr<'hir>,
        use_: ExprUse,
        state: &mut A::Domain,
    ) {
        let ExprKind::Call(callee, args, options) = &call.kind else { unreachable!() };
        let condition = self
            .cx()
            .call_info(call)
            .and_then(|info| {
                matches!(info.builtin(), Some(Builtin::Require | Builtin::Assert)).then_some(())
            })
            .and_then(|()| self.gcx.call_arg(call, 0));
        let transparent_conversion = options.is_none()
            && args.len() == 1
            && !ptr::eq(self.gcx.peel_injective_type_conversions(call), call);
        let mut operands: SmallVec<[Operand<'hir>; 4]> = SmallVec::new();
        operands.push(Operand::Expr(callee, ExprUse::Callee));
        if let Some(options) = options {
            for option in options.args {
                operands.push(Operand::Expr(&option.value, ExprUse::Value));
            }
        }
        for arg in args.exprs() {
            if condition.is_some_and(|condition| ptr::eq(arg, condition)) {
                operands.push(Operand::ConditionTrue(arg));
            } else {
                operands.push(Operand::Expr(
                    arg,
                    if transparent_conversion { use_.clone() } else { ExprUse::Value },
                ));
            }
        }
        self.analyze_operands(&operands, state);
    }

    fn analyze_expr(&mut self, expr: &'hir Expr<'hir>, use_: ExprUse, state: &mut A::Domain) {
        if self.expr_aborted {
            return;
        }

        match &expr.kind {
            ExprKind::Call(..) => {
                self.analyze_call_operands(expr, use_.clone(), state);
            }
            ExprKind::Binary(lhs, op, rhs)
                if matches!(op.kind, super::BinOpKind::And | super::BinOpKind::Or) =>
            {
                let (mut true_state, false_state) = self.analyze_condition(expr, state.clone());
                merge_opt(&mut true_state, false_state);
                if let Some(joined) = true_state {
                    *state = joined;
                    self.expr_aborted = false;
                } else {
                    self.expr_aborted = true;
                }
                return;
            }
            ExprKind::Assign(lhs, op, rhs) => {
                let rhs = if op.is_none() {
                    Operand::AssignmentValue(lhs, rhs)
                } else {
                    Operand::Expr(rhs, ExprUse::Value)
                };
                self.analyze_operands(
                    &[
                        Operand::Expr(
                            lhs,
                            if op.is_some() { ExprUse::Value } else { ExprUse::Place },
                        ),
                        rhs,
                    ],
                    state,
                );
            }
            ExprKind::Binary(lhs, _, rhs) => {
                self.analyze_operands(
                    &[Operand::Expr(lhs, ExprUse::Value), Operand::Expr(rhs, ExprUse::Value)],
                    state,
                );
            }
            ExprKind::Unary(_, inner) => self.analyze_expr(inner, ExprUse::Value, state),
            ExprKind::Delete(inner) => self.analyze_expr(inner, ExprUse::Place, state),
            ExprKind::Payable(inner) => {
                self.analyze_expr(inner, use_.clone(), state);
            }
            ExprKind::Index(base, index) => {
                let mut operands: SmallVec<[Operand<'hir>; 2]> = SmallVec::new();
                operands.push(Operand::Expr(
                    base,
                    if use_ == ExprUse::Place { ExprUse::Place } else { ExprUse::Projection },
                ));
                if let Some(index) = index {
                    operands.push(Operand::Expr(index, ExprUse::Value));
                }
                self.analyze_operands(&operands, state);
            }
            ExprKind::Slice(base, start, end) => {
                let mut operands: SmallVec<[Operand<'hir>; 3]> = SmallVec::new();
                operands.push(Operand::Expr(
                    base,
                    if use_ == ExprUse::Place { ExprUse::Place } else { ExprUse::Projection },
                ));
                if let Some(start) = start {
                    operands.push(Operand::Expr(start, ExprUse::Value));
                }
                if let Some(end) = end {
                    operands.push(Operand::Expr(end, ExprUse::Value));
                }
                self.analyze_operands(&operands, state);
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                let (then_state, else_state) = self.analyze_condition(condition, state.clone());
                let mut joined = None;
                if let Some(mut then_state) = then_state {
                    self.expr_aborted = false;
                    self.analyze_expr(then_expr, use_.clone(), &mut then_state);
                    if !self.expr_aborted {
                        merge_opt(&mut joined, Some(then_state));
                    }
                }
                if let Some(mut else_state) = else_state {
                    self.expr_aborted = false;
                    self.analyze_expr(else_expr, use_.clone(), &mut else_state);
                    if !self.expr_aborted {
                        merge_opt(&mut joined, Some(else_state));
                    }
                }
                if let Some(joined) = joined {
                    *state = joined;
                    self.expr_aborted = false;
                } else {
                    self.expr_aborted = true;
                }
            }
            ExprKind::Array(exprs) => {
                let operands: SmallVec<[Operand<'hir>; 4]> =
                    exprs.iter().map(|expr| Operand::Expr(expr, ExprUse::Value)).collect();
                self.analyze_operands(&operands, state);
            }
            ExprKind::Tuple(exprs) => {
                let element_use =
                    if use_ == ExprUse::Place { ExprUse::Place } else { ExprUse::Value };
                let operands: SmallVec<[Operand<'hir>; 4]> = exprs
                    .iter()
                    .copied()
                    .flatten()
                    .map(|expr| Operand::Expr(expr, element_use.clone()))
                    .collect();
                self.analyze_operands(&operands, state);
            }
            ExprKind::YulMember(base, member) if matches!(member.as_str(), "slot" | "offset") => {
                self.analyze_expr(base, ExprUse::Place, state)
            }
            ExprKind::Member(base, _) | ExprKind::YulMember(base, _) => self.analyze_expr(
                base,
                if use_ == ExprUse::Place { ExprUse::Place } else { ExprUse::Projection },
                state,
            ),
            ExprKind::Ident(_)
            | ExprKind::Lit(_)
            | ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }

        self.apply_expr_effects(expr, use_, state);
    }

    fn apply_expr_effects(&mut self, expr: &'hir Expr<'hir>, use_: ExprUse, state: &mut A::Domain) {
        let prior_halted = self.expr_halted.take();
        let cx = self.cx();
        if let Some(info) = cx.call_info(expr)
            && (info.function().is_some() || info.is_indirect_internal())
            && info.kind() == CallKind::Internal
        {
            if let Some(function) = info.function() {
                match self.analysis.internal_call_mode(cx, expr, function, state) {
                    InternalCallMode::Skip => {}
                    InternalCallMode::Analyze => {
                        self.analyze_internal_call(expr, function, state, true)
                    }
                    InternalCallMode::AnalyzeWithoutReports => {
                        self.analyze_internal_call(expr, function, state, false)
                    }
                }
            } else {
                let targets = self.gcx.indirect_internal_call_targets(expr);
                self.analyze_indirect_internal_call(expr, &targets, state);
            }
        }
        if self.expr_aborted {
            merge_opt(&mut self.expr_halted, prior_halted);
            return;
        }

        self.analysis.apply_expr_effect(self.cx(), expr, use_, state);
        match self.gcx.call_termination(expr) {
            Some(CallTermination::Revert) => self.expr_aborted = true,
            Some(CallTermination::SuccessfulHalt) => {
                merge_opt(&mut self.expr_halted, Some(state.clone()));
                self.expr_aborted = true;
            }
            None => {}
        }
        merge_opt(&mut self.expr_halted, prior_halted);
    }

    fn analyze_operands(&mut self, operands: &[Operand<'hir>], state: &mut A::Domain) {
        if operands.is_empty() {
            return;
        }
        let prior_halted = self.expr_halted.take();
        match self.analysis.operand_order() {
            OperandOrder::Source => {
                for operand in operands {
                    self.analyze_operand(operand, state);
                    if self.expr_aborted {
                        break;
                    }
                }
            }
            OperandOrder::Unspecified => self.analyze_unordered_operands(operands, state),
        }
        merge_opt(&mut self.expr_halted, prior_halted);
    }

    fn analyze_operand(&mut self, operand: &Operand<'hir>, state: &mut A::Domain) {
        match operand {
            Operand::Expr(expr, use_) => self.analyze_expr(expr, use_.clone(), state),
            Operand::AssignmentValue(target, value) => {
                self.analyze_assignment_value(target, value, state)
            }
            Operand::ConditionTrue(condition) => {
                let (true_state, _) = self.analyze_condition(condition, state.clone());
                if let Some(true_state) = true_state {
                    *state = true_state;
                    self.expr_aborted = false;
                } else {
                    self.expr_aborted = true;
                }
            }
        }
    }

    fn analyze_unordered_operands(&mut self, operands: &[Operand<'hir>], state: &mut A::Domain) {
        // Twelve covers ordinary ABI calls and tuple assignments while bounding the powerset to
        // 4096 states. Larger generated literals use the conservative closure below.
        const MAX_EXACT_OPERANDS: usize = 12;
        if operands.len() > MAX_EXACT_OPERANDS {
            self.analyze_large_unordered_operands(operands, state);
            return;
        }

        let state_count = 1usize << operands.len();
        let mut states = vec![None; state_count];
        states[0] = Some(state.clone());
        let mut halted = None;
        for mask in 0..state_count - 1 {
            let Some(prefix) = states[mask].clone() else { continue };
            for (index, operand) in operands.iter().enumerate() {
                let bit = 1usize << index;
                if mask & bit != 0 {
                    continue;
                }
                let (normal, branch_halted) = self.transfer_operand(operand, prefix.clone());
                merge_opt(&mut halted, branch_halted);
                merge_opt(&mut states[mask | bit], normal);
            }
        }

        self.expr_halted = halted;
        if let Some(exit) = states.pop().flatten() {
            *state = exit;
            self.expr_aborted = false;
        } else {
            self.expr_aborted = true;
        }
    }

    fn analyze_large_unordered_operands(
        &mut self,
        operands: &[Operand<'hir>],
        state: &mut A::Domain,
    ) {
        // A powerset is deliberately avoided for large literals and argument lists. Repeatedly
        // transferring every operand from the joined prefix is a conservative closure: it
        // contains every permutation, plus paths which repeat or omit siblings.
        let mut closure = state.clone();
        let mut halted = None;
        loop {
            let prefix = closure.clone();
            let mut next = closure.clone();
            for operand in operands {
                let (normal, branch_halted) = self.transfer_operand(operand, prefix.clone());
                merge_opt(&mut halted, branch_halted);
                if let Some(normal) = normal {
                    _ = next.join(&normal);
                }
            }
            if !closure.join(&next) {
                break;
            }
        }
        *state = closure;
        self.expr_halted = halted;
        self.expr_aborted = false;
    }

    fn transfer_operand(
        &mut self,
        operand: &Operand<'hir>,
        mut state: A::Domain,
    ) -> (Option<A::Domain>, Option<A::Domain>) {
        debug_assert!(self.expr_halted.is_none());
        self.expr_aborted = false;
        self.analyze_operand(operand, &mut state);
        let halted = self.expr_halted.take();
        let normal = (!self.expr_aborted).then_some(state);
        self.expr_aborted = false;
        (normal, halted)
    }

    fn analyze_condition(
        &mut self,
        condition: &'hir Expr<'hir>,
        mut entry: A::Domain,
    ) -> (Option<A::Domain>, Option<A::Domain>) {
        self.expr_aborted = false;
        let (mut true_state, mut false_state) = match &condition.peel_parens().kind {
            ExprKind::Unary(op, inner) if op.kind == super::UnOpKind::Not => {
                let (true_state, false_state) = self.analyze_condition(inner, entry);
                (false_state, true_state)
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind == super::BinOpKind::And => {
                let (lhs_true, mut false_state) = self.analyze_condition(lhs, entry);
                let mut true_state = None;
                if let Some(lhs_true) = lhs_true {
                    let (rhs_true, rhs_false) = self.analyze_condition(rhs, lhs_true);
                    true_state = rhs_true;
                    merge_opt(&mut false_state, rhs_false);
                }
                (true_state, false_state)
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind == super::BinOpKind::Or => {
                let (mut true_state, lhs_false) = self.analyze_condition(lhs, entry);
                let mut false_state = None;
                if let Some(lhs_false) = lhs_false {
                    let (rhs_true, rhs_false) = self.analyze_condition(rhs, lhs_false);
                    merge_opt(&mut true_state, rhs_true);
                    false_state = rhs_false;
                }
                (true_state, false_state)
            }
            _ => {
                self.analyze_expr(condition, ExprUse::Value, &mut entry);
                if self.expr_aborted {
                    return (None, None);
                }
                match self
                    .gcx
                    .try_eval_const_value(condition)
                    .ok()
                    .and_then(|value| value.as_bool())
                {
                    Some(true) => {
                        self.analysis.apply_condition_effect(
                            self.cx(),
                            condition,
                            true,
                            &mut entry,
                        );
                        (Some(entry), None)
                    }
                    Some(false) => {
                        self.analysis.apply_condition_effect(
                            self.cx(),
                            condition,
                            false,
                            &mut entry,
                        );
                        (None, Some(entry))
                    }
                    None => {
                        let mut true_state = entry.clone();
                        self.analysis.apply_condition_effect(
                            self.cx(),
                            condition,
                            true,
                            &mut true_state,
                        );
                        self.analysis.apply_condition_effect(
                            self.cx(),
                            condition,
                            false,
                            &mut entry,
                        );
                        (Some(true_state), Some(entry))
                    }
                }
            }
        };

        if matches!(
            &condition.peel_parens().kind,
            ExprKind::Unary(op, _) if op.kind == super::UnOpKind::Not
        ) || matches!(
            &condition.peel_parens().kind,
            ExprKind::Binary(_, op, _)
                if matches!(op.kind, super::BinOpKind::And | super::BinOpKind::Or)
        ) {
            self.apply_condition_parent_effect(condition, &mut true_state);
            self.apply_condition_parent_effect(condition, &mut false_state);
        }
        self.expr_aborted = true_state.is_none() && false_state.is_none();
        (true_state, false_state)
    }

    fn apply_condition_parent_effect(
        &mut self,
        condition: &'hir Expr<'hir>,
        state: &mut Option<A::Domain>,
    ) {
        let Some(mut branch) = state.take() else { return };
        self.expr_aborted = false;
        self.apply_expr_effects(condition, ExprUse::Value, &mut branch);
        if !self.expr_aborted {
            *state = Some(branch);
        }
    }

    fn analyze_multi_decl_value(
        &mut self,
        variables: &'hir [Option<VariableId>],
        expr: &'hir Expr<'hir>,
        state: &mut A::Domain,
    ) {
        if let ExprKind::Tuple(values) = &expr.peel_parens().kind {
            let operands: SmallVec<[Operand<'hir>; 4]> = values
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, value)| {
                    let value = value?;
                    let place = variables.get(index).copied().flatten().map(Place::from_local);
                    Some(Operand::Expr(value, ExprUse::Store(place)))
                })
                .collect();
            self.analyze_operands(&operands, state);
            if !self.expr_aborted {
                self.analysis.apply_expr_effect(self.cx(), expr, ExprUse::Store(None), state);
            }
        } else {
            let mut variables = variables.iter().flatten().copied();
            let variable = variables.next();
            self.analyze_expr(
                expr,
                ExprUse::Store(
                    variable.filter(|_| variables.next().is_none()).map(Place::from_local),
                ),
                state,
            );
        }
    }

    fn analyze_assignment_value(
        &mut self,
        target: &'hir Expr<'hir>,
        value: &'hir Expr<'hir>,
        state: &mut A::Domain,
    ) {
        if let ExprKind::Tuple(targets) = &target.peel_parens().kind
            && let ExprKind::Tuple(values) = &value.peel_parens().kind
        {
            let operands: SmallVec<[Operand<'hir>; 4]> = values
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(index, value)| {
                    let value = value?;
                    Some(match targets.get(index).copied().flatten() {
                        Some(target) => Operand::AssignmentValue(target, value),
                        // An omitted lvalue discards the produced value, but the corresponding
                        // rvalue is still evaluated and may have observable effects.
                        None => Operand::Expr(value, ExprUse::Discard),
                    })
                })
                .collect();
            self.analyze_operands(&operands, state);
            if !self.expr_aborted {
                self.analysis.apply_expr_effect(self.cx(), value, ExprUse::Store(None), state);
            }
        } else {
            self.analyze_expr(value, ExprUse::Store(self.gcx.expr_place(target)), state);
        }
    }

    fn analyze_internal_call(
        &mut self,
        call: &'hir Expr<'hir>,
        function_id: FunctionId,
        state: &mut A::Domain,
        enable_reports: bool,
    ) {
        let function = self.gcx.hir.function(function_id);
        let Some(body) = function.body else { return };

        let caller = state.clone();
        self.analysis.apply_call_entry_effect(self.cx(), call, function_id, state);
        let reports_enabled = self.reports_enabled && enable_reports;
        let call_cx = self.cx();
        if let Some(index) = self.call_summaries.iter().position(|summary| {
            summary.function == function_id
                && summary.entry == *state
                && summary.in_loop == call_cx.in_loop()
                && summary.in_enclosing_loop == call_cx.in_enclosing_loop()
        }) {
            self.apply_call_summary(index, call, function_id, &caller, state);
            return;
        }

        let index = self.call_summaries.len();
        self.call_summaries.push(CallSummary {
            function: function_id,
            entry: state.clone(),
            in_loop: call_cx.in_loop(),
            in_enclosing_loop: call_cx.in_enclosing_loop(),
            normal: None,
            halt_: None,
        });
        loop {
            let entry = self.call_summaries[index].entry.clone();
            let exits = self.analyze_call_body(function, body, function_id, entry, false);
            let mut normal = exits.fallthrough;
            merge_opt(&mut normal, exits.return_);

            let old_normal = self.call_summaries[index].normal.clone();
            let old_halt = self.call_summaries[index].halt_.clone();
            merge_opt(&mut self.call_summaries[index].normal, normal);
            merge_opt(&mut self.call_summaries[index].halt_, exits.halt_);
            if old_normal == self.call_summaries[index].normal
                && old_halt == self.call_summaries[index].halt_
            {
                break;
            }
        }
        if reports_enabled {
            let entry = self.call_summaries[index].entry.clone();
            _ = self.analyze_call_body(function, body, function_id, entry, true);
        }
        debug_assert_eq!(index + 1, self.call_summaries.len());
        let summary = self.call_summaries.pop().unwrap();
        self.apply_call_summary_states(
            summary.normal,
            summary.halt_,
            call,
            function_id,
            &caller,
            state,
        );
    }

    fn analyze_indirect_internal_call(
        &mut self,
        call: &'hir Expr<'hir>,
        targets: &super::FunctionValueTargets,
        state: &mut A::Domain,
    ) {
        let entry = state.clone();
        let outer_halted = self.expr_halted.take();
        let mut normal = None;
        let mut halted = None;
        let mut resolved = SmallVec::<[FunctionId; 4]>::new();

        for &known_target in targets.known() {
            let mut target = known_target.function();
            if known_target.requires_virtual_dispatch()
                && let Some(contract) = self.dispatch_contract
            {
                target = self.gcx.function_in_contract(target, contract);
            }
            if resolved.contains(&target) {
                continue;
            }
            resolved.push(target);

            let mut branch = entry.clone();
            self.expr_aborted = false;
            self.expr_halted = None;
            match self.analysis.internal_call_mode(self.cx(), call, target, &branch) {
                InternalCallMode::Skip => {}
                InternalCallMode::Analyze => {
                    self.analyze_internal_call(call, target, &mut branch, true);
                }
                InternalCallMode::AnalyzeWithoutReports => {
                    self.analyze_internal_call(call, target, &mut branch, false);
                }
            }
            merge_opt(&mut halted, self.expr_halted.take());
            if !self.expr_aborted {
                merge_opt(&mut normal, Some(branch));
            }
        }

        if targets.may_be_unknown() {
            let mut branch = entry;
            self.expr_aborted = false;
            self.expr_halted = None;
            self.analysis.apply_indirect_internal_call_effect(self.cx(), call, &mut branch);
            merge_opt(&mut halted, self.expr_halted.take());
            if !self.expr_aborted {
                merge_opt(&mut normal, Some(branch));
            }
        }

        self.expr_halted = outer_halted;
        merge_opt(&mut self.expr_halted, halted);
        if let Some(normal) = normal {
            *state = normal;
            self.expr_aborted = false;
        } else {
            self.expr_aborted = true;
        }
    }

    fn analyze_call_body(
        &mut self,
        function: &'hir Function<'hir>,
        body: Block<'hir>,
        function_id: FunctionId,
        entry: A::Domain,
        reports_enabled: bool,
    ) -> Exits<A::Domain> {
        debug_assert!(self.expr_halted.is_none());
        self.expr_aborted = false;
        let previous_function = self.current_function.replace(function_id);
        let previous_reports = std::mem::replace(&mut self.reports_enabled, reports_enabled);
        self.call_depth += 1;
        self.call_entry_loop_depths.push(self.loop_depth);
        let exits = self.analyze_callable(function, body, Some(function_id), entry);
        self.call_entry_loop_depths.pop();
        self.call_depth -= 1;
        self.reports_enabled = previous_reports;
        self.current_function = previous_function;
        self.expr_aborted = false;
        debug_assert!(self.expr_halted.is_none());
        exits
    }

    fn apply_call_summary(
        &mut self,
        index: usize,
        call: &'hir Expr<'hir>,
        function_id: FunctionId,
        caller: &A::Domain,
        state: &mut A::Domain,
    ) {
        let normal = self.call_summaries[index].normal.clone();
        let halt_ = self.call_summaries[index].halt_.clone();
        self.apply_call_summary_states(normal, halt_, call, function_id, caller, state);
    }

    fn apply_call_summary_states(
        &mut self,
        normal: Option<A::Domain>,
        halt_: Option<A::Domain>,
        call: &'hir Expr<'hir>,
        function_id: FunctionId,
        caller: &A::Domain,
        state: &mut A::Domain,
    ) {
        merge_opt(&mut self.expr_halted, halt_);
        if let Some(normal) = normal {
            *state = normal;
            self.analysis.apply_call_return_effect(self.cx(), call, function_id, caller, state);
            self.expr_aborted = false;
        } else {
            self.expr_aborted = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler;
    use solar_interface::{Session, config::CompileOpts};
    use std::path::PathBuf;

    const SOURCE: &str = r#"
interface Target {
    function ping() external;
    function check() external returns (bool);
}

contract C {
    event Seen();
    uint256 stateValue;

    struct Holder {
        uint256 value;
    }

    function projectionContexts(Holder memory holder, uint256[] memory values)
        external
        pure
        returns (uint256)
    {
        values[0] = holder.value;
        return values[0];
    }

    modifier emitAfterSuffix(Target target) {
        _;
        target.ping();
        emit Seen();
    }

    function recursiveEntry(Target target, bool flag) external {
        if (flag) {
            recursive(target);
            return;
        }
        noOp(target);
        emit Seen();
    }

    function noOp(Target target) internal {
        recursive(target);
    }

    function recursive(Target target) internal {
        noOp(target);
        target.ping();
    }

    function loopFixpoint(Target target) external {
        for (uint256 i; i < 2; ++i) {
            emit Seen();
            target.ping();
        }
    }

    function nestedLoop(Target target) external {
        for (uint256 i; i < 2; ++i) {
            helperLoop(target);
        }
    }

    function helperLoop(Target target) internal {
        target.ping();
        for (uint256 i; i < 2; ++i) {
            target.ping();
        }
    }

    function branchJoin(Target target, bool flag) external {
        if (flag) {
            target.ping();
        }
        emit Seen();
    }

    function abortingCall(Target target) external {
        alwaysAborts(target);
        emit Seen();
    }

    function alwaysAborts(Target target) internal {
        target.ping();
        revert();
    }

    function constantFalse(Target target) external {
        if (false) target.ping();
        emit Seen();
    }

    function constantShortCircuit(Target target) external {
        if (false && target.check()) {}
        if (true || target.check()) {}
        emit Seen();
    }

    function infiniteLoop() external {
        while (true) {}
        emit Seen();
    }

    function forContinuePost(Target target, bool flag) external {
        for (; flag; target.ping()) {
            continue;
        }
        emit Seen();
    }

    function doWhileContinueCondition(Target target) external {
        do {
            continue;
        } while (target.check());
        emit Seen();
    }

    function conditionlessForContinue(Target target) external {
        for (;; target.ping()) {
            emit Seen();
            continue;
        }
    }

    function modifierReturn(Target target) external emitAfterSuffix(target) {
        return;
    }

    function modifierYulStop(Target target) external emitAfterSuffix(target) {
        assembly {
            stop()
        }
    }

    function modifierYulHelperStop(Target target) external emitAfterSuffix(target) {
        assembly {
            function halt() { stop() }
            halt()
        }
    }

    function recursiveBaseCase(Target target, uint256 depth) public {
        if (depth == 0) {
            target.ping();
            return;
        }
        recursiveBaseCase(target, depth - 1);
        emit Seen();
    }

    function recursiveLoopEntry(bool flag) external {
        recursiveLoop(flag);
        emit Seen();
    }

    function recursiveLoop(bool flag) internal {
        while (flag) {
            recursiveLoop(flag);
        }
    }

    function mutualBaseEntry(Target target, uint256 depth) external {
        mutualA(target, depth);
        emit Seen();
    }

    function mutualA(Target target, uint256 depth) internal {
        if (depth == 0) {
            target.ping();
            return;
        }
        mutualB(target, depth - 1);
    }

    function mutualB(Target target, uint256 depth) internal {
        mutualA(target, depth);
    }

    function caughtCall(Target target) external {
        try target.ping() {
            emit Seen();
        } catch {
            emit Seen();
        }
    }

    function omittedTupleCall(Target target) external {
        uint256 value;
        (, value) = (target.check(), 1);
        emit Seen();
    }

    function yulCallBeforeEmit(address target) external {
        assembly {
            pop(call(gas(), target, 0, 0, 0, 0, 0))
        }
        emit Seen();
    }

    function yulStaticCallBeforeEmit(address target) external {
        assembly {
            pop(staticcall(gas(), target, 0, 0, 0, 0))
        }
        emit Seen();
    }

    function yulSwitchCallBeforeEmit(address target, uint256 selector) external {
        assembly {
            switch selector
            case 0 { pop(call(gas(), target, 0, 0, 0, 0, 0)) }
            default {}
        }
        emit Seen();
    }

    function unorderedArguments(Target target) external {
        consume(stateValue = 1, target.check());
    }

    function unorderedTupleAssignment(Target target) external {
        uint256 local;
        bool result;
        (local, result) = (stateValue = 1, target.check());
    }

    function unorderedTupleDeclaration(Target target) external {
        (uint256 local, bool result) = (stateValue = 1, target.check());
    }

    function consume(uint256, bool) internal {}

    function mixedHaltCondition(Target target, bool stop_) external {
        if (mayStop(stop_) && both(target.check(), target.check())) {
            emit Seen();
        }
    }

    function mayStop(bool stop_) internal returns (bool) {
        if (stop_) {
            assembly { stop() }
        }
        return true;
    }

    function both(bool, bool) internal pure returns (bool) {
        return true;
    }
}

contract DispatchBase {
    event Dispatched();

    function hook(Target target) internal virtual {}

    function inherited(Target target) public virtual {
        hook(target);
        emit Dispatched();
    }
}

contract DispatchLeaf is DispatchBase {
    function hook(Target target) internal override {
        target.ping();
    }
}

contract DispatchOverride is DispatchBase {
    function inherited(Target target) public override {
        target.ping();
    }
}

contract PointerRoot {
    event PointerDone();

    function pointerTarget(Target target) internal virtual {
        target.ping();
    }

    function virtualPointer(Target target) public {
        function(Target) internal callback = pointerTarget;
        callback(target);
        emit PointerDone();
    }
}

contract PointerBase is PointerRoot {
    function pointerTarget(Target target) internal virtual override {
        target.ping();
    }

    function superPointer(Target target) public {
        function(Target) internal callback = super.pointerTarget;
        callback(target);
        emit PointerDone();
    }
}

contract PointerLeaf is PointerBase {
    function pointerTarget(Target) internal override {}
}
"#;

    #[derive(Default)]
    struct ExternalCallBeforeEmit {
        emit_states: Vec<bool>,
        loop_contexts: std::collections::BTreeSet<(usize, bool)>,
    }

    impl<'hir> EffectiveFlowAnalysis<'hir> for ExternalCallBeforeEmit {
        type Domain = bool;

        fn operand_order(&self) -> OperandOrder {
            OperandOrder::Source
        }

        fn apply_expr_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            _use_: ExprUse,
            state: &mut Self::Domain,
        ) {
            if cx.call_info(expr).is_some_and(|info| info.is_state_mutating_external_interaction())
            {
                *state = true;
                self.loop_contexts.insert((cx.loop_depth(), cx.in_enclosing_loop()));
            }
        }

        fn apply_statement_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            stmt: &'hir Stmt<'hir>,
            state: &mut Self::Domain,
        ) {
            if matches!(stmt.kind, StmtKind::Emit(_)) && !self.emit_states.contains(state) {
                self.emit_states.push(*state);
            }
        }

        fn apply_indirect_internal_call_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            state: &mut Self::Domain,
        ) {
            *state = true;
        }
    }

    struct ExternalBeforeWrite {
        order: OperandOrder,
        hit: bool,
    }

    #[derive(Default)]
    struct ProjectionContexts {
        uses: Vec<(String, ExprUse)>,
    }

    impl<'hir> EffectiveFlowAnalysis<'hir> for ProjectionContexts {
        type Domain = bool;

        fn operand_order(&self) -> OperandOrder {
            OperandOrder::Source
        }

        fn apply_expr_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            use_: ExprUse,
            _state: &mut Self::Domain,
        ) {
            if matches!(expr.kind, ExprKind::Ident(_)) {
                self.uses
                    .push((cx.gcx().sess.source_map().span_to_snippet(expr.span).unwrap(), use_));
            }
        }

        fn apply_indirect_internal_call_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            _state: &mut Self::Domain,
        ) {
        }
    }

    impl<'hir> EffectiveFlowAnalysis<'hir> for ExternalBeforeWrite {
        type Domain = bool;

        fn operand_order(&self) -> OperandOrder {
            self.order
        }

        fn apply_expr_effect(
            &mut self,
            cx: EffectiveBodyCx<'hir>,
            expr: &'hir Expr<'hir>,
            _use_: ExprUse,
            state: &mut Self::Domain,
        ) {
            if cx.call_info(expr).is_some_and(|info| info.is_external_interaction()) {
                *state = true;
            }
            if matches!(expr.kind, ExprKind::Assign(..))
                && let ExprKind::Assign(target, ..) = &expr.kind
                && !cx.gcx().assigned_state_variables(target).is_empty()
                && *state
            {
                self.hit = true;
            }
        }

        fn apply_indirect_internal_call_effect(
            &mut self,
            _cx: EffectiveBodyCx<'hir>,
            _call: &'hir Expr<'hir>,
            state: &mut Self::Domain,
        ) {
            *state = true;
        }
    }

    #[test]
    fn handles_recursion_joins_fixpoints_and_aborts() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            pcx.parse();

            assert_eq!(c.lower_asts(), Ok(std::ops::ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(std::ops::ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let analyze = |name: &str| {
                let function_id = gcx
                    .hir
                    .function_ids()
                    .find(|&id| gcx.item_canonical_name(id).to_string() == name)
                    .unwrap();
                let mut analysis = ExternalCallBeforeEmit::default();
                analyze_effective_body_flow(gcx, function_id, false, &mut analysis);
                analysis
            };

            assert!(analyze("C.recursiveEntry").emit_states.is_empty());
            assert_eq!(analyze("C.branchJoin").emit_states, [true]);
            assert_eq!(analyze("C.loopFixpoint").emit_states, [false, true]);
            assert!(analyze("C.abortingCall").emit_states.is_empty());
            assert_eq!(analyze("C.constantFalse").emit_states, [false]);
            assert_eq!(analyze("C.constantShortCircuit").emit_states, [false]);
            assert!(analyze("C.infiniteLoop").emit_states.is_empty());
            assert_eq!(analyze("C.forContinuePost").emit_states, [true]);
            assert_eq!(analyze("C.doWhileContinueCondition").emit_states, [true]);
            assert_eq!(analyze("C.conditionlessForContinue").emit_states, [false, true]);
            assert_eq!(analyze("C.modifierReturn").emit_states, [true]);
            assert!(analyze("C.modifierYulStop").emit_states.is_empty());
            assert!(analyze("C.modifierYulHelperStop").emit_states.is_empty());
            assert_eq!(analyze("C.recursiveBaseCase").emit_states, [true]);
            assert_eq!(analyze("C.recursiveLoopEntry").emit_states, [false]);
            assert_eq!(analyze("C.mutualBaseEntry").emit_states, [true]);
            assert_eq!(analyze("C.caughtCall").emit_states, [true, false]);
            assert_eq!(analyze("C.omittedTupleCall").emit_states, [true]);
            assert_eq!(analyze("C.yulCallBeforeEmit").emit_states, [true]);
            assert_eq!(analyze("C.yulStaticCallBeforeEmit").emit_states, [false]);
            assert_eq!(analyze("C.yulSwitchCallBeforeEmit").emit_states, [true]);
            assert_eq!(analyze("C.mixedHaltCondition").emit_states, [true]);
            assert_eq!(
                analyze("C.nestedLoop").loop_contexts,
                [(1, true), (2, false)].into_iter().collect()
            );

            let inherited_id = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "DispatchBase.inherited")
                .unwrap();
            let mut dispatch_analysis = ExternalCallBeforeEmit::default();
            let results = analyze_effective_body_flow_dispatches(
                gcx,
                inherited_id,
                false,
                &mut dispatch_analysis,
            );
            assert_eq!(results.len(), 2);
            assert_eq!(dispatch_analysis.emit_states, [false, true]);

            let projection_contexts = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.projectionContexts")
                .unwrap();
            let mut projection_analysis = ProjectionContexts::default();
            let _ = analyze_effective_body_flow(
                gcx,
                projection_contexts,
                false,
                &mut projection_analysis,
            );
            assert_eq!(
                projection_analysis.uses,
                [
                    ("values".to_owned(), ExprUse::Place),
                    ("holder".to_owned(), ExprUse::Projection),
                    ("values".to_owned(), ExprUse::Projection),
                ]
            );

            let pointer_base = gcx
                .hir
                .contract_ids()
                .find(|&contract| gcx.hir.contract(contract).name.name.as_str() == "PointerBase")
                .unwrap();
            let pointer_leaf = gcx
                .hir
                .contract_ids()
                .find(|&contract| gcx.hir.contract(contract).name.name.as_str() == "PointerLeaf")
                .unwrap();
            let analyze_pointer = |function_name: &str, contract| {
                let function = gcx
                    .hir
                    .function_ids()
                    .find(|&id| gcx.item_canonical_name(id).to_string() == function_name)
                    .unwrap();
                let mut analysis = ExternalCallBeforeEmit::default();
                analyze_effective_body_flow_in_contract(
                    gcx,
                    function,
                    contract,
                    false,
                    &mut analysis,
                );
                analysis.emit_states
            };
            assert_eq!(analyze_pointer("PointerRoot.virtualPointer", pointer_base), [true]);
            assert_eq!(analyze_pointer("PointerRoot.virtualPointer", pointer_leaf), [false]);
            assert_eq!(analyze_pointer("PointerBase.superPointer", pointer_base), [true]);
            assert_eq!(analyze_pointer("PointerBase.superPointer", pointer_leaf), [true]);
            let root_target = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "PointerRoot.pointerTarget")
                .unwrap();
            assert!(
                gcx.function_reference_index()
                    .references_to(root_target)
                    .iter()
                    .any(|reference| reference.kind == super::super::FunctionReferenceKind::Value)
            );
        });
    }

    #[test]
    fn optionally_joins_unspecified_sibling_operand_orders() {
        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            pcx.parse();
            assert_eq!(c.lower_asts(), Ok(std::ops::ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(std::ops::ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            for name in [
                "C.unorderedArguments",
                "C.unorderedTupleAssignment",
                "C.unorderedTupleDeclaration",
            ] {
                let function = gcx
                    .hir
                    .function_ids()
                    .find(|&id| gcx.item_canonical_name(id).to_string() == name)
                    .unwrap();
                let mut source = ExternalBeforeWrite { order: OperandOrder::Source, hit: false };
                analyze_effective_body_flow(gcx, function, false, &mut source);
                assert!(!source.hit, "{name}");

                let mut unspecified =
                    ExternalBeforeWrite { order: OperandOrder::Unspecified, hit: false };
                analyze_effective_body_flow(gcx, function, false, &mut unspecified);
                assert!(unspecified.hit, "{name}");
            }
        });
    }
}
