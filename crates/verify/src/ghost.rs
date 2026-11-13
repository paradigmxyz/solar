//! Ghost-code erasure: a non-interference check run before any verification condition.
//!
//! Ghost code is everything solar-verify runs that the EVM does not: `@custom:ghost x = e`
//! updates, the expressions of every spec (asserts, assumes, loop invariants and variants,
//! requires, ensures, invariants, lemma arguments) and the bodies of the lemmas that
//! `@custom:use` applies and of the functions ghost updates call. Ghost state is the variables
//! a contract declares with `@custom:ghost T name`. A proof about the program with its ghost code
//! is a proof about the deployed program only if erasing that code leaves every real execution
//! unchanged. Following Filliâtre, Gondelman and Paskevich, "The Spirit of Ghost Code" (CAV 2014),
//! that holds when ghost code and real code do not interfere:
//!
//! - ghost code writes only ghost state (it may read real state);
//! - ghost code does not change real control flow: it never reverts (`require`, `assert`, `revert`,
//!   `selfdestruct`), makes no external call and deploys nothing, and no ghost value decides
//!   whether a real write happens;
//! - ghost code terminates: it calls only pure and view functions, every loop in the functions it
//!   runs carries a `@custom:decreases` variant, and those functions are not recursive;
//! - real code never reads ghost state, so no real value, condition, return or `@custom:check` (a
//!   `require` under `--instrument`) depends on it, and no real declaration shares a ghost's name.
//!
//! The check grades every variable: user ghost variables are ghost (grade 0); state variables,
//! parameters, locals and the model's own variables (`__balance`, `__raw`, `__traw`,
//! `__time`), which stand for real state, are real (grade 1). An expression is ghost when it
//! reads a ghost variable, and a ghost value must never reach a real sink: a real assignment, a
//! real condition or a real return. Spec expressions are ghost code, so a write in one is
//! checked like a ghost update; Solidity code is real. Functions are graded by their use:
//! those ghost code calls must be pure or view (they cannot write state) and must terminate.
//!
//! A lemma's ensures are assumed at each use, so a lemma must also return normally wherever its
//! requires hold: a lemma that always reverts proves anything. This module marks every function
//! `@custom:use` reaches, and every `@custom:broadcast` lemma with what it reaches
//! (`Func::ghost_fn`), and the verification conditions of those functions include that totality
//! obligation; termination stays the syntactic rule above. `@custom:broadcast-use`,
//! `@custom:group` and `@custom:trigger` are spec-only: the first two name lemmas, and trigger
//! terms are spec expressions.
//!
//! The check is syntactic and conservative. It sees names, not types: a member call it cannot
//! resolve to an internal or library function counts as an external call, and a loop variant is
//! required syntactically (its decrease is proved as for any loop). Recursion in ghost functions
//! is rejected rather than measured, since functions have no `@custom:decreases`. Yul identifiers
//! in inline assembly are not graded: assembly reaches only real state.
use crate::{
    prog::{Contract, Func, Program, Spec},
    walk,
};
use solar_parse::{
    ast::{self, BinOpKind, Expr, ExprKind, ItemFunction, Stmt, StmtKind, TypeKind, UnOpKind},
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
    },
};

/// The non-interference rule a violation breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rule {
    /// A real declaration has a ghost variable's name.
    Name,
    /// Real code reads ghost state.
    RealRead,
    /// A real condition reads ghost state.
    RealGuard,
    /// Ghost code writes real state.
    Write,
    /// A ghost condition decides a real write.
    Guard,
    /// Ghost code may revert.
    Revert,
    /// Ghost code calls a function that may write state, or calls out.
    Call,
    /// A loop in ghost code has no variant.
    Loop,
    /// A ghost function is recursive.
    Recursion,
}

impl Rule {
    /// The rule, in one line, for the diagnostic's note.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::Name => "a ghost variable needs a name no real declaration uses",
            Self::RealRead | Self::RealGuard => {
                "real code never reads ghost state: erasing the ghost code would leave it undefined"
            }
            Self::Write => "ghost code writes only ghost variables (`@custom:ghost T name`)",
            Self::Guard => "a ghost value never decides whether real code runs",
            Self::Revert => "ghost code never reverts: that would change real control flow",
            Self::Call => "ghost code calls only internal pure and view functions",
            Self::Loop => {
                "ghost code terminates: every loop it runs needs a `@custom:decreases` variant"
            }
            Self::Recursion => "ghost code terminates: ghost functions may not be recursive",
        }
    }
}

