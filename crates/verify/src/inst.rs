//! Counterexamples for queries that quantified assumptions leave undecided.
//!
//! Z3 answers unknown ("incomplete quantifiers") when a query assumes a quantified fact, such as
//! a contract invariant over every address, and its model-based instantiation cannot show that a
//! candidate model satisfies it. Such a query is often falsifiable at a handful of addresses: the
//! ones the function touches. [`relax`] rewrites the script so that each quantifier holds only
//! at the query's ground terms (a finite conjunction, or disjunction for `exists`), which a
//! quantifier-free solver decides at once; [`pins`] turns the model it finds into assertions
//! that fix every constant to its value there.
//!
//! The relaxed script is only a search for a candidate: it neither weakens nor strengthens what is
//! reported. The caller re-asks the original, fully quantified query with the constants pinned,
//! and reports a counterexample only when the solver confirms that query satisfiable. With every
//! constant fixed, each quantifier ranges over known values, which model-based instantiation
//! checks directly. A candidate the original query rejects, or that it cannot settle, leaves the
//! answer unknown.

use crate::sexp::{self, Sexp};
use std::collections::{HashMap, HashSet};

/// The most instances one quantifier gets: with n bound variables, each ranges over the first
/// `CAP^(1/n)` candidate terms.
const CAP: usize = 512;

/// A relaxed script and what [`pins`] needs to read its model.
pub(crate) struct Relaxed {
    /// The script: declarations, the instantiated assertions, `check-sat`, `get-model` and a
    /// `get-value` of `terms`.
    pub script: String,
    /// The constants live at the end of the script (unquoted).
    live: HashSet<String>,
    /// Array constants the script does not define as a term of others (an assumed state rather
    /// than one computed from it), with their sorts.
    inputs: Vec<(String, Sexp)>,
    /// Each ground read `(select .. (select a i0) .. ik)` of an input array down to a scalar.
    /// Their values follow in the `get-value`, read first and index terms after, in order.
    reads: Vec<Read>,
}

/// A read of an input array: the array, the index terms, the read itself, and whether the
/// original script makes it (rather than only an instance of a quantifier).
type Read = (String, Vec<Sexp>, Sexp, bool);

/// The candidates [`pins`] makes of a model of the relaxed script, in the order to try them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pin {
    /// Input arrays zero except at the reads of the original script.
    Ground,
    /// Input arrays zero except at every read, instances' included.
    Instances,
    /// Every constant as the model has it.
    Model,
}

