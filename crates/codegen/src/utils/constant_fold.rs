//! HIR constant folding helpers.
//!
//! These helpers evaluate constant HIR expressions before MIR lowering.

use alloy_primitives::U256;
use solar_sema::{
    eval::{ConstValue, ConstantEvaluator, EvalErrorKind},
    hir::Expr,
    ty::Gcx,
};

/// Result of a constant folding operation.
#[derive(Debug, Clone)]
pub(crate) enum FoldResult {
    /// The expression was folded to a constant integer value.
    Integer(U256),
    /// The expression was folded to a constant boolean value.
    Bool(bool),
    /// The expression cannot be folded.
    NotConstant,
    /// The expression is constant but cannot be folded into a MIR immediate.
    Error,
}

/// A constant folder that evaluates compile-time HIR expressions through sema.
pub(crate) struct ConstantFolder<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> ConstantFolder<'gcx> {
    /// Creates a new constant folder.
    pub(crate) fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx }
    }

    /// Attempts to fold an expression to a constant value.
    ///
    /// Returns `FoldResult::Integer` or `FoldResult::Bool` if the expression can be
    /// evaluated at compile time, `FoldResult::NotConstant` if it cannot, or
    /// `FoldResult::Error` if an error occurred during evaluation.
    pub(crate) fn try_fold(&self, expr: &Expr<'_>) -> FoldResult {
        match ConstantEvaluator::new(self.gcx).try_eval_value(expr) {
            Ok(ConstValue::Integer(value)) => FoldResult::Integer(value.as_evm_word()),
            Ok(ConstValue::Bool(value)) => FoldResult::Bool(value),
            Ok(ConstValue::String(_)) => FoldResult::NotConstant,
            Err(err) => match err.kind {
                EvalErrorKind::ArithmeticOverflow | EvalErrorKind::DivisionByZero => {
                    FoldResult::Error
                }
                _ => FoldResult::NotConstant,
            },
        }
    }
}