/// One broken rule.
pub(crate) struct Violation {
    pub span: Span,
    pub msg: String,
    pub rule: Rule,
    /// `Contract.fn` the violation is in, if any.
    pub func: Option<String>,
}

/// A function key: (declaring contract, name, number of parameters).
pub(crate) type FnKey = (String, String, usize);

/// Checks `prog` for interference between ghost and real code. Returns the violations, and the
/// functions `@custom:use` reaches, which owe the totality obligation.
pub(crate) fn check(prog: &Program<'_>) -> (Vec<Violation>, FxHashSet<FnKey>) {
    let mut cx =
        Cx { prog, out: vec![], seen: FxHashSet::default(), work: vec![], lemma_roots: vec![] };
    for c in prog.contracts.iter().filter(|c| !c.kind.is_interface()) {
        cx.contract(c);
    }
    let lemmas = cx.ghost_fns();
    let mut out = cx.out;
    out.sort_by_key(|v| (v.span.lo(), v.span.hi()));
    (out, lemmas)
}

fn key(f: &Func<'_>) -> FnKey {
    (f.contract.clone(), f.name.clone(), f.params.len())
}

/// The ghost variables of `c` and the real names in scope where ghost code runs.
struct Scope {
    ghosts: FxHashSet<String>,
    /// Parameters, returns and locals of the body being checked: real, and shadowing nothing
    /// (a collision with a ghost is reported).
    locals: FxHashSet<String>,
    func: Option<String>,
}

impl Scope {
    fn is_ghost(&self, n: &str) -> bool {
        self.ghosts.contains(n) && !self.locals.contains(n)
    }
}

struct Cx<'p, 'a> {
    prog: &'p Program<'a>,
    out: Vec<Violation>,
    seen: FxHashSet<(Span, String)>,
    /// Functions ghost code runs, still to check: their bodies must terminate.
    work: Vec<(&'p Contract<'a>, &'p Func<'a>)>,
    /// Functions `@custom:use` applies.
    lemma_roots: Vec<(&'p Contract<'a>, &'p Func<'a>)>,
}

impl<'p, 'a> Cx<'p, 'a> {
    fn emit(&mut self, span: Span, rule: Rule, msg: String, sc: &Scope) {
        if self.seen.insert((span, msg.clone())) {
            self.out.push(Violation { span, msg, rule, func: sc.func.clone() });
        }
    }

    fn contract(&mut self, c: &'p Contract<'a>) {
        // A ghost named like a real state variable is reported once below; the name stays real.
        let real = |n: &str| c.state.iter().any(|s| s.ghost_decl.is_none() && s.name == n);
        let ghosts: FxHashSet<String> = c
            .state
            .iter()
            .filter(|s| s.ghost_decl.is_some() && !real(&s.name))
            .map(|s| s.name.clone())
            .collect();
        let mut sc = Scope { ghosts, locals: FxHashSet::default(), func: None };
        // Names: a ghost shares none with a real state variable, function or modifier.
        for g in c.state.iter().filter(|s| s.ghost_decl.is_some()) {
            let sp = g.ghost_decl.unwrap();
            let clash = if real(&g.name) {
                Some("state variable")
            } else if c.funcs.iter().any(|f| f.name == g.name) {
                Some("function")
            } else if c.modifiers.contains_key(&g.name) {
                Some("modifier")
            } else {
                None
            };
            if let Some(what) = clash {
                let msg = format!("ghost variable `{}` has the name of a real {what}", g.name);
                self.emit(sp, Rule::Name, msg, &sc);
            }
        }
        // Real state initializers and constant values are real code.
        for s in c.state.iter().filter(|s| s.ghost_decl.is_none()) {
            if let Some(init) = s.init {
                self.real_expr(init, &sc, false);
            }
        }
        for inv in c.invariants.iter().chain(&c.lock_stable).chain(c.lock_expr.as_ref()) {
            self.spec(c, inv, &sc, Mode::Spec);
        }
        for f in c.funcs.iter().chain(&c.overridden) {
            sc.func = Some(format!("{}.{}", c.name, f.name));
            sc.locals = self.locals(f.item, &sc);
            let triggers = f.triggers.iter().flatten();
            for s in f.requires.iter().chain(&f.ensures).chain(&f.succeeds).chain(triggers) {
                self.spec(c, s, &sc, Mode::Spec);
            }
            if f.broadcast {
                // A broadcast lemma is a lemma wherever it is used: ghost code, and total.
                self.work.push((c, f));
                self.lemma_roots.push((c, f));
            }
            for s in &f.checks {
                self.check_claim(s, &sc);
            }
            self.body(c, f.item, &sc);
        }
        for (name, m) in &c.modifiers {
            sc.func = Some(format!("{}.{name}", c.name));
            sc.locals = self.locals(m, &sc);
            self.body(c, m, &sc);
        }
    }

