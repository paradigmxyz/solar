//! Foundry integration test support.
//!
//! The tests run `forge test` with both this compiler and solc, then compare
//! gas usage and test results.
//!
//! Run them with `cargo tq foundry`.
#![allow(clippy::uninlined_format_args, clippy::collapsible_if, clippy::disallowed_methods)]

use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

mod external;

// ============================================================================
// Configuration
// ============================================================================

/// How a project's results are judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssertionPolicy {
    /// In-repo projects: every test must pass under this compiler.
    InRepoStrict,
    /// External projects: solc's passing tests are the oracle; only
    /// Solc-pass/compiler-fail divergences and artifact audit errors fail.
    ExternalDifferential,
}

/// A skipped test or contract pattern with a mandatory reason.
#[derive(Debug, Clone)]
struct SkipEntry {
    /// Regex pattern, in the syntax forge accepts for `--no-match-*`.
    pattern: String,
    /// Why the entry is skipped.
    reason: String,
}

/// Configuration for running a test project.
#[derive(Debug, Clone)]
struct TestConfig {
    /// Project name (used for display).
    name: String,
    /// Foundry project root.
    path: PathBuf,
    /// Optional filter for test function names (substring match).
    test_filter: Option<String>,
    /// Optional filter for contract names (substring match).
    contract_filter: Option<String>,
    /// If true, only run with the compiler (no solc comparison).
    solar_only: bool,
    /// If true, defer this project until its compiler failures are fixed.
    ignored: bool,
    /// Skipped test name patterns, passed to `--no-match-test` and re-applied
    /// post-hoc so both legs judge the same set.
    skip_tests: Vec<SkipEntry>,
    /// Skipped contract name patterns, passed to `--no-match-contract` and
    /// re-applied post-hoc; also silences artifact audits for those names.
    skip_contracts: Vec<SkipEntry>,
    /// How results are judged.
    policy: AssertionPolicy,
    /// Compile with `forge build` instead of running tests; artifacts are
    /// still extracted and audited.
    build_only: bool,
    /// Fixed fuzz seed so both legs run identical fuzz and invariant inputs.
    fuzz_seed: Option<u64>,
    /// Solc version to emulate on the compiler leg (`SOLC_WRAPPER_VERSION`), for
    /// projects whose sources pin an exact solc version.
    solc_wrapper_version: Option<String>,
    /// Foundry profile to use for both compiler legs.
    foundry_profile: Option<String>,
    /// Pass `-vvvvv --decode-internal` to `forge test`.
    traces: bool,
    /// Command shown in runtime reports for reproducing this run.
    rerun_command: String,
}

impl TestConfig {
    /// Creates a config for an in-repo project under `tests/foundry`.
    fn in_repo(name: String, path: PathBuf, solar_only: bool, ignored: bool) -> Self {
        Self {
            name,
            path,
            test_filter: None,
            contract_filter: None,
            solar_only,
            ignored,
            skip_tests: Vec::new(),
            skip_contracts: Vec::new(),
            policy: AssertionPolicy::InRepoStrict,
            build_only: false,
            fuzz_seed: None,
            solc_wrapper_version: None,
            foundry_profile: None,
            traces: true,
            rerun_command: "cargo tq foundry".to_string(),
        }
    }

    /// Runs the test with this configuration.
    fn run(&self) {
        run_test_with_config(self);
    }
}

// ============================================================================
// Internal Types
// ============================================================================

/// Result of a single test.
#[derive(Debug, Clone)]
struct TestResult {
    name: String,
    contract: String,
    passed: bool,
    gas: u64,
}

/// Result of running a compiler on a project.
#[derive(Debug)]
#[allow(dead_code)]
struct CompilerRun {
    compiler: String,
    compile_time: Duration,
    tests: Vec<TestResult>,
    total_passed: usize,
    total_failed: usize,
    bytecode_sizes: HashMap<String, usize>,
    internal_only_library_stubs: HashSet<String>,
}

/// Data extracted from a compiler's Foundry artifacts.
#[derive(Debug, Default)]
struct ArtifactData {
    bytecode_sizes: HashMap<String, usize>,
    internal_only_library_stubs: HashSet<String>,
}

struct FoundrySolc {
    path: PathBuf,
}

impl FoundrySolc {
    fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Copy)]
enum ForgeCompiler {
    Solar,
    Solc,
}

