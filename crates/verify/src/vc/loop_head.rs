//! What a loop's head binds: for every loop the real run of verification reaches, the SSA term
//! each variable has at the loop's head and on entry, and what the variable is (a state
//! variable, a parameter, a local). Loop obligations name the head's terms (`|count@27|`), so a
//! proof about a loop maps them back to the source's storage slots and locals through this
//! record; `--emit-loop-heads` writes it as a manifest (`crate::loop_heads`).
//!
//! The record is taken where the head is built, in the run whose obligations the solver (and
//! `--dump-lean`) sees: inference's trial runs record nothing, and a function verified again
//! with Spacer's hints keeps only the last run's record.

use super::{Env, Vc};
use solar_parse::interface::Span;
use std::collections::BTreeSet;

/// One loop head reached by the real run of a function's verification.
#[derive(Clone, Debug)]
pub(crate) struct LoopHead {
    /// The loop statement.
    pub span: Span,
    /// The variables the loop's obligations range over, in name order.
    pub vars: Vec<HeadVar>,
}

/// A variable at a loop head.
#[derive(Clone, Debug)]
pub(crate) struct HeadVar {
    /// Its source name.
    pub name: String,
    /// `storage`, `transient`, `ghost`, `model` (the verifier's own state, such as
    /// `__balance`), `param`, `return` or `local`.
    pub kind: &'static str,
    /// Whether the loop writes it: the head then binds a fresh term, else the entry term.
    pub havocked: bool,
    /// Its term at the head.
    pub head: String,
    /// Its term on entry to the loop.
    pub entry: String,
    /// The terms of its length (dynamic arrays), at the head and on entry.
    pub head_len: Option<String>,
    pub entry_len: Option<String>,
    /// Its SMT sort.
    pub sort: String,
}

impl Vc<'_, '_> {
    /// Records the head of the loop at `span` over the variables `keep`, of which the loop writes
    /// `targets`; `entry` is the state on entry. Trial runs of inference record nothing.
    pub(super) fn record_loop_head(
        &mut self,
        span: Span,
        keep: &BTreeSet<String>,
        targets: &BTreeSet<String>,
        entry: &Env,
    ) {
        if self.trial > 0 {
            return;
        }
        let mut vars = vec![];
        for name in keep {
            let (Some(h), Some(e)) = (self.env.get(name), entry.get(name)) else { continue };
            vars.push(HeadVar {
                name: name.clone(),
                kind: self.var_kind(name),
                havocked: targets.contains(name),
                head: h.t.clone(),
                entry: e.t.clone(),
                head_len: h.len.clone(),
                entry_len: e.len.clone(),
                sort: h.ty.sort(),
            });
        }
        self.loop_heads.push(LoopHead { span, vars });
    }

    /// What the variable `name` of the current function is.
    fn var_kind(&self, name: &str) -> &'static str {
        if let Some(sv) =
            self.c.state.iter().find(|s| s.name == name && !s.constant && !s.immutable)
        {
            return if sv.ghost_decl.is_some() {
                "ghost"
            } else if sv.transient {
                "transient"
            } else if sv.ghost || name.starts_with("__") {
                "model"
            } else {
                "storage"
            };
        }
        if name.starts_with("__") {
            return "model";
        }
        let Some(f) = self.cur_func else { return "local" };
        if f.params.iter().any(|(n, _)| n == name) {
            "param"
        } else if f.rets.iter().any(|(n, _)| n == name) {
            "return"
        } else {
            "local"
        }
    }
}
