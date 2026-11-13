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

/// A relaxed script for `script` (a whole transcript with one query, ending in `check-sat`),
/// with the constants live at its end. `query` is the query's own text (its guard and goal),
/// whose constants come first among the instances. Commands from popped scopes are left out and
/// declarations are hoisted ahead of assertions, so an instance may name a constant declared
/// after its quantifier. `None` when the script does not parse or has no quantifier.
pub(crate) fn relax(script: &str, query: &str) -> Option<(String, HashSet<String>)> {
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
    let mut consts: Vec<(String, String)> = vec![];
    for c in &live {
        let Some(l) = c.as_list() else { continue };
        let sort = match (c.head(), l) {
            (Some("declare-const"), [_, _, sort]) => sort,
            (Some("declare-fun"), [_, _, Sexp::List(args), sort]) if args.is_empty() => sort,
            _ => continue,
        };
        let Some(name) = l[1].as_atom() else { continue };
        consts.push((name.to_string(), sort.to_string()));
    }

    // Candidate integer terms: constants the query names, then numerals used as array indices,
    // then the other constants, latest first.
    let query_atoms: HashSet<String> =
        sexp::parse(query).filter_map(Result::ok).flat_map(|e| atoms(&e)).collect();
    let mut ints: Vec<Sexp> = vec![];
    let mut seen = HashSet::new();
    let mut add = |t: Sexp, ints: &mut Vec<Sexp>| {
        if seen.insert(t.to_string()) {
            ints.push(t);
        }
    };
    for (name, sort) in consts.iter().rev() {
        if sort == "Int" && query_atoms.contains(name) {
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
        if sort == "Int" {
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
    // Models printed without `let`, so that each value is a closed term.
    let mut out = String::from(
        "(set-option :pp.min_alias_size 1000000)
(set-option :pp.max_depth 1000000)
",
    );
    for c in decls.iter().chain(&rest) {
        out.push_str(&instantiate(c, &ints).to_string());
        out.push('\n');
    }
    out.push_str("(check-sat)\n(get-model)\n");
    let names = consts.into_iter().map(|(n, _)| unquote(&n).to_string()).collect();
    Some((out, names))
}

/// Assertions that fix each of `live`'s constants to its value in `model` (a solver's
/// `get-model` output, after its `sat`). Values that name the solver's own auxiliary
/// functions (`as-array`, `k!0`) are left unpinned.
pub(crate) fn pins(model: &str, live: &HashSet<String>) -> Option<String> {
    let mut it = sexp::parse(model).filter_map(Result::ok);
    let sat = it.find(|e| e.as_atom().is_some())?;
    if sat.as_atom() != Some("sat") {
        return None;
    }
    let defs = it.find(|e| e.as_list().is_some())?;
    let mut out = String::new();
    for d in defs.as_list()? {
        let Some([_, name, Sexp::List(args), _, value]) = d.as_list() else { continue };
        if d.head() != Some("define-fun") || !args.is_empty() {
            continue;
        }
        let Some(name) = name.as_atom() else { continue };
        let name = unquote(name);
        if !live.contains(name) {
            continue;
        }
        if atoms(value).iter().any(|a| a == "as-array" || a == "lambda" || a.contains('!')) {
            continue;
        }
        out.push_str(&format!("(assert (= |{name}| {value}))\n"));
    }
    Some(out)
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
        let (relaxed, live) = relax(script, "(= |to@1| 1)").unwrap();
        snapbox::assert_data_eq!(
            relaxed,
            snapbox::str![[r#"
(set-option :pp.min_alias_size 1000000)
(set-option :pp.max_depth 1000000)
(declare-const a (Array Int Int))
(declare-const |to@1| Int)
(assert (and (= (select a |to@1|) 0) (= (select a 0) 0) (= (select a 7) 0)))
(assert (not (= (select a 7) 0)))
(check-sat)
(get-model)

"#]]
        );
        let mut live: Vec<_> = live.into_iter().collect();
        live.sort();
        assert_eq!(live, ["a", "to@1"]);
    }

    #[test]
    fn pins_skip_solver_functions() {
        let live = ["a", "b", "x"].into_iter().map(String::from).collect();
        let model = "sat\n((define-fun x () Int (- 1))\n (define-fun a () (Array Int Int) ((as const (Array Int Int)) 0))\n (define-fun b () (Array Int Int) (_ as-array k!0))\n (define-fun k!0 ((x!0 Int)) Int 0))\n";
        snapbox::assert_data_eq!(
            pins(model, &live).unwrap(),
            snapbox::str![[r#"
(assert (= |x| (- 1)))
(assert (= |a| ((as const (Array Int Int)) 0)))

"#]]
        );
    }
}