impl ForgeCompiler {
    fn cache_prefix(self) -> &'static str {
        match self {
            Self::Solar => "solar-foundry-cache-",
            Self::Solc => "solc-foundry-cache-",
        }
    }

    fn out_prefix(self) -> &'static str {
        match self {
            Self::Solar => "solar-foundry-out-",
            Self::Solc => "solc-foundry-out-",
        }
    }

    fn command_failure(self) -> &'static str {
        match self {
            Self::Solar => "failed to run forge test for Solar",
            Self::Solc => "failed to run forge test",
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

static SOLAR_BINARY: OnceLock<PathBuf> = OnceLock::new();

const TEMPORARILY_IGNORED_PROJECTS: &[&str] = &[
    "abi-encoding",
    "equivalence",
    "erc20-minimal",
    "erc721-minimal",
    "multicall",
    "stress-arrays",
    "stress-inheritance",
    "unifap-v2",
    "unifap-v2-create",
    "vault-minimal",
];

fn foundry_root() -> PathBuf {
    workspace_root().join("tests/foundry")
}

fn discover_projects(root: &Path) -> Vec<TestConfig> {
    assert!(root.is_dir(), "Foundry test root does not exist: {}", root.display());
    let selected = std::env::var_os("SOLAR_FOUNDRY_PROJECT");

    let mut paths = Vec::new();
    discover_project_paths(root, &mut paths);
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(root).expect("Foundry project outside root");
            let name = relative.to_string_lossy().replace('\\', "/");
            if selected.as_deref().is_some_and(|selected| selected != name.as_str()) {
                return None;
            }
            let solar_only = relative == Path::new("stack-deep");
            let ignored = TEMPORARILY_IGNORED_PROJECTS.contains(&name.as_str());

            Some(TestConfig::in_repo(name, path, solar_only, ignored))
        })
        .collect()
}

fn discover_project_paths(dir: &Path, projects: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| {
            panic!("failed to read Foundry test directory {}: {error}", dir.display())
        })
        .map(|entry| entry.expect("failed to read Foundry test directory entry").path())
        .collect();
    entries.sort();

    if entries.iter().any(|path| path.file_name().is_some_and(|name| name == "foundry.toml")) {
        projects.push(dir.to_path_buf());
        return;
    }

    for path in entries {
        if path.is_dir() {
            discover_project_paths(&path, projects);
        }
    }
}

/// Gets the path to the compiler binary.
///
/// Uses the binary supplied by the compiler test runner when available and
/// falls back to a binary on disk for this crate's unit tests.
fn get_solar_binary() -> PathBuf {
    if let Some(path) = SOLAR_BINARY.get() {
        return path.clone();
    }

    if let Some(path) = option_env!("CARGO_BIN_EXE_solar") {
        return PathBuf::from(path);
    }

    let workspace_root = workspace_root();
    let release_binary = workspace_root.join("target/release/solar");
    if release_binary.exists() {
        return release_binary;
    }
    workspace_root.join("target/debug/solar")
}

/// Gets the path to the workspace root.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// A `solc`-compatible executable for `FOUNDRY_SOLC`.
fn foundry_solc() -> FoundrySolc {
    FoundrySolc { path: get_solar_binary() }
}

/// Checks if forge is available.
fn forge_available() -> bool {
    Command::new("forge").arg("--version").output().is_ok()
}

/// Filters tests based on config.
///
/// Skip patterns are also passed to forge as `--no-match-*`, but are re-applied
/// here so both compiler legs judge the same set even if forge's flag
/// semantics drift.
fn filter_tests(tests: Vec<TestResult>, config: &TestConfig) -> Vec<TestResult> {
    let skip_tests = compile_skips(&config.skip_tests);
    let skip_contracts = compile_skips(&config.skip_contracts);
    tests
        .into_iter()
        .filter(|t| {
            let test_match =
                config.test_filter.as_ref().map(|f| t.name.contains(f)).unwrap_or(true);
            let contract_match =
                config.contract_filter.as_ref().map(|f| t.contract.contains(f)).unwrap_or(true);
            test_match
                && contract_match
                && !skip_tests.iter().any(|re| re.is_match(&t.name))
                && !skip_contracts.iter().any(|re| re.is_match(&t.contract))
        })
        .collect()
}

/// Compiles skip patterns, panicking on invalid ones: they come from the
/// curated manifest, so an invalid pattern is a bug in the manifest.
fn compile_skips(skips: &[SkipEntry]) -> Vec<Regex> {
    skips
        .iter()
        .map(|skip| {
            Regex::new(&skip.pattern)
                .unwrap_or_else(|error| panic!("invalid skip pattern `{}`: {error}", skip.pattern))
        })
        .collect()
}

