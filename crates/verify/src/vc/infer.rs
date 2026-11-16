//! Loop invariant inference.
//!
//! A loop is cut at its head: the variables it writes are havocked, its invariants are assumed,
//! the body runs once and the invariants must hold again. Hand-written `@custom:invariant`s are
//! optional hints. This module finds the rest:
//!
//! 1. Candidates are mined at the head: relations between the variables the loop writes and the
//!    variables, lengths, entry values and constants it reads; the function's specs and the
//!    contract invariants, also with an array's length replaced by a counter the loop advances
//!    (`forall j :: j < xs.length ==> P` becomes `forall j :: j < i ==> P`); facts the abstract
//!    interpreter found for the loop (`Program::absint`); and formulas a Horn-clause solver
//!    proposed for it (see `super::horn`).
//! 2. Houdini keeps the largest subset whose conjunction holds on entry and is preserved by the
//!    body: candidates that fail on entry are dropped, then the body runs in a solver scope that is
//!    popped afterwards, under the conjunction of the survivors, and every candidate that does not
//!    hold after it is dropped, until none is.
//! 3. A termination measure is picked among the distances a guard `a < b`, `a <= b`, `a != b`
//!    bounds: one that is non-negative at the head and strictly smaller after the body.
//!
//! Nothing a solver answered during inference is trusted: the survivors and the measure are
//! then checked by the ordinary loop obligations (initiation, preservation, decrease) on the
//! real run, with the same `check` every other obligation goes through, and a failure there is a
//! verification failure. A query that times out drops the candidate, so inference can only lose
//! facts, never add unproved ones. Safety and termination stay separate: a loop without a
//! measure is still verified for partial correctness, and a measure is reported only when its
//! obligations were proved.

use super::{Env, Frame, LoopCtx, R, Var, Vc};
use crate::{prog::Spec, ty::Ty, walk};
use solar_parse::{
    ast::{BinOpKind, Expr, ExprKind, IndexKind, LitKind, Stmt, StmtKind, UnOpKind},
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
    },
};
use std::collections::BTreeSet;

/// The most candidates one loop starts Houdini with.
const MAX_CANDIDATES: usize = 400;

/// The most literal constants mined from a loop.
const MAX_CONSTANTS: usize = 8;

/// How a candidate is evaluated in a state.
#[derive(Clone)]
pub(crate) enum Shape<'p, 'a> {
    /// A spec expression, evaluated against the state; with `subst`, the value of the variable
    /// (or, with `len`, the length of the array) named first is replaced by the value of the
    /// counter named second, as `n := i` turns `count == old(count) + n` into an invariant.
    Spec { spec: &'p Spec<'a>, subst: Option<Subst> },
    /// A conjunct of a postcondition, for loops without a counter: with `copy` = (to, from) the
    /// value of `from` stands for `to` (the body's `to = from` makes a fact about `to` after
    /// the loop a fact about `from` one iteration earlier); with `or_guard`, the guard or the
    /// conjunct (the conjunct holds once the loop is about to stop).
    Conjunct { expr: &'a Expr<'a>, copy: Option<(String, String)>, or_guard: Option<&'a Expr<'a>> },
    /// An SMT term over placeholders: `|$v:x|` is the value of `x` in the state, `|$l:x|` its
    /// length, `|$e:x|` and `|$el:x|` the value and length at loop entry.
    Term(String),
}

/// A candidate invariant.
#[derive(Clone)]
pub(crate) struct Cand<'p, 'a> {
    pub shape: Shape<'p, 'a>,
    /// What diagnostics show.
    pub text: String,
    /// Where it came from: `guard`, `spec`, `absint`, `horn`.
    pub origin: &'static str,
}

/// `name` (its length when `len`) := the value of `counter`.
#[derive(Clone)]
pub(crate) struct Subst {
    name: String,
    len: bool,
    counter: String,
}

/// A termination measure `hi - lo + off` read off a guard.
#[derive(Clone)]
pub(crate) struct Measure<'a> {
    lo: Option<&'a Expr<'a>>,
    hi: Option<&'a Expr<'a>>,
    off: i64,
    pub text: String,
}

