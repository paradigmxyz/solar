//! Broadcast lemmas and groups, after Verus's `broadcast proof` and `broadcast group` (see
//! Tunable Automation in Automated Program Verification, <https://arxiv.org/abs/2512.03926>).
//!
//! A lemma marked `@custom:broadcast` is a pure function proved on its own, as the lemmas
//! `@custom:use` applies are. Where `@custom:broadcast-use` names it, or a
//! `@custom:group` that contains it, its requires ==> ensures, quantified over its parameters'
//! ranges, is an axiom of the VC: on the contract for every function, on a function for that
//! function, and on a statement from that statement on along its path. The solver instantiates
//! the axiom at the ground terms its triggers match: the `@custom:trigger` terms, else the
//! innermost applications of uninterpreted functions in the ensures that mention every
//! parameter. Lemmas nobody names stay dormant, so they cost no solver time.
//!
//! An axiom is assumed only once its lemma is proved ([`prove_all`]), and never in the proof
//! of a function the lemma depends on: through calls, `@custom:use` and the broadcasts of its
//! own proof ([`reaches`]), so no proof assumes what it proves. A broadcast lemma's own proof
//! takes only the broadcasts it names itself, on the lemma or its statements, never the
//! contract's: lemmas of one contract-wide group would otherwise all depend on each other.
use crate::{
    ghost::FnKey,
    prog::{Contract, Func, Program, Spec},
    sexp::Sexp,
    smt::Smt,
    vc::{Opts, Vc},
    walk,
};
use solar_parse::{
    ast::ExprKind,
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
    },
};

/// A lemma as resolved from a contract, which names what its specs mean.
pub(crate) type Lemma<'p, 'a> = (&'p Contract<'a>, &'p Func<'a>);

type R<T> = Result<T, (Span, String)>;

pub(crate) fn key(f: &Func<'_>) -> FnKey {
    (f.contract.clone(), f.name.clone(), f.params.len())
}

/// Adds the broadcast lemmas `name` (a lemma or group, `L` or `C.L`) stands for in `c` to
/// `out`, through nested groups.
pub(crate) fn resolve<'p, 'a>(
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    name: &str,
    span: Span,
    out: &mut Vec<Lemma<'p, 'a>>,
) -> R<()> {
    resolve_in(prog, c, name, span, &mut vec![], out)
}

fn resolve_in<'p, 'a>(
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    name: &str,
    span: Span,
    stack: &mut Vec<String>,
    out: &mut Vec<Lemma<'p, 'a>>,
) -> R<()> {
    let (cc, n) = match name.split_once('.') {
        Some((x, n)) => {
            (prog.contract(x).ok_or((span, format!("unknown contract `{x}` in `{name}`")))?, n)
        }
        None => (c, name),
    };
    if let Some(g) = cc.groups.iter().find(|g| g.name == n) {
        let id = format!("{}.{n}", cc.name);
        if let Some(i) = stack.iter().position(|s| *s == id) {
            let mut cycle = stack[i..].to_vec();
            cycle.push(id);
            return Err((
                g.span,
                format!("broadcast group `{n}` contains itself: {}", cycle.join(" -> ")),
            ));
        }
        stack.push(id);
        for m in &g.members {
            resolve_in(prog, cc, m, g.span, stack, out)?;
        }
        stack.pop();
        return Ok(());
    }
    let mut fs = cc.funcs.iter().filter(|f| f.name == n).peekable();
    if fs.peek().is_none() {
        return Err((span, format!("unknown broadcast lemma or group `{name}`")));
    }
    let Some(f) = fs.find(|f| f.broadcast) else {
        return Err((
            span,
            format!("`{name}` is not a broadcast lemma: mark it `@custom:broadcast`"),
        ));
    };
    if !out.iter().any(|(_, g)| std::ptr::eq(*g, f)) {
        out.push((cc, f));
    }
    Ok(())
}

/// The broadcast lemmas the whole VC of `f`, verified in `c`, may assume: the contract's and
/// the function's `@custom:broadcast-use` (only the function's for a broadcast lemma).
pub(crate) fn active<'p, 'a>(
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    f: &'p Func<'a>,
) -> R<Vec<Lemma<'p, 'a>>> {
    let mut out = vec![];
    let contract = if f.broadcast { &[][..] } else { &c.broadcast_use[..] };
    for (n, sp) in contract.iter().chain(&f.broadcast_use) {
        resolve(prog, c, n, *sp, &mut out)?;
    }
    Ok(out)
}

