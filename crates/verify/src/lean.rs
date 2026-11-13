//! Export of the verification conditions to Lean (`--emit-lean DIR`), format version 1.
//!
//! Every [`Smt`] records, while `--emit-lean` is on, the commands it keeps on the solver's
//! assertion stack and every query it asks: the guard and goal, what the query is for
//! ([`Role`]) and how it was answered. At the end of a run [`write`] translates each recorded
//! script into a Lean term of the deep embedding in `lean/SolarVerify/Smt.lean`, one module per
//! verified function or broadcast lemma, and writes `manifest.json`, which binds them to the
//! sources, the compiler, the options and the exact SMT-LIB of each query. See
//! docs/VERIFY_LEAN.md for the format.
//!
//! The translation covers exactly the SMT-LIB the encoder writes and fails closed: a command,
//! sort, operator or symbol outside it makes the unit's export an error, never a dropped
//! assertion. A query's script is the solver's assertion stack when it was asked, so an obligation
//! assumes nothing the solver did not.

use crate::sexp::{self, Sexp};
use sha2::{Digest, Sha256};
use solar_parse::interface::{Session, Span};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt::Write as _,
    path::Path,
};

/// The version of the export format, which [`LEAN_SMT`] and [`LEAN_VC`] read.
pub(crate) const FORMAT_VERSION: u32 = 1;

/// The Lean library the exported modules import, as the binary was built with it.
pub(crate) const LEAN_SMT: &str = include_str!("../lean/SolarVerify/Smt.lean");
pub(crate) const LEAN_VC: &str = include_str!("../lean/SolarVerify/Vc.lean");
pub(crate) const LEAN_TOOLCHAIN: &str = include_str!("../lean/lean-toolchain");

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static UNIT: RefCell<String> = const { RefCell::new(String::new()) };
    static DONE: RefCell<Vec<Recorder>> = const { RefCell::new(Vec::new()) };
}

/// Turns recording on for the solvers this thread starts from now on.
pub(crate) fn start() {
    RECORDING.with(|r| r.set(true));
}

/// Names the unit (a function or lemma) whose solver starts next.
pub(crate) fn set_unit(name: impl Into<String>) {
    UNIT.with(|u| *u.borrow_mut() = name.into());
}

/// A recorder for a new solver, when recording is on.
pub(crate) fn recorder() -> Option<Box<Recorder>> {
    RECORDING.with(Cell::get).then(|| {
        Box::new(Recorder { unit: UNIT.with(|u| u.borrow().clone()), ..Default::default() })
    })
}

/// Keeps a dropped solver's recording for [`write`].
pub(crate) fn finish(mut r: Box<Recorder>) {
    r.close();
    if !r.queries.is_empty() {
        DONE.with(|d| d.borrow_mut().push(*r));
    }
}

/// What a query is for.
#[derive(Clone, Debug)]
pub(crate) enum Role {
    /// A spec obligation: its kind, where it is checked and the spec clause it discharges.
    Obligation { kind: String, span: Span, spec: Option<(String, Span)> },
    /// One case of a case split of obligation `parent` (an index into the unit's queries).
    Split { parent: usize },
    /// A query whose `unsat` the encoding relies on, such as whether a reentrancy lock is held.
    Side,
    /// A query only a warning or note reads.
    Advisory,
}

/// How a query was answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Unsat,
    Sat,
    Unknown(String),
    /// Not asked: `--dry-run`.
    NotRun,
    /// Not asked: the encoder settled it without a solver, for the reason given (the goal is the
    /// literal `true`, the guard is `false`, the goal was assumed at entry, or the interval
    /// analysis bounds it). It is still an obligation of the export.
    Settled(&'static str),
    /// Unknown, then proved by the case split whose cases are the queries naming it as parent.
    Split,
}

impl Answer {
    fn name(&self) -> &str {
        match self {
            Self::Unsat => "unsat",
            Self::Sat => "sat",
            Self::Unknown(_) => "unknown",
            Self::NotRun => "not-run",
            Self::Settled(_) => "settled",
            Self::Split => "split",
        }
    }
}

/// One query: does `goal` follow from the first `upto` commands of script `script` and `guard`?
#[derive(Clone, Debug)]
pub(crate) struct Query {
    pub script: usize,
    pub upto: usize,
    pub guard: String,
    pub goal: String,
    pub role: Role,
    pub answer: Answer,
    pub solver: Option<String>,
}

