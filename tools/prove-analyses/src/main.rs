//! Proves the compiler's analysis facts for all inputs with Z3.
//!
//! Compiles each Solidity source with `-Zprove-analyses`, which writes one SMT-LIB query per
//! fact the run-time checks of `-Zcheck-analyses` would test, then runs Z3 on every query.
//! `unsat` proves the fact for every execution; `sat` is a counterexample to triage: an analysis
//! bug, or a case the encoding is too coarse to rule out (see the docs of `prove_analyses.rs`).
//! `sat-opaque` marks a counterexample whose goal depends on values the query leaves free (loads
//! of havocked memory or storage, calls, earlier loop passes), which usually means a missing fact;
//! a plain `sat` depends only on transaction inputs and is worth replaying. `trip-count` marks a
//! counterexample to a loop counter's gas-derived bound, which the queries take as an axiom.
//!
//! Z3 runs with deterministic resource limits, as Vox does. A function's queries are first tried
//! as one batch in one process, each under the low `--rlimit-batch` budget and in its abstract
//! tier when it has one. Queries the batch does not prove are retried alone in the exact tier
//! under the full `--rlimit`. Resource counts do not depend on machine load, so a run's verdicts
//! are reproducible; `--timeout` is only a wall-clock backstop.
//!
//! With `--certify`, every `unsat` verdict also gets a proof certificate: cvc5 proves the query
//! again with a CPC proof, which Ethos checks against it (see `solar_smt::certify`). The
//! certificate, `<query>.cpc` beside `<query>.ref.smt2`, moves trust from Z3 and Bitwuzla to
//! Ethos and CPC's rules, and `ethos <query>.cpc` checks it again without a solver. Certificates
//! have their own wall-clock limit, `--certify-timeout`, since cvc5 often answers `unsat` in
//! seconds and then needs minutes to print the proof.
//!
//! With `--certify-lean`, a verdict cvc5 and Ethos do not certify gets a Lean 4 certificate
//! instead: the query as a theorem, `<query>.lean`, that Lean proves with `bv_decide` (see
//! `solar_smt::certify::lean` for what it trusts), recorded as `checked-lean`. `lean <query>.lean`
//! checks it again.
//!
//! A source's `//@ compile-flags:` lines pass on its `--evm-version` and `-Zmir-pipeline` flags,
//! so MIR text with hand-written `assume` records runs too.
//!
//! ```sh
//! cargo build --release && cargo run --release -p solar-prove-analyses -- tests/ui/codegen
//! ```

// A standalone tool over query files, not compiler sources.
#![allow(clippy::disallowed_methods)]

use clap::Parser;
use rayon::prelude::*;
use regex::Regex;
use ruint::aliases::U256;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use solar_smt::{Certificate, Certifier, certify::lean::Lean, sexp};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::{LazyLock, Mutex},
    thread,
    time::{Duration, Instant},
};

type Record = Map<String, Value>;

static COMPILE_FLAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^//@ compile-flags:(.*)$").unwrap());
static HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"0x[0-9a-f]+").unwrap());