/// What the inference concluded about one loop, for reports (`--show-inferred`).
#[derive(Clone, Debug)]
pub(crate) struct Inferred {
    pub span: Span,
    pub invariants: Vec<(String, &'static str)>,
    pub measure: Option<String>,
    pub candidates: usize,
}

/// A loop's head as the Horn-clause encoding needs it (see `super::horn`): recorded on the real
/// run only.
#[derive(Clone, Debug)]
pub(crate) struct LoopRec {
    pub span: Span,
    /// The head's variables of value type, with their head terms and sorts.
    pub vars: Vec<(String, String, String)>,
    pub entry_guard: String,
    pub entry_terms: Vec<String>,
    /// The guard of the state reaching the head again and the variables' terms there.
    pub back_guard: String,
    pub back_terms: Vec<String>,
}

/// What a trial body run may change in the encoder and must give back.
pub(super) struct Snapshot<'p, 'a> {
    guard: String,
    env: Env,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    declared_funs: FxHashSet<String>,
    ranges: FxHashMap<String, crate::interval::Interval>,
    defs: FxHashMap<String, String>,
    fails: usize,
    warnings: usize,
    revert_sites: Vec<String>,
    splits: Vec<String>,
    cex_terms: Vec<(String, String)>,
    zero_checked: FxHashSet<String>,
    aliases: FxHashMap<String, &'a Expr<'a>>,
    slocals: FxHashSet<String>,
    alias_env: FxHashMap<String, Env>,
    ret_alias: Option<(&'a Expr<'a>, Env)>,
    ret_alias_out: Option<(&'a Expr<'a>, Env)>,
    storage_param_out: Vec<Var>,
    call_value: Option<String>,
    broadcast_on: FxHashSet<crate::ghost::FnKey>,
    inv_entry: FxHashSet<String>,
    checks: usize,
    inferred: usize,
    loop_recs: usize,
    ph: Vec<(&'p crate::prog::Func<'a>, usize, Env)>,
}

impl<'p, 'a> Vc<'p, 'a> {
    /// Execute a loop statement: cut at its head with the hand-written invariants and the
    /// inferred ones.
    pub(super) fn exec_loop(
        &mut self,
        s: &'a Stmt<'a>,
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
    ) -> R<()> {
        let invs = self.stmt_specs(s, "invariant");
        let decs = self.stmt_specs(s, "decreases");
        for inv in &invs {
            let v = self.eval_spec(inv.expr)?;
            self.check(&v.t, "loop invariant not satisfied on entry", inv.span, Some(inv));
        }
        let (entry_guard, entry_env) = (self.guard.clone(), self.env.clone());
        self.mem_call_writes.borrow_mut().clear();
        let targets = self.loop_targets(body, next);
        // NOTE: a call in the body that writes a memory array passed to it havocs that array at
        // the head, but the frame of what the call leaves unchanged is then quantified, and the
        // solver answers unknown where a concrete counterexample exists. Refuse such loops until
        // the call's writes are modelled precisely.
        if let Some((callee, arg)) = self.mem_call_writes.borrow_mut().drain(..).next() {
            return Err((
                s.span,
                format!(
                    "loop calls `{callee}`, which writes the memory array `{arg}` passed to it: \
                     not supported in a loop yet"
                ),
            ));
        }
        // Inference first runs Houdini's initiation check in the entry state.
        let mut cands = vec![];
        let infer = self.infer && self.guard != "false";
        if infer {
            cands = self.mine(s, cond, next, body, &targets, &entry_env);
            let mined: Vec<String> = cands.iter().map(|c| c.text.clone()).collect();
            cands = self.holds_in(cands, &entry_env, &entry_env);
            if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() && self.trial == 0 {
                for t in mined.iter().filter(|t| !cands.iter().any(|c| &c.text == *t)) {
                    eprintln!("infer: does not hold on entry: {t}");
                }
            }
        }
        let ncands = cands.len();
        for t in &targets {
            let ty = self.env[t].ty.clone();
            let v = self.fresh_var(t, &ty);
            self.env.insert(t.clone(), v);
        }
        let (head_guard, head_env) = (self.guard.clone(), self.env.clone());
        let mut measure = None;
        if infer {
            let measures = if decs.is_empty() { Self::guard_measures(cond) } else { vec![] };
            let (kept, m) = self.houdini(cands, &measures, &invs, cond, next, body, &entry_env)?;
            cands = kept;
            measure = m;
            // A loop that lowers its bound instead of raising the other side (Newton's iteration
            // `while (x < z) { z = x; ... }`): the bound alone, tried only when no distance
            // decreases, so that a loop with a distance measure queries nothing more.
            if self.bound_measures && measure.is_none() && decs.is_empty() {
                let bounds = Self::bound_measures(cond);
                if !bounds.is_empty() {
                    let (kept, m) =
                        self.houdini(cands, &bounds, &invs, cond, next, body, &entry_env)?;
                    cands = kept;
                    measure = m;
                }
            }
        }
        // The real run: every survivor and the measure are ordinary obligations from here on.
        if !cands.is_empty() {
            self.guard = entry_guard.clone();
            self.env = entry_env.clone();
            for c in &cands {
                if let Some(t) = self.cand_term(c, &entry_env, &entry_env) {
                    self.check(
                        &t,
                        &format!("inferred loop invariant not satisfied on entry: {}", c.text),
                        s.span,
                        None,
                    );
                }
            }
            self.guard = head_guard;
            self.env = head_env.clone();
        }
        for inv in &invs {
            let v = self.eval_spec(inv.expr)?;
            self.assume(&v.t);
        }
        let mut assumed = vec![];
        for c in &cands {
            let t = self.cand_term(c, &head_env, &entry_env);
            if let Some(t) = &t {
                self.assume(t);
            }
            assumed.push(t);
        }
        let d0: Vec<String> =
            decs.iter().map(|d| self.eval_spec(d.expr).map(|v| v.t)).collect::<R<_>>()?;
        let c = match cond {
            Some(c) => self.eval(c)?.t,
            None => "true".into(),
        };
        let (g_head, env_head) = (self.guard.clone(), self.env.clone());
        self.assume(&c);
        let m0 = match &measure {
            Some(m) => self.measure_term(m),
            None => None,
        };
        if decs.is_empty() && m0.is_none() {
            // Without a termination measure the loop may run until the gas runs out: a
            // succeeds-if cannot rely on it ever exiting.
            self.reverts_if("true");
        }
        let rec_at = self.loop_recs.len();
        self.loops.push(LoopCtx { breaks: vec![], conts: vec![] });
        self.exec_stmt(body)?;
        let ctx = self.loops.pop().unwrap();
        let mut st = ctx.conts;
        st.push((self.guard.clone(), self.env.clone()));
        let (g, e) = self.merge(st);
        self.guard = g;
        self.env = e;
        if let Some(n) = next {
            self.eval(n)?;
        }
        if self.trial == 0 {
            let keep = self.loop_vars(cond, next, body, &targets);
            let rec = self.loop_rec(s.span, &env_head, &entry_guard, &entry_env, &keep);
            self.loop_recs.insert(rec_at, rec);
        }
        for inv in &invs {
            let v = self.eval_spec(inv.expr)?;
            self.check(&v.t, "loop invariant not preserved by the loop body", inv.span, Some(inv));
        }
        let post_env = self.env.clone();
        for c in &cands {
            if let Some(t) = self.cand_term(c, &post_env, &entry_env) {
                self.check(
                    &t,
                    &format!("inferred loop invariant not preserved by the loop body: {}", c.text),
                    s.span,
                    None,
                );
            }
        }
        for (d, t0) in decs.iter().zip(&d0) {
            let v = self.eval_spec(d.expr)?;
            self.check(
                &format!("(and (<= 0 {t0}) (< {} {t0}))", v.t),
                "loop does not terminate: decreases clause not decreased",
                d.span,
                Some(d),
            );
        }
        if let (Some(m), Some(t0)) = (&measure, &m0) {
            match self.measure_term(m) {
                Some(t1) => self.check(
                    &format!("(and (<= 0 {t0}) (< {t1} {t0}))"),
                    &format!("inferred termination measure not decreased: {}", m.text),
                    s.span,
                    None,
                ),
                None => measure = None,
            }
        }
        if infer && self.trial == 0 {
            self.inferred.push(Inferred {
                span: s.span,
                invariants: cands.iter().map(|c| (c.text.clone(), c.origin)).collect(),
                measure: measure.as_ref().map(|m| m.text.clone()),
                candidates: ncands,
            });
        }
        // exit
        self.guard = g_head;
        self.env = env_head;
        self.assume(&format!("(not {c})"));
        let mut exits = ctx.breaks;
        exits.push((self.guard.clone(), self.env.clone()));
        let (g, e) = self.merge(exits);
        self.guard = g;
        self.env = e;
        Ok(())
    }

