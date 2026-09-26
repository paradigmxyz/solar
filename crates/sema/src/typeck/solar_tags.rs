//! Checks of the `@custom:solar-*` tags that need types.
//!
//! A Solar tag states a requirement this compiler checks and relies on, while other compilers read
//! it as documentation. Each check here rejects a program whose tagged code breaks its tag, so a
//! program this compiler accepts behaves the same under a compiler that ignores the tags.

use crate::{
    builtins::Builtin,
    core::{CoreIntrinsic, intrinsic_of},
    hir::{self, ExprKind, SolarStmtTag, StmtKind},
    natspec::declaration_tag_applies,
    ty::{Gcx, TyKind},
};
use solar_ast::DataLocation;
use solar_interface::{Span, kw};

pub(super) fn check(gcx: Gcx<'_>) {
    check_views(gcx);
    check_view_parameters(gcx);
    check_terminates(gcx);
}

/// Checks that every `@custom:solar-view` declaration has a shape a view has: a `bytes memory`
/// variable initialized by `Bytes.slice` from `solar:core/v1/Bytes.sol`, or a declaration
/// initialized by `abi.decode` of `bytes` in memory or calldata into value types, `bytes`, and
/// `string`, whose `bytes memory` and `string memory` variables are the views.
///
/// Other compilers run the same declaration as the copy it names, so any other shape would give
/// the tag nothing to borrow.
fn check_views(gcx: Gcx<'_>) {
    for (tag, span) in gcx.hir.solar_views() {
        let (declaration, initializer) = match tag {
            SolarStmtTag::View(id) => {
                let variable = gcx.hir.variable(id);
                (variable.span, variable.initializer)
            }
            SolarStmtTag::DecodeView(_, expr) => (expr.span, Some(expr)),
            SolarStmtTag::Scratch(_) => continue,
        };
        let call = initializer.and_then(|initializer| match initializer.peel_parens().kind {
            ExprKind::Call(callee, args, _) => Some((callee, args)),
            _ => None,
        });
        let slice = call.is_some_and(|(callee, _)| {
            gcx.resolved_function(callee)
                .is_some_and(|function| intrinsic_of(gcx, function) == Some(CoreIntrinsic::Slice))
        });
        let decode =
            call.filter(|(callee, _)| gcx.resolved_builtin(callee) == Some(Builtin::AbiDecode));
        // A `bytes memory` variable, or a `string memory` one that only a decode makes.
        let viewable = |id: hir::VariableId| {
            let ty = gcx.type_of_item(id.into());
            ty.is_ref_at(DataLocation::Memory)
                && match ty.peel_refs().kind {
                    TyKind::Elementary(hir::ElementaryType::Bytes) => slice || decode.is_some(),
                    TyKind::Elementary(hir::ElementaryType::String) => decode.is_some(),
                    _ => false,
                }
        };
        let valid = match tag {
            SolarStmtTag::View(id) => viewable(id),
            SolarStmtTag::DecodeView(ids, _) => {
                decode.is_some() && ids.iter().flatten().any(|&id| viewable(id))
            }
            SolarStmtTag::Scratch(_) => false,
        };
        if !valid {
            gcx.dcx()
                .err(
                    "`@custom:solar-view` requires a `bytes memory` variable initialized by \
                     `Bytes.slice`, or `bytes` or `string` variables initialized by `abi.decode`",
                )
                .span(declaration)
                .span_note(span, "the tag is here")
                .help(
                    "declare the view as `bytes memory v = Bytes.slice(source, offset, count);` \
                     or `(bytes memory v) = abi.decode(data, (bytes));`",
                )
                .emit();
            continue;
        }
        if let Some((_, args)) = decode {
            check_decode_view(gcx, args, span);
        }
    }
}