/// Proves the compiler's analysis facts for all inputs with Z3.
#[derive(Parser)]
struct Args {
    /// Solidity files or directories.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// The compiler binary.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/release/solar"))]
    solar: PathBuf,
    /// The Z3 binary: `SOLAR_PROVE_ANALYSES_Z3`, or `z3` on `PATH`.
    #[arg(long)]
    z3: Option<PathBuf>,
    /// A Bitwuzla binary to try on the queries Z3 leaves undecided alone, for `--timeout`
    /// seconds each. Its limit is wall-clock time, so these verdicts can vary between runs.
    #[arg(long)]
    bitwuzla: Option<PathBuf>,
    /// Comma-separated `-O` levels.
    #[arg(long, default_value = "gas,size", value_delimiter = ',')]
    levels: Vec<String>,
    /// Wall-clock backstop per query, in seconds.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
    /// Z3 resource limit per query in a function's batch.
    #[arg(long, default_value_t = 1_000_000)]
    rlimit_batch: u64,
    /// Z3 resource limit per query proved alone.
    #[arg(long, default_value_t = 40_000_000)]
    rlimit: u64,
    /// Z3 memory limit per process, in MB.
    #[arg(long, default_value_t = 1500)]
    memory: u64,
    /// Parallel compiles and solver processes.
    #[arg(long, short)]
    jobs: Option<usize>,
    /// Certify each `unsat` verdict with a cvc5 proof that Ethos checks, for
    /// `--certify-timeout` seconds each.
    #[arg(long)]
    certify: bool,
    /// Wall-clock limit for each certificate step (cvc5 proving and printing its proof, then
    /// Ethos checking it), in seconds. Proofs take longer than verdicts: cvc5 can answer `unsat`
    /// quickly and then spend minutes printing the proof.
    #[arg(long, default_value_t = 300)]
    certify_timeout: u64,
    /// The cvc5 binary for `--certify`: `SOLAR_PROVE_ANALYSES_CVC5`, or `cvc5` on `PATH`.
    #[arg(long)]
    cvc5: Option<PathBuf>,
    /// The Ethos binary for `--certify`: `SOLAR_PROVE_ANALYSES_ETHOS`, or `ethos` on `PATH`.
    #[arg(long)]
    ethos: Option<PathBuf>,
    /// cvc5's `proofs/eo/cpc/Cpc.eo`, the rules Ethos checks proofs with, for `--certify`:
    /// `SOLAR_PROVE_ANALYSES_CPC_SIGNATURE`.
    #[arg(long)]
    cpc_signature: Option<PathBuf>,
    /// Certify the verdicts `--certify` does not, or every verdict without it, with a Lean 4
    /// theorem that `bv_decide` proves, for `--certify-timeout` seconds each.
    #[arg(long)]
    certify_lean: bool,
    /// The Lean binary for `--certify-lean`: `SOLAR_PROVE_ANALYSES_LEAN`, or `lean` on `PATH`.
    #[arg(long)]
    lean: Option<PathBuf>,
    /// The directory Lean runs in for `--certify-lean`, such as a project whose `lean-toolchain`
    /// picks elan's toolchain: `SOLAR_PROVE_ANALYSES_LEAN_PROJECT`.
    #[arg(long)]
    lean_project: Option<PathBuf>,
    /// Keep queries and write `results.jsonl` here.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if let Some(jobs) = args.jobs {
        rayon::ThreadPoolBuilder::new().num_threads(jobs).build_global().unwrap();
    }
    let solver = match Solver::new(&args) {
        Ok(solver) => solver,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let out = match &args.out {
        // Compiles run in their sources' directories.
        Some(out) => std::path::absolute(out).unwrap(),
        None => tempfile::Builder::new().prefix("prove-analyses-").tempdir().unwrap().keep(),
    };
    let solar = fs::canonicalize(&args.solar).unwrap_or_else(|_| args.solar.clone());

    let files = sources(&args.paths);
    let mut jobs = Vec::new();
    for file in &files {
        let file = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        let digest = hex(&Sha256::digest(file.to_string_lossy().as_bytes()))[..12].to_string();
        for level in &args.levels {
            jobs.push((file.clone(), level.clone(), out.join(format!("{digest}-{level}"))));
        }
    }
    let counts: Vec<usize> =
        jobs.par_iter().map(|(file, level, dir)| compile(&solar, file, level, dir)).collect();
    let origin: HashMap<PathBuf, String> =
        jobs.iter().map(|(file, _, dir)| (dir.clone(), file.display().to_string())).collect();

    let mut groups: BTreeMap<(PathBuf, String), Vec<PathBuf>> = BTreeMap::new();
    for (_, _, dir) in &jobs {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_query(&name) {
                continue;
            }
            let function = name.split('-').take(2).collect::<Vec<_>>().join("-");
            groups.entry((dir.clone(), function)).or_default().push(path);
        }
    }
    for queries in groups.values_mut() {
        queries.sort();
    }
    eprintln!(
        "{} sources, {} compiles, {} queries",
        files.len(),
        jobs.len(),
        counts.iter().sum::<usize>()
    );

    let results_path = out.join("results.jsonl");
    let results = Mutex::new(BufWriter::new(fs::File::create(&results_path).unwrap()));
    let records: Vec<Record> = groups
        .par_iter()
        .flat_map_iter(|((dir, _), queries)| {
            let source = origin.get(dir).cloned().unwrap_or_default();
            let records = solver.solve_group(queries, &source);
            let mut results = results.lock().unwrap();
            for record in &records {
                writeln!(results, "{}", Value::Object(record.clone())).unwrap();
            }
            results.flush().unwrap();
            records
        })
        .collect();
    summarize(&records);
    eprintln!("results: {}", results_path.display());
    ExitCode::SUCCESS
}