/// What one solver was sent, by unit.
#[derive(Debug, Default)]
pub(crate) struct Recorder {
    unit: String,
    /// The commands on the assertion stack now, as sent.
    cmds: Vec<String>,
    /// The number of commands below each open `push`.
    frames: Vec<usize>,
    /// Scripts that queries read: the earlier ones closed, then the current one.
    scripts: Vec<Vec<String>>,
    /// Whether a query reads the current script.
    used: bool,
    queries: Vec<Query>,
    /// Text that did not parse, which makes the unit's export an error.
    error: Option<String>,
}

impl Recorder {
    /// Records persistent commands as the solver receives them.
    pub(crate) fn send(&mut self, text: &str) {
        for cmd in sexp::parse(text) {
            let cmd = match cmd {
                Ok(cmd) => cmd,
                Err(e) => {
                    self.error.get_or_insert_with(|| format!("unparsable SMT-LIB: {e}"));
                    return;
                }
            };
            match (cmd.head(), cmd.as_list()) {
                (Some("push"), Some([_, n])) => {
                    for _ in 0..n.as_atom().and_then(|n| n.parse().ok()).unwrap_or(1usize) {
                        self.frames.push(self.cmds.len());
                    }
                }
                (Some("pop"), Some([_, n])) => {
                    for _ in 0..n.as_atom().and_then(|n| n.parse().ok()).unwrap_or(1usize) {
                        let keep = self.frames.pop().unwrap_or(0);
                        if keep < self.cmds.len() {
                            self.close();
                            self.cmds.truncate(keep);
                        }
                    }
                }
                _ => self.cmds.push(cmd.to_string()),
            }
        }
    }

    /// Closes the current script if a query reads it, so later commands start a new one.
    fn close(&mut self) {
        if self.used {
            self.scripts.push(self.cmds.clone());
            self.used = false;
        }
    }

    /// Records a query and returns its index.
    pub(crate) fn query(
        &mut self,
        guard: &str,
        goal: &str,
        role: Role,
        answer: Answer,
        solver: Option<String>,
    ) -> usize {
        self.used = true;
        let script = self.scripts.len();
        self.queries.push(Query {
            script,
            upto: self.cmds.len(),
            guard: guard.into(),
            goal: goal.into(),
            role,
            answer,
            solver,
        });
        self.queries.len() - 1
    }

    /// The index the next query will have.
    pub(crate) fn len(&self) -> usize {
        self.queries.len()
    }

    /// Marks query `i` as proved by its case split.
    pub(crate) fn split(&mut self, i: usize) {
        if let Some(q) = self.queries.get_mut(i) {
            q.answer = Answer::Split;
        }
    }
}

// Translation.

/// A sort, as the embedding's `Srt`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Srt {
    Bool,
    Int,
    Bv(u32),
    Array(Box<Self>, Box<Self>),
    Tup(Vec<Self>),
}

impl Srt {
    fn lean(&self) -> String {
        match self {
            Self::Bool => ".bool".into(),
            Self::Int => ".int".into(),
            Self::Bv(w) => format!("(.bv {w})"),
            Self::Array(i, e) => format!("(.array {} {})", i.lean(), e.lean()),
            Self::Tup(fs) => {
                format!("(.tup [{}])", fs.iter().map(Self::lean).collect::<Vec<_>>().join(", "))
            }
        }
    }
}

/// A declared function symbol.
struct Decl {
    name: String,
    args: Vec<Srt>,
    ret: Srt,
}

/// A record datatype: its field selectors and their sorts, as written.
struct Datatype {
    fields: Vec<(String, Sexp)>,
}

/// The symbols a script declares, as the translation reaches each command.
#[derive(Default)]
struct Scope {
    decls: Vec<Decl>,
    by_name: BTreeMap<String, usize>,
    datatypes: BTreeMap<String, Datatype>,
    ctors: BTreeMap<String, String>,
    selectors: BTreeMap<String, (String, usize)>,
}

type R<T> = Result<T, String>;

fn atom(s: &Sexp) -> R<&str> {
    s.as_atom().ok_or_else(|| format!("expected a symbol, found `{s}`"))
}

fn list(s: &Sexp) -> R<&[Sexp]> {
    s.as_list().ok_or_else(|| format!("expected a list, found `{s}`"))
}

fn numeral(s: &Sexp) -> R<u32> {
    atom(s)?.parse().map_err(|_| format!("expected a numeral, found `{s}`"))
}