/// Combines skip patterns into one alternation regex for forge `--no-match-*`.
fn combine_skips(skips: &[SkipEntry]) -> Option<String> {
    if skips.is_empty() {
        return None;
    }
    Some(skips.iter().map(|skip| format!("(?:{})", skip.pattern)).collect::<Vec<_>>().join("|"))
}

// ============================================================================
// Parsing & Extraction
// ============================================================================

/// Parses test results from forge JSON output.
///
/// Diagnostics can precede the JSON on stdout (e.g. cheatcode `ffi` error
/// logs), so parsing starts at the first line that looks like JSON.
fn parse_test_results(stdout: &str) -> Vec<TestResult> {
    let mut tests = Vec::new();

    let json_start = stdout.find("\n{").map(|i| i + 1).unwrap_or(0);
    let stdout = &stdout[json_start..];
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(obj) = json.as_object() {
            for (contract_path, contract_data) in obj {
                // Extract contract name from path (e.g., "src/Test.t.sol:TestContract")
                let contract_name =
                    contract_path.rsplit(':').next().unwrap_or(contract_path).to_string();

                if let Some(test_results) = contract_data.get("test_results") {
                    if let Some(tests_obj) = test_results.as_object() {
                        for (name, result) in tests_obj {
                            let passed = result
                                .get("status")
                                .and_then(|s| s.as_str())
                                .map(|s| s == "Success")
                                .unwrap_or(false);
                            let gas = result
                                .get("kind")
                                .and_then(|k| k.get("Unit"))
                                .and_then(|u| u.get("gas"))
                                .and_then(|g| g.as_u64())
                                .unwrap_or(0);
                            tests.push(TestResult {
                                name: name.clone(),
                                contract: contract_name.clone(),
                                passed,
                                gas,
                            });
                        }
                    }
                }
            }
        }
    }

    tests
}

/// Returns whether an artifact contains only one of solc's non-callable
/// library stubs. Depending on the compiler and optimizer, internal-only
/// libraries either revert immediately or first check their deployment
/// address; the remaining bytes are metadata.
fn is_internal_only_library_stub(json: &serde_json::Value, bytecode: &str) -> bool {
    let abi_has_callable_entry =
        json.get("abi").and_then(serde_json::Value::as_array).is_none_or(|abi| {
            abi.iter().any(|entry| {
                entry
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|kind| matches!(kind, "function" | "fallback" | "receive"))
            })
        });
    let methods_are_empty = json
        .get("methodIdentifiers")
        .and_then(serde_json::Value::as_object)
        .is_some_and(serde_json::Map::is_empty);
    if abi_has_callable_entry || !methods_are_empty {
        return false;
    }

    let bytecode = bytecode.strip_prefix("0x").unwrap_or(bytecode);
    const ADDRESS_GUARD: &str = "7300000000000000000000000000000000000000003014";
    const GUARDED_REVERTS: [&str; 4] = [
        "60806040525f80fdfe",
        "60806040525f5ffdfe",
        "6080604052600080fdfe",
        "608060405260006000fdfe",
    ];
    const BARE_REVERTS: [&str; 4] = ["5f80fdfe", "5f5ffdfe", "600080fdfe", "60006000fdfe"];
    BARE_REVERTS.iter().any(|stub| bytecode.starts_with(stub))
        || bytecode
            .strip_prefix(ADDRESS_GUARD)
            .is_some_and(|rest| GUARDED_REVERTS.iter().any(|stub| rest.starts_with(stub)))
}