/// The Solidity files under `paths`, skipping auxiliary sources.
fn sources(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "sol" || ext == "mir") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut out);
        } else {
            out.push(path.clone());
        }
    }
    out.retain(|path| !path.components().any(|c| c.as_os_str() == "auxiliary"));
    out
}

/// Compiles `source` at `level`, writing its queries to `out`, and returns how many it wrote.
fn compile(solar: &Path, source: &Path, level: &str, out: &Path) -> usize {
    let text =
        fs::read(source).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let flags = passed_flags(&text);
    // Queries an earlier run left behind, an abstract tier above all, would be solved as this
    // compile's.
    match fs::remove_dir_all(out) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            panic!("clearing {}: {error}", out.display())
        }
        _ => {}
    }
    fs::create_dir_all(out).unwrap();
    let mut command = Command::new(solar);
    command
        .args(["-O", level])
        .arg(format!("-Zprove-analyses={}", out.display()))
        .arg("--emit=bin-runtime")
        .args(flags)
        .arg(source)
        .current_dir(source.parent().unwrap_or(Path::new(".")));
    if run(command, None, Duration::from_secs(300)).is_none() {
        return 0;
    }
    fs::read_dir(out)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            is_query(&name)
        })
        .count()
}

/// Runs `command` with `input` on stdin and returns its stdout, or `None` past `timeout`.
fn run(mut command: Command, input: Option<String>, timeout: Duration) -> Option<String> {
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let writer = input.map(|input| {
        let mut stdin = child.stdin.take().unwrap();
        thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        })
    });
    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        output
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    reader.join().ok()
}

struct Solver {
    z3: PathBuf,
    bitwuzla: Option<PathBuf>,
    certifier: Option<Certifier>,
    lean: Option<Lean>,
    batch_limit: u64,
    limit: u64,
    timeout: Duration,
    certify_timeout: Duration,
    memory: u64,
}

impl Solver {
    fn new(args: &Args) -> Result<Self, String> {
        let z3 = args.z3.clone().unwrap_or_else(z3_binary);
        Command::new(&z3)
            .arg("--version")
            .output()
            .map_err(|e| format!("cannot run {}: {e}; pass --z3", z3.display()))?;
        let binary = |given: &Option<PathBuf>, var: &str, default: &str| {
            given.clone().unwrap_or_else(|| {
                std::env::var_os(var).map_or_else(|| default.into(), PathBuf::from)
            })
        };
        let certifier = if args.certify {
            Some(Certifier::new(
                binary(&args.cvc5, "SOLAR_PROVE_ANALYSES_CVC5", "cvc5"),
                binary(&args.ethos, "SOLAR_PROVE_ANALYSES_ETHOS", "ethos"),
                binary(&args.cpc_signature, "SOLAR_PROVE_ANALYSES_CPC_SIGNATURE", ""),
            )?)
        } else {
            None
        };
        let lean = if args.certify_lean {
            let project = args.lean_project.clone().or_else(|| {
                std::env::var_os("SOLAR_PROVE_ANALYSES_LEAN_PROJECT").map(PathBuf::from)
            });
            Some(Lean::new(binary(&args.lean, "SOLAR_PROVE_ANALYSES_LEAN", "lean"), project)?)
        } else {
            None
        };
        Ok(Self {
            z3,
            bitwuzla: args.bitwuzla.clone(),
            certifier,
            lean,
            batch_limit: args.rlimit_batch,
            limit: args.rlimit,
            timeout: Duration::from_secs(args.timeout),
            certify_timeout: Duration::from_secs(args.certify_timeout),
            memory: args.memory,
        })
    }