/// The SMT-LIB operators of the fragment and their names in the embedding's `Op`.
fn op(name: &str) -> Option<&'static str> {
    Some(match name {
        "not" => "not",
        "and" => "and",
        "or" => "or",
        "xor" => "xor",
        "=>" => "implies",
        "=" => "eq",
        "distinct" => "distinct",
        "ite" => "ite",
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "div" => "div",
        "mod" => "mod",
        "abs" => "abs",
        "<" => "lt",
        "<=" => "le",
        ">" => "gt",
        ">=" => "ge",
        "select" => "select",
        "store" => "store",
        "bv2nat" | "bv2int" => "bv2nat",
        "bvadd" => "bvadd",
        "bvsub" => "bvsub",
        "bvmul" => "bvmul",
        "bvudiv" => "bvudiv",
        "bvurem" => "bvurem",
        "bvsdiv" => "bvsdiv",
        "bvsrem" => "bvsrem",
        "bvsmod" => "bvsmod",
        "bvneg" => "bvneg",
        "bvand" => "bvand",
        "bvor" => "bvor",
        "bvxor" => "bvxor",
        "bvnot" => "bvnot",
        "bvshl" => "bvshl",
        "bvlshr" => "bvlshr",
        "bvashr" => "bvashr",
        "bvult" => "bvult",
        "bvule" => "bvule",
        "bvugt" => "bvugt",
        "bvuge" => "bvuge",
        "bvslt" => "bvslt",
        "bvsle" => "bvsle",
        "bvsgt" => "bvsgt",
        "bvsge" => "bvsge",
        "concat" => "concat",
        _ => return None,
    })
}

impl Scope {
    fn sort(&self, s: &Sexp, seen: &mut Vec<String>) -> R<Srt> {
        if let Some(a) = s.as_atom() {
            return match a {
                "Bool" => Ok(Srt::Bool),
                "Int" => Ok(Srt::Int),
                _ => {
                    let Some(dt) = self.datatypes.get(a) else {
                        return Err(format!("unsupported sort `{a}`"));
                    };
                    if seen.iter().any(|n| n == a) {
                        return Err(format!("recursive datatype `{a}`"));
                    }
                    seen.push(a.to_string());
                    let fields =
                        dt.fields.iter().map(|(_, f)| self.sort(f, seen)).collect::<R<_>>()?;
                    seen.pop();
                    Ok(Srt::Tup(fields))
                }
            };
        }
        match list(s)? {
            [u, b, w] if u.as_atom() == Some("_") && b.as_atom() == Some("BitVec") => {
                Ok(Srt::Bv(numeral(w)?))
            }
            [a, i, e] if a.as_atom() == Some("Array") => {
                Ok(Srt::Array(Box::new(self.sort(i, seen)?), Box::new(self.sort(e, seen)?)))
            }
            _ => Err(format!("unsupported sort `{s}`")),
        }
    }

    fn declare(&mut self, name: &str, args: Vec<Srt>, ret: Srt) -> R<usize> {
        if self.by_name.contains_key(name)
            || self.ctors.contains_key(name)
            || self.selectors.contains_key(name)
            || op(name).is_some()
        {
            return Err(format!("symbol `{name}` declared twice"));
        }
        self.decls.push(Decl { name: name.into(), args, ret });
        self.by_name.insert(name.into(), self.decls.len() - 1);
        Ok(self.decls.len() - 1)
    }

