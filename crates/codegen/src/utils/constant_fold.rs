//! HIR constant folding helpers.
//!
//! These helpers evaluate constant HIR expressions before MIR lowering.

use alloy_primitives::U256;
use solar_ast::{BinOpKind, LitKind, UnOpKind};
use solar_sema::hir::{Expr, ExprKind, Hir, Lit};

/// Result of a constant folding operation.
#[derive(Debug, Clone)]
pub(crate) enum FoldResult {
    /// The expression was folded to a constant integer value.
    Integer(U256),
    /// The expression was folded to a constant boolean value.
    Bool(bool),
    /// The expression cannot be folded.
    NotConstant,
    /// An error occurred during folding (e.g., division by zero).
    Error(FoldError),
}

/// Errors that can occur during constant folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FoldError {
    /// Division by zero.
    DivisionByZero,
    /// Arithmetic overflow.
    Overflow,
}

/// A constant folder that evaluates compile-time constant expressions.
///
/// This struct walks HIR expressions and attempts to fold binary and unary operations
/// where all operands are immediate values (literals).
pub(crate) struct ConstantFolder<'hir> {
    _hir: &'hir Hir<'hir>,
}

impl<'hir> ConstantFolder<'hir> {
    /// Creates a new constant folder.
    pub(crate) fn new(hir: &'hir Hir<'hir>) -> Self {
        Self { _hir: hir }
    }

    /// Attempts to fold an expression to a constant value.
    ///
    /// Returns `FoldResult::Integer` or `FoldResult::Bool` if the expression can be
    /// evaluated at compile time, `FoldResult::NotConstant` if it cannot, or
    /// `FoldResult::Error` if an error occurred during evaluation.
    pub(crate) fn try_fold(&self, expr: &Expr<'_>) -> FoldResult {
        self.fold_expr(expr)
    }

    /// Attempts to fold an expression and returns the integer value if successful.
    pub(crate) fn fold_to_integer(&self, expr: &Expr<'_>) -> Option<U256> {
        match self.try_fold(expr) {
            FoldResult::Integer(n) => Some(n),
            FoldResult::Bool(b) => Some(U256::from(b as u8)),
            _ => None,
        }
    }

