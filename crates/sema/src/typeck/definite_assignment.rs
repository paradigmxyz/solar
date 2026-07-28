//! Definite-assignment analysis for local storage pointers.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, StmtKind},
    ty::Gcx,
};
use rayon::prelude::*;
use solar_ast::{BinOpKind, DataLocation, UnOpKind};
use solar_data_structures::bit_set::GrowableBitSet;
use solar_interface::{
    diagnostics::{Diag, Level},
    error_code,
};

type State = GrowableBitSet<hir::VariableId>;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }

    let diagnostics = gcx
        .hir
        .par_functions()
        .map(|function| DefiniteAssignment::new(gcx).check_function(function))
        .collect::<Vec<_>>();
    for diagnostic in diagnostics.into_iter().flatten() {
        let _ = gcx.dcx().emit_diagnostic(diagnostic);
    }
}

struct DefiniteAssignment<'gcx> {
    gcx: Gcx<'gcx>,
    diagnostics: Vec<Diag>,
}

impl<'gcx> DefiniteAssignment<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, diagnostics: Vec::new() }
    }

    fn check_function(mut self, function: &'gcx hir::Function<'gcx>) -> Vec<Diag> {
        let Some(body) = function.body else { return self.diagnostics };

        let mut state = State::new_empty();
        for &parameter in function.parameters {
            if self.is_storage_pointer(parameter) {
                state.insert(parameter);
            }
        }
        let flow = self.analyze_block(body, state);
        if function.modifiers.is_empty()
            && let Some(exit_state) = merge_states(flow.normal, flow.returns)
        {
            for &return_variable in function.returns {
                if self.is_storage_pointer(return_variable) && !exit_state.contains(return_variable)
                {
                    let mut diagnostic = Diag::new(
                        Level::Error,
                        "storage pointer variable can be returned before assignment",
                    );
                    diagnostic
                        .code(error_code!(3464))
                        .span(self.gcx.hir.variable(return_variable).span);
                    self.diagnostics.push(diagnostic);
                }
            }
        }
        self.diagnostics
    }

    fn is_storage_pointer(&self, variable: hir::VariableId) -> bool {
        let declaration = self.gcx.hir.variable(variable);
        declaration.is_local_variable()
            && self.gcx.type_of_item(variable.into()).data_stored_in(DataLocation::Storage)
    }

    fn analyze_block(&mut self, block: hir::Block<'gcx>, state: State) -> Flow {
        self.analyze_statements(block.stmts, state)
    }

    fn analyze_statements(&mut self, statements: &'gcx [hir::Stmt<'gcx>], state: State) -> Flow {
        let mut flow = Flow::normal(state);
        for statement in statements {
            let Some(state) = flow.normal.take() else { break };
            let statement_flow = self.analyze_stmt(statement, state);
            merge_optional_state(&mut flow.breaks, statement_flow.breaks);
            merge_optional_state(&mut flow.continues, statement_flow.continues);
            merge_optional_state(&mut flow.returns, statement_flow.returns);
            flow.normal = statement_flow.normal;
        }
        flow
    }

    fn analyze_stmt(&mut self, statement: &'gcx hir::Stmt<'gcx>, state: State) -> Flow {
        match statement.kind {
            StmtKind::DeclSingle(variable) => {
                let declaration = self.gcx.hir.variable(variable);
                let state = if let Some(initializer) = declaration.initializer {
                    self.analyze_expr(initializer, state).merged()
                } else {
                    state
                };
                let mut state = state;
                if declaration.initializer.is_some() && self.is_storage_pointer(variable) {
                    state.insert(variable);
                }
                Flow::normal(state)
            }
            StmtKind::DeclMulti(variables, initializer) => {
                let mut state = self.analyze_expr(initializer, state).merged();
                for &variable in variables.iter().flatten() {
                    if self.is_storage_pointer(variable) {
                        state.insert(variable);
                    }
                }
                Flow::normal(state)
            }
            StmtKind::Block(block)
            | StmtKind::UncheckedBlock(block)
            | StmtKind::AssemblyBlock(block) => self.analyze_block(block, state),
            StmtKind::Emit(expr) => Flow::normal(self.analyze_expr(expr, state).merged()),
            StmtKind::Revert(expr) => {
                self.analyze_expr(expr, state);
                Flow::default()
            }
            StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.analyze_expr(expr, state);
                    Flow::default()
                } else {
                    Flow { returns: Some(state), ..Flow::default() }
                }
            }
            StmtKind::Break => Flow { breaks: Some(state), ..Flow::default() },
            StmtKind::Continue => Flow { continues: Some(state), ..Flow::default() },
            StmtKind::Loop(block, hir::LoopSource::DoWhile) => self.analyze_do_while(block, state),
            StmtKind::Loop(block, source) => {
                let entry = state.clone();
                let body = self.analyze_block(block, state);
                let normal = if source == hir::LoopSource::For && !loop_has_condition(block) {
                    Some(entry)
                } else {
                    body.breaks
                };
                Flow { normal, returns: body.returns, ..Flow::default() }
            }
            StmtKind::If(condition, then_statement, else_statement) => {
                let condition = self.analyze_expr(condition, state);
                let then_flow = self.analyze_stmt(then_statement, condition.when_true);
                let else_flow = if let Some(else_statement) = else_statement {
                    self.analyze_stmt(else_statement, condition.when_false)
                } else {
                    Flow::normal(condition.when_false)
                };
                Flow::merge(then_flow, else_flow)
            }
            StmtKind::Switch(switch) => {
                let state = self.analyze_expr(switch.selector, state).merged();
                let mut result = Flow::default();
                let mut has_default = false;
                for case in switch.cases {
                    has_default |= case.constant.is_none();
                    result = Flow::merge(result, self.analyze_block(case.body, state.clone()));
                }
                if !has_default {
                    merge_optional_state(&mut result.normal, Some(state));
                }
                result
            }
            StmtKind::Try(try_statement) => {
                let state = self.analyze_expr(&try_statement.expr, state).merged();
                let mut result = Flow::default();
                for clause in try_statement.clauses {
                    let mut clause_state = state.clone();
                    for &argument in clause.args {
                        if self.is_storage_pointer(argument) {
                            clause_state.insert(argument);
                        }
                    }
                    result = Flow::merge(result, self.analyze_block(clause.block, clause_state));
                }
                result
            }
            StmtKind::Expr(expr) => {
                let state = self.analyze_expr(expr, state).merged();
                if self.expr_terminates(expr) { Flow::default() } else { Flow::normal(state) }
            }
            StmtKind::Placeholder | StmtKind::Err(_) => Flow::normal(state),
        }
    }

    fn analyze_do_while(&mut self, block: hir::Block<'gcx>, state: State) -> Flow {
        // The final statement is the condition check synthesized by HIR
        // lowering. A source-level `continue` reaches that check too.
        let Some((condition, body)) = block.stmts.split_last() else {
            return Flow::default();
        };
        let body = self.analyze_statements(body, state);
        let mut exits = body.breaks;
        let mut returns = body.returns;
        let condition_entry = merge_states(body.normal, body.continues);
        if let Some(condition_entry) = condition_entry {
            let condition = self.analyze_stmt(condition, condition_entry);
            merge_optional_state(&mut exits, condition.breaks);
            merge_optional_state(&mut returns, condition.returns);
        }
        Flow { normal: exits, returns, ..Flow::default() }
    }

    fn analyze_expr(&mut self, expr: &'gcx hir::Expr<'gcx>, state: State) -> ExprFlow {
        match expr.kind {
            ExprKind::Assign(lhs, op, rhs) => {
                let mut state = self.analyze_expr(rhs, state).merged();
                if op.is_none() {
                    self.analyze_write(lhs, &mut state);
                } else {
                    state = self.analyze_expr(lhs, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind == BinOpKind::And => {
                let lhs = self.analyze_expr(lhs, state);
                let rhs = self.analyze_expr(rhs, lhs.when_true);
                ExprFlow {
                    when_true: rhs.when_true,
                    when_false: intersect_states(lhs.when_false, rhs.when_false),
                }
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind == BinOpKind::Or => {
                let lhs = self.analyze_expr(lhs, state);
                let rhs = self.analyze_expr(rhs, lhs.when_false);
                ExprFlow {
                    when_true: intersect_states(lhs.when_true, rhs.when_true),
                    when_false: rhs.when_false,
                }
            }
            ExprKind::Binary(lhs, _, rhs) => {
                let state = self.analyze_expr(lhs, state).merged();
                ExprFlow::both(self.analyze_expr(rhs, state).merged())
            }
            ExprKind::Unary(op, operand) if op.kind == UnOpKind::Not => {
                let operand = self.analyze_expr(operand, state);
                ExprFlow { when_true: operand.when_false, when_false: operand.when_true }
            }
            ExprKind::Unary(_, operand)
            | ExprKind::Delete(operand)
            | ExprKind::Member(operand, _)
            | ExprKind::Payable(operand)
            | ExprKind::YulMember(operand, _) => {
                ExprFlow::both(self.analyze_expr(operand, state).merged())
            }
            ExprKind::Ternary(condition, then_expr, else_expr) => {
                let condition = self.analyze_expr(condition, state);
                let then_expr = self.analyze_expr(then_expr, condition.when_true);
                let else_expr = self.analyze_expr(else_expr, condition.when_false);
                ExprFlow {
                    when_true: intersect_states(then_expr.when_true, else_expr.when_true),
                    when_false: intersect_states(then_expr.when_false, else_expr.when_false),
                }
            }
            ExprKind::Call(callee, ref args, options) => {
                let mut state = self.analyze_expr(callee, state).merged();
                if let Some(options) = options {
                    for option in options.args {
                        state = self.analyze_expr(&option.value, state).merged();
                    }
                }
                for argument in args.exprs() {
                    state = self.analyze_expr(argument, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Index(base, index) => {
                let mut state = self.analyze_expr(base, state).merged();
                if let Some(index) = index {
                    state = self.analyze_expr(index, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Slice(base, start, end) => {
                let mut state = self.analyze_expr(base, state).merged();
                if let Some(start) = start {
                    state = self.analyze_expr(start, state).merged();
                }
                if let Some(end) = end {
                    state = self.analyze_expr(end, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Array(elements) => {
                let mut state = state;
                for element in elements {
                    state = self.analyze_expr(element, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Tuple(elements) => {
                let mut state = state;
                for element in elements.iter().flatten() {
                    state = self.analyze_expr(element, state).merged();
                }
                ExprFlow::both(state)
            }
            ExprKind::Ident(_) => {
                self.check_read(expr, &state);
                ExprFlow::both(state)
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => ExprFlow::both(state),
        }
    }

    fn analyze_write(&mut self, expr: &'gcx hir::Expr<'gcx>, state: &mut State) {
        if let Some(variable) = self.gcx.resolved_variable(expr)
            && self.is_storage_pointer(variable)
        {
            state.insert(variable);
            return;
        }
        match expr.kind {
            ExprKind::Tuple(elements) => {
                for element in elements.iter().flatten() {
                    self.analyze_write(element, state);
                }
            }
            ExprKind::YulMember(base, member)
                if member.name == solar_interface::sym::slot
                    && let Some(variable) = self.gcx.resolved_variable(base)
                    && self.is_storage_pointer(variable) =>
            {
                state.insert(variable);
            }
            _ => {
                *state = self.analyze_expr(expr, state.clone()).merged();
            }
        }
    }

    fn check_read(&mut self, expr: &hir::Expr<'_>, state: &State) {
        let Some(variable) = self.gcx.resolved_variable(expr) else { return };
        if !self.is_storage_pointer(variable) || state.contains(variable) {
            return;
        }

        let mut diagnostic =
            Diag::new(Level::Error, "storage pointer variable can be accessed before assignment");
        diagnostic
            .code(error_code!(3464))
            .span(expr.span)
            .span_note(self.gcx.hir.variable(variable).span, "storage pointer declared here");
        self.diagnostics.push(diagnostic);
    }

    fn expr_terminates(&self, expr: &hir::Expr<'_>) -> bool {
        let ExprKind::Call(callee, _, _) = expr.kind else { return false };
        matches!(
            self.gcx.resolved_builtin(callee),
            Some(
                Builtin::Revert
                    | Builtin::RevertMsg
                    | Builtin::YulInvalid
                    | Builtin::YulReturn
                    | Builtin::YulRevert
                    | Builtin::YulSelfdestruct
                    | Builtin::YulStop
            )
        )
    }
}

#[derive(Default)]
struct Flow {
    normal: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
    returns: Option<State>,
}

impl Flow {
    fn normal(state: State) -> Self {
        Self { normal: Some(state), ..Self::default() }
    }

    fn merge(mut left: Self, right: Self) -> Self {
        merge_optional_state(&mut left.normal, right.normal);
        merge_optional_state(&mut left.breaks, right.breaks);
        merge_optional_state(&mut left.continues, right.continues);
        merge_optional_state(&mut left.returns, right.returns);
        left
    }
}

struct ExprFlow {
    when_true: State,
    when_false: State,
}

impl ExprFlow {
    fn both(state: State) -> Self {
        Self { when_true: state.clone(), when_false: state }
    }

    fn merged(self) -> State {
        intersect_states(self.when_true, self.when_false)
    }
}

fn intersect_states(mut left: State, right: State) -> State {
    left.intersect(&right);
    left
}

fn merge_states(left: Option<State>, right: Option<State>) -> Option<State> {
    let mut left = left;
    merge_optional_state(&mut left, right);
    left
}

fn merge_optional_state(target: &mut Option<State>, state: Option<State>) {
    match (target.as_mut(), state) {
        (Some(target), Some(state)) => {
            target.intersect(&state);
        }
        (None, Some(state)) => *target = Some(state),
        _ => {}
    }
}

/// Recognizes the condition guard synthesized around a normalized `for` body.
fn loop_has_condition(block: hir::Block<'_>) -> bool {
    let [statement] = block.stmts else { return false };
    let StmtKind::If(_, _, Some(else_statement)) = statement.kind else { return false };
    matches!(else_statement.kind, StmtKind::Break)
}