    /// The Lean term for an SMT-LIB term, with `bound` the bound variables, innermost last.
    fn term(&self, t: &Sexp, bound: &mut Vec<String>) -> R<String> {
        if let Some(a) = t.as_atom() {
            return self.atom(a, bound);
        }
        let items = list(t)?;
        let Some((head, args)) = items.split_first() else {
            return Err("empty application".into());
        };
        if let Some(h) = head.as_list() {
            return self.indexed(h, args, bound, t);
        }
        let h = atom(head)?;
        match h {
            "_" => self.literal(args, t),
            "!" => {
                // Attributes such as :pattern guide instantiation and do not change the meaning.
                let [body, attrs @ ..] = args else { return Err(format!("bad annotation `{t}`")) };
                if attrs.iter().step_by(2).any(|k| {
                    !matches!(
                        k.as_atom(),
                        Some(":pattern" | ":weight" | ":qid" | ":skolemid" | ":named")
                    )
                }) {
                    return Err(format!("unsupported attribute in `{t}`"));
                }
                self.term(body, bound)
            }
            "forall" | "exists" => {
                let [vars, body] = args else { return Err(format!("bad quantifier `{t}`")) };
                let mut sorts = vec![];
                let n = bound.len();
                for v in list(vars)? {
                    let [x, s] = list(v)? else { return Err(format!("bad binder `{v}`")) };
                    sorts.push(self.sort(s, &mut vec![])?.lean());
                    bound.push(atom(x)?.to_string());
                }
                let body = self.term(body, bound);
                bound.truncate(n);
                Ok(format!(
                    ".{} [{}] ({})",
                    if h == "forall" { "«forall»" } else { "«exists»" },
                    sorts.join(", "),
                    body?
                ))
            }
            "let" => Err("unsupported `let`".into()),
            _ => {
                let args = args.iter().map(|a| self.term(a, bound)).collect::<R<Vec<_>>>()?;
                let args = args.join(", ");
                if bound.iter().any(|b| b == h) {
                    return Err(format!("bound variable `{h}` applied to arguments"));
                }
                if let Some(o) = op(h) {
                    Ok(format!(".app .{o} [{args}]"))
                } else if let Some(&f) = self.by_name.get(h) {
                    Ok(format!(".fn {f} [{args}]"))
                } else if self.ctors.contains_key(h) {
                    Ok(format!(".mk [{args}]"))
                } else if let Some((_, i)) = self.selectors.get(h) {
                    Ok(format!(".proj {i} ({args})"))
                } else {
                    Err(format!("unsupported operator `{h}`"))
                }
            }
        }
    }

    fn atom(&self, a: &str, bound: &[String]) -> R<String> {
        if let Some(i) = bound.iter().rev().position(|b| b == a) {
            return Ok(format!(".var {i}"));
        }
        match a {
            "true" | "false" => return Ok(format!(".bool {a}")),
            _ => {}
        }
        if a.bytes().all(|b| b.is_ascii_digit()) && !a.is_empty() {
            if a.len() > 1 && a.starts_with('0') {
                return Err(format!("bad numeral `{a}`"));
            }
            return Ok(format!(".int {a}"));
        }
        if let Some(hex) = a.strip_prefix("#x") {
            let n = num_bigint::BigUint::parse_bytes(hex.as_bytes(), 16)
                .ok_or_else(|| format!("bad literal `{a}`"))?;
            return Ok(format!(".bv {} {n}", 4 * hex.len()));
        }
        if let Some(bin) = a.strip_prefix("#b") {
            let n = num_bigint::BigUint::parse_bytes(bin.as_bytes(), 2)
                .ok_or_else(|| format!("bad literal `{a}`"))?;
            return Ok(format!(".bv {} {n}", bin.len()));
        }
        if let Some(&f) = self.by_name.get(a) {
            if !self.decls[f].args.is_empty() {
                return Err(format!("function `{a}` used as a constant"));
            }
            return Ok(format!(".fn {f} []"));
        }
        if self.ctors.contains_key(a) {
            return Ok(".mk []".into());
        }
        Err(format!("undeclared symbol `{a}`"))
    }

    /// `(_ bvN W)`.
    fn literal(&self, args: &[Sexp], t: &Sexp) -> R<String> {
        let [n, w] = args else { return Err(format!("unsupported literal `{t}`")) };
        let n = atom(n)?
            .strip_prefix("bv")
            .and_then(|n| n.parse::<num_bigint::BigUint>().ok())
            .ok_or_else(|| format!("unsupported literal `{t}`"))?;
        Ok(format!(".bv {} {n}", numeral(w)?))
    }

    fn indexed(&self, h: &[Sexp], args: &[Sexp], bound: &mut Vec<String>, t: &Sexp) -> R<String> {
        let args = args.iter().map(|a| self.term(a, bound)).collect::<R<Vec<_>>>()?;
        match h {
            [u, name, rest @ ..] if u.as_atom() == Some("_") => {
                let o = match (atom(name)?, rest) {
                    ("int2bv", [w]) => format!("(.int2bv {})", numeral(w)?),
                    ("extract", [hi, lo]) => {
                        format!("(.extract {} {})", numeral(hi)?, numeral(lo)?)
                    }
                    ("zero_extend", [n]) => format!("(.zeroExtend {})", numeral(n)?),
                    ("sign_extend", [n]) => format!("(.signExtend {})", numeral(n)?),
                    _ => return Err(format!("unsupported operator in `{t}`")),
                };
                Ok(format!(".app {o} [{}]", args.join(", ")))
            }
            [a, c, s] if a.as_atom() == Some("as") && c.as_atom() == Some("const") => {
                let Srt::Array(i, _) = self.sort(s, &mut vec![])? else {
                    return Err(format!("`as const` of a non-array sort in `{t}`"));
                };
                let [v] = &args[..] else { return Err(format!("bad `as const` in `{t}`")) };
                Ok(format!(".constArray {} ({v})", i.lean()))
            }
            _ => Err(format!("unsupported operator in `{t}`")),
        }
    }

