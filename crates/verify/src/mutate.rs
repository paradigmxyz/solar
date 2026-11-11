//! `solar-verify mutate`: mutation testing of specifications.
//!
//! A spec is weak when a mutant of the implementation that behaves differently still verifies.
//! This mode mutates the bodies of functions that verify and that some spec reaches (their own
//! requires, ensures or asserts, their contract's invariants, or those of a function that calls
//! them), re-verifies each mutant in the mutated function and every function that can reach it,
//! and reports the mutants that survive. Only a failure with a counterexample kills a mutant; a
//! solver unknown leaves it undecided.
//!
//! Mutation sites come from the AST, and a mutant replaces the source text of one span:
//! relational (ROR), arithmetic (AOR) and logical (LCR) operator replacement, boolean (BOOL) and
//! decimal constant (CRP) replacement, deletion of expression statements (SDL), and deletion of
//! `if` guards that only revert (GDL). Inline assembly, constants in index position and statements
//! that are the bare body of a branch or loop are not mutated. At most `--per-fn` mutants are drawn
//! per function, round-robin over its operators, each operator's sites in a seeded random order.
//!
//! Each mutant is verified by a child process of this binary (`--json --only C.f`), as `--jobs`
//! does, so a solver that hangs on one mutant cannot stall the others.
//!
//! With solc (from `--solc`, `$SOLC` or `PATH`), survivors get a trivial compiler equivalence
//! check: a survivor whose optimized bytecode equals the original's is equivalent and leaves the
//! score. It stays a survivor, with a note, when the mutated function is in no bytecode (a revert
//! at the top of its body changes nothing) or is internal or a modifier (its callers may pin the
//! difference away, while its spec speaks for every input).
use crate::{
    prog::custom_tags,
    sources::{ImportOpts, Loader},
    walk,
};
use solar_parse::{
    ast::{
        self, BinOpKind, ContractKind, Expr, ExprKind, IndexKind, ItemKind, LitKind, Stmt,
        StmtKind, UnOpKind, Visibility, visit::Visit,
    },
    interface::{
        Session, Span,
        data_structures::{
            Never,
            map::{FxHashMap, FxHashSet},
        },
        kw, sym,
    },
};
use std::{
    fmt::Write as _,
    io::Write as _,
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// A mutation operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Op {
    Ror,
    Aor,
    Lcr,
    Bool,
    Crp,
    Sdl,
    Gdl,
}

impl Op {
    const ALL: [Self; 7] =
        [Self::Ror, Self::Aor, Self::Lcr, Self::Bool, Self::Crp, Self::Sdl, Self::Gdl];

    fn name(self) -> &'static str {
        match self {
            Self::Ror => "ROR",
            Self::Aor => "AOR",
            Self::Lcr => "LCR",
            Self::Bool => "BOOL",
            Self::Crp => "CRP",
            Self::Sdl => "SDL",
            Self::Gdl => "GDL",
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

struct Opts {
    file: PathBuf,
    list: bool,
    json: bool,
    /// Enabled operators, one bit per [`Op`].
    ops: u8,
    per_fn: usize,
    max_affected: usize,
    only: Option<String>,
    seed: u64,
    jobs: usize,
    timeout: u64,
    strict: bool,
    solc: Option<PathBuf>,
    no_tce: bool,
    imports: ImportOpts,
}

const USAGE: &str = "usage: solar-verify mutate [--list] [--json] [--ops ROR,AOR,LCR,BOOL,CRP,SDL,GDL] \
[--per-fn N] [--max-affected N] [--only Contract[.fn]] [--seed N] [--jobs N] [--timeout MS] \
[--strict] [--solc PATH | --no-tce] [PREFIX=PATH...] [--base-path PATH] [--include-path PATH] FILE.sol";

fn parse_opts(args: impl Iterator<Item = String>) -> Opts {
    let mut o = Opts {
        file: PathBuf::new(),
        list: false,
        json: false,
        ops: Op::ALL.iter().fold(0, |m, op| m | op.bit()),
        per_fn: 12,
        max_affected: 8,
        only: None,
        seed: 0,
        jobs: 1,
        timeout: 10_000,
        strict: false,
        solc: None,
        no_tce: false,
        imports: ImportOpts::default(),
    };
    let mut files = vec![];
    let mut it = args;
    let num = |x: Option<String>, what: &str| -> u64 {
        x.and_then(|x| x.parse().ok()).unwrap_or_else(|| {
            eprintln!("error: `{what}` needs a number\n{USAGE}");
            std::process::exit(2)
        })
    };
    while let Some(a) = it.next() {
        if o.imports.parse_arg(&a, &mut it) {
            continue;
        }
        match a.as_str() {
            "--list" => o.list = true,
            "--json" => o.json = true,
            "--ops" => {
                o.ops = 0;
                for name in it.next().unwrap_or_default().split(',') {
                    let Some(op) = Op::ALL.into_iter().find(|op| op.name() == name) else {
                        eprintln!("error: unknown operator `{name}`\n{USAGE}");
                        std::process::exit(2)
                    };
                    o.ops |= op.bit();
                }
            }
            "--per-fn" => o.per_fn = num(it.next(), "--per-fn") as usize,
            "--max-affected" => o.max_affected = num(it.next(), "--max-affected") as usize,
            "--only" => o.only = it.next(),
            "--seed" => o.seed = num(it.next(), "--seed"),
            "--jobs" | "-j" => o.jobs = num(it.next(), "--jobs").max(1) as usize,
            "--timeout" => o.timeout = num(it.next(), "--timeout"),
            "--strict" => o.strict = true,
            "--solc" => o.solc = it.next().map(PathBuf::from),
            "--no-tce" => o.no_tce = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0)
            }
            _ => files.push(PathBuf::from(a)),
        }
    }
    let [file] = <[PathBuf; 1]>::try_from(files).unwrap_or_else(|_| {
        eprintln!("error: `mutate` takes one file\n{USAGE}");
        std::process::exit(2)
    });
    o.file = file;
    o
}

