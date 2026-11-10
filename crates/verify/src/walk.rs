use solar_parse::ast::{Expr, ExprKind, Stmt, StmtKind};

pub(crate) fn sub_stmts<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Stmt<'a>)) {
    match &s.kind {
        StmtKind::Block(b) | StmtKind::UncheckedBlock(b) => b.stmts.iter().for_each(&mut *f),
        StmtKind::If(_, a, b) => {
            f(a);
            if let Some(b) = b {
                f(b)
            }
        }
        StmtKind::While(_, b) | StmtKind::DoWhile(b, _) => f(b),
        StmtKind::For { init, body, .. } => {
            if let Some(i) = init {
                f(i)
            }
            f(body)
        }
        StmtKind::Try(t) => t.clauses.iter().for_each(|c| c.block.stmts.iter().for_each(&mut *f)),
        _ => {}
    }
}

/// Visit every expression directly in this statement (not nested statements).
pub(crate) fn stmt_exprs<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Expr<'a>)) {
    match &s.kind {
        StmtKind::DeclSingle(v) => {
            if let Some(i) = &v.initializer {
                f(i)
            }
        }
        StmtKind::DeclMulti(_, e) | StmtKind::Expr(e) => f(e),
        StmtKind::If(c, ..) | StmtKind::While(c, _) | StmtKind::DoWhile(_, c) => f(c),
        StmtKind::For { cond, next, .. } => {
            if let Some(c) = cond {
                f(c)
            }
            if let Some(n) = next {
                f(n)
            }
        }
        StmtKind::Return(Some(e)) => f(e),
        StmtKind::Emit(_, a) | StmtKind::Revert(_, a) => a.exprs().for_each(&mut *f),
        StmtKind::Try(t) => f(t.expr),
        _ => {}
    }
}

pub(crate) fn all_stmts<'a>(s: &'a Stmt<'a>, f: &mut dyn FnMut(&'a Stmt<'a>)) {
    f(s);
    sub_stmts(s, &mut |c| all_stmts(c, f));
}

pub(crate) fn sub_exprs<'a>(e: &'a Expr<'a>, f: &mut dyn FnMut(&'a Expr<'a>)) {
    f(e);
    let mut g = |x: &'a Expr<'a>| sub_exprs(x, f);
    match &e.kind {
        ExprKind::Array(xs) => xs.iter().for_each(|x| g(x)),
        ExprKind::Assign(a, _, b) | ExprKind::Binary(a, _, b) => {
            g(a);
            g(b)
        }
        ExprKind::Call(c, args) => {
            g(c);
            args.exprs().for_each(&mut g)
        }
        ExprKind::CallOptions(c, opts) => {
            g(c);
            opts.iter().for_each(|o| g(o.value))
        }
        ExprKind::Delete(x) | ExprKind::Member(x, _) | ExprKind::Unary(_, x) => g(x),
        ExprKind::Index(b, k) => {
            g(b);
            match k {
                solar_parse::ast::IndexKind::Index(Some(i)) => g(i),
                solar_parse::ast::IndexKind::Range(a, b2) => {
                    if let Some(a) = a {
                        g(a)
                    }
                    if let Some(b2) = b2 {
                        g(b2)
                    }
                }
                _ => {}
            }
        }
        ExprKind::Ternary(a, b, c) => {
            g(a);
            g(b);
            g(c)
        }
        ExprKind::Tuple(xs) => xs.iter().for_each(|x| {
            if let Some(x) = x.as_ref().unspan() {
                g(x)
            }
        }),
        ExprKind::Payable(args) => args.exprs().for_each(&mut g),
        _ => {}
    }
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