    /// Houdini over `cands`, which hold on entry, from the havocked head state: returns the
    /// largest subset preserved by the body, and the first measure that decreases under it.
    #[allow(clippy::too_many_arguments)]
    fn houdini(
        &mut self,
        mut cands: Vec<Cand<'p, 'a>>,
        measures: &[Measure<'a>],
        invs: &[&'p Spec<'a>],
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
        entry_env: &Env,
    ) -> R<(Vec<Cand<'p, 'a>>, Option<Measure<'a>>)> {
        loop {
            let snap = self.snapshot();
            self.smt.send("(push 1)\n");
            self.trial += 1;
            let r = self.trial_run(&cands, measures, invs, cond, next, body, entry_env);
            self.trial -= 1;
            self.smt.send("(pop 1)\n");
            self.restore(snap);
            let (dead, m) = r?;
            if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
                for &i in &dead {
                    eprintln!("infer: not preserved: {}", cands[i].text);
                }
            }
            if dead.is_empty() {
                return Ok((cands, m));
            }
            let mut i = 0;
            cands.retain(|_| {
                i += 1;
                !dead.contains(&(i - 1))
            });
        }
    }

    /// One trial body run: which candidates the body does not preserve, and, when it preserves
    /// them all, the first measure that decreases.
    #[allow(clippy::too_many_arguments)]
    fn trial_run(
        &mut self,
        cands: &[Cand<'p, 'a>],
        measures: &[Measure<'a>],
        invs: &[&'p Spec<'a>],
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
        entry_env: &Env,
    ) -> R<(BTreeSet<usize>, Option<Measure<'a>>)> {
        let head_env = self.env.clone();
        for inv in invs {
            let v = self.eval_spec(inv.expr)?;
            self.assume(&v.t);
        }
        for c in cands {
            if let Some(t) = self.cand_term(c, &head_env, entry_env) {
                self.assume(&t);
            }
        }
        let c = match cond {
            Some(c) => self.eval(c)?.t,
            None => "true".into(),
        };
        self.assume(&c);
        let m0: Vec<Option<String>> = measures.iter().map(|m| self.measure_term(m)).collect();
        self.loops.push(LoopCtx { breaks: vec![], conts: vec![] });
        self.exec_stmt(body)?;
        let ctx = self.loops.pop().unwrap();
        let mut st = ctx.conts;
        st.push((self.guard.clone(), self.env.clone()));
        let (g, e) = self.merge(st);
        self.guard = g;
        self.env = e;
        if let Some(n) = next {
            self.eval(n)?;
        }
        let post = self.env.clone();
        let terms: Vec<Option<String>> =
            cands.iter().map(|c| self.cand_term(c, &post, entry_env)).collect();
        let dead = self.failing(&terms);
        if !dead.is_empty() {
            return Ok((dead, None));
        }
        for (m, t0) in measures.iter().zip(m0) {
            let (Some(t0), Some(t1)) = (t0, self.measure_term(m)) else { continue };
            if self.valid(&format!("(and (<= 0 {t0}) (< {t1} {t0}))")) {
                return Ok((dead, Some(m.clone())));
            }
        }
        Ok((dead, None))
    }

    /// The candidates whose terms hold in the state `env` (under the current guard).
    fn holds_in(
        &mut self,
        cands: Vec<Cand<'p, 'a>>,
        env: &Env,
        entry_env: &Env,
    ) -> Vec<Cand<'p, 'a>> {
        let terms: Vec<Option<String>> =
            cands.iter().map(|c| self.cand_term(c, env, entry_env)).collect();
        let dead = self.failing(&terms);
        cands.into_iter().enumerate().filter(|(i, _)| !dead.contains(i)).map(|(_, c)| c).collect()
    }

    /// The indices of the terms not proved valid under the current guard (a term that could not
    /// be built fails). The conjunction is tried first, so a stable round costs one query.
    fn failing(&mut self, terms: &[Option<String>]) -> BTreeSet<usize> {
        let mut dead: BTreeSet<usize> =
            terms.iter().enumerate().filter(|(_, t)| t.is_none()).map(|(i, _)| i).collect();
        let live: Vec<(usize, &String)> =
            terms.iter().enumerate().filter_map(|(i, t)| t.as_ref().map(|t| (i, t))).collect();
        if live.is_empty() || self.guard == "false" {
            return dead;
        }
        let all = format!(
            "(and true {})",
            live.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" ")
        );
        if self.valid(&all) {
            return dead;
        }
        for (i, t) in live {
            if !self.valid(t) {
                dead.insert(i);
            }
        }
        dead
    }