/// Checks the arguments of an `abi.decode` a `@custom:solar-view` tag documents: the data is
/// `bytes` in memory or calldata, and every decoded type is a value type, `bytes`, or `string`.
fn check_decode_view(gcx: Gcx<'_>, args: hir::CallArgs<'_>, tag: Span) {
    let mut exprs = args.exprs();
    let (Some(data), Some(types)) = (exprs.next(), exprs.next()) else { return };
    if let Some(ty) = gcx.type_of_expr(data.id)
        && !((ty.is_ref_at(DataLocation::Memory) || ty.is_ref_at(DataLocation::Calldata))
            && matches!(ty.peel_refs().kind, TyKind::Elementary(hir::ElementaryType::Bytes)))
    {
        gcx.dcx()
            .err("`@custom:solar-view` decodes only `bytes` held in memory or calldata")
            .span(data.span)
            .span_note(tag, "the tag is here")
            .emit();
    }
    let types = match types.peel_parens().kind {
        ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
        _ => vec![types],
    };
    for ty_expr in types {
        let Some(TyKind::Type(ty)) = gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind) else {
            continue;
        };
        let supported = ty.is_value_type()
            || matches!(
                ty.peel_refs().kind,
                TyKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
            );
        if !supported {
            gcx.dcx()
                .err(format!("`@custom:solar-view` cannot decode `{}` in place", ty.display(gcx)))
                .span(ty_expr.span)
                .span_note(tag, "the tag is here")
                .note("a view decode supports value types, `bytes`, and `string`")
                .emit();
        }
    }
}

/// Checks that every `@custom:solar-view` tag on a function names `bytes memory` or `string memory`
/// parameters of it.
fn check_view_parameters(gcx: Gcx<'_>) {
    for id in gcx.hir.function_ids() {
        // A misplaced tag is reported with the documentation.
        if !declaration_tag_applies(gcx, id.into()) {
            continue;
        }
        let function = gcx.hir.function(id);
        for (name, tag) in gcx.hir.solar_view_names(id) {
            if name == kw::Empty {
                gcx.dcx()
                    .err("`@custom:solar-view` on a function must name its view parameters")
                    .span(tag)
                    .help("list the names, as in `@custom:solar-view data`")
                    .emit();
                continue;
            }
            let parameter = function.parameters.iter().copied().find(|&param| {
                gcx.hir.variable(param).name.is_some_and(|param| param.name == name)
            });
            let Some(parameter) = parameter else {
                gcx.dcx()
                    .err(format!(
                        "`@custom:solar-view` names `{name}`, which is not a parameter of `{}`",
                        gcx.item_name(id)
                    ))
                    .span(tag)
                    .emit();
                continue;
            };
            let ty = gcx.type_of_item(parameter.into());
            if !(ty.is_ref_at(DataLocation::Memory)
                && matches!(
                    ty.peel_refs().kind,
                    TyKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
                ))
            {
                gcx.dcx()
                    .err(format!(
                        "the view parameter `{name}` must be `bytes memory` or `string memory`"
                    ))
                    .span(gcx.hir.variable(parameter).span)
                    .span_note(tag, "the tag is here")
                    .emit();
            }
        }
    }
}

/// Checks that every function tagged `@custom:solar-terminates` ends the call on every path: it
/// reverts, returns from the external call, or calls a function that does, and never returns to
/// its caller.
///
/// The check follows the structure of the body. A statement ends the call when it reverts, calls a
/// function known to end it, or is a block or an `if` with an `else` whose parts all do. Loops and
/// `try` never end it here, since they can be left. A modifier could skip the body and return, so
/// a tagged function takes none.
fn check_terminates(gcx: Gcx<'_>) {
    for id in gcx.hir.function_ids() {
        let Some(tag) = gcx.hir.solar_terminates(id) else { continue };
        // A misplaced tag is reported with the documentation.
        if !declaration_tag_applies(gcx, id.into()) {
            continue;
        }
        let function = gcx.hir.function(id);
        let name = gcx.item_name(id);
        if let Some(modifier) = function.modifiers.first() {
            gcx.dcx()
                .err(format!(
                    "the `@custom:solar-terminates` function `{name}` cannot take modifiers"
                ))
                .span(modifier.span)
                .span_note(tag, "the tag is here")
                .note("a modifier can skip the body and return to the caller")
                .emit();
            continue;
        }
        let Some(body) = function.body else { continue };
        let exit = match block_ends_call(gcx, body.stmts) {
            Ok(true) => continue,
            Ok(false) => None,
            Err(ret) => Some(ret),
        };
        let mut err = gcx
            .dcx()
            .err(format!(
                "`{name}` is tagged `@custom:solar-terminates` but can return to its caller"
            ))
            .span(name.span)
            .span_note(tag, "the tag is here");
        err = match exit {
            Some(ret) => err.span_note(ret, "it returns here"),
            None => err.note("control can reach the end of its body"),
        };
        err.help(
            "end every path with a revert, `Revert.raw`, `Return.abiEncoded`, or a call to another \
             `@custom:solar-terminates` function",
        )
        .emit();
    }
}

