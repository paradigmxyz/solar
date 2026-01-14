//! Static analyzer for detecting common issues and emitting warnings.
//!
//! This pass detects:
//! - Unused local variables and function parameters
//! - Shadowing of state variables
//! - Unreachable code after return/revert
//! - Division/modulo by zero
//! - Self-assignment
//! - Empty blocks
//! - Boolean constant comparisons
//! - Assert/require without message

use crate::{
    builtins::Builtin,
    eval::ConstantEvaluator,
    hir::{self, Visit},
    ty::Gcx,
};
use solar_ast::{BinOpKind, Span};
use solar_data_structures::{map::FxHashMap, Never};
use solar_interface::{diagnostics::DiagCtxt, error_code};
use std::ops::ControlFlow;

/// Run static analysis on the given source.
pub(super) fn analyze(gcx: Gcx<'_>, source: hir::SourceId) {
    let mut analyzer = StaticAnalyzer::new(gcx);
    let _ = analyzer.visit_nested_source(source);
}

/// Static analyzer that detects common issues.
struct StaticAnalyzer<'gcx> {
    gcx: Gcx<'gcx>,

    /// The current contract being analyzed, if any.
    current_contract: Option<hir::ContractId>,

    /// The current function being analyzed, if any.
    current_function: Option<hir::FunctionId>,

    /// Whether we're inside a constructor.
    in_constructor: bool,

    /// Tracks local variable usage: (variable_id) -> use_count.
    /// Only for local variables within the current function.
    local_var_uses: FxHashMap<hir::VariableId, usize>,

    /// Whether we've seen a terminating statement (return, revert, etc).
    /// Used to detect unreachable code.
    terminated: bool,
}

