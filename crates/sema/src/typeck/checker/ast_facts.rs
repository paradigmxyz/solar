//! What type checking records for the AST JSON, only when the ASTs are kept.

use super::{TypeChecker, index_not_lvalue_reason};
use crate::{
    builtins::Builtin,
    hir,
    ty::{TyKind, TypeckResults, YulReference},
};
use solar_ast::Span;
use solar_interface::Symbol;

impl<'gcx> TypeChecker<'gcx> {
    /// Records that the expression is an lvalue, for the AST JSON.
    pub(super) fn mark_lvalue(&mut self, id: hir::ExprId) {
        self.results.ast.lvalues.insert(id);
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

impl TypeckResults<'_> {
    /// Returns whether the type-checked expression is an lvalue, in solc's sense, if the ASTs are
    /// kept.
    pub(crate) fn is_lvalue(&self, expr: &hir::Expr<'_>) -> bool {
        match expr.kind {
            hir::ExprKind::Call(callee, ..) => {
                self.builtin_callee(callee.id) == Some(Builtin::ArrayPush0)
            }
            hir::ExprKind::Index(base, _) => self.type_of_expr(base.id).is_some_and(|ty| {
                index_not_lvalue_reason(ty).is_none()
                    && !matches!(ty.peel_refs().kind, TyKind::Type(_) | TyKind::Slice(_))
            }),
            _ => self.ast.lvalues.contains(&expr.id),
        }
    }
}
