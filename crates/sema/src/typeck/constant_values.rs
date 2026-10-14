//! Checks of constant values that need the results of type checking.
//!
//! These use [`Gcx::const_eval`], which evaluates with the declarations type checking selected:
//!
//! - The base slot of a storage layout specifier must fit in `uint256`, and so must the contract's
//!   storage after it, like solc's `PostTypeContractLevelChecker` checks. Type checking checks the
//!   base slots that solc's evaluator computes, with [`check_early_base_slot`]. Unlike solc, a base
//!   slot that only the full evaluation computes is accepted; see CONTRACT-001 in
//!   `docs/SOLC_DIVERGENCE.md`. Its errors get solc's code for expressions its evaluator cannot
//!   evaluate, 1505.
//! - Inline assembly cannot read a constant without a value at compile time, error 7615 as in solc.
//!   Every use of such a constant calls a helper that computes it, which can allocate memory over
//!   data the assembly stored past the free memory pointer.
//!
//! This only runs when no other error was reported, as solc's base slot checks do: a constant with
//! an error has no value. solc reports 7615 with its other type errors.
//!
//! Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/analysis/PostTypeContractLevelChecker.cpp#L87-L194>

use crate::{
    eval::{ConstContext, EvalErrorKind, IntScalar},
    hir::{self, Visit},
    ty::Gcx,
};
use solar_ast::DataLocation;
use solar_data_structures::Never;
use solar_interface::error_code;
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    if gcx.dcx().has_errors().is_err() {
        return;
    }
    for id in gcx.hir.contract_ids() {
        if let Some(slot) = gcx.hir.contract(id).layout
            && gcx.early_const_eval().eval_int(slot).is_err_and(|err| err.kind.is_unsupported())
        {
            match gcx.const_eval().eval_int(slot) {
                Ok(value) => check_base_slot_value(gcx, id, slot, value, false),
                Err(err) => {
                    let _ = gcx.const_eval().emit_error(slot, err, ConstContext::BaseSlot);
                }
            }
        }
    }
    let mut checker = AssemblyConstants { gcx, in_assembly: false };
    for id in gcx.hir.source_ids() {
        let _ = checker.visit_nested_source(id);
    }
}

/// Checks the value of the base slot `slot` of `contract` if solc's evaluator computes it, or
/// reports the error that solc's evaluator reports. The check after type checking checks the other
/// base slots.
pub(super) fn check_early_base_slot(gcx: Gcx<'_>, contract: hir::ContractId, slot: &hir::Expr<'_>) {
    let early = gcx.early_const_eval();
    match early.eval_int_with_depth_limit(slot) {
        Ok(value) => check_base_slot_value(gcx, contract, slot, value, true),
        Err(err) if err.kind.is_unsupported() => {}
        // solc reports cycles in the `PostTypeChecker`, before it evaluates base slots, so leave
        // them to the constant cycle check.
        Err(err) if err.kind == EvalErrorKind::Cycle => {}
        Err(err) => {
            let _ = early.emit_error(slot, err, ConstContext::BaseSlot);
        }
    }
}

/// Checks that the base slot `value` and the storage after it fit in `uint256`, with solc's codes
/// if solc's evaluator computes the value.
fn check_base_slot_value(
    gcx: Gcx<'_>,
    contract: hir::ContractId,
    slot: &hir::Expr<'_>,
    value: &IntScalar,
    solc: bool,
) {
    let code = |code| ConstContext::BaseSlot.value_code(solc, code);
    let Some(base_slot) = value.as_u256() else {
        gcx.dcx()
            .err("base slot of storage layout evaluates to a value outside the range of type `uint256`")
            .code(code(error_code!(6753)))
            .span(slot.span)
            .emit();
        return;
    };
    if let Ok(Some(size)) = super::storage_size_upper_bound(gcx, contract, DataLocation::Storage)
        && base_slot.checked_add(size).is_none()
    {
        gcx.dcx()
            .err("contract extends past the end of storage when this base slot value is specified")
            .code(code(error_code!(5015)))
            .span(slot.span)
            .emit();
    }
}

struct AssemblyConstants<'gcx> {
    gcx: Gcx<'gcx>,
    in_assembly: bool,
}

impl<'gcx> Visit<'gcx> for AssemblyConstants<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_function(&mut self, func: &'gcx hir::Function<'gcx>) -> ControlFlow<Self::BreakValue> {
        let previous = std::mem::replace(&mut self.in_assembly, func.is_yul);
        let result = self.walk_function(func);
        self.in_assembly = previous;
        result
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        let previous = self.in_assembly;
        self.in_assembly |= matches!(stmt.kind, hir::StmtKind::AssemblyBlock(_));
        let result = self.walk_stmt(stmt);
        self.in_assembly = previous;
        result
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // Expressions outside of assembly contain no assembly.
        if !self.in_assembly {
            return ControlFlow::Continue(());
        }
        if let Some(id) = expr.as_variable()
            && self.gcx.hir.variable(id).is_constant()
            && self.gcx.const_eval().eval_var(id).is_err()
        {
            self.gcx
                .dcx()
                .err("only direct number constants are supported in inline assembly")
                .code(error_code!(7615))
                .span(expr.span)
                .emit();
        }
        self.walk_expr(expr)
    }
}
