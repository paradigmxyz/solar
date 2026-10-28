//! `@custom:solar-terminates`: a function that ends the call on every path.
//!
//! The tag documents an internal or private function with a body, and turns "every path through
//! it ends the call" into a requirement: it reverts, halts, or calls another function the tag
//! guarantees ends the call, and never returns to its caller. Type checking proves this from the
//! structure of the body, so the tag is a checked statement about the code, which other compilers
//! read as documentation. Codegen needs nothing from it: the optimizer infers which functions
//! cannot return on its own, and ends its callers' blocks after the call.
//!
//! A statement ends the call when it reverts, calls a builtin that reverts or halts, calls a
//! non-virtual function tagged `@custom:solar-terminates`, or is a block, or an `if` with an
//! `else`, whose parts all do. Loops and `try` never end it here, since control can leave them,
//! and a `return` anywhere is reported. A virtual function may be overridden by one that returns,
//! and a modifier can skip the body and return, so neither counts: a call to a virtual tagged
//! function does not end the call, and a tagged function takes no modifiers.
//!
//! NOTE: A tagged function that ends the call successfully, through inline assembly's `return`
//! or `stop`, skips the code after `_` in the modifiers still running exactly as the same function
//! without the tag does; the tag does not reject it.

use crate::{
    builtins::Builtin,
    hir::{self, ExprKind, StmtKind},
    natspec::terminates_applies,
    ty::Gcx,
};
use solar_interface::{Span, sym};

/// Checks every function tagged `@custom:solar-terminates`.
pub(super) fn check(gcx: Gcx<'_>) {
    for id in gcx.hir.function_ids() {
        let Some(tag) = solar_terminates(gcx, id) else { continue };
        // A misplaced tag is reported with the documentation.
        if !terminates_applies(gcx, id.into()) {
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
        err.help("end every path with a revert, or a call to another `@custom:solar-terminates` function")
            .emit();
    }
}

/// Returns the span of the `@custom:solar-terminates` tag on the declaration of `id`.
fn solar_terminates(gcx: Gcx<'_>, id: hir::FunctionId) -> Option<Span> {
    gcx.hir
        .doc(gcx.hir.function(id).doc)
        .ast_comments
        .iter()
        .flat_map(|comment| comment.natspec.iter())
        .find_map(|natspec| {
            matches!(
                natspec.kind,
                solar_ast::NatSpecKind::Custom { name } if name.name == sym::solar_dash_terminates
            )
            .then_some(natspec.span)
        })
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
        | StmtKind::AssemblyBlock(block, _) => block_ends_call(gcx, block.stmts),
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

/// Whether evaluating `expr` ends the call: a call to a builtin that reverts or halts, or to a
/// `@custom:solar-terminates` function that no override can replace.
fn expr_ends_call(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> bool {
    let expr = expr.peel_parens();
    let call = match &expr.kind {
        ExprKind::Assign(_, None, rhs) => rhs.peel_parens(),
        _ => expr,
    };
    let Some((callee, _, _)) = call.as_call() else { return false };
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
    solar_terminates(gcx, function).is_some()
        && terminates_applies(gcx, function.into())
        && !gcx.hir.function(function).virtual_
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