    /// Reads one command: its declarations, and the Lean assertion it adds, if any.
    fn command(&mut self, cmd: &Sexp) -> R<Option<String>> {
        let items = list(cmd)?;
        match (atom(&items[0])?, &items[1..]) {
            ("declare-const", [x, s]) => {
                let s = self.sort(s, &mut vec![])?;
                self.declare(atom(x)?, vec![], s)?;
                Ok(None)
            }
            ("declare-fun", [x, args, ret]) => {
                let args =
                    list(args)?.iter().map(|s| self.sort(s, &mut vec![])).collect::<R<_>>()?;
                let ret = self.sort(ret, &mut vec![])?;
                self.declare(atom(x)?, args, ret)?;
                Ok(None)
            }
            ("define-fun", [x, params, ret, body]) => {
                // A definition is a declaration and an assertion that the symbol equals its body.
                let mut sorts = vec![];
                let mut names = vec![];
                for p in list(params)? {
                    let [n, s] = list(p)? else { return Err(format!("bad parameter `{p}`")) };
                    sorts.push(self.sort(s, &mut vec![])?);
                    names.push(atom(n)?.to_string());
                }
                let mut bound = names.clone();
                let body = self.term(body, &mut bound)?;
                let ret = self.sort(ret, &mut vec![])?;
                let lean_sorts = sorts.iter().map(Srt::lean).collect::<Vec<_>>().join(", ");
                let f = self.declare(atom(x)?, sorts, ret)?;
                let n = names.len();
                let vars = (0..n).rev().map(|i| format!(".var {i}")).collect::<Vec<_>>().join(", ");
                let eq = format!(".app .eq [.fn {f} [{vars}], {body}]");
                Ok(Some(if n == 0 { eq } else { format!(".«forall» [{lean_sorts}] ({eq})") }))
            }
            ("declare-datatypes", [heads, bodies]) => {
                let heads = list(heads)?;
                let bodies = list(bodies)?;
                if heads.len() != bodies.len() {
                    return Err(format!("bad datatype declaration `{cmd}`"));
                }
                for (h, b) in heads.iter().zip(bodies) {
                    let [name, arity] = list(h)? else { return Err(format!("bad datatype `{h}`")) };
                    if numeral(arity)? != 0 {
                        return Err(format!("parametric datatype `{name}`"));
                    }
                    let [ctor] = list(b)? else {
                        return Err(format!("datatype `{name}` needs exactly one constructor"));
                    };
                    let ctor = list(ctor)?;
                    let cname = atom(&ctor[0])?.to_string();
                    let mut fields = vec![];
                    for (i, f) in ctor[1..].iter().enumerate() {
                        let [sel, s] = list(f)? else { return Err(format!("bad field `{f}`")) };
                        let sel = atom(sel)?.to_string();
                        if self
                            .selectors
                            .insert(sel.clone(), (atom(name)?.to_string(), i))
                            .is_some()
                        {
                            return Err(format!("selector `{sel}` declared twice"));
                        }
                        fields.push((sel, s.clone()));
                    }
                    let name = atom(name)?.to_string();
                    if self.ctors.insert(cname.clone(), name.clone()).is_some() {
                        return Err(format!("constructor `{cname}` declared twice"));
                    }
                    self.datatypes.insert(name, Datatype { fields });
                }
                // Every field sort must resolve, with no recursion.
                for name in self.datatypes.keys() {
                    self.sort(&Sexp::Atom(name.clone()), &mut vec![])?;
                }
                Ok(None)
            }
            ("assert", [t]) => Ok(Some(self.term(t, &mut vec![])?)),
            _ => Err(format!("unsupported command `{}`", items[0])),
        }
    }
}

/// A script, translated: its Lean declarations and assertions, and for each command count the
/// number of assertions among the first that many commands.
struct Translated {
    decls: Vec<(String, String)>,
    asserts: Vec<String>,
    asserts_upto: Vec<usize>,
    scope: Scope,
}

