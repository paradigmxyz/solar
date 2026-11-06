//! What type checking records for the AST JSON, only when the ASTs are kept.

use super::TypeChecker;
use crate::{
    hir,
    ty::{ExprFlags, YulReference},
};
use solar_ast::Span;
use solar_interface::Symbol;

impl<'gcx> TypeChecker<'gcx> {
    /// Records that the expression is an lvalue, for the AST JSON.
    pub(super) fn mark_lvalue(&mut self, id: hir::ExprId) {
        let exprs = self.results.ast.exprs.get_or_insert_default();
        exprs.update(id, |flags| flags.insert(ExprFlags::LVALUE));
    }

    /// Records whether the expression is pure and an lvalue, for the AST JSON.
    ///
    /// Its operands are recorded already, since they are checked first.
    pub(super) fn record_expr_facts(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        // solc does not treat a tuple that is written to as pure.
        let written_tuple = self.in_lvalue() && matches!(expr.kind, hir::ExprKind::Tuple(_));
        let pure = !written_tuple && self.results.is_pure(self.gcx, expr);
        let exprs = self.results.ast.exprs.get_or_insert_default();
        exprs.update(expr.id, |flags| {
            flags.insert(if pure { ExprFlags::PURE } else { ExprFlags::IMPURE });
            // A tuple that is written to is an lvalue.
            if written_tuple {
                flags.insert(ExprFlags::LVALUE);
            }
        });
    }

    /// Notes the variables that writing to `expr` writes: the variable, or those of a tuple.
    pub(super) fn note_write(&mut self, expr: &hir::Expr<'_>) {
        match expr.kind {
            hir::ExprKind::Tuple(components) => {
                for component in components.iter().flatten() {
                    self.note_write(component);
                }
            }
            _ => {
                if let Some(variable) =
                    self.results.resolved_expr(expr).and_then(|res| res.as_variable())
                {
                    self.loop_writes.push(variable);
                }
            }
        }
    }

    /// Records that a `for` loop cannot overflow its counter, if it is `for (...; i < n; i++)` or
    /// with `++i`, where `i` is a local variable of an integer type that the condition and the body
    /// do not write to, in Solidity or in inline assembly.
    ///
    /// `first_write` is where the writes of the loop start in `loop_writes`; the update itself
    /// writes the counter once.
    pub(super) fn record_simple_counter_loop(
        &mut self,
        stmt: &hir::Stmt<'gcx>,
        block: &hir::Block<'gcx>,
        update: &hir::Stmt<'gcx>,
        first_write: usize,
    ) {
        let gcx = self.gcx;
        // The lowered loop is `if (cond) { body } else break`, with a `break` of the span of the
        // loop.
        let (
            [hir::Stmt { kind: hir::StmtKind::If(cond, _, Some(break_stmt)), .. }],
            hir::StmtKind::Expr(next),
        ) = (block.stmts, &update.kind)
        else {
            return;
        };
        if !matches!(break_stmt.kind, hir::StmtKind::Break) || break_stmt.span != stmt.span {
            return;
        }
        let hir::ExprKind::Binary(lhs, op, _) = cond.kind else { return };
        let hir::ExprKind::Unary(inc, counter) = next.kind else { return };
        if op.kind != hir::BinOpKind::Lt
            || !matches!(inc.kind, hir::UnOpKind::PreInc | hir::UnOpKind::PostInc)
            || self.results.user_operator(cond.id).is_some()
            || self.results.user_operator(next.id).is_some()
            || !matches!(lhs.kind, hir::ExprKind::Ident(_))
            || !matches!(counter.kind, hir::ExprKind::Ident(_))
        {
            return;
        }
        let variable = self.results.resolved_expr(lhs).and_then(|res| res.as_variable());
        let Some(variable) = variable.filter(|&id| {
            Some(id) == self.results.resolved_expr(counter).and_then(|res| res.as_variable())
        }) else {
            return;
        };
        let lhs_ty = self.results.type_of_expr(lhs.id);
        if gcx.hir.variable(variable).is_local_variable()
            && lhs_ty.is_some_and(|ty| ty.is_integer())
            && lhs_ty == self.results.solc_ty(cond.id)
            && self.loop_writes[first_write..].iter().filter(|&&id| id == variable).count() == 1
        {
            self.results.ast.simple_counter_loops.insert(cond.id);
        }
    }

    /// Records a Yul identifier that names a variable, for the AST JSON.
    pub(super) fn record_yul_reference(
        &mut self,
        span: Span,
        suffix: Option<Symbol>,
        declaration: hir::VariableId,
    ) {
        self.results.ast.yul_references.push(YulReference { span, suffix, declaration });
    }
}
