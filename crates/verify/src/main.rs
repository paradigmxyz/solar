//! solverus: Verus-style deductive verification for Solidity, built on Solar.
mod bvlower;
mod interval;
mod prog;
mod smt;
mod spec;
mod ty;
mod vc;
mod walk;

use solar_parse::{
    Parser, ast,
    interface::{Session, Span, data_structures::map::FxHashSet},
};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

#[cfg(test)]
use serde_json as _;
#[cfg(test)]
use snapbox as _;

struct Cli {
    files: Vec<PathBuf>,
    strict: bool,
    timeout: u64,
    dump_smt: Option<PathBuf>,
    only: Option<String>,
    json: bool,
    jobs: usize,
    instrument: Option<PathBuf>,
    dry_run: bool,
}

fn parse_cli() -> Cli {
    let mut c = Cli {
        files: vec![],
        strict: false,
        timeout: 10_000,
        dump_smt: None,
        only: None,
        json: false,
        instrument: None,
        jobs: 1,
        dry_run: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--strict" => c.strict = true,
            "--timeout" => c.timeout = it.next().and_then(|x| x.parse().ok()).unwrap_or(10_000),
            "--dump-smt" => c.dump_smt = it.next().map(PathBuf::from),
            "--only" => c.only = it.next(),
            "--json" => c.json = true,
            "--dry-run" => c.dry_run = true,
            "--instrument" => c.instrument = it.next().map(PathBuf::from),
            "--jobs" | "-j" => c.jobs = it.next().and_then(|x| x.parse().ok()).unwrap_or(1),
            "-h" | "--help" => {
                println!(
                    "usage: solverus [--strict] [--timeout MS] [--only Contract.fn] [--jobs N] [--dump-smt FILE] [--instrument OUT.sol] FILE.sol...\n\n\
                     --dry-run  generate every obligation without running a solver (see --dump-smt)
\
                     --instrument OUT.sol  write FILE with every @custom:check claim as a require\n\
                     \x20          (deploy that: the verification assumed the checks hold)\n\
                     --strict   arithmetic overflow, division by zero, out-of-bounds and pop-on-empty are\n\
                     \x20          verification errors instead of (sound) reverting paths"
                );
                std::process::exit(0);
            }
            _ => c.files.push(PathBuf::from(a)),
        }
    }
    if c.files.is_empty() {
        eprintln!("usage: solverus [--strict] FILE.sol...");
        std::process::exit(2);
    }
    c
}

/// The 1-based line of `sp`.
fn line(sess: &Session, sp: Span) -> usize {
    sess.source_map().lookup_char_pos(sp.lo()).data.line
}

fn report(sess: &Session, kind: &str, sp: Span, notes: &[String]) {
    let mut diag = sess.dcx.err(kind.to_string()).span(sp);
    for n in notes {
        diag = diag.note(n.clone());
    }
    diag.emit();
}

/// The verdict mark, coloured when stdout is a terminal.
fn mark(ok: bool) -> &'static str {
    match (ok, std::io::stdout().is_terminal()) {
        (true, true) => "\x1b[32m✔\x1b[0m",
        (true, false) => "✔",
        (false, true) => "\x1b[31m✘\x1b[0m",
        (false, false) => "✘",
    }
}

/// A session whose diagnostics go to stderr. None are deduplicated (two failures may read alike)
/// or tracked (they are verification results, not compiler bugs).
fn session() -> Session {
    let s = Session::builder().with_stderr_emitter().build();
    s.dcx.set_flags(|f| {
        f.deduplicate_diagnostics = false;
        f.track_diagnostics = false;
    });
    s
}

