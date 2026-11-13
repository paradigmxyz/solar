//! The SMT-LIB transcript of a run as a Lean module, for the Lean checks of
//! `SolidCore.Verify.VC` (paradigmxyz/solidity-lean).
//!
//! `--dump-lean FILE` writes the commands solar-verify sent the solver as a Lean `List Cmd`
//! (`SolidCore/Verify/VC/Term.lean`), and the transcript itself as a string, so that the Lean side
//! can show that the term prints back to exactly those bytes. Every term must lie in the fragment
//! the Lean side models: integer, boolean and `(Array Int Int)` constants, the operators of
//! `applyOp`, and `forall` over one integer. Anything else is an error, never skipped: a term the
//! Lean side would read differently must not reach it.

use crate::sexp::{self, Sexp};
use std::{collections::HashMap, fmt::Write as _};

const OPS: &[&str] = &[
    "+", "-", "*", "div", "mod", "<=", "<", ">=", ">", "=", "not", "=>", "and", "or", "ite",
    "select", "store",
];

#[derive(Clone, Copy)]
enum Sort {
    Int,
    Bool,
    Arr,
}

impl Sort {
    fn parse(s: &Sexp) -> Option<Self> {
        match s {
            Sexp::Atom(a) if a == "Int" => Some(Self::Int),
            Sexp::Atom(a) if a == "Bool" => Some(Self::Bool),
            Sexp::List(xs) if xs.len() == 3 && xs.iter().all(|x| x.as_atom().is_some()) => {
                let names: Vec<&str> = xs.iter().filter_map(Sexp::as_atom).collect();
                (names == ["Array", "Int", "Int"]).then_some(Self::Arr)
            }
            _ => None,
        }
    }

    fn lean(self) -> &'static str {
        match self {
            Self::Int => ".int",
            Self::Bool => ".bool",
            Self::Arr => ".arr",
        }
    }
}

/// A `|name|` symbol's name.
fn quoted(a: &str) -> Option<&str> {
    let inner = a.strip_prefix('|')?.strip_suffix('|')?;
    (!inner.contains('|')).then_some(inner)
}

/// A Lean string literal.
pub(crate) fn lean_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Conv {
    consts: HashMap<String, Sort>,
}

impl Conv {
    fn term(&self, t: &Sexp, bound: &mut Vec<String>) -> Result<String, String> {
        match t {
            Sexp::Atom(a) if a == "true" => Ok(".tt".into()),
            Sexp::Atom(a) if a == "false" => Ok(".ff".into()),
            Sexp::Atom(a) if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()) => {
                if a.len() > 1 && a.starts_with('0') {
                    return Err(format!("numeral with a leading zero: {a}"));
                }
                Ok(format!("(.num {a})"))
            }
            Sexp::Atom(a) => {
                let name = quoted(a).ok_or_else(|| format!("unquoted symbol: {a}"))?;
                if bound.iter().rev().any(|b| b == name) {
                    return Ok(format!("(.ivar {})", lean_str(name)));
                }
                match self.consts.get(name) {
                    Some(Sort::Int) => Ok(format!("(.ivar {})", lean_str(name))),
                    Some(Sort::Bool) => Ok(format!("(.bvar {})", lean_str(name))),
                    Some(Sort::Arr) => Ok(format!("(.avar {})", lean_str(name))),
                    None => Err(format!("undeclared symbol: {a}")),
                }
            }
            Sexp::List(xs) => {
                let head =
                    xs.first().and_then(Sexp::as_atom).ok_or("application without an operator")?;
                if head == "forall" {
                    let [_, binders, body] = &xs[..] else {
                        return Err(format!("forall with a pattern or no body: {t}"));
                    };
                    let bs = binders.as_list().ok_or("forall without binders")?;
                    let [b] = bs else {
                        return Err(format!("forall over more than one variable: {t}"));
                    };
                    let b = b.as_list().ok_or("bad binder")?;
                    let (Some(x), Some(Sort::Int)) = (
                        b.first().and_then(Sexp::as_atom).and_then(quoted),
                        b.get(1).and_then(Sort::parse),
                    ) else {
                        return Err(format!("forall over a non-integer or unquoted variable: {t}"));
                    };
                    if b.len() != 2 {
                        return Err(format!("bad binder: {t}"));
                    }
                    bound.push(x.to_string());
                    let body = self.term(body, bound);
                    bound.pop();
                    return Ok(format!("(.all {} {})", lean_str(x), body?));
                }
                if !OPS.contains(&head) {
                    return Err(format!("operator outside the Lean fragment: {head}"));
                }
                let mut args = vec![];
                for x in &xs[1..] {
                    args.push(self.term(x, bound)?);
                }
                Ok(format!("(.app {} [{}])", lean_str(head), args.join(", ")))
            }
        }
    }

    fn command(&mut self, line: &str) -> Result<String, String> {
        if let Some(rest) = line.strip_prefix(";@elem ") {
            let parts: Vec<&str> = rest.split(' ').collect();
            if let [x, lo, hi] = parts[..]
                && let Some(x) = quoted(x)
                && lo.bytes().all(|b| b.is_ascii_digit())
                && hi.bytes().all(|b| b.is_ascii_digit())
                && !lo.is_empty()
                && !hi.is_empty()
            {
                return Ok(format!(".elem {} {lo} {hi}", lean_str(x)));
            }
            return Err(format!("bad ;@elem line: {line}"));
        }
        if line.starts_with(';') {
            return Ok(format!(".raw {}", lean_str(line)));
        }
        let es =
            sexp::parse(line).collect::<Result<Vec<_>, _>>().map_err(|e| format!("{e}: {line}"))?;
        let [e] = &es[..] else { return Err(format!("not one command per line: {line}")) };
        let xs = e.as_list().ok_or_else(|| format!("not a command: {line}"))?;
        let atoms: Vec<Option<&str>> = xs.iter().map(Sexp::as_atom).collect();
        match atoms[..] {
            [Some("push"), Some("1")] => Ok(".push".into()),
            [Some("pop"), Some("1")] => Ok(".pop".into()),
            [Some("check-sat")] => Ok(".checkSat".into()),
            [Some("declare-const"), Some(x), _] => {
                let x = quoted(x).ok_or_else(|| format!("unquoted constant: {line}"))?;
                let s = Sort::parse(&xs[2])
                    .ok_or_else(|| format!("sort outside the Lean fragment: {line}"))?;
                self.consts.insert(x.to_string(), s);
                Ok(format!(".decl {} {}", lean_str(x), s.lean()))
            }
            [Some("assert"), _] => Ok(format!(".assert {}", self.term(&xs[1], &mut vec![])?)),
            _ => Err(format!("command outside the Lean fragment: {line}")),
        }
    }
}

