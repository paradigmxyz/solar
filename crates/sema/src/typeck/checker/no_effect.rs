use super::TypeChecker;
use crate::{
    builtins::Builtin,
    hir,
    ty::{Ty, TyKind},
};
use solar_ast::{BinOpKind, DataLocation, ElementaryType, UnOpKind};

impl<'gcx> TypeChecker<'gcx> {
    /// Returns whether an expression statement does nothing: evaluating it neither writes state
    /// nor can revert.
    ///
    /// Solc only reports compile-time constants (6133). This also reports reads of builtins and
    /// comparisons, but never an expression that could revert, such as checked arithmetic,
    /// division, indexing, or an enum conversion.
    ///
    /// A bare mention of a variable (`x;`) is not reported, since solc accepts it silently and
    /// code uses it to silence unused variable warnings and to force a function to read state.
    pub(super) fn has_no_effect(&self, expr: &hir::Expr<'gcx>) -> bool {
        !self.is_variable_mention(expr) && self.is_effect_free(expr)
    }

    fn is_variable_mention(&self, expr: &hir::Expr<'gcx>) -> bool {
        match expr.kind {
            hir::ExprKind::Tuple(exprs) => {
                exprs.iter().all(|expr| expr.is_none_or(|expr| self.is_variable_mention(expr)))
            }
            hir::ExprKind::Ident(_) => match self.results.resolved_expr(expr) {
                Some(hir::Res::Item(hir::ItemId::Variable(id))) => {
                    !self.gcx.hir.variable(id).is_constant()
                }
                Some(hir::Res::Builtin(Builtin::This | Builtin::Super)) => true,
                _ => false,
            },
            _ => false,
        }
    }

    fn is_effect_free(&self, expr: &hir::Expr<'gcx>) -> bool {
        let Some(ty) = self.results.type_of_expr(expr.id) else { return false };
        if ty.references_error() {
            return false;
        }
        if expr.is_numeric_literal() {
            return true;
        }
        if let Some(id) = self.constant_var(expr) {
            // A constant without a value runs its initializer, which can revert.
            return self.gcx.early_const_eval().eval_var(id).is_ok();
        }
        match expr.kind {
            hir::ExprKind::Array(exprs) => {
                exprs.iter().all(|expr| self.is_copied_effect_free(expr))
            }
            hir::ExprKind::Tuple(exprs) => {
                exprs.iter().all(|expr| expr.is_none_or(|expr| self.is_effect_free(expr)))
            }
            hir::ExprKind::Binary(lhs, op, rhs) => {
                self.results.user_operator(expr.id).is_none()
                    && is_non_reverting_binary(op.kind)
                    && self.is_effect_free(lhs)
                    && self.is_effect_free(rhs)
            }
            hir::ExprKind::Unary(op, inner) => {
                // Not `Neg`, which reverts on the minimum value in checked code.
                matches!(op.kind, UnOpKind::Not | UnOpKind::BitNot)
                    && self.results.user_operator(expr.id).is_none()
                    && self.is_effect_free(inner)
            }
            hir::ExprKind::Call(callee, ref args) => {
                let (callee, options) = callee.split_call_options();
                if options.is_some() || !args.exprs().all(|arg| self.is_copied_effect_free(arg)) {
                    return false;
                }
                // Only conversions and struct constructors, and not an enum conversion.
                let Some(callee_ty) = self.results.type_of_expr(callee.id) else { return false };
                let TyKind::Type(inner) = callee_ty.kind else { return false };
                matches!(
                    inner.peel_refs().kind,
                    TyKind::Elementary(_)
                        | TyKind::Contract(_)
                        | TyKind::Struct(_)
                        | TyKind::Udvt(..)
                )
            }
            hir::ExprKind::Ident(_) => match self.results.resolved_expr(expr) {
                None | Some(hir::Res::Err(_)) => false,
                Some(hir::Res::Item(hir::ItemId::Variable(id))) => !self.is_validated_load(id),
                Some(_) => true,
            },
            hir::ExprKind::Member(receiver, _) => {
                // Reading calldata decodes lazily and can revert on malformed data.
                self.results
                    .type_of_expr(receiver.id)
                    .is_some_and(|ty| !ty.is_ref_at(DataLocation::Calldata))
                    && self.is_effect_free(receiver)
            }
            hir::ExprKind::Lit(_) | hir::ExprKind::Type(_) | hir::ExprKind::TypeCall(_) => true,
            hir::ExprKind::Payable(inner) => self.is_effect_free(inner),
            hir::ExprKind::Ternary(cond, then, els) => {
                self.is_effect_free(cond)
                    && self.is_copied_effect_free(then)
                    && self.is_copied_effect_free(els)
            }
            hir::ExprKind::Assign(..)
            | hir::ExprKind::CallOptions(..)
            | hir::ExprKind::Delete(_)
            | hir::ExprKind::Index(..)
            | hir::ExprKind::Slice(..)
            | hir::ExprKind::New(_)
            | hir::ExprKind::YulMember(..)
            | hir::ExprKind::Err(_) => false,
        }
    }

    /// Like [`Self::is_effect_free`], for an operand that an array literal, ternary, or struct
    /// constructor can copy to memory. Copying calldata decodes it, and copying storage validates
    /// it and bounds dynamic lengths, so both can revert on malformed data.
    fn is_copied_effect_free(&self, expr: &hir::Expr<'gcx>) -> bool {
        self.is_effect_free(expr)
            && self.results.type_of_expr(expr.id).is_some_and(|ty| {
                !ty.is_ref_at(DataLocation::Calldata) && !ty.is_ref_at(DataLocation::Storage)
            })
    }

    /// Returns whether reading a variable loads storage whose value is validated, which reverts
    /// on malformed data: enums, and the headers of `bytes` and `string`.
    fn is_validated_load(&self, id: hir::VariableId) -> bool {
        let var = self.gcx.hir.variable(id);
        let in_storage = var.data_location == Some(DataLocation::Storage)
            || (var.is_state_variable() && !var.is_constant() && !var.is_immutable());
        in_storage && self.validates_on_load(self.gcx.type_of_item(id.into()), 0)
    }

    fn validates_on_load(&self, ty: Ty<'gcx>, depth: usize) -> bool {
        match ty.peel_refs().kind {
            TyKind::Enum(_)
            | TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => true,
            TyKind::Array(elem, _) | TyKind::DynArray(elem) => self.validates_on_load(elem, depth),
            // Recursive structs end at a bound.
            TyKind::Struct(id) => {
                depth > 4
                    || self
                        .gcx
                        .struct_field_types(id)
                        .iter()
                        .any(|&field| self.validates_on_load(field, depth + 1))
            }
            _ => false,
        }
    }
}

/// Returns whether a binary operator never reverts on built-in types: not checked arithmetic,
/// division, or modulo.
fn is_non_reverting_binary(op: BinOpKind) -> bool {
    !matches!(
        op,
        BinOpKind::Add
            | BinOpKind::Sub
            | BinOpKind::Mul
            | BinOpKind::Pow
            | BinOpKind::Div
            | BinOpKind::Rem
    )
}