/// --jobs N: verify each function in its own child process, N at a time. Children re-parse the
/// file (Solar is fast) and run one `--only Contract.fn` each; the parent merges their output.
fn run_parallel(cli: &Cli) -> ! {
    let mut names: Vec<(PathBuf, String)> = vec![];
    let mut fe_errors = 0;
    let sess = session();
    let ok = sess.enter_sequential(|| -> Result<(), ()> {
        let arena = ast::Arena::new();
        for file in &cli.files {
            let mut parser = Parser::from_file(&sess, &arena, file).map_err(|_| ())?;
            let unit = parser.parse_file().map_err(|e| {
                e.emit();
            })?;
            let unit: &ast::SourceUnit<'_> = arena.alloc(unit);
            let program = prog::build(&sess, &arena, unit);
            for (sp, msg) in &program.errors {
                report(&sess, msg, *sp, &[]);
                fe_errors += 1;
            }
            for c in program.contracts.iter().filter(|c| !c.kind.is_interface()) {
                for f in &c.funcs {
                    if (f.item.body.is_none() && !f.synthetic) || f.trusted {
                        continue;
                    }
                    let n = format!("{}.{}", c.name, f.name);
                    if !names.iter().any(|(p, m)| p == file && *m == n) {
                        names.push((file.clone(), n));
                    }
                }
            }
        }
        Ok(())
    });
    if ok.is_err() {
        std::process::exit(2);
    }
    let exe = std::env::current_exe().expect("current exe");
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outs: Vec<std::sync::Mutex<Option<String>>> =
        names.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..cli.jobs.max(1) {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some((file, n)) = names.get(i) else { break };
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.env("SOLVERUS_CHILD", "1").args([
                        "--timeout",
                        &cli.timeout.to_string(),
                        "--only",
                        n,
                    ]);
                    if cli.strict {
                        cmd.arg("--strict");
                    }
                    if cli.dry_run {
                        cmd.arg("--dry-run");
                    }
                    if cli.json {
                        cmd.arg("--json");
                    }
                    cmd.arg(file);
                    let o = cmd
                        .output()
                        .map(|o| {
                            String::from_utf8_lossy(&o.stdout).to_string()
                                + &String::from_utf8_lossy(&o.stderr)
                        })
                        .unwrap_or_default();
                    *outs[i].lock().unwrap() = Some(o);
                }
            });
        }
    });
    let (mut ok_n, mut err_n) = (0usize, fe_errors);
    let mut rows: Vec<String> = vec![];
    for o in outs {
        let o = o.into_inner().unwrap().unwrap_or_default();
        for line in o.lines() {
            if let Some(rest) = line.strip_prefix("verification results:: ") {
                let nums: Vec<usize> = rest
                    .split(|c: char| !c.is_ascii_digit())
                    .filter_map(|x| x.parse().ok())
                    .collect();
                if nums.len() >= 2 {
                    ok_n += nums[0];
                    err_n += nums[1];
                }
            } else if let Some(j) = line.strip_prefix("JSON ") {
                let j = j.trim();
                let inner = j.trim_start_matches('[').trim_end_matches(']');
                if !inner.is_empty() {
                    rows.push(inner.to_string());
                }
            } else {
                println!("{line}");
            }
        }
    }
    if cli.json {
        println!("JSON [{}]", rows.join(","));
    }
    println!("verification results:: {ok_n} verified, {err_n} errors");
    std::process::exit(if err_n == 0 { 0 } else { 1 });
}