    /// Is `phi` proved under the current guard? Unknown counts as no.
    fn valid(&mut self, phi: &str) -> bool {
        if self.guard == "false" || phi == "true" {
            return true;
        }
        let guard = self.guard.clone();
        let ms = self.smt.timeout_ms().min(3000);
        matches!(self.smt.check_with_timeout(&guard, phi, ms), super::Res::Unsat)
    }

    /// The term of candidate `c` in the state `env`, or None where it cannot be built there.
    pub(super) fn cand_term(&mut self, c: &Cand<'p, 'a>, env: &Env, entry: &Env) -> Option<String> {
        match &c.shape {
            Shape::Spec { spec, subst } => {
                let mut e = env.clone();
                if let Some(Subst { name, len, counter }) = subst {
                    let i = e.get(counter)?.t.clone();
                    let v = e.get_mut(name)?;
                    if *len {
                        v.len = Some(i);
                    } else {
                        v.t = i;
                    }
                }
                let saved = std::mem::replace(&mut self.env, e);
                self.trial += 1;
                let r = self.eval_spec(spec.expr);
                self.trial -= 1;
                self.env = saved;
                r.ok().map(|v| v.t)
            }
            Shape::Conjunct { expr, copy, or_guard } => {
                let mut e = env.clone();
                if let Some((to, from)) = copy {
                    let v = e.get(from)?.clone();
                    let w = e.get_mut(to)?;
                    if w.ty != v.ty {
                        return None;
                    }
                    *w = v;
                }
                let saved = std::mem::replace(&mut self.env, e);
                self.trial += 1;
                let c = self.eval_spec(expr).map(|v| v.t);
                let g = or_guard.map(|g| self.eval_spec(g).map(|v| v.t));
                self.trial -= 1;
                self.env = saved;
                match g {
                    None => c.ok(),
                    Some(g) => Some(format!("(or {} {})", g.ok()?, c.ok()?)),
                }
            }
            Shape::Term(t) => instantiate(t, env, entry),
        }
    }