/// A relaxed script for `script` (a whole transcript with one query, ending in `check-sat`).
/// `query` is the query's own text (its guard and goal), whose constants come first among the
/// instances. Commands from popped scopes are left out and declarations are hoisted ahead of
/// assertions, so an instance may name a constant declared after its quantifier. `None` when
/// the script does not parse or has no quantifier.
pub(crate) fn relax(script: &str, query: &str) -> Option<Relaxed> {
    let commands = sexp::parse(script).collect::<Result<Vec<_>, _>>().ok()?;
    // Commands of each open scope, outermost first.
    let mut scopes: Vec<Vec<Sexp>> = vec![vec![]];
    for c in commands {
        match c.head() {
            Some("push") => {
                let n = c.as_list().and_then(|l| l.get(1)).and_then(|n| n.as_atom()?.parse().ok());
                for _ in 0..n.unwrap_or(1) {
                    scopes.push(vec![]);
                }
            }
            Some("pop") => {
                let n = c.as_list().and_then(|l| l.get(1)).and_then(|n| n.as_atom()?.parse().ok());
                for _ in 0..n.unwrap_or(1) {
                    if scopes.len() > 1 {
                        scopes.pop();
                    }
                }
            }
            Some("check-sat" | "get-value" | "get-model" | "get-info" | "echo" | "exit") => {}
            _ => scopes.last_mut().unwrap().push(c),
        }
    }
    let live: Vec<Sexp> = scopes.into_iter().flatten().collect();
    let text = live.iter().map(ToString::to_string).collect::<String>();
    if !text.contains("(forall") && !text.contains("(exists") {
        return None;
    }

    // Live constants and their sorts, in declaration order.
    let mut consts: Vec<(String, Sexp)> = vec![];
    for c in &live {
        let Some(l) = c.as_list() else { continue };
        let sort = match (c.head(), l) {
            (Some("declare-const"), [_, _, sort]) => sort,
            (Some("declare-fun"), [_, _, Sexp::List(args), sort]) if args.is_empty() => sort,
            _ => continue,
        };
        let Some(name) = l[1].as_atom() else { continue };
        consts.push((name.to_string(), sort.clone()));
    }

    // Candidate integer terms: constants the query names, then numerals used as array indices,
    // then the other constants, latest first.
    let query_atoms: HashSet<String> =
        sexp::parse(query).filter_map(Result::ok).flat_map(|e| atoms(&e)).collect();
    let is_int = |s: &Sexp| s.as_atom() == Some("Int");
    let mut ints: Vec<Sexp> = vec![];
    let mut seen = HashSet::new();
    let mut add = |t: Sexp, ints: &mut Vec<Sexp>| {
        if seen.insert(t.to_string()) {
            ints.push(t);
        }
    };
    for (name, sort) in consts.iter().rev() {
        if is_int(sort) && query_atoms.contains(name) {
            add(Sexp::Atom(name.clone()), &mut ints);
        }
    }
    let mut numerals = vec![];
    for c in &live {
        select_numerals(c, &mut numerals);
    }
    add(Sexp::Atom("0".into()), &mut ints);
    for n in numerals {
        add(Sexp::Atom(n), &mut ints);
    }
    for (name, sort) in consts.iter().rev() {
        if is_int(sort) {
            add(Sexp::Atom(name.clone()), &mut ints);
        }
    }

    let (decls, rest): (Vec<_>, Vec<_>) = live.into_iter().partition(|c| {
        matches!(
            c.head(),
            Some(
                "declare-const"
                    | "declare-fun"
                    | "define-fun"
                    | "declare-sort"
                    | "define-sort"
                    | "declare-datatypes"
                    | "declare-datatype"
                    | "set-option"
                    | "set-logic"
            )
        )
    });
    let ground_rest = rest;
    let rest: Vec<Sexp> = ground_rest.iter().map(|c| instantiate(c, &ints)).collect();

    // Arrays defined as a term of others, by an assertion `(= a t)`, with the arrays `t` names.
    let arrays: HashMap<&str, &Sexp> = consts
        .iter()
        .filter(|(_, s)| s.head() == Some("Array"))
        .map(|(n, s)| (n.as_str(), s))
        .collect();
    let mut defined: HashMap<String, Vec<String>> = HashMap::new();
    for c in &rest {
        if let Some([_, eq]) = c.as_list()
            && let Some([Sexp::Atom(e), Sexp::Atom(a), t]) = eq.as_list()
            && e == "="
            && arrays.contains_key(a.as_str())
        {
            let uses = atoms(t).into_iter().filter(|x| arrays.contains_key(x.as_str())).collect();
            defined.entry(a.clone()).or_insert(uses);
        }
    }
    let inputs: Vec<(String, Sexp)> = consts
        .iter()
        .filter(|(n, s)| s.head() == Some("Array") && !defined.contains_key(n))
        .cloned()
        .collect();
    // Scalar reads of any array; a read of a computed array also reads, at the same indices,
    // each input array it is computed from (its value there, unless a store overrode it).
    let mut all = vec![];
    let mut seen = HashSet::new();
    for c in &rest {
        scalar_reads(c, &arrays, &mut seen, &mut all);
    }
    // The reads the original script makes outside quantifiers.
    let (mut own, mut own_seen) = (vec![], HashSet::new());
    for c in &ground_rest {
        scalar_reads(c, &arrays, &mut own_seen, &mut own);
    }
    let mut reads: Vec<Read> = vec![];
    let mut seen = HashMap::new();
    for (a, idx, read) in all {
        let is_own = own_seen.contains(&read.to_string());
        let mut todo = vec![a];
        let mut visited = HashSet::new();
        while let Some(x) = todo.pop() {
            if !visited.insert(x.clone()) {
                continue;
            }
            match defined.get(&x) {
                Some(uses) => todo.extend(uses.iter().cloned()),
                None if arrays.get(x.as_str()).is_some_and(|s| depth(s) == idx.len()) => {
                    let r = if x == read_path(&read).map_or("", |(r, _)| r) {
                        read.clone()
                    } else {
                        idx.iter().fold(Sexp::Atom(x.clone()), |acc, i| {
                            Sexp::List(vec![Sexp::Atom("select".into()), acc, i.clone()])
                        })
                    };
                    match seen.get(&r.to_string()) {
                        Some(&k) => {
                            let prev: &mut Read = &mut reads[k];
                            prev.3 |= is_own;
                        }
                        None => {
                            seen.insert(r.to_string(), reads.len());
                            reads.push((x, idx.clone(), r, is_own));
                        }
                    }
                }
                None => {}
            }
        }
    }

    // Models printed without `let`, so that each value is a closed term.
    let mut out = String::from(
        "(set-option :pp.min_alias_size 1000000)\n(set-option :pp.max_depth 1000000)\n",
    );
    for c in decls.iter().chain(&rest) {
        out.push_str(&c.to_string());
        out.push('\n');
    }
    out.push_str("(check-sat)\n(get-model)\n");
    let terms: Vec<String> = reads
        .iter()
        .flat_map(|(_, idx, read, _): &Read| {
            std::iter::once(read.to_string()).chain(idx.iter().map(ToString::to_string))
        })
        .collect();
    if !terms.is_empty() {
        out.push_str(&format!("(get-value ({}))\n", terms.join(" ")));
    }
    let live = consts.into_iter().map(|(n, _)| unquote(&n).to_string()).collect();
    Some(Relaxed { script: out, live, inputs, reads })
}

