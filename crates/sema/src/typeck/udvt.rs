use crate::{
    hir,
    ty::{
        Gcx, Ty, TyFn, TyKind, UserOperatorCandidate, user_operator_parameter_error,
        user_operator_return_type,
    },
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
    if let Some(err) = user_operator_parameter_error(using_ty, params, op) {
        gcx.dcx()
            .err("wrong parameters in operator definition")
            .span(function.span)
            .span_note(entry.span, "function was used to implement an operator here")
            .help(format!(
                "function `{}` needs to have {expected} to be used for operator `{}`",
                gcx.item_name(function_id).as_str(),
                op.to_str(),
                expected = err.expected(using_ty, gcx)
            ))
            .emit();
    }

    let return_ty = user_operator_return_type(gcx, using_ty, op);
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
            &mut |candidate| {
                if let UserOperatorCandidate::Function(candidate) = candidate
                    && !candidate.has_definition_error
                {
                    matches += 1;
                }
            },
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
