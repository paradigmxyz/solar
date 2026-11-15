//! `--emit-loopvc DIR`: a function's loop cut as a frozen interface, format 1 (see
//! `docs/loopvc-interface.md`).
//!
//! The encoder cuts a loop at its head (`crate::vc::infer`): the loop's variables are havocked,
//! the candidate invariant is assumed once, as one conjunction, the body runs once and the
//! candidate must hold again. On the real run it records here where the candidate entered the
//! script (one assertion, the head assumption), which queries check it on entry and after the
//! body, which check the ranking function, and where the run's paths revert. From that and the
//! unit's recording this module writes, next to the unit's `--emit-lean` module:
//!
//! - `<unit>.loopvc.json`: the cutpoints, the edges between them and, per edge, the templates of
//!   its obligations (the run's own obligations with the candidate replaced by the predicate
//!   `|Inv@L0|` and the ranking function `|Rank@L0|` over the cut's parameters), the candidate of
//!   the run, and the revert paths;
//! - `<unit>.horn.smt2`: the same templates as constrained Horn clauses over `|Inv@L0|`;
//! - `<unit>_loopvc.lean`: the templates as Lean terms, with a theorem per template, proved by
//!   `rfl`, that its instances at the run's candidate are exactly the obligations of the unit's
//!   `--emit-lean` module.
//!
//! Every template is checked here before it is written: a query whose goal is not the candidate
//! substituted at the recorded arguments, a head assumption not where the run recorded it, or a
//! candidate over constants that are not the cut's parameters is an error, never a dropped edge.

use super::{R, Recorder, Scope, Srt, lean_string, location, sexp_of, sha256, translate};
use crate::sexp::Sexp;
use solar_parse::interface::{Session, Span};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::Path,
};

/// The version of the loop-cut format.
pub(crate) const FORMAT_VERSION: u32 = 1;

/// The Lean definitions every exported loop-cut module imports, written to `DIR/LoopVc.lean`.
pub(crate) const LIBRARY: &str = include_str!("LoopVc.lean");

/// Where `LIBRARY` is written in the export directory.
pub(crate) const LIBRARY_FILE: &str = "LoopVc.lean";

/// The cut's invariant predicate and ranking function.
const INV: &str = "|Inv@L0|";
const RANK: &str = "|Rank@L0|";

/// A parameter of the cut: a constant of the function's script the candidate ranges over.
#[derive(Clone, Debug, Default)]
pub(crate) struct Param {
    /// The variable, or `old(x)` for the function-entry value of a variable the loop writes.
    pub name: String,
    /// Its constant at the head.
    pub symbol: String,
    pub sort: String,
    /// `loop` (written by the loop), `read` or `old`.
    pub role: &'static str,
    /// Its term where the loop is entered, and after the body (the back edge).
    pub entry: String,
    pub back: String,
}

/// A conjunct of the candidate: its term at the head.
#[derive(Clone, Debug, Default)]
pub(crate) struct Conjunct {
    pub term: String,
    pub text: String,
    /// `annotation` or `inferred:<origin>`.
    pub origin: String,
    pub span: Option<Span>,
}

/// The candidate's ranking function: its term at the head and the run's term after the body.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rank {
    pub term: String,
    pub back: String,
    pub text: String,
    pub origin: String,
    pub span: Option<Span>,
}

/// The head assumption: the command of script `script` that asserts `after = (and before C)`,
/// `C` the candidate's conjunction.
#[derive(Clone, Debug, Default)]
pub(crate) struct Head {
    pub script: usize,
    pub cmd: usize,
    pub before: String,
    pub after: String,
}

/// A loop cut of the real run.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cut {
    pub span: Option<Span>,
    pub kind: &'static str,
    pub params: Vec<Param>,
    pub conj: Vec<Conjunct>,
    pub rank: Option<Rank>,
    pub head: Option<Head>,
    /// (query, conjunct) of the checks on entry and after the body.
    pub entry: Vec<(usize, usize)>,
    pub back: Vec<(usize, usize)>,
    /// The queries checking the ranking function.
    pub rank_q: Vec<usize>,
    /// The first query after the head assumption, and after the loop's exit.
    pub head_q: usize,
    pub exit_q: usize,
    /// The recorder's position at the head assumption and at the loop's exit.
    pub head_seq: usize,
    pub exit_seq: usize,
    /// Why the cut cannot be exported.
    pub error: Option<String>,
}

