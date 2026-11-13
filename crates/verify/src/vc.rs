//! Verification-condition generation by guarded symbolic execution.
//!
//! State = (guard, env). `guard` is an SMT Bool naming the paths that reach the current point
//! without reverting; `env` maps every variable to its current SSA term. A reverting path
//! (require/revert/checked-arithmetic panic) simply drops out of the guard, which is exactly
//! Solidity's transaction semantics: reverted executions have no observable effect.
use crate::{
    broadcast,
    ghost::FnKey,
    lean::Role,
    prog::{Contract, FREE, Func, Program, Spec},
    smt::{Res, Smt},
    ty::{Ty, num, parse_ty_text, pow2},
    walk,
};
use num_bigint::BigInt;
use solar_parse::{
    ast::{self, BinOpKind, Expr, ExprKind, IndexKind, LitKind, Stmt, StmtKind, UnOpKind},
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
        sym,
    },
};
use std::collections::{BTreeMap, BTreeSet};

mod horn;
mod infer;
mod sample;
mod yul;

pub(crate) use sample::{Concrete, Sample};

#[derive(Clone, Debug)]
pub(crate) struct Var {
    pub ty: Ty,
    pub t: String,
    pub len: Option<String>,
}
pub(crate) type Val = Var;
type Env = BTreeMap<String, Var>;
type R<T> = Result<T, (Span, String)>;

pub(crate) struct Failure {
    pub kind: String,
    pub span: Span,
    pub spec: Option<(String, Span)>,
    pub cex: Vec<(String, String)>,
    pub note: Option<String>,
}

pub(crate) struct FnReport {
    pub name: String,
    pub checks: usize,
    pub fails: Vec<Failure>,
    pub unsupported: Option<(Span, String)>,
    pub warnings: Vec<String>,
    /// What inference concluded about each loop of the function (its real run).
    pub inferred: Vec<infer::Inferred>,
}

#[derive(Clone)]
struct Frame {
    rets: Vec<(String, Ty)>,
    exits: Vec<(String, Env)>,
}

#[derive(Clone)]
struct LoopCtx {
    breaks: Vec<(String, Env)>,
    conts: Vec<(String, Env)>,
}

pub(crate) struct Opts {
    pub strict: bool,
    /// Infer loop invariants and termination measures (see `infer`).
    pub infer: bool,
}

pub(crate) struct Vc<'p, 'a> {
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    pub smt: Smt,
    n: usize,
    guard: String,
    env: Env,
    old: Option<Env>,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    ph: Vec<(&'p Func<'a>, usize, Env)>,
    unchecked: bool,
    spec: bool,
    bound: Vec<String>,
    strict: bool,
    fails: Vec<Failure>,
    checks: usize,
    cex_terms: Vec<(String, String)>,
    depth: usize,
    declared_funs: FxHashSet<String>,
    splits: Vec<String>,
    /// The `value:` option of the call being evaluated (ETH it sends), consumed by external_call.
    call_value: Option<String>,
    /// Invariant terms assumed at function entry, before any branch: every path implies them, so a
    /// later check of the identical term (the state it reads is unchanged) needs no solver call.
    inv_entry: FxHashSet<String>,
    /// Path conditions under which the function reverts (revert, a failing require, a panic): a
    /// succeeds_if proof shows none is reachable. Loop havoc over-approximates the reachable
    /// states, so proving them unreachable is sound.
    pub(crate) revert_sites: Vec<String>,
    /// The span of the call a try statement makes: a revert there runs a catch clause instead of
    /// reverting us.
    try_call: Option<Span>,
    /// While computing what entry points open under the reentrancy lock may write: their external
    /// calls run under the same lock, so they reach only those entry points again.
    lock_modset: std::cell::Cell<bool>,
    /// Calls `loop_targets` found writing a memory reference argument: (callee, argument root).
    mem_call_writes: std::cell::RefCell<Vec<(String, String)>>,
    pub split_wins: usize,
    ranges: FxHashMap<String, crate::interval::Interval>,
    defs: FxHashMap<String, String>,
    warnings: Vec<String>,
    /// `@custom:reveal f(..)` being evaluated: unfold opaque `f` once
    reveal: Option<String>,
    /// structs whose invariant was already checked against the zero value in this function
    zero_checked: FxHashSet<String>,
    /// the function being verified (lemma cycles)
    cur_fn: Option<(String, String)>,
    rets: Vec<String>,
    aliases: FxHashMap<String, &'a Expr<'a>>,
    storage_param_out: Vec<Val>,
    /// The contract being verified: its state stays live (as `@host.<name>`) while library code
    /// runs, so external calls made from libraries havoc it and check its invariants.
    host: &'p Contract<'a>,
    /// storage-pointer locals (and storage return variables) of the running function
    slocals: FxHashSet<String>,
    /// values of the callee's names a pointer returned by a function was built from
    alias_env: FxHashMap<String, Env>,
    /// the location a just-inlined function's storage return points at
    ret_alias: Option<(&'a Expr<'a>, Env)>,
    ret_alias_out: Option<(&'a Expr<'a>, Env)>,
    /// Defining contract of each function body being executed (empty inside a modifier), for
    /// `super`.
    cur: Vec<String>,
    /// Broadcast lemmas proved before this VC (see `crate::broadcast::prove_all`): only those
    /// are assumed.
    pub(crate) proved: FxHashMap<FnKey, bool>,
    /// Broadcast lemmas asserted for the whole VC of the function being verified.
    broadcast_on: FxHashSet<FnKey>,
    /// The function being verified, whose proof no broadcast it rests on may assume.
    cur_key: Option<FnKey>,
    /// Internal functions referenced as values in this VC, with the symbolic tag each stands for
    /// (see `fn_tag`).
    fn_tags: Vec<(&'p Contract<'a>, &'p Func<'a>, String)>,
    /// Slot constants of the state variables whose `.slot` assembly read in this VC.
    slot_consts: Vec<String>,
    /// keccak terms assembly computed in this VC: the byte length and value of each input.
    keccak_in: FxHashMap<String, (BigInt, String)>,
    /// State variables whose `.slot` assembly anywhere in the program reads: a raw `sstore` may
    /// write them.
    slot_taken: BTreeSet<String>,
    /// Enum variables of the running function that assembly assigned a raw word: their range is
    /// owed at each Solidity-level read and at return (`check_raw_enum`), not at the assignment.
    raw_enums: FxHashSet<String>,
    /// Infer loop invariants (`Opts::infer`).
    infer: bool,
    /// Nesting depth of inference's trial runs: obligations met inside one are not checked.
    trial: usize,
    /// What inference concluded per loop on the real run.
    inferred: Vec<infer::Inferred>,
    /// Loop heads of the real run, for the Horn-clause fallback.
    loop_recs: Vec<infer::LoopRec>,
    /// Candidates the Horn-clause fallback proposed, per loop span.
    horn_hints: FxHashMap<Span, Vec<infer::Cand<'p, 'a>>>,
    /// The function being verified.
    cur_func: Option<&'p Func<'a>>,
    /// Guard and goal of each obligation that failed on the real run, for the Horn fallback.
    failed_goals: Vec<(String, String)>,
}

fn is_atom(t: &str) -> bool {
    !t.contains(' ')
}

impl<'p, 'a> Vc<'p, 'a> {
    pub(crate) fn new(prog: &'p Program<'a>, c: &'p Contract<'a>, mut smt: Smt, o: &Opts) -> Self {
        if !prog.structs.is_empty() {
            let heads: Vec<String> =
                prog.structs.iter().map(|(n, _)| format!("(|S.{n}| 0)")).collect();
            let ctors: Vec<String> = prog
                .structs
                .iter()
                .map(|(n, fs)| {
                    let fields: Vec<String> =
                        fs.iter().map(|(f, t)| format!("(|{n}.{f}| {})", t.sort())).collect();
                    format!("((|mk.{n}| {}))", fields.join(" "))
                })
                .collect();
            let mut pre = format!(
                "(declare-datatypes ({}) ({}))
",
                heads.join(" "),
                ctors.join(" ")
            );
            // every zero value first: a field's zero may be another struct's (in an array)
            for (n, _) in &prog.structs {
                pre.push_str(&format!(
                    "(declare-const |S.{n}.zero| |S.{n}|)
"
                ));
            }
            for (n, fs) in &prog.structs {
                for (f, t) in fs {
                    pre.push_str(&format!(
                        "(assert (= (|{n}.{f}| |S.{n}.zero|) {}))
",
                        t.zero()
                    ));
                }
            }
            smt.send(&pre);
        }
        Vc {
            prog,
            c,
            smt,
            n: 0,
            guard: "true".into(),
            env: Env::new(),
            old: None,
            frames: vec![],
            loops: vec![],
            ph: vec![],
            unchecked: false,
            spec: false,
            bound: vec![],
            strict: o.strict,
            fails: vec![],
            checks: 0,
            cex_terms: vec![],
            depth: 0,
            declared_funs: FxHashSet::default(),
            splits: vec![],
            split_wins: 0,
            call_value: None,
            inv_entry: FxHashSet::default(),
            revert_sites: vec![],
            try_call: None,
            lock_modset: std::cell::Cell::new(false),
            mem_call_writes: Default::default(),
            ranges: Default::default(),
            defs: Default::default(),
            warnings: vec![],
            reveal: None,
            zero_checked: FxHashSet::default(),
            cur_fn: None,
            rets: vec![],
            aliases: Default::default(),
            storage_param_out: vec![],
            host: c,
            cur: vec![],
            slocals: FxHashSet::default(),
            alias_env: Default::default(),
            ret_alias: None,
            ret_alias_out: None,
            proved: FxHashMap::default(),
            broadcast_on: FxHashSet::default(),
            cur_key: None,
            fn_tags: vec![],
            slot_consts: vec![],
            keccak_in: FxHashMap::default(),
            slot_taken: yul::slot_taken(prog),
            raw_enums: FxHashSet::default(),
            infer: o.infer,
            trial: 0,
            inferred: vec![],
            loop_recs: vec![],
            horn_hints: FxHashMap::default(),
            cur_func: None,
            failed_goals: vec![],
        }
    }

    /// A fact the model needs to go on, not a property of the program: proved on this path and
    /// then assumed, or else the function is not modelled (with a witness when the solver has
    /// one), never a failure blamed on the code.
    fn must_hold(&mut self, phi: &str, why: &str, span: Span) -> R<()> {
        if self.guard == "false" || phi == "true" {
            return Ok(());
        }
        let cex = self.cex_terms.clone();
        let guard = self.guard.clone();
        match self.smt.check(&guard, phi, &cex) {
            Res::Unsat => {
                self.assume(phi);
                Ok(())
            }
            Res::Sat(cex) if !cex.is_empty() => {
                let w: Vec<String> =
                    cex.iter().take(6).map(|(n, v)| format!("{n} = {v}")).collect();
                Err((span, format!("{why}; e.g. {}", w.join(", "))))
            }
            Res::Sat(_) => Err((span, why.to_string())),
            Res::Unknown(r) => Err((span, format!("{why} (solver returned {r})"))),
        }
    }

    /// Forgets what the solver scope being popped declared.
    fn clear_scope(&mut self) {
        self.declared_funs.clear();
        self.fn_tags.clear();
        self.slot_consts.clear();
        self.keccak_in.clear();
        self.raw_enums.clear();
    }

    /// An enum variable assembly may have set to a raw word must hold one of its variants where
    /// Solidity reads it or returns it: the model (and a compiler's cleanup) assumes it does. A
    /// read in a spec sees the raw word.
    fn check_raw_enum(&mut self, n: &str, span: Span) -> R<()> {
        if self.spec || !self.raw_enums.contains(n) {
            return Ok(());
        }
        let Some(v) = self.env.get(n).cloned() else { return Ok(()) };
        let Ty::Enum(en, _) = &v.ty else { return Ok(()) };
        let r = v.ty.range(&v.t).unwrap();
        let why = format!(
            "`{n}`, set in assembly, may hold a word outside enum `{en}` where Solidity uses it"
        );
        self.must_hold(&r, &why, span)
    }

    // ---------------------------------------------------------------- SMT helpers
    fn fresh(&mut self, base: &str) -> String {
        self.n += 1;
        let base: String =
            base.chars().map(|c| if c == '|' || c == '\\' { '_' } else { c }).collect();
        format!("|{base}@{}|", self.n)
    }
    fn def(&mut self, base: &str, sort: &str, term: &str) -> String {
        // inside a quantified spec a term may mention binder symbols: it cannot be named
        if is_atom(term) || !self.bound.is_empty() {
            return term.to_string();
        }
        let x = self.fresh(base);
        self.smt.send(&format!("(declare-const {x} {sort})\n(assert (= {x} {term}))\n"));
        if sort == "Int" {
            if term.len() < 4000 {
                self.defs.insert(x.clone(), term.to_string());
            }
            if let Some(r) = crate::interval::interval(term, &self.ranges) {
                self.ranges.insert(x.clone(), r);
            }
        }
        x
    }
    /// Bitvector view of an Int term. A variable whose range fits n bits gets a bitvector twin
    /// (x = bv2nat x#bv), declared once, so bitwise reasoning stays in bitvectors: solvers prove
    /// bit tricks on twins that they cannot on int2bv of an integer.
    fn bv_term(&mut self, t: &str, n: u32) -> String {
        let mut pending = vec![];
        let out = bv_view(t, n, &self.defs, &self.ranges, &mut self.declared_funs, &mut pending, 0);
        for p in pending {
            self.smt.send(&p);
        }
        out
    }
    fn iv(&self, t: &str) -> Option<crate::interval::Interval> {
        crate::interval::interval(t, &self.ranges)
    }
    /// `wrap`, unless the interval analysis shows the value is already in range.
    fn wrapr(&self, ty: &Ty, t: &str, lo: &BigInt, hi: &BigInt) -> String {
        if let Some((l, h)) = self.iv(t)
            && &l >= lo
            && &h <= hi
        {
            return t.to_string();
        }
        wrap(ty, t, lo, hi)
    }
    fn decl(&mut self, base: &str, ty: &Ty) -> String {
        let x = self.fresh(base);
        self.smt.send(&format!("(declare-const {x} {})\n", ty.sort()));
        // typing hint for bvlower: innermost element range of a mapping / array
        let mut inner = ty;
        while let Ty::Map(_, v) | Ty::Arr(v, _) = inner {
            inner = v;
        }
        if inner != ty
            && let Some((lo, hi)) = inner.bounds()
        {
            self.smt.send(&format!(";@elem {x} {lo} {hi}\n"));
        }
        if let Some(r) = ty.range(&x) {
            self.smt.send(&format!("(assert {r})\n"));
        }
        if let Some(b) = ty.bounds() {
            self.ranges.insert(x.clone(), b);
        }
        x
    }
    /// The length of an array value whose length we do not track: its size when the type is
    /// a fixed-size array, otherwise an unknown length (never 0, which would make every index
    /// out of bounds and its path vacuous).
    fn arr_len(&mut self, ty: &Ty) -> String {
        match ty.fixed_len() {
            Some(n) => n.to_string(),
            None => self.decl("len", &Ty::Uint(256)),
        }
    }
    fn fresh_var(&mut self, base: &str, ty: &Ty) -> Var {
        let t = self.decl(base, ty);
        let len = if let Ty::Arr(_, n) = ty {
            Some(match n {
                Some(n) => n.to_string(),
                None => self.decl(&format!("{base}.length"), &Ty::Uint(256)),
            })
        } else {
            None
        };
        Var { ty: ty.clone(), t, len }
    }
    fn zero_var(&self, ty: &Ty) -> Var {
        Var {
            ty: ty.clone(),
            t: ty.zero(),
            len: if let Ty::Arr(_, n) = ty { Some(n.unwrap_or(0).to_string()) } else { None },
        }
    }
    /// Typing hint for bvlower: the Int results of `name` lie in `ty`'s range.
    fn ret_bound(&mut self, name: &str, ty: &Ty) {
        if let Some((lo, hi)) = ty.bounds()
            && self.declared_funs.insert(format!("{name}/ret"))
        {
            self.smt.send(&format!(";@ret {name} {lo} {hi}\n"));
        }
    }
    fn declare_fun(&mut self, name: &str, arity: usize, ret: &str) {
        if self.declared_funs.insert(format!("{name}/{arity}")) {
            let args = vec!["Int"; arity].join(" ");
            self.smt.send(&format!("(declare-fun {name} ({args}) {ret})\n"));
        }
    }
    /// Collision resistance of a hash, as injectivity per arity (the usual verifier assumption).
    fn injective(&mut self, f: &str, arity: usize) {
        if arity == 0 || !self.declared_funs.insert(format!("{f}/{arity}/inj")) {
            return;
        }
        let xs: Vec<String> = (0..arity).map(|i| format!("x{i}")).collect();
        let binders: Vec<String> = xs.iter().map(|x| format!("({x} Int)")).collect();
        let app = format!("({f} {})", xs.join(" "));
        let mut s = String::new();
        for (i, x) in xs.iter().enumerate() {
            let inv = format!("{f}_inv{arity}_{i}");
            s.push_str(&format!(
                "(declare-fun {inv} (Int) Int)
"
            ));
            s.push_str(&format!(
                "(assert (forall ({}) (! (= ({inv} {app}) {x}) :pattern ({app}))))
",
                binders.join(" ")
            ));
        }
        self.smt.send(&s);
    }

    /// |pow2| e = 2^e for 0 <= e < 256, and 2^256 for e >= 256 (only reached as an exponent or
    /// shift amount, where 2^256 makes the wrapped or checked result exact). Negative e cannot
    /// occur: exponents and shift amounts are unsigned.
    fn ensure_pow2(&mut self) {
        if !self.declared_funs.insert("|pow2|".into()) {
            return;
        }
        let mut t = num(&pow2(256));
        for k in (0..256u32).rev() {
            t = format!("(ite (= e {k}) {} {t})", num(&pow2(k)));
        }
        self.smt.send(&format!(
            "(define-fun |pow2| ((e Int)) Int {t})
"
        ));
    }

    fn assume(&mut self, phi: &str) {
        if phi == "true" || self.guard == "false" {
            return;
        }
        let g = format!("(and {} {phi})", self.guard);
        self.guard = self.def("g", "Bool", &g);
    }
    fn check(&mut self, phi: &str, kind: &str, span: Span, spec: Option<&Spec<'a>>) {
        // A trial run of inference only learns which candidates survive.
        if self.trial > 0 {
            return;
        }
        self.checks += 1;
        let role = Role::Obligation {
            kind: kind.into(),
            span,
            spec: spec.map(|s| (s.text.clone(), s.span)),
        };
        if self.guard == "false" || phi == "true" {
            let why = if phi == "true" { "the goal is true" } else { "the guard is false" };
            let guard = self.guard.clone();
            self.smt.settled(&guard, phi, role, why);
            return;
        }
        let cex = self.cex_terms.clone();
        let guard = self.guard.clone();
        let parent = self.smt.next_query();
        self.smt.role = Some(role);
        let (cex, note) = match self.smt.check(&guard, phi, &cex) {
            Res::Unsat => return,
            Res::Sat(cex) => (cex, None),
            Res::Unknown(why) => {
                // Case split on the branch conditions seen so far: each case is a smaller query,
                // and together the cases cover every path, so proving all of them proves phi.
                // Bounded by sub-check count and by wall time (2x the per-query timeout), so a
                // hopeless query costs a bounded amount and the rest of the file still runs.
                let mut budget = 12usize;
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(2 * self.smt.timeout_ms());
                if !self.splits.is_empty()
                    && self.split_prove(&guard, phi, 0, &mut budget, deadline, parent)
                {
                    self.split_wins += 1;
                    self.smt.split(parent);
                    return;
                }
                (vec![], Some(format!("solver returned {why}")))
            }
        };
        self.failed_goals.push((guard, phi.to_string()));
        self.fails.push(Failure {
            kind: kind.into(),
            span,
            spec: spec.map(|s| (s.text.clone(), s.span)),
            cex,
            note,
        });
    }
    fn split_prove(
        &mut self,
        guard: &str,
        phi: &str,
        i: usize,
        budget: &mut usize,
        deadline: std::time::Instant,
        parent: usize,
    ) -> bool {
        if i >= self.splits.len() || *budget == 0 || std::time::Instant::now() >= deadline {
            return false;
        }
        let c = self.splits[i].clone();
        let sub = (self.smt.timeout_ms() / 3).max(2000);
        for lit in [c.clone(), format!("(not {c})")] {
            let left =
                deadline.saturating_duration_since(std::time::Instant::now()).as_millis() as u64;
            if *budget == 0 || left < 500 {
                return false;
            }
            *budget -= 1;
            let g = format!("(and {guard} {lit})");
            self.smt.role = Some(Role::Split { parent });
            let r = self.smt.check_with_timeout(&g, phi, sub.min(left));
            if !matches!(r, Res::Unsat)
                && (matches!(r, Res::Sat(_))
                    || !self.split_prove(&g, phi, i + 1, budget, deadline, parent))
            {
                return false;
            }
        }
        true
    }
    fn check_inv(&mut self, t: &str, kind: &str, span: Span, inv: &'p Spec<'a>) {
        if self.inv_entry.contains(t) {
            self.checks += 1;
            let role = Role::Obligation {
                kind: kind.into(),
                span,
                spec: Some((inv.text.clone(), inv.span)),
            };
            let guard = self.guard.clone();
            self.smt.settled(&guard, t, role, "assumed at entry, over unchanged state");
            return;
        }
        self.check(t, kind, span, Some(inv));
    }
    /// `@custom:check P`: the claim runs as a require (`--instrument`), so a path where it is
    /// false reverts and every later step may use it. If the solver proves it here anyway, say
    /// so: the runtime check can become a proof.
    fn runtime_check(&mut self, sp: &Spec<'a>) -> R<()> {
        let v = self.eval_spec(sp.expr)?;
        self.checks += 1;
        if self.guard != "false" && v.t != "true" {
            let guard = self.guard.clone();
            self.smt.role = Some(Role::Advisory);
            if matches!(self.smt.check(&guard, &v.t, &[]), Res::Unsat) {
                let w = format!(
                    "note: `@custom:check {}` is proved here: it can become an @custom:assert (statement) or @custom:requires (function), with no runtime check",
                    sp.text
                );
                if !self.warnings.contains(&w) {
                    self.warnings.push(w);
                }
            }
        }
        self.reverts_if(&format!("(not {})", v.t));
        self.assume(&v.t);
        Ok(())
    }