/// The `@custom:broadcast-use` statement specs in the body of `f`.
pub(crate) fn stmt_uses<'p, 'a>(prog: &'p Program<'a>, f: &Func<'a>) -> Vec<&'p Spec<'a>> {
    let mut out = vec![];
    for s in f.item.body.iter().flat_map(|b| b.stmts.iter()) {
        walk::all_stmts(s, &mut |s| {
            for (tag, sp) in prog.stmt_specs.get(&s.span).into_iter().flatten() {
                if tag == "broadcast-use" {
                    out.push(sp);
                }
            }
        });
    }
    out
}

/// Every broadcast lemma `f`'s proof may assume, at any scope; names that do not resolve are
/// left out (the VC reports them).
fn all_active<'p, 'a>(
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    f: &'p Func<'a>,
) -> Vec<Lemma<'p, 'a>> {
    let mut out = active(prog, c, f).unwrap_or_default();
    for sp in stmt_uses(prog, f) {
        let _ = resolve(prog, c, &sp.text, sp.span, &mut out);
    }
    out
}

/// What the proof of `f` (in `c`) rests on: the functions it calls, the lemmas it uses and the
/// broadcast lemmas it may assume.
fn deps<'p, 'a>(prog: &'p Program<'a>, c: &'p Contract<'a>, f: &'p Func<'a>) -> Vec<Lemma<'p, 'a>> {
    let mut calls = vec![];
    for s in f.item.body.iter().flat_map(|b| b.stmts.iter()) {
        walk::all_stmts(s, &mut |s| {
            walk::stmt_exprs(s, &mut |e| {
                walk::sub_exprs(e, &mut |x| {
                    if let ExprKind::Call(cal, a) = &x.kind {
                        calls.push((&**cal, a.exprs().count()));
                    }
                })
            });
            for (tag, sp) in prog.stmt_specs.get(&s.span).into_iter().flatten() {
                if tag == "use"
                    && let ExprKind::Call(cal, a) = &sp.expr.peel_parens().kind
                {
                    calls.push((&**cal, a.exprs().count()));
                }
            }
        });
    }
    let mut out = all_active(prog, c, f);
    for (cal, n) in calls {
        let (c2, name) = match &cal.kind {
            ExprKind::Ident(i) => (Some(c), i.as_str()),
            ExprKind::Member(b, m) => match &b.kind {
                ExprKind::Ident(l) => (prog.contract(l.as_str()), m.as_str()),
                _ => continue,
            },
            _ => continue,
        };
        let Some(c2) = c2 else { continue };
        if let Some(g) = c2.funcs.iter().find(|g| g.name == name && g.params.len() == n) {
            out.push((c2, g));
        }
    }
    out
}

/// Whether the proof of `from` rests, directly or not, on function `to`.
pub(crate) fn reaches<'p, 'a>(prog: &'p Program<'a>, from: Lemma<'p, 'a>, to: &FnKey) -> bool {
    let mut seen = FxHashSet::default();
    let mut stack = vec![from];
    while let Some((c, f)) = stack.pop() {
        for (c2, g) in deps(prog, c, f) {
            let k = key(g);
            if &k == to {
                return true;
            }
            if seen.insert(k) {
                stack.push((c2, g));
            }
        }
    }
    false
}

/// Proves every broadcast lemma some `@custom:broadcast-use` names, each after the lemmas its own
/// proof assumes: whether each verified. With `dry`, no solver runs and every lemma counts as
/// proved, so `--dump-smt` shows the axioms.
pub(crate) fn prove_all(
    prog: &Program<'_>,
    opts: &Opts,
    dry: bool,
    mk: &mut dyn FnMut() -> Smt,
) -> FxHashMap<FnKey, bool> {
    let mut used = vec![];
    for c in prog.contracts.iter().filter(|c| !c.kind.is_interface()) {
        for (n, sp) in &c.broadcast_use {
            let _ = resolve(prog, c, n, *sp, &mut used);
        }
        for f in &c.funcs {
            used.extend(all_active(prog, c, f));
        }
    }
    let mut out = FxHashMap::default();
    for l in used {
        if dry {
            out.insert(key(l.1), true);
        } else {
            prove(prog, l, opts, mk, &mut out);
        }
    }
    out
}