    /// The real names declared in `f`: parameters, returns, locals and catch parameters. A ghost
    /// variable's name among them is an error.
    fn locals(&mut self, f: &'a ItemFunction<'a>, sc: &Scope) -> FxHashSet<String> {
        let mut names: Vec<ast::Ident> = vec![];
        let params = f.header.parameters.iter().chain(f.header.returns().iter());
        names.extend(params.filter_map(|p| p.name));
        for s in f.body.iter().flat_map(|b| b.stmts.iter()) {
            walk::all_stmts(s, &mut |s| match &s.kind {
                StmtKind::DeclSingle(v) => names.extend(v.name),
                StmtKind::DeclMulti(vs, _) => names
                    .extend(vs.iter().filter_map(|v| v.as_ref().unspan()).filter_map(|v| v.name)),
                StmtKind::Try(t) => {
                    for cl in t.clauses.iter() {
                        names.extend(cl.args.iter().filter_map(|p| p.name));
                    }
                }
                _ => {}
            });
        }
        let mut out = FxHashSet::default();
        for n in names {
            if sc.ghosts.contains(n.as_str()) {
                let msg = format!("local `{n}` has the name of a ghost variable");
                self.emit(n.span, Rule::Name, msg, sc);
            }
            out.insert(n.as_str().to_string());
        }
        out
    }

    /// The statements of a function or modifier body, and the specs they carry.
    fn body(&mut self, c: &'p Contract<'a>, f: &'a ItemFunction<'a>, sc: &Scope) {
        let Some(b) = &f.body else { return };
        let prog = self.prog;
        for s in b.stmts.iter() {
            walk::all_stmts(s, &mut |s| {
                self.real_stmt(s, sc);
                for (tag, sp) in prog.stmt_specs.get(&s.span).into_iter().flatten() {
                    match tag.as_str() {
                        "check" => self.check_claim(sp, sc),
                        "ghost" => self.spec(c, sp, sc, Mode::Update),
                        "use" => self.spec(c, sp, sc, Mode::Use),
                        // Names of lemmas, which are checked as roots of their own.
                        "broadcast-use" => {}
                        _ => self.spec(c, sp, sc, Mode::Spec),
                    }
                }
                if let StmtKind::Block(bl) | StmtKind::UncheckedBlock(bl) = &s.kind {
                    for (_, sp) in prog.block_tail.get(&bl.span).into_iter().flatten() {
                        self.spec(c, sp, sc, Mode::Spec);
                    }
                }
            });
        }
        for (_, sp) in prog.block_tail.get(&b.span).into_iter().flatten() {
            self.spec(c, sp, sc, Mode::Spec);
        }
    }

    /// The expressions directly in real statement `s`, with the conditions it branches on.
    fn real_stmt(&mut self, s: &'a Stmt<'a>, sc: &Scope) {
        match &s.kind {
            StmtKind::If(cond, ..) | StmtKind::While(cond, _) | StmtKind::DoWhile(_, cond) => {
                self.real_expr(cond, sc, true)
            }
            StmtKind::For { cond, next, .. } => {
                if let Some(cond) = cond {
                    self.real_expr(cond, sc, true);
                }
                if let Some(next) = next {
                    self.real_expr(next, sc, false);
                }
            }
            StmtKind::Assembly(_) => {}
            _ => walk::stmt_exprs(s, &mut |e| self.real_expr(e, sc, false)),
        }
    }

