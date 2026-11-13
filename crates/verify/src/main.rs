//! solar-verify: Verus-style deductive verification for Solidity, built on Solar.
mod broadcast;
mod bvlower;
mod ghost;
mod interval;
mod mutate;
mod operator;
mod prog;
mod sexp;
mod smt;
mod sources;
mod spec;
mod spec_lean;
mod speccheck;
mod ty;
mod vc;
mod walk;

use solar_parse::{
    ast,
    interface::{Session, Span, data_structures::map::FxHashSet},
};
use sources::{ImportOpts, Loader};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

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
    imports: ImportOpts,
    /// `--mutant-of ORIGINAL`, internal to mutation testing: each file holds a mutant of
    /// `ORIGINAL`, verified as if it stood at `ORIGINAL`, so its imports resolve from there.
    mutant_of: Option<PathBuf>,
    spec_check: bool,
    spec_check_runs: usize,
    seed: u64,
    emit_lean: Option<PathBuf>,
    lean_module: Option<String>,
    lean_compiler: String,
    lean_bytecode_sha256: String,
}

impl Cli {
    /// The files to verify, as the paths they stand at and, for a mutant, the file holding it.
    fn inputs(&self) -> impl Iterator<Item = (&Path, Option<&Path>)> {
        self.files.iter().map(|file| match &self.mutant_of {
            Some(original) => (original.as_path(), Some(file.as_path())),
            None => (file.as_path(), None),
        })
    }

