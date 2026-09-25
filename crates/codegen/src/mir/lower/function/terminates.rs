//! `@custom:solar-terminates` functions and successful early exits under modifiers.
//!
//! A function tagged `@custom:solar-terminates` ends the call on every path: it reverts, returns
//! from the external call, or calls another function that does. Type checking proves this from
//! the body's structure, which makes the tag a checked requirement. Codegen needs nothing from it:
//! the optimizer already infers which functions never return, and ends their callers' blocks
//! after the call itself.
//!
//! Returning from the external call skips the code after `_` in every modifier still running,
//! and a skipped cleanup, such as a reentrancy lock's release, commits state no reverting exit
//! would. `Return.abiEncoded`, or a call that can reach it or a tagged function's successful
//! exit, is therefore rejected while a modifier's cleanup is pending. The check runs once the
//! contract is lowered: a call's callee can end the call successfully when it returns from the
//! external call itself, calls `Return.abiEncoded`, or calls another callee that can, and a tagged
//! function's own exits through untagged callees count too. Untagged functions that return
//! through inline assembly are left alone, as before these tags.
//!
//! NOTE: the modifier check follows calls, not internal function pointers: a pointer call from a
//! modifier's `_` to a function that returns from the external call is not reported.

use super::*;
use crate::mir::{Module, Terminator};

/// A call made while a modifier's code after `_` is still pending.
pub(in crate::mir::lower) struct PostludeCall {
    /// The callee.
    callee: FunctionId,
    /// The call expression.
    span: Span,
    /// The first statement of the pending modifier code.
    postlude: Span,
    /// The modifier's name.
    modifier: Symbol,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Records the internal call to `callee` at `span` for the check when a modifier's code after
    /// `_` is pending.
    pub(super) fn record_postlude_call(&mut self, callee: FunctionId, span: Span) {
        if let Some(&(postlude, modifier)) = self.pending_postludes.last() {
            self.cx.state.postlude_calls.push(PostludeCall { callee, span, postlude, modifier });
        }
    }

    /// Rejects `Return.abiEncoded` at `span` while a modifier's code after `_` is pending, and
    /// records that this function returns from the external call.
    pub(super) fn before_core_return(&mut self, span: Span) -> Option<()> {
        self.cx.state.returns_from_call = true;
        let Some(&(postlude, modifier)) = self.pending_postludes.last() else { return Some(()) };
        report_skipped_postlude(self.cx.gcx, span, postlude, modifier, true);
        None
    }
}

/// The first statement that runs after `_` in the modifier body `stmts`, if any runs.
///
/// A placeholder inside a loop runs the loop's remaining iterations after it, so the loop itself
/// is reported.
pub(super) fn modifier_postlude(stmts: &[hir::Stmt<'_>]) -> Option<Span> {
    for (index, stmt) in stmts.iter().enumerate() {
        if !contains_placeholder(stmt) {
            continue;
        }
        let inner = match &stmt.kind {
            StmtKind::Placeholder => None,
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                modifier_postlude(block.stmts)
            }
            StmtKind::If(_, then, else_) => modifier_postlude(std::slice::from_ref(*then))
                .or_else(|| else_.and_then(|else_| modifier_postlude(std::slice::from_ref(else_)))),
            _ => Some(stmt.span),
        };
        return inner.or_else(|| stmts.get(index + 1).map(|next| next.span));
    }
    None
}

fn contains_placeholder(stmt: &hir::Stmt<'_>) -> bool {
    match &stmt.kind {
        StmtKind::Placeholder => true,
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) | StmtKind::Loop(block, _) => {
            block.stmts.iter().any(contains_placeholder)
        }
        StmtKind::If(_, then, else_) => {
            contains_placeholder(then) || else_.is_some_and(contains_placeholder)
        }
        StmtKind::Try(stmt) => {
            stmt.clauses.iter().any(|clause| clause.block.stmts.iter().any(contains_placeholder))
        }
        _ => false,
    }
}

/// Rejects every recorded call made under a pending modifier cleanup whose callee can return from
/// the external call through `Return.abiEncoded` or a `@custom:solar-terminates` function.
///
/// `tagged` holds the functions carrying the tag, and `returning` those that lowered
/// `Return.abiEncoded` themselves.
pub(in crate::mir::lower) fn check_postlude_calls(
    gcx: Gcx<'_>,
    module: &Module,
    tagged: &FxHashSet<FunctionId>,
    returning: &FxHashSet<FunctionId>,
    calls: &[PostludeCall],
) {
    if calls.is_empty() {
        return;
    }
    let callees = |id: FunctionId| {
        let func = module.function(id);
        func.instructions()
            .filter_map(move |inst| match func.inst(inst).kind {
                InstKind::ICall { function: crate::mir::Callee::Function(callee), .. } => {
                    Some(callee)
                }
                _ => None,
            })
            .chain(func.blocks.iter().filter_map(|block| match block.terminator {
                Some(Terminator::TailCall { function, .. }) => Some(function),
                _ => None,
            }))
    };
    // Functions that can return from the external call, through any callee.
    let mut exits = module
        .functions
        .iter_enumerated()
        .filter(|(_, func)| {
            func.blocks.iter().any(|block| {
                matches!(
                    block.terminator,
                    Some(
                        Terminator::ReturnData { .. }
                            | Terminator::Stop
                            | Terminator::SelfDestruct { .. }
                    )
                )
            })
        })
        .map(|(id, _)| id)
        .collect::<FxHashSet<_>>();
    grow_to_callers(module, &mut exits, callees);
    // The exits this check is about: `Return.abiEncoded` and tagged functions' exits.
    let mut finishing = returning
        .iter()
        .copied()
        .chain(tagged.iter().copied().filter(|id| exits.contains(id)))
        .collect::<FxHashSet<_>>();
    grow_to_callers(module, &mut finishing, callees);
    for call in calls {
        if finishing.contains(&call.callee) {
            report_skipped_postlude(gcx, call.span, call.postlude, call.modifier, false);
        }
    }
}

/// Adds to `set` every function that calls one already in it, to a fixed point.
fn grow_to_callers<I: Iterator<Item = FunctionId>>(
    module: &Module,
    set: &mut FxHashSet<FunctionId>,
    callees: impl Fn(FunctionId) -> I,
) {
    loop {
        let before = set.len();
        for id in module.functions.indices() {
            if !set.contains(&id) && callees(id).any(|callee| set.contains(&callee)) {
                set.insert(id);
            }
        }
        if set.len() == before {
            return;
        }
    }
}

fn report_skipped_postlude(
    gcx: Gcx<'_>,
    span: Span,
    postlude: Span,
    modifier: Symbol,
    direct: bool,
) {
    let message = if direct {
        format!("this returns from the external call and skips the rest of modifier `{modifier}`")
    } else {
        format!(
            "this call can return from the external call and skip the rest of modifier `{modifier}`"
        )
    };
    gcx.dcx()
        .err(message)
        .span(span)
        .span_note(postlude, "this modifier code would not run")
        .help("revert instead, or return from the external call outside the modifier's `_`")
        .emit();
}
