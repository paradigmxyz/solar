//! Horn-clause fallback for loop invariants.
//!
//! When a function still fails after Houdini, each loop the failure may depend on is encoded as
//! constrained Horn clauses over one unknown predicate `Inv` of the loop head's value-type
//! variables, and Z3's Spacer engine is asked for a solution:
//!
//! ```text
//! defs /\ entry_guard                       ==> Inv(entry terms)
//! defs /\ Inv(head terms) /\ back_guard      ==> Inv(terms after the body)
//! defs /\ Inv(head terms) /\ fail_guard     ==> goal
//! ```
//!
//! `defs` are the definitions and range facts of the function's real run (the encoder names every
//! intermediate term with a constant), sliced to the cone of each clause; Houdini's trial scopes
//! are left out. Only a small fragment is encoded: Int and Bool constants, no uninterpreted
//! functions, no quantifiers. Anything else, and an `unknown` or a timeout, gives nothing.
//!
//! Spacer's answer is never trusted: the formula it returns for `Inv` becomes one more Houdini
//! candidate of that loop, and the function is verified again, so the formula is checked by the
//! same initiation, preservation and use obligations as every other invariant.

use super::{
    Vc,
    infer::{Cand, LoopRec, Shape},
};
use crate::sexp::{self, Sexp};
use solar_parse::interface::{Span, data_structures::map::FxHashMap};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::{Command, Stdio},
};

/// Seconds Spacer gets per problem.
const SPACER_SECONDS: u64 = 5;

/// The most (loop, failure) problems one function tries.
const MAX_PROBLEMS: usize = 8;

/// The persistent part of a function's script, outside Houdini's trial scopes.
#[derive(Default)]
struct Script {
    consts: BTreeMap<String, String>,
    defs: BTreeMap<String, Sexp>,
    facts: Vec<(Sexp, BTreeSet<String>)>,
    funs: BTreeSet<String>,
    macros: Vec<(String, Sexp)>,
}

impl Script {
    fn parse(text: &str) -> Option<Self> {
        let mut s = Self::default();
        let mut depth = 0usize;
        for cmd in sexp::parse(text) {
            let cmd = cmd.ok()?;
            let Some(items) = cmd.as_list() else { continue };
            match cmd.head() {
                Some("push") => depth += 1,
                Some("pop") => depth = depth.saturating_sub(1),
                _ if depth > 0 => {}
                Some("declare-const") => {
                    let (Some(n), Some(t)) = (items.get(1)?.as_atom(), items.get(2)) else {
                        continue;
                    };
                    s.consts.insert(n.to_string(), t.to_string());
                }
                Some("declare-fun") | Some("declare-datatypes") => {
                    if let Some(n) = items.get(1).and_then(Sexp::as_atom) {
                        s.funs.insert(n.to_string());
                    }
                }
                Some("define-fun") => {
                    if let Some(n) = items.get(1).and_then(Sexp::as_atom) {
                        s.macros.push((n.to_string(), cmd.clone()));
                    }
                }
                Some("assert") => {
                    let t = items.get(1)?.clone();
                    if let Some([eq, x, _]) = t.as_list()
                        && eq.as_atom() == Some("=")
                        && let Some(x) = x.as_atom()
                        && s.consts.contains_key(x)
                        && !s.defs.contains_key(x)
                    {
                        s.defs.insert(x.to_string(), t);
                        continue;
                    }
                    let syms = atoms(&t);
                    s.facts.push((t, syms));
                }
                _ => {}
            }
        }
        Some(s)
    }

    /// The constants and assertions `terms` rest on, or None outside the fragment.
    fn cone(&self, terms: &[&Sexp]) -> Option<(BTreeSet<String>, Vec<Sexp>)> {
        let mut seen = BTreeSet::new();
        let mut work: Vec<String> = terms.iter().flat_map(|t| atoms(t)).collect();
        let mut out = vec![];
        while let Some(a) = work.pop() {
            if self.funs.contains(&a) {
                return None;
            }
            if !self.consts.contains_key(&a) || !seen.insert(a.clone()) {
                continue;
            }
            if let Some(d) = self.defs.get(&a) {
                work.extend(atoms(d));
                out.push(d.clone());
            }
        }
        for (f, syms) in &self.facts {
            if !syms.is_empty()
                && syms.iter().all(|x| seen.contains(x) || !self.consts.contains_key(x))
                && syms.iter().any(|x| seen.contains(x))
            {
                if syms.iter().any(|x| self.funs.contains(x)) {
                    return None;
                }
                out.push(f.clone());
            }
        }
        for c in &seen {
            let sort = &self.consts[c];
            if sort != "Int" && sort != "Bool" {
                return None;
            }
        }
        for t in out.iter().chain(terms.iter().copied()) {
            let text = t.to_string();
            if text.contains("(forall") || text.contains("(exists") || text.contains("select") {
                return None;
            }
        }
        Some((seen, out))
    }
}

/// The atoms of a term.
fn atoms(t: &Sexp) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![t];
    while let Some(t) = stack.pop() {
        match t {
            Sexp::Atom(a) => {
                out.insert(a.clone());
            }
            Sexp::List(xs) => stack.extend(xs.iter()),
        }
    }
    out
}

fn term(t: &str) -> Option<Sexp> {
    sexp::parse(t).next()?.ok()
}