/// The Lean module of a transcript: `script`, its commands, and `transcript`, its bytes.
pub(crate) fn module(smt: &str, namespace: &str) -> Result<String, String> {
    let mut conv = Conv { consts: HashMap::new() };
    let mut cmds = vec![];
    for line in smt.lines() {
        cmds.push(conv.command(line)?);
    }
    if !smt.is_empty() && !smt.ends_with('\n') {
        return Err("the transcript does not end in a newline".into());
    }
    let mut out = String::new();
    out.push_str("-- Generated by solar-verify --dump-lean. Do not edit.\n\n");
    out.push_str("import SolidCore.Verify.VC.Term\n\n");
    let _ = writeln!(out, "namespace {namespace}\n");
    out.push_str("open SolidCore.Verify.VC\n\n");
    out.push_str("/-- The commands solar-verify sent the solver. -/\n");
    out.push_str("def script : List Cmd := [\n");
    for (i, c) in cmds.iter().enumerate() {
        let sep = if i + 1 == cmds.len() { "" } else { "," };
        let _ = writeln!(out, "  {c}{sep}");
    }
    out.push_str("]\n\n");
    out.push_str("/-- The transcript, as solar-verify wrote it. -/\n");
    let _ = writeln!(out, "def transcript : String := {}\n", lean_str(smt));
    let _ = writeln!(out, "end {namespace}");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_fragment() {
        let smt = "(push 1)\n; ===== C.f\n(declare-const |x@1| Int)\n(declare-const |m@2| (Array Int Int))\n;@elem |m@2| 0 255\n(assert (and (<= 0 |x@1|) (<= |x@1| 255)))\n(assert (forall ((|a!3| Int)) (= (select |m@2| |a!3|) |x@1|)))\n(check-sat)\n(pop 1)\n";
        let m = module(smt, "T").unwrap();
        assert!(m.contains(".decl \"x@1\" .int"));
        assert!(m.contains(".decl \"m@2\" .arr"));
        assert!(m.contains(".elem \"m@2\" 0 255"));
        assert!(m.contains("(.all \"a!3\" (.app \"=\" [(.app \"select\" [(.avar \"m@2\"), (.ivar \"a!3\")]), (.ivar \"x@1\")]))"));
    }

    #[test]
    fn rejects_what_lean_does_not_model() {
        for bad in [
            "(declare-fun f (Int) Int)\n",
            "(declare-const |b@1| (_ BitVec 8))\n",
            "(declare-const |x@1| Int)\n(assert (= (bv2nat |x@1|) 0))\n",
            "(assert (= |y@9| 0))\n",
            "(declare-const |x@1| Int)\n(assert (forall ((|a| Int) (|b| Int)) (= |a| |b|)))\n",
            "(declare-const |x@1| Int)\n(assert (forall ((|a| Int)) (! (= |a| |x@1|) :pattern (|x@1|))))\n",
            "(declare-const |x@1| Int)\n(assert (= |x@1| 007))\n",
        ] {
            assert!(module(bad, "T").is_err(), "accepted {bad:?}");
        }
    }
}