/// Whether running `stmts` ends the call on every path: `Ok(true)` when it does, `Ok(false)` when
/// control can continue after them, and the span of a `return` that leaves the function otherwise.
fn block_ends_call(gcx: Gcx<'_>, stmts: &[hir::Stmt<'_>]) -> Result<bool, Span> {
    for stmt in stmts {
        if stmt_ends_call(gcx, stmt)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `stmt` ends the call on every path, as in [`block_ends_call`].
fn stmt_ends_call(gcx: Gcx<'_>, stmt: &hir::Stmt<'_>) -> Result<bool, Span> {
    match &stmt.kind {
        StmtKind::Revert(_) => Ok(true),
        StmtKind::Return(_) => Err(stmt.span),
        StmtKind::Expr(expr) => Ok(expr_ends_call(gcx, expr)),
        StmtKind::DeclSingle(id) => {
            Ok(gcx.hir.variable(*id).initializer.is_some_and(|expr| expr_ends_call(gcx, expr)))
        }
        StmtKind::DeclMulti(_, expr) => Ok(expr_ends_call(gcx, expr)),
        // Inline assembly is lowered to the same statements, calling builtins such as `revert`.
        StmtKind::Block(block)
        | StmtKind::UncheckedBlock(block)
        | StmtKind::AssemblyBlock(block) => block_ends_call(gcx, block.stmts),
        StmtKind::If(_, then, else_) => {
            let then = stmt_ends_call(gcx, then)?;
            let else_ = match else_ {
                Some(else_) => stmt_ends_call(gcx, else_)?,
                None => false,
            };
            Ok(then && else_)
        }
        // Control can leave a loop or a `try`, and a `return` inside one leaves the function.
        StmtKind::Loop(block, _) => find_return(block.stmts).map_or(Ok(false), Err),
        StmtKind::Try(stmt) => stmt
            .clauses
            .iter()
            .find_map(|clause| find_return(clause.block.stmts))
            .map_or(Ok(false), Err),
        _ => Ok(false),
    }
}

/// Whether evaluating `expr` ends the call: a call to a builtin that reverts or halts, to a
/// compiler-owned module function that ends the call, or to a `@custom:solar-terminates`
/// function that no override can replace.
fn expr_ends_call(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> bool {
    let expr = expr.peel_parens();
    let call = match &expr.kind {
        ExprKind::Assign(_, None, rhs) => rhs.peel_parens(),
        _ => expr,
    };
    let ExprKind::Call(callee, ..) = &call.kind else { return false };
    if let Some(builtin) = gcx.resolved_builtin(callee) {
        return matches!(
            builtin,
            Builtin::Revert
                | Builtin::RevertMsg
                | Builtin::Selfdestruct
                | Builtin::YulReturn
                | Builtin::YulRevert
                | Builtin::YulStop
                | Builtin::YulInvalid
                | Builtin::YulSelfdestruct
        );
    }
    let Some(function) = gcx.resolved_function(callee) else { return false };
    matches!(
        intrinsic_of(gcx, function),
        Some(CoreIntrinsic::RevertRaw | CoreIntrinsic::ReturnAbiEncoded)
    ) || (gcx.hir.solar_terminates(function).is_some()
        && declaration_tag_applies(gcx, function.into())
        && !gcx.hir.function(function).virtual_)
}

/// Returns the span of the first `return` among `stmts` and the statements nested in them.
fn find_return(stmts: &[hir::Stmt<'_>]) -> Option<Span> {
    stmts.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::Return(_) => Some(stmt.span),
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) | StmtKind::Loop(block, _) => {
            find_return(block.stmts)
        }
        StmtKind::If(_, then, else_) => find_return(std::slice::from_ref(*then))
            .or_else(|| else_.and_then(|else_| find_return(std::slice::from_ref(else_)))),
        StmtKind::Try(stmt) => {
            stmt.clauses.iter().find_map(|clause| find_return(clause.block.stmts))
        }
        _ => None,
    })
}