    fn solve_group(&self, queries: &[PathBuf], source: &str) -> Vec<Record> {
        let paths: Vec<&Path> = queries.iter().map(PathBuf::as_path).collect();
        let verdicts = self.batch(&paths);
        queries
            .iter()
            .zip(verdicts)
            .map(|(query, verdict)| {
                let mut record = if verdict == "unsat" {
                    let tier = if abstract_of(query).exists() { "abstract" } else { "exact" };
                    record(query, source, "unsat", &format!("batch-{tier}"))
                } else {
                    self.alone(query, source)
                };
                if (self.certifier.is_some() || self.lean.is_some())
                    && record.get("result").and_then(Value::as_str) == Some("unsat")
                {
                    let (certificate, tier, lean) = self.certify(query);
                    record.insert("certificate".into(), json!(certificate.describe()));
                    record.insert("certified_tier".into(), json!(tier));
                    if let Some(lean) = lean {
                        record.insert("lean_certificate".into(), json!(lean.describe()));
                    }
                }
                record
            })
            .collect()
    }

    /// A certificate for a proved query: of the exact query, or else of its abstract tier, which
    /// implies it; from cvc5 and Ethos, or else from Lean. Without a certificate, the first
    /// failure, and Lean's when it was tried.
    fn certify(&self, query: &Path) -> (Certificate, &'static str, Option<Certificate>) {
        let mut tiers = vec![("exact", query.to_path_buf())];
        let tier = abstract_of(query);
        if tier.exists() {
            tiers.push(("abstract", tier));
        }
        let mut failure = None;
        if let Some(certifier) = &self.certifier {
            for (name, path) in &tiers {
                let text = fs::read_to_string(path).unwrap_or_default();
                match certifier.certify(&text, &path.with_extension(""), self.certify_timeout) {
                    Certificate::Checked => return (Certificate::Checked, name, None),
                    certificate => failure = failure.or(Some(certificate)),
                }
            }
        }
        let mut lean_failure = None;
        if let Some(lean) = &self.lean {
            for (name, path) in &tiers {
                let text = fs::read_to_string(path).unwrap_or_default();
                match lean.certify(&text, &path.with_extension(""), self.certify_timeout) {
                    Certificate::CheckedLean => return (Certificate::CheckedLean, name, None),
                    certificate => lean_failure = lean_failure.or(Some(certificate)),
                }
            }
        }
        match failure {
            Some(failure) => (failure, "exact", lean_failure),
            None => (lean_failure.unwrap_or(Certificate::NoProof("unknown")), "exact", None),
        }
    }

    fn batch(&self, queries: &[&Path]) -> Vec<String> {
        if queries.is_empty() {
            return Vec::new();
        }
        let mut script = format!(
            "(set-option :print-success false)\n(set-logic QF_AUFBV)\n(set-option :rlimit {})\n",
            self.batch_limit
        );
        for (index, query) in queries.iter().enumerate() {
            let tier = abstract_of(query);
            let text =
                fs::read_to_string(if tier.exists() { &tier } else { *query }).unwrap_or_default();
            script.push_str("(push)\n");
            // The query's declarations, definitions and assertions; its own logic and
            // `check-sat` are the batch's.
            for command in sexp::parse(&text).map_while(Result::ok).filter(|command| {
                command.head().is_some_and(|head| {
                    head.starts_with("declare-") || head.starts_with("define-") || head == "assert"
                })
            }) {
                script.push_str(&format!("{command}\n"));
            }
            script.push_str(&format!("(echo \"@{index}\")\n(check-sat)\n(pop)\n"));
        }
        let timeout = self.timeout * queries.len() as u32 + Duration::from_secs(10);
        let output = self.z3(script, timeout).unwrap_or_default();
        let mut verdicts = vec!["unknown".to_string(); queries.len()];
        let mut current = None;
        for response in sexp::parse(&output).map_while(Result::ok) {
            let Some(atom) = response.as_atom() else { continue };
            if let Some(index) = atom.strip_prefix('@').and_then(|i| i.parse::<usize>().ok()) {
                current = Some(index);
            } else if let Some(index) = current
                && matches!(atom, "sat" | "unsat" | "unknown")
            {
                verdicts[index] = atom.to_string();
                current = None;
            }
        }
        verdicts
    }