/// A function, constructor or modifier with a body, as written in the file.
struct Func {
    /// `Contract.name`, as the verifier names it.
    name: String,
    /// Internal, private or a modifier: callers may pin what its spec leaves open.
    internal: bool,
    /// Byte offset just after the `{` that opens the body.
    body_start: usize,
    /// Some spec speaks about it directly.
    has_spec: bool,
    /// Names it calls or uses as modifiers.
    callees: FxHashSet<String>,
    is_modifier: bool,
    /// `@custom:trusted`: its body is not verified, so mutating it measures nothing.
    trusted: bool,
    /// Where its body can be mutated, in source order.
    sites: Vec<Site>,
}

/// One way to mutate a span.
struct Site {
    op: Op,
    span: Span,
    orig: String,
    rep: String,
}

struct Mutant {
    func: usize,
    op: Op,
    span: Span,
    lo: usize,
    hi: usize,
    line: usize,
    orig: String,
    rep: String,
    /// Functions to verify, the mutated one first when it is a target itself.
    affected: Vec<String>,
}

/// What became of one mutant.
struct Outcome {
    result: &'static str,
    by: Option<String>,
    tce: Option<&'static str>,
}

/// Runs `solar-verify mutate` with the arguments after `mutate`.
pub(crate) fn main(args: impl Iterator<Item = String>) -> ! {
    let o = parse_opts(args);
    let sess = crate::session();
    let mut src = String::new();
    let mut funcs = vec![];
    let parsed = sess.enter_sequential(|| -> Result<(), ()> {
        let arena = ast::Arena::new();
        let loader = Loader::new(&sess, &arena, &o.imports).map_err(|error| {
            eprintln!("error: {error}");
        })?;
        // Only the file itself is mutated; what it imports is used as it is.
        let loaded = loader.load(&o.file, None)?;
        src = loaded.file.src.to_string();
        collect(&sess, loaded.units.last().unwrap(), o.ops, &mut funcs);
        Ok(())
    });
    if parsed.is_err() {
        std::process::exit(2);
    }
    let exe = std::env::current_exe().expect("current exe");

    // Which functions verify on the unmodified file.
    let verified = if o.list {
        None
    } else {
        let mut cmd = Command::new(&exe);
        cmd.args(["--json", "--jobs", &o.jobs.to_string(), "--timeout", &o.timeout.to_string()]);
        if o.strict {
            cmd.arg("--strict");
        }
        cmd.args(o.imports.args());
        let rows = json_rows(&cmd.arg(&o.file).output().expect("run solar-verify").stdout);
        Some(
            rows.into_iter()
                .filter(|(_, st)| st == "verified")
                .map(|(f, _)| f)
                .collect::<FxHashSet<_>>(),
        )
    };

    let (plan, uncovered) = draw(&o, &funcs, verified.as_ref(), &sess);
    if o.list {
        let mut rows = vec![];
        for m in &plan {
            let f = &funcs[m.func].name;
            println!("{f} {} line {}: `{}` -> `{}`", m.op.name(), m.line, m.orig, m.rep);
            rows.push(serde_json::json!({
                "fn": f,
                "op": m.op.name(),
                "line": m.line,
                "orig": m.orig,
                "rep": m.rep,
                "affected": m.affected,
            }));
        }
        if o.json {
            println!("JSON {}", serde_json::Value::Array(rows));
        }
        println!(
            "{} mutants in {} functions; {uncovered} mutation sites in trusted code or code no spec reaches",
            plan.len(),
            plan.iter().map(|m| m.func).collect::<FxHashSet<_>>().len()
        );
        std::process::exit(0);
    }

    let dir = std::env::temp_dir().join(format!("solar-verify-mutate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create a temporary directory");
    let base = o.file.file_name().unwrap().to_string_lossy().into_owned();
    let mut outcomes = run(&o, &exe, &dir, &base, &src, &plan);

    let solc = if o.no_tce {
        None
    } else {
        o.solc
            .clone()
            .or_else(|| std::env::var_os("SOLC").map(PathBuf::from))
            .or_else(|| find_on_path("solc"))
    };
    let tce_note = match &solc {
        Some(solc) if outcomes.iter().any(|x| x.result == "survived") => {
            let solc = Solc { path: solc, unit: &o.file, imports: &o.imports };
            tce(&solc, &src, &funcs, &plan, &mut outcomes)
        }
        Some(_) => None,
        None if o.no_tce => None,
        None => Some("no solc (pass --solc or set SOLC): survivors are not checked".to_string()),
    };
    let _ = std::fs::remove_dir_all(&dir);

    report(&o, &sess, &funcs, &plan, &outcomes, tce_note);
}

/// Collects the functions of `unit` and their mutation sites.
fn collect<'a>(sess: &Session, unit: &'a ast::SourceUnit<'a>, ops: u8, funcs: &mut Vec<Func>) {
    let off = |pos| sess.source_map().lookup_byte_offset(pos).pos.0 as usize;
    for item in unit.items.iter() {
        let ItemKind::Contract(c) = &item.kind else { continue };
        if c.kind == ContractKind::Interface {
            continue;
        }
        let contract_invs =
            custom_tags(&item.docs).iter().any(|(t, ..)| t == "invariant" || t == "lock-stable");
        for sub in c.body.iter() {
            let ItemKind::Function(f) = &sub.kind else { continue };
            let Some(body) = &f.body else { continue };
            let is_modifier = f.kind.is_modifier();
            let name = match f.header.name {
                Some(n) if !f.kind.is_constructor() => n.as_str().to_string(),
                _ => f.kind.to_str().to_string(),
            };
            let mut callees: FxHashSet<String> =
                f.header.modifiers.iter().map(|m| m.name.last().as_str().to_string()).collect();
            let mut asserts = false;
            for st in body.stmts.iter() {
                walk::all_stmts(st, &mut |s| {
                    asserts |= custom_tags(&s.docs).iter().any(|(t, ..)| t == "assert");
                    walk::stmt_exprs(s, &mut |e| {
                        walk::sub_exprs(e, &mut |x| {
                            let ExprKind::Call(callee, _) = &x.kind else { return };
                            let id = match &callee.peel_parens().kind {
                                ExprKind::Ident(i) => i,
                                ExprKind::Member(_, m) => m,
                                _ => return,
                            };
                            asserts |= id.name == sym::assert;
                            callees.insert(id.as_str().to_string());
                        })
                    });
                });
            }
            let tags = custom_tags(&sub.docs);
            let own_specs = tags.iter().any(|(t, ..)| t == "requires" || t == "ensures");
            let trusted = tags.iter().any(|(t, ..)| t == "trusted" || t == "external_body");
            funcs.push(Func {
                name: format!("{}.{name}", c.name.as_str()),
                internal: is_modifier
                    || matches!(
                        f.header.visibility(),
                        Some(Visibility::Internal | Visibility::Private)
                    ),
                body_start: off(body.span.lo()) + 1,
                has_spec: own_specs || asserts || (contract_invs && !is_modifier),
                callees,
                is_modifier,
                trusted,
                sites: {
                    let mut v = Sites { sess, ops, bare: FxHashSet::default(), out: vec![] };
                    for st in body.stmts.iter() {
                        let _ = v.visit_stmt(st);
                    }
                    v.out.sort_by_key(|s| (s.span.lo(), s.op));
                    v.out
                },
            });
        }
    }
}