fn translate(cmds: &[String]) -> R<Translated> {
    let mut scope = Scope::default();
    let mut asserts = vec![];
    let mut asserts_upto = vec![0];
    for text in cmds {
        let cmd = sexp::parse(text).next().ok_or("empty command")?.map_err(|e| e.to_string())?;
        if let Some(a) = scope.command(&cmd)? {
            asserts.push(a);
        }
        asserts_upto.push(asserts.len());
    }
    let decls = scope
        .decls
        .iter()
        .map(|d| {
            let args = d.args.iter().map(Srt::lean).collect::<Vec<_>>().join(", ");
            (d.name.clone(), format!("⟨{}, [{args}], {}⟩", lean_string(&d.name), d.ret.lean()))
        })
        .collect();
    Ok(Translated { decls, asserts, asserts_upto, scope })
}

/// A Lean string literal.
fn lean_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A Lean identifier part made of letters, digits and underscores.
fn ident(s: &str) -> String {
    let mut out: String =
        s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert(0, 'U');
    }
    out
}

/// The exact SMT-LIB of a query: its script's first commands, then the check.
fn query_smt(cmds: &[String], q: &Query) -> String {
    let mut s = String::new();
    for c in &cmds[..q.upto] {
        s.push_str(c);
        s.push('\n');
    }
    let _ = write!(s, "(assert {})\n(assert (not {}))\n(check-sat)\n", q.guard, q.goal);
    s
}

/// The file, line and column of a span.
fn location(sess: &Session, sp: Span) -> serde_json::Value {
    let pos = sess.source_map().lookup_char_pos(sp.lo());
    serde_json::json!({
        "file": pos.file.name.display().to_string(),
        "line": pos.data.line,
        "column": pos.data.col.0 + 1,
    })
}

/// What the run verified, by unit name: `verified`, `failed`, `unknown` or `unsupported`.
pub(crate) type Statuses = BTreeMap<String, String>;

/// What binds the export to its run.
pub(crate) struct Run<'a> {
    pub arguments: Vec<String>,
    pub options: serde_json::Value,
    pub statuses: &'a Statuses,
    pub solvers: serde_json::Value,
}

/// The version line of each solver a run can use, or `null` for one it cannot start.
pub(crate) fn solver_versions(dry: bool) -> serde_json::Value {
    if dry {
        return serde_json::Value::Null;
    }
    let version = |var: &str, default: &str| {
        let bin = std::env::var(var).unwrap_or_else(|_| default.into());
        if bin == "off" {
            return serde_json::Value::Null;
        }
        std::process::Command::new(&bin)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            .map_or(serde_json::Value::Null, Into::into)
    };
    serde_json::json!({
        "z3": version("SOLAR_VERIFY_Z3", "z3"),
        "cvc5": version("SOLAR_VERIFY_CVC5", "cvc5"),
        "bitwuzla": version("SOLAR_VERIFY_BITWUZLA", "bitwuzla"),
    })
}

/// Writes every recorded unit to `dir` and returns the errors of the units that could not be
/// exported.
pub(crate) fn write(sess: &Session, dir: &Path, run: Run<'_>) -> Vec<String> {
    let units = DONE.with(|d| std::mem::take(&mut *d.borrow_mut()));
    let mut errors = vec![];
    let mut taken = BTreeMap::<String, usize>::new();
    let mut functions = vec![];
    for u in &units {
        let base = u.unit.split('.').map(ident).collect::<Vec<_>>().join(".");
        let n = taken.entry(base.clone()).or_default();
        *n += 1;
        let module = if *n == 1 { base } else { format!("{base}_{n}") };
        match unit(sess, dir, u, &module) {
            Ok(mut json) => {
                json["status"] =
                    run.statuses.get(&u.unit).cloned().unwrap_or_else(|| "lemma".into()).into();
                functions.push(json);
            }
            Err(e) => {
                errors.push(format!("{}: {e}", u.unit));
                functions.push(serde_json::json!({
                    "name": u.unit,
                    "status": "not-exported",
                    "error": e,
                }));
            }
        }
    }
    let mut sources = vec![];
    for file in sess.source_map().files().iter() {
        if let Some(path) = file.name.as_real() {
            sources.push(serde_json::json!({
                "path": path.display().to_string(),
                "sha256": sha256(file.src.as_bytes()),
            }));
        }
    }
    let manifest = serde_json::json!({
        "format": "solar-verify-lean",
        "version": FORMAT_VERSION,
        "compiler": solar_cli::version::solc_compat_version(),
        "arguments": run.arguments,
        "options": run.options,
        "solvers": run.solvers,
        "sources": sources,
        "lean": {
            "toolchain": LEAN_TOOLCHAIN.trim(),
            "library": "SolarVerify",
            "smt_sha256": sha256(LEAN_SMT.as_bytes()),
            "vc_sha256": sha256(LEAN_VC.as_bytes()),
        },
        "units": functions,
        "errors": errors,
    });
    let mut text = serde_json::to_string_pretty(&manifest).expect("the manifest serializes");
    text.push('\n');
    if let Err(e) = std::fs::write(dir.join("manifest.json"), text) {
        errors.push(format!("manifest.json: {e}"));
    }
    errors
}