/// Extracts deployed artifact data from a forge output directory.
fn extract_artifact_data(out_path: &Path) -> ArtifactData {
    let mut artifacts = ArtifactData::default();

    if let Ok(entries) = std::fs::read_dir(out_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip test contract artifacts
                let dir_name = path.file_name().unwrap().to_string_lossy();
                if dir_name.ends_with(".t.sol") {
                    continue;
                }

                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let file_path = file.path();
                        if file_path.extension().is_some_and(|e| e == "json") {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&content)
                                {
                                    if let Some(bytecode) = json
                                        .get("deployedBytecode")
                                        .and_then(|b| b.get("object"))
                                        .and_then(|o| o.as_str())
                                    {
                                        let hex = bytecode.strip_prefix("0x").unwrap_or(bytecode);
                                        let size = hex.len() / 2;
                                        if size > 0 {
                                            let name = file_path
                                                .file_stem()
                                                .unwrap()
                                                .to_string_lossy()
                                                .to_string();
                                            if is_internal_only_library_stub(&json, bytecode) {
                                                artifacts
                                                    .internal_only_library_stubs
                                                    .insert(name.clone());
                                            }
                                            artifacts.bytecode_sizes.insert(name, size);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    artifacts
}

fn duration_millis(duration: Duration) -> u128 {
    duration.as_millis()
}

fn compiler_run_json(run: &CompilerRun) -> serde_json::Value {
    let tests = run
        .tests
        .iter()
        .map(|test| {
            serde_json::json!({
                "name": test.name.as_str(),
                "contract": test.contract.as_str(),
                "passed": test.passed,
                "gas": test.gas,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "compiler": run.compiler,
        "compile_time_ms": duration_millis(run.compile_time),
        "total_passed": run.total_passed,
        "total_failed": run.total_failed,
        "bytecode_sizes": run.bytecode_sizes,
        "tests": tests,
    })
}

fn report_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + ".json".len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.push_str(".json");
    out
}

fn write_runtime_report(
    config: &TestConfig,
    solar_run: &CompilerRun,
    solc_run: Option<&CompilerRun>,
) {
    let Some(report_dir) = std::env::var_os("SOLAR_FOUNDRY_REPORT_DIR") else {
        return;
    };

    let report_dir = PathBuf::from(report_dir);
    let report_dir =
        if report_dir.is_absolute() { report_dir } else { workspace_root().join(report_dir) };
    fs::create_dir_all(&report_dir).expect("failed to create Foundry report directory");

    let report = serde_json::json!({
        "project": {
            "name": config.name.as_str(),
            "path": config.path.display().to_string(),
            "test_filter": config.test_filter.as_deref(),
            "contract_filter": config.contract_filter.as_deref(),
            "solar_only": config.solar_only,
        },
        "rerun": {
            "command": config.rerun_command.as_str(),
            "env": {
                "SOLAR_FOUNDRY_REPORT_DIR": report_dir.display().to_string(),
            },
        },
        "solar": compiler_run_json(solar_run),
        "solc": solc_run.map(compiler_run_json),
    });

    let path = report_dir.join(report_file_name(&config.name));
    let json = serde_json::to_string_pretty(&report).expect("failed to serialize Foundry report");
    fs::write(&path, json).expect("failed to write Foundry report");
}

// ============================================================================
// Forge Execution
// ============================================================================

/// Runs forge test for a compiler.
fn run_forge_test(
    project_dir: &Path,
    label: &str,
    config: &TestConfig,
    compiler: ForgeCompiler,
) -> (Duration, Vec<TestResult>, ArtifactData) {
    let cache_dir = tempfile::Builder::new()
        .prefix(compiler.cache_prefix())
        .tempdir()
        .expect("failed to create Foundry cache directory");
    let out_dir = tempfile::Builder::new()
        .prefix(compiler.out_prefix())
        .tempdir()
        .expect("failed to create Foundry output directory");
    let foundry_solc = match compiler {
        ForgeCompiler::Solar => Some(foundry_solc()),
        ForgeCompiler::Solc => None,
    };

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir);
    if config.build_only {
        cmd.arg("build").arg("--force").arg("--no-lint");
    } else {
        cmd.arg("test").arg("--force").arg("--json");
        if config.traces {
            cmd.arg("-vvvvv").arg("--decode-internal");
        }
    }
    cmd.arg("--out").arg(out_dir.path()).arg("--cache-path").arg(cache_dir.path());

    if let Some(profile) = &config.foundry_profile {
        cmd.env("FOUNDRY_PROFILE", profile);
    }

    if let Some(foundry_solc) = &foundry_solc {
        // Foundry expects solc-compatible `--version` output when probing `FOUNDRY_SOLC`.
        cmd.env("SOLC_WRAPPER", "1").env("FOUNDRY_SOLC", foundry_solc.path());
        if let Some(version) = &config.solc_wrapper_version {
            cmd.env("SOLC_WRAPPER_VERSION", version);
        }
    }

    // Add forge match filters if specified
    if !config.build_only {
        if let Some(test_filter) = &config.test_filter {
            cmd.arg("--match-test").arg(test_filter);
        }
        if let Some(contract_filter) = &config.contract_filter {
            cmd.arg("--match-contract").arg(contract_filter);
        }
        if let Some(pattern) = combine_skips(&config.skip_tests) {
            cmd.arg("--no-match-test").arg(pattern);
        }
        if let Some(pattern) = combine_skips(&config.skip_contracts) {
            cmd.arg("--no-match-contract").arg(pattern);
        }
        if let Some(seed) = config.fuzz_seed {
            cmd.arg("--fuzz-seed").arg(seed.to_string());
        }
    }

    let start = Instant::now();
    let command_failure = compiler.command_failure();
    let output = cmd.output().unwrap_or_else(|err| panic!("{command_failure}: {err}"));
    let test_time = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);

    let failed = if config.build_only {
        !output.status.success()
    } else {
        !output.status.success() || stdout.contains("\"status\":\"Failure\"")
    };
    if failed {
        let what =
            if config.build_only { "forge build failed" } else { "forge test reported failures" };
        eprintln!("\n[{}] {what}", label);
        if !output.stderr.is_empty() {
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    let tests = if config.build_only { Vec::new() } else { parse_test_results(&stdout) };
    let artifacts = extract_artifact_data(out_dir.path());

    (test_time, tests, artifacts)
}

// ============================================================================
// Comparison & Reporting
// ============================================================================

/// Compares the compiler and solc test results and prints a diff summary.
fn print_test_diff(solar_tests: &[TestResult], solc_tests: &[TestResult], label: &str) {
    let solar_map: HashMap<&str, &TestResult> =
        solar_tests.iter().map(|t| (t.name.as_str(), t)).collect();
    let solc_map: HashMap<&str, &TestResult> =
        solc_tests.iter().map(|t| (t.name.as_str(), t)).collect();

    let mut regressions = Vec::new();
    let mut gas_diffs = Vec::new();

    for (name, solc_test) in &solc_map {
        match solar_map.get(name) {
            Some(solar_test) => {
                // Test exists in both - check for regression
                if solc_test.passed && !solar_test.passed {
                    regressions.push(*name);
                }
                // Track gas difference for passing tests
                if solar_test.passed && solc_test.passed && solc_test.gas > 0 {
                    let diff_pct = ((solar_test.gas as f64 / solc_test.gas as f64) - 1.0) * 100.0;
                    gas_diffs.push((*name, solar_test.gas, solc_test.gas, diff_pct));
                }
            }
            None => {
                // Test only in solc
                if solc_test.passed {
                    regressions.push(*name);
                }
            }
        }
    }

    if !regressions.is_empty() {
        eprintln!(
            "\n❌ [{}] REGRESSIONS: {} tests pass in solc but fail in Solar:",
            label,
            regressions.len()
        );
        for name in &regressions {
            eprintln!("   - {}", name);
        }
    }

    if !gas_diffs.is_empty() {
        eprintln!("\n⛽ [{}] Gas comparison (Solar vs solc):", label);
        for (name, solar_gas, solc_gas, diff_pct) in &gas_diffs {
            let indicator = if *diff_pct > 5.0 {
                "📈"
            } else if *diff_pct < -5.0 {
                "📉"
            } else {
                "≈"
            };
            eprintln!(
                "   {} {:40} Solar: {:>8} | solc: {:>8} | {:>+6.1}%",
                indicator, name, solar_gas, solc_gas, diff_pct
            );
        }
    }
}

/// Runs a full comparison between the compiler and solc for a project.
fn run_project_comparison(config: &TestConfig) -> (CompilerRun, CompilerRun) {
    let project_dir = &config.path;

    // Step 1: Run tests with the compiler
    let (solar_test_time, solar_tests, solar_artifacts) = run_forge_test(
        project_dir,
        &format!("{}-solar", config.name),
        config,
        ForgeCompiler::Solar,
    );
    let solar_tests = filter_tests(solar_tests, config);
    let solar_passed = solar_tests.iter().filter(|t| t.passed).count();
    let solar_failed = solar_tests.iter().filter(|t| !t.passed).count();

    let solar_run = CompilerRun {
        compiler: "solar".to_string(),
        compile_time: solar_test_time,
        tests: solar_tests,
        total_passed: solar_passed,
        total_failed: solar_failed,
        bytecode_sizes: solar_artifacts.bytecode_sizes,
        internal_only_library_stubs: solar_artifacts.internal_only_library_stubs,
    };

    // Step 2: Run tests with solc
    let (solc_test_time, solc_tests, solc_artifacts) =
        run_forge_test(project_dir, &format!("{}-solc", config.name), config, ForgeCompiler::Solc);
    let solc_tests = filter_tests(solc_tests, config);
    let solc_passed = solc_tests.iter().filter(|t| t.passed).count();
    let solc_failed = solc_tests.iter().filter(|t| !t.passed).count();

    let solc_run = CompilerRun {
        compiler: "solc".to_string(),
        compile_time: solc_test_time,
        tests: solc_tests,
        total_passed: solc_passed,
        total_failed: solc_failed,
        bytecode_sizes: solc_artifacts.bytecode_sizes,
        internal_only_library_stubs: solc_artifacts.internal_only_library_stubs,
    };

    // Print diff summary if there are regressions
    if solar_run.total_failed > 0 && solc_run.total_failed < solar_run.total_failed {
        print_test_diff(&solar_run.tests, &solc_run.tests, &config.name);
    }

    // Print comparison
    println!("\n{}", "=".repeat(70));
    println!(" {} ", config.name.to_uppercase());
    if config.test_filter.is_some() || config.contract_filter.is_some() {
        println!(" Filters: test={:?} contract={:?}", config.test_filter, config.contract_filter);
    }
    println!("{}", "=".repeat(70));

    // Test time
    println!("\n📦 Test Time:");
    let time_diff = if solc_run.compile_time.as_secs_f64() > 0.0 {
        ((solar_run.compile_time.as_secs_f64() / solc_run.compile_time.as_secs_f64()) - 1.0) * 100.0
    } else {
        0.0
    };
    println!(
        "   Solar: {:>6.2}s | solc: {:>6.2}s | {:+.0}%",
        solar_run.compile_time.as_secs_f64(),
        solc_run.compile_time.as_secs_f64(),
        time_diff
    );

    // Test results
    println!("\n✅ Test Results:");
    println!("   Solar: {} passed, {} failed", solar_run.total_passed, solar_run.total_failed);
    println!("   solc:  {} passed, {} failed", solc_run.total_passed, solc_run.total_failed);

    // Bytecode sizes
    println!("\n📏 Bytecode Sizes (deployed):");
    let mut all_contracts: Vec<_> =
        solar_run.bytecode_sizes.keys().chain(solc_run.bytecode_sizes.keys()).collect();
    all_contracts.sort();
    all_contracts.dedup();

    for contract in all_contracts {
        if contract.ends_with("Test") {
            continue;
        }
        let solar_size = solar_run.bytecode_sizes.get(contract).copied().unwrap_or(0);
        let solc_size = solc_run.bytecode_sizes.get(contract).copied().unwrap_or(0);
        if solar_size > 0 && solc_size > 0 {
            let savings = ((1.0 - (solar_size as f64 / solc_size as f64)) * 100.0) as i32;
            println!(
                "   {:20} Solar: {:>5}B | solc: {:>5}B | {:>+3}% smaller",
                contract, solar_size, solc_size, savings
            );
        } else if solar_size > 0 {
            println!("   {:20} Solar: {:>5}B | solc: N/A", contract, solar_size);
        }
    }

    // Gas comparison
    println!("\n⛽ Gas Usage (per test):");
    let solar_test_map: HashMap<_, _> = solar_run.tests.iter().map(|t| (&t.name, t)).collect();
    let solc_test_map: HashMap<_, _> = solc_run.tests.iter().map(|t| (&t.name, t)).collect();

    let mut test_names: Vec<_> = solar_test_map.keys().collect();
    test_names.sort();

    for name in test_names {
        if let (Some(solar_test), Some(solc_test)) =
            (solar_test_map.get(name), solc_test_map.get(name))
        {
            let status = if solar_test.passed && solc_test.passed {
                "✓"
            } else if solar_test.passed != solc_test.passed {
                "⚠"
            } else {
                "✗"
            };

            let gas_diff = if solc_test.gas > 0 {
                ((solar_test.gas as f64 / solc_test.gas as f64) - 1.0) * 100.0
            } else {
                0.0
            };

            let short_name: String = name.chars().take(35).collect();
            println!(
                "   {} {:35} Solar: {:>10} | solc: {:>10} | {:>+6.1}%",
                status, short_name, solar_test.gas, solc_test.gas, gas_diff
            );
        }
    }

    (solar_run, solc_run)
}

// ============================================================================
// Test Runner
// ============================================================================

/// Main test runner using config.
fn run_test_with_config(config: &TestConfig) {
    if !forge_available() {
        eprintln!("Skipping {}: forge not found in PATH", config.name);
        return;
    }

    let solar_binary = get_solar_binary();
    if !solar_binary.exists() {
        eprintln!("Skipping {}: Solar binary not found at {:?}", config.name, solar_binary);
        return;
    }

    let project_dir = &config.path;
    if !project_dir.exists() {
        panic!("Project directory not found: {:?}", project_dir);
    }

    for entry in &config.skip_tests {
        eprintln!(
            "[{}] skipping tests matching `{}`: {}",
            config.name, entry.pattern, entry.reason
        );
    }
    for entry in &config.skip_contracts {
        eprintln!(
            "[{}] skipping contracts matching `{}`: {}",
            config.name, entry.pattern, entry.reason
        );
    }

    if config.solar_only {
        run_test_solar_only(config);
    } else {
        run_test_with_comparison(config);
    }
}

/// Runs test with the compiler only (no solc comparison).
fn run_test_solar_only(config: &TestConfig) {
    let project_dir = &config.path;
    let (test_time, tests, artifacts) =
        run_forge_test(project_dir, &config.name, config, ForgeCompiler::Solar);
    let tests = filter_tests(tests, config);

    let total_passed = tests.iter().filter(|t| t.passed).count();
    let total_failed = tests.iter().filter(|t| !t.passed).count();

    let solar_run = CompilerRun {
        compiler: "solar".to_string(),
        compile_time: test_time,
        tests,
        total_passed,
        total_failed,
        bytecode_sizes: artifacts.bytecode_sizes,
        internal_only_library_stubs: artifacts.internal_only_library_stubs,
    };
    write_runtime_report(config, &solar_run, None);

    println!("\n✅ [{}] Solar-only: {} passed, {} failed", config.name, total_passed, total_failed);

    enforce_policy(config, &solar_run, None);
}

/// Runs test with a compiler-versus-solc comparison.
fn run_test_with_comparison(config: &TestConfig) {
    let (solar_run, solc_run) = run_project_comparison(config);
    write_runtime_report(config, &solar_run, Some(&solc_run));
    enforce_policy(config, &solar_run, Some(&solc_run));
}

/// Applies the config's assertion policy to the run results.
fn enforce_policy(config: &TestConfig, solar_run: &CompilerRun, solc_run: Option<&CompilerRun>) {
    match config.policy {
        AssertionPolicy::InRepoStrict => {
            assert_eq!(
                solar_run.total_failed, 0,
                "[{}] {} Solar tests failed",
                config.name, solar_run.total_failed
            );
            assert!(solar_run.total_passed > 0, "[{}] No Solar tests ran", config.name);

            let Some(solc_run) = solc_run else { return };
            if solc_run.total_passed > solar_run.total_passed {
                eprintln!(
                    "⚠️  [{}] solc passed {} more tests than Solar",
                    config.name,
                    solc_run.total_passed - solar_run.total_passed
                );
            }

            println!("\n✓ [{}] {} tests passed with Solar", config.name, solar_run.total_passed);
        }
        AssertionPolicy::ExternalDifferential => {
            let Some(solc_run) = solc_run else {
                panic!("[{}] external projects always run the solc leg", config.name)
            };
            enforce_external(config, solar_run, solc_run);
        }
    }
}

/// Judges an external project differentially: solc's passing tests are the
/// oracle and the compiler must uphold every one of them. An empty solc leg
/// means the baseline itself is broken (offline, incompatible forge), so the
/// project is skipped instead of failed.
fn enforce_external(config: &TestConfig, solar_run: &CompilerRun, solc_run: &CompilerRun) {
    let audit_errors = audit_artifacts(config, solar_run, solc_run);

    if config.build_only {
        if solc_run.bytecode_sizes.is_empty() {
            eprintln!("[{}] SKIPPED (cannot judge): solc leg produced no artifacts", config.name);
            return;
        }
        assert!(
            audit_errors.is_empty(),
            "[{}] artifact audit failed:\n  {}",
            config.name,
            audit_errors.join("\n  ")
        );
        println!(
            "\n✓ [{}] build-only: {} artifacts audited against solc",
            config.name,
            solc_run.bytecode_sizes.len()
        );
        return;
    }

    if solc_run.tests.is_empty() {
        eprintln!("[{}] SKIPPED (cannot judge): solc leg ran no tests", config.name);
        return;
    }

    let solar_by_key: HashMap<(&str, &str), &TestResult> = solar_run
        .tests
        .iter()
        .map(|test| ((test.contract.as_str(), test.name.as_str()), test))
        .collect();
    let mut regressions = Vec::new();
    for solc_test in &solc_run.tests {
        if !solc_test.passed {
            continue;
        }
        let key = (solc_test.contract.as_str(), solc_test.name.as_str());
        if solar_by_key.get(&key).is_some_and(|test| test.passed) {
            continue;
        }
        let state = if solar_by_key.contains_key(&key) { "fails" } else { "is missing" };
        regressions.push(format!(
            "{}::{} passes under solc but {state} under Solar",
            solc_test.contract, solc_test.name
        ));
    }

    assert!(
        regressions.is_empty(),
        "[{}] {} differential regressions:\n  {}",
        config.name,
        regressions.len(),
        regressions.join("\n  ")
    );
    assert!(
        audit_errors.is_empty(),
        "[{}] artifact audit failed:\n  {}",
        config.name,
        audit_errors.join("\n  ")
    );

    // Remaining compiler failures correspond to tests that also fail under solc
    // (or only exist under the compiler); they are upstream issues, not compiler bugs.
    if solar_run.total_failed > 0 {
        eprintln!(
            "ℹ️  [{}] {} tests fail under both compilers",
            config.name, solar_run.total_failed
        );
    }

    let oracle = solc_run.tests.iter().filter(|test| test.passed).count();
    println!("\n✓ [{}] differential: {} solc-passing tests upheld with Solar", config.name, oracle);
}

/// Audits the compiler's artifacts against solc's: every contract solc deploys
/// must have nonempty deployed bytecode from the compiler. Contracts matching
/// `skip_contracts` are exempt.
fn audit_artifacts(
    config: &TestConfig,
    solar_run: &CompilerRun,
    solc_run: &CompilerRun,
) -> Vec<String> {
    let skip_contracts = compile_skips(&config.skip_contracts);
    let mut names: Vec<_> = solc_run.bytecode_sizes.keys().collect();
    names.sort();

    let mut errors = Vec::new();
    for name in names {
        if skip_contracts.iter().any(|re| re.is_match(name)) {
            continue;
        }
        let solc_size = solc_run.bytecode_sizes[name];
        match solar_run.bytecode_sizes.get(name) {
            // Internal-only libraries get a call-protection stub from solc
            // and no compiler artifact; nothing can call either.
            None if solc_run.internal_only_library_stubs.contains(name) => {
                eprintln!(
                    "[audit] `{name}`: {solc_size}B solc stub with no Solar artifact (internal-only library)"
                );
            }
            None => errors.push(format!(
                "`{name}`: {solc_size}B deployed bytecode under solc, none under Solar"
            )),
            Some(_) => {}
        }
    }
    errors
}

/// Runs the default Foundry suite.
pub(super) fn run_default_suite(solar: &Path) {
    let _ = SOLAR_BINARY.set(solar.to_path_buf());
    let projects = discover_projects(&foundry_root());
    assert!(!projects.is_empty(), "No Foundry projects found");

    let mut failures = Vec::new();
    for config in projects {
        if config.ignored {
            eprintln!("[{}] temporarily ignored", config.name);
            continue;
        }
        let name = config.name.clone();
        if catch_unwind(AssertUnwindSafe(|| config.run())).is_err() {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "Foundry projects failed: {}", failures.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identifies_internal_only_library_stubs() {
        let push0 = json!({
            "abi": [
                {"type": "event", "name": "Used", "inputs": []},
                {"type": "error", "name": "Failed", "inputs": []},
            ],
            "methodIdentifiers": {},
        });
        let push0_bytecode = concat!(
            "0x7300000000000000000000000000000000000000003014",
            "60806040525f80fdfe",
            "a26469706673582212200033",
        );
        assert!(is_internal_only_library_stub(&push0, push0_bytecode));

        let push1_bytecode =
            concat!("7300000000000000000000000000000000000000003014", "6080604052600080fdfe",);
        assert!(is_internal_only_library_stub(&push0, push1_bytecode));

        let bare_revert = "600080fdfea164736f6c6343000813000a";
        assert!(is_internal_only_library_stub(&push0, bare_revert));

        let bare_push0_revert = "5f80fdfea164736f6c634300081a000a";
        assert!(is_internal_only_library_stub(&push0, bare_push0_revert));

        let separate_zeros = concat!(
            "7300000000000000000000000000000000000000003014",
            "60806040525f5ffdfe",
            "a26469706673582212200033",
        );
        assert!(is_internal_only_library_stub(&push0, separate_zeros));
    }

    #[test]
    fn rejects_callable_or_non_library_artifacts() {
        let callable = json!({
            "abi": [{"type": "function", "name": "f", "inputs": [], "outputs": []}],
            "methodIdentifiers": {"f()": "26121ff0"},
        });
        let stub = concat!("7300000000000000000000000000000000000000003014", "60806040525f80fdfe",);
        assert!(!is_internal_only_library_stub(&callable, stub));

        let empty_contract = json!({
            "abi": [],
            "methodIdentifiers": {},
        });
        assert!(!is_internal_only_library_stub(&empty_contract, "60806040525f80fdfea26469706673",));
    }

    #[test]
    fn foundry() {
        run_default_suite(&get_solar_binary());
    }

    #[test]
    #[ignore = "external Foundry suite; run via `cargo tq foundry-external`"]
    fn external() {
        external::run_external_suite(&get_solar_binary());
    }
}
