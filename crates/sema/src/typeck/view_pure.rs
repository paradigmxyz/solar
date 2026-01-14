//! View/Pure function checker.
//!
//! This pass validates that functions declared with `view` or `pure` modifiers
//! do not perform operations that violate their state mutability.
//!
//! Reference: solc's ViewPureChecker.cpp

use crate::{
    builtins::Builtin,
    hir::{self, Visit},
    ty::{Gcx, TyKind},
};
use solar_ast::StateMutability;
use solar_data_structures::{Never, map::FxHashMap};
use solar_interface::{Span, error_code};
use std::ops::ControlFlow;

/// Check view/pure constraints for all functions in a source.
pub(crate) fn check(gcx: Gcx<'_>, source: hir::SourceId) {
    let mut checker = ViewPureChecker::new(gcx, source);
    let _ = checker.visit_nested_source(source);
}

/// Tracks the inferred mutability and its location for reporting.
#[derive(Clone, Copy, Debug)]
struct MutabilityAndLocation {
    mutability: StateMutability,
    location: Span,
}

impl Default for MutabilityAndLocation {
    fn default() -> Self {
        Self { mutability: StateMutability::Pure, location: Span::DUMMY }
    }
}

struct ViewPureChecker<'gcx> {
    gcx: Gcx<'gcx>,
    #[allow(dead_code)]
    source: hir::SourceId,

    /// The current function being checked, if any.
    current_function: Option<&'gcx hir::Function<'gcx>>,

    /// The best (most restrictive) mutability inferred so far and its location.
    best_mutability: MutabilityAndLocation,

    /// Cached inferred mutability for modifiers.
    inferred_modifier_mutability: FxHashMap<hir::FunctionId, MutabilityAndLocation>,

    /// Whether we're currently in an lvalue context (writing to).
    in_lvalue: bool,
}

impl<'gcx> ViewPureChecker<'gcx> {
    fn new(gcx: Gcx<'gcx>, source: hir::SourceId) -> Self {
        Self {
            gcx,
            source,
            current_function: None,
            best_mutability: MutabilityAndLocation::default(),
            inferred_modifier_mutability: FxHashMap::default(),
            in_lvalue: false,
        }
    }

    /// Reports that an expression requires a certain mutability level.
    fn report_mutability(&mut self, mutability: StateMutability, location: Span) {
        self.report_mutability_with_nested(mutability, location, None);
    }

    /// Reports mutability with an optional nested location (for modifiers).
    fn report_mutability_with_nested(
        &mut self,
        mutability: StateMutability,
        location: Span,
        nested_location: Option<Span>,
    ) {
        // Update best mutability if this is more permissive
        if mutability > self.best_mutability.mutability {
            self.best_mutability = MutabilityAndLocation { mutability, location };
        }

        // Check if we're violating the current function's declared mutability
        let Some(func) = self.current_function else { return };
        if mutability <= func.state_mutability {
            return;
        }

        // Handle different mutability violations
        match mutability {
            StateMutability::View => {
                // Pure function reading state
                if func.state_mutability == StateMutability::Pure {
                    self.gcx
                        .dcx()
                        .err(
                            "function declared as pure, but this expression \
                             (potentially) reads from the environment or state and thus \
                             requires \"view\"",
                        )
                        .code(error_code!(2527))
                        .span(location)
                        .emit();
                }
            }
            StateMutability::NonPayable => {
                // View or pure function modifying state
                let msg = format!(
                    "function cannot be declared as {} because this expression \
                     (potentially) modifies the state",
                    func.state_mutability
                );
                self.gcx.dcx().err(msg).code(error_code!(8961)).span(location).emit();
            }
            StateMutability::Payable => {
                // Only check for public/external non-library functions
                if (func.is_constructor() || func.visibility >= hir::Visibility::Public)
                    && !self.is_library_function(func)
                {
                    let msg = if func.is_constructor() {
                        "\"msg.value\" and \"callvalue()\" can only be used in payable \
                         constructors. Make the constructor \"payable\" to avoid this error."
                    } else {
                        "\"msg.value\" and \"callvalue()\" can only be used in payable \
                         public functions. Make the function \"payable\" or use an internal \
                         function to avoid this error."
                    };

                    let mut diag = self.gcx.dcx().err(msg).code(error_code!(5887)).span(location);
                    if let Some(nested) = nested_location {
                        diag = diag.span_note(
                            nested,
                            "\"msg.value\" or \"callvalue()\" appear here inside the modifier",
                        );
                    }
                    diag.emit();
                }
            }
            StateMutability::Pure => {
                // Pure is the most restrictive, no violation possible
            }
        }
    }

    /// Reports mutability for a function call, treating payable as nonpayable.
    fn report_function_call_mutability(&mut self, mutability: StateMutability, location: Span) {
        // Calling a payable function only requires nonpayable
        let effective = if mutability == StateMutability::Payable {
            StateMutability::NonPayable
        } else {
            mutability
        };
        self.report_mutability(effective, location);
    }

