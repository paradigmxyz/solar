use super::TypeChecker;
use crate::{
    builtins::Builtin,
    hir,
    ty::{Ty, TyKind},
};
use solar_ast::ElementaryType;
use solar_interface::{Ident, sym};

impl<'gcx> TypeChecker<'gcx> {
    /// Returns whether a type-checked expression is a compile-time constant, like solc's `isPure`
    /// annotation.
    ///
    /// Expressions without a type or with an erroneous type count as pure to avoid follow-up
    /// errors.
    pub(super) fn is_pure(&self, expr: &hir::Expr<'gcx>) -> bool {
        let Some(ty) = self.results.type_of_expr(expr.id) else { return true };
        if ty.references_error() {
            return true;
        }
        match expr.kind {
            hir::ExprKind::Array(exprs) => exprs.iter().all(|expr| self.is_pure(expr)),
            hir::ExprKind::Tuple(exprs) => {
                exprs.iter().all(|expr| expr.is_some_and(|expr| self.is_pure(expr)))
            }
            hir::ExprKind::Assign(..)
            | hir::ExprKind::CallOptions(..)
            | hir::ExprKind::Delete(_)
            | hir::ExprKind::YulMember(..) => false,
            hir::ExprKind::Binary(lhs, _, rhs) => {
                self.results.user_operator(expr.id).is_none()
                    && self.is_pure(lhs)
                    && self.is_pure(rhs)
            }
            hir::ExprKind::Unary(op, inner) => {
                !op.kind.has_side_effects()
                    && self.results.user_operator(expr.id).is_none()
                    && self.is_pure(inner)
            }
            hir::ExprKind::Call(callee, ref args) => {
                let (callee, options) = callee.split_call_options();
                if options.is_some() || !args.exprs().all(|arg| self.is_pure(arg)) {
                    return false;
                }
                match self.results.type_of_expr(callee.id).map(|ty| ty.kind) {
                    // Type conversions and struct constructors.
                    Some(TyKind::Type(_)) | None => true,
                    Some(TyKind::Fn(_)) => {
                        let pure_fn =
                            self.results.builtin_callee(callee.id).is_some_and(Builtin::is_pure)
                                || matches!(callee.kind, hir::ExprKind::New(_));
                        pure_fn && self.is_pure(callee)
                    }
                    Some(_) => false,
                }
            }
            hir::ExprKind::Ident(_) => match self.results.resolved_expr(expr) {
                Some(hir::Res::Item(hir::ItemId::Variable(id))) => {
                    self.gcx.hir.variable(id).is_constant()
                }
                Some(hir::Res::Builtin(_)) => matches!(ty.kind, TyKind::Fn(_)),
                _ => matches!(ty.kind, TyKind::Type(_) | TyKind::Module(_)),
            },
            hir::ExprKind::Index(base, index) => {
                self.is_pure(base) && index.is_none_or(|index| self.is_pure(index))
            }
            hir::ExprKind::Slice(base, start, end) => {
                self.is_pure(base) && [start, end].into_iter().flatten().all(|e| self.is_pure(e))
            }
            hir::ExprKind::Lit(_)
            | hir::ExprKind::Type(_)
            | hir::ExprKind::TypeCall(_)
            | hir::ExprKind::Err(_) => true,
            hir::ExprKind::Member(receiver, ident) => {
                self.is_pure_member(expr, ty, receiver, ident)
            }
            hir::ExprKind::New(_) => !matches!(ty.kind, TyKind::Fn(f) if f.is_creation()),
            hir::ExprKind::Payable(inner) => self.is_pure(inner),
            hir::ExprKind::Ternary(cond, then, els) => {
                self.is_pure(cond) && self.is_pure(then) && self.is_pure(els)
            }
        }
    }

    /// Reference: <https://github.com/argotorg/solidity/blob/v0.8.37/libsolidity/analysis/TypeChecker.cpp#L3343-L3571>
    fn is_pure_member(
        &self,
        expr: &hir::Expr<'gcx>,
        ty: Ty<'gcx>,
        receiver: &hir::Expr<'gcx>,
        ident: Ident,
    ) -> bool {
        let Some(receiver_ty) = self.results.type_of_expr(receiver.id) else { return true };
        match receiver_ty.kind {
            // `this.f.selector`, `super.f.selector` and `<pure expression>.f.selector`.
            TyKind::Fn(f)
                if ident.name == sym::selector
                    && f.function_id.is_some_and(|id| !self.gcx.hir.function(id).is_getter()) =>
            {
                let hir::ExprKind::Member(inner, _) = receiver.kind else { return false };
                matches!(
                    inner.kind,
                    hir::ExprKind::Ident([hir::Res::Builtin(Builtin::This | Builtin::Super)])
                ) || self.is_pure(inner)
            }
            TyKind::Event(..)
            | TyKind::Error(..)
            | TyKind::BuiltinModule(Builtin::Abi)
            | TyKind::Meta(_) => true,
            TyKind::Type(inner) => match inner.kind {
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                | TyKind::Array(..)
                | TyKind::DynArray(_)
                | TyKind::Enum(_)
                | TyKind::Udvt(..) => true,
                TyKind::Contract(_) if matches!(ty.kind, TyKind::Fn(f) if f.is_declaration()) => {
                    self.is_pure(receiver)
                }
                _ => self.is_constant_res(expr),
            },
            TyKind::Module(_) => self.is_pure(receiver),
            _ => self.is_constant_res(expr),
        }
    }

    fn is_constant_res(&self, expr: &hir::Expr<'gcx>) -> bool {
        self.results
            .resolved_expr(expr)
            .and_then(|res| res.as_variable())
            .is_some_and(|id| self.gcx.hir.variable(id).is_constant())
    }
}