/// Collects the mutation sites of a function body.
struct Sites<'s> {
    sess: &'s Session,
    ops: u8,
    /// Statements and literals not to mutate: bare bodies of branches and loops (deleting one
    /// leaves no statement), and constant indices.
    bare: FxHashSet<Span>,
    out: Vec<Site>,
}

impl Sites<'_> {
    fn text(&self, sp: Span) -> String {
        self.sess.source_map().span_to_snippet(sp).unwrap_or_default()
    }

    fn push(&mut self, op: Op, sp: Span, rep: impl Into<String>) {
        if self.ops & op.bit() != 0 {
            let orig = self.text(sp);
            self.out.push(Site { op, span: sp, orig, rep: rep.into() });
        }
    }
}

/// Whether `s` only reverts: `revert ...;`, `revert(...);` or a block of one of these.
fn only_reverts(s: &Stmt<'_>) -> bool {
    match &s.kind {
        StmtKind::Revert(..) => true,
        StmtKind::Expr(e) => matches!(
            &e.kind,
            ExprKind::Call(c, _) if matches!(&c.kind, ExprKind::Ident(i) if i.name == kw::Revert)
        ),
        StmtKind::Block(b) => b.stmts.len() == 1 && only_reverts(&b.stmts[0]),
        _ => false,
    }
}

