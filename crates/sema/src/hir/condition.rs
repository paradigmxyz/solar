//! Normalized source conditions for HIR analyses.

use super::{BinOpKind, Expr, ExprKind, JoinSemiLattice, UnOpKind};

/// A source-level comparison with explicit operand order and polarity.
#[derive(Clone, Copy, Debug)]
pub struct Comparison<'hir> {
    /// The left operand.
    pub lhs: &'hir Expr<'hir>,
    /// The comparison operator.
    pub op: BinOpKind,
    /// The right operand.
    pub rhs: &'hir Expr<'hir>,
}

impl<'hir> Comparison<'hir> {
    /// Extracts a comparison from `expr` after peeling parentheses.
    pub fn from_expr(expr: &'hir Expr<'hir>) -> Option<Self> {
        let ExprKind::Binary(lhs, op, rhs) = &expr.peel_parens().kind else { return None };
        matches!(
            op.kind,
            BinOpKind::Lt
                | BinOpKind::Le
                | BinOpKind::Gt
                | BinOpKind::Ge
                | BinOpKind::Eq
                | BinOpKind::Ne
        )
        .then_some(Self { lhs, op: op.kind, rhs })
    }

    /// Reverses the operand order while preserving meaning.
    pub fn reversed(self) -> Self {
        Self { lhs: self.rhs, op: reverse_comparison(self.op), rhs: self.lhs }
    }

    /// Negates the comparison.
    pub fn negated(self) -> Self {
        Self { op: negate_comparison(self.op), ..self }
    }

    /// Returns this comparison and its equivalent reversed form.
    pub fn orientations(self) -> [Self; 2] {
        [self, self.reversed()]
    }
}

/// Applies the comparison facts implied by a boolean condition edge.
///
/// Negations and the conjunctive forms of `&&` and `||` are decomposed directly. Disjunctive
/// forms refine separate states and join them, preserving soundness without requiring each
/// analysis to reimplement boolean-path handling.
pub fn apply_condition_facts<'hir, D>(
    condition: &'hir Expr<'hir>,
    value: bool,
    state: &mut D,
    apply: &mut impl FnMut(Comparison<'hir>, &mut D),
) where
    D: Clone + JoinSemiLattice,
{
    match &condition.peel_parens().kind {
        ExprKind::Unary(op, inner) if op.kind == UnOpKind::Not => {
            apply_condition_facts(inner, !value, state, apply);
        }
        ExprKind::Binary(lhs, op, rhs) if matches!(op.kind, BinOpKind::And | BinOpKind::Or) => {
            let conjunctive =
                matches!((op.kind, value), (BinOpKind::And, true) | (BinOpKind::Or, false));
            if conjunctive {
                apply_condition_facts(lhs, value, state, apply);
                apply_condition_facts(rhs, value, state, apply);
            } else {
                let mut left = state.clone();
                apply_condition_facts(lhs, value, &mut left, apply);
                let mut right = state.clone();
                apply_condition_facts(rhs, value, &mut right, apply);
                _ = left.join(&right);
                *state = left;
            }
        }
        _ => {
            let Some(comparison) = Comparison::from_expr(condition) else { return };
            apply(if value { comparison } else { comparison.negated() }, state);
        }
    }
}

/// Negates a comparison operator.
pub const fn negate_comparison(op: BinOpKind) -> BinOpKind {
    match op {
        BinOpKind::Lt => BinOpKind::Ge,
        BinOpKind::Le => BinOpKind::Gt,
        BinOpKind::Gt => BinOpKind::Le,
        BinOpKind::Ge => BinOpKind::Lt,
        BinOpKind::Eq => BinOpKind::Ne,
        BinOpKind::Ne => BinOpKind::Eq,
        _ => op,
    }
}

/// Reverses a comparison operator's operand order.
pub const fn reverse_comparison(op: BinOpKind) -> BinOpKind {
    match op {
        BinOpKind::Lt => BinOpKind::Gt,
        BinOpKind::Le => BinOpKind::Ge,
        BinOpKind::Gt => BinOpKind::Lt,
        BinOpKind::Ge => BinOpKind::Le,
        _ => op,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{BinOp, ExprId};
    use solar_interface::Span;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Paths(Vec<Vec<BinOpKind>>);

    impl JoinSemiLattice for Paths {
        fn join(&mut self, other: &Self) -> bool {
            let old_len = self.0.len();
            for path in &other.0 {
                if !self.0.contains(path) {
                    self.0.push(path.clone());
                }
            }
            self.0.len() != old_len
        }
    }

    fn ident(index: usize) -> Expr<'static> {
        Expr { id: ExprId::new(index), kind: ExprKind::Ident(&[]), span: Span::DUMMY }
    }

    #[test]
    fn decomposes_conjunctive_and_disjunctive_edges() {
        let lhs = ident(0);
        let rhs = ident(1);
        let other_lhs = ident(2);
        let other_rhs = ident(3);
        let first = Expr {
            id: ExprId::new(4),
            kind: ExprKind::Binary(&lhs, BinOp { span: Span::DUMMY, kind: BinOpKind::Lt }, &rhs),
            span: Span::DUMMY,
        };
        let second = Expr {
            id: ExprId::new(5),
            kind: ExprKind::Binary(
                &other_lhs,
                BinOp { span: Span::DUMMY, kind: BinOpKind::Ne },
                &other_rhs,
            ),
            span: Span::DUMMY,
        };
        let condition = Expr {
            id: ExprId::new(6),
            kind: ExprKind::Binary(
                &first,
                BinOp { span: Span::DUMMY, kind: BinOpKind::And },
                &second,
            ),
            span: Span::DUMMY,
        };
        let mut apply = |comparison: Comparison<'_>, paths: &mut Paths| {
            for path in &mut paths.0 {
                path.push(comparison.op);
            }
        };

        let mut true_paths = Paths(vec![Vec::new()]);
        apply_condition_facts(&condition, true, &mut true_paths, &mut apply);
        assert_eq!(true_paths.0, [vec![BinOpKind::Lt, BinOpKind::Ne]]);

        let mut false_paths = Paths(vec![Vec::new()]);
        apply_condition_facts(&condition, false, &mut false_paths, &mut apply);
        assert_eq!(false_paths.0, [vec![BinOpKind::Ge], vec![BinOpKind::Eq]]);
    }
}