    /// Real expression `e`: no ghost variable may appear in it. `control` says `e` decides a
    /// branch.
    fn real_expr(&mut self, e: &'a Expr<'a>, sc: &Scope, control: bool) {
        if sc.ghosts.is_empty() {
            return;
        }
        let conds = cond_spans(e, control);
        let mut hits = vec![];
        walk::sub_exprs(e, &mut |x| {
            if let ExprKind::Ident(i) = &x.kind
                && sc.is_ghost(i.as_str())
            {
                hits.push(*i);
            }
        });
        for i in hits {
            if conds.iter().any(|c| within(i.span, *c)) {
                let msg = format!("real control flow depends on ghost variable `{i}`");
                self.emit(i.span, Rule::RealGuard, msg, sc);
            } else {
                let msg = format!("real code reads ghost variable `{i}`");
                self.emit(i.span, Rule::RealRead, msg, sc);
            }
        }
    }

    /// `@custom:check P`: real code, a `require` under `--instrument`.
    fn check_claim(&mut self, sp: &Spec<'a>, sc: &Scope) {
        let mut hits = vec![];
        walk::sub_exprs(sp.expr, &mut |x| {
            if let ExprKind::Ident(i) = &x.kind
                && sc.is_ghost(i.as_str())
            {
                hits.push(i.to_string());
            }
        });
        for i in hits {
            let msg =
                format!("`@custom:check` is real code (a require) and reads ghost variable `{i}`");
            self.emit(sp.span, Rule::RealRead, msg, sc);
        }
    }

    /// Ghost code: spec expression `sp`. In an update or a lemma use, the functions it calls run
    /// as ghost code and must terminate.
    fn spec(&mut self, c: &'p Contract<'a>, sp: &Spec<'a>, sc: &Scope, mode: Mode) {
        let e = sp.expr;
        // The branches a ghost condition guards.
        let mut guarded = vec![];
        walk::sub_exprs(e, &mut |x| match &x.kind {
            ExprKind::Ternary(cnd, a, b) if reads_ghost(cnd, sc) => {
                guarded.push(a.span);
                guarded.push(b.span);
            }
            ExprKind::Binary(l, op, r)
                if matches!(op.kind, BinOpKind::And | BinOpKind::Or) && reads_ghost(l, sc) =>
            {
                guarded.push(r.span)
            }
            _ => {}
        });
        let mut found: Vec<(Rule, String)> = vec![];
        let mut calls: Vec<(&'p Contract<'a>, &'p Func<'a>)> = vec![];
        let is_use = mode == Mode::Use;
        let lemma = if is_use { self.resolve_call(c, e) } else { None };
        walk::sub_exprs(e, &mut |x| {
            let write = |l: &'a Expr<'a>, found: &mut Vec<(Rule, String)>| {
                let roots = walk::root(l);
                if roots.is_empty() {
                    found.push((Rule::Write, "ghost code writes a location it cannot name".into()));
                }
                for r in roots {
                    if sc.is_ghost(&r) {
                        continue;
                    }
                    if guarded.iter().any(|g| within(x.span, *g)) {
                        found.push((
                            Rule::Guard,
                            format!("a ghost condition guards a write of real `{r}`"),
                        ));
                    } else {
                        found.push((Rule::Write, format!("ghost code writes real `{r}`")));
                    }
                }
            };
            match &x.kind {
                ExprKind::Assign(l, ..) | ExprKind::Delete(l) => write(l, &mut found),
                ExprKind::Unary(op, l)
                    if matches!(
                        op.kind,
                        UnOpKind::PreInc | UnOpKind::PreDec | UnOpKind::PostInc | UnOpKind::PostDec
                    ) =>
                {
                    write(l, &mut found)
                }
                ExprKind::CallOptions(..) => {
                    found.push((Rule::Call, "ghost code makes an external call".into()))
                }
                ExprKind::New(t) if matches!(&t.kind, TypeKind::Custom(p) if self.prog.contract(p.last().as_str()).is_some()) => {
                    found.push((Rule::Call, "ghost code deploys a contract".into()))
                }
                ExprKind::Call(callee, _) => {
                    if is_use && std::ptr::eq(x, e.peel_parens()) {
                        return;
                    }
                    match &callee.peel_parens().kind {
                        ExprKind::Ident(i)
                            if matches!(
                                i.as_str(),
                                "require" | "assert" | "revert" | "selfdestruct"
                            ) =>
                        {
                            found.push((
                                Rule::Revert,
                                format!("ghost code calls `{i}`, which may revert"),
                            ));
                            return;
                        }
                        ExprKind::Member(b, m) if matches!(m.as_str(), "push" | "pop") => {
                            write(b, &mut found);
                            return;
                        }
                        ExprKind::Member(_, m)
                            if matches!(
                                m.as_str(),
                                "transfer" | "send" | "call" | "delegatecall" | "staticcall"
                            ) =>
                        {
                            found.push((
                                Rule::Call,
                                format!("ghost code makes an external call (`.{m}`)"),
                            ));
                            return;
                        }
                        _ => {}
                    }
                    match self.callees(c, x) {
                        Callees::Internal(fs) => {
                            for (fc, f) in fs {
                                if f.is_static() {
                                    calls.push((fc, f));
                                } else {
                                    found.push((
                                        Rule::Call,
                                        format!(
                                            "ghost code calls `{}`, which is not pure or view",
                                            f.name
                                        ),
                                    ));
                                }
                            }
                        }
                        // A spec evaluates an external call without its effects.
                        Callees::External(_) if mode == Mode::Spec => {}
                        Callees::External(what) => found.push((
                            Rule::Call,
                            format!("ghost code makes an external call (`{what}`)"),
                        )),
                        Callees::Builtin => {}
                    }
                }
                _ => {}
            }
        });
        for (rule, msg) in found {
            self.emit(sp.span, rule, msg, sc);
        }
        if mode != Mode::Spec {
            self.work.extend(calls);
            if let Some((lc, lf)) = lemma {
                self.work.push((lc, lf));
                self.lemma_roots.push((lc, lf));
            }
        }
    }