impl<'ast> Visit<'ast> for Sites<'_> {
    type BreakValue = Never;

    fn visit_stmt(&mut self, s: &'ast Stmt<'ast>) -> ControlFlow<Never> {
        let mut bare = |b: &Stmt<'_>| {
            if !matches!(b.kind, StmtKind::Block(_)) {
                self.bare.insert(b.span);
            }
        };
        match &s.kind {
            StmtKind::If(_, then, els) => {
                bare(then);
                if let Some(e) = els {
                    bare(e);
                }
            }
            StmtKind::While(_, b) | StmtKind::DoWhile(b, _) => bare(b),
            StmtKind::For { body, .. } => bare(body),
            _ => {}
        }
        if !self.bare.contains(&s.span) {
            match &s.kind {
                // if (c) revert(); -> (deleted)
                StmtKind::If(_, then, None) if only_reverts(then) => self.push(Op::Gdl, s.span, ""),
                // (a, b) = f(); is a declaration in all but name: deleting it is not interesting
                StmtKind::Expr(e) if !matches!(&e.kind, ExprKind::Assign(l, ..) if matches!(l.kind, ExprKind::Tuple(_))) =>
                {
                    // x = f(); -> (deleted)
                    self.push(Op::Sdl, s.span, "")
                }
                _ => {}
            }
        }
        self.walk_stmt(s)
    }

    fn visit_expr(&mut self, e: &'ast Expr<'ast>) -> ControlFlow<Never> {
        match &e.kind {
            ExprKind::Binary(_, op, _) => {
                use BinOpKind::*;
                let (o, to) = match op.kind {
                    Lt => (Op::Ror, Le),
                    Le => (Op::Ror, Lt),
                    Gt => (Op::Ror, Ge),
                    Ge => (Op::Ror, Gt),
                    Eq => (Op::Ror, Ne),
                    Ne => (Op::Ror, Eq),
                    Add => (Op::Aor, Sub),
                    Sub => (Op::Aor, Add),
                    Mul => (Op::Aor, Div),
                    Div | Rem => (Op::Aor, Mul),
                    And => (Op::Lcr, Or),
                    Or => (Op::Lcr, And),
                    _ => return self.walk_expr(e),
                };
                // a < b -> a <= b
                self.push(o, op.span, to.to_str());
            }
            ExprKind::Assign(_, Some(op), _)
                if matches!(op.kind, BinOpKind::Add | BinOpKind::Sub) =>
            {
                // a += b -> a -= b
                let to = if op.kind == BinOpKind::Add { "-=" } else { "+=" };
                self.push(Op::Aor, op.span, to);
            }
            ExprKind::Unary(op, _) => {
                use UnOpKind::*;
                // i++ -> i--
                match op.kind {
                    PreInc | PostInc => self.push(Op::Aor, op.span, "--"),
                    PreDec | PostDec => self.push(Op::Aor, op.span, "++"),
                    _ => {}
                }
            }
            ExprKind::Index(_, IndexKind::Index(Some(i))) => {
                if let ExprKind::Lit(..) = i.kind {
                    self.bare.insert(i.span);
                }
            }
            ExprKind::Lit(lit, _) if !self.bare.contains(&e.span) => match lit.kind {
                // true -> false
                LitKind::Bool(b) => self.push(Op::Bool, lit.span, (!b).to_string()),
                LitKind::Number(_) => {
                    // 5 -> 6, 5 -> 4: decimal constants below a million, as written
                    let t = self.text(lit.span);
                    if let Ok(n) = t.parse::<u64>()
                        && t.bytes().all(|c| c.is_ascii_digit())
                        && n < 1_000_000
                    {
                        self.push(Op::Crp, lit.span, (n + 1).to_string());
                        if n > 0 {
                            self.push(Op::Crp, lit.span, (n - 1).to_string());
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.walk_expr(e)
    }
}

/// SplitMix64: a small seeded generator, so mutant selection is stable across runs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, (self.next() % (i as u64 + 1)) as usize);
        }
    }
}

/// Chooses the mutants to run, and counts the sites it skips: in trusted functions and in those
/// no spec reaches.
fn draw(
    o: &Opts,
    funcs: &[Func],
    verified: Option<&FxHashSet<String>>,
    sess: &Session,
) -> (Vec<Mutant>, usize) {
    // callers of each name (overloads and contracts alike, as a textual call graph would)
    let mut callers: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
    for (i, f) in funcs.iter().enumerate() {
        for c in &f.callees {
            callers.entry(c.as_str()).or_default().push(i);
        }
    }
    let short = |f: &Func| f.name.rsplit('.').next().unwrap().to_string();
    let affected = |start: usize| -> Vec<String> {
        let mut seen = FxHashSet::default();
        seen.insert(start);
        let mut order = vec![start];
        let mut i = 0;
        while let Some(&g) = order.get(i) {
            i += 1;
            for &c in callers.get(short(&funcs[g]).as_str()).into_iter().flatten() {
                if seen.insert(c) {
                    order.push(c);
                }
            }
        }
        let mut names: Vec<String> = vec![];
        for g in order {
            let f = &funcs[g];
            if f.is_modifier
                || !f.has_spec
                || verified.is_some_and(|v| !v.contains(&f.name))
                || names.contains(&f.name)
            {
                continue;
            }
            names.push(f.name.clone());
        }
        names.truncate(o.max_affected);
        names
    };
    let off = |pos| sess.source_map().lookup_byte_offset(pos).pos.0 as usize;
    let mut rng = Rng(o.seed);
    let mut plan = vec![];
    let mut uncovered = 0;
    for (fi, f) in funcs.iter().enumerate() {
        if f.sites.is_empty()
            || o.only
                .as_ref()
                .is_some_and(|w| *w != f.name && f.name.split_once('.').is_none_or(|(c, _)| c != w))
        {
            continue;
        }
        let aff = affected(fi);
        if aff.is_empty() || f.trusted {
            uncovered += f.sites.len();
            continue;
        }
        // one queue per operator, in the order operators first appear
        let mut queues: Vec<(Op, Vec<&Site>)> = vec![];
        for s in &f.sites {
            match queues.iter_mut().find(|(op, _)| *op == s.op) {
                Some((_, q)) => q.push(s),
                None => queues.push((s.op, vec![s])),
            }
        }
        for (_, q) in &mut queues {
            rng.shuffle(q);
        }
        let mut picked = vec![];
        while picked.len() < o.per_fn && queues.iter().any(|(_, q)| !q.is_empty()) {
            for (_, q) in &mut queues {
                if picked.len() < o.per_fn
                    && let Some(s) = q.pop()
                {
                    picked.push(s);
                }
            }
        }
        picked.sort_by_key(|s| (s.span.lo(), s.op));
        for s in picked {
            plan.push(Mutant {
                func: fi,
                op: s.op,
                span: s.span,
                lo: off(s.span.lo()),
                hi: off(s.span.hi()),
                line: crate::line(sess, s.span),
                orig: s.orig.clone(),
                rep: s.rep.clone(),
                affected: aff.clone(),
            });
        }
    }
    (plan, uncovered)
}

/// `(fn, status)` of each row of a `--json` run.
fn json_rows(stdout: &[u8]) -> Vec<(String, String)> {
    let out = String::from_utf8_lossy(stdout);
    let Some(json) = out.lines().find_map(|l| l.strip_prefix("JSON ")) else { return vec![] };
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    rows.iter()
        .filter_map(|r| Some((r["fn"].as_str()?.to_string(), r["status"].as_str()?.to_string())))
        .collect()
}

/// Verifies every mutant, `--jobs` at a time.
fn run(o: &Opts, exe: &Path, dir: &Path, base: &str, src: &str, plan: &[Mutant]) -> Vec<Outcome> {
    let next = AtomicUsize::new(0);
    let outs: Vec<Mutex<Option<Outcome>>> = plan.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..o.jobs {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(m) = plan.get(i) else { break };
                    let file = dir.join(format!("{i}-{base}"));
                    let msrc = format!("{}{}{}", &src[..m.lo], m.rep, &src[m.hi..]);
                    std::fs::write(&file, msrc).expect("write a mutant");
                    let mut out = Outcome { result: "survived", by: None, tce: None };
                    for f in &m.affected {
                        let mut cmd = Command::new(exe);
                        cmd.args(["--json", "--timeout", &o.timeout.to_string(), "--only", f]);
                        if o.strict {
                            cmd.arg("--strict");
                        }
                        // The mutant is verified as if it stood where the file does, so its
                        // imports resolve from there.
                        cmd.args(o.imports.args()).arg("--mutant-of").arg(&o.file);
                        let res = cmd.arg(&file).output().expect("run solar-verify");
                        if res.status.code() == Some(2) {
                            out = Outcome { result: "stillborn", by: None, tce: None };
                            break;
                        }
                        let rows = json_rows(&res.stdout);
                        if rows.iter().any(|(_, st)| st == "failed") {
                            out.result = "killed";
                            out.by = Some(f.clone());
                            break;
                        }
                        if rows.is_empty() || rows.iter().any(|(_, st)| st != "verified") {
                            // keep looking for a kill
                            out.result = "undecided";
                            out.by = Some(f.clone());
                        }
                    }
                    let _ = std::fs::remove_file(&file);
                    *outs[i].lock().unwrap() = Some(out);
                }
            });
        }
    });
    outs.into_iter().map(|o| o.into_inner().unwrap().unwrap()).collect()
}