    /// A loader for the files, or an exit with the reason it cannot be made.
    fn loader<'s, 'a>(&self, sess: &'s Session, arena: &'a ast::Arena) -> Loader<'s, 'a> {
        Loader::new(sess, arena, &self.imports).unwrap_or_else(|error| {
            eprintln!("error: {error}");
            std::process::exit(2)
        })
    }
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
        imports: ImportOpts::default(),
        mutant_of: None,
        spec_check: false,
        spec_check_runs: 256,
        seed: 0,
        emit_lean: None,
        lean_module: None,
        lean_compiler: String::new(),
        lean_bytecode_sha256: String::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if c.imports.parse_arg(&a, &mut it) {
            continue;
        }
        match a.as_str() {
            "--strict" => c.strict = true,
            "--timeout" => c.timeout = it.next().and_then(|x| x.parse().ok()).unwrap_or(10_000),
            "--dump-smt" => c.dump_smt = it.next().map(PathBuf::from),
            "--only" => c.only = it.next(),
            "--json" => c.json = true,
            "--dry-run" => c.dry_run = true,
            "--spec-check" => c.spec_check = true,
            "--spec-check-runs" => {
                c.spec_check_runs = it.next().and_then(|x| x.parse().ok()).unwrap_or(256)
            }
            "--seed" => c.seed = it.next().and_then(|x| x.parse().ok()).unwrap_or(0),
            "--instrument" => c.instrument = it.next().map(PathBuf::from),
            "--emit-lean-spec" => c.emit_lean = it.next().map(PathBuf::from),
            "--lean-spec-module" => c.lean_module = it.next(),
            "--lean-spec-compiler" => c.lean_compiler = it.next().unwrap_or_default(),
            "--lean-spec-bytecode-sha256" => c.lean_bytecode_sha256 = it.next().unwrap_or_default(),
            "--jobs" | "-j" => c.jobs = it.next().and_then(|x| x.parse().ok()).unwrap_or(1),
            "--mutant-of" => c.mutant_of = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!(
                    "usage: solar-verify [--strict] [--timeout MS] [--only Contract.fn] [--jobs N] [--dump-smt FILE] [--instrument OUT.sol] [IMPORT OPTIONS] FILE.sol...\n\n\
                     --dry-run  generate every obligation without running a solver (see --dump-smt)\n\
                     --instrument OUT.sol  write FILE with every @custom:check claim as a require\n\
                     \x20          (deploy that: the verification assumed the checks hold)\n\
                     --emit-lean-spec OUT.lean  write the specs as a Lean module over solidity-lean's SolidCore.Spec\n\
                     \x20          (with --lean-spec-module NAME, --lean-spec-compiler TEXT,\n\
                     \x20          --lean-spec-bytecode-sha256 HEX)\n\
                     --spec-check  test the trusted contracts (@custom:trusted bodies, interface\n\
                     \x20          specs, the Math.mulDiv summary) against their code in the file\n\
                     --spec-check-runs N  samples per tested function (default 256); --seed N\n\
                     --strict   arithmetic overflow, division by zero, out-of-bounds and pop-on-empty are\n\
                     \x20          verification errors instead of (sound) reverting paths\n\n\
                     Imports resolve as with solar and solc: PREFIX=PATH remappings (CONTEXT:PREFIX=PATH\n\
                     to scope one), --base-path PATH and --include-path PATH (-I). Contracts from imported\n\
                     files are not verified themselves.\n\n\
                     solar-verify mutate --help: spec mutation testing"
                );
                std::process::exit(0);
            }
            _ => c.files.push(PathBuf::from(a)),
        }
    }
    if c.files.is_empty() {
        eprintln!("usage: solar-verify [--strict] FILE.sol...");
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

/// Reports the ghost code of `program` that is not erasable; returns how many violations there are.
fn report_ghost(sess: &Session, program: &prog::Program<'_>) -> usize {
    for v in &program.ghost_errors {
        let mut notes: Vec<String> = v.func.iter().map(|f| format!("in {f}")).collect();
        notes.push(format!("non-interference: {}", v.rule.note()));
        report(sess, &format!("ghost code is not erasable: {}", v.msg), v.span, &notes);
    }
    program.ghost_errors.len()
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
        let loader = cli.loader(&sess, &arena);
        for (file, (path, mutant)) in cli.files.iter().zip(cli.inputs()) {
            let loaded = loader.load(path, mutant)?;
            let program = prog::build(&sess, &arena, &loaded.units);
            for (sp, msg) in &program.errors {
                report(&sess, msg, *sp, &[]);
                fe_errors += 1;
            }
            if !program.ghost_errors.is_empty() {
                // Not erasable: nothing about this file is verified.
                fe_errors += report_ghost(&sess, &program);
                continue;
            }
            for c in program.contracts.iter().filter(|c| !c.kind.is_interface() && !c.imported) {
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
                    cmd.env("SOLAR_VERIFY_CHILD", "1").args([
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
                    cmd.args(cli.imports.args());
                    if let Some(original) = &cli.mutant_of {
                        cmd.arg("--mutant-of").arg(original);
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
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "mutate") {
        mutate::main(args.skip(1));
    }
    let cli = parse_cli();
    if cli.spec_check {
        spec_check(&cli);
    }
    if cli.jobs > 1 && cli.only.is_none() && cli.dump_smt.is_none() && cli.instrument.is_none() {
        run_parallel(&cli);
    }
    let child = std::env::var_os("SOLAR_VERIFY_CHILD").is_some();
    let sess = session();
    let mut total_ok = 0;
    let mut total_err = 0;
    let mut smt_log = String::new();
    let mut json_rows: Vec<String> = vec![];
    let ok = sess.enter_sequential(|| -> Result<(), ()> {
        let arena = ast::Arena::new();
        let loader = cli.loader(&sess, &arena);
        for (path, mutant) in cli.inputs() {
            let loaded = loader.load(path, mutant)?;
            let program = prog::build(&sess, &arena, &loaded.units);
            if let Some(out) = &cli.instrument {
                instrument(&sess, &program, &loaded.file, out);
                std::process::exit(if program.errors.is_empty() { 0 } else { 1 });
            }
            if let Some(out) = &cli.emit_lean {
                std::process::exit(emit_lean(&sess, &cli, &program, &loaded.units, &loaded.file, path, out));
            }
            if !child {
                for (sp, msg) in &program.errors {
                    report(&sess, msg, *sp, &[]);
                    total_err += 1;
                }
            }
            if !program.ghost_errors.is_empty() {
                // Ghost code that is not erasable: the proof would not be about the deployed
                // program, so nothing in this file is verified.
                total_err += report_ghost(&sess, &program);
                continue;
            }
            let opts = vc::Opts { strict: cli.strict };
            let mut mk = || {
                if cli.dry_run {
                    smt::Smt::dry(false)
                } else {
                    smt::Smt::new(cli.timeout, false).expect("could not start z3 (set SOLAR_VERIFY_Z3)")
                }
            };
            let proved = broadcast::prove_all(&program, &opts, cli.dry_run, &mut mk);
            for c in &program.contracts {
                if c.kind.is_interface() || c.imported {
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
                        smt::Smt::new(cli.timeout, cli.dump_smt.is_some()).expect("could not start z3 (set SOLAR_VERIFY_Z3)")
                    };
                    let mut v = vc::Vc::new(&program, c, smt, &opts);
                    v.proved = proved.clone();
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
                            esc(&path.display().to_string()), esc(&rep.name), t0.elapsed().as_millis(), rep.checks, msgs.join(",")
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

/// --spec-check: test the trusted contracts of each file against their code (see `speccheck`).
fn spec_check(cli: &Cli) -> ! {
    let sess = session();
    let o = speccheck::Opts {
        runs: cli.spec_check_runs,
        seed: cli.seed,
        timeout: cli.timeout,
        only: cli.only.clone(),
    };
    let mut t = speccheck::Totals::default();
    let ok = sess.enter_sequential(|| -> Result<(), ()> {
        let arena = ast::Arena::new();
        let loader = cli.loader(&sess, &arena);
        for (file, (path, mutant)) in cli.files.iter().zip(cli.inputs()) {
            let loaded = loader.load(path, mutant)?;
            let program = prog::build(&sess, &arena, &loaded.units);
            for (sp, msg) in &program.errors {
                report(&sess, msg, *sp, &[]);
            }
            let unit = *loaded.units.last().expect("the file to verify is loaded");
            let r = speccheck::run(&sess, unit, &program, file, &o);
            t.held += r.held;
            t.violated += r.violated;
            t.untested += r.untested;
        }
        Ok(())
    });
    if ok.is_err() {
        std::process::exit(2);
    }
    println!(
        "spec-check results:: {} held, {} violated, {} not tested",
        t.held, t.violated, t.untested
    );
    std::process::exit(if t.violated == 0 { 0 } else { 1 });
}

/// Write the file with each \x60@custom:check P\x60 as \x60require(P, "...")\x60: at the start of
/// the function body for a function's checks, before the statement for a statement's. The checks
/// are the runtime half of a checked claim: verification assumed them.
/// Writes `file` with every `@custom:check` as a `require`. The verification assumed the checks
/// hold, so one in an imported file, which this does not rewrite, is an error.
fn instrument(
    sess: &Session,
    program: &prog::Program<'_>,
    file: &std::sync::Arc<solar_parse::interface::source_map::SourceFile>,
    out: &Path,
) {
    for (sp, msg) in &program.errors {
        report(sess, msg, *sp, &[]);
    }
    let in_file =
        |sp: Span| std::sync::Arc::ptr_eq(&sess.source_map().lookup_byte_offset(sp.lo()).sf, file);
    let mut imported_checks = vec![];
    let src = file.src.to_string();
    let off = |sp: Span| sess.source_map().lookup_byte_offset(sp.lo()).pos.0 as usize;
    let off_hi = |sp: Span| sess.source_map().lookup_byte_offset(sp.hi()).pos.0 as usize;
    let req = |text: &str| {
        let msg = text.replace(['"', '\\'], "'");
        format!("require({text}, \"solar-verify check failed: {msg}\");")
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
            if !in_file(b.span) {
                imported_checks.push(b.span);
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
        if !in_file(*sp) {
            imported_checks.push(*sp);
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
    if !imported_checks.is_empty() {
        for sp in imported_checks {
            sess.dcx
                .err("cannot instrument a `@custom:check` in an imported file")
                .span(sp)
                .note("the verification assumed it holds; instrument that file, or flatten it in")
                .emit();
        }
        std::process::exit(1);
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

/// Write the specs of `file` as a Lean module (see `spec_lean.rs`). Nothing is written when the file
/// has front-end errors, ghost code that is not erasable, or a spec the export does not translate.
fn emit_lean(
    sess: &Session,
    cli: &Cli,
    program: &prog::Program<'_>,
    units: &[&ast::SourceUnit<'_>],
    source: &solar_parse::interface::source_map::SourceFile,
    file: &Path,
    out: &Path,
) -> i32 {
    use sha2::Digest;
    let mut errors = 0;
    for (sp, msg) in &program.errors {
        report(sess, msg, *sp, &[]);
        errors += 1;
    }
    errors += report_ghost(sess, program);
    if errors > 0 {
        return 1;
    }
    // The manifest binds one source file: a spec that reaches into imported files is not bound.
    let [unit] = units else {
        sess.dcx.err("cannot export to Lean: the file imports other files").emit();
        return 1;
    };
    let digest: String =
        sha2::Sha256::digest(source.src.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Spec");
    let mut options = vec![];
    if cli.strict {
        options.push("--strict".to_string());
    }
    let opts = spec_lean::Opts {
        module: cli.lean_module.clone().unwrap_or_else(|| format!("SolidCore.Spec.Export.{stem}")),
        source: file.display().to_string(),
        source_sha256: digest,
        compiler: cli.lean_compiler.clone(),
        options,
        bytecode_sha256: cli.lean_bytecode_sha256.clone(),
    };
    match spec_lean::export(program, unit, &opts) {
        Ok(text) => {
            if let Err(e) = std::fs::write(out, text) {
                sess.dcx.err(format!("cannot write {}: {e}", out.display())).emit();
                return 1;
            }
            0
        }
        Err((sp, msg)) => {
            report(sess, &format!("cannot export to Lean: {msg}"), sp, &[]);
            1
        }
    }
}