/// Where a path of the real run reverts: the guard of the reverting state, at a position.
#[derive(Clone, Debug)]
pub(crate) struct Site {
    pub seq: usize,
    pub script: usize,
    pub upto: usize,
    pub guard: String,
}

/// `t` with every atom `m` binds replaced.
fn subst(t: &Sexp, m: &BTreeMap<String, Sexp>) -> Sexp {
    match t {
        Sexp::Atom(a) => m.get(a).cloned().unwrap_or_else(|| t.clone()),
        Sexp::List(xs) => Sexp::List(xs.iter().map(|x| subst(x, m)).collect()),
    }
}

/// The symbols of `t` in argument positions (not applied as functions) that look like
/// constants of the script.
fn constants(t: &Sexp, out: &mut BTreeSet<String>) {
    match t {
        Sexp::Atom(a) if a.starts_with('|') => {
            out.insert(a.clone());
        }
        Sexp::Atom(_) => {}
        Sexp::List(xs) => {
            for (i, x) in xs.iter().enumerate() {
                if i == 0 && x.as_atom().is_some() {
                    continue;
                }
                constants(x, out);
            }
        }
    }
}

/// The candidate's conjunction as the encoder assumes it.
fn conjoin(ts: &[String]) -> String {
    match ts {
        [t] => t.clone(),
        _ => format!("(and {})", ts.join(" ")),
    }
}

fn app(f: &str, args: &[String]) -> String {
    format!("({f} {})", args.join(" "))
}

/// One obligation template.
struct Template {
    /// Its edge and what it checks: `entry`, `back`, `rank` or `path` (an obligation on
    /// the way, whose goal does not mention the cut).
    edge: &'static str,
    role: &'static str,
    script: usize,
    upto: usize,
    guard: String,
    goal: String,
    /// The run's queries it stands for, in order.
    instances: Vec<usize>,
}