fn find_on_path(bin: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(bin)).find(|p| p.is_file())
}

/// solc, compiling versions of the file at the file's path, so its imports resolve.
struct Solc<'a> {
    path: &'a Path,
    /// The file's path, which names the source.
    unit: &'a Path,
    imports: &'a ImportOpts,
}

impl Solc<'_> {
    /// The bytecode of every contract `src` compiles to, standing at the file's path.
    fn compile(&self, src: &str, via_ir: bool) -> Result<String, String> {
        let unit = self.unit.display().to_string();
        let remappings = self.imports.remappings.clone();
        let input = serde_json::json!({
            "language": "Solidity",
            "sources": { unit: { "content": src } },
            "settings": {
                "optimizer": { "enabled": true },
                "metadata": { "bytecodeHash": "none" },
                "viaIR": via_ir,
                "remappings": remappings,
                "outputSelection": {
                    "*": { "*": ["evm.bytecode.object", "evm.deployedBytecode.object"] }
                },
            },
        });
        let mut cmd = Command::new(self.path);
        cmd.arg("--standard-json");
        if let Some(base) = &self.imports.base_path {
            cmd.arg("--base-path").arg(base);
        }
        for path in &self.imports.include_paths {
            cmd.arg("--include-path").arg(path);
        }
        // Imports may lie outside the base path: the file's directory and remapping targets.
        let mut allowed: Vec<String> = self
            .unit
            .parent()
            .map(|parent| parent.display().to_string())
            .into_iter()
            .filter(|parent| !parent.is_empty())
            .collect();
        allowed.extend(
            self.imports.remappings.iter().filter_map(|r| Some(r.split_once('=')?.1.to_string())),
        );
        if !allowed.is_empty() {
            cmd.arg("--allow-paths").arg(allowed.join(","));
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().unwrap();
        let input = input.to_string();
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let _ = writer.join();
        let output: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|_| String::from_utf8_lossy(&out.stderr).trim().to_string())?;
        let errors = output["errors"].as_array().into_iter().flatten();
        if let Some(error) = errors.into_iter().find(|e| e["severity"] == "error") {
            return Err(error["message"].as_str().unwrap_or("solc failed").to_string());
        }
        Ok(output["contracts"].to_string())
    }
}

