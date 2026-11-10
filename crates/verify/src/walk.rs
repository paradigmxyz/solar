//! AST traversal helpers over [`Visit`].
use solar_parse::{
    ast::{Expr, ExprKind, Stmt, visit::Visit},
    interface::data_structures::Never,
};
use std::ops::ControlFlow;

/// Calls `f` on every statement directly nested in `s` (not on `s`, and not deeper).
pub(crate) fn sub_stmts<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Stmt<'a>)) {
    struct V<'f, 'a>(&'f mut dyn FnMut(&'a Stmt<'a>));
    impl<'a> Visit<'a> for V<'_, 'a> {
        type BreakValue = Never;
        fn visit_stmt(&mut self, s: &'a Stmt<'a>) -> ControlFlow<Never> {
            (self.0)(s);
            ControlFlow::Continue(())
        }
    }
    let _ = V(f).walk_stmt(s);
}

/// Calls `f` on every expression directly in `s`: not in nested statements, and not on the
/// subexpressions of those expressions.
pub(crate) fn stmt_exprs<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Expr<'a>)) {
    struct V<'f, 'a>(&'f mut dyn FnMut(&'a Expr<'a>));
    impl<'a> Visit<'a> for V<'_, 'a> {
        type BreakValue = Never;
        fn visit_stmt(&mut self, _: &'a Stmt<'a>) -> ControlFlow<Never> {
            ControlFlow::Continue(())
        }
        fn visit_expr(&mut self, e: &'a Expr<'a>) -> ControlFlow<Never> {
            (self.0)(e);
            ControlFlow::Continue(())
        }
    }
    let _ = V(f).walk_stmt(s);
}

/// Calls `f` on `s` and every statement nested in it, in pre-order.
pub(crate) fn all_stmts<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Stmt<'a>)) {
    struct V<'f, 'a>(&'f mut dyn FnMut(&'a Stmt<'a>));
    impl<'a> Visit<'a> for V<'_, 'a> {
        type BreakValue = Never;
        fn visit_stmt(&mut self, s: &'a Stmt<'a>) -> ControlFlow<Never> {
            (self.0)(s);
            self.walk_stmt(s)
        }
    }
    let _ = V(f).visit_stmt(s);
}

/// Calls `f` on `e` and every subexpression of it, in pre-order.
pub(crate) fn sub_exprs<'a>(e: &'a Expr<'a>, f: &mut dyn FnMut(&'a Expr<'a>)) {
    struct V<'f, 'a>(&'f mut dyn FnMut(&'a Expr<'a>));
    impl<'a> Visit<'a> for V<'_, 'a> {
        type BreakValue = Never;
        fn visit_expr(&mut self, e: &'a Expr<'a>) -> ControlFlow<Never> {
            (self.0)(e);
            self.walk_expr(e)
        }
    }
    let _ = V(f).visit_expr(e);
}

/// Root variable name of an lvalue (`a[i][j].x` -> `a`).
pub(crate) fn root<'a>(e: &'a Expr<'a>) -> Vec<String> {
    match &e.peel_parens().kind {
        ExprKind::Ident(i) => vec![i.as_str().to_string()],
        ExprKind::Index(b, _) | ExprKind::Member(b, _) => root(b),
        ExprKind::Tuple(xs) => {
            xs.iter().filter_map(|x| x.as_ref().unspan()).flat_map(|x| root(x)).collect()
        }
        _ => vec![],
    }
}