/// Writes the loop-cut files of unit `u` (module `module`, manifest entry `unit`), or nothing
/// when the unit has no loop.
pub(super) fn write_unit(
    sess: &Session,
    dir: &Path,
    u: &Recorder,
    module: &str,
    unit: &serde_json::Value,
) -> R<Option<serde_json::Value>> {
    let cut = match u.cuts.as_slice() {
        [] => return Ok(None),
        [c] => c,
        cs => {
            return Err(format!(
                "{} loops: format {FORMAT_VERSION} exports functions with one loop",
                cs.len()
            ));
        }
    };
    if let Some(e) = &cut.error {
        return Err(e.clone());
    }
    if cut.conj.is_empty() {
        return Err("the loop has no invariant: no candidate to cut at".into());
    }
    let head = cut.head.as_ref().ok_or("the loop head is unreachable on the run: no cut")?;
    if cut.params.is_empty() {
        return Err("the cut has no parameters".into());
    }
    let symbols: BTreeSet<String> = cut.params.iter().map(|p| p.symbol.clone()).collect();
    if symbols.len() != cut.params.len() {
        return Err("two parameters of the cut are one constant".into());
    }
    for s in &u.scripts {
        if s.iter().any(|c| c.contains(INV) || c.contains(RANK)) {
            return Err(format!("the script already names {INV} or {RANK}"));
        }
    }
    // The candidate ranges over the parameters only.
    let mut terms: Vec<&str> = cut.conj.iter().map(|c| c.term.as_str()).collect();
    if let Some(r) = &cut.rank {
        terms.push(&r.term);
    }
    for t in terms {
        let mut cs = BTreeSet::new();
        constants(&sexp_of(t)?, &mut cs);
        if let Some(c) = cs.iter().find(|c| !symbols.contains(*c)) {
            return Err(format!("the candidate term {t} mentions {c}, not a parameter of the cut"));
        }
    }
    let sub = |args: &[String]| -> R<BTreeMap<String, Sexp>> {
        cut.params.iter().zip(args).map(|(p, a)| Ok((p.symbol.clone(), sexp_of(a)?))).collect()
    };
    let head_args: Vec<String> = cut.params.iter().map(|p| p.symbol.clone()).collect();
    let entry_args: Vec<String> = cut.params.iter().map(|p| p.entry.clone()).collect();
    let back_args: Vec<String> = cut.params.iter().map(|p| p.back.clone()).collect();
    let conj_terms: Vec<String> = cut.conj.iter().map(|c| c.term.clone()).collect();
    let head_cmd = |phi: &str| -> R<String> {
        Ok(sexp_of(&format!("(assert (= {} (and {} {phi})))", head.after, head.before))?
            .to_string())
    };
    let original = head_cmd(&conjoin(&conj_terms))?;
    let template_head = head_cmd(&app(INV, &head_args))?;
    if u.scripts.get(head.script).and_then(|s| s.get(head.cmd)) != Some(&original) {
        return Err("the head assumption is not where the run recorded it".into());
    }
    // The template of each script: the head assumption, where the script holds it, with the cut's
    // predicate for the candidate.
    let template_cmds = |s: usize| -> Vec<String> {
        let mut cmds = u.scripts[s].clone();
        if cmds.get(head.cmd) == Some(&original) {
            cmds[head.cmd] = template_head.clone();
        }
        cmds
    };
    let obs = unit["obligations"].as_array().ok_or("the unit has no obligations")?;
    let entry: BTreeMap<usize, usize> = cut.entry.iter().copied().collect();
    let back: BTreeMap<usize, usize> = cut.back.iter().copied().collect();
    let rank_q: BTreeSet<usize> = cut.rank_q.iter().copied().collect();
    let mut templates: Vec<Template> = vec![];
    // The checks of the candidate on one edge: one template whose goal is the predicate at the
    // edge's arguments, which must give each check's goal for its conjunct.
    let group =
        |checks: &BTreeMap<usize, usize>, args: &[String], edge: &'static str| -> R<Template> {
            let qs: Vec<(usize, usize)> = checks.iter().map(|(q, j)| (*q, *j)).collect();
            let js: Vec<usize> = qs.iter().map(|q| q.1).collect();
            if js != (0..cut.conj.len()).collect::<Vec<_>>() {
                return Err(format!(
                    "the {edge} checks do not cover the candidate's conjuncts in order"
                ));
            }
            let first = &u.queries[qs[0].0];
            let m = sub(args)?;
            for (q, j) in &qs {
                let qq = &u.queries[*q];
                if (qq.script, qq.upto, &qq.guard) != (first.script, first.upto, &first.guard) {
                    return Err(format!("the {edge} checks read different prefixes of the script"));
                }
                let want = subst(&sexp_of(&cut.conj[*j].term)?, &m);
                if sexp_of(&qq.goal)? != want {
                    return Err(format!(
                        "{edge} check {} is not conjunct {j} at the edge's arguments: {} vs {want}",
                        qq.goal, u.unit
                    ));
                }
            }
            Ok(Template {
                edge,
                role: "invariant",
                script: first.script,
                upto: first.upto,
                guard: first.guard.clone(),
                goal: app(INV, args),
                instances: qs.iter().map(|q| q.0).collect(),
            })
        };
    let entry_t = group(&entry, &entry_args, "entry")?;
    let back_t = group(&back, &back_args, "back")?;
    let mut rank_t = None;
    for &q in &rank_q {
        let r = cut.rank.as_ref().ok_or("a ranking check without a ranking function")?;
        let qq = &u.queries[q];
        let t1 = subst(&sexp_of(&r.term)?, &sub(&back_args)?);
        if t1 != sexp_of(&r.back)? {
            return Err(format!("the ranking function after the body is {}, not {t1}", r.back));
        }
        let want = sexp_of(&format!("(and (<= 0 {}) (< {t1} {}))", r.term, r.term))?;
        if sexp_of(&qq.goal)? != want {
            return Err(format!("the ranking check {} is not the ranking function's", qq.goal));
        }
        let (rh, rb) = (app(RANK, &head_args), app(RANK, &back_args));
        rank_t = Some(Template {
            edge: "back",
            role: "rank",
            script: qq.script,
            upto: qq.upto,
            guard: qq.guard.clone(),
            goal: format!("(and (<= 0 {rh}) (< {rb} {rh}))"),
            instances: vec![q],
        });
    }
    if rank_q.len() > 1 {
        return Err("several ranking checks".into());
    }
    for (i, q) in u.queries.iter().enumerate() {
        if obs.get(i).is_none_or(|o| o["role"] != "obligation")
            || entry.contains_key(&i)
            || back.contains_key(&i)
            || rank_q.contains(&i)
        {
            continue;
        }
        let edge = if i < cut.head_q {
            "entry"
        } else if i < cut.exit_q {
            "back"
        } else {
            "exit"
        };
        templates.push(Template {
            edge,
            role: "path",
            script: q.script,
            upto: q.upto,
            guard: q.guard.clone(),
            goal: q.goal.clone(),
            instances: vec![i],
        });
    }
    // Order: entry, back, exit; on each edge the obligations on the way, then the cut's.
    let mut ordered: Vec<Template> = vec![];
    let mut take = |edge: &str, ordered: &mut Vec<Template>| {
        let (mine, rest): (Vec<_>, Vec<_>) = templates.drain(..).partition(|t| t.edge == edge);
        templates = rest;
        ordered.extend(mine);
    };
    take("entry", &mut ordered);
    ordered.push(entry_t);
    take("back", &mut ordered);
    ordered.push(back_t);
    ordered.extend(rank_t);
    take("exit", &mut ordered);
    let templates = ordered;
    // Every template's script must hold the head where its prefix passes it.
    for t in &templates {
        let cmds = &u.scripts[t.script];
        if t.upto > head.cmd && cmds.get(head.cmd) != Some(&original) {
            return Err("an obligation reads past the head in a script without the head".into());
        }
    }

    // Revert paths, by where they leave: before the head, in the body, after the exit.
    let reverts: Vec<(&'static str, &Site)> = u
        .reverts
        .iter()
        .map(|s| {
            let from = if s.seq < cut.head_seq {
                "entry"
            } else if s.seq < cut.exit_seq {
                "body"
            } else {
                "exit"
            };
            (from, s)
        })
        .collect();

    // Lean: one template script per script the templates read.
    let path = module.replace('.', "/");
    let ns = format!("SolarVerifyLoopVc.{module}");
    let vc_ns = format!("SolarVerifyVc.{module}");
    let sorts: Vec<&str> = cut.params.iter().map(|p| p.sort.as_str()).collect();
    let used: BTreeSet<usize> = templates.iter().map(|t| t.script).collect();
    let mut lean = format!(
        "-- Generated by solar-verify --emit-loopvc (format {FORMAT_VERSION}); do not edit.\n\
         import LoopVc\n\
         import {module}\n\n\
         set_option maxRecDepth 100000\n\n\
         namespace {ns}\nopen SolarVerify.Smt SolarVerify.Vc SolarVerify.LoopVc\n\n"
    );
    let mut scopes: BTreeMap<usize, (Scope, Vec<usize>, usize)> = BTreeMap::new();
    let mut param_ids: Option<Vec<usize>> = None;
    for &s in &used {
        let cmds = &u.scripts[s];
        let t = translate(cmds)?;
        let base = t.scope.decls.len();
        let mut scope = t.scope;
        let decl = |name: &str, ret: &str| {
            sexp_of(&format!("(declare-fun {name} ({}) {ret})", sorts.join(" ")))
        };
        scope.command(&decl(INV, "Bool")?)?;
        scope.command(&decl(RANK, "Int")?)?;
        let ids: Vec<usize> = cut
            .params
            .iter()
            .map(|p| {
                scope.by_name.get(&p.symbol).copied().ok_or(format!("{} undeclared", p.symbol))
            })
            .collect::<R<_>>()?;
        match &param_ids {
            Some(p) if *p != ids => return Err("the parameters differ between scripts".into()),
            _ => param_ids = Some(ids),
        }
        let mut asserts = t.asserts.clone();
        if cmds.get(head.cmd) == Some(&original) {
            let h = sexp_of(&template_head)?;
            let body = h.as_list().and_then(|l| l.get(1)).ok_or("bad head")?;
            asserts[t.asserts_upto[head.cmd]] = scope.term(body, &mut vec![])?;
        }
        let _ = writeln!(
            lean,
            "/-- Script {s} of {} with the cut's candidate abstracted: {INV} in the head \
             assumption, and the cut's symbols declared last. -/",
            u.unit
        );
        let _ = writeln!(lean, "def tscript{s} : Script where\n  decls := [");
        for (j, d) in scope.decls.iter().enumerate() {
            let args = d.args.iter().map(Srt::lean).collect::<Vec<_>>().join(", ");
            let _ = writeln!(
                lean,
                "    ⟨{}, [{args}], {}⟩{}",
                lean_string(&d.name),
                d.ret.lean(),
                if j + 1 < scope.decls.len() { "," } else { "" }
            );
        }
        let _ = writeln!(lean, "  ]\n  asserts := [");
        for (j, a) in asserts.iter().enumerate() {
            let _ = writeln!(lean, "    {a}{}", if j + 1 < asserts.len() { "," } else { "" });
        }
        let _ = writeln!(lean, "  ]\n");
        scopes.insert(s, (scope, t.asserts_upto, base));
    }
    let params = param_ids.unwrap_or_default();
    let (hscope, _, _) = scopes.get(&templates[0].script).ok_or("no script")?;
    let term = |scope: &Scope, t: &str| -> R<String> { scope.term(&sexp_of(t)?, &mut vec![]) };
    let _ = writeln!(
        lean,
        "/-- The cut: the constants the candidate ranges over, in the order of {INV}'s \
         arguments. -/\ndef cut : Cut where\n  params := [{}]\n",
        params.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
    );
    let conj_lean: Vec<String> = conj_terms.iter().map(|t| term(hscope, t)).collect::<R<_>>()?;
    let rank_lean = match &cut.rank {
        Some(r) => format!("some ({})", term(hscope, &r.term)?),
        None => "none".into(),
    };
    let _ = writeln!(
        lean,
        "/-- The candidate of the run. -/\ndef cand : Cand where\n  conj := [\n    {}\n  ]\n  rank := {rank_lean}\n",
        conj_lean.join(",\n    ")
    );
    let mut json_templates = vec![];
    for (n, t) in templates.iter().enumerate() {
        let (scope, asserts_upto, base) = &scopes[&t.script];
        let upto = asserts_upto[t.upto];
        let _ = writeln!(
            lean,
            "/-- Template {n}: {} edge, {}. -/\ndef t{n} : Template where\n  script := tscript{}\n  base := {base}\n  upto := {upto}\n  guard := {}\n  goal := {}\n",
            t.edge,
            t.role,
            t.script,
            term(scope, &t.guard)?,
            term(scope, &t.goal)?
        );
        let inst: Vec<String> = t.instances.iter().map(|i| format!("{vc_ns}.ob{i}")).collect();
        let _ = writeln!(
            lean,
            "theorem t{n}_exact : t{n}.instances cut cand = [{}] := rfl\n",
            inst.join(", ")
        );
        let _ = writeln!(
            lean,
            "theorem t{n}_valid : t{n}.Valid cut cand ↔ ∀ o ∈ [{}], o.Valid :=\n  Template.valid_iff t{n}_exact\n",
            inst.join(", ")
        );
        let ids: Vec<serde_json::Value> = t
            .instances
            .iter()
            .map(|i| {
                let o = &obs[*i];
                serde_json::json!({
                    "id": o["id"],
                    "lean": o["lean"],
                    "kind": o["kind"],
                    "site": o["site"],
                    "spec": o["spec"],
                })
            })
            .collect();
        json_templates.push(serde_json::json!({
            "id": format!("t{n}"),
            "edge": t.edge,
            "role": t.role,
            "script": t.script,
            "commands": t.upto,
            "asserts": upto,
            "guard": t.guard,
            "goal": t.goal,
            "lean": format!("{ns}.t{n}"),
            "instances": ids,
        }));
    }
    let _ = writeln!(lean, "end {ns}");

    // The Horn view: one clause per template of the invariant, the predicate in its head or
    // a goal to refute; the revert paths as clauses of the nullary `|Reverted|`.
    let mut horn = format!(
        "; Generated by solar-verify --emit-loopvc (format {FORMAT_VERSION}); do not edit.\n\
         ; {}: constrained Horn clauses over {INV} ({}), one per obligation template of the\n\
         ; cut that does not mention {RANK}, then one per revert path. See loopvc.json.\n\
         (set-logic HORN)\n",
        u.unit,
        cut.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
    );
    let mut defined = BTreeSet::new();
    // A clause's body is its prefix's assertions and its guard. The predicate must occur in it
    // only as a conjunct, so the head assumption `after = (and before Inv(..))` becomes
    // `after = before` and the conjunct `Inv(..)`: the same body wherever the guard implies
    // `after`, which is checked on the guard's definitions.
    let horn_script = |s: usize,
                       upto: usize,
                       guard: &str|
     -> R<(Vec<String>, Vec<String>, Vec<String>)> {
        let (mut binders, mut asserts, mut defs) = (vec![], vec![], vec![]);
        let mut guard_defs: BTreeMap<String, Sexp> = BTreeMap::new();
        let mut cut_at_head = false;
        for (i, c) in template_cmds(s)[..upto].iter().enumerate() {
            let x = sexp_of(c)?;
            let items = x.as_list().unwrap_or(&[]);
            match x.head() {
                Some("declare-const") if items.len() == 3 => {
                    binders.push(format!("({} {})", items[1], items[2]))
                }
                Some("assert") if items.len() == 2 => {
                    if let Some([eq, g, d]) = items[1].as_list()
                        && eq.as_atom() == Some("=")
                        && let Some(g) = g.as_atom()
                    {
                        guard_defs.insert(g.to_string(), d.clone());
                    }
                    if i == head.cmd && s == head.script {
                        cut_at_head = true;
                        asserts.push(format!("(= {} {})", head.after, head.before));
                        asserts.push(app(INV, &head_args));
                    } else {
                        asserts.push(items[1].to_string());
                    }
                }
                Some("define-fun") => defs.push(c.clone()),
                _ => return Err(format!("the Horn view has no clause form for {c}")),
            }
        }
        fn implies(g: &Sexp, after: &str, defs: &BTreeMap<String, Sexp>, depth: usize) -> bool {
            if depth > 10_000 {
                return false;
            }
            match g {
                Sexp::Atom(a) if a == after => true,
                Sexp::Atom(a) => defs.get(a).is_some_and(|d| implies(d, after, defs, depth + 1)),
                Sexp::List(xs) => match xs.first().and_then(Sexp::as_atom) {
                    Some("and") => xs[1..].iter().any(|x| implies(x, after, defs, depth + 1)),
                    Some("or") => {
                        xs.len() > 1 && xs[1..].iter().all(|x| implies(x, after, defs, depth + 1))
                    }
                    _ => false,
                },
            }
        }
        if cut_at_head && !implies(&sexp_of(guard)?, &head.after, &guard_defs, 0) {
            return Err(format!(
                "a clause past the head has guard {guard}, which does not imply the head's {}",
                head.after
            ));
        }
        Ok((binders, asserts, defs))
    };
    let mut clauses = String::new();
    let clause = |binders: &[String], body: &[String], headf: &str| {
        let imp = format!("(=> (and {}) {headf})", body.join(" "));
        if binders.is_empty() {
            format!("(assert {imp})\n")
        } else {
            format!("(assert (forall ({}) {imp}))\n", binders.join(" "))
        }
    };
    for (n, t) in templates.iter().enumerate() {
        if t.role == "rank" {
            continue;
        }
        let (binders, mut body, defs) = horn_script(t.script, t.upto, &t.guard)?;
        for d in defs {
            if defined.insert(d.clone()) {
                horn.push_str(&d);
                horn.push('\n');
            }
        }
        body.push(t.guard.clone());
        let headf = if t.role == "invariant" {
            t.goal.clone()
        } else {
            body.push(format!("(not {})", t.goal));
            "false".into()
        };
        let _ = writeln!(clauses, "; t{n}: {} edge, {}", t.edge, t.role);
        clauses.push_str(&clause(&binders, &body, &headf));
    }
    let mut json_reverts = vec![];
    for (n, (from, s)) in reverts.iter().enumerate() {
        let (binders, mut body, defs) = horn_script(s.script, s.upto, &s.guard)?;
        for d in defs {
            if defined.insert(d.clone()) {
                horn.push_str(&d);
                horn.push('\n');
            }
        }
        body.push(s.guard.clone());
        let _ = writeln!(clauses, "; r{n}: revert path leaving the {from}");
        clauses.push_str(&clause(&binders, &body, "|Reverted|"));
        let asserts = translate(&u.scripts[s.script][..s.upto])?.asserts.len();
        json_reverts.push(serde_json::json!({
            "id": format!("r{n}"),
            "from": from,
            "script": s.script,
            "commands": s.upto,
            "asserts": asserts,
            "guard": s.guard,
        }));
    }
    let _ = writeln!(horn, "(declare-fun {INV} ({}) Bool)", sorts.join(" "));
    horn.push_str("(declare-fun |Reverted| () Bool)\n");
    horn.push_str(&clauses);
    horn.push_str("(check-sat)\n");

    let horn_file = format!("{path}.horn.smt2");
    let lean_file = format!("{path}_loopvc.lean");
    let json_file = format!("{path}.loopvc.json");
    std::fs::write(dir.join(&horn_file), &horn).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&lean_file), &lean).map_err(|e| e.to_string())?;

    let loc = |sp: Option<Span>| sp.map_or(serde_json::Value::Null, |sp| location(sess, sp));
    let params_json: Vec<serde_json::Value> = cut
        .params
        .iter()
        .zip(&params)
        .map(|(p, id)| {
            serde_json::json!({
                "name": p.name,
                "symbol": p.symbol,
                "lean_symbol": id,
                "sort": p.sort,
                "role": p.role,
                "entry": p.entry,
                "back": p.back,
            })
        })
        .collect();
    let cut_json = serde_json::json!({
        "id": "L0",
        "loop": cut.kind,
        "site": loc(cut.span),
        "predicate": INV,
        "rank": RANK,
        "params": params_json,
        "head": {
            "script": head.script,
            "command": head.cmd,
            "guard_before": head.before,
            "guard_after": head.after,
            "template": template_head,
        },
    });
    let candidate = serde_json::json!({
        "conjuncts": cut.conj.iter().map(|c| serde_json::json!({
            "term": c.term,
            "text": c.text,
            "origin": c.origin,
            "site": loc(c.span),
        })).collect::<Vec<_>>(),
        "rank": cut.rank.as_ref().map(|r| serde_json::json!({
            "term": r.term,
            "text": r.text,
            "origin": r.origin,
            "site": loc(r.span),
        })),
        "head_assumption": original,
    });
    let edges = serde_json::json!([
        { "id": "entry", "from": "pre", "to": "L0", "outcome": null,
          "templates": json_templates.iter().filter(|t| t["edge"] == "entry").map(|t| t["id"].clone()).collect::<Vec<_>>() },
        { "id": "back", "from": "L0", "to": "L0", "outcome": null,
          "templates": json_templates.iter().filter(|t| t["edge"] == "back").map(|t| t["id"].clone()).collect::<Vec<_>>() },
        { "id": "exit", "from": "L0", "to": "return", "outcome": "normal",
          "templates": json_templates.iter().filter(|t| t["edge"] == "exit").map(|t| t["id"].clone()).collect::<Vec<_>>() },
        { "id": "revert", "from": ["pre", "L0"], "to": "revert", "outcome": "revert",
          "paths": json_reverts.iter().map(|r| r["id"].clone()).collect::<Vec<_>>() },
        { "id": "call", "from": "L0", "to": "L0", "outcome": null, "templates": [] },
    ]);
    // What does not depend on the candidate or the source's layout: the formulas of the cut.
    let formulas = serde_json::json!({
        "params": cut_json["params"],
        "head": cut_json["head"]["template"],
        "templates": json_templates.iter().map(|t| {
            let mut t = t.clone();
            if let Some(o) = t.as_object_mut() {
                o.remove("instances");
            }
            t
        }).collect::<Vec<_>>(),
        "reverts": json_reverts,
    });
    let interface = serde_json::json!({
        "cut": cut_json,
        "edges": edges,
        "templates": json_templates,
        "reverts": json_reverts,
    });
    let canon =
        |v: &serde_json::Value| sha256(serde_json::to_string(v).unwrap_or_default().as_bytes());
    let out = serde_json::json!({
        "format": "solar-verify-loopvc",
        "version": FORMAT_VERSION,
        "unit": u.unit,
        "obligations": { "module": format!("{path}.lean"), "namespace": vc_ns },
        "lean": {
            "module": lean_file,
            "namespace": ns,
            "library": LIBRARY_FILE,
            "library_sha256": sha256(LIBRARY.as_bytes()),
        },
        "horn": { "file": horn_file, "sha256": sha256(horn.as_bytes()) },
        "cutpoints": [
            { "id": "pre", "kind": "function entry" },
            { "id": "L0", "kind": "loop head", "site": loc(cut.span) },
            { "id": "return", "kind": "function exit", "outcome": "normal" },
            { "id": "revert", "kind": "function exit", "outcome": "revert" },
        ],
        "cut": interface["cut"],
        "edges": interface["edges"],
        "templates": interface["templates"],
        "reverts": interface["reverts"],
        "candidate": candidate,
        "sha256": { "formulas": canon(&formulas), "candidate": canon(&candidate) },
    });
    let mut text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(dir.join(&json_file), text).map_err(|e| e.to_string())?;
    Ok(Some(serde_json::json!({
        "file": json_file,
        "lean": lean_file,
        "horn": horn_file,
        "templates": templates.len(),
    })))
}