/// One clause `body ==> head`, universally closed over the constants of its cone.
fn clause(
    s: &Script,
    body: &[String],
    head: &str,
    macros: &mut BTreeSet<String>,
) -> Option<String> {
    let parts: Vec<Sexp> = body
        .iter()
        .chain(std::iter::once(&head.to_string()))
        .map(|t| term(t))
        .collect::<Option<_>>()?;
    let refs: Vec<&Sexp> = parts.iter().collect();
    let (consts, defs) = s.cone(&refs)?;
    for t in refs.iter().copied().chain(defs.iter()) {
        for a in atoms(t) {
            if s.macros.iter().any(|(n, _)| *n == a) {
                macros.insert(a);
            }
        }
    }
    let binders: Vec<String> = consts.iter().map(|c| format!("({c} {})", s.consts[c])).collect();
    let mut conj: Vec<String> = defs.iter().map(|d| d.to_string()).collect();
    conj.extend(body.iter().cloned());
    let imp = format!("(=> (and true {}) {head})", conj.join(" "));
    Some(if binders.is_empty() {
        format!("(assert {imp})\n")
    } else {
        format!("(assert (forall ({}) {imp}))\n", binders.join(" "))
    })
}

/// The Horn problem of loop `l` with safety goal `goal` under `guard`.
fn problem(s: &Script, l: &LoopRec, guard: &str, goal: &str) -> Option<String> {
    if l.vars.is_empty() {
        return None;
    }
    let app = |ts: &[String]| format!("(|Inv| {})", ts.join(" "));
    let head: Vec<String> = l.vars.iter().map(|v| v.1.clone()).collect();
    let mut macros = BTreeSet::new();
    let mut clauses = String::new();
    clauses.push_str(&clause(s, &[l.entry_guard.clone()], &app(&l.entry_terms), &mut macros)?);
    clauses.push_str(&clause(
        s,
        &[app(&head), l.back_guard.clone()],
        &app(&l.back_terms),
        &mut macros,
    )?);
    clauses.push_str(&clause(s, &[app(&head), guard.to_string()], goal, &mut macros)?);
    let mut out = String::from("(set-logic HORN)\n");
    for (n, m) in &s.macros {
        if macros.contains(n) {
            out.push_str(&format!("{m}\n"));
        }
    }
    let sorts: Vec<&str> = l.vars.iter().map(|v| v.2.as_str()).collect();
    out.push_str(&format!("(declare-fun |Inv| ({}) Bool)\n", sorts.join(" ")));
    out.push_str(&clauses);
    out.push_str("(check-sat)\n(get-model)\n");
    Some(out)
}

/// Spacer's solution for `Inv`, as a candidate over the loop's variables.
fn solve<'p, 'a>(script: &str, l: &LoopRec) -> Option<Cand<'p, 'a>> {
    let z3 = std::env::var("SOLAR_VERIFY_Z3").unwrap_or_else(|_| "z3".into());
    let mut child = Command::new(z3)
        .args(["-in", "-smt2", &format!("-T:{SPACER_SECONDS}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(script.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
        eprintln!("horn: problem\n{script}horn: answer\n{text}");
    }
    let mut it = sexp::parse(&text);
    if it.next()?.ok()?.as_atom() != Some("sat") {
        return None;
    }
    let model = it.next()?.ok()?;
    let mut stack = vec![&model];
    while let Some(t) = stack.pop() {
        let Some(xs) = t.as_list() else { continue };
        if t.head() == Some("define-fun")
            && xs.get(1).and_then(Sexp::as_atom).is_some_and(|n| n.trim_matches('|') == "Inv")
        {
            let params = xs.get(2)?.as_list()?;
            let body = xs.get(4)?;
            let mut place = FxHashMap::default();
            let mut shown = FxHashMap::default();
            for (p, v) in params.iter().zip(&l.vars) {
                let n = p.as_list()?.first()?.as_atom()?.to_string();
                place.insert(n.clone(), format!("|$v:{}|", v.0));
                shown.insert(n, v.0.clone());
            }
            let t = rename(body, &place).to_string();
            let text = rename(body, &shown).to_string();
            return Some(Cand { shape: Shape::Term(t), text, origin: "horn" });
        }
        stack.extend(xs.iter());
    }
    None
}

fn rename(t: &Sexp, m: &FxHashMap<String, String>) -> Sexp {
    match t {
        Sexp::Atom(a) => Sexp::Atom(m.get(a).cloned().unwrap_or_else(|| a.clone())),
        Sexp::List(xs) => Sexp::List(xs.iter().map(|x| rename(x, m)).collect()),
    }
}

impl<'p, 'a> Vc<'p, 'a> {
    /// Candidates Spacer proposes for the loops of the function whose real run is `transcript`,
    /// against the obligations that failed in it.
    pub(super) fn horn_fallback(&self, transcript: &str) -> FxHashMap<Span, Vec<Cand<'p, 'a>>> {
        let mut out: FxHashMap<Span, Vec<Cand<'p, 'a>>> = FxHashMap::default();
        let Some(script) = Script::parse(transcript) else { return out };
        let mut tried = 0;
        for l in &self.loop_recs {
            for (guard, goal) in &self.failed_goals {
                if tried >= MAX_PROBLEMS {
                    return out;
                }
                let Some(p) = problem(&script, l, guard, goal) else { continue };
                tried += 1;
                if let Some(c) = solve(&p, l) {
                    out.entry(l.span).or_default().push(c);
                }
            }
        }
        out
    }
}