fn main() {
    let cli = parse_cli();
    if cli.jobs > 1 && cli.only.is_none() && cli.dump_smt.is_none() && cli.instrument.is_none() {
        run_parallel(&cli);
    }
    let child = std::env::var_os("SOLVERUS_CHILD").is_some();
    let sess = session();
    let mut total_ok = 0;
    let mut total_err = 0;
    let mut smt_log = String::new();
    let mut json_rows: Vec<String> = vec![];
    let ok = sess.enter_sequential(|| -> Result<(), ()> {
        let arena = ast::Arena::new();
        for file in &cli.files {
            let mut parser = Parser::from_file(&sess, &arena, file).map_err(|_| ())?;
            let unit = parser.parse_file().map_err(|e| {
                e.emit();
            })?;
            let unit: &ast::SourceUnit<'_> = arena.alloc(unit);
            let program = prog::build(&sess, &arena, unit);
            if let Some(out) = &cli.instrument {
                instrument(&sess, &program, file, out);
                std::process::exit(if program.errors.is_empty() { 0 } else { 1 });
            }
            if !child {
                for (sp, msg) in &program.errors {
                    report(&sess, msg, *sp, &[]);
                    total_err += 1;
                }
            }
            for c in &program.contracts {
                if c.kind.is_interface() {
                    continue;
                }
                for f in &c.funcs {
                    if (f.item.body.is_none() && !f.synthetic) || f.trusted {
                        continue;
                    }
                    let name = format!("{}.{}", c.name, f.name);
                    if cli.only.as_ref().is_some_and(|o| o != &name && o != &c.name) {
                        continue;
                    }
                    let t0 = std::time::Instant::now();
                    let smt = if cli.dry_run {
                        smt::Smt::dry(cli.dump_smt.is_some())
                    } else {
                        smt::Smt::new(cli.timeout, cli.dump_smt.is_some()).expect("could not start z3 (set SOLVERUS_Z3)")
                    };
                    let mut v = vc::Vc::new(&program, c, smt, &vc::Opts { strict: cli.strict });
                    if v.inlined_only(f) {
                        continue;
                    }
                    let mut rep = v.verify_fn(f);
                    if c.lock.is_some() && f.is_entry() && !f.is_static() && f.name != "constructor" && rep.unsupported.is_none() {
                        let lr = v.verify_lock(f);
                        rep.checks += lr.checks;
                        rep.fails.extend(lr.fails);
                        if lr.unsupported.is_some() {
                            rep.unsupported = lr.unsupported;
                        }
                    }
                    if let Some(l) = v.smt.log.take() {
                        smt_log.push_str(&l);
                    }
                    if cli.json {
                        let esc = |x: &str| x.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
                        let status = if rep.unsupported.is_some() {
                            "unsupported"
                        } else if rep.fails.is_empty() {
                            "verified"
                        } else if rep.fails.iter().all(|f| f.note.is_some()) {
                            "unknown"
                        } else {
                            "failed"
                        };
                        let mut msgs: Vec<String> = vec![];
                        if let Some((sp, m)) = &rep.unsupported {
                            msgs.push(format!("{}:{}", line(&sess, *sp), m));
                        }
                        for fl in &rep.fails {
                            let sp = fl.spec.as_ref().map(|s| s.1).unwrap_or(fl.span);
                            let n = fl.note.clone().map(|n| format!(" ({n})")).unwrap_or_default();
                            msgs.push(format!("{}:{}{}{}", line(&sess, sp), fl.kind, fl.spec.as_ref().map(|s| format!(": {}", s.0)).unwrap_or_default(), n));
                        }
                        let msgs: Vec<String> = msgs.iter().map(|m| format!("\"{}\"", esc(m))).collect();
                        json_rows.push(format!(
                            "{{\"file\":\"{}\",\"fn\":\"{}\",\"status\":\"{status}\",\"ms\":{},\"checks\":{},\"msgs\":[{}]}}",
                            esc(&file.display().to_string()), esc(&rep.name), t0.elapsed().as_millis(), rep.checks, msgs.join(",")
                        ));
                    }
                    if let Some((sp, msg)) = &rep.unsupported {
                        report(&sess, &format!("unsupported: {msg}"), *sp, &[format!("in {}", rep.name)]);
                        total_err += 1;
                        continue;
                    }
                    for w in &rep.warnings {
                        if let Some(n) = w.strip_prefix("note: ") {
                            sess.dcx.note(format!("{}: {n}", rep.name)).emit();
                        } else {
                            sess.dcx.warn(format!("{}: {w}", rep.name)).emit();
                        }
                    }
                    if rep.fails.is_empty() {
                        total_ok += 1;
                        println!("{} {} ({} obligations)", mark(true), rep.name, rep.checks);
                    } else {
                        total_err += 1;
                        println!("{} {} ({} of {} obligations failed)", mark(false), rep.name, rep.fails.len(), rep.checks);
                        for fl in &rep.fails {
                            let mut notes = vec![format!("in {}", rep.name)];
                            let mut sp = fl.span;
                            if let Some((text, ssp)) = &fl.spec {
                                notes.push(format!("failing spec: {text}"));
                                if ssp != &fl.span {
                                    let (l, l2) = (line(&sess, *ssp), line(&sess, fl.span));
                                    if l != l2 {
                                        notes.push(format!("spec declared at line {l}; checked at line {l2}"));
                                    }
                                }
                                if fl.kind.starts_with("precondition") || fl.kind.contains("external call") || fl.kind.contains("invariant not preserved") {
                                    sp = fl.span;
                                } else {
                                    sp = *ssp;
                                }
                            }
                            if !fl.cex.is_empty() {
                                let cex: Vec<String> = fl.cex.iter().map(|(n, v)| format!("{n} = {v}")).collect();
                                notes.push(format!("counterexample (pre-state): {}", cex.join(", ")));
                            }
                            if let Some(n) = &fl.note {
                                notes.push(n.clone());
                            }
                            report(&sess, &fl.kind, sp, &notes);
                        }
                    }
                }
            }
        }
        Ok(())
    });
    if let Some(p) = &cli.dump_smt {
        let _ = std::fs::write(p, smt_log);
    }
    if cli.json {
        println!(
            "JSON {}",
            if json_rows.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", json_rows.join(","))
            }
        );
    }
    if ok.is_err() {
        std::process::exit(2);
    }
    println!("verification results:: {total_ok} verified, {total_err} errors");
    std::process::exit(if total_err == 0 { 0 } else { 1 });
}

