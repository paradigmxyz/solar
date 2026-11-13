//! Reduces a diverging case while it keeps diverging the same way.
//!
//! A generated case is reduced on its AST: calls, functions, statements and expressions go, and
//! expressions become literals of their type or one of their operands of the same type. A case
//! from a source file is reduced by calls and then by lines.

use crate::{
    Cli,
    case::{self, Case, Divergence},
    grammar::{Contract, Expr, Lit, Stmt, Ty},
};

/// Whether `case` still shows `d`.
fn keeps(cli: &Cli, case: &Case, d: &Divergence, tries: &mut usize) -> bool {
    *tries += 1;
    let dir = cli.out.join("reduce-work");
    let out = case::run(
        &Cli {
            lean: if d.legs.contains("solidcore") { cli.lean.clone() } else { None },
            ..cli.clone()
        },
        case,
        &dir,
    );
    out.solc_reject.is_none() && out.divergences.iter().any(|x| x.same(d))
}

pub(crate) fn reduce_file(cli: &Cli, path: &std::path::Path) {
    let case: Case = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let Some(d) = case.divergence.clone() else {
        eprintln!("{}: no divergence recorded", path.display());
        return;
    };
    let reduced = reduce(cli, &case, &d);
    let out = cli.out.join(path.file_name().unwrap()).with_extension("reduced.json");
    std::fs::write(&out, serde_json::to_string_pretty(&reduced).unwrap()).unwrap();
    println!("{}\n{}", out.display(), reduced.source());
}

/// Every statement list of the contract, in a fixed order.
fn lists(c: &mut Contract) -> Vec<*mut Vec<Stmt>> {
    fn walk(v: &mut Vec<Stmt>, out: &mut Vec<*mut Vec<Stmt>>) {
        out.push(v as *mut _);
        for s in v.iter_mut() {
            for b in s.blocks_mut() {
                walk(b, out);
            }
        }
    }
    let mut out = vec![];
    walk(&mut c.ctor, &mut out);
    for f in c.helpers.iter_mut().chain(c.funcs.iter_mut()) {
        walk(&mut f.body, &mut out);
    }
    out
}

/// Every expression of the contract, outermost first.
fn exprs(c: &mut Contract) -> Vec<*mut Expr> {
    fn expr(e: &mut Expr, out: &mut Vec<*mut Expr>) {
        out.push(e as *mut _);
        for c in e.children_mut() {
            expr(c, out);
        }
    }
    let mut out = vec![];
    for l in lists(c) {
        // SAFETY: the lists are disjoint parts of `c`, alive while `out` is used below.
        for s in unsafe { &mut *l }.iter_mut() {
            for e in s.exprs_mut() {
                expr(e, &mut out);
            }
        }
    }
    out
}

fn replacements(e: &Expr) -> Vec<Expr> {
    let ty = e.ty();
    let mut out = vec![];
    if !matches!(e, Expr::Lit(_)) {
        out.push(Expr::Lit(Lit::zero(ty)));
        if ty != Ty::Addr {
            out.push(Expr::Lit(Lit { ty, value: "1".into() }));
        }
    }
    for c in e.children() {
        if c.ty() == ty {
            out.push(c.clone());
        }
    }
    out
}

pub(crate) fn reduce(cli: &Cli, case: &Case, d: &Divergence) -> Case {
    let mut tries = 0;
    let mut best = case.clone();
    best.divergence = Some(d.clone());
    if !keeps(cli, &best, d, &mut tries) {
        eprintln!("reduce: the case does not reproduce; kept as is");
        return best;
    }
    // Calls after the diverging one cannot matter.
    if let Some(i) = d.call {
        let mut c = best.clone();
        c.calls.truncate(i + 1);
        if keeps(cli, &c, d, &mut tries) {
            best = c;
        }
    }
    loop {
        let before = serde_json::to_string(&best).unwrap();
        // Drop calls, then simplify their arguments.
        let mut i = 0;
        while i < best.calls.len() {
            let mut c = best.clone();
            c.calls.remove(i);
            if (!c.calls.is_empty() || d.call.is_none()) && keeps(cli, &c, d, &mut tries) {
                best = c;
                continue;
            }
            i += 1;
        }
        for i in 0..best.calls.len() {
            for j in 0..best.calls[i].args.len() {
                for v in ["0", "1"] {
                    if best.calls[i].args[j].value == v {
                        break;
                    }
                    let mut c = best.clone();
                    c.calls[i].args[j].value = v.into();
                    if keeps(cli, &c, d, &mut tries) {
                        best = c;
                        break;
                    }
                }
            }
        }
        if best.contract.is_some() {
            best = reduce_ast(cli, best, d, &mut tries);
        } else {
            best = reduce_lines(cli, best, d, &mut tries);
        }
        if serde_json::to_string(&best).unwrap() == before {
            break;
        }
    }
    eprintln!("reduce: {tries} tries");
    best
}