    /// The lemma `@custom:use` names in `e`, resolved as the verifier does.
    fn resolve_call(
        &self,
        c: &'p Contract<'a>,
        e: &'a Expr<'a>,
    ) -> Option<(&'p Contract<'a>, &'p Func<'a>)> {
        let ExprKind::Call(callee, args) = &e.peel_parens().kind else { return None };
        let n = args.exprs().count();
        let (lc, name) = match &callee.peel_parens().kind {
            ExprKind::Ident(i) => (c, i.as_str()),
            ExprKind::Member(b, m) => match &b.kind {
                ExprKind::Ident(l) => (self.prog.contract(l.as_str())?, m.as_str()),
                _ => return None,
            },
            _ => return None,
        };
        lc.funcs.iter().find(|f| f.name == name && f.params.len() == n).map(|f| (lc, f))
    }

    /// What call expression `call`, in contract `c`, may run.
    fn callees(&self, c: &'p Contract<'a>, call: &'a Expr<'a>) -> Callees<'p, 'a> {
        let ExprKind::Call(callee, args) = &call.kind else { return Callees::Builtin };
        let n = args.exprs().count();
        let pick = |cc: &'p Contract<'a>, fs: &'p [Func<'a>], name: &str, n: usize| {
            let all: Vec<(&'p Contract<'a>, &'p Func<'a>)> =
                fs.iter().filter(|f| f.name == name).map(|f| (cc, f)).collect();
            let exact: Vec<_> = all.iter().copied().filter(|(_, f)| f.params.len() == n).collect();
            if exact.is_empty() { all } else { exact }
        };
        match &callee.peel_parens().kind {
            ExprKind::Ident(i) => {
                let fs = pick(c, &c.funcs, i.as_str(), n);
                if fs.is_empty() { Callees::Builtin } else { Callees::Internal(fs) }
            }
            ExprKind::Member(b, m) => {
                let m = m.as_str();
                if let ExprKind::Ident(l) = &b.peel_parens().kind {
                    let l = l.as_str();
                    if matches!(l, "abi" | "bytes" | "string" | "block" | "msg" | "tx")
                        || self.prog.udvts.contains_key(l)
                    {
                        return Callees::Builtin;
                    }
                    if l == "super" {
                        let fs = pick(c, &c.overridden, m, n);
                        return if fs.is_empty() {
                            Callees::Builtin
                        } else {
                            Callees::Internal(fs)
                        };
                    }
                    if l == "this" {
                        return Callees::External(format!("this.{m}"));
                    }
                    if let Some(lc) = self.prog.contract(l)
                        && !lc.kind.is_interface()
                    {
                        let fs = pick(lc, &lc.funcs, m, n);
                        if !fs.is_empty() {
                            return Callees::Internal(fs);
                        }
                    }
                }
                // `x.f(args)` through `using L for T`.
                let fs: Vec<_> = self.prog.using_fns(m, n);
                if fs.is_empty() {
                    Callees::External(format!(".{m}"))
                } else {
                    Callees::Internal(fs)
                }
            }
            _ => Callees::Builtin,
        }
    }

    /// Checks the bodies of the functions ghost code runs, transitively: every loop has a variant,
    /// every call is internal and to a pure or view function, and no function recurses. Returns
    /// the functions lemmas reach, which owe totality.
    fn ghost_fns(&mut self) -> FxHashSet<FnKey> {
        let mut edges: FxHashMap<FnKey, Vec<FnKey>> = FxHashMap::default();
        let mut uses: FxHashMap<FnKey, Vec<FnKey>> = FxHashMap::default();
        let mut by_key: FxHashMap<FnKey, &'p Func<'a>> = FxHashMap::default();
        let mut done = FxHashSet::default();
        let prog = self.prog;
        while let Some((c, f)) = self.work.pop() {
            let k = key(f);
            if !done.insert(k.clone()) {
                continue;
            }
            by_key.insert(k.clone(), f);
            let home = prog.contract(&f.contract).unwrap_or(c);
            let sc = Scope {
                ghosts: FxHashSet::default(),
                locals: FxHashSet::default(),
                func: Some(format!("{}.{}", home.name, f.name)),
            };
            let Some(b) = &f.item.body else { continue };
            let mut found: Vec<(Span, Rule, String)> = vec![];
            let mut next: Vec<(&'p Contract<'a>, &'p Func<'a>)> = vec![];
            let mut lemma_next: Vec<(&'p Contract<'a>, &'p Func<'a>)> = vec![];
            for s in b.stmts.iter() {
                walk::all_stmts(s, &mut |s| {
                    let specs = prog.stmt_specs.get(&s.span);
                    if matches!(
                        s.kind,
                        StmtKind::While(..) | StmtKind::For { .. } | StmtKind::DoWhile(..)
                    ) && !specs.is_some_and(|v| v.iter().any(|(t, _)| t == "decreases"))
                    {
                        found.push((
                            s.span,
                            Rule::Loop,
                            format!(
                                "loop in ghost function `{}` has no `@custom:decreases` variant",
                                f.name
                            ),
                        ));
                    }
                    for (t, sp) in specs.into_iter().flatten() {
                        if t == "use"
                            && let Some(l) = self.resolve_call(home, sp.expr)
                        {
                            lemma_next.push(l);
                        }
                    }
                    walk::stmt_exprs(s, &mut |e| {
                        walk::sub_exprs(e, &mut |x| match &x.kind {
                            ExprKind::Call(..) => match self.callees(home, x) {
                                Callees::Internal(fs) => {
                                    for (fc, g) in fs {
                                        if g.is_static() {
                                            next.push((fc, g));
                                        } else {
                                            found.push((x.span, Rule::Call, format!("ghost function `{}` calls `{}`, which is not pure or view", f.name, g.name)));
                                        }
                                    }
                                }
                                Callees::External(what) => found.push((
                                    x.span,
                                    Rule::Call,
                                    format!(
                                        "ghost function `{}` makes an external call (`{what}`)",
                                        f.name
                                    ),
                                )),
                                Callees::Builtin => {}
                            },
                            ExprKind::CallOptions(..) => found.push((
                                x.span,
                                Rule::Call,
                                format!("ghost function `{}` makes an external call", f.name),
                            )),
                            _ => {}
                        })
                    });
                });
            }
            for (sp, rule, msg) in found {
                self.emit(sp, rule, msg, &sc);
            }
            edges.insert(k.clone(), next.iter().map(|(_, g)| key(g)).collect());
            uses.insert(k, lemma_next.iter().map(|(_, g)| key(g)).collect());
            self.work.extend(next);
            self.work.extend(lemma_next);
        }
        // Recursion through calls (cycles through `@custom:use` are the verifier's cyclic-proof
        // error).
        let mut keys: Vec<&FnKey> = edges.keys().collect();
        keys.sort();
        for k in keys {
            if reaches(&edges, k, k, &mut FxHashSet::default()) {
                let f = by_key[k];
                let sc = Scope {
                    ghosts: FxHashSet::default(),
                    locals: FxHashSet::default(),
                    func: Some(format!("{}.{}", k.0, k.1)),
                };
                let msg = format!("ghost function `{}` is recursive", f.name);
                self.emit(f.item.header.span, Rule::Recursion, msg, &sc);
            }
        }
        // Everything a lemma reaches, by calls or uses, owes totality.
        let mut lemmas = FxHashSet::default();
        let mut stack: Vec<FnKey> = self.lemma_roots.iter().map(|(_, f)| key(f)).collect();
        while let Some(k) = stack.pop() {
            if lemmas.insert(k.clone()) {
                stack.extend(edges.get(&k).into_iter().flatten().cloned());
                stack.extend(uses.get(&k).into_iter().flatten().cloned());
            }
        }
        lemmas
    }
}