/// Writes one unit's module and SMT-LIB scripts, and returns its manifest entry.
fn unit(sess: &Session, dir: &Path, u: &Recorder, module: &str) -> R<serde_json::Value> {
    if let Some(e) = &u.error {
        return Err(e.clone());
    }
    let path = module.replace('.', "/");
    let file = dir.join(format!("{path}.lean"));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let ns = format!("SolarVerifyVc.{module}");
    let mut lean = format!(
        "-- Generated by solar-verify --emit-lean (format {FORMAT_VERSION}); do not edit.\n\
         import SolarVerify\n\n\
         set_option maxRecDepth 100000\n\n\
         namespace {ns}\nopen SolarVerify.Smt SolarVerify.Vc\n\n"
    );
    let mut scripts = vec![];
    let mut translated = vec![];
    for (i, cmds) in u.scripts.iter().enumerate() {
        let t = translate(cmds)?;
        let _ = writeln!(lean, "/-- Script {i} of {}: the solver's assertion stack. -/", u.unit);
        let _ = writeln!(lean, "def script{i} : Script where");
        let _ = writeln!(lean, "  decls := [");
        for (j, (_, d)) in t.decls.iter().enumerate() {
            let _ = writeln!(lean, "    {d}{}", if j + 1 < t.decls.len() { "," } else { "" });
        }
        let _ = writeln!(lean, "  ]");
        let _ = writeln!(lean, "  asserts := [");
        for (j, a) in t.asserts.iter().enumerate() {
            let _ = writeln!(lean, "    {a}{}", if j + 1 < t.asserts.len() { "," } else { "" });
        }
        let _ = writeln!(lean, "  ]\n");
        let smt_file = format!("{path}.script{i}.smt2");
        std::fs::write(dir.join(&smt_file), cmds.join("\n") + "\n").map_err(|e| e.to_string())?;
        scripts.push(serde_json::json!({
            "lean": format!("{ns}.script{i}"),
            "smt": smt_file,
            "commands": cmds.len(),
            "decls": t.decls.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        }));
        translated.push(t);
    }
    let mut obligations = vec![];
    for (i, q) in u.queries.iter().enumerate() {
        let t = &translated[q.script];
        let guard = t.scope.term(&sexp_of(&q.guard)?, &mut vec![])?;
        let goal = t.scope.term(&sexp_of(&q.goal)?, &mut vec![])?;
        let upto = t.asserts_upto[q.upto];
        let id = format!("{}#{i}", u.unit);
        let _ = writeln!(lean, "/-- {} -/", doc(&id, q));
        let _ = writeln!(
            lean,
            "def ob{i} : Obligation where\n  script := script{}\n  upto := {upto}\n  guard := {guard}\n  goal := {goal}\n",
            q.script
        );
        let smt = query_smt(&u.scripts[q.script], q);
        let mut json = serde_json::json!({
            "id": id,
            "lean": format!("{ns}.ob{i}"),
            "script": q.script,
            "commands": q.upto,
            "asserts": upto,
            "guard": q.guard,
            "goal": q.goal,
            "answer": q.answer.name(),
            "solver": q.solver,
            "smt_sha256": sha256(smt.as_bytes()),
        });
        match &q.answer {
            Answer::Unknown(why) => json["reason"] = why.clone().into(),
            Answer::Settled(why) => json["reason"] = (*why).into(),
            _ => {}
        }
        let required = match &q.role {
            Role::Obligation { kind, span, spec } => {
                json["role"] = "obligation".into();
                json["kind"] = kind.clone().into();
                json["site"] = location(sess, *span);
                json["spec"] = spec.as_ref().map_or(serde_json::Value::Null, |(text, sp)| {
                    let mut l = location(sess, *sp);
                    l["text"] = text.clone().into();
                    l
                });
                true
            }
            Role::Split { parent } => {
                json["role"] = "split".into();
                json["parent"] = format!("{}#{parent}", u.unit).into();
                false
            }
            Role::Side => {
                json["role"] = "side".into();
                // The encoding relied on the answer only where it was unsat or settled.
                matches!(q.answer, Answer::Unsat | Answer::Settled(_))
            }
            Role::Advisory => {
                json["role"] = "advisory".into();
                false
            }
        };
        json["required"] = required.into();
        obligations.push(json);
    }
    let _ = writeln!(lean, "end {ns}");
    std::fs::write(&file, lean).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "name": u.unit,
        "module": format!("{path}.lean"),
        "namespace": ns,
        "scripts": scripts,
        "obligations": obligations,
    }))
}