fn reduce_ast(cli: &Cli, mut best: Case, d: &Divergence, tries: &mut usize) -> Case {
    // Functions no call names, then any function.
    let called: Vec<String> = best.calls.iter().map(|c| c.func.clone()).collect();
    let mut i = 0;
    while i < best.contract.as_ref().unwrap().funcs.len() {
        let mut c = best.clone();
        let k = c.contract.as_mut().unwrap();
        let name = k.funcs[i].name.clone();
        k.funcs.remove(i);
        c.calls.retain(|x| x.func != name);
        if (k.funcs.is_empty() || (called.contains(&name) && c.calls.is_empty()))
            && d.call.is_some()
        {
            i += 1;
            continue;
        }
        if keeps(cli, &c, d, tries) {
            best = c;
        } else {
            i += 1;
        }
    }
    // Helpers, state variables, events and errors.
    for what in 0..4 {
        let mut i = 0;
        loop {
            let mut c = best.clone();
            let k = c.contract.as_mut().unwrap();
            let len = [k.helpers.len(), k.state.len(), k.events.len(), k.errors.len()][what];
            if i >= len {
                break;
            }
            let last = i + 1 == len;
            match what {
                0 => drop(k.helpers.remove(i)),
                1 => drop(k.state.remove(i)),
                // Events and errors are named by index: only the last can go.
                2 if last => drop(k.events.remove(i)),
                3 if last => drop(k.errors.remove(i)),
                _ => {
                    i += 1;
                    continue;
                }
            }
            if keeps(cli, &c, d, tries) {
                best = c;
            } else {
                i += 1;
            }
        }
    }
    // Statements: drop each, or replace a compound one by one of its bodies.
    let mut li = 0;
    loop {
        let mut c = best.clone();
        let ls = lists(c.contract.as_mut().unwrap());
        if li >= ls.len() {
            break;
        }
        let n = unsafe { &*ls[li] }.len();
        let mut si = n;
        let mut changed = false;
        while si > 0 {
            si -= 1;
            let mut c = best.clone();
            let ls = lists(c.contract.as_mut().unwrap());
            let p = ls[li];
            let list = unsafe { &mut *p };
            if si >= list.len() {
                continue;
            }
            let s = list.remove(si);
            if keeps(cli, &c, d, tries) {
                best = c;
                changed = true;
                continue;
            }
            let bodies: Vec<Vec<Stmt>> = match &s {
                Stmt::If(_, a, b) => vec![a.clone(), b.clone()],
                Stmt::For(_, _, b) | Stmt::Unchecked(b) => vec![b.clone()],
                _ => vec![],
            };
            for body in bodies {
                let mut c = best.clone();
                let ls = lists(c.contract.as_mut().unwrap());
                let p = ls[li];
                let list = unsafe { &mut *p };
                list.splice(si..si + 1, body);
                if keeps(cli, &c, d, tries) {
                    best = c;
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            li += 1;
        }
    }
    // Expressions.
    let mut ei = 0;
    loop {
        let mut c = best.clone();
        let es = exprs(c.contract.as_mut().unwrap());
        if ei >= es.len() {
            break;
        }
        let cur = unsafe { &*es[ei] }.clone();
        let mut done = false;
        for r in replacements(&cur) {
            let mut c = best.clone();
            let es = exprs(c.contract.as_mut().unwrap());
            let p = es[ei];
            unsafe { *p = r };
            if keeps(cli, &c, d, tries) {
                best = c;
                done = true;
                break;
            }
        }
        if !done {
            ei += 1;
        }
    }
    best
}

fn reduce_lines(cli: &Cli, mut best: Case, d: &Divergence, tries: &mut usize) -> Case {
    let mut chunk = 8;
    while chunk >= 1 {
        let mut i = 0;
        loop {
            let src = best.source.clone().unwrap();
            let lines: Vec<&str> = src.lines().collect();
            if i >= lines.len() {
                break;
            }
            let end = (i + chunk).min(lines.len());
            let mut kept: Vec<&str> = lines[..i].to_vec();
            kept.extend(&lines[end..]);
            let mut c = best.clone();
            c.source = Some(kept.join("\n") + "\n");
            if keeps(cli, &c, d, tries) {
                best = c;
            } else {
                i += chunk;
            }
        }
        chunk /= 2;
    }
    best
}