impl<'gcx> StaticAnalyzer<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            current_contract: None,
            current_function: None,
            in_constructor: false,
            local_var_uses: FxHashMap::default(),
            terminated: false,
        }
    }

    fn dcx(&self) -> &'gcx DiagCtxt {
        self.gcx.dcx()
    }

    /// Records usage of a variable if it's a local variable.
    fn record_var_use(&mut self, var_id: hir::VariableId) {
        if let Some(count) = self.local_var_uses.get_mut(&var_id) {
            *count += 1;
        }
    }

    /// Registers a local variable for tracking.
    fn register_local_var(&mut self, var_id: hir::VariableId) {
        let var = self.gcx.hir.variable(var_id);
        if var.name.is_some() && var.is_local_variable() {
            self.local_var_uses.entry(var_id).or_insert(0);
        }
    }

    /// Check for unused local variables at the end of a function.
    fn check_unused_variables(&self, func: &hir::Function<'_>) {
        if func.body.is_none() {
            return;
        }

        for (&var_id, &count) in &self.local_var_uses {
            if count == 0 {
                let var = self.gcx.hir.variable(var_id);
                if let Some(name) = var.name {
                    let msg = if var.is_callable_or_catch_parameter() {
                        let kind = if var.is_try_catch_parameter() {
                            "try/catch"
                        } else {
                            "function"
                        };
                        format!(
                            "unused {kind} parameter `{name}`. Remove or comment out the variable name to silence this warning."
                        )
                    } else if var.is_return_parameter() {
                        format!("unused return variable `{name}`")
                    } else {
                        format!("unused local variable `{name}`")
                    };
                    self.dcx().warn(msg).span(var.span).code(error_code!(2072)).emit();
                }
            }
        }
    }

    /// Check for shadowing of state variables.
    fn check_shadowing(&self, var_id: hir::VariableId) {
        let var = self.gcx.hir.variable(var_id);
        let Some(name) = var.name else { return };

        // Only check local variables and parameters
        if !var.is_local_variable() {
            return;
        }

        // Check if this shadows a state variable in the current contract
        if let Some(contract_id) = self.current_contract {
            for base_id in self.gcx.hir.contract(contract_id).linearized_bases {
                let base = self.gcx.hir.contract(*base_id);
                for state_var_id in base.variables() {
                    let state_var = self.gcx.hir.variable(state_var_id);
                    if let Some(state_name) = state_var.name
                        && state_name.name == name.name
                        && state_var.is_state_variable()
                    {
                        self.dcx()
                            .warn(format!(
                                "local variable `{name}` shadows a state variable"
                            ))
                            .span(var.span)
                            .code(error_code!(2519))
                            .span_note(state_var.span, "state variable declared here")
                            .emit();
                        return;
                    }
                }
            }
        }
    }

    /// Check division/modulo by zero in binary operations.
    fn check_division_by_zero(&self, expr: &hir::Expr<'_>, op: BinOpKind, rhs: &hir::Expr<'_>) {
        if !matches!(op, BinOpKind::Div | BinOpKind::Rem) {
            return;
        }

        // Try to evaluate the RHS as a constant
        let mut evaluator = ConstantEvaluator::new(self.gcx);
        if let Ok(value) = evaluator.try_eval(rhs)
            && value.data.is_zero()
        {
            let msg = if op == BinOpKind::Div { "division by zero" } else { "modulo zero" };
            self.dcx().err(msg).span(expr.span).code(error_code!(1211)).emit();
        }
    }

    /// Check for self-assignment (x = x).
    fn check_self_assignment(&self, lhs: &hir::Expr<'_>, rhs: &hir::Expr<'_>) {
        // Check if both sides are simple identifiers referring to the same variable
        let hir::ExprKind::Ident(lhs_res) = &lhs.peel_parens().kind else { return };
        let hir::ExprKind::Ident(rhs_res) = &rhs.peel_parens().kind else { return };

        // Both must resolve to a single variable
        let [lhs_res] = lhs_res else { return };
        let [rhs_res] = rhs_res else { return };

        let Some(lhs_var) = lhs_res.as_variable() else { return };
        let Some(rhs_var) = rhs_res.as_variable() else { return };

        if lhs_var == rhs_var {
            let var = self.gcx.hir.variable(lhs_var);
            let name = var.name.map(|n| n.to_string()).unwrap_or_else(|| "<unknown>".into());
            self.dcx()
                .warn(format!("self-assignment of `{name}` has no effect"))
                .span(lhs.span.to(rhs.span))
                .code(error_code!(7324))
                .emit();
        }
    }

    /// Check for boolean constant comparisons like `x == true` or `x == false`.
    fn check_boolean_constant_comparison(
        &self,
        expr: &hir::Expr<'_>,
        lhs: &hir::Expr<'_>,
        op: BinOpKind,
        rhs: &hir::Expr<'_>,
    ) {
        if !matches!(op, BinOpKind::Eq | BinOpKind::Ne) {
            return;
        }

        let is_bool_lit = |e: &hir::Expr<'_>| -> Option<bool> {
            if let hir::ExprKind::Lit(lit) = &e.peel_parens().kind
                && let solar_ast::LitKind::Bool(b) = lit.kind
            {
                return Some(b);
            }
            None
        };

        let (bool_val, _other) = if let Some(b) = is_bool_lit(lhs) {
            (b, rhs)
        } else if let Some(b) = is_bool_lit(rhs) {
            (b, lhs)
        } else {
            return;
        };

        let suggestion = if (bool_val && op == BinOpKind::Eq) || (!bool_val && op == BinOpKind::Ne) {
            "expression can be simplified to just the boolean expression"
        } else {
            "expression can be simplified using `!`"
        };

        self.dcx()
            .warn("comparison with boolean constant")
            .span(expr.span)
            .code(error_code!(6326))
            .help(suggestion)
            .emit();
    }

    /// Check for empty blocks.
    /// Note: Empty function bodies are quite common in Solidity (interfaces, abstract contracts),
    /// so we don't warn on empty function blocks. This check is kept for potential future use.
    #[allow(dead_code)]
    fn check_empty_block(&self, _block: &hir::Block<'_>, _context: &str) {
        // Disabled for now as empty function bodies are very common in Solidity
        // and warning on them would be too noisy.
    }

    /// Check for unreachable code warning.
    fn warn_unreachable(&self, span: Span) {
        self.dcx().warn("unreachable code").span(span).code(error_code!(5765)).emit();
    }

    /// Check addmod/mulmod for modulo zero.
    fn check_addmod_mulmod_zero(&self, expr: &hir::Expr<'_>, callee: &hir::Expr<'_>, args: &[&hir::Expr<'_>]) {
        // Check if callee is addmod or mulmod
        let hir::ExprKind::Ident(res) = &callee.kind else { return };
        let [res] = res else { return };
        let hir::Res::Builtin(builtin) = res else { return };

        let is_mod_builtin = matches!(builtin, Builtin::AddMod | Builtin::MulMod);
        if !is_mod_builtin {
            return;
        }

        // Third argument is the modulo
        if args.len() != 3 {
            return;
        }

        let mut evaluator = ConstantEvaluator::new(self.gcx);
        if let Ok(value) = evaluator.try_eval(args[2])
            && value.data.is_zero()
        {
            self.dcx()
                .err("arithmetic modulo zero")
                .span(expr.span)
                .code(error_code!(4195))
                .emit();
        }
    }

    /// Check for assert/require without message.
    fn check_assert_require_message(&self, callee: &hir::Expr<'_>, args_len: usize) {
        let hir::ExprKind::Ident(resolutions) = &callee.kind else { return };

        // Check if any resolution is Assert or Require (handles overloaded builtins)
        for res in *resolutions {
            let hir::Res::Builtin(builtin) = res else { continue };

            match builtin {
                Builtin::Assert => {
                    if args_len == 1 {
                        self.dcx()
                            .warn("assertion without description")
                            .span(callee.span)
                            .code(error_code!(5765))
                            .help("consider adding a description string as second argument")
                            .emit();
                        return;
                    }
                }
                Builtin::Require => {
                    if args_len == 1 {
                        self.dcx()
                            .warn("require without error message")
                            .span(callee.span)
                            .code(error_code!(5765))
                            .help("consider adding an error message as second argument")
                            .emit();
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// Check for statement with no effect.
    fn check_statement_no_effect(&self, expr: &hir::Expr<'_>) {
        // Pure expressions have no effect when used as statements
        if self.is_pure_expression(expr) {
            self.dcx()
                .warn("statement has no effect")
                .span(expr.span)
                .code(error_code!(6133))
                .emit();
        }
    }

    /// Returns true if the expression is pure (has no side effects).
    fn is_pure_expression(&self, expr: &hir::Expr<'_>) -> bool {
        match &expr.kind {
            hir::ExprKind::Lit(_) => true,
            hir::ExprKind::Ident(_) => true,
            hir::ExprKind::Binary(lhs, _op, rhs) => {
                self.is_pure_expression(lhs) && self.is_pure_expression(rhs)
            }
            hir::ExprKind::Unary(op, inner) => {
                // Increment/decrement operators have side effects
                !matches!(
                    op.kind,
                    hir::UnOpKind::PreInc
                        | hir::UnOpKind::PreDec
                        | hir::UnOpKind::PostInc
                        | hir::UnOpKind::PostDec
                )
                    && self.is_pure_expression(inner)
            }
            hir::ExprKind::Tuple(exprs) => {
                exprs.iter().flatten().all(|e| self.is_pure_expression(e))
            }
            hir::ExprKind::Array(exprs) => exprs.iter().all(|e| self.is_pure_expression(e)),
            hir::ExprKind::Member(base, _) => self.is_pure_expression(base),
            hir::ExprKind::Index(base, index) => {
                self.is_pure_expression(base)
                    && index.map(|i| self.is_pure_expression(i)).unwrap_or(true)
            }
            hir::ExprKind::Ternary(cond, t, f) => {
                self.is_pure_expression(cond)
                    && self.is_pure_expression(t)
                    && self.is_pure_expression(f)
            }
            // Calls, assignments, delete, etc. are not pure
            _ => false,
        }
    }
}

impl<'gcx> Visit<'gcx> for StaticAnalyzer<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_contract(&mut self, contract: &'gcx hir::Contract<'gcx>) -> ControlFlow<Never> {
        let prev_contract = self.current_contract;
        self.current_contract = Some(
            self.gcx
                .hir
                .contract_ids()
                .find(|&id| std::ptr::eq(self.gcx.hir.contract(id), contract))
                .unwrap(),
        );

        // Visit all contract items
        for base in contract.bases_args {
            self.visit_modifier(base)?;
        }
        for &item_id in contract.items {
            self.visit_nested_item(item_id)?;
        }

        self.current_contract = prev_contract;
        ControlFlow::Continue(())
    }

    fn visit_function(&mut self, func: &'gcx hir::Function<'gcx>) -> ControlFlow<Never> {
        let prev_function = self.current_function;
        let prev_in_constructor = self.in_constructor;

        self.current_function = Some(
            self.gcx
                .hir
                .function_ids()
                .find(|&id| std::ptr::eq(self.gcx.hir.function(id), func))
                .unwrap(),
        );
        self.in_constructor = func.is_constructor();
        self.local_var_uses.clear();
        self.terminated = false;

        // Register parameters and return variables
        for &param in func.parameters {
            self.register_local_var(param);
            self.check_shadowing(param);
        }
        for &ret in func.returns {
            self.register_local_var(ret);
            self.check_shadowing(ret);
        }

        // Visit modifiers
        for modifier in func.modifiers {
            self.visit_modifier(modifier)?;
        }

        // Visit body
        if let Some(body) = &func.body {
            for stmt in body.stmts {
                self.visit_stmt(stmt)?;
            }
        }

        // Check for unused variables
        self.check_unused_variables(func);

        self.current_function = prev_function;
        self.in_constructor = prev_in_constructor;
        ControlFlow::Continue(())
    }

    fn visit_var(&mut self, var: &'gcx hir::Variable<'gcx>) -> ControlFlow<Never> {
        // Visit type and initializer
        self.visit_ty(&var.ty)?;
        if let Some(init) = var.initializer {
            self.visit_expr(init)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Never> {
        // Check for unreachable code
        if self.terminated {
            self.warn_unreachable(stmt.span);
            // Continue analyzing anyway to find more issues
        }

        match &stmt.kind {
            hir::StmtKind::DeclSingle(var_id) => {
                self.register_local_var(*var_id);
                self.check_shadowing(*var_id);
                let var = self.gcx.hir.variable(*var_id);
                if let Some(init) = var.initializer {
                    self.visit_expr(init)?;
                }
            }
            hir::StmtKind::DeclMulti(vars, init) => {
                for &var_id in vars.iter().flatten() {
                    self.register_local_var(var_id);
                    self.check_shadowing(var_id);
                }
                self.visit_expr(init)?;
            }
            hir::StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr)?;
                    // Return with expression marks return variables as "used"
                    if let Some(func_id) = self.current_function {
                        let func = self.gcx.hir.function(func_id);
                        for &ret in func.returns {
                            self.record_var_use(ret);
                        }
                    }
                }
                self.terminated = true;
            }
            hir::StmtKind::Revert(expr) => {
                self.visit_expr(expr)?;
                self.terminated = true;
            }
            hir::StmtKind::Block(block) => {
                let prev_terminated = self.terminated;
                self.terminated = false;
                for stmt in block.stmts {
                    self.visit_stmt(stmt)?;
                }
                // Propagate termination up if block terminated
                if !self.terminated {
                    self.terminated = prev_terminated;
                }
            }
            hir::StmtKind::UncheckedBlock(block) => {
                let prev_terminated = self.terminated;
                self.terminated = false;
                for stmt in block.stmts {
                    self.visit_stmt(stmt)?;
                }
                if !self.terminated {
                    self.terminated = prev_terminated;
                }
            }
            hir::StmtKind::If(cond, then_stmt, else_stmt) => {
                self.visit_expr(cond)?;

                let prev_terminated = self.terminated;
                self.terminated = false;
                self.visit_stmt(then_stmt)?;
                let then_terminated = self.terminated;

                let else_terminated = if let Some(else_stmt) = else_stmt {
                    self.terminated = false;
                    self.visit_stmt(else_stmt)?;
                    self.terminated
                } else {
                    false
                };

                // Only terminate if both branches terminate
                self.terminated = prev_terminated || (then_terminated && else_terminated);
            }
            hir::StmtKind::Loop(block, _) => {
                // Reset termination for loop body
                let prev_terminated = self.terminated;
                self.terminated = false;

                for stmt in block.stmts {
                    self.visit_stmt(stmt)?;
                }

                // Loops don't propagate termination (could have break)
                self.terminated = prev_terminated;
            }
            hir::StmtKind::Try(try_stmt) => {
                self.visit_expr(&try_stmt.expr)?;
                for clause in try_stmt.clauses {
                    for &var in clause.args {
                        self.register_local_var(var);
                    }
                    for stmt in clause.block.stmts {
                        self.visit_stmt(stmt)?;
                    }
                }
            }
            hir::StmtKind::Expr(expr) => {
                self.check_statement_no_effect(expr);
                self.visit_expr(expr)?;
            }
            hir::StmtKind::Emit(expr) => {
                self.visit_expr(expr)?;
            }
            hir::StmtKind::Break | hir::StmtKind::Continue => {}
            hir::StmtKind::Placeholder | hir::StmtKind::Err(_) => {}
        }

        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Never> {
        match &expr.kind {
            hir::ExprKind::Ident(res) => {
                // Record variable uses
                for r in *res {
                    if let Some(var_id) = r.as_variable() {
                        self.record_var_use(var_id);
                    }
                }
            }
            hir::ExprKind::Assign(lhs, op, rhs) => {
                // Check for self-assignment only for simple assignment (no compound op)
                if op.is_none() {
                    self.check_self_assignment(lhs, rhs);
                }
                self.visit_expr(lhs)?;
                self.visit_expr(rhs)?;
                return ControlFlow::Continue(());
            }
            hir::ExprKind::Binary(lhs, op, rhs) => {
                self.check_division_by_zero(expr, op.kind, rhs);
                self.check_boolean_constant_comparison(expr, lhs, op.kind, rhs);
                self.visit_expr(lhs)?;
                self.visit_expr(rhs)?;
                return ControlFlow::Continue(());
            }
            hir::ExprKind::Call(callee, args, _opts) => {
                self.visit_expr(callee)?;
                let arg_exprs: Vec<_> = args.kind.exprs().collect();
                self.check_addmod_mulmod_zero(expr, callee, &arg_exprs);
                self.check_assert_require_message(callee, arg_exprs.len());
                for arg in arg_exprs {
                    self.visit_expr(arg)?;
                }
                return ControlFlow::Continue(());
            }
            _ => {}
        }

        // Default traversal for other expressions
        match &expr.kind {
            hir::ExprKind::Array(exprs) => {
                for e in *exprs {
                    self.visit_expr(e)?;
                }
            }
            hir::ExprKind::Delete(e) | hir::ExprKind::Payable(e) | hir::ExprKind::Unary(_, e) => {
                self.visit_expr(e)?;
            }
            hir::ExprKind::Member(e, _) => {
                self.visit_expr(e)?;
            }
            hir::ExprKind::Index(e, idx) => {
                self.visit_expr(e)?;
                if let Some(idx) = idx {
                    self.visit_expr(idx)?;
                }
            }
            hir::ExprKind::Slice(e, start, end) => {
                self.visit_expr(e)?;
                if let Some(s) = start {
                    self.visit_expr(s)?;
                }
                if let Some(e) = end {
                    self.visit_expr(e)?;
                }
            }
            hir::ExprKind::Ternary(cond, t, f) => {
                self.visit_expr(cond)?;
                self.visit_expr(t)?;
                self.visit_expr(f)?;
            }
            hir::ExprKind::Tuple(exprs) => {
                for e in exprs.iter().flatten() {
                    self.visit_expr(e)?;
                }
            }
            hir::ExprKind::New(ty) | hir::ExprKind::TypeCall(ty) | hir::ExprKind::Type(ty) => {
                self.visit_ty(ty)?;
            }
            hir::ExprKind::Ident(_) | hir::ExprKind::Lit(_) | hir::ExprKind::Err(_) => {}
            hir::ExprKind::Assign(_, _, _)
            | hir::ExprKind::Binary(_, _, _)
            | hir::ExprKind::Call(_, _, _) => unreachable!("handled above"),
        }

        ControlFlow::Continue(())
    }
}