/// Trivial compiler equivalence for the survivors; returns a note when it cannot run.
fn tce(
    solc: &Solc<'_>,
    src: &str,
    funcs: &[Func],
    plan: &[Mutant],
    outcomes: &mut [Outcome],
) -> Option<String> {
    // stack too deep is the one error the other pipeline can fix
    let (via_ir, base) = match solc.compile(src, false) {
        Ok(b) => (false, b),
        Err(e) if e.contains("Stack too deep") => match solc.compile(src, true) {
            Ok(b) => (true, b),
            Err(e) => return Some(format!("solc: {e}: survivors are not checked")),
        },
        Err(e) => return Some(format!("solc: {e}: survivors are not checked")),
    };
    let mut live: FxHashMap<usize, bool> = FxHashMap::default();
    for (m, out) in plan.iter().zip(outcomes.iter_mut()) {
        if out.result != "survived" {
            continue;
        }
        let alive = *live.entry(m.func).or_insert_with(|| {
            // an opaque guard keeps the rest of the body reachable (immutables stay assigned);
            // a bare revert is the fallback
            let at = funcs[m.func].body_start;
            [" if (msg.data.length == 0x5e471e15) revert(); ", " revert(); "]
                .iter()
                .find_map(|g| {
                    solc.compile(&format!("{}{g}{}", &src[..at], &src[at..]), via_ir).ok()
                })
                .is_some_and(|b| b != base)
        });
        let msrc = format!("{}{}{}", &src[..m.lo], m.rep, &src[m.hi..]);
        out.tce = Some(match solc.compile(&msrc, via_ir) {
            _ if !alive => "dead",
            Err(_) => "compile-error",
            Ok(b) if b != base => "different",
            Ok(_) if funcs[m.func].internal => "equivalent-in-context",
            Ok(_) => {
                out.result = "equivalent";
                "equivalent"
            }
        });
    }
    None
}