    fn alone(&self, query: &Path, source: &str) -> Record {
        let script = format!(
            "(set-option :rlimit {})\n{}",
            self.limit,
            fs::read_to_string(query).unwrap_or_default()
        );
        let mut output = self.z3(script, self.timeout + Duration::from_secs(10));
        let mut tier = "exact";
        if !matches!(verdict_of(output.as_deref()), "sat" | "unsat")
            && let Some(bitwuzla) = &self.bitwuzla
        {
            // Bitwuzla's abstraction of wide multiplication and division decides many of the
            // queries Z3 runs out of resources on.
            let mut command = Command::new(bitwuzla);
            let limit = self.timeout.as_millis().to_string();
            let memory = self.memory.to_string();
            command.args(["-m", "-t", &limit, "-M", &memory]).arg(query);
            let fallback = run(command, None, self.timeout + Duration::from_secs(10));
            if matches!(verdict_of(fallback.as_deref()), "sat" | "unsat") {
                output = fallback;
                tier = "bitwuzla";
            }
        }
        let verdict = verdict_of(output.as_deref());
        let mut record = record(query, source, verdict, tier);
        // The `get-value` answer after the verdict line, which spans a line per goal.
        let values = output
            .as_deref()
            .and_then(|output| output.split_once('\n'))
            .and_then(|(_, rest)| sexp::parse(rest).next()?.ok());
        if verdict == "sat"
            && let Some(values) = values
        {
            record.insert("counterexample".into(), json!(values.to_string()));
            if is_trip_count_bound(&record) {
                record.insert("result".into(), json!("trip-count"));
            } else if record.get("opaque").and_then(Value::as_str).is_some_and(|o| !o.is_empty()) {
                // The model picks values for loads, calls or loop state the query does not
                // follow: it may show a missing fact rather than a bug.
                record.insert("result".into(), json!("sat-opaque"));
            }
        }
        record
    }

    fn z3(&self, script: String, timeout: Duration) -> Option<String> {
        let mut command = Command::new(&self.z3);
        command.args(["-in", "-smt2", &format!("-memory:{}", self.memory)]);
        run(command, Some(script), timeout)
    }
}

/// A solver's verdict from its output, or `timeout` without one.
fn verdict_of(output: Option<&str>) -> &str {
    let Some(output) = output else { return "timeout" };
    match output.lines().next() {
        Some(line) if line.starts_with("(error") && line.contains("memory") => "memout",
        Some(line) if line.starts_with("(error") => "error",
        Some(line) => line,
        None => "error",
    }
}

/// A query's verdict with the facts its header comments give.
fn record(query: &Path, source: &str, verdict: &str, tier: &str) -> Record {
    let mut record = Record::new();
    record.insert("query".into(), json!(query.display().to_string()));
    record.insert("source".into(), json!(source));
    record.insert("result".into(), json!(verdict));
    record.insert("tier".into(), json!(tier));
    let text = fs::read_to_string(query).unwrap_or_default();
    for line in text.lines().take_while(|line| line.starts_with(';')) {
        let line = line.trim_start_matches([';', ' ']);
        let (key, value) = line.split_once(": ").unwrap_or((line, ""));
        record.insert(key.into(), json!(value));
    }
    record
}

/// Whether a file in a compile's output directory is a query, rather than a query's abstract
/// tier or a certificate's reference script.
fn is_query(name: &str) -> bool {
    name.ends_with(".smt2") && !name.ends_with(".abstract.smt2") && !name.ends_with(".ref.smt2")
}

fn abstract_of(query: &Path) -> PathBuf {
    let name = query.file_name().unwrap().to_string_lossy().replace(".smt2", ".abstract.smt2");
    query.with_file_name(name)
}