    fn measure_term(&mut self, m: &Measure<'a>) -> Option<String> {
        self.trial += 1;
        let side = |e: Option<&'a Expr<'a>>, s: &mut Self| match e {
            Some(e) => s.eval_spec(e).ok().map(|v| v.t),
            None => Some("0".to_string()),
        };
        let hi = side(m.hi, self);
        let lo = side(m.lo, self);
        self.trial -= 1;
        Some(format!("(+ (- {} {}) {})", hi?, lo?, m.off))
    }

    /// Measures from the guard's comparisons: `b - a` for `a < b`, `b - a + 1` for `a <= b`,
    /// both orders for `a != b`; conjuncts each give theirs.
    fn guard_measures(cond: Option<&'a Expr<'a>>) -> Vec<Measure<'a>> {
        let mut out = vec![];
        let Some(c) = cond else { return out };
        let mut stack = vec![c];
        while let Some(e) = stack.pop() {
            let ExprKind::Binary(a, op, b) = &e.peel_parens().kind else { continue };
            let (a, b): (&'a Expr<'a>, &'a Expr<'a>) = (a, b);
            let mk = |lo: &'a Expr<'a>, hi: &'a Expr<'a>, off: i64| Measure {
                lo: Some(lo),
                hi: Some(hi),
                off,
                text: if off == 0 {
                    format!("{} - {}", expr_text(hi), expr_text(lo))
                } else {
                    format!("{} - {} + {off}", expr_text(hi), expr_text(lo))
                },
            };
            match op.kind {
                BinOpKind::And => {
                    stack.push(b);
                    stack.push(a);
                }
                BinOpKind::Lt => out.push(mk(a, b, 0)),
                BinOpKind::Le => out.push(mk(a, b, 1)),
                BinOpKind::Gt => out.push(mk(b, a, 0)),
                BinOpKind::Ge => out.push(mk(b, a, 1)),
                BinOpKind::Ne => {
                    out.push(mk(a, b, 0));
                    out.push(mk(b, a, 0));
                }
                _ => {}
            }
        }
        out
    }