    fn fold_expr(&self, expr: &Expr<'_>) -> FoldResult {
        let expr = expr.peel_parens();
        match &expr.kind {
            ExprKind::Lit(lit) => self.fold_lit(lit),
            ExprKind::Binary(left, op, right) => {
                let left_val = self.fold_expr(left);
                let right_val = self.fold_expr(right);
                self.fold_binary(left_val, op.kind, right_val)
            }
            ExprKind::Unary(op, operand) => {
                let val = self.fold_expr(operand);
                self.fold_unary(op.kind, val)
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => match self.fold_expr(cond) {
                FoldResult::Bool(true) => self.fold_expr(then_expr),
                FoldResult::Bool(false) => self.fold_expr(else_expr),
                FoldResult::Integer(n) if !n.is_zero() => self.fold_expr(then_expr),
                FoldResult::Integer(_) => self.fold_expr(else_expr),
                _ => FoldResult::NotConstant,
            },
            _ => FoldResult::NotConstant,
        }
    }

    fn fold_lit(&self, lit: &Lit<'_>) -> FoldResult {
        match &lit.kind {
            LitKind::Number(n) => FoldResult::Integer(*n),
            LitKind::Bool(b) => FoldResult::Bool(*b),
            _ => FoldResult::NotConstant,
        }
    }

    fn fold_binary(&self, left: FoldResult, op: BinOpKind, right: FoldResult) -> FoldResult {
        let (l, r) = match (left, right) {
            (FoldResult::Integer(l), FoldResult::Integer(r)) => (l, r),
            (FoldResult::Bool(l), FoldResult::Bool(r)) => {
                return self.fold_bool_binary(l, op, r);
            }
            (FoldResult::Bool(l), FoldResult::Integer(r)) => (U256::from(l as u8), r),
            (FoldResult::Integer(l), FoldResult::Bool(r)) => (l, U256::from(r as u8)),
            (FoldResult::Error(e), _) | (_, FoldResult::Error(e)) => return FoldResult::Error(e),
            _ => return FoldResult::NotConstant,
        };

        match op {
            BinOpKind::Add => {
                l.checked_add(r).map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer)
            }
            BinOpKind::Sub => {
                l.checked_sub(r).map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer)
            }
            BinOpKind::Mul => {
                l.checked_mul(r).map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer)
            }
            BinOpKind::Div => {
                if r.is_zero() {
                    FoldResult::Error(FoldError::DivisionByZero)
                } else {
                    FoldResult::Integer(l / r)
                }
            }
            BinOpKind::Rem => {
                if r.is_zero() {
                    FoldResult::Error(FoldError::DivisionByZero)
                } else {
                    FoldResult::Integer(l % r)
                }
            }
            BinOpKind::Pow => {
                l.checked_pow(r).map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer)
            }

            BinOpKind::BitAnd => FoldResult::Integer(l & r),
            BinOpKind::BitOr => FoldResult::Integer(l | r),
            BinOpKind::BitXor => FoldResult::Integer(l ^ r),

            BinOpKind::Shl => {
                let shift: usize = r.try_into().unwrap_or(usize::MAX);
                if shift >= 256 {
                    FoldResult::Integer(U256::ZERO)
                } else {
                    FoldResult::Integer(l << shift)
                }
            }
            BinOpKind::Shr => {
                let shift: usize = r.try_into().unwrap_or(usize::MAX);
                if shift >= 256 {
                    FoldResult::Integer(U256::ZERO)
                } else {
                    FoldResult::Integer(l >> shift)
                }
            }
            BinOpKind::Sar => {
                let shift: usize = r.try_into().unwrap_or(usize::MAX);
                FoldResult::Integer(l.arithmetic_shr(shift))
            }

            BinOpKind::Lt => FoldResult::Bool(l < r),
            BinOpKind::Le => FoldResult::Bool(l <= r),
            BinOpKind::Gt => FoldResult::Bool(l > r),
            BinOpKind::Ge => FoldResult::Bool(l >= r),
            BinOpKind::Eq => FoldResult::Bool(l == r),
            BinOpKind::Ne => FoldResult::Bool(l != r),

            BinOpKind::And => FoldResult::Bool(!l.is_zero() && !r.is_zero()),
            BinOpKind::Or => FoldResult::Bool(!l.is_zero() || !r.is_zero()),
        }
    }

    fn fold_bool_binary(&self, left: bool, op: BinOpKind, right: bool) -> FoldResult {
        match op {
            BinOpKind::And => FoldResult::Bool(left && right),
            BinOpKind::Or => FoldResult::Bool(left || right),
            BinOpKind::Eq => FoldResult::Bool(left == right),
            BinOpKind::Ne => FoldResult::Bool(left != right),
            BinOpKind::BitAnd => FoldResult::Bool(left && right),
            BinOpKind::BitOr => FoldResult::Bool(left || right),
            BinOpKind::BitXor => FoldResult::Bool(left ^ right),
            _ => FoldResult::NotConstant,
        }
    }

    fn fold_unary(&self, op: UnOpKind, operand: FoldResult) -> FoldResult {
        match operand {
            FoldResult::Integer(n) => match op {
                UnOpKind::Not | UnOpKind::BitNot => FoldResult::Integer(!n),
                UnOpKind::Neg => FoldResult::Integer(n.wrapping_neg()),
                UnOpKind::PreInc | UnOpKind::PostInc => n
                    .checked_add(U256::from(1))
                    .map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer),
                UnOpKind::PreDec | UnOpKind::PostDec => n
                    .checked_sub(U256::from(1))
                    .map_or(FoldResult::Error(FoldError::Overflow), FoldResult::Integer),
            },
            FoldResult::Bool(b) => match op {
                UnOpKind::Not | UnOpKind::BitNot => FoldResult::Bool(!b),
                _ => FoldResult::NotConstant,
            },
            FoldResult::Error(e) => FoldResult::Error(e),
            FoldResult::NotConstant => FoldResult::NotConstant,
        }
    }
}
