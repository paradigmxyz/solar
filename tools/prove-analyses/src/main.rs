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
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{BufWriter, Read, Write},
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
        Some(out) => out.clone(),
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
            if !name.ends_with(".smt2") || name.ends_with(".abstract.smt2") {
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
    let flags = COMPILE_FLAGS
        .captures_iter(&text)
        .flat_map(|c| c[1].split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .filter(|flag| flag.starts_with("--evm-version") || flag.starts_with("-Zmir-pipeline"));
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
            name.ends_with(".smt2") && !name.ends_with(".abstract.smt2")
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
    batch_limit: u64,
    limit: u64,
    timeout: Duration,
    memory: u64,
}

impl Solver {
    fn new(args: &Args) -> Result<Self, String> {
        let z3 = args.z3.clone().unwrap_or_else(z3_binary);
        Command::new(&z3)
            .arg("--version")
            .output()
            .map_err(|e| format!("cannot run {}: {e}; pass --z3", z3.display()))?;
        Ok(Self {
            z3,
            bitwuzla: args.bitwuzla.clone(),
            batch_limit: args.rlimit_batch,
            limit: args.rlimit,
            timeout: Duration::from_secs(args.timeout),
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
                if verdict == "unsat" {
                    let tier = if abstract_of(query).exists() { "abstract" } else { "exact" };
                    record(query, source, "unsat", &format!("batch-{tier}"))
                } else {
                    self.alone(query, source)
                }
            })
            .collect()
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
            for line in
                text.lines().filter(|l| l.starts_with("(declare") || l.starts_with("(assert"))
            {
                script.push_str(line);
                script.push('\n');
            }
            script.push_str(&format!("(echo \"@{index}\")\n(check-sat)\n(pop)\n"));
        }
        let timeout = self.timeout * queries.len() as u32 + Duration::from_secs(10);
        let output = self.z3(script, timeout).unwrap_or_default();
        let mut verdicts = vec!["unknown".to_string(); queries.len()];
        let mut current = None;
        for line in output.lines() {
            if let Some(index) = line.strip_prefix('@').and_then(|i| i.parse::<usize>().ok()) {
                current = Some(index);
            } else if let Some(index) = current
                && matches!(line, "sat" | "unsat" | "unknown")
            {
                verdicts[index] = line.to_string();
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
        let mut lines = output.as_deref().map(str::lines).into_iter().flatten().skip(1);
        let mut record = record(query, source, verdict, tier);
        if verdict == "sat"
            && let Some(model) = lines.next()
        {
            record.insert("counterexample".into(), json!(model));
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
    for record in records {
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