/// Assertions that fix the query's state to a model of the relaxed script (its output, from
/// `sat` on).
///
/// With [`Pin::Model`], every live constant gets its value in the model, except values that name
/// the solver's own auxiliary functions (`as-array`, `k!0`).
///
/// Otherwise each scalar constant gets its value, each input array is the zero array (storage
/// nobody wrote) except at the reads of it (the original script's, or every one with
/// [`Pin::Instances`]), which keep their values, and
/// arrays the script computes from others are left to follow from them. The model picks array
/// defaults freely, so they often break a quantified invariant that zeros keep, and the solver
/// settles a quantifier over arrays it is given far more readily than one over arrays it must
/// build.
pub(crate) fn pins(output: &str, r: &Relaxed, how: Pin) -> Option<String> {
    let zero = how != Pin::Model;
    let mut it = sexp::parse(output).filter_map(Result::ok);
    let sat = it.find(|e| e.as_atom().is_some())?;
    if sat.as_atom() != Some("sat") {
        return None;
    }
    let defs = it.find(|e| e.as_list().is_some())?;
    let mut out = String::new();
    for d in defs.as_list()? {
        let Some([_, name, Sexp::List(args), sort, value]) = d.as_list() else { continue };
        if d.head() != Some("define-fun") || !args.is_empty() {
            continue;
        }
        let Some(name) = name.as_atom() else { continue };
        let name = unquote(name);
        if !r.live.contains(name) || (zero && sort.head() == Some("Array")) {
            continue;
        }
        if atoms(value).iter().any(|a| a == "as-array" || a == "lambda" || a.contains('!')) {
            continue;
        }
        out.push_str(&format!("(assert (= |{name}| {value}))\n"));
    }
    if !zero {
        return Some(out);
    }
    // Values of the reads and their indices, in the order of the get-value.
    let mut values = vec![];
    if !r.reads.is_empty() {
        let vals = it.find(|e| e.as_list().is_some())?;
        for pair in vals.as_list()? {
            let Some([_, v]) = pair.as_list() else { return None };
            values.push(v.clone());
        }
    }
    let mut values = values.into_iter();
    let mut points: HashMap<&str, Vec<(Vec<Sexp>, Sexp)>> = HashMap::new();
    for (a, idx, _, own) in &r.reads {
        let leaf = values.next()?;
        let idx: Vec<Sexp> = idx.iter().map(|_| values.next()).collect::<Option<_>>()?;
        if *own || how == Pin::Instances {
            points.entry(a.as_str()).or_default().push((idx, leaf));
        }
    }
    for (a, sort) in &r.inputs {
        let v = build(sort, points.get(a.as_str()).map_or(&[][..], Vec::as_slice))?;
        out.push_str(&format!("(assert (= {a} {v}))\n"));
    }
    Some(out)
}

