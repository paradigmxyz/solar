//! Validates build-tool supplied test-link locations before removing bytecode dependencies.
//!
//! A stale location must fail compilation: silently embedding a supposedly dynamic dependency
//! would allow the build tool to reuse a test after its embedded contract changed. Locations
//! name the exclusive source byte offset of a `new C` or `type(C).creationCode` expression.

use solar_data_structures::{Never, map::FxHashSet};
use solar_interface::{Result, sym};
use solar_sema::{
    Gcx,
    hir::{self, Visit},
};
use std::ops::ControlFlow;

pub(crate) fn validate(gcx: Gcx<'_>) -> Result {
    if gcx.sess.opts.test_links.is_empty() {
        return Ok(());
    }
    let mut visitor =
        Validator { gcx, matched: FxHashSet::default(), constant: false, in_try: false };
    for source in gcx.hir.source_ids() {
        let _ = visitor.visit_nested_source(source);
    }
    for link in &gcx.sess.opts.test_links {
        if !visitor.matched.contains(&(link.source.clone(), link.end)) {
            gcx.dcx()
                .err(format!(
                    "test link `{}:{}` does not select a supported bytecode reference",
                    link.source, link.end
                ))
                .emit();
        }
    }
    gcx.dcx().has_errors()
}

struct Validator<'gcx> {
    gcx: Gcx<'gcx>,
    matched: FxHashSet<(String, usize)>,
    constant: bool,
    in_try: bool,
}

impl<'gcx> Visit<'gcx> for Validator<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        let supported = match &expr.kind {
            hir::ExprKind::New(ty) => {
                matches!(ty.kind, hir::TypeKind::Custom(hir::ItemId::Contract(_)))
            }
            hir::ExprKind::Member(base, name) if name.name == sym::creationCode => {
                matches!(
                    &base.kind,
                    hir::ExprKind::TypeCall(hir::Type {
                        kind: hir::TypeKind::Custom(hir::ItemId::Contract(_)),
                        ..
                    })
                )
            }
            _ => false,
        };
        if supported && !self.constant && !self.in_try && self.gcx.is_test_linked(expr.span) {
            let source = self.gcx.sess.source_map().span_to_source(expr.span).unwrap();
            self.matched.insert((source.file.name.display().to_string(), source.data.end));
        }
        self.walk_expr(expr)
    }

    fn visit_var(&mut self, variable: &'gcx hir::Variable<'gcx>) -> ControlFlow<Self::BreakValue> {
        let previous = self.constant;
        self.constant |= variable.is_constant();
        self.walk_var(variable)?;
        self.constant = previous;
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, statement: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if let hir::StmtKind::Try(value) = &statement.kind {
            let previous = self.in_try;
            self.in_try = true;
            self.visit_expr(&value.expr)?;
            self.in_try = previous;
            for clause in value.clauses {
                for &variable in clause.args {
                    self.visit_nested_var(variable)?;
                }
                for statement in clause.block.stmts {
                    self.visit_stmt(statement)?;
                }
            }
            return ControlFlow::Continue(());
        }
        self.walk_stmt(statement)
    }
}