fn report(
    o: &Opts,
    sess: &Session,
    funcs: &[Func],
    plan: &[Mutant],
    outcomes: &[Outcome],
    tce_note: Option<String>,
) -> ! {
    let count = |r: &str| outcomes.iter().filter(|x| x.result == r).count();
    let mut rows = vec![];
    for (m, out) in plan.iter().zip(outcomes) {
        let f = &funcs[m.func].name;
        let mut line = format!(
            "{:<10} {f} {} line {}: `{}` -> `{}`",
            out.result,
            m.op.name(),
            m.line,
            m.orig,
            m.rep
        );
        if let Some(by) = &out.by {
            let _ = write!(line, " (by {by})");
        }
        if let Some(tce) = out.tce {
            let _ = write!(line, " (bytecode: {tce})");
        }
        println!("{line}");
        if out.result == "survived" {
            let mut d = sess
                .dcx
                .warn(format!("mutant survives: `{}` -> `{}`", m.orig, m.rep))
                .span(m.span)
                .note(format!("in {f}; verified by {}", m.affected.join(", ")));
            d = match out.tce {
                Some("dead") => d.note("the function is in no deployed bytecode"),
                Some("equivalent-in-context") => {
                    d.note("identical bytecode here: no caller reaches the difference")
                }
                _ => d.help("strengthen a spec so that it tells the mutant apart"),
            };
            d.emit();
        }
        rows.push(serde_json::json!({
            "fn": f,
            "op": m.op.name(),
            "line": m.line,
            "orig": m.orig,
            "rep": m.rep,
            "result": out.result,
            "by": out.by,
            "tce": out.tce,
        }));
    }
    if let Some(n) = tce_note {
        sess.dcx.note(n).emit();
    }
    if o.json {
        println!("JSON {}", serde_json::Value::Array(rows));
    }
    let (k, s) = (count("killed"), count("survived"));
    let score = if k + s > 0 {
        format!("{k}/{} = {}%", k + s, (200 * k + k + s) / (2 * (k + s)))
    } else {
        "-".into()
    };
    println!(
        "mutation results:: {k} killed, {s} survived, {} equivalent, {} undecided, {} stillborn; score {score}",
        count("equivalent"),
        count("undecided"),
        count("stillborn")
    );
    std::process::exit(0);
}
