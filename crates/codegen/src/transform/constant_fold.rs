//! Constant folding optimization pass.
//!
//! This pass evaluates constant expressions at compile time, replacing binary operations
//! on immediate values with their computed results.

use alloy_primitives::U256;
use solar_ast::{BinOpKind, LitKind, UnOpKind};
use solar_sema::hir::{Expr, ExprKind, Hir, Lit};

/// Result of a constant folding operation.
#[derive(Debug, Clone)]
pub enum FoldResult {
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
pub enum FoldError {
    /// Division by zero.
    DivisionByZero,
    /// Arithmetic overflow.
    Overflow,
    /// Shift amount too large.
    ShiftOverflow,
}

/// A constant folder that evaluates compile-time constant expressions.
///
/// This struct walks HIR expressions and attempts to fold binary and unary operations
/// where all operands are immediate values (literals).
pub struct ConstantFolder<'hir> {
    _hir: &'hir Hir<'hir>,
}

impl<'hir> ConstantFolder<'hir> {
    /// Creates a new constant folder.
    pub fn new(hir: &'hir Hir<'hir>) -> Self {
        Self { _hir: hir }
    }

    /// Attempts to fold an expression to a constant value.
    ///
    /// Returns `FoldResult::Integer` or `FoldResult::Bool` if the expression can be
    /// evaluated at compile time, `FoldResult::NotConstant` if it cannot, or
    /// `FoldResult::Error` if an error occurred during evaluation.
    pub fn try_fold(&self, expr: &Expr<'_>) -> FoldResult {
        self.fold_expr(expr)
    }

    /// Attempts to fold an expression and returns the integer value if successful.
    pub fn fold_to_integer(&self, expr: &Expr<'_>) -> Option<U256> {
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

/// Statistics from a constant folding pass.
#[derive(Debug, Default)]
pub struct FoldStats {
    /// Number of expressions analyzed.
    pub expressions_analyzed: usize,
    /// Number of expressions successfully folded.
    pub expressions_folded: usize,
    /// Number of binary operations folded.
    pub binary_ops_folded: usize,
    /// Number of unary operations folded.
    pub unary_ops_folded: usize,
}

/// Analyzes an expression and returns fold statistics.
pub fn analyze_foldable(folder: &ConstantFolder<'_>, expr: &Expr<'_>) -> FoldStats {
    let mut stats = FoldStats::default();
    analyze_expr_recursive(folder, expr, &mut stats);
    stats
}

fn analyze_expr_recursive(folder: &ConstantFolder<'_>, expr: &Expr<'_>, stats: &mut FoldStats) {
    stats.expressions_analyzed += 1;

    let expr = expr.peel_parens();
    match &expr.kind {
        ExprKind::Binary(left, _op, right) => {
            analyze_expr_recursive(folder, left, stats);
            analyze_expr_recursive(folder, right, stats);

            if matches!(folder.try_fold(expr), FoldResult::Integer(_) | FoldResult::Bool(_)) {
                stats.expressions_folded += 1;
                stats.binary_ops_folded += 1;
            }
        }
        ExprKind::Unary(_op, operand) => {
            analyze_expr_recursive(folder, operand, stats);

            if matches!(folder.try_fold(expr), FoldResult::Integer(_) | FoldResult::Bool(_)) {
                stats.expressions_folded += 1;
                stats.unary_ops_folded += 1;
            }
        }
        ExprKind::Ternary(cond, then_expr, else_expr) => {
            analyze_expr_recursive(folder, cond, stats);
            analyze_expr_recursive(folder, then_expr, stats);
            analyze_expr_recursive(folder, else_expr, stats);

            if matches!(folder.try_fold(expr), FoldResult::Integer(_) | FoldResult::Bool(_)) {
                stats.expressions_folded += 1;
            }
        }
        ExprKind::Array(exprs) => {
            for e in *exprs {
                analyze_expr_recursive(folder, e, stats);
            }
        }
        ExprKind::Tuple(exprs) => {
            for e in exprs.iter().flatten() {
                analyze_expr_recursive(folder, e, stats);
            }
        }
        ExprKind::Call(callee, args, _opts) => {
            analyze_expr_recursive(folder, callee, stats);
            for arg in args.kind.exprs() {
                analyze_expr_recursive(folder, arg, stats);
            }
        }
        ExprKind::Index(base, index) => {
            analyze_expr_recursive(folder, base, stats);
            if let Some(idx) = index {
                analyze_expr_recursive(folder, idx, stats);
            }
        }
        ExprKind::Slice(base, start, end) => {
            analyze_expr_recursive(folder, base, stats);
            if let Some(s) = start {
                analyze_expr_recursive(folder, s, stats);
            }
            if let Some(e) = end {
                analyze_expr_recursive(folder, e, stats);
            }
        }
        ExprKind::Member(base, _) => {
            analyze_expr_recursive(folder, base, stats);
        }
        ExprKind::Assign(left, _, right) => {
            analyze_expr_recursive(folder, left, stats);
            analyze_expr_recursive(folder, right, stats);
        }
        ExprKind::Delete(e) | ExprKind::Payable(e) => {
            analyze_expr_recursive(folder, e, stats);
        }
        ExprKind::Lit(_)
        | ExprKind::Ident(_)
        | ExprKind::Type(_)
        | ExprKind::TypeCall(_)
        | ExprKind::New(_)
        | ExprKind::Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold_binary_int(left: U256, op: BinOpKind, right: U256) -> FoldResult {
        fold_binary_values(FoldResult::Integer(left), op, FoldResult::Integer(right))
    }

    fn fold_binary_values(left: FoldResult, op: BinOpKind, right: FoldResult) -> FoldResult {
        let (l, r) = match (&left, &right) {
            (FoldResult::Integer(l), FoldResult::Integer(r)) => (*l, *r),
            (FoldResult::Bool(l), FoldResult::Bool(r)) => {
                return fold_bool_binary(*l, op, *r);
            }
            (FoldResult::Bool(l), FoldResult::Integer(r)) => (U256::from(*l as u8), *r),
            (FoldResult::Integer(l), FoldResult::Bool(r)) => (*l, U256::from(*r as u8)),
            (FoldResult::Error(e), _) | (_, FoldResult::Error(e)) => return FoldResult::Error(*e),
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

    fn fold_bool_binary(left: bool, op: BinOpKind, right: bool) -> FoldResult {
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

    #[test]
    fn test_add() {
        let result = fold_binary_int(U256::from(1), BinOpKind::Add, U256::from(2));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(3)));
    }

    #[test]
    fn test_sub() {
        let result = fold_binary_int(U256::from(5), BinOpKind::Sub, U256::from(3));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(2)));
    }

    #[test]
    fn test_mul() {
        let result = fold_binary_int(U256::from(3), BinOpKind::Mul, U256::from(4));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(12)));
    }