    fn iface_env(&mut self, f: &'p Func<'a>, args: &[Val], rets: &[Val]) -> (Env, Vec<String>) {
        let saved = std::mem::take(&mut self.env);
        for k in ["this", "block.timestamp"] {
            if let Some(v) = saved.get(k) {
                self.env.insert(k.to_string(), v.clone());
            }
        }
        // in the callee, msg.sender is the calling contract
        if let Some(v) = saved.get("this") {
            self.env.insert("msg.sender".to_string(), v.clone());
        }
        for ((n, ty), a) in f.params.iter().zip(args) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
        }
        for ((n, ty), a) in f.rets.iter().zip(rets) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
        }
        let r = std::mem::replace(&mut self.rets, f.rets.iter().map(|(n, _)| n.clone()).collect());
        (saved, r)
    }
    fn iface_requires(&mut self, f: &'p Func<'a>, args: &[Val], span: Span) -> R<()> {
        let (saved, rets) = self.iface_env(f, args, &[]);
        let r = (|| -> R<()> {
            for rq in &f.requires {
                let v = self.eval_spec(rq.expr).map_err(|(s, m)| (s, format!("interface specs may mention only the call's parameters and returns: {m}")))?;
                self.check(
                    &v.t,
                    &format!("precondition of external `{}.{}` not satisfied", f.contract, f.name),
                    span,
                    Some(rq),
                );
            }
            Ok(())
        })();
        self.env = saved;
        self.rets = rets;
        r
    }
    /// An interface function's succeeds-if conditions at this call, as one term.
    fn iface_succeeds(&mut self, f: &'p Func<'a>, args: &[Val]) -> R<String> {
        let (saved, rets) = self.iface_env(f, args, &[]);
        let r = (|| -> R<String> {
            let mut ps = vec![];
            for sp in &f.succeeds {
                ps.push(self.eval_spec(sp.expr)?.t);
            }
            Ok(if ps.len() == 1 { ps.pop().unwrap() } else { format!("(and {})", ps.join(" ")) })
        })();
        self.env = saved;
        self.rets = rets;
        r
    }
    fn iface_ensures(&mut self, f: &'p Func<'a>, args: &[Val], out: &[Val], _span: Span) -> R<()> {
        let (saved, rets) = self.iface_env(f, args, out);
        let r = (|| -> R<()> {
            for en in &f.ensures {
                let v = self.eval_spec(en.expr).map_err(|(s, m)| (s, format!("interface specs may mention only the call's parameters and returns: {m}")))?;
                self.assume(&v.t);
            }
            Ok(())
        })();
        self.env = saved;
        self.rets = rets;
        r
    }

    /// A struct's `@custom:invariant`s about the value `t`, as one term (None when it has none).
    fn struct_inv(&mut self, sn: &str, t: &str) -> R<Option<String>> {
        let prog = self.prog;
        let Some(invs) = prog.struct_invs.get(sn) else {
            return Ok(None);
        };
        let fields = prog.struct_fields(sn).cloned().unwrap_or_default();
        let saved = self.env.clone();
        for (f, ty) in &fields {
            let ft = format!("(|{sn}.{f}| {t})");
            if let Some(r) = ty.range(&ft) {
                self.smt.send(&format!(
                    "(assert {r})
"
                ));
            }
            self.env.insert(
                f.clone(),
                Var { ty: ty.clone(), t: ft, len: ty.fixed_len().map(|n| n.to_string()) },
            );
        }
        let mut parts = vec![];
        let mut err = None;
        for inv in invs {
            match self.eval_spec(inv.expr) {
                Ok(v) => parts.push(v.t),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        self.env = saved;
        if let Some(e) = err {
            return Err(e);
        }
        Ok(Some(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            format!("(and {})", parts.join(" "))
        }))
    }
    /// Storage and memory start zeroed, so a struct invariant must hold of the zero value before
    /// stored values may be assumed to satisfy it.
    fn check_struct_zero(&mut self, sn: &str, span: Span) -> R<()> {
        if !self.zero_checked.insert(sn.to_string()) {
            return Ok(());
        }
        if let Some(z) = self.struct_inv(sn, &format!("|S.{sn}.zero|"))? {
            let g = std::mem::replace(&mut self.guard, "true".into());
            self.check(&z, &format!("invariant of struct `{sn}` does not hold for its zero value (storage and memory start zeroed)"), span, None);
            self.guard = g;
        }
        Ok(())
    }
    /// A struct value read from storage satisfies its invariant: every write was checked, and
    /// the zero value satisfies it.
    fn assume_stored_struct(&mut self, base: &'a Expr<'a>, v: &Val, span: Span) -> R<()> {
        if self.spec && !self.bound.is_empty() {
            return Ok(());
        }
        let Ty::Struct(sn) = &v.ty else { return Ok(()) };
        if !self.prog.struct_invs.contains_key(sn) || !self.rooted_in_state(base) {
            return Ok(());
        }
        let sn = sn.clone();
        self.check_struct_zero(&sn, span)?;
        if let Some(i) = self.struct_inv(&sn, &v.t)? {
            self.assume(&i);
        }
        Ok(())
    }
    fn rooted_in_state(&self, e: &'a Expr<'a>) -> bool {
        let c = self.c;
        crate::walk::root(e).iter().any(|r| {
            c.state.iter().any(|s| &s.name == r && !s.ghost)
                || self.aliases.get(r).is_some_and(|t| {
                    crate::walk::root(t).iter().any(|r2| c.state.iter().any(|s| &s.name == r2))
                })
        })
    }
    /// A struct value written to storage must satisfy its invariant.
    fn check_stored_struct(&mut self, target: &'a Expr<'a>, v: &Val, span: Span) -> R<()> {
        if self.spec {
            return Ok(());
        }
        let Ty::Struct(sn) = &v.ty else { return Ok(()) };
        if !self.prog.struct_invs.contains_key(sn) || !self.rooted_in_state(target) {
            return Ok(());
        }
        let sn = sn.clone();
        self.check_struct_zero(&sn, span)?;
        if let Some(i) = self.struct_inv(&sn, &v.t)? {
            self.check(
                &i,
                &format!("invariant of struct `{sn}` may not hold for the value stored"),
                span,
                None,
            );
        }
        Ok(())
    }

    /// `@custom:use L(args)`: an erased lemma call. Its requires are proved here and its ensures
    /// become facts; nothing runs and no bytecode changes.
    fn use_lemma(&mut self, sp: &Spec<'a>) -> R<()> {
        let ExprKind::Call(callee, args) = &sp.expr.peel_parens().kind else {
            return Err((sp.span, "@custom:use takes a lemma call, such as `lemma(x, y)`".into()));
        };
        let (lc, lf) = self.resolve_lemma(callee, args.exprs().count(), sp.span)?;
        if !matches!(lf.item.header.state_mutability(), ast::StateMutability::Pure) {
            return Err((sp.span, format!("lemma `{}` must be pure: a use is erased", lf.name)));
        }
        if let Some((cc, cf)) = self.cur_fn.clone() {
            let mut seen = FxHashSet::default();
            if (lc.name == cc && lf.name == cf) || self.lemma_reaches(lc, lf, &cc, &cf, &mut seen) {
                return Err((
                    sp.span,
                    format!(
                        "lemma `{}` is used by a function it reaches: a cyclic proof is not allowed",
                        lf.name
                    ),
                ));
            }
        }
        let mut vals = vec![];
        for a in args.exprs() {
            vals.push(self.eval_spec(a)?);
        }
        let saved = self.env.clone();
        let saved_old = self.old.take();
        let saved_c = std::mem::replace(&mut self.c, lc);
        for ((n, ty), a) in lf.params.iter().zip(&vals) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
        }
        let r = (|| -> R<()> {
            for rq in &lf.requires {
                let v = self.eval_spec(rq.expr)?;
                self.check(
                    &v.t,
                    &format!("precondition of lemma `{}` not satisfied", lf.name),
                    sp.span,
                    Some(rq),
                );
            }
            for en in &lf.ensures {
                let v = self.eval_spec(en.expr)?;
                self.assume(&v.t);
            }
            Ok(())
        })();
        self.c = saved_c;
        self.env = saved;
        self.old = saved_old;
        r
    }
    /// Assumes the axioms of broadcast lemmas `lemmas`: for the whole VC with `global`, else on
    /// the current path from here on. A lemma that is not proved, or whose proof rests on the
    /// function being verified, is left out.
    fn broadcast(&mut self, lemmas: Vec<broadcast::Lemma<'p, 'a>>, global: bool) -> R<()> {
        for (lc, lf) in lemmas {
            let k = broadcast::key(lf);
            if self.broadcast_on.contains(&k) {
                continue;
            }
            if let Some(cur) = &self.cur_key
                && (*cur == k || broadcast::reaches(self.prog, (lc, lf), cur))
            {
                continue;
            }
            let ax = self.broadcast_axiom(lc, lf)?;
            if self.proved.get(&k) != Some(&true) {
                let w =
                    format!("broadcast lemma `{}` is not verified, so it is not assumed", lf.name);
                if !self.warnings.contains(&w) {
                    self.warnings.push(w);
                }
                continue;
            }
            if global {
                self.broadcast_on.insert(k);
                self.smt.send(&format!("(assert {ax})\n"));
            } else {
                self.assume(&ax);
            }
        }
        Ok(())
    }

    /// The axiom of broadcast lemma `lf`, with its specs read in `lc`: for all parameter values
    /// in range, its requires imply its ensures, instantiated at its triggers.
    fn broadcast_axiom(&mut self, lc: &'p Contract<'a>, lf: &'p Func<'a>) -> R<String> {
        let span = lf.item.header.span;
        let name = lf.name.clone();
        if !matches!(lf.item.header.state_mutability(), ast::StateMutability::Pure) {
            return Err((span, format!("broadcast lemma `{name}` must be pure")));
        }
        if lf.trusted || lf.item.body.is_none() {
            return Err((
                span,
                format!(
                    "broadcast lemma `{name}` needs a body that is verified: it cannot be trusted"
                ),
            ));
        }
        if !lf.rets.is_empty() {
            return Err((
                span,
                format!(
                    "broadcast lemma `{name}` returns values: its ensures may speak only of its parameters"
                ),
            ));
        }
        if lf.ensures.is_empty() {
            return Err((span, format!("broadcast lemma `{name}` has no ensures to broadcast")));
        }
        let saved_env = std::mem::take(&mut self.env);
        let saved_old = self.old.take();
        let saved_c = std::mem::replace(&mut self.c, lc);
        let nbound = self.bound.len();
        let mut decls = vec![];
        let mut syms = vec![];
        let r = (|| -> R<_> {
            let mut pre = vec![];
            for (n, ty) in &lf.params {
                let sort = ty.sort();
                if sort != "Int" && sort != "Bool" {
                    return Err((
                        span,
                        format!(
                            "parameter `{n}` of broadcast lemma `{name}` must have a value type"
                        ),
                    ));
                }
                self.n += 1;
                let sym = format!("|{n}!{}|", self.n);
                decls.push(format!("({sym} {sort})"));
                pre.extend(ty.range(&sym));
                self.env.insert(n.clone(), Var { ty: ty.clone(), t: sym.clone(), len: None });
                self.bound.push(n.clone());
                syms.push((n.clone(), sym));
            }
            for rq in &lf.requires {
                pre.push(self.eval_spec(rq.expr)?.t);
            }
            let mut post = vec![];
            for en in &lf.ensures {
                post.push(self.eval_spec(en.expr)?.t);
            }
            let mut pats = vec![];
            for group in &lf.triggers {
                let mut ts = vec![];
                for t in group {
                    ts.push((t, self.eval_spec(t.expr)?.t));
                }
                pats.push(ts);
            }
            Ok((pre, post, pats))
        })();
        self.env = saved_env;
        self.old = saved_old;
        self.c = saved_c;
        self.bound.truncate(nbound);
        let (pre, post, pats) = r?;
        let conj = |v: Vec<String>| match v.len() {
            0 => "true".to_string(),
            1 => v.into_iter().next().unwrap(),
            _ => format!("(and {})", v.join(" ")),
        };
        let post = conj(post);
        let body = if pre.is_empty() { post.clone() } else { format!("(=> {} {post})", conj(pre)) };
        if syms.is_empty() {
            return Ok(body);
        }
        let pats = broadcast::patterns(&name, &syms, &post, &pats, span)?;
        let pats: Vec<String> = pats.iter().map(|p| format!(":pattern {p}")).collect();
        Ok(format!("(forall ({}) (! {body} {}))", decls.join(" "), pats.join(" ")))
    }

    fn resolve_lemma(
        &self,
        callee: &'a Expr<'a>,
        n: usize,
        span: Span,
    ) -> R<(&'p Contract<'a>, &'p Func<'a>)> {
        let prog = self.prog;
        let (cname, fname) = match &callee.kind {
            ExprKind::Ident(i) => (self.c.name.clone(), i.as_str().to_string()),
            ExprKind::Member(b, m) => match &b.kind {
                ExprKind::Ident(l) => (l.as_str().to_string(), m.as_str().to_string()),
                _ => return Err((span, "unsupported lemma reference".into())),
            },
            _ => return Err((span, "unsupported lemma reference".into())),
        };
        let c = prog.contract(&cname).ok_or((span, format!("unknown contract `{cname}`")))?;
        let f = c
            .funcs
            .iter()
            .find(|f| f.name == fname && f.params.len() == n)
            .ok_or((span, format!("unknown lemma `{cname}.{fname}` with {n} arguments")))?;
        Ok((c, f))
    }
    /// Whether lemma `f` reaches function `cc.cf` through calls or uses in its body.
    fn lemma_reaches(
        &self,
        c: &'p Contract<'a>,
        f: &'p Func<'a>,
        cc: &str,
        cf: &str,
        seen: &mut FxHashSet<String>,
    ) -> bool {
        if !seen.insert(format!("{}.{}", c.name, f.name)) {
            return false;
        }
        let Some(b) = &f.item.body else { return false };
        let mut targets: Vec<(&'a Expr<'a>, usize)> = vec![];
        for st in b.stmts.iter() {
            crate::walk::all_stmts(st, &mut |s| {
                crate::walk::stmt_exprs(s, &mut |e| {
                    crate::walk::sub_exprs(e, &mut |x| {
                        if let ExprKind::Call(cal, a) = &x.kind {
                            targets.push((&**cal, a.exprs().count()));
                        }
                    })
                });
                if let Some(specs) = self.prog.stmt_specs.get(&s.span) {
                    for (tag, sp) in specs {
                        if tag == "use"
                            && let ExprKind::Call(cal, a) = &sp.expr.peel_parens().kind
                        {
                            targets.push((&**cal, a.exprs().count()));
                        }
                    }
                }
            });
        }
        for (cal, n) in targets {
            let r = match &cal.kind {
                ExprKind::Ident(i) => self.prog.contract(&c.name).and_then(|c2| {
                    c2.funcs
                        .iter()
                        .find(|g| g.name == i.as_str() && g.params.len() == n)
                        .map(|g| (c2, g))
                }),
                ExprKind::Member(b, m) => match &b.kind {
                    ExprKind::Ident(l) => self.prog.contract(l.as_str()).and_then(|c2| {
                        c2.funcs
                            .iter()
                            .find(|g| g.name == m.as_str() && g.params.len() == n)
                            .map(|g| (c2, g))
                    }),
                    _ => None,
                },
                _ => None,
            };
            if let Some((c2, g)) = r {
                if (c2.name == cc && g.name == cf) || (c2.name == c.name && g.name == f.name) {
                    return true;
                }
                if self.lemma_reaches(c2, g, cc, cf, seen) {
                    return true;
                }
            }
        }
        false
    }

    /// `@custom:reveal f(args)`: the opaque spec function's body, as a fact about this application.
    fn reveal(&mut self, sp: &Spec<'a>) -> R<()> {
        let ExprKind::Call(callee, _) = &sp.expr.peel_parens().kind else {
            return Err((sp.span, "@custom:reveal takes a call of an opaque spec function".into()));
        };
        let name = match &callee.kind {
            ExprKind::Ident(i) => i.as_str().to_string(),
            ExprKind::Member(_, m) => m.as_str().to_string(),
            _ => return Err((sp.span, "unsupported reveal".into())),
        };
        let a = self.eval_spec(sp.expr)?;
        self.reveal = Some(name);
        let b = self.eval_spec(sp.expr);
        self.reveal = None;
        let b = b?;
        self.assume(&format!("(= {} {})", a.t, b.t));
        Ok(())
    }

    /// Prove, then keep as a fact (Verus `assert`).
    fn prove(&mut self, phi: &str, kind: &str, span: Span, spec: Option<&Spec<'a>>) {
        self.check(phi, kind, span, spec);
        self.assume(phi);
    }
    /// A Solidity panic condition: reverts at runtime (assume), or an error in --strict mode.
    /// Record that the current path reverts when `cond` holds.
    pub(crate) fn reverts_if(&mut self, cond: &str) {
        if self.spec || self.guard == "false" || cond == "false" {
            return;
        }
        self.revert_sites.push(if cond == "true" {
            self.guard.clone()
        } else {
            format!("(and {} {cond})", self.guard)
        });
    }

    fn panic_unless(&mut self, phi: &str, what: &str, span: Span) {
        if self.spec {
            return;
        }
        self.reverts_if(&format!("(not {phi})"));
        if self.strict { self.prove(phi, what, span, None) } else { self.assume(phi) }
    }

    fn merge(&mut self, states: Vec<(String, Env)>) -> (String, Env) {
        let live: Vec<(String, Env)> = states.into_iter().filter(|(g, _)| g != "false").collect();
        if live.is_empty() {
            return ("false".into(), self.env.clone());
        }
        if live.len() == 1 {
            return live.into_iter().next().unwrap();
        }
        let gs: Vec<String> = live.iter().map(|(g, _)| g.clone()).collect();
        let guard = self.def("g", "Bool", &format!("(or {})", gs.join(" ")));
        let mut env = Env::new();
        let keys: Vec<String> = live[0]
            .1
            .keys()
            .filter(|k| live.iter().all(|(_, e)| e.contains_key(*k)))
            .cloned()
            .collect();
        for k in keys {
            let first = live[0].1[&k].clone();
            let pick = |f: &dyn Fn(&Var) -> Option<String>| -> Option<String> {
                let ts: Vec<Option<String>> = live.iter().map(|(_, e)| f(&e[&k])).collect();
                if ts.iter().all(|t| *t == ts[0]) {
                    return ts[0].clone();
                }
                let mut acc = ts.last().unwrap().clone().unwrap_or("0".into());
                for i in (0..ts.len() - 1).rev() {
                    acc = format!("(ite {} {} {acc})", gs[i], ts[i].clone().unwrap_or("0".into()));
                }
                Some(acc)
            };
            let t = pick(&|v| Some(v.t.clone())).unwrap();
            let len = pick(&|v| v.len.clone());
            let sort = first.ty.sort();
            // a binding every state agrees on keeps its term (memory bytes in assembly are
            // recognised by their canonical terms)
            let t = if t == first.t { t } else { self.def(&k, &sort, &t) };
            let len = len.map(|l| self.def(&format!("{k}.length"), "Int", &l));
            env.insert(k, Var { ty: first.ty, t, len });
        }
        (guard, env)
    }

    // ---------------------------------------------------------------- function driver
    pub(crate) fn verify_fn(&mut self, f: &'p Func<'a>) -> FnReport {
        self.horn_hints.clear();
        let at = self.begin_fn(f);
        let mut r = self.verify_fn_inner(f);
        // Spacer's proposals become candidates of one more run, checked like every other.
        if self.infer
            && r.is_ok()
            && !self.fails.is_empty()
            && !self.loop_recs.is_empty()
            && self.smt.timeout_ms() > 0
        {
            let hints = self.horn_fallback(self.smt.transcript_from(at));
            if !hints.is_empty() {
                self.smt.send("(pop 1)\n");
                self.horn_hints = hints;
                self.begin_fn(f);
                r = self.verify_fn_inner(f);
                self.horn_hints.clear();
            }
        }
        self.smt.send("(pop 1)\n");
        FnReport {
            name: format!("{}.{}", self.c.name, f.name),
            checks: self.checks,
            fails: std::mem::take(&mut self.fails),
            unsupported: r.err(),
            warnings: std::mem::take(&mut self.warnings),
            inferred: std::mem::take(&mut self.inferred),
        }
    }

    /// Open the solver scope of `f` and reset the per-function state; returns where the
    /// scope's transcript starts.
    fn begin_fn(&mut self, f: &'p Func<'a>) -> usize {
        self.smt.send(&format!("(push 1)\n; ===== {}.{}\n", self.c.name, f.name));
        let at = self.smt.transcript_len();
        self.clear_scope();
        self.smt.clear_error();
        self.fails.clear();
        self.warnings.clear();
        self.zero_checked.clear();
        self.cur_fn = Some((f.contract.clone(), f.name.clone()));
        self.cur_key = Some(broadcast::key(f));
        self.broadcast_on.clear();
        self.checks = 0;
        self.cur_func = Some(f);
        self.inferred.clear();
        self.loop_recs.clear();
        self.failed_goals.clear();
        at
    }

    fn init_env(&mut self, ctor: bool) -> R<()> {
        self.inv_entry.clear();
        self.revert_sites.clear();
        self.env.clear();
        self.guard = "true".into();
        self.old = None;
        self.frames.clear();
        self.loops.clear();
        self.ph.clear();
        self.cex_terms.clear();
        self.splits.clear();
        self.aliases.clear();
        self.slocals.clear();
        self.alias_env.clear();
        self.ret_alias = None;
        self.cur.clear();
        let this = self.decl("this", &Ty::Addr);
        self.env.insert(
            "this".into(),
            Var { ty: Ty::Contract(self.c.name.clone()), t: this, len: None },
        );
        for (n, ty) in [
            ("msg.sender", Ty::Addr),
            ("msg.value", Ty::Uint(256)),
            ("block.timestamp", Ty::Uint(256)),
            ("block.number", Ty::Uint(256)),
            ("block.chainid", Ty::Uint(256)),
            ("tx.origin", Ty::Addr),
            ("tx.gasprice", Ty::Uint(256)),
            ("msg.sig", Ty::Uint(32)),
            ("msg.data", Ty::Opaque),
            ("block.basefee", Ty::Uint(256)),
            ("block.blobbasefee", Ty::Uint(256)),
            ("block.coinbase", Ty::Addr),
            ("block.prevrandao", Ty::Uint(256)),
            ("block.difficulty", Ty::Uint(256)),
            ("block.gaslimit", Ty::Uint(256)),
        ] {
            let v = self.fresh_var(n, &ty);
            if n == "msg.sender" {
                self.cex_terms.push((n.into(), v.t.clone()));
                // nobody holds the zero address's key, so no call comes from it
                self.smt.send(&format!(
                    "(assert (not (= {} 0)))
",
                    v.t
                ));
            }
            self.env.insert(n.into(), v);
        }
        let c = self.c;
        for sv in c.state.iter().filter(|s| !s.constant) {
            // transient storage is not known to be clear when a constructor runs (another
            // create in the same transaction may have used the address): keep it arbitrary
            let v = if ctor
                && !sv.immutable
                && sv.name != "__balance"
                && sv.name != "__code_epoch"
                && !sv.transient
            {
                self.zero_var(&sv.ty)
            } else {
                self.fresh_var(&sv.name, &sv.ty)
            };
            if (!ctor && sv.ty.bounds().is_some() || matches!(sv.ty, Ty::Bool)) && !sv.ghost {
                self.cex_terms.push((sv.name.clone(), v.t.clone()));
            }
            self.env.insert(sv.name.clone(), v);
        }
        Ok(())
    }

    fn verify_fn_inner(&mut self, f: &'p Func<'a>) -> R<()> {
        if let Some((sp, t)) = f.orphans.first() {
            return Err((
                *sp,
                format!(
                    "spec comment `{t}` is attached to no statement: only @custom:assert and @custom:assume may end a block; put a statement after it"
                ),
            ));
        }
        if f.broadcast {
            // Its axiom must be well formed (a pure lemma with a valid trigger) to be proved.
            self.broadcast_axiom(self.c, f)?;
        }
        let lemmas = broadcast::active(self.prog, self.c, f)?;
        self.broadcast(lemmas, true)?;
        let ctor = f.item.kind.is_constructor();
        self.init_env(ctor)?;
        self.rets = f.rets.iter().map(|(n, _)| n.clone()).collect();
        for (n, ty) in &f.params {
            let v = self.fresh_var(n, ty);
            if ty.bounds().is_some() || matches!(ty, Ty::Bool) {
                self.cex_terms.push((n.clone(), v.t.clone()));
            }
            self.env.insert(n.clone(), v);
        }
        for (n, ty) in &f.rets {
            let z = self.zero_var(ty);
            self.env.insert(n.clone(), z);
        }
        for (p, (n, _)) in f.item.header.returns().iter().zip(&f.rets) {
            if p.data_location == Some(ast::DataLocation::Storage) {
                self.slocals.insert(n.clone());
            }
        }
        let c = self.c;
        if ctor {
            for sv in c.state.iter().filter(|s| !s.constant) {
                if let Some(init) = sv.init {
                    let v = self.eval(init)?;
                    self.env.insert(sv.name.clone(), Var { ty: sv.ty.clone(), t: v.t, len: v.len });
                }
            }
            // Solidity order: every base constructor's arguments are evaluated first, from the
            // most derived constructor down (a base constructor's modifier list can pass arguments
            // to its own bases, evaluated with its parameters bound), then bodies run base-first.
            let mut argvals: FxHashMap<String, Vec<Val>> = FxHashMap::default();
            {
                let mut sources: ModifierSources<'a> = vec![(&f.item.header.modifiers[..], vec![])];
                for (bn, a) in &c.base_args {
                    if !argvals.contains_key(bn) {
                        let vals = a.exprs().map(|x| self.eval(x)).collect::<R<Vec<_>>>()?;
                        argvals.insert(bn.clone(), vals);
                    }
                }
                for (bn, bf) in c.base_ctors.iter().rev() {
                    let mut found = argvals.contains_key(bn);
                    if !found {
                        for (mods, scope) in sources.iter() {
                            if let Some(m) = mods.iter().find(|m| m.name.last().as_str() == bn) {
                                let mut saved = vec![];
                                for (n, ty, v) in scope {
                                    saved.push((n.clone(), self.env.remove(n)));
                                    self.bind(n, ty.clone(), v.clone());
                                }
                                let vals = m
                                    .arguments
                                    .exprs()
                                    .map(|x| self.eval(x))
                                    .collect::<R<Vec<_>>>();
                                for (n, old) in saved {
                                    match old {
                                        Some(v) => self.env.insert(n, v),
                                        None => self.env.remove(&n),
                                    };
                                }
                                argvals.insert(bn.clone(), vals?);
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found && bf.header.parameters.is_empty() {
                        argvals.insert(bn.clone(), vec![]);
                    }
                    if let Some(vals) = argvals.get(bn) {
                        let scope = bf
                            .header
                            .parameters
                            .iter()
                            .zip(vals.iter())
                            .map(|(p, v)| {
                                (
                                    p.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
                                    self.ty_of_decl(&p.ty),
                                    v.clone(),
                                )
                            })
                            .collect();
                        sources.push((&bf.header.modifiers[..], scope));
                    }
                }
            }
            for (bn, bf) in &c.base_ctors {
                let vals = match argvals.get(bn) {
                    Some(v) if v.len() == bf.header.parameters.len() => v.clone(),
                    // an abstract contract may leave base arguments to its subclasses: any value
                    None if c.kind == ast::ContractKind::AbstractContract => {
                        let ps: Vec<(String, Ty)> = bf
                            .header
                            .parameters
                            .iter()
                            .map(|p| {
                                (
                                    p.name.map(|n| n.as_str().to_string()).unwrap_or_default(),
                                    self.ty_of_decl(&p.ty),
                                )
                            })
                            .collect();
                        ps.iter().map(|(n, t)| self.fresh_var(n, t)).collect()
                    }
                    _ => {
                        return Err((
                            bf.header.span,
                            format!("arguments for base constructor {bn} not found"),
                        ));
                    }
                };
                let mut bound = vec![];
                for (p, v) in bf.header.parameters.iter().zip(vals) {
                    let n = p.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                    let ty = self.ty_of_decl(&p.ty);
                    bound.push((n.clone(), self.env.remove(&n)));
                    self.bind(&n, ty, v);
                }
                if let Some(b) = &bf.body {
                    self.frames.push(Frame { rets: vec![], exits: vec![] });
                    self.exec_block(b)?;
                    let fr = self.frames.pop().unwrap();
                    let mut st = fr.exits;
                    st.push((self.guard.clone(), self.env.clone()));
                    let (g, e) = self.merge(st);
                    self.guard = g;
                    self.env = e;
                }
                for (n, old) in bound {
                    match old {
                        Some(v) => self.env.insert(n, v),
                        None => self.env.remove(&n),
                    };
                }
            }
        } else if f.is_entry() || !f.has_contract() {
            // An internal function without its own contract is inlined at every call site, which
            // is where it is really checked; its standalone run starts from a state satisfying the
            // invariants, so it does not report invariant failures no caller can produce.
            for inv in &c.invariants {
                let v = self.eval_spec(inv.expr)?;
                self.assume(&v.t);
                self.inv_entry.insert(v.t);
            }
        }
        for r in &f.requires {
            let v = self.eval_spec(r.expr)?;
            self.assume(&v.t);
        }
        // Entry effects of the transaction, after the invariants and requires were assumed of the
        // state it starts from: block time moves forward, and the EVM credits msg.value before
        // running the callee (non-payable entry points revert on value).
        if (f.is_entry() || ctor)
            && let (Some(t), Some(now)) =
                (self.env.get("__time").cloned(), self.env.get("block.timestamp").cloned())
        {
            if !ctor {
                self.assume(&format!("(>= {} {})", now.t, t.t));
            }
            self.bind("__time", t.ty.clone(), Self::val(t.ty, now.t));
        }
        if f.is_entry()
            && let (Some(bal), Some(this), Some(val)) = (
                self.env.get("__balance").cloned(),
                self.env.get("this").cloned(),
                self.env.get("msg.value").cloned(),
            )
        {
            if f.item.header.state_mutability() == ast::StateMutability::Payable {
                let nb = format!(
                    "(store {b} {t} (+ (select {b} {t}) {v}))",
                    b = bal.t,
                    t = this.t,
                    v = val.t
                );
                self.bind("__balance", bal.ty.clone(), Self::val(bal.ty, nb));
            } else {
                self.assume(&format!("(= {} 0)", val.t));
            }
        }
        self.old = Some(self.env.clone());
        // liveness: the pre-state condition of each succeeds_if, and the entry guard
        let mut live_pre = vec![];
        for sp in &f.succeeds {
            let v = self.eval_spec(sp.expr)?;
            live_pre.push((sp, v.t));
        }
        let entry_guard = self.guard.clone();
        if f.trusted || (f.item.body.is_none() && !f.synthetic) {
            return Ok(());
        }
        self.frames.push(Frame { rets: f.rets.clone(), exits: vec![] });
        self.exec_fn_body(f, 0)?;
        let fr = self.frames.pop().unwrap();
        let mut states = fr.exits;
        states.push((self.guard.clone(), self.env.clone()));
        let (g, e) = self.merge(states);
        self.guard = g;
        self.env = e;
        for (n, _) in &f.rets {
            self.check_raw_enum(n, f.item.header.span)?;
        }
        if self.guard == "false" || {
            self.smt.role = Some(Role::Advisory);
            matches!(self.smt.check(&self.guard.clone(), "false", &[]), Res::Unsat)
        } {
            self.warnings.push("no execution of this function can return normally (contradictory requires/invariants, or it always reverts); its postconditions hold vacuously".into());
        }
        // succeeds_if P: every pre-state satisfying the entry facts and P reaches a normal return,
        // i.e. (entry guard and P) implies the merged guard of the normal exits.
        let _ = &entry_guard;
        let sites = if self.revert_sites.is_empty() {
            "false".to_string()
        } else {
            format!("(or {})", self.revert_sites.join(" "))
        };
        for (sp, p) in live_pre {
            let exit = std::mem::replace(&mut self.guard, p);
            self.check(
                &format!("(not {sites})"),
                "function may revert where @custom:succeeds_if says it returns",
                sp.span,
                Some(sp),
            );
            self.guard = exit;
        }
        // A lemma's ensures are assumed at each `@custom:use`, which a lemma that reverts would
        // make vacuous: what lemmas run must return normally wherever their requires hold.
        if f.ghost_fn {
            let exit = std::mem::replace(&mut self.guard, "true".into());
            self.check(
                &format!("(not {sites})"),
                "ghost function may revert: what `@custom:use` runs must return wherever its requires hold",
                f.item.header.span,
                None,
            );
            self.guard = exit;
        }
        for en in &f.ensures {
            let v = self.eval_spec(en.expr)?;
            self.check(&v.t, "postcondition not satisfied", en.span, Some(en));
        }
        if f.is_entry() {
            for inv in &c.invariants {
                let v = self.eval_spec(inv.expr)?;
                let kind = if ctor {
                    "contract invariant not established by constructor"
                } else {
                    "contract invariant not preserved"
                };
                self.check_inv(&v.t, kind, f.item.header.span, inv);
            }
            // Transient storage is cleared when the transaction ends, and the next one starts
            // from that state: invariants over it must also hold with it cleared.
            let trans: Vec<(String, Ty)> = c
                .state
                .iter()
                .filter(|s| s.transient)
                .map(|s| (s.name.clone(), s.ty.clone()))
                .collect();
            for inv in &c.invariants {
                if !trans.iter().any(|(n, _)| inv.text.contains(n.as_str())) {
                    continue;
                }
                let saved = self.env.clone();
                for (n, ty) in &trans {
                    let z = self.zero_var(ty);
                    self.env.insert(n.clone(), z);
                }
                let v = self.eval_spec(inv.expr);
                self.env = saved;
                let v = v?;
                self.check(
                    &v.t,
                    "contract invariant must also hold once transient storage is cleared at the end of the transaction",
                    f.item.header.span,
                    Some(inv),
                );
            }
        }
        Ok(())
    }

    fn exec_fn_body(&mut self, f: &'p Func<'a>, mi: usize) -> R<()> {
        if mi == 0 {
            self.mem_alias_check(f)?;
        }
        let mods: Vec<&'a ast::Modifier<'a>> = f
            .item
            .header
            .modifiers
            .iter()
            .filter(|m| self.c.modifiers.contains_key(m.name.last().as_str()))
            .collect();
        if mi < mods.len() {
            let m = mods[mi];
            let mdef = self.c.modifiers[m.name.last().as_str()];
            let args: Vec<&'a Expr<'a>> = m.arguments.exprs().collect();
            // Arguments are evaluated in the function's scope; the modifier body then runs in a
            // scope of its own (its parameters and locals never shadow the function's).
            let mut vals = vec![];
            for a in args {
                vals.push(self.eval(a)?);
            }
            let fn_locals = self.split_locals();
            for (p, v) in mdef.header.parameters.iter().zip(vals) {
                self.env.insert(p.name.map(|n| n.as_str().to_string()).unwrap_or_default(), v);
            }
            self.ph.push((f, mi + 1, fn_locals));
            if let Some(b) = &mdef.body {
                self.cur.push(String::new());
                let r = self.exec_block(b);
                self.cur.pop();
                r?;
            }
            let (_, _, fn_locals) = self.ph.pop().unwrap();
            self.split_locals();
            self.env.extend(fn_locals);
            return Ok(());
        }
        if let Some(b) = &f.item.body {
            for ck in &f.checks {
                self.runtime_check(ck)?;
            }
            self.cur.push(f.contract.clone());
            let r = self.exec_block(b);
            self.cur.pop();
            r?;
        }
        Ok(())
    }

    /// `super.name(args)` from a function defined in the contract on top of `cur`: the nearest
    /// definition before it in the linearization.
    fn super_target(&self, name: &str, args: &[Val]) -> Option<&'p Func<'a>> {
        let c = self.c;
        let from = self.cur.last()?;
        let pos = |n: &str| c.lin.iter().position(|x| x == n);
        let here = pos(from)?;
        let best = c
            .funcs
            .iter()
            .chain(c.overridden.iter())
            .filter(|f| f.name == name && f.params.len() == args.len())
            .filter_map(|f| pos(&f.contract).filter(|p| *p < here))
            .max()?;
        let cands: Vec<&'p Func<'a>> = c
            .funcs
            .iter()
            .chain(c.overridden.iter())
            .filter(|f| {
                f.name == name && f.params.len() == args.len() && pos(&f.contract) == Some(best)
            })
            .collect();
        pick_in(cands, args)
    }

    // ---------------------------------------------------------------- statements
    fn exec_block(&mut self, b: &'a ast::Block<'a>) -> R<()> {
        for s in b.stmts.iter() {
            if self.guard == "false" {
                break;
            }
            self.exec_stmt(s)?;
        }
        if self.guard != "false"
            && let Some(specs) = self.prog.block_tail.get(&b.span)
        {
            for (tag, sp) in specs {
                let v = self.eval_spec(sp.expr)?;
                if tag == "assert" {
                    self.prove(&v.t, "assertion failed", sp.span, Some(sp));
                } else {
                    self.assume(&v.t);
                }
            }
        }
        Ok(())
    }

    fn stmt_specs(&self, s: &Stmt<'a>, tag: &str) -> Vec<&'p Spec<'a>> {
        self.prog
            .stmt_specs
            .get(&s.span)
            .map(|v| v.iter().filter(|(t, _)| t == tag).map(|(_, s)| s).collect())
            .unwrap_or_default()
    }

    fn exec_stmt(&mut self, s: &'a Stmt<'a>) -> R<()> {
        let prog = self.prog;
        if let Some(specs) = prog.stmt_specs.get(&s.span) {
            for (tag, sp) in specs {
                match tag.as_str() {
                    "assert" => {
                        let v = self.eval_spec(sp.expr)?;
                        self.prove(&v.t, "assertion failed", sp.span, Some(sp));
                    }
                    "assume" => {
                        let v = self.eval_spec(sp.expr)?;
                        self.assume(&v.t);
                    }
                    "ghost" => {
                        self.eval_spec(sp.expr)?;
                    }
                    "check" => self.runtime_check(sp)?,
                    "use" => self.use_lemma(sp)?,
                    "reveal" => self.reveal(sp)?,
                    "broadcast-use" => {
                        let mut lemmas = vec![];
                        broadcast::resolve(self.prog, self.c, &sp.text, sp.span, &mut lemmas)?;
                        self.broadcast(lemmas, false)?;
                    }
                    _ => {}
                }
            }
        }
        match &s.kind {
            StmtKind::DeclSingle(v)
                if v.data_location == Some(ast::DataLocation::Storage)
                    && v.initializer.as_ref().is_some_and(|i| {
                        matches!(
                            i.peel_parens().kind,
                            ExprKind::Ident(_) | ExprKind::Index(..) | ExprKind::Member(..)
                        )
                    }) =>
            {
                let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                let init: &'a Expr<'a> = v.initializer.as_deref().unwrap();
                // a storage pointer: reads and writes go to the aliased location
                // (index expressions are re-evaluated at each use; see README limits)
                let mut cap = None;
                let init = match &init.peel_parens().kind {
                    ExprKind::Ident(i) => {
                        cap = self.alias_env.get(i.as_str()).cloned();
                        self.aliases.get(i.as_str()).copied().unwrap_or(init)
                    }
                    _ => init,
                };
                self.env.remove(&name);
                self.slocals.insert(name.clone());
                match cap {
                    Some(c) => self.alias_env.insert(name.clone(), c),
                    None => self.alias_env.remove(&name),
                };
                self.aliases.insert(name, init);
            }
            StmtKind::DeclSingle(v)
                if v.data_location == Some(ast::DataLocation::Storage)
                    && v.initializer
                        .as_ref()
                        .is_some_and(|i| matches!(i.peel_parens().kind, ExprKind::Call(..))) =>
            {
                // a function returning a storage pointer: follow the location it returns,
                // with the callee's index values captured at the call
                let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                self.ret_alias = None;
                self.eval_multi(v.initializer.as_deref().unwrap())?;
                let Some((t, env)) = self.ret_alias.take() else {
                    return Err((
                        s.span,
                        format!(
                            "storage pointer `{name}` from a call whose result we cannot follow"
                        ),
                    ));
                };
                self.env.remove(&name);
                self.slocals.insert(name.clone());
                self.alias_env.insert(name.clone(), env);
                self.aliases.insert(name, t);
            }
            StmtKind::DeclSingle(v) if v.data_location == Some(ast::DataLocation::Storage) => {
                let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                if v.initializer.is_some() {
                    return Err((
                        s.span,
                        format!(
                            "storage pointer `{name}` from a function call or conditional is not supported (writes through it would not be tracked)"
                        ),
                    ));
                }
                self.env.remove(&name);
                self.slocals.insert(name);
            }
            StmtKind::DeclMulti(vars, _)
                if vars.iter().any(|v| {
                    v.as_ref()
                        .unspan()
                        .is_some_and(|v| v.data_location == Some(ast::DataLocation::Storage))
                }) =>
            {
                return Err((s.span, "storage pointers from a tuple are not supported".into()));
            }
            StmtKind::DeclSingle(v) => {
                let ty = self.ty_of_decl(&v.ty);
                let val = match &v.initializer {
                    Some(i) => self.eval(i)?,
                    None => self.zero_var(&ty),
                };
                let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                self.bind(&name, ty, val);
            }
            StmtKind::DeclMulti(vars, e) => {
                let vals = self.eval_multi(e)?;
                for (v, val) in vars.iter().zip(vals) {
                    if let Some(v) = v.as_ref().unspan() {
                        let ty = self.ty_of_decl(&v.ty);
                        let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                        self.bind(&name, ty, val);
                    }
                }
            }
            StmtKind::Expr(e) => {
                self.eval_multi(e)?;
            }
            StmtKind::Block(b) => self.exec_block(b)?,
            StmtKind::UncheckedBlock(b) => {
                let u = self.unchecked;
                self.unchecked = true;
                self.exec_block(b)?;
                self.unchecked = u;
            }
            StmtKind::If(c, a, b) => {
                let cv = self.eval(c)?;
                if !self.spec
                    && self.loops.is_empty()
                    && self.splits.len() < 32
                    && cv.t.len() < 2000
                    && cv.t != "true"
                    && cv.t != "false"
                    && !self.splits.contains(&cv.t)
                {
                    self.splits.push(cv.t.clone());
                }
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                self.assume(&cv.t);
                self.exec_stmt(a)?;
                let s1 = (
                    std::mem::replace(&mut self.guard, g0),
                    std::mem::replace(&mut self.env, env0),
                );
                self.assume(&format!("(not {})", cv.t));
                if let Some(b) = b {
                    self.exec_stmt(b)?;
                }
                let s2 = (self.guard.clone(), self.env.clone());
                let (g, e) = self.merge(vec![s1, s2]);
                self.guard = g;
                self.env = e;
            }
            StmtKind::While(c, body) => self.exec_loop(s, Some(c), None, body)?,
            StmtKind::For { init, cond, next, body } => {
                if let Some(i) = init {
                    self.exec_stmt(i)?;
                }
                self.exec_loop(s, cond.as_deref(), next.as_deref(), body)?;
            }
            StmtKind::Return(e) => {
                let rets = self.frames.last().map(|f| f.rets.clone()).unwrap_or_default();
                if let Some(e) = e {
                    let vals = self.eval_multi(e)?;
                    for ((n, ty), v) in rets.iter().zip(vals) {
                        self.env.insert(n.clone(), Var { ty: ty.clone(), t: v.t, len: v.len });
                    }
                }
                let st = (self.guard.clone(), self.env.clone());
                self.frames.last_mut().unwrap().exits.push(st);
                self.guard = "false".into();
            }
            StmtKind::Revert(..) => {
                self.reverts_if("true");
                self.guard = "false".into();
            }
            StmtKind::Emit(_, args) => {
                for a in args.exprs() {
                    self.eval(a)?;
                }
            }
            StmtKind::Break => {
                let st = (self.guard.clone(), self.env.clone());
                self.loops
                    .last_mut()
                    .ok_or((s.span, "break outside loop".to_string()))?
                    .breaks
                    .push(st);
                self.guard = "false".into();
            }
            StmtKind::Continue => {
                let st = (self.guard.clone(), self.env.clone());
                self.loops
                    .last_mut()
                    .ok_or((s.span, "continue outside loop".to_string()))?
                    .conts
                    .push(st);
                self.guard = "false".into();
            }
            StmtKind::Placeholder => {
                let (f, i, fn_locals) =
                    self.ph.pop().ok_or((s.span, "placeholder outside modifier".to_string()))?;
                let mod_locals = self.split_locals();
                self.env.extend(fn_locals);
                self.frames.push(Frame { rets: f.rets.clone(), exits: vec![] });
                self.exec_fn_body(f, i)?;
                let fr = self.frames.pop().unwrap();
                let mut st = fr.exits;
                st.push((self.guard.clone(), self.env.clone()));
                let (g, e) = self.merge(st);
                self.guard = g;
                self.env = e;
                let fn_locals = self.split_locals();
                self.env.extend(mod_locals);
                self.ph.push((f, i, fn_locals));
            }
            StmtKind::Assembly(a) => self.exec_asm(a, s.span)?,
            StmtKind::Try(t) => {
                // The callee either returns (the first clause runs with its results) or reverts,
                // undoing everything it did, and one of the catch clauses runs. Which one is
                // chosen nondeterministically; a failure no clause catches reverts us too.
                let sel = self.decl("try_path", &Ty::Uint(8));
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                let mut states = vec![];
                self.assume(&format!("(= {sel} 0)"));
                let saved_try = self.try_call.replace(t.expr.span);
                let vals = self.eval_multi(t.expr);
                self.try_call = saved_try;
                let vals = vals?;
                if let Some(c) = t.clauses.first() {
                    for (p, val) in c.args.iter().zip(vals) {
                        if let Some(n) = p.name {
                            let ty = self.ty_of_decl(&p.ty);
                            self.bind(n.as_str(), ty, val);
                        }
                    }
                    self.exec_block(&c.block)?;
                }
                states.push((
                    std::mem::replace(&mut self.guard, g0.clone()),
                    std::mem::replace(&mut self.env, env0.clone()),
                ));
                for (i, c) in t.clauses.iter().enumerate().skip(1) {
                    self.assume(&format!("(= {sel} {i})"));
                    for p in c.args.iter() {
                        if let Some(n) = p.name {
                            let ty = self.ty_of_decl(&p.ty);
                            let v = self.fresh_var(n.as_str(), &ty);
                            self.bind(n.as_str(), ty, v);
                        }
                    }
                    self.exec_block(&c.block)?;
                    states.push((
                        std::mem::replace(&mut self.guard, g0.clone()),
                        std::mem::replace(&mut self.env, env0.clone()),
                    ));
                }
                let (g, e) = self.merge(states);
                self.guard = g;
                self.env = e;
            }
            StmtKind::DoWhile(..) => {
                return Err((s.span, "do-while is not supported (use while)".into()));
            }
        }
        Ok(())
    }

    /// Remove and return the local (non-state, non-environment) bindings of `env`.
    fn split_locals(&mut self) -> Env {
        let c = self.c;
        let keys: Vec<String> = self
            .env
            .keys()
            .filter(|k| !(k.contains('.') || *k == "this" || c.state.iter().any(|s| &s.name == *k)))
            .cloned()
            .collect();
        keys.into_iter()
            .map(|k| {
                let v = self.env.remove(&k).unwrap();
                (k, v)
            })
            .collect()
    }

    fn bind(&mut self, name: &str, ty: Ty, val: Val) {
        let t = self.def(name, &ty.sort(), &val.t);
        let len = val.len.map(|l| self.def(&format!("{name}.length"), "Int", &l));
        let len = if let Ty::Arr(..) = ty {
            Some(match len {
                Some(l) => l,
                None => self.arr_len(&ty),
            })
        } else {
            len
        };
        self.env.insert(name.to_string(), Var { ty, t, len });
    }

    fn ty_of_decl(&self, t: &'a ast::Type<'a>) -> Ty {
        ty_of(self.prog, t)
    }

    fn loop_targets(&self, body: &'a Stmt<'a>, next: Option<&'a Expr<'a>>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut all_state = false;
        let visit = |e: &'a Expr<'a>, out: &mut BTreeSet<String>, all: &mut bool| {
            walk::sub_exprs(e, &mut |x| self.effects(x, out, all, &mut FxHashSet::default()));
        };
        if let Some(n) = next {
            visit(n, &mut out, &mut all_state);
        }
        walk::all_stmts(body, &mut |s| {
            walk::stmt_exprs(s, &mut |e| visit(e, &mut out, &mut all_state));
            if let StmtKind::Assembly(a) = &s.kind {
                yul::asm_effects(&a.block, &mut out, &mut all_state, self.lock_modset.get());
                if out.contains("__raw") {
                    out.extend(self.slot_taken.iter().cloned());
                }
            }
            if let Some(specs) = self.prog.stmt_specs.get(&s.span) {
                for (tag, sp) in specs {
                    if tag == "ghost" {
                        visit(sp.expr, &mut out, &mut all_state);
                    }
                }
            }
        });
        let mut al = Self::alias_roots(&[body], &[]);
        for (k, v) in &self.aliases {
            al.entry(k.clone()).or_insert_with(|| walk::root(v));
        }
        let mut out = Self::expand(out, &al);
        if out.remove("*") {
            all_state = true;
        }
        if all_state {
            for sv in self.c.state.iter().filter(|s| !s.constant && !s.immutable) {
                out.insert(sv.name.clone());
            }
        }
        out.into_iter().filter(|n| self.env.contains_key(n)).collect()
    }

    /// Syntactic write effects of one expression node (names may be locals, params or storage
    /// aliases; callers expand aliases and filter to state).
    fn effects(
        &self,
        e: &'a Expr<'a>,
        out: &mut BTreeSet<String>,
        all: &mut bool,
        seen: &mut FxHashSet<String>,
    ) {
        match &e.kind {
            ExprKind::Assign(l, ..) => out.extend(walk::root(l)),
            ExprKind::Unary(op, x) if op.kind.has_side_effects() => out.extend(walk::root(x)),
            ExprKind::Delete(x) => out.extend(walk::root(x)),
            ExprKind::Call(callee, args) => {
                let callee = match &callee.kind {
                    ExprKind::CallOptions(c, opts) => {
                        if opts.iter().any(|o| o.name.name == sym::value) {
                            out.insert("__balance".into());
                        }
                        c
                    }
                    _ => callee,
                };
                let argv: Vec<&'a Expr<'a>> = args.exprs().collect();
                match &callee.kind {
                    ExprKind::New(t) => {
                        // contract creation runs outside code
                        if matches!(t.kind, ast::TypeKind::Custom(_)) && !self.lock_modset.get() {
                            *all = true;
                        }
                    }
                    ExprKind::Ident(id) => {
                        let n = id.as_str();
                        let free = self.prog.contract(FREE).map(|l| &l.funcs[..]).unwrap_or(&[]);
                        let named: Vec<&'p Func<'a>> = self
                            .c
                            .funcs
                            .iter()
                            .chain(free)
                            .filter(|f| f.name == n && f.params.len() == argv.len())
                            .collect();
                        if named.is_empty() && !self.names_no_pointer(n) {
                            // a function pointer: any function it may hold
                            self.ptr_call_effects(&argv, out, all, seen);
                        }
                        for f in named {
                            self.call_effects(f, &argv, out, all, seen);
                        }
                    }
                    ExprKind::Member(base, m) => {
                        let m = m.as_str();
                        if m == "push" || m == "pop" {
                            out.extend(walk::root(base));
                            return;
                        }
                        if m == "transfer" || m == "send" {
                            out.insert("__balance".into());
                        }
                        if let ExprKind::Ident(b) = &base.kind {
                            let b = b.as_str();
                            if b == "abi" || self.prog.udvts.contains_key(b) {
                                return;
                            }
                            if b == "super" {
                                // any definition super may reach (over-approximation)
                                for f in self
                                    .c
                                    .funcs
                                    .iter()
                                    .chain(self.c.overridden.iter())
                                    .filter(|f| f.name == m && f.params.len() == argv.len())
                                {
                                    self.call_effects(f, &argv, out, all, seen);
                                }
                                return;
                            }
                            if let Some(lib) = self
                                .prog
                                .contract(b)
                                .filter(|c| c.kind == ast::ContractKind::Library)
                            {
                                for f in lib
                                    .funcs
                                    .iter()
                                    .filter(|f| f.name == m && f.params.len() == argv.len())
                                {
                                    self.call_effects(f, &argv, out, all, seen);
                                }
                                return;
                            }
                        }
                        if let Some((_, f)) = self.prog.using_fn(m, argv.len()) {
                            let mut a2: Vec<&'a Expr<'a>> = vec![&**base];
                            a2.extend(argv.iter().copied());
                            self.call_effects(f, &a2, out, all, seen);
                            return;
                        }
                        if !self.static_member(m) && !self.lock_modset.get() {
                            *all = true;
                        }
                    }
                    // a call through a pointer some expression computes
                    ExprKind::Call(..)
                    | ExprKind::Index(..)
                    | ExprKind::Ternary(..)
                    | ExprKind::Tuple(..) => self.ptr_call_effects(&argv, out, all, seen),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Whether calling `n(..)` is no call through a function pointer: a builtin, a type
    /// conversion or a struct, event or error constructor.
    fn names_no_pointer(&self, n: &str) -> bool {
        const BUILTINS: &[&str] = &[
            "require",
            "assert",
            "revert",
            "keccak256",
            "sha256",
            "ripemd160",
            "ecrecover",
            "addmod",
            "mulmod",
            "gasleft",
            "blockhash",
            "blobhash",
            "selfdestruct",
            "old",
            "__implies",
            "payable",
        ];
        BUILTINS.contains(&n)
            || n.starts_with("__q")
            || self.prog.struct_fields(n).is_some()
            || self.prog.contract(n).is_some()
            || self.prog.enums.contains_key(n)
            || self.prog.udvts.contains_key(n)
    }

    /// What a call through a function pointer may write: what any internal function of its
    /// arity may (a pointer holds only functions of the program, else the call fails).
    fn ptr_call_effects(
        &self,
        argv: &[&'a Expr<'a>],
        out: &mut BTreeSet<String>,
        all: &mut bool,
        seen: &mut FxHashSet<String>,
    ) {
        let libs = self.prog.contracts.iter().filter(|c| c.kind == ast::ContractKind::Library);
        let fs: Vec<&'p Func<'a>> = self
            .c
            .funcs
            .iter()
            .chain(libs.flat_map(|c| c.funcs.iter()))
            .filter(|f| f.params.len() == argv.len() && !f.item.kind.is_modifier())
            .collect();
        for f in fs {
            self.call_effects(f, argv, out, all, seen);
        }
    }

    fn call_effects(
        &self,
        f: &'p Func<'a>,
        args: &[&'a Expr<'a>],
        out: &mut BTreeSet<String>,
        all: &mut bool,
        seen: &mut FxHashSet<String>,
    ) {
        // A memory object passed by reference: what the callee writes through its parameter the
        // caller sees through its own name, so a loop that makes the call changes that name.
        for (i, p) in f.item.header.parameters.iter().enumerate() {
            if Self::mem_ref_param(p, &f.params[i].1)
                && self.mem_param_written(f, &f.params[i].0)
                && let Some(a) = args.get(i)
            {
                let roots = walk::root(a);
                let mut w = self.mem_call_writes.borrow_mut();
                for r in &roots {
                    w.push((f.name.clone(), r.clone()));
                }
                out.extend(roots);
            }
        }
        match self.modset_raw(f, seen) {
            None => *all = true,
            Some(raw) => {
                for (i, p) in f.item.header.parameters.iter().enumerate() {
                    if p.data_location == Some(ast::DataLocation::Storage)
                        && raw.contains(&f.params[i].0)
                        && let Some(a) = args.get(i)
                    {
                        out.extend(walk::root(a));
                    }
                }
                let state: FxHashSet<&str> = self.c.state.iter().map(|s| s.name.as_str()).collect();
                out.extend(raw.into_iter().filter(|n| state.contains(n.as_str())));
            }
        }
    }

    /// Storage-pointer locals declared in these statements: name -> roots it points into.
    /// `extra`: storage-located return variables. A pointer from something we cannot trace (a
    /// call's result) points at "*": anything.
    fn alias_roots(stmts: &[&'a Stmt<'a>], extra: &[String]) -> FxHashMap<String, Vec<String>> {
        fn roots<'a>(e: &'a Expr<'a>) -> Vec<String> {
            match &e.peel_parens().kind {
                ExprKind::Ternary(_, a, b) => {
                    let mut v = roots(a);
                    v.extend(roots(b));
                    v
                }
                _ => {
                    let r = walk::root(e);
                    if r.is_empty() { vec!["*".to_string()] } else { r }
                }
            }
        }
        let mut m: FxHashMap<String, Vec<String>> = FxHashMap::default();
        let mut names: FxHashSet<String> = extra.iter().cloned().collect();
        for s in stmts {
            walk::all_stmts(s, &mut |s| {
                if let StmtKind::DeclSingle(v) = &s.kind
                    && v.data_location == Some(ast::DataLocation::Storage)
                    && let Some(n) = v.name
                {
                    names.insert(n.as_str().to_string());
                    let r = v.initializer.as_ref().map(|i| roots(i)).unwrap_or_default();
                    m.entry(n.as_str().to_string()).or_default().extend(r);
                }
            });
        }
        for s in stmts {
            walk::all_stmts(s, &mut |s| {
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| {
                        if let ExprKind::Assign(l, None, r) = &x.kind
                            && let ExprKind::Ident(i) = &l.peel_parens().kind
                            && names.contains(i.as_str())
                        {
                            m.entry(i.as_str().to_string()).or_default().extend(roots(r));
                        }
                    })
                });
            });
        }
        m
    }

    fn expand(names: BTreeSet<String>, al: &FxHashMap<String, Vec<String>>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut work: Vec<String> = names.into_iter().collect();
        let mut seen = FxHashSet::default();
        while let Some(n) = work.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            match al.get(&n) {
                Some(rs) => work.extend(rs.iter().cloned()),
                None => {
                    out.insert(n);
                }
            }
        }
        out
    }

    fn static_member(&self, m: &str) -> bool {
        if [
            "transfer",
            "send",
            "encode",
            "encodePacked",
            "encodeWithSelector",
            "decode",
            "staticcall",
        ]
        .contains(&m)
        {
            return true;
        }
        let mut found = false;
        for c in &self.prog.contracts {
            for f in &c.funcs {
                if f.name == m {
                    if !f.is_static() {
                        return false;
                    }
                    found = true;
                }
            }
        }
        found
    }

    /// What a call to `f` may write when this contract's reentrancy lock is held at the call and
    /// `f` never touches the lock: its own writes, with its external calls reaching only the entry
    /// points that stay callable under the lock, and so whatever those may write. None when the
    /// lock isn't known to be held, `f` reaches the lock, or some write set is unknown.
    fn modset_under_lock(
        &mut self,
        f: &'p Func<'a>,
    ) -> Option<(BTreeSet<String>, Option<BTreeSet<String>>)> {
        if self.spec || self.c.lock.is_none() || !self.lock_held() {
            return None;
        }
        let names: FxHashSet<String> = match &self.c.lock_expr {
            Some(_) => self.lock_names(),
            None => self.c.lock.iter().map(|(g, _)| g.clone()).collect(),
        };
        if self.reaches_names(self.fn_stmts(f), &names, &mut FxHashSet::default()) {
            return None;
        }
        // whether f can call out at all: its plain write set is unknown only through such calls
        let calls_out = self.modset(f, &mut FxHashSet::default()).is_none();
        self.lock_modset.set(true);
        let own = self.modset(f, &mut FxHashSet::default());
        let mut out = own.clone();
        if calls_out && out.is_some() {
            let open: Vec<&'p Func<'a>> =
                self.c.funcs.iter().filter(|g| self.lock_open(g)).collect();
            let mut seen = FxHashSet::default();
            for g in open {
                match (self.modset(g, &mut seen), out.as_mut()) {
                    (Some(m), Some(o)) => o.extend(m),
                    _ => out = None,
                }
            }
        }
        self.lock_modset.set(false);
        let (own, out) = (own?, out?);
        // the lock's own state is never written under it: f doesn't reach it, and the open entry
        // points don't either
        let mut out: BTreeSet<String> = out.into_iter().filter(|n| !names.contains(n)).collect();
        // outside code changes these whether or not the lock is held: it can force ETH on us
        // (selfdestruct), and deploy or destroy code anywhere
        if calls_out {
            out.extend(["__balance".to_string(), "__code_epoch".to_string()]);
        }
        // invariants f's own writes can't touch hold afterwards when f calls out: f's proof
        // checks them before each of its calls, the open entry points keep them, and nothing after
        // the last call writes what they read
        // without a call out, the open entry points never run, so nothing new holds afterwards
        let keep = if calls_out { Some(own) } else { None };
        Some((out, keep))
    }

    /// The state variables a spec expression reads.
    fn spec_state_names(&self, e: &'a Expr<'a>) -> FxHashSet<String> {
        let mut out = FxHashSet::default();
        walk::sub_exprs(e, &mut |x| {
            if let ExprKind::Ident(i) = &x.kind
                && self.c.state.iter().any(|s| s.name == i.as_str())
            {
                out.insert(i.as_str().to_string());
            }
        });
        out
    }

    /// State variables a function may write; None = anything (external calls).
    fn modset(&self, f: &'p Func<'a>, seen: &mut FxHashSet<String>) -> Option<BTreeSet<String>> {
        let raw = self.modset_raw(f, seen)?;
        let state: FxHashSet<&str> = self.c.state.iter().map(|s| s.name.as_str()).collect();
        Some(raw.into_iter().filter(|n| state.contains(n.as_str())).collect())
    }

    /// Names a function may write (state variables and its own storage parameters), with storage
    /// pointers resolved; None = anything.
    fn modset_raw(
        &self,
        f: &'p Func<'a>,
        seen: &mut FxHashSet<String>,
    ) -> Option<BTreeSet<String>> {
        if f.is_static() {
            return Some(BTreeSet::new());
        }
        let key = format!("{}.{}/{}", f.contract, f.name, f.params.len());
        if !seen.insert(key) {
            return Some(BTreeSet::new());
        }
        if f.item.body.is_none() || f.trusted {
            return None;
        }
        let mut out = BTreeSet::new();
        let mut all = false;
        let mut stmts: Vec<&'a Stmt<'a>> = vec![];
        if let Some(b) = &f.item.body {
            stmts.extend(b.stmts.iter());
        }
        for m in f.item.header.modifiers.iter() {
            if let Some(md) = self.c.modifiers.get(m.name.last().as_str())
                && let Some(b) = &md.body
            {
                stmts.extend(b.stmts.iter());
            }
        }
        for s in &stmts {
            walk::all_stmts(s, &mut |s| {
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| self.effects(x, &mut out, &mut all, seen))
                });
                // `@custom:ghost x = e` updates write x
                for g in self.stmt_specs(s, "ghost") {
                    walk::sub_exprs(g.expr, &mut |x| self.effects(x, &mut out, &mut all, seen));
                }
                if let StmtKind::Assembly(a) = &s.kind {
                    yul::asm_effects(&a.block, &mut out, &mut all, self.lock_modset.get());
                    if out.contains("__raw") {
                        out.extend(self.slot_taken.iter().cloned());
                    }
                }
            });
        }
        if all {
            return None;
        }
        let sret: Vec<String> = f
            .item
            .header
            .returns()
            .iter()
            .zip(&f.rets)
            .filter(|(p, _)| p.data_location == Some(ast::DataLocation::Storage))
            .map(|(_, (n, _))| n.clone())
            .collect();
        let al = Self::alias_roots(&stmts, &sret);
        let out = Self::expand(out, &al);
        if out.contains("*") {
            return None;
        }
        Some(out)
    }

    // ---------------------------------------------------------------- lvalues
    fn assign(&mut self, l: &'a Expr<'a>, v: Val) -> R<()> {
        match &l.peel_parens().kind {
            ExprKind::Ident(id) if self.aliases.contains_key(id.as_str()) => {
                self.with_alias(id.as_str(), |s, t| s.assign(t, v))
            }
            ExprKind::Ident(id) => {
                let n = id.as_str().to_string();
                let ty = self
                    .env
                    .get(&n)
                    .map(|x| x.ty.clone())
                    .ok_or((l.span, format!("assignment to unknown `{n}`")))?;
                self.check_stored_struct(l, &v, l.span)?;
                self.bind(&n, ty, v);
                Ok(())
            }
            ExprKind::Index(base, IndexKind::Index(Some(idx))) => {
                let b = self.eval(base)?;
                let i = self.eval(idx)?;
                if let Ty::Arr(..) = &b.ty {
                    let len = match &b.len {
                        Some(l) => l.clone(),
                        None => self.arr_len(&b.ty),
                    };
                    self.panic_unless(
                        &format!("(< {} {len})", i.t),
                        "array index may be out of bounds",
                        idx.span,
                    );
                }
                self.check_stored_struct(l, &v, l.span)?;
                let nb = Var {
                    ty: b.ty.clone(),
                    t: format!("(store {} {} {})", b.t, i.t, v.t),
                    len: b.len.clone(),
                };
                self.assign(base, nb)
            }
            ExprKind::Member(base, m) if m.name == sym::length => {
                let mut b = self.eval(base)?;
                b.len = Some(v.t);
                self.assign(base, b)
            }
            ExprKind::Member(base, m) => {
                let b = self.eval(base)?;
                let Ty::Struct(sn) = &b.ty else {
                    return Err((l.span, "unsupported assignment target".into()));
                };
                let fields = self
                    .prog
                    .struct_fields(sn)
                    .ok_or((l.span, format!("unknown struct `{sn}`")))?
                    .clone();
                if !fields.iter().any(|(f, _)| f == m.as_str()) {
                    return Err((l.span, format!("struct `{sn}` has no field `{m}`")));
                }
                let args: Vec<String> = fields
                    .iter()
                    .map(|(f, _)| {
                        if f == m.as_str() { v.t.clone() } else { format!("(|{sn}.{f}| {})", b.t) }
                    })
                    .collect();
                let nb = Var {
                    ty: b.ty.clone(),
                    t: format!("(|mk.{sn}| {})", args.join(" ")),
                    len: None,
                };
                self.assign(base, nb)
            }
            ExprKind::Tuple(_) => {
                Err((l.span, "tuple assignment must use a tuple right-hand side".into()))
            }
            _ => Err((l.span, "unsupported assignment target".into())),
        }
    }

    // ---------------------------------------------------------------- expressions
    pub(crate) fn eval_spec(&mut self, e: &'a Expr<'a>) -> R<Val> {
        let s = self.spec;
        self.spec = true;
        let r = self.eval(e);
        self.spec = s;
        r
    }

    fn eval_multi(&mut self, e: &'a Expr<'a>) -> R<Vec<Val>> {
        match &e.peel_parens().kind {
            ExprKind::Tuple(xs) if xs.len() != 1 => {
                let mut out = vec![];
                for x in xs.iter() {
                    match x.as_ref().unspan() {
                        Some(x) => out.push(self.eval(x)?),
                        None => out.push(Var { ty: Ty::Opaque, t: "0".into(), len: None }),
                    }
                }
                Ok(out)
            }
            ExprKind::Call(callee, args) => self.call(e, callee, args),
            ExprKind::Ternary(c, a, b)
                if !self.spec
                    && [a, b].iter().any(|x| match &x.peel_parens().kind {
                        ExprKind::Tuple(xs) => xs.len() != 1,
                        ExprKind::Call(..) => true,
                        _ => false,
                    }) =>
            {
                // the branches run on forked states, merged as an if statement's are: a call in
                // one branch must not leave its effects on the paths through the other
                let cv = self.eval(c)?;
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                self.assume(&cv.t);
                let av = self.eval_multi(a)?;
                let s1 = (
                    std::mem::replace(&mut self.guard, g0),
                    std::mem::replace(&mut self.env, env0),
                );
                self.assume(&format!("(not {})", cv.t));
                let bv = self.eval_multi(b)?;
                let s2 = (self.guard.clone(), self.env.clone());
                let (g, en) = self.merge(vec![s1, s2]);
                self.guard = g;
                self.env = en;
                if av.len() != bv.len() {
                    return Err((e.span, "tuple sizes differ".into()));
                }
                Ok(av
                    .into_iter()
                    .zip(bv)
                    .map(|(x, y)| Var {
                        ty: if x.ty == Ty::Lit { y.ty.clone() } else { x.ty.clone() },
                        t: format!("(ite {} {} {})", cv.t, x.t, y.t),
                        len: match (x.len, y.len) {
                            (Some(l1), Some(l2)) => Some(format!("(ite {} {l1} {l2})", cv.t)),
                            _ => None,
                        },
                    })
                    .collect())
            }
            ExprKind::Assign(l, None, r) if matches!(l.peel_parens().kind, ExprKind::Tuple(_)) => {
                if let ExprKind::Tuple(ls) = &l.peel_parens().kind {
                    for x in ls.iter().filter_map(|x| x.as_ref().unspan()) {
                        if matches!(&x.peel_parens().kind, ExprKind::Ident(i) if self.slocals.contains(i.as_str()))
                        {
                            return Err((
                                e.span,
                                "storage pointers set from a tuple are not supported".into(),
                            ));
                        }
                    }
                }
                let vals = self.eval_multi(r)?;
                if let ExprKind::Tuple(ls) = &l.peel_parens().kind {
                    for (l, v) in ls.iter().zip(vals.clone()) {
                        if let Some(l) = l.as_ref().unspan() {
                            self.assign(l, v)?;
                        }
                    }
                }
                Ok(vals)
            }
            _ => Ok(vec![self.eval(e)?]),
        }
    }

    /// A constant of contract `c` (or a file-level one), evaluated in a clean scope: its
    /// initializer can only name other constants.
    fn eval_const(&mut self, c: &'p Contract<'a>, n: &str, span: Span) -> Option<R<Val>> {
        let sv = c.state.iter().find(|s| s.name == n && s.constant)?;
        let Some(init) = sv.init else {
            return Some(Err((span, "constant without initializer".to_string())));
        };
        if self.depth > 16 {
            return Some(Err((span, "constants nested too deeply".to_string())));
        }
        let saved_env = std::mem::take(&mut self.env);
        let saved_aliases = std::mem::take(&mut self.aliases);
        let saved_slocals = std::mem::take(&mut self.slocals);
        let saved_alias_env = std::mem::take(&mut self.alias_env);
        let saved_c = std::mem::replace(&mut self.c, c);
        let saved_bound = std::mem::take(&mut self.bound);
        let saved_spec = std::mem::replace(&mut self.spec, false);
        self.depth += 1;
        let v = self.eval(init);
        self.depth -= 1;
        self.spec = saved_spec;
        self.bound = saved_bound;
        self.c = saved_c;
        self.aliases = saved_aliases;
        self.slocals = saved_slocals;
        self.alias_env = saved_alias_env;
        self.env = saved_env;
        let ty = sv.ty.clone();
        Some(v.map(|v| Var { ty, ..v }))
    }

    /// A contract (or library, interface) named `n`.
    fn contract_named(&self, n: &str) -> Option<&'p Contract<'a>> {
        self.prog.contracts.iter().find(|c| c.name == n)
    }

    /// An opaque compile-time constant (a selector, an interface id): equal names, equal values.
    fn named_const(&mut self, name: &str, ty: &Ty) -> Val {
        let f = format!("|{name}|");
        self.declare_fun(&f, 0, "Int");
        if let Some(r) = ty.range(&f)
            && self.declared_funs.insert(format!("{f}/range"))
        {
            self.smt.send(&format!(
                "(assert {r})
"
            ));
        }
        Self::val(ty.clone(), f)
    }

    fn mk_struct(
        &mut self,
        n: &str,
        fields: &[(String, Ty)],
        args: &'a ast::CallArgs<'a>,
        span: Span,
    ) -> R<Val> {
        let mut vals = vec![];
        match &args.kind {
            ast::CallArgsKind::Named(named) => {
                for (f, _) in fields {
                    let a = named
                        .iter()
                        .find(|na| na.name.as_str() == f)
                        .ok_or((span, format!("missing field `{f}`")))?;
                    vals.push(self.eval(a.value)?.t);
                }
            }
            ast::CallArgsKind::Unnamed(_) => {
                for a in args.exprs() {
                    vals.push(self.eval(a)?.t);
                }
            }
        }
        if vals.len() != fields.len() {
            return Err((span, format!("struct `{n}` needs {} fields", fields.len())));
        }
        if vals.is_empty() {
            return Ok(Self::val(Ty::Struct(n.into()), format!("|S.{n}.zero|")));
        }
        let v = Self::val(Ty::Struct(n.into()), format!("(|mk.{n}| {})", vals.join(" ")));
        if !self.spec
            && let Some(i) = self.struct_inv(n, &v.t)?
        {
            self.prove(
                &i,
                &format!("invariant of struct `{n}` not established at construction"),
                span,
                None,
            );
        }
        Ok(v)
    }

    /// Evaluate through the storage pointer `name`: its target expression, with the values it
    /// captured (indices of a pointer a function returned) in scope.
    fn with_alias<T>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Self, &'a Expr<'a>) -> R<T>,
    ) -> R<T> {
        let t = self.aliases[name];
        let Some(cap) = self.alias_env.get(name).cloned() else {
            return f(self, t);
        };
        let mut saved = vec![];
        for (k, v) in cap {
            saved.push((
                k.clone(),
                self.env.insert(k.clone(), v),
                self.aliases.remove(&k),
                self.alias_env.remove(&k),
            ));
        }
        let r = f(self, t);
        for (k, v, a, ae) in saved {
            match v {
                Some(v) => self.env.insert(k.clone(), v),
                None => self.env.remove(&k),
            };
            if let Some(a) = a {
                self.aliases.insert(k.clone(), a);
            }
            if let Some(ae) = ae {
                self.alias_env.insert(k, ae);
            }
        }
        r
    }

    fn val(ty: Ty, t: impl Into<String>) -> Val {
        Var { ty, t: t.into(), len: None }
    }

    fn eval(&mut self, e: &'a Expr<'a>) -> R<Val> {
        match &e.kind {
            ExprKind::Lit(lit, sub) => self.lit(lit, *sub, e.span),
            ExprKind::Ident(id)
                if self.aliases.contains_key(id.as_str())
                    && !self.bound.iter().any(|b| b == id.as_str()) =>
            {
                self.with_alias(id.as_str(), |s, t| s.eval(t))
            }
            ExprKind::Ident(id) => {
                let n = id.as_str();
                self.check_raw_enum(n, e.span)?;
                if let Some(v) = self.env.get(n) {
                    let v = v.clone();
                    if matches!(v.ty, Ty::Struct(_)) && self.c.state.iter().any(|s| s.name == n) {
                        self.assume_stored_struct(e, &v, e.span)?;
                    }
                    return Ok(v);
                }
                if self.spec && n == "result" {
                    if let [r] = &self.rets[..]
                        && let Some(v) = self.env.get(r)
                    {
                        return Ok(v.clone());
                    }
                    return Err((e.span, "`result` needs exactly one return value (name multiple returns and use the names)".into()));
                }
                let c = self.c;
                if let Some(v) = self.eval_const(c, n, e.span) {
                    return v;
                }
                if let Some(v) = self.fn_ref(None, n, e.span)? {
                    return Ok(v);
                }
                Err((e.span, format!("unknown identifier `{n}`")))
            }
            ExprKind::Tuple(xs) => match &xs[..] {
                [x] if x.as_ref().unspan().is_some() => self.eval(x.as_ref().unspan().unwrap()),
                _ => Err((e.span, "tuples are only supported in assignments and returns".into())),
            },
            ExprKind::Unary(op, x) => self.unary(e, op.kind, x),
            ExprKind::Binary(l, op, r) => self.binary(e, op.kind, l, r),
            ExprKind::Assign(l, None, r) if matches!(&l.peel_parens().kind, ExprKind::Ident(i) if self.slocals.contains(i.as_str())) =>
            {
                let ExprKind::Ident(i) = &l.peel_parens().kind else { unreachable!() };
                let name = i.as_str().to_string();
                let mut cap = None;
                let target: &'a Expr<'a> = match &r.peel_parens().kind {
                    ExprKind::Ident(j) if self.aliases.contains_key(j.as_str()) => {
                        cap = self.alias_env.get(j.as_str()).cloned();
                        self.aliases[j.as_str()]
                    }
                    ExprKind::Ident(_) | ExprKind::Index(..) | ExprKind::Member(..) => r,
                    ExprKind::Call(..) => {
                        // a function returning a storage pointer we can follow
                        self.ret_alias = None;
                        self.eval_multi(r)?;
                        match self.ret_alias.take() {
                            Some((t, env)) => {
                                cap = Some(env);
                                t
                            }
                            None => {
                                return Err((
                                    e.span,
                                    format!(
                                        "storage pointer `{name}` set from a call whose result we cannot follow"
                                    ),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err((
                            e.span,
                            format!(
                                "storage pointer `{name}` set from a function call or conditional is not supported"
                            ),
                        ));
                    }
                };
                if let Some(old) = self.aliases.get(&name)
                    && !std::ptr::eq(*old, target)
                {
                    return Err((
                        e.span,
                        format!("storage pointer `{name}` is pointed at more than one location"),
                    ));
                }
                self.env.remove(&name);
                self.aliases.insert(name.clone(), target);
                match cap {
                    Some(c) => self.alias_env.insert(name.clone(), c),
                    None => self.alias_env.remove(&name),
                };
                self.with_alias(&name, |s, t| s.eval(t))
            }
            ExprKind::Assign(l, op, r) => {
                if matches!(l.peel_parens().kind, ExprKind::Tuple(_)) {
                    self.eval_multi(e)?;
                    return Ok(Self::val(Ty::Bool, "true"));
                }
                let v = match op {
                    None => self.eval(r)?,
                    Some(op) => {
                        let lv = self.eval(l)?;
                        let rv = self.eval(r)?;
                        self.arith(op.kind, lv, rv, e.span)?
                    }
                };
                self.assign(l, v.clone())?;
                Ok(v)
            }
            ExprKind::Ternary(c, a, b) => {
                let cv = self.eval(c)?;
                if self.spec {
                    let av = self.eval(a)?;
                    let bv = self.eval(b)?;
                    return Ok(Var {
                        ty: av.ty.clone(),
                        t: format!("(ite {} {} {})", cv.t, av.t, bv.t),
                        len: None,
                    });
                }
                // forked states, merged as an if statement's are (see the tuple form above)
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                self.assume(&cv.t);
                let av = self.eval(a)?;
                let s1 = (
                    std::mem::replace(&mut self.guard, g0),
                    std::mem::replace(&mut self.env, env0),
                );
                self.assume(&format!("(not {})", cv.t));
                let bv = self.eval(b)?;
                let s2 = (self.guard.clone(), self.env.clone());
                let (g, en) = self.merge(vec![s1, s2]);
                self.guard = g;
                self.env = en;
                let ty = if av.ty == Ty::Lit { bv.ty.clone() } else { av.ty.clone() };
                Ok(Var { ty, t: format!("(ite {} {} {})", cv.t, av.t, bv.t), len: None })
            }
            ExprKind::Index(base, IndexKind::Index(Some(idx))) => {
                let b = self.eval(base)?;
                let i = self.eval(idx)?;
                match &b.ty {
                    Ty::Map(_, v) => {
                        let t = format!("(select {} {})", b.t, i.t);
                        let ty = (**v).clone();
                        if self.bound.is_empty()
                            && let Some(r) = ty.range(&t)
                        {
                            self.smt.send(&format!("(assert {r})\n"));
                        }
                        let out = Self::val(ty, t);
                        self.assume_stored_struct(e, &out, e.span)?;
                        Ok(out)
                    }
                    Ty::Arr(el, _) => {
                        let len = match &b.len {
                            Some(l) => l.clone(),
                            None => self.arr_len(&b.ty),
                        };
                        self.panic_unless(
                            &format!("(and (<= 0 {}) (< {} {len}))", i.t, i.t),
                            "array index may be out of bounds",
                            idx.span,
                        );
                        let t = format!("(select {} {})", b.t, i.t);
                        let ty = (**el).clone();
                        if self.bound.is_empty()
                            && let Some(r) = ty.range(&t)
                        {
                            self.smt.send(&format!("(assert {r})\n"));
                        }
                        let out = Self::val(ty, t);
                        self.assume_stored_struct(e, &out, e.span)?;
                        Ok(out)
                    }
                    Ty::Opaque => {
                        // A byte of bytes we do not track; an index past the length reverts.
                        let len = self.opaque_len(&b.t);
                        self.panic_unless(
                            &format!("(and (<= 0 {}) (< {} {len}))", i.t, i.t),
                            "bytes index may be out of bounds",
                            idx.span,
                        );
                        let t = self.decl("byte", &Ty::Uint(8));
                        Ok(Self::val(Ty::Uint(8), t))
                    }
                    t => Err((e.span, format!("cannot index a value of type {}", t.name()))),
                }
            }
            ExprKind::Member(base, m) => self.member(e, base, m.as_str()),
            ExprKind::Index(base, IndexKind::Range(lo, hi)) => {
                // A calldata/bytes slice: its contents are not tracked; it reverts unless
                // start <= end <= length.
                let b = self.eval(base)?;
                let len = match (&b.len, &b.ty) {
                    (Some(l), _) => l.clone(),
                    (None, Ty::Opaque) => self.opaque_len(&b.t),
                    (None, ty) => self.arr_len(ty),
                };
                let lo = match lo {
                    Some(x) => self.eval(x)?.t,
                    None => "0".into(),
                };
                let hi = match hi {
                    Some(x) => self.eval(x)?.t,
                    None => len.clone(),
                };
                let ok = format!("(and (<= 0 {lo}) (<= {lo} {hi}) (<= {hi} {len}))");
                self.reverts_if(&format!("(not {ok})"));
                self.assume(&ok);
                let t = self.decl("slice", &Ty::Opaque);
                Ok(Self::val(Ty::Opaque, t))
            }
            ExprKind::Call(callee, args) => {
                let vs = self.call(e, callee, args)?;
                Ok(vs.into_iter().next().unwrap_or(Self::val(Ty::Bool, "true")))
            }
            ExprKind::Payable(args) => {
                let a = args
                    .exprs()
                    .next()
                    .ok_or((e.span, "payable() needs an argument".to_string()))?;
                let v = self.eval(a)?;
                Ok(Self::val(Ty::Addr, v.t))
            }
            ExprKind::Delete(x) => {
                let v = self.eval(x)?;
                let z = self.zero_var(&v.ty);
                self.assign(x, z)?;
                Ok(Self::val(Ty::Bool, "true"))
            }
            _ => Err((e.span, "unsupported expression".into())),
        }
    }

    fn lit(&mut self, lit: &ast::Lit<'_>, sub: Option<ast::SubDenomination>, span: Span) -> R<Val> {
        let _ = sub; // Solar already folds sub-denominations into the literal value
        let mul = BigInt::from(1);
        match &lit.kind {
            LitKind::Number(n) => {
                let b: BigInt = n.to_string().parse().unwrap();
                Ok(Self::val(Ty::Lit, num(&(b * mul))))
            }
            LitKind::Bool(b) => Ok(Self::val(Ty::Bool, if *b { "true" } else { "false" })),
            LitKind::Address(a) => {
                let s = a.to_string();
                let b = BigInt::parse_bytes(s.trim_start_matches("0x").as_bytes(), 16)
                    .unwrap_or_default();
                Ok(Self::val(Ty::Addr, num(&b)))
            }
            LitKind::Str(..) => {
                let t = self.decl("str", &Ty::Opaque);
                Ok(Self::val(Ty::Opaque, t))
            }
            _ => Err((span, "unsupported literal".into())),
        }
    }

    fn unary(&mut self, e: &'a Expr<'a>, op: UnOpKind, x: &'a Expr<'a>) -> R<Val> {
        let v = self.eval(x)?;
        match op {
            UnOpKind::Not => Ok(Self::val(Ty::Bool, format!("(not {})", v.t))),
            UnOpKind::Neg => {
                let r = Self::val(v.ty.clone(), format!("(- {})", v.t));
                self.fit(r, e.span)
            }
            UnOpKind::BitNot => match v.ty {
                Ty::Uint(n) => Ok(Self::val(
                    v.ty.clone(),
                    format!("(- {} {})", num(&(pow2(n as u32) - 1)), v.t),
                )),
                _ => Ok(Self::val(v.ty.clone(), format!("(- (- {}) 1)", v.t))),
            },
            UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                let k = if matches!(op, UnOpKind::PreInc | UnOpKind::PostInc) {
                    BinOpKind::Add
                } else {
                    BinOpKind::Sub
                };
                let nv = self.arith(k, v.clone(), Self::val(Ty::Lit, "1"), e.span)?;
                self.assign(x, nv.clone())?;
                Ok(if op.is_prefix() { nv } else { v })
            }
        }
    }

    fn binary(
        &mut self,
        e: &'a Expr<'a>,
        op: BinOpKind,
        l: &'a Expr<'a>,
        r: &'a Expr<'a>,
    ) -> R<Val> {
        use BinOpKind::*;
        match op {
            And | Or => {
                let lv = self.eval(l)?;
                if self.spec {
                    let rv = self.eval(r)?;
                    let o = if op == And { "and" } else { "or" };
                    return Ok(Self::val(Ty::Bool, format!("({o} {} {})", lv.t, rv.t)));
                }
                let g0 = self.guard.clone();
                let lt = if op == And { lv.t.clone() } else { format!("(not {})", lv.t) };
                self.assume(&lt);
                let rv = self.eval(r)?;
                let gr = self.guard.clone();
                let g = format!("(or (and {g0} (not {lt})) {gr})");
                self.guard = self.def("g", "Bool", &g);
                let o = if op == And { "and" } else { "or" };
                Ok(Self::val(Ty::Bool, format!("({o} {} {})", lv.t, rv.t)))
            }
            Eq | Ne | Lt | Le | Gt | Ge => {
                let a = self.eval(l)?;
                let b = self.eval(r)?;
                let t = match op {
                    Eq => format!("(= {} {})", a.t, b.t),
                    Ne => format!("(not (= {} {}))", a.t, b.t),
                    Lt => format!("(< {} {})", a.t, b.t),
                    Le => format!("(<= {} {})", a.t, b.t),
                    Gt => format!("(> {} {})", a.t, b.t),
                    _ => format!("(>= {} {})", a.t, b.t),
                };
                Ok(Self::val(Ty::Bool, t))
            }
            _ => {
                let a = self.eval(l)?;
                let b = self.eval(r)?;
                self.arith(op, a, b, e.span)
            }
        }
    }

    /// Range handling for an arithmetic result: math in specs, wrap in `unchecked`, panic
    /// otherwise.
    fn fit(&mut self, v: Val, span: Span) -> R<Val> {
        if self.spec || v.ty == Ty::Lit {
            return Ok(v);
        }
        let Some((lo, hi)) = v.ty.bounds() else {
            return Ok(v);
        };
        if self.unchecked {
            return Ok(Self::val(v.ty.clone(), self.wrapr(&v.ty, &v.t, &lo, &hi)));
        }
        if let Some((l, h)) = self.iv(&v.t)
            && l >= lo
            && h <= hi
        {
            // in range by construction: no overflow obligation, no solver query
            self.checks += 1;
            let guard = self.guard.clone();
            let range = format!("(and (<= {} {}) (<= {} {}))", num(&lo), v.t, v.t, num(&hi));
            self.smt.settled(&guard, &range, Role::Side, "the interval analysis bounds it");
            return Ok(v);
        }
        let t = self.def("arith", "Int", &v.t);
        self.panic_unless(
            &format!("(and (<= {} {t}) (<= {t} {}))", num(&lo), num(&hi)),
            "possible arithmetic overflow/underflow",
            span,
        );
        Ok(Self::val(v.ty, t))
    }

    fn arith(&mut self, op: BinOpKind, a: Val, b: Val, span: Span) -> R<Val> {
        use BinOpKind::*;
        let ty = if matches!(op, Pow | Shl | Shr | Sar) {
            // The result has the base's type. A literal base with a non-literal exponent or shift
            // amount is computed as uint256 (int256 if negative) since Solidity 0.7.
            match (&a.ty, &b.ty) {
                (Ty::Lit, Ty::Lit) => Ty::Lit,
                (Ty::Lit, _) if a.t.starts_with("(-") => Ty::Int(256),
                (Ty::Lit, _) => Ty::Uint(256),
                _ => a.ty.clone(),
            }
        } else if a.ty == Ty::Lit {
            b.ty.clone()
        } else {
            a.ty.clone()
        };
        let ty = if self.spec { Ty::Lit } else { ty };
        // exponent / shift amount small enough for the pow2 table even in (unbounded) spec math
        let small = |scale: u64| {
            b.ty.bounds()
                .is_some_and(|(lo, hi)| lo >= BigInt::from(0) && hi * scale < BigInt::from(256))
        };
        let signed = matches!(ty, Ty::Int(_));
        let (x, y) = (a.t, b.t.clone());
        let t = match op {
            Add => format!("(+ {x} {y})"),
            Sub => format!("(- {x} {y})"),
            // multiplying by a 0/1 flag (bool-to-int idioms) is a choice, not nonlinear arithmetic
            Mul if y.starts_with("(ite ") && y.ends_with(" 1 0)") => {
                format!("(ite {} {x} 0)", &y[5..y.len() - 5])
            }
            Mul if x.starts_with("(ite ") && x.ends_with(" 1 0)") => {
                format!("(ite {} {y} 0)", &x[5..x.len() - 5])
            }
            Mul => format!("(* {x} {y})"),
            Div | Rem => {
                self.panic_unless(&format!("(not (= {y} 0))"), "possible division by zero", span);
                let q = if signed || self.spec {
                    format!(
                        "(* (ite (< {y} 0) (- 1) 1) (ite (>= {x} 0) (div {x} (abs {y})) (- (div (- {x}) (abs {y})))))"
                    )
                } else {
                    format!("(div {x} {y})")
                };
                if op == Div { q } else { format!("(- {x} (* {y} {q}))") }
            }
            Pow => {
                let lit: Option<u32> = y.parse().ok();
                match (x.parse::<BigInt>().ok(), lit) {
                    (Some(bx), Some(k)) => num(&bx.pow(k)),
                    (_, Some(k)) if k <= 8 => {
                        if k == 0 {
                            "1".into()
                        } else {
                            format!("(* {})", vec![x.as_str(); k as usize].join(" "))
                        }
                    }
                    (Some(bx), None)
                        if bx > BigInt::from(1)
                            && (&bx & (&bx - 1u32)) == BigInt::from(0)
                            && (!self.spec || small(bx.bits() - 1)) =>
                    {
                        // (2^j)^y = 2^(j*y); exact through the pow2 table (see ensure_pow2)
                        let j = bx.bits() - 1;
                        self.ensure_pow2();
                        format!("(|pow2| (* {j} {y}))")
                    }
                    (Some(bx), None)
                        if bx > BigInt::from(1) && (&bx & (&bx - 1u32)) == BigInt::from(0) =>
                    {
                        // spec math is unbounded: 2^e is exact through the table for 0 <= e < 256,
                        // and for larger e an uninterpreted value known to be at least 2^256 (so a
                        // 256-bit word divided by it is 0, as the EVM's shift gives)
                        let j = bx.bits() - 1;
                        self.ensure_pow2();
                        if self.declared_funs.insert("sv_pow2big/1".into()) {
                            self.smt.send(&format!(
                                "(declare-fun sv_pow2big (Int) Int)\n(assert (forall ((e Int)) (! (>= (sv_pow2big e) {}) :pattern ((sv_pow2big e)))))\n",
                                num(&pow2(256))
                            ));
                        }
                        self.declare_fun("sv_pow", 2, "Int");
                        let e = format!("(* {j} {y})");
                        format!(
                            "(ite (and (<= 0 {e}) (< {e} 256)) (|pow2| {e}) (ite (< {e} 0) (sv_pow {x} {y}) (sv_pow2big {e})))"
                        )
                    }
                    _ => {
                        self.declare_fun("sv_pow", 2, "Int");
                        format!("(sv_pow {x} {y})")
                    }
                }
            }
            Shl | Shr | Sar => {
                let k: Option<u32> = y.parse().ok();
                match k {
                    Some(k) => {
                        let p = num(&pow2(k));
                        if op == Shl {
                            let (lo, hi) = ty.bounds().unwrap_or((BigInt::from(0), pow2(256) - 1));
                            return Ok(Self::val(
                                ty.clone(),
                                self.wrapr(&ty, &format!("(* {x} {p})"), &lo, &hi),
                            ));
                        }
                        return Ok(Self::val(ty, format!("(div {x} {p})")));
                    }
                    // >> is floor(x / 2^y) in specs too (exact, no wrap to worry about); << wraps,
                    // so specs keep it exact only for small shift amounts
                    None if !self.spec || small(1) || op != Shl => {
                        // exact: x << y = x * 2^y (wrapped), x >> y = floor(x / 2^y); 2^y for
                        // y >= 256 reads 2^256, which gives 0 (or -1 for negative x) as the EVM
                        // does
                        self.ensure_pow2();
                        let p = format!("(|pow2| {y})");
                        if op == Shl {
                            let (lo, hi) = ty.bounds().unwrap_or((BigInt::from(0), pow2(256) - 1));
                            return Ok(Self::val(
                                ty.clone(),
                                self.wrapr(&ty, &format!("(* {x} {p})"), &lo, &hi),
                            ));
                        }
                        return Ok(Self::val(ty, format!("(div {x} {p})")));
                    }
                    None => {
                        let f = format!("sv_{}", if op == Shl { "shl" } else { "shr" });
                        self.declare_fun(&f, 2, "Int");
                        self.ret_bound(&f, &ty);
                        let t = format!("({f} {x} {y})");
                        if let Some(r) = ty.range(&t) {
                            self.smt.send(&format!("(assert {r})\n"));
                        }
                        return Ok(Self::val(ty, t));
                    }
                }
            }
            BitAnd | BitOr | BitXor => {
                let n = match ty {
                    Ty::Uint(n) => n,
                    Ty::Lit => 256,
                    _ => {
                        let f = format!("sv_bit{op:?}").to_lowercase();
                        self.declare_fun(&f, 2, "Int");
                        return Ok(Self::val(ty, format!("({f} {x} {y})")));
                    }
                };
                let bv = match op {
                    BitAnd => "bvand",
                    BitOr => "bvor",
                    _ => "bvxor",
                };
                let (bx, by) = (self.bv_term(&x, n as u32), self.bv_term(&y, n as u32));
                return Ok(Self::val(ty, format!("(bv2nat ({bv} {bx} {by}))")));
            }
            _ => return Err((span, format!("unsupported operator {op:?}"))),
        };
        self.fit(Self::val(ty, t), span)
    }

    fn member(&mut self, e: &'a Expr<'a>, base: &'a Expr<'a>, m: &str) -> R<Val> {
        if m == "selector" {
            // a function selector: an opaque bytes4 constant per named function
            let key = match &base.kind {
                ExprKind::Member(b2, f) => match &b2.kind {
                    ExprKind::Ident(c) => Some(format!("sel.{}.{}", c.as_str(), f.as_str())),
                    _ => None,
                },
                ExprKind::Ident(f) => Some(format!("sel.{}.{}", self.c.name, f.as_str())),
                _ => None,
            };
            if let Some(k) = key {
                return Ok(self.named_const(&k, &Ty::Uint(32)));
            }
        }
        if let ExprKind::Member(b2, en) = &base.kind {
            // `C.Enum.Variant`
            if let ExprKind::Ident(cn) = &b2.kind
                && self.contract_named(cn.as_str()).is_some()
                && self.prog.enums.contains_key(en.as_str())
                && let Some(idx) = enum_index(self.prog, en.as_str(), m)
            {
                return Ok(Self::val(Ty::Lit, idx.to_string()));
            }
        }
        if let ExprKind::Ident(b) = &base.kind {
            let key = format!("{}.{m}", b.as_str());
            if let Some(v) = self.env.get(&key) {
                return Ok(v.clone());
            }
            if !self.env.contains_key(b.as_str())
                && let Some(owner) = self.contract_named(b.as_str())
            {
                // `Lib.CONSTANT` / `C.CONSTANT`
                if let Some(v) = self.eval_const(owner, m, e.span) {
                    return v;
                }
                // `Lib.f` as a value
                if owner.kind == ast::ContractKind::Library
                    && let Some(v) = self.fn_ref(Some(owner), m, e.span)?
                {
                    return Ok(v);
                }
            }
            if let Some(k) = self.prog.enums.get(b.as_str()) {
                let _ = k;
                // enum member: find index
                let idx = enum_index(self.prog, b.as_str(), m).unwrap_or(0);
                return Ok(Self::val(Ty::Lit, idx.to_string()));
            }
        }
        if let ExprKind::TypeCall(t) = &base.kind {
            if m == "interfaceId" {
                let n = match &t.kind {
                    ast::TypeKind::Custom(p) => p.last().as_str().to_string(),
                    _ => "?".into(),
                };
                return Ok(self.named_const(&format!("iid.{n}"), &Ty::Uint(32)));
            }
            if m == "name" || m == "creationCode" || m == "runtimeCode" {
                let t = self.decl("typeinfo", &Ty::Opaque);
                return Ok(Self::val(Ty::Opaque, t));
            }
            let ty = self.ty_of_decl(t);
            let (lo, hi) = ty.bounds().ok_or((e.span, "type(..) of non-integer".to_string()))?;
            return Ok(Self::val(ty, num(if m == "max" { &hi } else { &lo })));
        }
        if !["length", "balance", "code", "codehash"].contains(&m) {
            let b = self.eval(base)?;
            if let Ty::Struct(sn) = &b.ty {
                let fty = self
                    .prog
                    .struct_fields(sn)
                    .and_then(|fs| fs.iter().find(|(f, _)| f == m))
                    .map(|(_, t)| t.clone())
                    .ok_or((e.span, format!("struct `{sn}` has no field `{m}`")))?;
                let t = format!("(|{sn}.{m}| {})", b.t);
                if self.bound.is_empty()
                    && let Some(r) = fty.range(&t)
                {
                    self.smt.send(&format!(
                        "(assert {r})
"
                    ));
                }
                return Ok(Var {
                    len: if let Ty::Arr(_, Some(n)) = fty { Some(n.to_string()) } else { None },
                    ty: fty,
                    t,
                });
            }
            return Err((e.span, format!("unsupported member access `.{m}` on {}", b.ty.name())));
        }
        if m == "length" {
            let b = self.eval(base)?;
            return match b.len {
                Some(l) => Ok(Self::val(Ty::Uint(256), l)),
                None if b.ty.fixed_len().is_some() => {
                    Ok(Self::val(Ty::Uint(256), b.ty.fixed_len().unwrap().to_string()))
                }
                None if b.ty == Ty::Opaque => {
                    // untracked bytes: the length is a function of the value, so the same
                    // value read twice (in code and in a spec) has the same length
                    self.declare_fun("sv_blen", 1, "Int");
                    let t = format!("(sv_blen {})", b.t);
                    if self.bound.is_empty() {
                        self.smt.send(&format!(
                            "(assert {})
",
                            Ty::Uint(256).range(&t).unwrap()
                        ));
                    }
                    Ok(Self::val(Ty::Uint(256), t))
                }
                None => {
                    let t = self.decl("len", &Ty::Uint(256));
                    Ok(Self::val(Ty::Uint(256), t))
                }
            };
        }
        if m == "balance" {
            let b = self.eval(base)?;
            if let Some(bal) = self.env.get("__balance").cloned() {
                let t = format!("(select {} {})", bal.t, b.t);
                if self.bound.is_empty() {
                    self.smt.send(&format!(
                        "(assert {})
",
                        Ty::Uint(256).range(&t).unwrap()
                    ));
                }
                return Ok(Self::val(Ty::Uint(256), t));
            }
        }
        if (m == "code" || m == "codehash")
            && let Some(ep) = self.env.get("__code_epoch").cloned()
        {
            // the code at an address is a function of the address and the code epoch
            let b = self.eval(base)?;
            let f = format!("sv_{m}");
            self.declare_fun(&f, 2, "Int");
            let t = format!("({f} {} {})", b.t, ep.t);
            if m == "code" {
                return Ok(Self::val(Ty::Opaque, t));
            }
            if self.bound.is_empty() {
                self.smt.send(&format!("(assert {})\n", Ty::Uint(256).range(&t).unwrap()));
            }
            return Ok(Self::val(Ty::Uint(256), t));
        }
        if m == "balance" || m == "code" || m == "codehash" {
            let b = self.eval(base)?;
            let _ = b;
            let t = self.decl(m, &Ty::Uint(256));
            return Ok(Self::val(Ty::Uint(256), t));
        }
        Err((e.span, format!("unsupported member access `.{m}`")))
    }

    fn call(
        &mut self,
        e: &'a Expr<'a>,
        callee: &'a Expr<'a>,
        args: &'a ast::CallArgs<'a>,
    ) -> R<Vec<Val>> {
        let argv: Vec<&'a Expr<'a>> = args.exprs().collect();
        let callee = match &callee.kind {
            ExprKind::CallOptions(c, opts) => {
                if let Some(o) = opts.iter().find(|o| o.name.name == sym::value) {
                    let v = self.eval(o.value)?;
                    self.call_value = Some(v.t);
                }
                &**c
            }
            _ => {
                self.call_value = None;
                callee
            }
        };
        match &callee.kind {
            ExprKind::Ident(id)
                if self.env.get(id.as_str()).is_some_and(|v| matches!(v.ty, Ty::Fn(_))) =>
            {
                let v = self.env[id.as_str()].clone();
                self.call_ptr(e, v, &argv)
            }
            ExprKind::Ident(id) => {
                let n = id.as_str();
                match n {
                    "require" => {
                        let c = self.eval(argv[0])?;
                        self.reverts_if(&format!("(not {})", c.t));
                        self.assume(&c.t);
                        return Ok(vec![]);
                    }
                    "assert" => {
                        let c = self.eval(argv[0])?;
                        self.prove(&c.t, "assertion failed", e.span, None);
                        return Ok(vec![]);
                    }
                    "revert" => {
                        self.reverts_if("true");
                        self.guard = "false".into();
                        return Ok(vec![]);
                    }
                    "old" if self.spec => {
                        let mut oldenv = self.old.clone().ok_or((
                            e.span,
                            "old() is only allowed in postconditions and loop invariants"
                                .to_string(),
                        ))?;
                        for b in &self.bound {
                            if let Some(v) = self.env.get(b) {
                                oldenv.insert(b.clone(), v.clone());
                            }
                        }
                        let saved = std::mem::replace(&mut self.env, oldenv);
                        let r = self.eval(argv[0]);
                        self.env = saved;
                        return Ok(vec![r?]);
                    }
                    "__implies" => {
                        let a = self.eval(argv[0])?;
                        let b = self.eval(argv[1])?;
                        return Ok(vec![Self::val(Ty::Bool, format!("(=> {} {})", a.t, b.t))]);
                    }
                    "keccak256" => return Ok(vec![self.keccak_solidity(&argv)?]),
                    "sha256" | "ripemd160" => {
                        let mut flat = vec![];
                        for a in &argv {
                            match &a.kind {
                                ExprKind::Call(c2, inner) if matches!(&c2.kind, ExprKind::Member(b, _) if matches!(&b.kind, ExprKind::Ident(i) if i.name == sym::abi)) => {
                                    for x in inner.exprs() {
                                        flat.push(self.eval(x)?.t);
                                    }
                                }
                                _ => flat.push(self.eval(a)?.t),
                            }
                        }
                        let f = format!("sv_{n}");
                        self.declare_fun(&f, flat.len(), "Int");
                        self.ret_bound(&f, &Ty::Uint(256));
                        self.injective(&f, flat.len());
                        let t = if flat.is_empty() {
                            f.clone()
                        } else {
                            format!("({f} {})", flat.join(" "))
                        };
                        let ty = Ty::Uint(256);
                        if self.bound.is_empty() {
                            self.smt.send(&format!("(assert {})\n", ty.range(&t).unwrap()));
                        }
                        return Ok(vec![Self::val(ty, t)]);
                    }
                    "ecrecover" => {
                        let mut xs = vec![];
                        for a in &argv {
                            xs.push(self.eval(a)?.t);
                        }
                        self.declare_fun("sv_ecrecover", xs.len(), "Int");
                        self.ret_bound("sv_ecrecover", &Ty::Addr);
                        let t = format!("(sv_ecrecover {})", xs.join(" "));
                        if self.bound.is_empty() {
                            self.smt.send(&format!(
                                "(assert {})
",
                                Ty::Addr.range(&t).unwrap()
                            ));
                        }
                        return Ok(vec![Self::val(Ty::Addr, t)]);
                    }
                    "addmod" | "mulmod" => {
                        let a = self.eval(argv[0])?;
                        let b = self.eval(argv[1])?;
                        let m = self.eval(argv[2])?;
                        self.panic_unless(
                            &format!("(not (= {} 0))", m.t),
                            "possible division by zero",
                            e.span,
                        );
                        let o = if n == "addmod" { "+" } else { "*" };
                        return Ok(vec![Self::val(
                            Ty::Uint(256),
                            format!("(mod ({o} {} {}) {})", a.t, b.t, m.t),
                        )]);
                    }
                    "gasleft" | "blockhash" => {
                        let t = self.decl(n, &Ty::Uint(256));
                        return Ok(vec![Self::val(Ty::Uint(256), t)]);
                    }
                    _ => {}
                }
                if let Some(idx) = n.strip_prefix("__q").and_then(|i| i.parse::<usize>().ok()) {
                    return Ok(vec![self.quant(e, idx, argv[0])?]);
                }
                if let Some(fields) = self.prog.struct_fields(n).cloned() {
                    return Ok(vec![self.mk_struct(n, &fields, args, e.span)?]);
                }
                if self.prog.contract(n).is_some() {
                    let v = self.eval(argv[0])?;
                    return Ok(vec![Self::val(Ty::Contract(n.into()), v.t)]);
                }
                if let Some(k) = self.prog.enums.get(n) {
                    let v = self.eval(argv[0])?;
                    let ty = Ty::Enum(n.into(), *k);
                    self.panic_unless(
                        &ty.range(&v.t).unwrap(),
                        "enum conversion out of range",
                        e.span,
                    );
                    return Ok(vec![Self::val(ty, v.t)]);
                }
                let c = self.c;
                let vals = argv.iter().map(|a| self.eval(a)).collect::<R<Vec<_>>>()?;
                if let Some(f) = pick(&c.funcs, n, &vals) {
                    let rets = self.internal_call(c, f, vals, e.span, false)?;
                    self.write_back_storage_params(f, &argv, e.span)?;
                    return Ok(rets);
                }
                // a file-level (free) function
                let free = self.prog.contract(FREE);
                let Some((lib, f)) = free.and_then(|l| Some((l, pick(&l.funcs, n, &vals)?))) else {
                    return Err((e.span, format!("unknown function `{n}`")));
                };
                let rets = self.internal_call(lib, f, vals, e.span, true)?;
                self.write_back_storage_params(f, &argv, e.span)?;
                Ok(rets)
            }
            ExprKind::Type(t) => {
                let ty = self.ty_of_decl(t);
                let v = self.eval(argv[0])?;
                Ok(vec![self.cast(ty, v)])
            }
            ExprKind::New(t) => {
                let value = self.call_value.take();
                match &t.kind {
                    ast::TypeKind::Array(a) => {
                        // a fresh zeroed memory array of the given length
                        let el = self.ty_of_decl(&a.element);
                        let n = self.eval(argv[0])?;
                        self.alloc_limit(&n.t, 32);
                        let ty = Ty::Arr(Box::new(el), None);
                        Ok(vec![Var { t: ty.zero(), ty, len: Some(n.t) }])
                    }
                    ast::TypeKind::Elementary(_) => {
                        for a in &argv {
                            let n = self.eval(a)?;
                            self.alloc_limit(&n.t, 1);
                        }
                        let t = self.decl("newbytes", &Ty::Opaque);
                        Ok(vec![Self::val(Ty::Opaque, t)])
                    }
                    ast::TypeKind::Custom(p) => {
                        // contract creation: its constructor is outside code that may call back
                        // in, like any external call; the result is a fresh address
                        let cn = p.last().as_str().to_string();
                        for a in &argv {
                            self.eval(a)?;
                        }
                        if let Some(v) = value {
                            self.send_value(&v, None);
                        }
                        let ty = if self.contract_named(&cn).is_some() {
                            Ty::Contract(cn)
                        } else {
                            Ty::Addr
                        };
                        self.external_call(vec![ty], false, e.span)
                    }
                    _ => Err((e.span, "unsupported `new`".into())),
                }
            }
            ExprKind::Member(base, m) => {
                let m = m.as_str();
                if let ExprKind::Ident(b) = &base.kind {
                    let bn = b.as_str();
                    if bn == "super" {
                        let vals = argv.iter().map(|a| self.eval(a)).collect::<R<Vec<_>>>()?;
                        let f = self
                            .super_target(m, &vals)
                            .ok_or((e.span, format!("cannot resolve `super.{m}` here")))?;
                        let c = self.c;
                        let rets = self.internal_call(c, f, vals, e.span, false)?;
                        self.write_back_storage_params(f, &argv, e.span)?;
                        return Ok(rets);
                    }
                    if bn == "abi" {
                        // the encoded bytes of abi.decode, as a term
                        let mut decode_src: Option<String> = None;
                        for (ai, a) in
                            argv.iter().enumerate().take(if m == "decode" { 1 } else { argv.len() })
                        {
                            // function references (encodeCall's target) and types are not values
                            let fref = (m == "encodeCall" && ai == 0)
                                || match &a.peel_parens().kind {
                                    ExprKind::Type(_) => true,
                                    ExprKind::Member(b, _) => {
                                        matches!(&b.kind, ExprKind::Ident(i) if self.contract_named(i.as_str()).is_some() && !self.env.contains_key(i.as_str()))
                                    }
                                    ExprKind::Ident(i) => {
                                        !self.env.contains_key(i.as_str())
                                            && self.c.funcs.iter().any(|f| f.name == i.as_str())
                                    }
                                    _ => false,
                                };
                            if !fref {
                                if m == "decode" && ai == 0 {
                                    let v = self.eval(a)?;
                                    if v.ty.sort() == "Int" {
                                        decode_src = Some(v.t);
                                    }
                                } else {
                                    self.eval_multi(a)?;
                                }
                            }
                        }
                        if m == "decode" && argv.len() == 2 {
                            // Decoding reverts on data too short or not of the types' ranges,
                            // which the untracked bytes do not tell.
                            self.reverts_if("true");
                            // decoded values are arbitrary values of their types
                            let tys: Vec<&'a Expr<'a>> = match &argv[1].peel_parens().kind {
                                ExprKind::Tuple(xs) if xs.len() != 1 => xs
                                    .iter()
                                    .filter_map(|x| x.as_ref().unspan().map(|x| &**x))
                                    .collect(),
                                _ => vec![argv[1].peel_parens()],
                            };
                            let mut out = vec![];
                            let n_tys = tys.len();
                            for te in tys {
                                let ty = match &te.kind {
                                    ExprKind::Type(t) => self.ty_of_decl(t),
                                    ExprKind::Ident(i)
                                        if self.prog.struct_fields(i.as_str()).is_some() =>
                                    {
                                        Ty::Struct(i.as_str().into())
                                    }
                                    ExprKind::Ident(i)
                                        if self.contract_named(i.as_str()).is_some() =>
                                    {
                                        Ty::Contract(i.as_str().into())
                                    }
                                    ExprKind::Ident(i)
                                        if self.prog.enums.contains_key(i.as_str()) =>
                                    {
                                        Ty::Enum(i.as_str().into(), self.prog.enums[i.as_str()])
                                    }
                                    _ => {
                                        return Err((
                                            te.span,
                                            "unsupported type in abi.decode".into(),
                                        ));
                                    }
                                };
                                if let Ty::Struct(sn) = &ty {
                                    // struct fields get their ranges
                                    let fields =
                                        self.prog.struct_fields(sn).cloned().unwrap_or_default();
                                    let mut ts = vec![];
                                    for (f, fty) in &fields {
                                        ts.push(self.fresh_var(f, fty).t);
                                    }
                                    let t = if ts.is_empty() {
                                        format!("|S.{sn}.zero|")
                                    } else {
                                        format!("(|mk.{sn}| {})", ts.join(" "))
                                    };
                                    out.push(Self::val(ty, t));
                                } else if let (
                                    Some(src),
                                    Ty::Bool
                                    | Ty::Uint(_)
                                    | Ty::Int(_)
                                    | Ty::Addr
                                    | Ty::Contract(_)
                                    | Ty::Enum(..),
                                ) = (&decode_src, &ty)
                                {
                                    // decoding is a function of the bytes (as keccak256 is): the
                                    // same bytes decode to the same value, within the type's range
                                    let mangled: String = format!("{ty:?}")
                                        .chars()
                                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                        .collect();
                                    let f =
                                        format!("|sv_decode.{mangled}.{}.{}|", out.len(), n_tys);
                                    self.declare_fun(&f, 1, &ty.sort());
                                    let app = format!("({f} {src})");
                                    if let Some(r) = ty.range(&format!("({f} sv_b)"))
                                        && self.declared_funs.insert(format!("{f}/range"))
                                    {
                                        self.smt.send(&format!(
                                                "(assert (forall ((sv_b Int)) (! {r} :pattern (({f} sv_b)))))\n"
                                            ));
                                    }
                                    out.push(Self::val(ty.clone(), app));
                                } else {
                                    out.push(self.fresh_var("decoded", &ty));
                                }
                            }
                            return Ok(out);
                        }
                        let t = self.decl("abi", &Ty::Opaque);
                        return Ok(vec![Self::val(Ty::Opaque, t)]);
                    }
                    if let Some(fields) =
                        self.contract_named(bn).and_then(|_| self.prog.struct_fields(m).cloned())
                    {
                        return Ok(vec![self.mk_struct(m, &fields, args, e.span)?]);
                    }
                    if let Some(uty) = self.prog.udvts.get(bn).cloned()
                        && (m == "wrap" || m == "unwrap")
                    {
                        let v = self.eval(argv[0])?;
                        return Ok(vec![Self::val(
                            if m == "wrap" { uty } else { v.ty.clone() },
                            v.t,
                        )]);
                    }
                    if let Some(lib) =
                        self.prog.contract(bn).filter(|c| c.kind == ast::ContractKind::Library)
                    {
                        let vals = argv.iter().map(|a| self.eval(a)).collect::<R<Vec<_>>>()?;
                        let f = pick(&lib.funcs, m, &vals)
                            .ok_or((e.span, format!("unknown library function `{bn}.{m}`")))?;
                        let rets = self.internal_call(lib, f, vals, e.span, true)?;
                        self.write_back_storage_params(f, &argv, e.span)?;
                        return Ok(rets);
                    }
                }
                if let Some((lib, f, vals, all)) = self.pick_using(base, m, &argv)? {
                    let rets = self.internal_call(lib, f, vals, e.span, true)?;
                    self.write_back_storage_params(f, &all, e.span)?;
                    return Ok(rets);
                }
                if matches!(base.kind, ExprKind::Type(_)) && m == "concat" {
                    // bytes.concat / string.concat
                    for a in &argv {
                        self.eval(a)?;
                    }
                    let t = self.decl("concat", &Ty::Opaque);
                    return Ok(vec![Self::val(Ty::Opaque, t)]);
                }
                let bv = self.eval(base)?;
                match (&bv.ty, m) {
                    (Ty::Arr(el, _), "push") => {
                        let el = (**el).clone();
                        let len = match &bv.len {
                            Some(l) => l.clone(),
                            None => self.arr_len(&bv.ty),
                        };
                        let v = match argv.first() {
                            Some(a) => self.eval(a)?,
                            None => self.zero_var(&el),
                        };
                        self.check_stored_struct(base, &v, e.span)?;
                        let nlen =
                            self.fit(Self::val(Ty::Uint(256), format!("(+ {len} 1)")), e.span)?;
                        let nb = Var {
                            ty: bv.ty.clone(),
                            t: format!("(store {} {len} {})", bv.t, v.t),
                            len: Some(nlen.t),
                        };
                        self.assign(base, nb)?;
                        Ok(vec![])
                    }
                    (Ty::Arr(..), "pop") => {
                        let len = match &bv.len {
                            Some(l) => l.clone(),
                            None => self.arr_len(&bv.ty),
                        };
                        self.panic_unless(
                            &format!("(> {len} 0)"),
                            "pop() on a possibly empty array",
                            e.span,
                        );
                        let nb = Var {
                            ty: bv.ty.clone(),
                            t: bv.t.clone(),
                            len: Some(format!("(- {len} 1)")),
                        };
                        self.assign(base, nb)?;
                        Ok(vec![])
                    }
                    (_, "transfer" | "send")
                        if argv.len() == 1 && !matches!(bv.ty, Ty::Contract(_)) =>
                    {
                        let v = self.eval(argv[0])?;
                        let to = bv.t.clone();
                        if self.in_library() {
                            return self.with_host(|s| s.transfer_send(m, &to, v));
                        }
                        self.transfer_send(m, &to, v)
                    }
                    (_, "call" | "delegatecall" | "staticcall") => {
                        let value = self.call_value.take();
                        for a in &argv {
                            // `new bytes(n)` only allocates memory: nothing to evaluate
                            if matches!(&a.kind, ExprKind::Call(c, _) if matches!(c.kind, ExprKind::New(_)))
                            {
                                continue;
                            }
                            self.eval(a)?;
                        }
                        // A low-level call may fail (returning false, moving nothing); on success
                        // its value leaves this contract before the callee runs.
                        let ok = self.decl("call_ok", &Ty::Bool);
                        if let Some(v) = value {
                            self.send_value(&v, Some(&ok));
                        }
                        let mut r = self.external_call(
                            vec![Ty::Bool, Ty::Opaque],
                            m == "staticcall",
                            e.span,
                        )?;
                        r[0] = Self::val(Ty::Bool, ok);
                        Ok(r)
                    }
                    (Ty::Contract(cn), _) => {
                        let value = self.call_value.take();
                        let mut avals = vec![];
                        for a in &argv {
                            avals.push(self.eval(a)?);
                        }
                        let target = self.prog.contract(cn).and_then(|c| {
                            c.funcs.iter().find(|f| f.name == m && f.params.len() == argv.len())
                        });
                        let (rets, st) = match target {
                            Some(f) => {
                                (f.rets.iter().map(|(_, t)| t.clone()).collect(), f.is_static())
                            }
                            None => (vec![Ty::Opaque], false),
                        };
                        // a high-level call with value reverts unless the value is sent
                        if let Some(v) = value {
                            self.send_value(&v, None);
                        }
                        // An interface's exported contract (its functions' requires/ensures, over
                        // their parameters and returns): callers are checked against it, and its
                        // ensures are trusted of whatever code answers.
                        let iface = target.filter(|f| {
                            (!f.requires.is_empty() || !f.ensures.is_empty())
                                && self
                                    .prog
                                    .contract(cn)
                                    .is_some_and(|c| c.kind.is_interface() || f.item.body.is_none())
                        });
                        if let Some(f) = iface {
                            self.iface_requires(f, &avals, e.span)?;
                        }
                        // Any external call may revert (the callee reverts, runs out of gas, or
                        // isn't a contract), unless a try catches it or the interface promises it
                        // returns under its succeeds-if.
                        if self.try_call != Some(e.span) {
                            let live = match target.filter(|f| !f.succeeds.is_empty()) {
                                Some(f) => Some(self.iface_succeeds(f, &avals)?),
                                None => None,
                            };
                            match live {
                                Some(p) => self.reverts_if(&format!("(not {p})")),
                                None => self.reverts_if("true"),
                            }
                        }
                        let mut out = self.external_call(rets, st, e.span)?;
                        if st && target.is_some() {
                            let tv = Var { ty: bv.ty.clone(), t: bv.t.clone(), len: None };
                            self.deterministic_view(cn, m, &tv, &avals, &mut out);
                        }
                        if let Some(f) = iface {
                            self.iface_ensures(f, &avals, &out, e.span)?;
                        }
                        Ok(out)
                    }
                    _ => Err((e.span, format!("unsupported call `.{m}(..)`"))),
                }
            }
            _ => {
                // a call through a function pointer some expression computes
                let v = self.eval(callee)?;
                if matches!(v.ty, Ty::Fn(_)) {
                    return self.call_ptr(e, v, &argv);
                }
                Err((e.span, "unsupported call".into()))
            }
        }
    }

    /// The internal function `n` names as a value (in library `lib`, or else in this contract
    /// or among the free functions): its tag, typed by its signature. None when `n` names no
    /// function; an error when it names several (an overload a value cannot pick here).
    fn fn_ref(&mut self, lib: Option<&'p Contract<'a>>, n: &str, span: Span) -> R<Option<Val>> {
        let fns = |c: &'p Contract<'a>| -> Vec<&'p Func<'a>> {
            c.funcs
                .iter()
                .filter(|f| f.name == n && !f.item.kind.is_modifier() && f.name != "constructor")
                .collect()
        };
        let mut found = None;
        let owners: Vec<&'p Contract<'a>> = match lib {
            Some(l) => vec![l],
            None => [Some(self.c), self.prog.contract(FREE)].into_iter().flatten().collect(),
        };
        for c in owners {
            let fs = fns(c);
            if fs.len() > 1 {
                return Err((span, format!("overloaded function `{n}` used as a value")));
            }
            if let Some(f) = fs.first() {
                found = Some((c, *f));
                break;
            }
        }
        let Some((c, f)) = found else { return Ok(None) };
        if self.spec {
            return Err((span, format!("function `{n}` used as a value in a spec")));
        }
        if f.item.header.visibility() == Some(ast::Visibility::External) {
            return Err((span, format!("external function `{n}` used as an internal value")));
        }
        let t = self.fn_tag(c, f);
        let sig = (
            f.params.iter().map(|p| p.1.clone()).collect(),
            f.rets.iter().map(|r| r.1.clone()).collect(),
        );
        Ok(Some(Self::val(Ty::Fn(Box::new(sig)), t)))
    }

    /// The tag of internal function `f` as a pointer value: a constant of its own, nonzero and
    /// distinct from every other function's tag. Tags are symbolic, so nothing a proof shows
    /// depends on how the compiled code numbers functions (code offsets, or ids).
    fn fn_tag(&mut self, c: &'p Contract<'a>, f: &'p Func<'a>) -> String {
        if let Some((_, _, t)) = self.fn_tags.iter().find(|(_, g, _)| std::ptr::eq(*g, f)) {
            return t.clone();
        }
        let t = format!("|fnptr.{}|", self.fn_tags.len() + 1);
        let mut decl = format!(
            "(declare-const {t} Int)
(assert (and (< 0 {t}) (< {t} {})))
",
            num(&pow2(256))
        );
        for (_, _, o) in &self.fn_tags {
            decl.push_str(&format!(
                "(assert (not (= {t} {o})))
"
            ));
        }
        self.smt.send(&decl);
        self.fn_tags.push((c, f, t.clone()));
        t
    }

    /// A call through internal function pointer `v`. The pointer holds 0 (unset: Solidity
    /// panics) or the tag of a function this VC referenced as a value, of the pointer's arity,
    /// which is an obligation: a value from anywhere else (arithmetic in assembly, storage)
    /// fails with a counterexample. Each target the path allows runs as an internal call; a
    /// target whose parameter or return types differ from the pointer's (a cast that
    /// reinterprets memory) is not modelled.
    fn call_ptr(&mut self, e: &'a Expr<'a>, v: Val, argv: &[&'a Expr<'a>]) -> R<Vec<Val>> {
        let Ty::Fn(sig) = v.ty.clone() else { unreachable!() };
        let (ptys, rtys) = *sig;
        if self.spec {
            return Err((e.span, "call through a function pointer in a spec".into()));
        }
        let vals = argv.iter().map(|a| self.eval(a)).collect::<R<Vec<_>>>()?;
        let cands: Vec<(&'p Contract<'a>, &'p Func<'a>, String)> = self
            .fn_tags
            .iter()
            .filter(|(_, f, _)| f.params.len() == ptys.len() && f.rets.len() == rtys.len())
            .cloned()
            .collect();
        let mut member = vec![format!("(= {} 0)", v.t)];
        member.extend(cands.iter().map(|(_, _, t)| format!("(= {} {t})", v.t)));
        let member = format!("(or {})", member.join(" "));
        self.must_hold(
            &member,
            "call through a function pointer that may hold no internal function of its type (one from storage, or computed in assembly)",
            e.span,
        )?;
        self.panic_unless(
            &format!("(not (= {} 0))", v.t),
            "call through an unset function pointer",
            e.span,
        );
        let zeros = |s: &Self| rtys.iter().map(|t| s.zero_var(t)).collect::<Vec<_>>();
        if self.guard == "false" {
            return Ok(zeros(self));
        }
        let mut live = vec![];
        for (c, f, t) in cands {
            let guard = self.guard.clone();
            if matches!(self.smt.check(&guard, &format!("(not (= {} {t}))", v.t), &[]), Res::Unsat)
            {
                continue;
            }
            let same = |a: &Ty, b: &Ty| {
                a == b || matches!((a, b), (Ty::Addr | Ty::Contract(_), Ty::Addr | Ty::Contract(_)))
            };
            for (i, ((pn, pt), at)) in f.params.iter().zip(&ptys).enumerate() {
                if !same(pt, at) {
                    return Err((
                        e.span,
                        format!(
                            "call through a function pointer of type {} to `{}`, whose parameter {} (`{pn}`) is {}: casts that reinterpret a value are not modelled",
                            Ty::Fn(Box::new((ptys.clone(), rtys.clone()))).name(),
                            f.name,
                            i + 1,
                            pt.name()
                        ),
                    ));
                }
            }
            for (i, ((_, rt), want)) in f.rets.iter().zip(&rtys).enumerate() {
                if !same(rt, want) {
                    return Err((
                        e.span,
                        format!(
                            "call through a function pointer of type {} to `{}`, whose return value {} is {}: casts that reinterpret a value are not modelled",
                            Ty::Fn(Box::new((ptys.clone(), rtys.clone()))).name(),
                            f.name,
                            i + 1,
                            rt.name()
                        ),
                    ));
                }
            }
            live.push((c, f, t));
        }
        let retype = |rets: Vec<Val>| -> Vec<Val> {
            rets.into_iter().zip(&rtys).map(|(r, t)| Var { ty: t.clone(), ..r }).collect()
        };
        match live.len() {
            0 => {
                // every path here calls an unset pointer and panicked
                self.guard = "false".into();
                Ok(zeros(self))
            }
            1 => {
                let (c, f, _) = live.pop().unwrap();
                let lib = c.kind == ast::ContractKind::Library;
                let rets = self.internal_call(c, f, vals, e.span, lib)?;
                self.write_back_storage_params(f, argv, e.span)?;
                Ok(retype(rets))
            }
            _ => {
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                let mut states = vec![];
                for (c, f, t) in live {
                    self.guard = g0.clone();
                    self.env = env0.clone();
                    self.assume(&format!("(= {} {t})", v.t));
                    let lib = c.kind == ast::ContractKind::Library;
                    let rets = self.internal_call(c, f, vals.clone(), e.span, lib)?;
                    self.write_back_storage_params(f, argv, e.span)?;
                    for (i, r) in rets.into_iter().enumerate() {
                        self.env.insert(format!("@fnret.{i}"), r);
                    }
                    states.push((self.guard.clone(), std::mem::take(&mut self.env)));
                }
                let (g, mut env) = self.merge(states);
                let mut rets = vec![];
                for (i, t) in rtys.iter().enumerate() {
                    let r = env.remove(&format!("@fnret.{i}")).unwrap_or_else(|| self.zero_var(t));
                    rets.push(r);
                }
                self.guard = g;
                self.env = env;
                Ok(retype(rets))
            }
        }
    }

    fn write_back_storage_params(
        &mut self,
        f: &'p Func<'a>,
        args: &[&'a Expr<'a>],
        span: Span,
    ) -> R<()> {
        if self.spec {
            return Ok(());
        }
        let outs = std::mem::take(&mut self.storage_param_out);
        let mut k = 0;
        for (i, p) in f.item.header.parameters.iter().enumerate() {
            if !Self::writes_back(p, &f.params[i].1) {
                continue;
            }
            let v =
                outs.get(k).cloned().ok_or((span, "internal: storage parameter".to_string()))?;
            k += 1;
            if p.data_location == Some(ast::DataLocation::Storage) {
                self.assign(args[i], v)?;
                continue;
            }
            // a memory object passed by reference: the caller's name for it sees the writes
            match &args[i].peel_parens().kind {
                ExprKind::Ident(_) | ExprKind::Member(..) | ExprKind::Index(..) => {
                    self.assign(args[i], v)?
                }
                // a fresh object: nothing of the caller names it
                ExprKind::New(_) | ExprKind::Array(_) => {}
                _ if !self.mem_param_written(f, &f.params[i].0) => {}
                _ => {
                    return Err((
                        args[i].span,
                        format!(
                            "memory argument `{}` of `{}` may alias a caller object the callee writes",
                            f.params[i].0, f.name
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Memory references are values in this encoding, so two names for one memory object would
    /// drift apart once either is written. Refuse a function that copies a memory reference into
    /// another name (`b = a`, `S memory s = xs[i]`, a call passing it that may return it) and
    /// then writes through either name.
    fn mem_alias_check(&self, f: &'p Func<'a>) -> R<()> {
        let Some(body) = &f.item.body else {
            return Ok(());
        };
        let mut mem: FxHashSet<String> = FxHashSet::default();
        for (p, (n, ty)) in f.item.header.parameters.iter().zip(&f.params) {
            if Self::mem_ref_param(p, ty) {
                mem.insert(n.clone());
            }
        }
        for (p, (n, ty)) in f.item.header.returns().iter().zip(&f.rets) {
            if Self::mem_ref_param(p, ty) {
                mem.insert(n.clone());
            }
        }
        for s in body.stmts.iter() {
            walk::all_stmts(s, &mut |s| {
                if let StmtKind::DeclSingle(v) = &s.kind
                    && v.data_location == Some(ast::DataLocation::Memory)
                    && let Some(n) = v.name
                {
                    mem.insert(n.as_str().to_string());
                }
            });
        }
        // the memory names an expression's value may share an object with
        let sources = |e: &'a Expr<'a>| -> Vec<String> {
            match &e.peel_parens().kind {
                ExprKind::Ident(_) | ExprKind::Member(..) | ExprKind::Index(..) => {
                    walk::root(e).into_iter().filter(|r| mem.contains(r)).collect()
                }
                ExprKind::Call(callee, args) if self.user_call(callee) => {
                    args.exprs().flat_map(walk::root).filter(|r| mem.contains(r)).collect()
                }
                ExprKind::Ternary(_, a, b) => walk::root(a)
                    .into_iter()
                    .chain(walk::root(b))
                    .filter(|r| mem.contains(r))
                    .collect(),
                _ => vec![],
            }
        };
        let mut pairs: Vec<(String, String, Span)> = vec![];
        for s in body.stmts.iter() {
            walk::all_stmts(s, &mut |s| {
                if let StmtKind::DeclSingle(v) = &s.kind
                    && let Some(n) = v.name
                    && mem.contains(n.as_str())
                    && let Some(i) = v.initializer.as_deref()
                {
                    for r in sources(i) {
                        pairs.push((n.as_str().to_string(), r, s.span));
                    }
                }
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| {
                        if let ExprKind::Assign(l, None, r) = &x.kind
                            && let ExprKind::Ident(n) = &l.peel_parens().kind
                            && mem.contains(n.as_str())
                        {
                            for src in sources(r) {
                                pairs.push((n.as_str().to_string(), src, x.span));
                            }
                        }
                    })
                });
            });
        }
        for (a, b, span) in pairs {
            if a == b {
                continue;
            }
            for n in [&a, &b] {
                if self.name_written(body, n) {
                    return Err((
                        span,
                        format!(
                            "memory reference `{a}` aliases `{b}` and `{n}` is written; aliased memory is not modelled"
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Is the object `n` names written in `body`: a write rooted at it, or any assembly?
    /// Passing it to a call counts too (the callee may write it), unless the call is
    /// the alias-creating one itself, which `mem_alias_check` already records.
    fn name_written(&self, body: &'a ast::Block<'a>, n: &str) -> bool {
        let mut out = BTreeSet::new();
        let mut hit = false;
        for s in body.stmts.iter() {
            walk::all_stmts(s, &mut |s| {
                if matches!(s.kind, StmtKind::Assembly(_)) {
                    hit = true;
                }
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| match &x.kind {
                        ExprKind::Assign(l, _, _) => out.extend(walk::root(l)),
                        ExprKind::Unary(op, y) if op.kind.has_side_effects() => {
                            out.extend(walk::root(y))
                        }
                        ExprKind::Delete(y) => out.extend(walk::root(y)),
                        ExprKind::Call(callee, args) if self.user_call(callee) => {
                            for a in args.exprs() {
                                if walk::root(a).iter().any(|r| r == n) {
                                    hit = true;
                                }
                            }
                            if let ExprKind::Member(b, _) = &callee.peel_parens().kind
                                && walk::root(b).iter().any(|r| r == n)
                            {
                                hit = true;
                            }
                        }
                        _ => {}
                    })
                });
            });
        }
        hit || out.contains(n)
    }

    /// A call that may run user code with a memory argument (not a builtin such as
    /// abi.encode, keccak256 or require).
    fn user_call(&self, callee: &'a Expr<'a>) -> bool {
        match &callee.peel_parens().kind {
            ExprKind::Ident(i) => self.c.funcs.iter().any(|f| f.name == i.as_str()),
            ExprKind::Member(b, m) => {
                let builtin = matches!(&b.peel_parens().kind, ExprKind::Ident(i)
                    if matches!(i.as_str(), "abi" | "type" | "string" | "bytes" | "block" | "msg" | "tx"));
                !builtin && !matches!(m.as_str(), "length" | "push" | "pop" | "concat")
            }
            _ => false,
        }
    }

    /// A parameter whose final value flows back to the caller: a storage pointer, or a memory
    /// reference (arrays, structs, bytes and strings pass by reference to internal functions).
    fn writes_back(p: &ast::VariableDefinition<'_>, ty: &Ty) -> bool {
        p.data_location == Some(ast::DataLocation::Storage) || Self::mem_ref_param(p, ty)
    }

    fn mem_ref_param(p: &ast::VariableDefinition<'_>, ty: &Ty) -> bool {
        p.data_location == Some(ast::DataLocation::Memory)
            && matches!(ty, Ty::Arr(..) | Ty::Struct(_) | Ty::Opaque)
    }

    /// May the body of `f` change the memory object its parameter `n` refers to? Syntactic and
    /// conservative: a write rooted at `n`, passing it on to a call, any assembly, or copying the
    /// reference into another variable (which may then write it) all count.
    fn mem_param_written(&self, f: &'p Func<'a>, n: &str) -> bool {
        let Some(body) = &f.item.body else {
            return true;
        };
        let is_n =
            |e: &Expr<'_>| matches!(&e.peel_parens().kind, ExprKind::Ident(i) if i.as_str() == n);
        let mut hit = false;
        let mut out = BTreeSet::new();
        let mut all = false;
        for s in body.stmts.iter() {
            walk::all_stmts(s, &mut |s| {
                match &s.kind {
                    StmtKind::Assembly(_) => hit = true,
                    StmtKind::DeclSingle(v) if v.initializer.as_deref().is_some_and(is_n) => {
                        hit = true
                    }
                    StmtKind::DeclMulti(_, e) if walk::root(e).iter().any(|r| r == n) => hit = true,
                    StmtKind::Return(Some(e)) if walk::root(e).iter().any(|r| r == n) => hit = true,
                    _ => {}
                }
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| {
                        self.effects(x, &mut out, &mut all, &mut FxHashSet::default());
                        match &x.kind {
                            ExprKind::Call(callee, args) => {
                                if args.exprs().any(is_n) {
                                    hit = true;
                                }
                                if let ExprKind::Member(b, _) = &callee.peel_parens().kind
                                    && is_n(b)
                                {
                                    hit = true;
                                }
                            }
                            ExprKind::Assign(_, _, r) if is_n(r) => hit = true,
                            ExprKind::Ternary(_, a, b) if is_n(a) || is_n(b) => hit = true,
                            _ => {}
                        }
                    })
                });
            });
        }
        hit || out.contains(n)
    }

    fn cast(&mut self, ty: Ty, v: Val) -> Val {
        if v.ty == Ty::Lit && !self.spec {
            return Self::val(ty, v.t);
        }
        match (&ty, &v.ty) {
            (Ty::Uint(n), Ty::Uint(m)) if n >= m => Self::val(ty, v.t),
            (Ty::Int(n), Ty::Int(m)) if n >= m => Self::val(ty, v.t),
            (Ty::Int(n), Ty::Uint(m)) if n > m => Self::val(ty, v.t),
            (Ty::Uint(_) | Ty::Int(_), Ty::Uint(_) | Ty::Int(_) | Ty::Lit) => {
                let (lo, hi) = ty.bounds().unwrap();
                let t = self.wrapr(&ty, &v.t, &lo, &hi);
                Self::val(ty, t)
            }
            _ => Self::val(ty, v.t),
        }
    }

    fn quant(&mut self, _e: &'a Expr<'a>, idx: usize, body: &'a Expr<'a>) -> R<Val> {
        let q = self.prog.quants[idx].clone();
        let mut saved = vec![];
        let mut decls = vec![];
        let mut ranges = vec![];
        for (tytxt, name) in &q.binders {
            let ty = parse_ty_text(tytxt)
                .ok_or((body.span, format!("unknown binder type `{tytxt}`")))?;
            self.n += 1;
            let sym = format!("|{name}!{}|", self.n);
            decls.push(format!("({sym} {})", ty.sort()));
            if let Some(r) = ty.range(&sym) {
                ranges.push(r);
            }
            saved
                .push((name.clone(), self.env.insert(name.clone(), Var { ty, t: sym, len: None })));
            self.bound.push(name.clone());
        }
        let b = self.eval_spec(body);
        for (name, old) in saved.into_iter().rev() {
            self.bound.pop();
            match old {
                Some(v) => self.env.insert(name, v),
                None => self.env.remove(&name),
            };
        }
        let b = b?;
        let rng =
            if ranges.is_empty() { "true".into() } else { format!("(and {})", ranges.join(" ")) };
        let t = if q.forall {
            format!("(forall ({}) (=> {rng} {}))", decls.join(" "), b.t)
        } else {
            format!("(exists ({}) (and {rng} {}))", decls.join(" "), b.t)
        };
        Ok(Self::val(Ty::Bool, t))
    }

    /// The lock condition in the current state: the guard variable, or the guard expression.
    fn lock_term(&mut self) -> Option<R<String>> {
        let c = self.c;
        let (g, _) = c.lock.as_ref()?;
        if let Some(sp) = &c.lock_expr {
            return Some(self.eval_spec(sp.expr).map(|v| v.t));
        }
        self.env.get(g).map(|v| Ok(v.t.clone()))
    }

    fn lock_held(&mut self) -> bool {
        let Some(Ok(t)) = self.lock_term() else {
            return false;
        };
        if self.guard == "false" {
            return true;
        }
        let guard = self.guard.clone();
        matches!(self.smt.check(&guard, &t, &[]), Res::Unsat)
    }

    /// State names (constants included) a guard expression mentions: an entry point that reaches
    /// one of them, through its modifiers and internal calls, is taken to take the lock.
    fn lock_names(&self) -> FxHashSet<String> {
        let mut out = FxHashSet::default();
        if let Some(sp) = &self.c.lock_expr {
            walk::sub_exprs(sp.expr, &mut |x| {
                if let ExprKind::Ident(i) = &x.kind
                    && self.c.state.iter().any(|s| s.name == i.as_str() && !s.ghost)
                {
                    out.insert(i.as_str().to_string());
                }
            });
        }
        out
    }

    /// Whether these statements mention one of `names`, directly or through modifiers and the
    /// internal functions (and `super` targets) they call.
    fn reaches_names(
        &self,
        stmts: Vec<&'a Stmt<'a>>,
        names: &FxHashSet<String>,
        seen: &mut FxHashSet<String>,
    ) -> bool {
        let mut hit = false;
        let mut calls: Vec<String> = vec![];
        for s in &stmts {
            walk::all_stmts(s, &mut |s| {
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| match &x.kind {
                        ExprKind::Ident(i) if names.contains(i.as_str()) => hit = true,
                        ExprKind::Call(c, _) => match &c.kind {
                            ExprKind::Ident(i) => calls.push(i.as_str().to_string()),
                            ExprKind::Member(b, m) if matches!(&b.kind, ExprKind::Ident(i) if i.name == sym::super_) => {
                                calls.push(m.as_str().to_string())
                            }
                            _ => {}
                        },
                        _ => {}
                    })
                });
            });
        }
        if hit {
            return true;
        }
        let c = self.c;
        for n in calls {
            for g in c.funcs.iter().chain(c.overridden.iter()).filter(|g| g.name == n) {
                let key = format!("{}.{}/{}", g.contract, g.name, g.params.len());
                if !seen.insert(key) {
                    continue;
                }
                if self.reaches_names(self.fn_stmts(g), names, seen) {
                    return true;
                }
            }
        }
        false
    }

    /// A function's body statements followed by its modifiers' bodies.
    fn fn_stmts(&self, f: &'p Func<'a>) -> Vec<&'a Stmt<'a>> {
        let mut stmts: Vec<&'a Stmt<'a>> = vec![];
        if let Some(b) = &f.item.body {
            stmts.extend(b.stmts.iter());
        }
        for m in f.item.header.modifiers.iter() {
            if let Some(md) = self.c.modifiers.get(m.name.last().as_str())
                && let Some(b) = &md.body
            {
                stmts.extend(b.stmts.iter());
            }
        }
        stmts
    }

    /// Obligation behind `@custom:reentrancy-guard`: with the lock held, this entry point reverts.
    pub(crate) fn verify_lock(&mut self, f: &'p Func<'a>) -> FnReport {
        let (g, gspan) = self.c.lock.clone().unwrap();
        self.smt.send(&format!(
            "(push 1)
; ===== lock {}.{}
",
            self.c.name, f.name
        ));
        self.clear_scope();
        self.smt.clear_error();
        self.fails.clear();
        self.checks = 1;
        self.broadcast_on.clear();
        let r = (|| -> R<()> {
            let lemmas = broadcast::active(self.prog, self.c, f)?;
            self.broadcast(lemmas, true)?;
            self.init_env(false)?;
            for (n, ty) in &f.params {
                let v = self.fresh_var(n, ty);
                self.env.insert(n.clone(), v);
            }
            for (n, ty) in &f.rets {
                let z = self.zero_var(ty);
                self.env.insert(n.clone(), z);
            }
            let lt = self.lock_term().unwrap_or_else(|| {
                Err((gspan, format!("reentrancy guard `{g}` is not a state variable")))
            })?;
            self.assume(&lt);
            // An entry point that never touches the lock can run under it (reentered from a call
            // the contract makes while locked): instead of reverting, it must keep every
            // `@custom:lock-stable` property, starting from a state satisfying the invariants.
            let open = self.lock_open(f);
            let c = self.c;
            let mut pre = vec![];
            if open {
                for inv in &c.invariants {
                    let v = self.eval_spec(inv.expr)?;
                    self.assume(&v.t);
                }
                for p in &c.lock_stable {
                    pre.push(self.eval_spec(p.expr)?.t);
                }
            }
            self.old = Some(self.env.clone());
            self.frames.push(Frame { rets: f.rets.clone(), exits: vec![] });
            self.exec_fn_body(f, 0)?;
            let fr = self.frames.pop().unwrap();
            let mut st = fr.exits;
            st.push((self.guard.clone(), self.env.clone()));
            let (gd, e) = self.merge(st);
            self.guard = gd;
            self.env = e;
            if open {
                self.checks = c.lock_stable.len();
                for (p, pre) in c.lock_stable.iter().zip(pre) {
                    let v = self.eval_spec(p.expr)?;
                    self.check(
                        &format!("(=> {pre} {})", v.t),
                        "lock-stable property not preserved by an entry point callable under the reentrancy lock",
                        p.span,
                        Some(p),
                    );
                }
            } else {
                self.check(
                    "false",
                    &format!("entry point does not revert while reentrancy guard `{g}` is held"),
                    f.item.header.span,
                    None,
                );
            }
            Ok(())
        })();
        self.smt.send(
            "(pop 1)
",
        );
        FnReport {
            name: format!("{}.{} [reentrancy guard]", self.c.name, f.name),
            checks: self.checks,
            fails: std::mem::take(&mut self.fails),
            unsupported: r.err(),
            warnings: vec![],
            inferred: vec![],
        }
    }

    /// Debit `v` wei from this contract (EVM CALL moves value before the callee runs). With
    /// `ok`, only when the call succeeds, and success implies the balance covered it.
    fn send_value(&mut self, v: &str, ok: Option<&str>) {
        if self.in_library() {
            let v = v.to_string();
            let ok = ok.map(|o| o.to_string());
            return self.with_host(|s| s.send_value(&v, ok.as_deref()));
        }
        let (Some(bal), Some(this)) =
            (self.env.get("__balance").cloned(), self.env.get("this").cloned())
        else {
            return;
        };
        let enough = format!("(>= (select {} {}) {v})", bal.t, this.t);
        let debited = format!("(store {} {} (- (select {} {}) {v}))", bal.t, this.t, bal.t, this.t);
        let nb = match ok {
            Some(ok) => {
                self.assume(&format!("(=> {ok} {enough})"));
                format!("(ite {ok} {debited} {})", bal.t)
            }
            None => {
                self.assume(&enough);
                debited
            }
        };
        self.bind("__balance", bal.ty.clone(), Self::val(bal.ty, nb));
    }

    /// An entry point that can run while the reentrancy lock is held: its body and modifiers never
    /// mention the lock variable. Every other non-view entry point must be proven to revert under
    /// the lock (verify_lock).
    /// An internal function without its own contract that some function of the contract calls:
    /// it is inlined, and so checked, at every call site; a standalone run would start from an
    /// arbitrary state no caller produces.
    pub(crate) fn inlined_only(&self, f: &'p Func<'a>) -> bool {
        if f.is_entry() || f.has_contract() || f.name == "constructor" {
            return false;
        }
        let mut called = false;
        let bodies = self
            .c
            .funcs
            .iter()
            .filter_map(|g| g.item.body.as_ref())
            .chain(self.c.modifiers.values().filter_map(|m| m.body.as_ref()));
        for b in bodies {
            for s in b.stmts.iter() {
                walk::all_stmts(s, &mut |s| {
                    walk::stmt_exprs(s, &mut |e| {
                        walk::sub_exprs(e, &mut |x| {
                            if let ExprKind::Call(c, _) = &x.kind
                                && matches!(&c.kind, ExprKind::Ident(i) if i.as_str() == f.name)
                            {
                                called = true;
                            }
                        })
                    });
                });
            }
        }
        called
    }

    pub(crate) fn lock_open(&self, f: &'p Func<'a>) -> bool {
        let Some((g, _)) = &self.c.lock else {
            return false;
        };
        if !f.is_entry() || f.is_static() || f.name == "constructor" {
            return false;
        }
        if self.c.lock_expr.is_some() {
            let names = self.lock_names();
            return !self.reaches_names(self.fn_stmts(f), &names, &mut FxHashSet::default());
        }
        // the plain-variable form: an entry point is under the lock when its body or modifiers
        // reach the variable, directly or through internal calls (such as _lock() in a modifier);
        // every such entry point is then proven to revert while the lock is held (verify_lock)
        let names: FxHashSet<String> = [g.clone()].into_iter().collect();
        !self.reaches_names(self.fn_stmts(f), &names, &mut FxHashSet::default())
    }

    /// addr.transfer(v) / addr.send(v): moves v wei out of this contract (2300 gas: no reentry).
    fn transfer_send(&mut self, m: &str, to: &str, v: Val) -> R<Vec<Val>> {
        let (Some(bal), Some(this)) =
            (self.env.get("__balance").cloned(), self.env.get("this").cloned())
        else {
            if m == "send" {
                let t = self.decl("send", &Ty::Bool);
                return Ok(vec![Self::val(Ty::Bool, t)]);
            }
            return Ok(vec![]);
        };
        let enough = format!("(>= (select {} {}) {})", bal.t, this.t, v.t);
        let debited =
            format!("(store {} {} (- (select {} {}) {}))", bal.t, this.t, bal.t, this.t, v.t);
        let moved = format!("(store {debited} {} (+ (select {debited} {}) {}))", to, to, v.t);
        if m == "transfer" {
            self.reverts_if(&format!("(not {enough})"));
            // The recipient's code may revert, which reverts the transfer.
            self.reverts_if("true");
            self.assume(&enough);
            self.bind("__balance", bal.ty.clone(), Self::val(bal.ty, moved));
            Ok(vec![])
        } else {
            let ok = self.decl("send", &Ty::Bool);
            self.assume(&format!("(=> {ok} {enough})"));
            self.bind(
                "__balance",
                bal.ty.clone(),
                Self::val(bal.ty.clone(), format!("(ite {ok} {moved} {})", bal.t)),
            );
            Ok(vec![Self::val(Ty::Bool, ok)])
        }
    }

    fn in_library(&self) -> bool {
        self.c.kind == ast::ContractKind::Library && self.host.kind != ast::ContractKind::Library
    }

    /// Run f in the verified contract's own scope (its state under its own names, its invariants
    /// and lock), from inside library code, and carry its state changes back.
    fn with_host<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let host = self.host;
        let saved_c = std::mem::replace(&mut self.c, host);
        let saved = std::mem::take(&mut self.env);
        let saved_aliases = std::mem::take(&mut self.aliases);
        let saved_slocals = std::mem::take(&mut self.slocals);
        let saved_alias_env = std::mem::take(&mut self.alias_env);
        for (k, v) in &saved {
            if let Some(n) = k.strip_prefix("@host.") {
                self.env.insert(n.to_string(), v.clone());
            } else if k.contains('.') || k == "this" {
                self.env.insert(k.clone(), v.clone());
            }
        }
        let r = f(self);
        let inner = std::mem::replace(&mut self.env, saved);
        for (k, v) in inner {
            let hk = format!("@host.{k}");
            if self.env.contains_key(&hk) {
                self.env.insert(hk, v);
            } else if k.contains('.') && self.env.contains_key(&k) {
                self.env.insert(k, v);
            }
        }
        self.aliases = saved_aliases;
        self.slocals = saved_slocals;
        self.alias_env = saved_alias_env;
        self.c = saved_c;
        r
    }

    fn external_call(&mut self, rets: Vec<Ty>, is_static: bool, span: Span) -> R<Vec<Val>> {
        if self.in_library() {
            return self.with_host(|s| s.external_call(rets, is_static, span));
        }
        self.call_value = None;
        if !is_static && let Some(ep) = self.env.get("__code_epoch").cloned() {
            let ne = self.fresh_var("__code_epoch", &ep.ty);
            self.bind("__code_epoch", ep.ty.clone(), ne);
        }
        let locked = !self.spec && !is_static && self.lock_held();
        // Entry points the callee can still reach under the lock: the invariants must hold before
        // the call, and afterwards everything those entry points may write is unknown (each of
        // them preserves the invariants, which are proved separately).
        let open: Vec<&'p Func<'a>> = if locked {
            self.c.funcs.iter().filter(|f| self.lock_open(f)).collect()
        } else {
            vec![]
        };
        let mut stable_pre = vec![];
        if locked && !open.is_empty() {
            let c = self.c;
            for p in &c.lock_stable {
                stable_pre.push(self.eval_spec(p.expr)?.t);
            }
            for inv in &c.invariants {
                let v = self.eval_spec(inv.expr)?;
                self.check_inv(&v.t, "contract invariant must hold before an external call (entry points without the reentrancy lock can still run)", span, inv);
            }
        }
        if locked {
            // Under a reentrancy lock the callee cannot run our code, but it can still force ETH
            // on us (selfdestruct), and other accounts' balances change arbitrarily: our balance
            // can only grow.
            if let (Some(bal), Some(this)) =
                (self.env.get("__balance").cloned(), self.env.get("this").cloned())
            {
                let nb = self.fresh_var("__balance", &bal.ty);
                self.assume(&format!(
                    "(>= (select {} {}) (select {} {}))",
                    nb.t, this.t, bal.t, this.t
                ));
                self.bind("__balance", bal.ty.clone(), nb);
            }
            if !open.is_empty() {
                let mut seen = FxHashSet::default();
                let mut names: Option<BTreeSet<String>> = Some(BTreeSet::new());
                self.lock_modset.set(true);
                for f in &open {
                    match (self.modset(f, &mut seen), names.as_mut()) {
                        (Some(m), Some(n)) => n.extend(m),
                        _ => names = None,
                    }
                }
                self.lock_modset.set(false);
                let c = self.c;
                for sv in
                    c.state.iter().filter(|s| !s.constant && !s.immutable && s.name != "__time")
                {
                    if names.as_ref().is_none_or(|n| n.contains(&sv.name)) {
                        let v = self.fresh_var(&sv.name, &sv.ty);
                        self.env.insert(sv.name.clone(), v);
                    }
                }
                for inv in &c.invariants {
                    let v = self.eval_spec(inv.expr)?;
                    self.assume(&v.t);
                }
                for (p, pre) in c.lock_stable.iter().zip(&stable_pre) {
                    let v = self.eval_spec(p.expr)?;
                    self.assume(&format!("(=> {pre} {})", v.t));
                }
            }
        }
        if !self.spec && !is_static && !locked {
            let c = self.c;
            for inv in &c.invariants {
                let v = self.eval_spec(inv.expr)?;
                self.check_inv(
                    &v.t,
                    "contract invariant must hold before an external call (reentrancy)",
                    span,
                    inv,
                );
            }
            // reentrant calls run in this transaction, at this block.timestamp: __time is unchanged
            for sv in c.state.iter().filter(|s| !s.constant && !s.immutable && s.name != "__time") {
                let v = self.fresh_var(&sv.name, &sv.ty);
                self.env.insert(sv.name.clone(), v);
            }
            for inv in &c.invariants {
                let v = self.eval_spec(inv.expr)?;
                self.assume(&v.t);
            }
        }
        Ok(rets.iter().map(|t| self.fresh_var("ext_ret", t)).collect())
    }

    /// A view call's results as a function of the call (contract, function, target and
    /// arguments) and of the world it reads: this contract's state, raw storage, ETH balances
    /// and the code epoch (which outside code moves). Two view calls in the same world, in the
    /// body or in a spec, then answer alike; any write between them changes the world value.
    fn deterministic_view(
        &mut self,
        cn: &str,
        m: &str,
        target: &Var,
        args: &[Val],
        out: &mut [Val],
    ) {
        let scalar = |ty: &Ty| {
            matches!(ty, Ty::Uint(_) | Ty::Int(_) | Ty::Bool | Ty::Addr | Ty::Contract(_))
        };
        if self.in_library()
            || !self.bound.is_empty()
            || !scalar(&target.ty)
            || !args.iter().all(|a| scalar(&a.ty))
            || !out.iter().all(|o| scalar(&o.ty))
        {
            return;
        }
        let mut names: Vec<String> = self
            .c
            .state
            .iter()
            .filter(|s| {
                !s.constant
                    && !s.immutable
                    && (!s.ghost || s.name == "__balance" || s.name == "__code_epoch")
            })
            .map(|s| s.name.clone())
            .collect();
        names.push("__raw".into());
        names.push("__traw".into());
        let world: Vec<Var> = names.iter().filter_map(|n| self.env.get(n).cloned()).collect();
        if !self.env.contains_key("__code_epoch") {
            return;
        }
        let sorts: Vec<String> = world.iter().map(|v| v.ty.sort()).collect();
        // The sorts name the function, so one contract's world has one function. A struct's sort is
        // a quoted symbol, whose bars can't appear inside the quoted name.
        let wfun = format!("|sv_world {}|", sorts.join(" ").replace('|', ""));
        if self.declared_funs.insert(wfun.clone()) {
            self.smt.send(&format!("(declare-fun {wfun} ({}) Int)\n", sorts.join(" ")));
        }
        let w = if world.is_empty() {
            "0".to_string()
        } else {
            format!("({wfun} {})", world.iter().map(|v| v.t.as_str()).collect::<Vec<_>>().join(" "))
        };
        let mut asorts = vec![target.ty.sort()];
        asorts.extend(args.iter().map(|a| a.ty.sort()));
        asorts.push("Int".into());
        let mut avs = vec![target.t.clone()];
        avs.extend(args.iter().map(|a| a.t.clone()));
        avs.push(w);
        for (i, o) in out.iter_mut().enumerate() {
            let f = format!("|sv_view {cn}.{m}/{} {i}|", args.len());
            if self.declared_funs.insert(format!("{f}:{}", asorts.join(","))) {
                self.smt.send(&format!(
                    "(declare-fun {f} ({}) {})\n",
                    asorts.join(" "),
                    o.ty.sort()
                ));
            }
            let term = format!("({f} {})", avs.join(" "));
            self.smt.send(&format!("(assert (= {} {term}))\n", o.t));
        }
    }

    /// The declared named type of an expression, where it is visible syntactically: a call
    /// whose target declares a named return type (`slot.asBoolean()`, `f(x)`) or `T.wrap(x)`.
    /// User-defined value types share their underlying type in `Ty`, so this is what tells
    /// overloads such as TransientSlot's `tload(BooleanSlot)` and `tload(Int256Slot)` apart.
    fn nominal(&self, e: &'a Expr<'a>) -> Option<String> {
        let ExprKind::Call(callee, args) = &e.peel_parens().kind else {
            return None;
        };
        match &callee.peel_parens().kind {
            ExprKind::Member(b, m) => {
                let m = m.as_str();
                if let ExprKind::Ident(bn) = &b.peel_parens().kind
                    && m == "wrap"
                    && self.prog.udvts.contains_key(bn.as_str())
                {
                    return Some(bn.as_str().to_string());
                }
                let n = args.exprs().count();
                let cands = self.prog.using_fns(m, n);
                let want = self.nominal(b);
                let f = cands
                    .iter()
                    .find(|(_, f)| {
                        want.is_some()
                            && f.item.header.parameters.first().and_then(|p| decl_type_name(&p.ty))
                                == want
                    })
                    .or(cands.first())?
                    .1;
                f.item.header.returns().first().and_then(|p| decl_type_name(&p.ty))
            }
            ExprKind::Ident(n) => {
                let f = self.c.funcs.iter().find(|f| f.name == n.as_str())?;
                f.item.header.returns().first().and_then(|p| decl_type_name(&p.ty))
            }
            _ => None,
        }
    }

    /// Resolve `base.m(argv)` to a `using L for T` library function: by the receiver's declared
    /// named type when it is known, otherwise by argument types.
    #[allow(clippy::type_complexity)]
    fn pick_using(
        &mut self,
        base: &'a Expr<'a>,
        m: &str,
        argv: &[&'a Expr<'a>],
    ) -> R<Option<(&'p Contract<'a>, &'p Func<'a>, Vec<Val>, Vec<&'a Expr<'a>>)>> {
        let cands = self.prog.using_fns(m, argv.len());
        if cands.is_empty() {
            return Ok(None);
        }
        let mut all: Vec<&'a Expr<'a>> = vec![base];
        all.extend(argv.iter().copied());
        let vals = all.iter().map(|a| self.eval(a)).collect::<R<Vec<_>>>()?;
        let (lib, f) = if cands.len() == 1 {
            cands[0]
        } else {
            let want = self.nominal(base);
            let named = want.as_ref().and_then(|w| {
                cands.iter().copied().find(|(_, f)| {
                    f.item.header.parameters.first().and_then(|p| decl_type_name(&p.ty)).as_ref()
                        == Some(w)
                })
            });
            match named {
                Some(x) => x,
                None => {
                    let fs: Vec<&'p Func<'a>> = cands.iter().map(|(_, f)| *f).collect();
                    let f = pick_in(fs, &vals).unwrap_or(cands[0].1);
                    *cands.iter().find(|(_, g)| std::ptr::eq(*g, f)).unwrap()
                }
            }
        };
        if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
            eprintln!(
                "using {m}: {} candidates, receiver {:?}, picked {}({:?})",
                cands.len(),
                self.nominal(base),
                f.name,
                f.item.header.parameters.first().and_then(|p| decl_type_name(&p.ty))
            );
        }
        Ok(Some((lib, f, vals, all)))
    }

    fn internal_call(
        &mut self,
        c: &'p Contract<'a>,
        f: &'p Func<'a>,
        args: Vec<Val>,
        span: Span,
        library: bool,
    ) -> R<Vec<Val>> {
        if self.spec && f.opaque && self.reveal.as_deref() != Some(f.name.as_str()) {
            let ret = f
                .rets
                .first()
                .map(|r| r.1.clone())
                .ok_or((span, format!("opaque `{}` needs one return value", f.name)))?;
            let fname = format!("|op.{}.{}/{}|", c.name, f.name, f.params.len());
            if self.declared_funs.insert(fname.clone()) {
                let a: Vec<String> = f.params.iter().map(|p| p.1.sort()).collect();
                self.smt.send(&format!(
                    "(declare-fun {fname} ({}) {})
",
                    a.join(" "),
                    ret.sort()
                ));
                self.ret_bound(&fname, &ret);
            }
            let t = if args.is_empty() {
                fname
            } else {
                format!(
                    "({fname} {})",
                    args.iter().map(|a| a.t.clone()).collect::<Vec<_>>().join(" ")
                )
            };
            if self.bound.is_empty()
                && let Some(r) = ret.range(&t)
            {
                self.smt.send(&format!(
                    "(assert {r})
"
                ));
            }
            return Ok(vec![Self::val(ret, t)]);
        }
        if self.spec && f.opaque {
            // revealing this application: unfold once, nested uses stay opaque
            self.reveal = None;
        }
        if self.spec {
            let Some(body) = f.spec_body else {
                // A pure function is deterministic, so in specs it can stand for an uninterpreted
                // function of its arguments (equal inputs, equal outputs; nothing else is known).
                let pure = matches!(f.item.header.state_mutability(), ast::StateMutability::Pure);
                let ret = f.rets.first().map(|r| r.1.clone());
                let scalar = |t: &Ty| t.sort() == "Int" || t.sort() == "Bool";
                if pure
                    && f.rets.len() == 1
                    && f.params.iter().all(|p| scalar(&p.1))
                    && ret.as_ref().is_some_and(scalar)
                {
                    let ret = ret.unwrap();
                    let sig: Vec<String> = f.params.iter().map(|p| p.1.name()).collect();
                    let fname = format!("|fn.{}.{}({})|", c.name, f.name, sig.join(","));
                    let key = format!("{fname}/{}", args.len());
                    if self.declared_funs.insert(key) {
                        let a: Vec<String> = f.params.iter().map(|p| p.1.sort()).collect();
                        self.smt.send(&format!(
                            "(declare-fun {fname} ({}) {})\n",
                            a.join(" "),
                            ret.sort()
                        ));
                        self.ret_bound(&fname, &ret);
                    }
                    let t = if args.is_empty() {
                        fname
                    } else {
                        format!(
                            "({fname} {})",
                            args.iter().map(|a| a.t.clone()).collect::<Vec<_>>().join(" ")
                        )
                    };
                    if self.bound.is_empty()
                        && let Some(r) = ret.range(&t)
                    {
                        self.smt.send(&format!("(assert {r})\n"));
                    }
                    return Ok(vec![Self::val(ret, t)]);
                }
                return self.spec_inline(c, f, &args, span);
            };
            let saved = self.env.clone();
            for ((n, ty), a) in f.params.iter().zip(&args) {
                self.env
                    .insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
            }
            let r = self.eval(body);
            self.env = saved;
            let mut v = r?;
            if let Some((_, ty)) = f.rets.first() {
                v.ty = ty.clone();
            }
            return Ok(vec![v]);
        }
        let saved_env = std::mem::take(&mut self.env);
        let saved_aliases = std::mem::take(&mut self.aliases);
        let saved_slocals = std::mem::take(&mut self.slocals);
        let saved_alias_env = std::mem::take(&mut self.alias_env);
        let saved_raw_enums = std::mem::take(&mut self.raw_enums);
        let saved_old = self.old.take();
        let saved_c = std::mem::replace(&mut self.c, c);
        let caller_lib = saved_c.kind == ast::ContractKind::Library;
        for (k, v) in &saved_env {
            if k.contains('.') || k == "this" || (!library && c.state.iter().any(|s| &s.name == k))
            {
                self.env.insert(k.clone(), v.clone());
            } else if library && !caller_lib && saved_c.state.iter().any(|s| &s.name == k) {
                // the caller's state stays live, out of reach of the library's names
                self.env.insert(format!("@host.{k}"), v.clone());
            }
        }
        for ((n, ty), a) in f.params.iter().zip(&args) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
        }
        let result = (|| -> R<Vec<Val>> {
            if f.has_contract() {
                for r in &f.requires {
                    let v = self.eval_spec(r.expr)?;
                    self.check(
                        &v.t,
                        &format!("precondition of `{}` not satisfied", f.name),
                        span,
                        Some(r),
                    );
                }
                // Its body is not executed here, so whether it reverts is known only through
                // its succeeds-if: without one, the call may revert. A callee that can call back
                // into the function being proved would make that proof rest on itself (and the
                // recursion need not end), so such a call may revert too.
                if f.succeeds.is_empty() || self.reaches_current(f) {
                    self.reverts_if("true");
                } else {
                    let mut ps = vec![];
                    for sp in &f.succeeds {
                        ps.push(self.eval_spec(sp.expr)?.t);
                    }
                    let p = if ps.len() == 1 {
                        ps.pop().unwrap()
                    } else {
                        format!("(and {})", ps.join(" "))
                    };
                    self.reverts_if(&format!("(not {p})"));
                }
                // its runtime checks: the call reverts unless they hold, and then they are facts
                for ck in &f.checks {
                    self.runtime_check(ck)?;
                }
                self.old = Some(self.env.clone());
                let under_lock = if library { None } else { self.modset_under_lock(f) };
                let keep_invs = under_lock.as_ref().and_then(|(_, own)| own.clone());
                if let Some(ms) = under_lock
                    .map(|(ms, _)| Some(ms))
                    .unwrap_or_else(|| self.modset(f, &mut FxHashSet::default()))
                    .map(Some)
                    .unwrap_or_else(|| {
                        Some(
                            c.state
                                .iter()
                                .filter(|s| !s.constant && !s.immutable)
                                .map(|s| s.name.clone())
                                .collect(),
                        )
                    })
                {
                    for n in ms {
                        if let Some(sv) = c.state.iter().find(|s| s.name == n) {
                            let v = self.fresh_var(&n, &sv.ty);
                            self.env.insert(n, v);
                        } else if let Some(hv) = self.env.get(&format!("@host.{n}")).cloned() {
                            // raw/transient storage a library writes with sstore/tstore
                            let v = self.fresh_var(&n, &hv.ty);
                            self.env.insert(format!("@host.{n}"), v);
                        }
                    }
                }
                if let Some(own) = keep_invs {
                    let c = self.c;
                    for inv in &c.invariants {
                        if self.spec_state_names(inv.expr).iter().all(|n| !own.contains(n)) {
                            let v = self.eval_spec(inv.expr)?;
                            self.assume(&v.t);
                        }
                    }
                }
                if library && !f.trusted && self.modset(f, &mut FxHashSet::default()).is_none() {
                    // a library function that may call out: everything of the caller may change
                    // (a @custom:trusted library function is trusted not to reenter: like a
                    // summary, it touches the caller only through its storage parameters)
                    let keys: Vec<(String, Ty)> = self
                        .env
                        .iter()
                        .filter(|(k, _)| k.starts_with("@host.") && *k != "@host.__time")
                        .map(|(k, v)| (k.clone(), v.ty.clone()))
                        .collect();
                    for (k, ty) in keys {
                        let v = self.fresh_var(&k[6..], &ty);
                        self.env.insert(k, v);
                    }
                }
                if !f.is_static() {
                    for (i, p) in f.item.header.parameters.iter().enumerate() {
                        if p.data_location == Some(ast::DataLocation::Storage) {
                            let (n, ty) = &f.params[i];
                            let v = self.fresh_var(n, ty);
                            self.env.insert(n.clone(), v);
                        }
                    }
                }
                // memory reference parameters the body may write: the caller's object takes the
                // callee's final value, known only through the ensures (old(p) is the value
                // before the call)
                for (i, p) in f.item.header.parameters.iter().enumerate() {
                    let (n, ty) = &f.params[i];
                    if Self::mem_ref_param(p, ty) && self.mem_param_written(f, n) {
                        let v = self.fresh_var(n, ty);
                        self.env.insert(n.clone(), v);
                    }
                }
                for (n, ty) in &f.rets {
                    let v = self.fresh_var(n, ty);
                    self.env.insert(n.clone(), v);
                }
                let saved_rets = std::mem::replace(
                    &mut self.rets,
                    f.rets.iter().map(|(n, _)| n.clone()).collect(),
                );
                for en in &f.ensures {
                    let v = self.eval_spec(en.expr);
                    if v.is_err() {
                        self.rets = saved_rets.clone();
                    }
                    let v = v?;
                    self.assume(&v.t);
                }
                self.rets = saved_rets;
            } else {
                if self.depth > 8 {
                    return Err((
                        span,
                        format!("recursive call to `{}` needs @custom:requires/ensures", f.name),
                    ));
                }
                self.depth += 1;
                self.old = Some(self.env.clone());
                for (n, ty) in &f.rets {
                    let z = self.zero_var(ty);
                    self.env.insert(n.clone(), z);
                }
                for (p, (n, _)) in f.item.header.returns().iter().zip(&f.rets) {
                    if p.data_location == Some(ast::DataLocation::Storage) {
                        self.slocals.insert(n.clone());
                    }
                }
                self.frames.push(Frame { rets: f.rets.clone(), exits: vec![] });
                let saved_ph = std::mem::take(&mut self.ph);
                let saved_loops = std::mem::take(&mut self.loops);
                // An unchecked block covers only its own statements, not the functions it calls.
                let saved_unchecked = std::mem::replace(&mut self.unchecked, false);
                let r = self.exec_fn_body(f, 0);
                self.unchecked = saved_unchecked;
                self.ph = saved_ph;
                self.loops = saved_loops;
                let fr = self.frames.pop().unwrap();
                self.depth -= 1;
                r?;
                let mut st = fr.exits;
                st.push((self.guard.clone(), self.env.clone()));
                let (g, e) = self.merge(st);
                self.guard = g;
                self.env = e;
            }
            let mut out: Vec<Val> = vec![];
            let mut ra = None;
            for (p, (n, _)) in f.item.header.returns().iter().zip(&f.rets) {
                if p.data_location != Some(ast::DataLocation::Storage) {
                    continue;
                }
                if let Some(t) = self.aliases.get(n).copied() {
                    // capture the callee's own names the location is built from
                    let mut cap = self.alias_env.get(n).cloned().unwrap_or_default();
                    let mut ids = vec![];
                    walk::sub_exprs(t, &mut |x| {
                        if let ExprKind::Ident(i) = &x.kind {
                            ids.push(i.as_str().to_string());
                        }
                    });
                    let mut ok = true;
                    for i in ids {
                        if cap.contains_key(&i) || c.state.iter().any(|s| s.name == i) {
                            continue;
                        }
                        if self.aliases.contains_key(&i) {
                            ok = false;
                        } else if let Some(v) = self.env.get(&i) {
                            cap.insert(i, v.clone());
                        }
                    }
                    if ok && !library {
                        ra = Some((t, cap));
                    }
                }
                break;
            }
            self.ret_alias_out = ra;
            for (n, _) in &f.rets {
                self.check_raw_enum(n, span)?;
            }
            for (n, _) in &f.rets {
                match self.aliases.get(n).copied() {
                    // a storage return variable pointed somewhere: its current value
                    Some(t) => out.push(self.eval(t)?),
                    None => out.push(
                        self.env
                            .get(n)
                            .cloned()
                            .ok_or((span, format!("return value `{n}` is unset")))?,
                    ),
                }
            }
            for (i, p) in f.item.header.parameters.iter().enumerate() {
                if Self::writes_back(p, &f.params[i].1) {
                    out.push(self.env[&f.params[i].0].clone());
                }
            }
            Ok(out)
        })();
        let callee_env = std::mem::replace(&mut self.env, saved_env);
        self.raw_enums = saved_raw_enums;
        self.aliases = saved_aliases;
        self.slocals = saved_slocals;
        self.alias_env = saved_alias_env;
        self.old = saved_old;
        self.c = saved_c;
        let mut rets = result?;
        self.ret_alias = self.ret_alias_out.take();
        self.storage_param_out = rets.split_off(f.rets.len());
        for (k, v) in callee_env {
            if let Some(n) = k.strip_prefix("@host.").filter(|_| !caller_lib) {
                if self.env.contains_key(n) {
                    self.env.insert(n.to_string(), v);
                }
            } else if self.env.contains_key(&k)
                && (k.contains('.') || (!library && c.state.iter().any(|s| s.name == k)))
            {
                self.env.insert(k, v);
            }
        }
        Ok(rets)
    }

    /// The length of untracked bytes t: a function of the value, as .length reads it.
    fn opaque_len(&mut self, t: &str) -> String {
        self.declare_fun("sv_blen", 1, "Int");
        let l = format!("(sv_blen {t})");
        if self.bound.is_empty() {
            self.smt.send(&format!("(assert {})\n", Ty::Uint(256).range(&l).unwrap()));
        }
        l
    }

    /// A memory allocation of n items of size bytes panics when the free memory pointer would
    /// pass 2^64 (it starts below 2^32, see the assembly model).
    fn alloc_limit(&mut self, n: &str, size: u32) {
        let limit = pow2(64) - pow2(32) - BigInt::from(64);
        let ok = format!("(<= (* {size} {n}) {})", num(&limit));
        self.reverts_if(&format!("(not {ok})"));
        self.assume(&ok);
    }

    /// Whether internal function f is, or can call back into, the function being verified.
    /// Calls are matched by name alone, over every contract's functions and the modifiers they
    /// use, which can only find more paths.
    fn reaches_current(&self, f: &'p Func<'a>) -> bool {
        let Some((_, cf)) = self.cur_fn.clone() else { return false };
        f.name == cf || self.calls_reach(self.fn_stmts(f), &cf, &mut FxHashSet::default())
    }

    fn calls_reach(
        &self,
        stmts: Vec<&'a Stmt<'a>>,
        target: &str,
        seen: &mut FxHashSet<String>,
    ) -> bool {
        let mut calls: Vec<String> = vec![];
        for s in &stmts {
            walk::all_stmts(s, &mut |s| {
                walk::stmt_exprs(s, &mut |e| {
                    walk::sub_exprs(e, &mut |x| {
                        if let ExprKind::Call(c, _) = &x.kind {
                            match &c.kind {
                                ExprKind::Ident(i) => calls.push(i.as_str().to_string()),
                                ExprKind::Member(_, m) => calls.push(m.as_str().to_string()),
                                _ => {}
                            }
                        }
                    })
                });
            });
        }
        if calls.iter().any(|n| n == target) {
            return true;
        }
        for n in calls {
            if !seen.insert(n.clone()) {
                continue;
            }
            for c in &self.prog.contracts {
                for g in c.funcs.iter().chain(c.overridden.iter()).filter(|g| g.name == n) {
                    if self.calls_reach(self.fn_stmts(g), target, seen) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn wrap(_ty: &Ty, t: &str, lo: &BigInt, hi: &BigInt) -> String {
    let m = hi - lo + 1;
    if lo == &BigInt::from(0) {
        format!("(mod {t} {})", num(&m))
    } else {
        format!("(- (mod (+ {t} {}) {}) {})", num(&-lo), num(&m), num(&-lo))
    }
}

fn enum_index(p: &Program<'_>, e: &str, m: &str) -> Option<usize> {
    p.enum_variants.get(&format!("{e}.{m}")).copied()
}

pub(crate) fn ty_of<'a>(p: &Program<'a>, t: &'a ast::Type<'a>) -> Ty {
    use ast::{ElementaryType as E, TypeKind as K};
    match &t.kind {
        K::Elementary(e) => match e {
            E::Bool => Ty::Bool,
            E::Address(_) => Ty::Addr,
            E::UInt(n) => Ty::Uint(n.bits()),
            E::Int(n) => Ty::Int(n.bits()),
            E::FixedBytes(n) => Ty::Uint(n.bytes() as u16 * 8),
            _ => Ty::Opaque,
        },
        K::Mapping(m) => Ty::Map(Box::new(ty_of(p, &m.key)), Box::new(ty_of(p, &m.value))),
        K::Array(a) => Ty::Arr(
            Box::new(ty_of(p, &a.element)),
            a.size.as_deref().and_then(|e| crate::prog::array_size(&p.const_sizes, e)),
        ),
        K::Function(f) if f.visibility() != Some(ast::Visibility::External) => {
            let params = f.parameters.iter().map(|v| ty_of(p, &v.ty)).collect();
            let rets = f.returns().iter().map(|v| ty_of(p, &v.ty)).collect();
            Ty::Fn(Box::new((params, rets)))
        }
        K::Custom(path) => {
            let n = path.last().as_str();
            if let Some(k) = p.enums.get(n) {
                Ty::Enum(n.into(), *k)
            } else if p.contract(n).is_some() {
                Ty::Contract(n.into())
            } else if p.struct_fields(n).is_some() {
                Ty::Struct(n.into())
            } else if let Some(u) = p.udvts.get(n) {
                u.clone()
            } else {
                Ty::Opaque
            }
        }
        _ => Ty::Opaque,
    }
}

/// Overload resolution: among same-name, same-arity functions, the one whose parameter types
/// accept the arguments (exact matches preferred, then implicit widening, then literals).
/// Name of a user-defined value type (or other named type) a parameter is declared with.
fn decl_type_name(ty: &ast::Type<'_>) -> Option<String> {
    match &ty.kind {
        ast::TypeKind::Custom(p) => Some(p.last().as_str().to_string()),
        _ => None,
    }
}

fn pick<'q, 'a>(funcs: &'q [Func<'a>], name: &str, args: &[Val]) -> Option<&'q Func<'a>> {
    let cands: Vec<&Func<'a>> =
        funcs.iter().filter(|f| f.name == name && f.params.len() == args.len()).collect();
    pick_in(cands, args)
}

fn pick_in<'q, 'a>(cands: Vec<&'q Func<'a>>, args: &[Val]) -> Option<&'q Func<'a>> {
    if cands.len() <= 1 {
        return cands.into_iter().next();
    }
    let score = |p: &Ty, a: &Ty| -> Option<u32> {
        match (p, a) {
            _ if p == a => Some(3),
            (Ty::Uint(n), Ty::Uint(m)) if n >= m => Some(2),
            (Ty::Int(n), Ty::Int(m)) if n >= m => Some(2),
            (Ty::Int(n), Ty::Uint(m)) if n > m => Some(1),
            (Ty::Uint(_) | Ty::Int(_) | Ty::Enum(..), Ty::Lit) => Some(1),
            (Ty::Addr, Ty::Contract(_)) | (Ty::Contract(_), Ty::Addr) => Some(1),
            (Ty::Opaque, _) | (_, Ty::Opaque) => Some(0),
            _ => None,
        }
    };
    cands
        .into_iter()
        .filter_map(|f| {
            let mut tot = 0;
            for ((_, p), a) in f.params.iter().zip(args) {
                tot += score(p, &a.ty)?;
            }
            Some((tot, f))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, f)| f)
}

/// Bitvector view of an Int term: SSA names defined by a term are viewed through their
/// definition (so shifts and masks chain in bitvectors); other in-range variables get a twin.
fn bv_view(
    t: &str,
    n: u32,
    defs: &FxHashMap<String, String>,
    ranges: &FxHashMap<String, crate::interval::Interval>,
    declared: &mut FxHashSet<String>,
    pending: &mut Vec<String>,
    depth: usize,
) -> String {
    crate::interval::to_bv(t, n, &mut |a: &str| {
        if depth < 24
            && let Some(d) = defs.get(a)
        {
            let r = bv_view(d, n, defs, ranges, declared, pending, depth + 1);
            if r.len() < 20_000 {
                return Some(r);
            }
        }
        let (lo, hi) = ranges.get(a)?;
        if lo < &BigInt::from(0) || hi >= &pow2(n) {
            return None;
        }
        let tw = format!("|{}#bv{n}|", a.trim_matches('|'));
        if declared.insert(tw.clone()) {
            pending.push(format!(
                "(declare-const {tw} (_ BitVec {n}))\n(assert (= {a} (bv2nat {tw})))\n"
            ));
        }
        Some(tw)
    })
}

/// Modifier lists with the parameter bindings they run under.
type ModifierSources<'a> = Vec<(&'a [ast::Modifier<'a>], Vec<(String, Ty, Val)>)>;