    /// Measures from the bounds of the guard's comparisons alone: `b` for `a < b` and
    /// `a <= b`, `a` for `a > b` and `a >= b`; conjuncts each give theirs.
    fn bound_measures(cond: Option<&'a Expr<'a>>) -> Vec<Measure<'a>> {
        let mut out = vec![];
        let Some(c) = cond else { return out };
        let mut stack = vec![c];
        while let Some(e) = stack.pop() {
            let ExprKind::Binary(a, op, b) = &e.peel_parens().kind else { continue };
            let (a, b): (&'a Expr<'a>, &'a Expr<'a>) = (a, b);
            let hi = match op.kind {
                BinOpKind::And => {
                    stack.push(b);
                    stack.push(a);
                    continue;
                }
                BinOpKind::Lt | BinOpKind::Le => b,
                BinOpKind::Gt | BinOpKind::Ge => a,
                _ => continue,
            };
            out.push(Measure { lo: None, hi: Some(hi), off: 0, text: expr_text(hi) });
        }
        out
    }

    /// The candidates for a loop, before Houdini.
    fn mine(
        &mut self,
        s: &'a Stmt<'a>,
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
        targets: &BTreeSet<String>,
        entry: &Env,
    ) -> Vec<Cand<'p, 'a>> {
        // What the loop reads: identifiers in the guard, the step and the body.
        let mut read = BTreeSet::new();
        let mut consts = BTreeSet::new();
        let mut visit = |e: &'a Expr<'a>| {
            walk::sub_exprs(e, &mut |x| match &x.kind {
                ExprKind::Ident(id) => {
                    read.insert(id.as_str().to_string());
                }
                ExprKind::Lit(l, None) => {
                    if let LitKind::Number(n) = &l.kind
                        && let Ok(k) = n.to_string().parse::<u32>()
                    {
                        consts.insert(u64::from(k));
                    }
                }
                _ => {}
            });
        };
        for e in cond.into_iter().chain(next) {
            visit(e);
        }
        walk::all_stmts(body, &mut |st| walk::stmt_exprs(st, &mut visit));
        let int = |v: &Var| v.ty.bounds().is_some() && !matches!(v.ty, Ty::Arr(..));
        let mut cands: Vec<Cand<'p, 'a>> = vec![];
        let mut seen = FxHashSet::default();
        let mut push = |cands: &mut Vec<Cand<'p, 'a>>, t: String, text: String| {
            if seen.insert(t.clone()) {
                cands.push(Cand { shape: Shape::Term(t), text, origin: "guard" });
            }
        };
        // Operands: what the loop reads, array lengths, entry values, constants.
        let mut ops: Vec<(String, String)> = vec![];
        for n in read.iter().chain(targets.iter()) {
            let Some(v) = entry.get(n) else { continue };
            if int(v) {
                ops.push((format!("|$v:{n}|"), n.clone()));
            }
            if matches!(v.ty, Ty::Arr(_, None)) && v.len.is_some() {
                ops.push((format!("|$l:{n}|"), format!("{n}.length")));
            }
        }
        for n in targets {
            if entry.get(n).is_some_and(int) {
                ops.push((format!("|$e:{n}|"), format!("entry({n})")));
            }
        }
        consts.insert(0);
        consts.insert(1);
        for k in consts.iter().take(MAX_CONSTANTS) {
            ops.push((k.to_string(), k.to_string()));
        }
        ops.sort();
        ops.dedup();
        for x in targets {
            let Some(v) = entry.get(x) else { continue };
            if int(v) {
                let xt = format!("|$v:{x}|");
                // What the type's range already says is no invariant worth checking.
                let (lo, _) = v.ty.bounds().unwrap();
                for (yt, yn) in &ops {
                    if *yt == xt {
                        continue;
                    }
                    push(&mut cands, format!("(<= {xt} {yt})"), format!("{x} <= {yn}"));
                    if yt.parse::<num_bigint::BigInt>().is_ok_and(|k| k <= lo) {
                        continue;
                    }
                    push(&mut cands, format!("(>= {xt} {yt})"), format!("{x} >= {yn}"));
                    push(&mut cands, format!("(= {xt} {yt})"), format!("{x} == {yn}"));
                }
            } else if matches!(v.ty, Ty::Bool) {
                push(&mut cands, format!("(= |$v:{x}| |$e:{x}|)"), format!("{x} == entry({x})"));
            } else if let Ty::Arr(_, None) = v.ty
                && v.len.is_some()
            {
                // A loop that writes an array's elements keeps its length; the elements it has
                // not reached keep their entry values.
                push(
                    &mut cands,
                    format!("(= |$l:{x}| |$el:{x}|)"),
                    format!("{x}.length == entry({x}.length)"),
                );
                for i in targets {
                    if entry.get(i).is_some_and(int) {
                        push(
                            &mut cands,
                            format!(
                                "(forall ((k Int)) (=> (<= |$v:{i}| k) (= (select |$v:{x}| k) (select |$e:{x}| k))))"
                            ),
                            format!("forall k :: k >= {i} ==> {x}[k] == entry({x}[k])"),
                        );
                    }
                }
            }
        }
        // Specs of the function and the contract, as they are and with an array's length
        // replaced by a counter the loop advances.
        let mut specs: Vec<&'p Spec<'a>> = vec![];
        if let Some(f) = self.cur_func {
            specs.extend(f.ensures.iter());
            specs.extend(f.requires.iter());
        }
        specs.extend(self.c.invariants.iter());
        // Bounds the loop reads: array lengths, and value-type variables it does not write.
        let mut bounds: Vec<(String, bool)> = entry
            .iter()
            .filter(|(_, v)| matches!(v.ty, Ty::Arr(_, None)) && v.len.is_some())
            .map(|(n, _)| (n.clone(), true))
            .collect();
        bounds.extend(
            read.iter()
                .filter(|n| !targets.contains(*n) && entry.get(*n).is_some_and(int))
                .map(|n| (n.clone(), false)),
        );
        let counters: Vec<String> =
            targets.iter().filter(|n| entry.get(*n).is_some_and(int)).cloned().collect();
        for sp in specs {
            cands.push(Cand {
                shape: Shape::Spec { spec: sp, subst: None },
                text: sp.text.clone(),
                origin: "spec",
            });
            for (b, len) in bounds.iter().filter(|(b, _)| mentions(&sp.text, b)) {
                for i in &counters {
                    let shown = if *len { format!("{b}.length") } else { b.clone() };
                    cands.push(Cand {
                        shape: Shape::Spec {
                            spec: sp,
                            subst: Some(Subst { name: b.clone(), len: *len, counter: i.clone() }),
                        },
                        text: format!("{} [{shown} := {i}]", sp.text),
                        origin: "spec",
                    });
                }
            }
        }
        // Loops without a counter (a Newton iteration that runs while its next estimate
        // improves): each conjunct of a postcondition, the same through the body's plain copies
        // between variables, and the guard or the conjunct.
        let mut copies: Vec<(String, String)> = vec![];
        walk::all_stmts(body, &mut |st| {
            if let StmtKind::Expr(e) = &st.kind
                && let ExprKind::Assign(a, None, b) = &e.peel_parens().kind
                && let (ExprKind::Ident(a), ExprKind::Ident(b)) =
                    (&a.peel_parens().kind, &b.peel_parens().kind)
                && entry.get(a.as_str()).is_some_and(int)
                && entry.get(b.as_str()).is_some_and(int)
            {
                let pair = (a.as_str().to_string(), b.as_str().to_string());
                if !copies.contains(&pair) {
                    copies.push(pair);
                }
            }
        });
        if let Some(f) = self.cur_func {
            for sp in &f.ensures {
                let mut parts = vec![];
                conjuncts(sp.expr, &mut parts);
                if parts.len() < 2 {
                    continue;
                }
                for c in parts {
                    let text = expr_text(c);
                    let mk = |copy, or_guard, text| Cand {
                        shape: Shape::Conjunct { expr: c, copy, or_guard },
                        text,
                        origin: "spec",
                    };
                    cands.push(mk(None, None, text.clone()));
                    for (to, from) in &copies {
                        if mentions(&text, to) {
                            cands.push(mk(
                                Some((to.clone(), from.clone())),
                                None,
                                format!("{text} [{to} := {from}]"),
                            ));
                        }
                    }
                    if let Some(g) = cond {
                        cands.push(mk(None, Some(g), format!("({}) || ({text})", expr_text(g))));
                    }
                }
            }
        }
        cands.extend(self.absint_candidates(s));
        cands.extend(self.horn_candidates(s.span));
        cands.truncate(MAX_CANDIDATES);
        cands
    }

    /// Facts the abstract interpreter found for the loop `s` (`Program::absint`), evaluated
    /// like any spec. The abstract-interpretation work fills `Program::absint`; this is the one
    /// place inference reads it.
    fn absint_candidates(&self, s: &'a Stmt<'a>) -> Vec<Cand<'p, 'a>> {
        let prog: &'p crate::prog::Program<'a> = self.prog;
        prog.absint
            .get(&s.span)
            .map(|v| {
                v.iter()
                    .map(|sp| Cand {
                        shape: Shape::Spec { spec: sp, subst: None },
                        text: sp.text.clone(),
                        origin: "absint",
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Candidates a Horn-clause solver proposed for the loop at `span` (see `super::horn`).
    fn horn_candidates(&self, span: Span) -> Vec<Cand<'p, 'a>> {
        self.horn_hints.get(&span).cloned().unwrap_or_default()
    }

    /// The head of a loop as the Horn encoding sees it, once the body ran.
    fn loop_rec(
        &self,
        span: Span,
        head: &Env,
        entry_guard: &str,
        entry: &Env,
        keep: &BTreeSet<String>,
    ) -> LoopRec {
        let mut vars = vec![];
        let (mut entry_terms, mut back_terms) = (vec![], vec![]);
        for (n, v) in head.iter().filter(|(n, _)| keep.contains(*n)) {
            let sort = v.ty.sort();
            if sort != "Int" && sort != "Bool" {
                continue;
            }
            let (Some(e), Some(b)) = (entry.get(n), self.env.get(n)) else { continue };
            vars.push((n.clone(), v.t.clone(), sort));
            entry_terms.push(e.t.clone());
            back_terms.push(b.t.clone());
        }
        LoopRec {
            span,
            vars,
            entry_guard: entry_guard.to_string(),
            entry_terms,
            back_guard: self.guard.clone(),
            back_terms,
        }
    }

    /// The variables a Horn predicate of the loop ranges over: what it writes or reads, and the
    /// function's parameters and return variables.
    fn loop_vars(
        &self,
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
        targets: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut keep = targets.clone();
        let mut visit = |e: &'a Expr<'a>| {
            walk::sub_exprs(e, &mut |x| {
                if let ExprKind::Ident(id) = &x.kind {
                    keep.insert(id.as_str().to_string());
                }
            });
        };
        for e in cond.into_iter().chain(next) {
            visit(e);
        }
        walk::all_stmts(body, &mut |st| walk::stmt_exprs(st, &mut visit));
        if let Some(f) = self.cur_func {
            keep.extend(f.params.iter().map(|(n, _)| n.clone()));
            keep.extend(f.rets.iter().map(|(n, _)| n.clone()));
        }
        keep
    }

    pub(super) fn snapshot(&self) -> Snapshot<'p, 'a> {
        Snapshot {
            guard: self.guard.clone(),
            env: self.env.clone(),
            frames: self.frames.clone(),
            loops: self.loops.clone(),
            declared_funs: self.declared_funs.clone(),
            ranges: self.ranges.clone(),
            defs: self.defs.clone(),
            fails: self.fails.len(),
            warnings: self.warnings.len(),
            revert_sites: self.revert_sites.clone(),
            splits: self.splits.clone(),
            cex_terms: self.cex_terms.clone(),
            zero_checked: self.zero_checked.clone(),
            aliases: self.aliases.clone(),
            slocals: self.slocals.clone(),
            alias_env: self.alias_env.clone(),
            ret_alias: self.ret_alias.clone(),
            ret_alias_out: self.ret_alias_out.clone(),
            storage_param_out: self.storage_param_out.clone(),
            call_value: self.call_value.clone(),
            broadcast_on: self.broadcast_on.clone(),
            inv_entry: self.inv_entry.clone(),
            checks: self.checks,
            inferred: self.inferred.len(),
            loop_recs: self.loop_recs.len(),
            ph: self.ph.clone(),
        }
    }

    pub(super) fn restore(&mut self, s: Snapshot<'p, 'a>) {
        self.guard = s.guard;
        self.env = s.env;
        self.frames = s.frames;
        self.loops = s.loops;
        self.declared_funs = s.declared_funs;
        self.ranges = s.ranges;
        self.defs = s.defs;
        self.fails.truncate(s.fails);
        self.warnings.truncate(s.warnings);
        self.revert_sites = s.revert_sites;
        self.splits = s.splits;
        self.cex_terms = s.cex_terms;
        self.zero_checked = s.zero_checked;
        self.aliases = s.aliases;
        self.slocals = s.slocals;
        self.alias_env = s.alias_env;
        self.ret_alias = s.ret_alias;
        self.ret_alias_out = s.ret_alias_out;
        self.storage_param_out = s.storage_param_out;
        self.call_value = s.call_value;
        self.broadcast_on = s.broadcast_on;
        self.inv_entry = s.inv_entry;
        self.checks = s.checks;
        self.inferred.truncate(s.inferred);
        self.loop_recs.truncate(s.loop_recs);
        self.ph = s.ph;
    }
}

/// `t` with its placeholders replaced by the terms of `env` and `entry`, or None when one
/// names a variable the state does not have.
pub(crate) fn instantiate(t: &str, env: &Env, entry: &Env) -> Option<String> {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find("|$") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let j = after.find('|')?;
        let (kind, name) = after[..j].split_once(':')?;
        let term = match kind {
            "v" => env.get(name)?.t.clone(),
            "l" => env.get(name)?.len.clone()?,
            "e" => entry.get(name)?.t.clone(),
            "el" => entry.get(name)?.len.clone()?,
            _ => return None,
        };
        out.push_str(&term);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Does spec text mention the identifier `name`?
/// The top-level conjuncts of `e`, split on the AST so precedence is never misread.
fn conjuncts<'a>(e: &'a Expr<'a>, out: &mut Vec<&'a Expr<'a>>) {
    match &e.peel_parens().kind {
        ExprKind::Binary(a, op, b) if op.kind == BinOpKind::And => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        _ => out.push(e),
    }
}

fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let ok = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        ok(text[..i].chars().next_back()) && ok(text[i + name.len()..].chars().next())
    })
}

/// Solidity-like text of a simple expression, for diagnostics.
pub(crate) fn expr_text(e: &Expr<'_>) -> String {
    match &e.peel_parens().kind {
        ExprKind::Ident(id) => id.as_str().to_string(),
        ExprKind::Member(b, m) => format!("{}.{}", expr_text(b), m.as_str()),
        ExprKind::Index(b, IndexKind::Index(Some(i))) => {
            format!("{}[{}]", expr_text(b), expr_text(i))
        }
        ExprKind::Lit(l, _) => match &l.kind {
            LitKind::Number(n) => n.to_string(),
            _ => "<lit>".into(),
        },
        ExprKind::Binary(a, op, b) => {
            format!("({} {} {})", expr_text(a), op.kind.to_str(), expr_text(b))
        }
        ExprKind::Unary(op, x) if op.kind == UnOpKind::Neg => format!("-{}", expr_text(x)),
        _ => "<expr>".into(),
    }
}
