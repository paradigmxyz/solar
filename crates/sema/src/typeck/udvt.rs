use crate::{
    hir,
    ty::{Gcx, Ty, TyFn, TyKind},
};
use solar_ast::{StateMutability, UserDefinableOperator};

pub(super) fn check_using_operator<'gcx>(
    gcx: Gcx<'gcx>,
    using: &hir::UsingDirective<'_>,
    entry: &hir::UsingEntry<'_>,
    function_id: hir::FunctionId,
    function_ty: &TyFn<'gcx>,
    using_ty: Option<Ty<'gcx>>,
    op: UserDefinableOperator,
) {
    let function = gcx.hir.function(function_id);

    if function_ty.state_mutability != StateMutability::Pure || !function.is_free() {
        gcx.dcx().emit_err_span_note(
            entry.span,
            "only pure free functions can be used to define operators",
            function.span,
            "function defined here",
        );
    }

    let Some(using_ty) = using_ty else { return };
    let TyKind::Udvt(_, _) = using_ty.kind else {
        gcx.dcx()
            .emit_err(entry.span, "operators can only be implemented for user-defined value types");
        return;
    };

    let params = function_ty.parameters;
    let is_unary_only = matches!(op, UserDefinableOperator::BitNot);
    let is_binary_only = !matches!(op, UserDefinableOperator::Sub | UserDefinableOperator::BitNot);
    let first_matches = params.first().is_some_and(|&ty| ty == using_ty);
    let first_two_match = params.len() < 2 || params[0] == params[1];

    let wrong_params = if is_binary_only && (params.len() != 2 || !first_two_match) {
        Some(format!("two parameters of type `{}`", using_ty.display(gcx)))
    } else if is_unary_only && (params.len() != 1 || !first_matches) {
        Some(format!("exactly one parameter of type `{}`", using_ty.display(gcx)))
    } else if params.len() >= 3 || !first_matches || !first_two_match {
        Some(format!("one or two parameters of type `{}`", using_ty.display(gcx)))
    } else {
        None
    };
    if let Some(expected) = wrong_params {
        gcx.dcx()
            .err("wrong parameters in operator definition")
            .span(function.span)
            .span_note(entry.span, "function was used to implement an operator here")
            .help(format!(
                "function `{}` needs to have {expected} to be used for operator `{}`",
                gcx.item_name(function_id).as_str(),
                op.to_str()
            ))
            .emit();
    }

    let return_ty = match op {
        UserDefinableOperator::Eq
        | UserDefinableOperator::Ne
        | UserDefinableOperator::Lt
        | UserDefinableOperator::Le
        | UserDefinableOperator::Gt
        | UserDefinableOperator::Ge => gcx.types.bool,
        _ => using_ty,
    };
    let returns = function_ty.returns;
    if returns.len() != 1 || returns[0] != return_ty {
        gcx.dcx()
            .err("wrong return parameters in operator definition")
            .span(function.span)
            .span_note(entry.span, "function was used to implement an operator here")
            .help(format!(
                "function `{}` needs to return `{}` to be used for operator `{}`",
                gcx.item_name(function_id).as_str(),
                return_ty.display(gcx),
                op.to_str()
            ))
            .emit();
    }

    if matches!(params.len(), 1 | 2) {
        let mut matches = 0;
        gcx.for_each_user_operator(
            using_ty,
            using.source,
            using.contract,
            op,
            params.len() == 1,
            &mut |_| matches += 1,
        );
        if matches >= 2 {
            gcx.dcx().emit_err(entry.span, format!(
                "user-defined {} operator `{}` has more than one definition matching the operand type visible in the current scope",
                if params.len() == 1 { "unary" } else { "binary" },
                op.to_str()
            ));
        }
    }
}