    fn is_library_function(&self, func: &hir::Function<'_>) -> bool {
        func.contract.map(|c| self.gcx.hir.contract(c).kind.is_library()).unwrap_or(false)
    }

    /// Gets or computes the mutability for a modifier.
    fn modifier_mutability(&mut self, modifier_id: hir::FunctionId) -> MutabilityAndLocation {
        if let Some(&cached) = self.inferred_modifier_mutability.get(&modifier_id) {
            return cached;
        }

        // Save current state
        let saved_best = std::mem::take(&mut self.best_mutability);
        let saved_function = self.current_function.take();

        // Visit the modifier
        let modifier = self.gcx.hir.function(modifier_id);
        let _ = self.visit_function(modifier);

        // Get result and restore state
        let result = self.best_mutability;
        self.best_mutability = saved_best;
        self.current_function = saved_function;

        self.inferred_modifier_mutability.insert(modifier_id, result);
        result
    }

    /// Checks an identifier expression for state access.
    fn check_identifier(&mut self, res: &[hir::Res], span: Span) {
        for r in res {
            self.check_single_res(*r, span);
        }
    }

    fn check_single_res(&mut self, res: hir::Res, span: Span) {
        let mutability = match res {
            hir::Res::Item(hir::ItemId::Variable(var_id)) => {
                let var = self.gcx.hir.variable(var_id);

                // Skip constants
                if let Some(m) = var.mutability {
                    if m.is_constant() {
                        return;
                    }
                    if m.is_immutable() {
                        // Immutables with literal values are pure, otherwise view
                        if var.initializer.is_some() {
                            let ty = self.gcx.type_of_item(var_id.into());
                            // Check if it's a literal type (IntLiteral or StringLiteral)
                            if matches!(ty.kind, TyKind::IntLiteral(..) | TyKind::StringLiteral(..))
                            {
                                return;
                            }
                        }
                        StateMutability::View
                    } else if var.is_state_variable() {
                        if self.in_lvalue {
                            StateMutability::NonPayable
                        } else {
                            StateMutability::View
                        }
                    } else {
                        return;
                    }
                } else if var.is_state_variable() {
                    if self.in_lvalue { StateMutability::NonPayable } else { StateMutability::View }
                } else {
                    return;
                }
            }
            hir::Res::Builtin(Builtin::This) => {
                // `this` reads the address
                StateMutability::View
            }
            hir::Res::Builtin(_) => return,
            _ => return,
        };

        self.report_mutability(mutability, span);
    }

    /// Checks a member access for state mutability by examining the builtin being accessed.
    fn check_member_builtin(&mut self, member_name: solar_interface::Symbol, span: Span) {
        // Check for magic variable members that affect mutability
        use solar_interface::{kw, sym};

        // block.* members - all are view
        if member_name == kw::Coinbase
            || member_name == kw::Timestamp
            || member_name == kw::Difficulty
            || member_name == kw::Prevrandao
            || member_name == kw::Number
            || member_name == kw::Gaslimit
            || member_name == kw::Chainid
            || member_name == kw::Basefee
            || member_name == kw::Blobbasefee
        {
            self.report_mutability(StateMutability::View, span);
            return;
        }

        // msg.* members
        if member_name == sym::sender {
            self.report_mutability(StateMutability::View, span);
            return;
        }
        if member_name == sym::value {
            self.report_mutability(StateMutability::Payable, span);
            return;
        }
        if member_name == kw::Gas {
            self.report_mutability(StateMutability::View, span);
            return;
        }

        // tx.* members
        if member_name == kw::Origin || member_name == kw::Gasprice {
            self.report_mutability(StateMutability::View, span);
            return;
        }

        // address.balance, address.code, address.codehash
        if member_name == kw::Balance || member_name == sym::code || member_name == sym::codehash {
            self.report_mutability(StateMutability::View, span);
        }
    }
}

impl<'gcx> Visit<'gcx> for ViewPureChecker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_function(&mut self, func: &'gcx hir::Function<'gcx>) -> ControlFlow<Self::BreakValue> {
        // Skip if no body (interface functions, etc.)
        if func.body.is_none() {
            return ControlFlow::Continue(());
        }

        // Modifiers are handled separately when invoked
        if func.kind == hir::FunctionKind::Modifier {
            return self.walk_function(func);
        }

        // Save and set current function
        let prev_function = self.current_function.replace(func);
        let prev_best = std::mem::replace(
            &mut self.best_mutability,
            MutabilityAndLocation { mutability: StateMutability::Pure, location: func.span },
        );

        // Check modifiers first
        for modifier in func.modifiers {
            if let hir::ItemId::Function(mod_id) = modifier.id {
                let mod_func = self.gcx.hir.function(mod_id);
                if mod_func.kind == hir::FunctionKind::Modifier {
                    let mod_mutability = self.modifier_mutability(mod_id);
                    self.report_mutability_with_nested(
                        mod_mutability.mutability,
                        modifier.span,
                        Some(mod_mutability.location),
                    );
                }
            }
            // Visit modifier arguments
            let _ = self.visit_call_args(&modifier.args);
        }

