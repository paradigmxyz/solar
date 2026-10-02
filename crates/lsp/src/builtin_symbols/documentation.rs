//! Renders builtin signatures from the type selected by semantic analysis.
//!
//! Hover signatures retain their receiver's concrete types, including user-defined value types
//! and bound array operations.

use lsp_types::{MarkupContent, MarkupKind};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, StateMutability},
    ty::{Ty, TyKind},
};
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BuiltinDocumentation {
    builtin: Builtin,
    signature: String,
}

impl BuiltinDocumentation {
    pub(crate) fn for_expr<'gcx>(
        gcx: Gcx<'gcx>,
        expr: &hir::Expr<'gcx>,
        builtin: Builtin,
    ) -> Option<Self> {
        let expr = expr.peel_parens();
        // Several receiver-dependent builtins deliberately have no standalone builtin type.
        let ty = gcx.type_of_expr(expr.id)?;
        if ty.references_error() {
            return None;
        }
        let name = Self::name(gcx, expr, builtin);
        let signature = match ty.kind {
            _ if builtin.members().is_some() => format!("namespace {name}"),
            TyKind::Fn(function) => {
                let mut parameters = function.parameters;
                // Called members already have the implicit receiver removed by type checking.
                // A member value can still carry its unbound storage-array parameter.
                if function.attached
                    || (gcx.resolved_callee(expr.id).is_none() && builtin.is_array_mutator())
                {
                    parameters = parameters.get(1..)?;
                }
                let mut signature = format!("function {name}");
                Self::append_types(&mut signature, gcx, parameters);
                if function.state_mutability != StateMutability::NonPayable {
                    write!(signature, " {}", function.state_mutability).unwrap();
                }
                if !function.returns.is_empty() {
                    signature.push_str(" returns ");
                    Self::append_types(&mut signature, gcx, function.returns);
                }
                signature
            }
            _ => format!("{} {name}", ty.display(gcx)),
        };
        Some(Self { builtin, signature })
    }

    pub(crate) fn hover(&self) -> MarkupContent {
        MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```solidity\n{}\n```", self.signature),
        }
    }

    fn append_types<'gcx>(output: &mut String, gcx: Gcx<'gcx>, types: &[Ty<'gcx>]) {
        output.push('(');
        for (index, ty) in types.iter().enumerate() {
            if index != 0 {
                output.push_str(", ");
            }
            write!(output, "{}", ty.display(gcx)).unwrap();
        }
        output.push(')');
    }

    fn name<'gcx>(gcx: Gcx<'gcx>, expr: &hir::Expr<'gcx>, builtin: Builtin) -> String {
        let name = builtin.name();
        if let hir::ExprKind::Member(receiver, _) = expr.kind
            && let Some(receiver_ty) = gcx.type_of_expr(receiver.id)
        {
            match receiver_ty.kind {
                TyKind::Type(inner) => return format!("{}.{name}", inner.display(gcx)),
                TyKind::Fn(_) => return format!("function.{name}"),
                TyKind::Error(..) => return format!("error.{name}"),
                TyKind::Event(..) => return format!("event.{name}"),
                _ => return format!("{}.{name}", receiver_ty.display(gcx)),
            }
        }
        name.to_string()
    }
}