/// An index path into an array and the scalar there.
type Point = (Vec<Sexp>, Sexp);

/// Assertions that fix every constant of a `get-model` response to its value there, except values
/// that name the solver's own auxiliary functions.
pub(crate) fn model_pins(model: &Sexp) -> String {
    let mut out = String::new();
    for d in model.as_list().unwrap_or_default() {
        let Some([_, Sexp::Atom(name), Sexp::List(args), _, value]) = d.as_list() else { continue };
        if d.head() != Some("define-fun") || !args.is_empty() {
            continue;
        }
        if atoms(value).iter().any(|a| a == "as-array" || a == "lambda" || a.contains('!')) {
            continue;
        }
        out.push_str(&format!("(assert (= |{}| {value}))\n", unquote(name)));
    }
    out
}

/// The array of sort `sort` that is zero except at `points` (index paths with their values);
/// the first value given for a path wins.
fn build(sort: &Sexp, points: &[Point]) -> Option<Sexp> {
    let Some([_, _, elem]) = sort.as_list() else { return None };
    let mut acc = Sexp::List(vec![
        Sexp::List(vec![Sexp::Atom("as".into()), Sexp::Atom("const".into()), sort.clone()]),
        zero_of(elem)?,
    ]);
    // Stores are applied in reverse, so the first point for an index ends up outermost.
    let mut groups: Vec<(Sexp, Vec<Point>)> = vec![];
    for (path, v) in points {
        let (i, rest) = path.split_first()?;
        match groups.iter_mut().find(|(j, _)| j == i) {
            Some((_, g)) => g.push((rest.to_vec(), v.clone())),
            None => groups.push((i.clone(), vec![(rest.to_vec(), v.clone())])),
        }
    }
    for (i, g) in groups.into_iter().rev() {
        let v = if elem.head() == Some("Array") { build(elem, &g)? } else { g[0].1.clone() };
        acc = Sexp::List(vec![Sexp::Atom("store".into()), acc, i, v]);
    }
    Some(acc)
}

/// The zero of a sort: `0`, `false` or a constant array of zeros.
fn zero_of(sort: &Sexp) -> Option<Sexp> {
    match sort {
        Sexp::Atom(s) if s == "Int" => Some(Sexp::Atom("0".into())),
        Sexp::Atom(s) if s == "Bool" => Some(Sexp::Atom("false".into())),
        Sexp::List(l) if sort.head() == Some("Array") && l.len() == 3 => Some(Sexp::List(vec![
            Sexp::List(vec![Sexp::Atom("as".into()), Sexp::Atom("const".into()), sort.clone()]),
            zero_of(&l[2])?,
        ])),
        _ => None,
    }
}

/// The reads in `e` of an array down to a scalar, each once: `(array, indices, read)`. Quantified
/// subterms are skipped: their reads name bound variables.
fn scalar_reads(
    e: &Sexp,
    inputs: &HashMap<&str, &Sexp>,
    seen: &mut HashSet<String>,
    out: &mut Vec<(String, Vec<Sexp>, Sexp)>,
) {
    let Sexp::List(items) = e else { return };
    if matches!(e.head(), Some("forall" | "exists")) {
        return;
    }
    if let Some((a, idx)) = read_path(e)
        && let Some(sort) = inputs.get(a)
        && depth(sort) == idx.len()
        && seen.insert(e.to_string())
    {
        out.push((a.to_string(), idx, e.clone()));
    }
    for i in items {
        scalar_reads(i, inputs, seen, out);
    }
}