        // Visit function body
        if let Some(body) = &func.body {
            for stmt in body.stmts {
                let _ = self.visit_stmt(stmt);
            }
        }

        // Issue warning if function could be more restrictive
        if self.best_mutability.mutability < func.state_mutability
            && func.state_mutability != StateMutability::Payable
            && func.body.is_some()
            && !func.body.map(|b| b.stmts.is_empty()).unwrap_or(true)
            && !func.is_constructor()
            && !func.is_special()
            && !func.virtual_
        {
            self.gcx
                .dcx()
                .warn(format!(
                    "function state mutability can be restricted to {}",
                    self.best_mutability.mutability
                ))
                .code(error_code!(2018))
                .span(func.span)
                .emit();
        }

        // Restore state
        self.best_mutability = prev_best;
        self.current_function = prev_function;

        ControlFlow::Continue(())
    }

    fn visit_nested_var(&mut self, id: hir::VariableId) -> ControlFlow<Self::BreakValue> {
        // Check variable initializers
        let var = self.gcx.hir.variable(id);
        if let Some(init) = var.initializer {
            let _ = self.visit_expr(init);
        }
        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match &expr.kind {
            hir::ExprKind::Ident(res) => {
                self.check_identifier(res, expr.span);
            }

            hir::ExprKind::Assign(lhs, _, rhs) => {
                // Check lhs in lvalue context
                let prev_lvalue = self.in_lvalue;
                self.in_lvalue = true;
                let _ = self.visit_expr(lhs);
                self.in_lvalue = prev_lvalue;

                let _ = self.visit_expr(rhs);
                return ControlFlow::Continue(());
            }

            hir::ExprKind::Member(base_expr, member) => {
                let _ = self.visit_expr(base_expr);
                // Check if this is a magic variable member access
                self.check_member_builtin(member.name, expr.span);
                return ControlFlow::Continue(());
            }

            hir::ExprKind::Call(callee, args, opts) => {
                let _ = self.visit_expr(callee);

                // Get callee type to check state mutability
                if let hir::ExprKind::Ident(res) = &callee.kind {
                    for r in *res {
                        match r {
                            hir::Res::Item(hir::ItemId::Function(f_id)) => {
                                let called_func = self.gcx.hir.function(*f_id);
                                self.report_function_call_mutability(
                                    called_func.state_mutability,
                                    expr.span,
                                );
                            }
                            hir::Res::Builtin(builtin) => {
                                let ty = builtin.ty(self.gcx);
                                if let Some(sm) = ty.state_mutability() {
                                    self.report_function_call_mutability(sm, expr.span);
                                }
                            }
                            _ => {}
                        }
                    }
                } else if let hir::ExprKind::Member(_, member) = &callee.kind {
                    // Check member function calls (e.g., address.call)
                    use solar_interface::kw;
                    if member.name == kw::Call
                        || member.name == kw::Delegatecall
                        || member.name == kw::Staticcall
                    {
                        // External calls are view
                        self.report_function_call_mutability(StateMutability::View, expr.span);
                    }
                }

                // Check call options (value, gas, salt)
                if let Some(opts) = opts {
                    for opt in *opts {
                        let _ = self.visit_expr(&opt.value);
                        // {value: ...} requires NonPayable (sending ether)
                        if opt.name.name == solar_interface::sym::value {
                            self.report_mutability(StateMutability::NonPayable, opt.value.span);
                        }
                    }
                }

                let _ = self.visit_call_args(args);
                return ControlFlow::Continue(());
            }

            hir::ExprKind::New(ty) => {
                // Creating new contracts requires NonPayable
                let new_ty = self.gcx.type_of_hir_ty(ty);
                if matches!(new_ty.kind, TyKind::Contract(_)) {
                    self.report_mutability(StateMutability::NonPayable, expr.span);
                }
            }

            hir::ExprKind::Unary(op, inner_expr) => {
                // Pre/post increment/decrement are writes
                if op.kind.has_side_effects() {
                    let prev_lvalue = self.in_lvalue;
                    self.in_lvalue = true;
                    let _ = self.visit_expr(inner_expr);
                    self.in_lvalue = prev_lvalue;
                    return ControlFlow::Continue(());
                }
            }

            hir::ExprKind::Delete(inner_expr) => {
                // Delete is a write
                let prev_lvalue = self.in_lvalue;
                self.in_lvalue = true;
                let _ = self.visit_expr(inner_expr);
                self.in_lvalue = prev_lvalue;
                return ControlFlow::Continue(());
            }

            _ => {}
        }

        self.walk_expr(expr)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Emit(_) = &stmt.kind {
            // Emitting events requires NonPayable
            self.report_mutability(StateMutability::NonPayable, stmt.span);
        }

        self.walk_stmt(stmt)
    }
}