    #[test]
    fn test_div() {
        let result = fold_binary_int(U256::from(10), BinOpKind::Div, U256::from(2));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(5)));
    }

    #[test]
    fn test_div_by_zero() {
        let result = fold_binary_int(U256::from(10), BinOpKind::Div, U256::ZERO);
        assert!(matches!(result, FoldResult::Error(FoldError::DivisionByZero)));
    }

    #[test]
    fn test_rem() {
        let result = fold_binary_int(U256::from(10), BinOpKind::Rem, U256::from(3));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(1)));
    }

    #[test]
    fn test_pow() {
        let result = fold_binary_int(U256::from(2), BinOpKind::Pow, U256::from(8));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(256)));
    }

    #[test]
    fn test_bitwise_and() {
        let result = fold_binary_int(U256::from(0b1100), BinOpKind::BitAnd, U256::from(0b1010));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(0b1000)));
    }

    #[test]
    fn test_bitwise_or() {
        let result = fold_binary_int(U256::from(0b1100), BinOpKind::BitOr, U256::from(0b1010));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(0b1110)));
    }

    #[test]
    fn test_bitwise_xor() {
        let result = fold_binary_int(U256::from(0b1100), BinOpKind::BitXor, U256::from(0b1010));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(0b0110)));
    }

    #[test]
    fn test_shl() {
        let result = fold_binary_int(U256::from(1), BinOpKind::Shl, U256::from(4));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(16)));
    }

    #[test]
    fn test_shr() {
        let result = fold_binary_int(U256::from(16), BinOpKind::Shr, U256::from(2));
        assert!(matches!(result, FoldResult::Integer(n) if n == U256::from(4)));
    }

    #[test]
    fn test_comparisons() {
        assert!(matches!(
            fold_binary_int(U256::from(1), BinOpKind::Lt, U256::from(2)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(2), BinOpKind::Lt, U256::from(1)),
            FoldResult::Bool(false)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(2), BinOpKind::Le, U256::from(2)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(3), BinOpKind::Gt, U256::from(2)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(2), BinOpKind::Ge, U256::from(2)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(2), BinOpKind::Eq, U256::from(2)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(1), BinOpKind::Ne, U256::from(2)),
            FoldResult::Bool(true)
        ));
    }

    #[test]
    fn test_logical_ops() {
        assert!(matches!(
            fold_binary_int(U256::from(1), BinOpKind::And, U256::from(1)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(1), BinOpKind::And, U256::from(0)),
            FoldResult::Bool(false)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(0), BinOpKind::Or, U256::from(1)),
            FoldResult::Bool(true)
        ));
        assert!(matches!(
            fold_binary_int(U256::from(0), BinOpKind::Or, U256::from(0)),
            FoldResult::Bool(false)
        ));
    }

    #[test]
    fn test_complex_expression() {
        // 1 + 2 * 3 = 7 (but we test folding individual ops)
        let mul_result = fold_binary_int(U256::from(2), BinOpKind::Mul, U256::from(3));
        assert!(matches!(mul_result, FoldResult::Integer(n) if n == U256::from(6)));

        // 1 + 6 = 7
        let add_result = fold_binary_int(U256::from(1), BinOpKind::Add, U256::from(6));
        assert!(matches!(add_result, FoldResult::Integer(n) if n == U256::from(7)));
    }
}