/// Write the file with each \x60@custom:check P\x60 as \x60require(P, "...")\x60: at the start of
/// the function body for a function's checks, before the statement for a statement's. The checks
/// are the runtime half of a checked claim: verification assumed them.
fn instrument(sess: &Session, program: &prog::Program<'_>, file: &Path, out: &Path) {
    for (sp, msg) in &program.errors {
        report(sess, msg, *sp, &[]);
    }
    let src = sess.source_map().load_file(file).expect("read source").src.to_string();
    let off = |sp: Span| sess.source_map().lookup_byte_offset(sp.lo()).pos.0 as usize;
    let off_hi = |sp: Span| sess.source_map().lookup_byte_offset(sp.hi()).pos.0 as usize;
    let req = |text: &str| {
        let msg = text.replace(['"', '\\'], "'");
        format!("require({text}, \"solverus check failed: {msg}\");")
    };
    // (offset, text to insert), applied from the end
    let mut ins: Vec<(usize, String)> = vec![];
    let mut seen = FxHashSet::default();
    for c in &program.contracts {
        for f in &c.funcs {
            let Some(b) = &f.item.body else { continue };
            if f.checks.is_empty() || !seen.insert(b.span.lo()) {
                continue;
            }
            let lb = off(b.span);
            let at = src[lb..].find('{').map(|i| lb + i + 1).unwrap_or(lb);
            let body: String = f.checks.iter().map(|ck| format!(" {}", req(&ck.text))).collect();
            ins.push((at, body));
        }
    }
    for (sp, specs) in &program.stmt_specs {
        let checks: Vec<String> =
            specs.iter().filter(|(t, _)| t == "check").map(|(_, s)| req(&s.text)).collect();
        if checks.is_empty() {
            continue;
        }
        let (lo, hi) = (off(*sp), off_hi(*sp));
        // a declaration keeps its scope; any other statement is wrapped in a block, so the check
        // stays with it even as the single statement of an if or loop
        let decl = program.decl_stmts.contains(sp);
        if decl {
            ins.push((lo, format!("{} ", checks.join(" "))));
        } else {
            ins.push((lo, format!("{{ {} ", checks.join(" "))));
            ins.push((hi, " }".into()));
        }
    }
    ins.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut o = src;
    for (at, t) in ins {
        o.insert_str(at, &t);
    }
    std::fs::write(out, o).expect("write instrumented source");
    eprintln!(
        "wrote {} ({} checks as require)",
        out.display(),
        program.stmt_specs.values().flatten().filter(|(t, _)| t == "check").count()
            + program
                .contracts
                .iter()
                .flat_map(|c| c.funcs.iter())
                .map(|f| f.checks.len())
                .sum::<usize>()
    );
}