fn sexp_of(text: &str) -> R<Sexp> {
    sexp::parse(text).next().ok_or("empty term")?.map_err(|e| e.to_string())
}

fn doc(id: &str, q: &Query) -> String {
    let what = match &q.role {
        Role::Obligation { kind, spec, .. } => match spec {
            Some((text, _)) => format!("{kind}: `{text}`"),
            None => kind.clone(),
        },
        Role::Split { parent } => format!("a case of obligation {parent}"),
        Role::Side => "a query the encoding relies on".into(),
        Role::Advisory => "a query only a warning reads".into(),
    };
    format!("{id}: {what} ({}).", q.answer.name()).replace("-/", "- /")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lean(cmds: &[&str]) -> R<Vec<String>> {
        let cmds: Vec<String> = cmds.iter().map(|c| c.to_string()).collect();
        Ok(translate(&cmds)?.asserts)
    }

    #[test]
    fn translates_the_fragment() {
        let out = lean(&[
            "(declare-datatypes ((|S.P| 0)) (((|mk.P| (|P.a| Int) (|P.b| Bool)))))",
            "(declare-const |p@1| |S.P|)",
            "(declare-fun f (Int (Array Int Int)) Int)",
            "(define-fun |pow2| ((e Int)) Int (ite (= e 0) 1 2))",
            "(assert (forall ((x Int) (y Bool)) (! (=> y (>= (f x ((as const (Array Int Int)) 0)) x)) :pattern ((f x x)))))",
            "(assert (= (|P.b| (|mk.P| (- 3) true)) (bvult (_ bv1 8) #x0f)))",
            "(assert (= ((_ extract 7 0) ((_ int2bv 256) (|P.a| |p@1|))) #b00000001))",
        ])
        .unwrap();
        assert_eq!(
            out,
            [
                ".«forall» [.int] (.app .eq [.fn 2 [.var 0], .app .ite [.app .eq [.var 0, .int 0], .int 1, .int 2]])",
                ".«forall» [.int, .bool] (.app .implies [.var 0, .app .ge [.fn 1 [.var 1, .constArray .int (.int 0)], .var 1]])",
                ".app .eq [.proj 1 (.mk [.app .sub [.int 3], .bool true]), .app .bvult [.bv 8 1, .bv 8 15]]",
                ".app .eq [.app (.extract 7 0) [.app (.int2bv 256) [.proj 0 (.fn 0 [])]], .bv 8 1]",
            ]
        );
    }

    #[test]
    fn fails_closed() {
        for (cmds, why) in [
            (&["(declare-sort U 0)"][..], "unsupported command"),
            (&["(assert (let ((x 1)) (= x 1)))"][..], "unsupported `let`"),
            (&["(assert (= y 1))"][..], "undeclared symbol `y`"),
            (&["(declare-const x Int)", "(assert (bvfoo x))"][..], "unsupported operator `bvfoo`"),
            (&["(declare-const x Real)"][..], "unsupported sort `Real`"),
            (
                &["(declare-datatypes ((L 0)) (((nil) (cons (hd Int) (tl L)))))"][..],
                "needs exactly one constructor",
            ),
            (&["(declare-datatypes ((L 0)) (((cons (hd Int) (tl L)))))"][..], "recursive datatype"),
            (&["(declare-const x Int)", "(declare-const x Int)"][..], "declared twice"),
            (&["(assert (! true :weird 1))"][..], "unsupported attribute"),
        ] {
            let e = lean(cmds).unwrap_err();
            assert!(e.contains(why), "{cmds:?}: {e}");
        }
    }
}