/// `(select .. (select a i0) .. ik)` as `a` and `[i0, .., ik]`.
fn read_path(e: &Sexp) -> Option<(&str, Vec<Sexp>)> {
    match e {
        Sexp::Atom(a) => Some((a, vec![])),
        Sexp::List(l) => {
            let [Sexp::Atom(s), inner, i] = &l[..] else { return None };
            if s != "select" {
                return None;
            }
            let (a, mut idx) = read_path(inner)?;
            idx.push(i.clone());
            Some((a, idx))
        }
    }
}

/// How many indices reach a scalar in an array sort.
fn depth(sort: &Sexp) -> usize {
    match sort.as_list() {
        Some([_, _, elem]) if sort.head() == Some("Array") => 1 + depth(elem),
        _ => 0,
    }
}

/// The expression with each quantifier over integers replaced by its instances at `ints`,
/// innermost first; a quantifier over another sort becomes `true` (`forall`) or `false`
/// (`exists`).
fn instantiate(e: &Sexp, ints: &[Sexp]) -> Sexp {
    let Sexp::List(items) = e else { return e.clone() };
    let items: Vec<Sexp> = items.iter().map(|i| instantiate(i, ints)).collect();
    let q = match items.first().and_then(Sexp::as_atom) {
        Some(q @ ("forall" | "exists")) => q == "forall",
        _ => return Sexp::List(items),
    };
    let [_, Sexp::List(binders), body] = &items[..] else { return Sexp::List(items) };
    let mut body = body.clone();
    // `(! body :pattern ...)`
    if body.head() == Some("!")
        && let Some(inner) = body.as_list().and_then(|l| l.get(1))
    {
        body = inner.clone();
    }
    let mut vars = vec![];
    for b in binders {
        match b.as_list() {
            Some([Sexp::Atom(v), s]) if s.as_atom() == Some("Int") => vars.push(v.clone()),
            _ => return Sexp::Atom(if q { "true" } else { "false" }.into()),
        }
    }
    if vars.is_empty() || ints.is_empty() {
        return Sexp::Atom(if q { "true" } else { "false" }.into());
    }
    let mut k = ints.len();
    while k > 1 && k.checked_pow(vars.len() as u32).is_none_or(|p| p > CAP) {
        k -= 1;
    }
    let pool = &ints[..k];
    let mut instances = vec![Sexp::Atom(if q { "and" } else { "or" }.into())];
    let mut idx = vec![0; vars.len()];
    loop {
        let map: HashMap<&str, &Sexp> =
            vars.iter().zip(&idx).map(|(v, &i)| (v.as_str(), &pool[i])).collect();
        instances.push(subst(&body, &map));
        // The next tuple of indices, in odometer order.
        let mut j = 0;
        while j < idx.len() {
            idx[j] += 1;
            if idx[j] < k {
                break;
            }
            idx[j] = 0;
            j += 1;
        }
        if j == idx.len() {
            break;
        }
    }
    if instances.len() == 2 { instances.pop().unwrap() } else { Sexp::List(instances) }
}

fn subst(e: &Sexp, map: &HashMap<&str, &Sexp>) -> Sexp {
    match e {
        Sexp::Atom(a) => map.get(a.as_str()).map_or_else(|| e.clone(), |t| (*t).clone()),
        Sexp::List(items) => Sexp::List(items.iter().map(|i| subst(i, map)).collect()),
    }
}

fn atoms(e: &Sexp) -> Vec<String> {
    match e {
        Sexp::Atom(a) => vec![a.clone()],
        Sexp::List(items) => items.iter().flat_map(atoms).collect(),
    }
}

