use crate::{
    hir,
    ty::{Gcx, Ty, TyFn, TyKind},
};
use solar_ast::{StateMutability, UserDefinableOperator};
use solar_interface::Span;

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
        gcx.dcx()
            .err("only pure free functions can be used to define operators")
            .span(entry.span)
            .span_note(function.span, "function defined here")
            .emit();
    }

    let Some(using_ty) = using_ty else { return };
    let TyKind::Udvt(_, _) = using_ty.kind else {
        gcx.dcx()
            .err("operators can only be implemented for user-defined value types")
            .span(entry.span)
            .emit();
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
            .span_label(
                wrong_param_span(gcx, function, params, using_ty, op),
                format!(
                    "function `{}` needs to have {expected} to be used for operator `{}`",
                    gcx.item_name(function_id).as_str(),
                    op.to_str()
                ),
            )
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
            .span_label(
                wrong_return_span(gcx, function, returns, return_ty),
                format!(
                    "function `{}` needs to return `{}` to be used for operator `{}`",
                    gcx.item_name(function_id).as_str(),
                    return_ty.display(gcx),
                    op.to_str()
                ),
            )
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
            gcx.dcx()
                .err(format!(
                    "user-defined {} operator `{}` has more than one definition matching the operand type visible in the current scope",
                    if params.len() == 1 { "unary" } else { "binary" },
                    op.to_str()
                ))
                .span(entry.span)
                .emit();
        }
    }
}

fn wrong_param_span(
    gcx: Gcx<'_>,
    function: &hir::Function<'_>,
    params: &[Ty<'_>],
    using_ty: Ty<'_>,
    op: UserDefinableOperator,
) -> Span {
    let wrong_param = if let Some(i) = params.iter().position(|&ty| ty != using_ty) {
        function.parameters.get(i)
    } else if matches!(op, UserDefinableOperator::BitNot) && params.len() > 1 {
        function.parameters.get(1)
    } else if params.len() > 2 {
        function.parameters.get(2)
    } else {
        function.parameters.last()
    };
    wrong_param.map_or(function.span, |&id| gcx.hir.variable(id).ty.span)
}

fn wrong_return_span(
    gcx: Gcx<'_>,
    function: &hir::Function<'_>,
    returns: &[Ty<'_>],
    return_ty: Ty<'_>,
) -> Span {
    let wrong_return = if let Some(i) = returns.iter().position(|&ty| ty != return_ty) {
        function.returns.get(i)
    } else if returns.len() > 1 {
        function.returns.get(1)
    } else {
        function.returns.last()
    };
    wrong_return.map_or(function.span, |&id| gcx.hir.variable(id).ty.span)
}