fn prove<'p, 'a>(
    prog: &'p Program<'a>,
    (c, f): Lemma<'p, 'a>,
    opts: &Opts,
    mk: &mut dyn FnMut() -> Smt,
    out: &mut FxHashMap<FnKey, bool>,
) {
    let k = key(f);
    if out.contains_key(&k) {
        return;
    }
    // Not proved while its proof runs.
    out.insert(k.clone(), false);
    let home = prog.contract(&f.contract).unwrap_or(c);
    for m in all_active(prog, home, f) {
        if key(m.1) != k && !reaches(prog, m, &k) {
            prove(prog, m, opts, mk, out);
        }
    }
    crate::lean::set_unit(format!("{}.{} [lemma]", f.contract, f.name));
    let mut v = Vc::new(prog, home, mk(), opts);
    v.proved = out.clone();
    let rep = v.verify_fn(f);
    out.insert(k, rep.fails.is_empty() && rep.unsupported.is_none());
}

/// The `:pattern`s of broadcast lemma `name`'s axiom over the binder symbols `syms`
/// (`(parameter, symbol)`): one per `@custom:trigger` tag (`explicit`, each term with its
/// SMT form), else one per innermost application of an uninterpreted function in `post` that
/// mentions every binder.
pub(crate) fn patterns(
    name: &str,
    syms: &[(String, String)],
    post: &str,
    explicit: &[Vec<(&Spec<'_>, String)>],
    span: Span,
) -> R<Vec<String>> {
    let parse = |t: &str| crate::sexp::parse(t).next().and_then(|r| r.ok());
    let covers = |found: &FxHashSet<String>| syms.iter().find(|(_, s)| !found.contains(s));
    if !explicit.is_empty() {
        let mut out = vec![];
        for group in explicit {
            let mut found = FxHashSet::default();
            let mut terms = vec![];
            for (sp, t) in group {
                let s = parse(t).filter(is_app).ok_or_else(|| {
                    (
                        sp.span,
                        format!(
                            "trigger `{}` of broadcast lemma `{name}` must apply an uninterpreted function (an opaque spec function, or a pure function with more than a return); arithmetic and comparisons cannot trigger",
                            sp.text
                        ),
                    )
                })?;
                atoms(&s, &mut found);
                terms.push(t.clone());
            }
            if let Some((n, _)) = covers(&found) {
                return Err((
                    group[0].0.span,
                    format!("trigger of broadcast lemma `{name}` does not mention parameter `{n}`"),
                ));
            }
            out.push(format!("({})", terms.join(" ")));
        }
        return Ok(out);
    }
    let mut cands = vec![];
    if let Some(s) = parse(post) {
        candidates(
            &s,
            &|s: &Sexp| {
                let mut found = FxHashSet::default();
                atoms(s, &mut found);
                covers(&found).is_none()
            },
            &mut cands,
        );
    }
    let mut out: Vec<String> = vec![];
    for c in cands {
        let p = format!("({c})");
        if !out.contains(&p) {
            out.push(p);
        }
    }
    if out.is_empty() {
        return Err((
            span,
            format!(
                "broadcast lemma `{name}` has no trigger: no application of an uninterpreted function in its ensures mentions every parameter; name one with `@custom:trigger`"
            ),
        ));
    }
    Ok(out)
}

/// SMT operators with a fixed meaning, which a pattern cannot be headed by.
const INTERPRETED: &[&str] = &[
    "+", "-", "*", "/", "div", "mod", "abs", "=", "distinct", "<", "<=", ">", ">=", "and", "or",
    "not", "=>", "xor", "ite", "store", "let", "forall", "exists", "!", "_", "int2bv", "bv2nat",
    "bv2int", "concat", "extract",
];

/// An application of an uninterpreted function (or an array read) with no binder inside.
fn is_app(s: &Sexp) -> bool {
    let Some(h) = s.head() else { return false };
    !INTERPRETED.contains(&h) && !h.starts_with("bv") && !binds(s)
}

fn binds(s: &Sexp) -> bool {
    s.as_list().is_some_and(|l| {
        matches!(s.head(), Some("forall" | "exists" | "let")) || l.iter().any(binds)
    })
}

fn atoms(s: &Sexp, out: &mut FxHashSet<String>) {
    match s {
        Sexp::Atom(a) => {
            out.insert(a.clone());
        }
        Sexp::List(l) => l.iter().for_each(|x| atoms(x, out)),
    }
}

/// Pushes the innermost applications in `s` that satisfy `ok`, outside nested binders;
/// returns whether it pushed any.
fn candidates(s: &Sexp, ok: &dyn Fn(&Sexp) -> bool, out: &mut Vec<Sexp>) -> bool {
    let Some(l) = s.as_list() else { return false };
    if matches!(s.head(), Some("forall" | "exists" | "let")) {
        return false;
    }
    let mut inner = false;
    for x in l {
        inner |= candidates(x, ok, out);
    }
    if !inner && is_app(s) && ok(s) {
        out.push(s.clone());
        return true;
    }
    inner
}
