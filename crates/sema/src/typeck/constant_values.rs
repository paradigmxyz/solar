//! Checks of constant values after type checking and the constant cycle check.
//!
//! - The base slot of a storage layout specifier must fit in `uint256`, and so must the contract's
//!   storage after it, like solc's `PostTypeContractLevelChecker` checks. Base slots that solc's
//!   evaluator computes are checked with [`Gcx::early_const_eval`], and the rest with
//!   [`Gcx::const_eval`], which accepts base slots that solc rejects; see CONTRACT-001 in
//!   `docs/SOLC_DIVERGENCE.md`. Errors in those get solc's code for expressions its evaluator
//!   cannot evaluate, 1505.
//! - Inline assembly cannot read a constant that the evaluator does not compute, error 7615 as in
//!   solc. Every use of such a constant calls a helper that computes it, which can allocate memory
//!   over data the assembly stored past the free memory pointer.
//!
//! The checks that use [`Gcx::const_eval`] only run when no other error was reported, as solc's
//! base slot checks do: a constant with an error has no value. solc reports 7615 with its other
//! type errors.
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
    let mut full_base_slots = Vec::new();
    for id in gcx.hir.contract_ids() {
        // Type checking reports a base slot that is not an integer.
        if let Some(slot) = gcx.hir.contract(id).layout
            && let Some(ty) = gcx.type_of_expr(slot.id)
            && ty.is_integer()
            && needs_full_eval(gcx, id, slot)
        {
            full_base_slots.push((id, slot));
        }
    }
    if gcx.dcx().has_errors().is_err() {
        return;
    }
    for (id, slot) in full_base_slots {
        match gcx.const_eval().eval_int(slot) {
            Ok(value) => check_base_slot_value(gcx, id, slot, value, false),
            Err(err) => {
                let _ = gcx.const_eval().emit_error(slot, err, ConstContext::BaseSlot);
            }
        }
    }
    let mut checker = AssemblyConstants { gcx, in_assembly: false };
    for id in gcx.hir.source_ids() {
        let _ = checker.visit_nested_source(id);
    }
}

/// Checks the value of the base slot `slot` of `contract` if solc's evaluator computes it, or
/// reports the error that solc's evaluator reports. Returns `true` if only the full evaluation
/// computes the base slot.
fn needs_full_eval(gcx: Gcx<'_>, contract: hir::ContractId, slot: &hir::Expr<'_>) -> bool {
    let early = gcx.early_const_eval();
    match early.eval_int_with_depth_limit(slot) {
        Ok(value) => check_base_slot_value(gcx, contract, slot, value, true),
        Err(err) if err.kind.is_unsupported() => return true,
        // The constant cycle check reports cycles.
        Err(err) if err.kind == EvalErrorKind::Cycle => {}
        Err(err) => {
            let _ = early.emit_error(slot, err, ConstContext::BaseSlot);
        }
    }
    false
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
        // Allow a constant whose initializer reverts: its helper halts before it can overwrite
        // memory.
        if let Some(id) = expr.as_variable()
            && self.gcx.hir.variable(id).is_constant()
            && let Err(err) = self.gcx.const_eval().eval_var(id)
            && err.kind.is_unsupported()
        {
            self.gcx
                .dcx()
                .err("inline assembly can only read constants with a compile-time value")
                .code(error_code!(7615))
                .span(expr.span)
                .span_label(err.span, "this has no value at compile time")
                .note("every use of the constant runs its initializer, which can allocate memory")
                .emit();
        }
        self.walk_expr(expr)
    }
}