/// Whether a counterexample is to the bound check-elim derives from the gas limit.
///
/// A counter with a constant start and step cannot travel more than `step << 64` from its
/// start, since every iteration burns gas. The queries do not model gas, so these claims are
/// axioms the run-time checks test.
fn is_trip_count_bound(record: &Record) -> bool {
    let field = |key: &str| record.get(key).and_then(Value::as_str).unwrap_or_default();
    let claim = field("claim");
    if field("definition") != "phi"
        || !(claim.starts_with("check-elim") || claim.starts_with("check_elim"))
    {
        return false;
    }
    let bounds: Vec<U256> = HEX
        .find_iter(claim)
        .filter_map(|m| U256::from_str_radix(&m.as_str()[2..], 16).ok())
        .collect();
    let word = U256::from(1u8) << 64;
    match bounds[..] {
        // A recorded bound `phi ule start + step << 64`.
        [bound] => bound >= word && bound % word < U256::from(1u64 << 32),
        [start, end, ..] => end > start && (end - start) % word == U256::ZERO,
        [] => false,
    }
}

fn summarize(records: &[Record]) {
    let field = |record: &Record, key: &str| {
        record.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
    };
    let mut table: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut tiers: BTreeMap<String, usize> = BTreeMap::new();
    let mut axioms: BTreeMap<String, usize> = BTreeMap::new();
    let mut certificates: BTreeMap<String, usize> = BTreeMap::new();
    for record in records {
        let certificate = field(record, "certificate");
        if let Some(kind) = certificate.split_whitespace().next() {
            *certificates.entry(kind.to_string()).or_default() += 1;
        }
        let claim = field(record, "claim");
        let analysis = claim.split_whitespace().next().unwrap_or("?").to_string();
        let result = field(record, "result");
        if result == "unsat" {
            *tiers.entry(field(record, "tier")).or_default() += 1;
        }
        *table.entry((analysis, result)).or_default() += 1;
        for axiom in field(record, "axioms").split_whitespace() {
            *axioms.entry(axiom.to_string()).or_default() += 1;
        }
    }
    for ((analysis, verdict), count) in &table {
        println!("{analysis:14} {verdict:12} {count}");
    }
    let join = |map: &BTreeMap<String, usize>, sep: &str| {
        map.iter().map(|(k, n)| format!("{k}{sep}{n}")).collect::<Vec<_>>().join(", ")
    };
    println!("proved by tier: {}", join(&tiers, " "));
    if !certificates.is_empty() {
        println!("certificates: {}", join(&certificates, " "));
    }
    let trusted: Vec<_> = axioms.iter().map(|(a, n)| format!("{a} in {n} queries")).collect();
    println!("trusted: trip-count bounds (gas), {}", trusted.join(", "));
    for record in records.iter().filter(|record| field(record, "result") == "sat") {
        println!(
            "COUNTEREXAMPLE {}\n  {}\n  {}: {}\n  {}",
            field(record, "source"),
            field(record, "query"),
            field(record, "function"),
            field(record, "claim"),
            field(record, "counterexample"),
        );
    }
}

/// The Z3 binary when `--z3` is not given.
fn z3_binary() -> PathBuf {
    std::env::var_os("SOLAR_PROVE_ANALYSES_Z3").map_or_else(|| "z3".into(), PathBuf::from)
}

/// The `--evm-version` and `-Zmir-pipeline` flags of a source's `//@ compile-flags:` lines, with
/// the value that follows a flag written without `=`.
fn passed_flags(text: &str) -> Vec<String> {
    let tokens: Vec<&str> = COMPILE_FLAGS
        .captures_iter(text)
        .flat_map(|c| c.get(1).map_or("", |m| m.as_str()).split_whitespace().collect::<Vec<_>>())
        .collect();
    let mut flags = Vec::new();
    let mut tokens = tokens.into_iter();
    while let Some(token) = tokens.next() {
        if !(token.starts_with("--evm-version") || token.starts_with("-Zmir-pipeline")) {
            continue;
        }
        flags.push(token.to_owned());
        if !token.contains('=')
            && let Some(value) = tokens.next()
        {
            flags.push(value.to_owned());
        }
    }
    flags
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_flag_values() {
        let text = "//@ compile-flags: --evm-version paris -Zmir-pipeline=sccp --emit=abi\n//@ compile-flags: --evm-version=cancun\n";
        assert_eq!(
            passed_flags(text),
            ["--evm-version", "paris", "-Zmir-pipeline=sccp", "--evm-version=cancun"]
        );
    }
}