/// Where ghost code `spec` checks sits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A spec expression: evaluated, never run.
    Spec,
    /// `@custom:ghost x = e`: run, with the functions it calls.
    Update,
    /// `@custom:use L(args)`: `L` is a lemma, run as ghost code and total.
    Use,
}

enum Callees<'p, 'a> {
    /// Internal or library functions, by overload.
    Internal(Vec<(&'p Contract<'a>, &'p Func<'a>)>),
    /// A call that leaves the contract.
    External(String),
    /// A builtin, cast or constructor of a value.
    Builtin,
}

/// Whether `from` reaches `to` in one or more call edges.
fn reaches(
    edges: &FxHashMap<FnKey, Vec<FnKey>>,
    from: &FnKey,
    to: &FnKey,
    seen: &mut FxHashSet<FnKey>,
) -> bool {
    for n in edges.get(from).into_iter().flatten() {
        if n == to || (seen.insert(n.clone()) && reaches(edges, n, to, seen)) {
            return true;
        }
    }
    false
}

fn within(inner: Span, outer: Span) -> bool {
    outer.lo() <= inner.lo() && inner.hi() <= outer.hi()
}

/// Whether `e` reads a ghost variable of `sc`.
fn reads_ghost<'a>(e: &'a Expr<'a>, sc: &Scope) -> bool {
    let mut hit = false;
    walk::sub_exprs(e, &mut |x| {
        hit |= matches!(&x.kind, ExprKind::Ident(i) if sc.is_ghost(i.as_str()));
    });
    hit
}

/// The spans in `e` that decide control flow: `e` itself when `control`, ternary conditions,
/// left operands of `&&` and `||`, and the condition of `require` and `assert`.
fn cond_spans<'a>(e: &'a Expr<'a>, control: bool) -> Vec<Span> {
    let mut out = vec![];
    if control {
        out.push(e.span);
    }
    walk::sub_exprs(e, &mut |x| match &x.kind {
        ExprKind::Ternary(c, ..) => out.push(c.span),
        ExprKind::Binary(l, op, _) if matches!(op.kind, BinOpKind::And | BinOpKind::Or) => {
            out.push(l.span)
        }
        ExprKind::Call(callee, args) if matches!(&callee.kind, ExprKind::Ident(i) if matches!(i.as_str(), "require" | "assert")) => {
            out.extend(args.exprs().next().map(|a| a.span))
        }
        _ => {}
    });
    out
}