/// Numerals used as the index of a `select`.
fn select_numerals(e: &Sexp, out: &mut Vec<String>) {
    let Sexp::List(items) = e else { return };
    if let [Sexp::Atom(s), _, Sexp::Atom(n)] = &items[..]
        && s == "select"
        && n.bytes().all(|b| b.is_ascii_digit())
    {
        out.push(n.clone());
    }
    for i in items {
        select_numerals(i, out);
    }
}

fn unquote(name: &str) -> &str {
    name.strip_prefix('|').and_then(|n| n.strip_suffix('|')).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_at_ground_terms() {
        let script = "(declare-const a (Array Int Int))\n(push 1)\n(declare-const |gone| Int)\n(pop 1)\n(assert (forall ((x Int)) (! (= (select a x) 0) :pattern ((select a x)))))\n(declare-const |to@1| Int)\n(assert (not (= (select a 7) 0)))\n(check-sat)\n";
        let r = relax(script, "(= |to@1| 1)").unwrap();
        snapbox::assert_data_eq!(
            r.script,
            snapbox::str![[r#"
(set-option :pp.min_alias_size 1000000)
(set-option :pp.max_depth 1000000)
(declare-const a (Array Int Int))
(declare-const |to@1| Int)
(assert (and (= (select a |to@1|) 0) (= (select a 0) 0) (= (select a 7) 0)))
(assert (not (= (select a 7) 0)))
(check-sat)
(get-model)
(get-value ((select a |to@1|) |to@1| (select a 0) 0 (select a 7) 7))

"#]]
        );
        let mut live: Vec<_> = r.live.iter().cloned().collect();
        live.sort();
        assert_eq!(live, ["a", "to@1"]);
    }

    #[test]
    fn pins_skip_solver_functions() {
        let script = "(declare-const a (Array Int Int))\n(declare-const b (Array Int Int))\n(declare-const x Int)\n(assert (forall ((y Int)) (= (select b y) x)))\n(check-sat)\n";
        let r = relax(script, "").unwrap();
        let model = "sat\n((define-fun x () Int (- 1))\n (define-fun a () (Array Int Int) ((as const (Array Int Int)) 0))\n (define-fun b () (Array Int Int) (_ as-array k!0))\n (define-fun k!0 ((x!0 Int)) Int 0))\n";
        snapbox::assert_data_eq!(
            pins(model, &r, Pin::Model).unwrap(),
            snapbox::str![[r#"
(assert (= |x| (- 1)))
(assert (= |a| ((as const (Array Int Int)) 0)))

"#]]
        );
    }

    #[test]
    fn pins_zero_arrays_keep_reads() {
        let script = "(declare-const m (Array Int (Array Int Int)))\n(declare-const n (Array Int (Array Int Int)))\n(declare-const k Int)\n(assert (forall ((s Int)) (= (select (select m 0) s) 0)))\n(assert (= n (store m 0 (select m k))))\n(assert (= (select (select m k) 2) 5))\n(check-sat)\n";
        let r = relax(script, "").unwrap();
        let out = "sat\n((define-fun k () Int 3)\n (define-fun m () (Array Int (Array Int Int)) ((as const (Array Int (Array Int Int))) ((as const (Array Int Int)) 20))))\n(((select (select m k) 0) 20) (k 3) (0 0) ((select (select m 0) 0) 0) (0 0) (0 0) ((select (select m k) 2) 5) (k 3) (2 2) ((select (select m 0) 3) 0) (0 0) (3 3) ((select (select m 0) k) 0) (0 0) (k 3))\n";
        snapbox::assert_data_eq!(
            pins(out, &r, Pin::Instances).unwrap(),
            snapbox::str![[r#"
(assert (= |k| 3))
(assert (= m (store (store ((as const (Array Int (Array Int Int))) ((as const (Array Int Int)) 0)) 0 (store (store ((as const (Array Int Int)) 0) 3 0) 0 0)) 3 (store (store ((as const (Array Int Int)) 0) 2 5) 0 20))))

"#]]
        );
    }
}
