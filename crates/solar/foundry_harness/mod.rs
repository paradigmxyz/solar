//! Foundry integration test harness.
//!
//! This module tests Solar's codegen by:
//! 1. Running `forge test` with `FOUNDRY_SOLC=solar` (compiles everything with Solar)
//! 2. Running `forge test` with solc (baseline)
//! 3. Comparing gas usage and test results
//!
//! Run with: cargo test -p solar-compiler --test foundry
#![allow(clippy::uninlined_format_args, clippy::collapsible_if, clippy::disallowed_methods)]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for running a test project.
#[derive(Debug, Clone)]
struct TestConfig {
    /// Project name (used for display).
    name: String,
    /// Path to project relative to the workspace root.
    path: String,
    /// Optional filter for test function names (substring match).
    test_filter: Option<String>,
    /// Optional filter for contract names (substring match).
    contract_filter: Option<String>,
    /// If true, only run with Solar (no solc comparison).
    solar_only: bool,
}

impl TestConfig {
    /// Creates a new config with default settings.
    fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            test_filter: None,
            contract_filter: None,
            solar_only: false,
        }
    }

    /// Sets test function filter (substring match on test names).
    #[allow(dead_code)]
    fn test_filter(mut self, filter: impl Into<String>) -> Self {
        self.test_filter = Some(filter.into());
        self
    }

    /// Sets contract filter (substring match on contract names).
    #[allow(dead_code)]
    fn contract_filter(mut self, filter: impl Into<String>) -> Self {
        self.contract_filter = Some(filter.into());
        self
    }

    /// Sets whether to run Solar-only (no solc comparison).
    fn solar_only(mut self, value: bool) -> Self {
        self.solar_only = value;
        self
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
}

struct FoundrySolc {
    path: PathBuf,
    _temp_dir: tempfile::TempDir,
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

/// Gets the path to the Solar binary.
///
/// Prefers `CARGO_BIN_EXE_solar`, which Cargo sets to the `solar` binary it
/// built for this test run. That binary is always rebuilt from the current
/// sources before the test runs, so it cannot go stale — avoiding the race
/// where the harness picked up an out-of-date `target/release/solar` and
/// produced flaky failures. Falls back to a binary on disk when the variable
/// is absent (e.g. when the harness is reused outside `cargo test`).
fn get_solar_binary() -> PathBuf {
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

/// A `solc`-compatible executable for `FOUNDRY_SOLC` that enables Solar's
/// experimental code generator.
///
/// Code generation is gated behind `-Zcodegen`, but Forge invokes
/// `FOUNDRY_SOLC` with solc-style arguments and cannot pass that flag itself.
/// We therefore point it at a tiny wrapper script that forwards every call to
/// Solar with `-Zcodegen` prepended; otherwise Forge would receive empty
/// bytecode.
fn foundry_solc() -> FoundrySolc {
    let solar = get_solar_binary();
    let temp_dir = tempfile::Builder::new()
        .prefix("solar-zcodegen-")
        .tempdir()
        .expect("failed to create Solar codegen wrapper directory");

    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;

            let path = temp_dir.path().join("solar-zcodegen.sh");
            let script = format!(
                "#!/bin/sh\nexec \"{}\" -Zcodegen \"$@\"\n",
                solar.display()
            );
            fs::write(&path, script).expect("failed to write solar codegen wrapper");
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
            FoundrySolc { path, _temp_dir: temp_dir }
        } else if #[cfg(windows)] {
            let path = temp_dir.path().join("solar-zcodegen.cmd");
            let script = format!(
                "@echo off\r\n\"{}\" -Zcodegen %*\r\nexit /b %ERRORLEVEL%\r\n",
                solar.display()
            );
            fs::write(&path, script).expect("failed to write Solar codegen wrapper");
            FoundrySolc { path, _temp_dir: temp_dir }
        } else {
            FoundrySolc { path: solar, _temp_dir: temp_dir }
        }
    }
}

/// Checks if forge is available.
fn forge_available() -> bool {
    Command::new("forge").arg("--version").output().is_ok()
}

/// Filters tests based on config.
fn filter_tests(tests: Vec<TestResult>, config: &TestConfig) -> Vec<TestResult> {
    tests
        .into_iter()
        .filter(|t| {
            let test_match =
                config.test_filter.as_ref().map(|f| t.name.contains(f)).unwrap_or(true);
            let contract_match =
                config.contract_filter.as_ref().map(|f| t.contract.contains(f)).unwrap_or(true);
            test_match && contract_match
        })
        .collect()
}

// ============================================================================
// Parsing & Extraction
// ============================================================================

/// Parses test results from forge JSON output.
fn parse_test_results(stdout: &str) -> Vec<TestResult> {
    let mut tests = Vec::new();

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

/// Extracts bytecode sizes from forge output directory.
fn extract_bytecode_sizes(out_path: &Path) -> HashMap<String, usize> {
    let mut sizes = HashMap::new();

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
                                            sizes.insert(name, size);
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

    sizes
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
            "path": config.path.as_str(),
            "test_filter": config.test_filter.as_deref(),
            "contract_filter": config.contract_filter.as_deref(),
            "solar_only": config.solar_only,
        },
        "rerun": {
            "command": "cargo test -p solar-compiler --test foundry -- --test-threads=1",
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
) -> (Duration, Vec<TestResult>, HashMap<String, usize>) {
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
    cmd.current_dir(project_dir)
        .arg("test")
        .arg("--force")
        .arg("--json")
        .arg("-vvvvv")
        .arg("--decode-internal")
        .arg("--out")
        .arg(out_dir.path())
        .arg("--cache-path")
        .arg(cache_dir.path());

    if let Some(foundry_solc) = &foundry_solc {
        // Foundry expects solc-compatible `--version` output when probing `FOUNDRY_SOLC`.
        cmd.env("SOLC_WRAPPER", "1").env("FOUNDRY_SOLC", foundry_solc.path());
    }

    // Add forge match filters if specified
    if let Some(ref test_filter) = config.test_filter {
        cmd.arg("--match-test").arg(test_filter);
    }
    if let Some(ref contract_filter) = config.contract_filter {
        cmd.arg("--match-contract").arg(contract_filter);
    }

    let start = Instant::now();
    let command_failure = compiler.command_failure();
    let output = cmd.output().unwrap_or_else(|err| panic!("{command_failure}: {err}"));
    let test_time = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() || stdout.contains("\"status\":\"Failure\"") {
        eprintln!("\n[{}] forge test reported failures", label);
        if !output.stderr.is_empty() {
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    let tests = parse_test_results(&stdout);
    let sizes = extract_bytecode_sizes(out_dir.path());

    (test_time, tests, sizes)
}

// ============================================================================
// Comparison & Reporting
// ============================================================================

/// Compares Solar and solc test results and prints a diff summary.
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

/// Runs a full comparison between Solar and solc for a project.
fn run_project_comparison(config: &TestConfig) -> (CompilerRun, CompilerRun) {
    let project_dir = workspace_root().join(&config.path);

    // Step 1: Run tests with Solar
    let (solar_test_time, solar_tests, solar_sizes) = run_forge_test(
        &project_dir,
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
        bytecode_sizes: solar_sizes,
    };

    // Step 2: Run tests with solc
    let (solc_test_time, solc_tests, solc_sizes) =
        run_forge_test(&project_dir, &format!("{}-solc", config.name), config, ForgeCompiler::Solc);
    let solc_tests = filter_tests(solc_tests, config);
    let solc_passed = solc_tests.iter().filter(|t| t.passed).count();
    let solc_failed = solc_tests.iter().filter(|t| !t.passed).count();

    let solc_run = CompilerRun {
        compiler: "solc".to_string(),
        compile_time: solc_test_time,
        tests: solc_tests,
        total_passed: solc_passed,
        total_failed: solc_failed,
        bytecode_sizes: solc_sizes,
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

    let project_dir = workspace_root().join(&config.path);
    if !project_dir.exists() {
        panic!("Project directory not found: {:?}", project_dir);
    }

    if config.solar_only {
        run_test_solar_only(config);
    } else {
        run_test_with_comparison(config);
    }
}

/// Runs test with Solar only (no solc comparison).
fn run_test_solar_only(config: &TestConfig) {
    let project_dir = workspace_root().join(&config.path);
    let (test_time, tests, bytecode_sizes) =
        run_forge_test(&project_dir, &config.name, config, ForgeCompiler::Solar);
    let tests = filter_tests(tests, config);

    let total_passed = tests.iter().filter(|t| t.passed).count();
    let total_failed = tests.iter().filter(|t| !t.passed).count();

    let solar_run = CompilerRun {
        compiler: "solar".to_string(),
        compile_time: test_time,
        tests,
        total_passed,
        total_failed,
        bytecode_sizes,
    };
    write_runtime_report(config, &solar_run, None);

    println!("\n✅ [{}] Solar-only: {} passed, {} failed", config.name, total_passed, total_failed);

    assert_eq!(total_failed, 0, "[{}] {} Solar tests failed", config.name, total_failed);
    assert!(total_passed > 0, "[{}] No Solar tests ran", config.name);
}

/// Runs test with Solar vs solc comparison.
fn run_test_with_comparison(config: &TestConfig) {
    let (solar_run, solc_run) = run_project_comparison(config);
    write_runtime_report(config, &solar_run, Some(&solc_run));

    // Assert Solar tests pass
    assert_eq!(
        solar_run.total_failed, 0,
        "[{}] {} Solar tests failed",
        config.name, solar_run.total_failed
    );
    assert!(solar_run.total_passed > 0, "[{}] No Solar tests ran", config.name);

    if solc_run.total_passed > solar_run.total_passed {
        eprintln!(
            "⚠️  [{}] solc passed {} more tests than Solar",
            config.name,
            solc_run.total_passed - solar_run.total_passed
        );
    }

    println!("\n✓ [{}] {} tests passed with Solar", config.name, solar_run.total_passed);
}

// ============================================================================
// Legacy API (for backward compatibility)
// ============================================================================

/// Tests a project with Solar vs solc comparison (legacy API).
fn test_project_solar(project_name: &str, project_path: &str) {
    TestConfig::new(project_name, project_path).run();
}

/// Tests a project where solc can't compile (legacy API).
fn test_project_solar_only(project_name: &str, project_path: &str) {
    TestConfig::new(project_name, project_path).solar_only(true).run();
}

/// Runs the default Foundry suite.
///
/// This is used by `crates/solar/tests.rs` when invoked with
/// `TESTER_MODE=foundry`, so Foundry can be selected like the other compiler
/// test modes. The dedicated `foundry` integration-test target still exists to
/// keep per-project Rust test discovery in nextest.
#[allow(dead_code)]
pub(crate) fn run_default_suite() {
    test_project_solar("arithmetic", "tests/foundry/arithmetic");
    test_project_solar("control_flow", "tests/foundry/control-flow");
    test_project_solar("storage", "tests/foundry/storage");
    test_project_solar("events", "tests/foundry/events");
    test_project_solar("calls", "tests/foundry/calls");
    test_project_solar("interfaces", "tests/foundry/interfaces");
    test_project_solar("libraries", "tests/foundry/libraries");
    test_project_solar("constructor_args", "tests/foundry/constructor-args");
    test_project_solar("multi_return", "tests/foundry/multi-return");
    test_project_solar("correctness", "tests/foundry/correctness");
    test_project_solar("inheritance", "tests/foundry/inheritance");
    test_project_solar_only("stack_deep", "tests/foundry/stack-deep");
    run_compilation_smoke();
}

fn run_compilation_smoke() {
    if !forge_available() {
        eprintln!("Skipping: forge not found");
        return;
    }

    let solar_binary = get_solar_binary();
    if !solar_binary.exists() {
        eprintln!("Skipping: Solar binary not found");
        return;
    }

    let config = TestConfig::new("compilation-test", "tests/foundry/arithmetic");
    let project_dir = workspace_root().join(&config.path);
    let (test_time, tests, sizes) =
        run_forge_test(&project_dir, "compilation-test", &config, ForgeCompiler::Solar);

    println!("Test time: {:?}", test_time);
    println!("Tests: {:?}", tests.iter().map(|t| &t.name).collect::<Vec<_>>());
    println!("Bytecode sizes: {:?}", sizes);

    assert!(!tests.is_empty(), "No tests ran");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #[test]
    fn test_arithmetic() {
        super::test_project_solar("arithmetic", "tests/foundry/arithmetic");
    }

    #[test]
    fn test_control_flow() {
        super::test_project_solar("control_flow", "tests/foundry/control-flow");
    }

    #[test]
    fn test_storage() {
        super::test_project_solar("storage", "tests/foundry/storage");
    }

    #[test]
    fn test_events() {
        super::test_project_solar("events", "tests/foundry/events");
    }

    #[test]
    fn test_calls() {
        super::test_project_solar("calls", "tests/foundry/calls");
    }

    #[test]
    fn test_interfaces() {
        super::test_project_solar("interfaces", "tests/foundry/interfaces");
    }

    #[test]
    fn test_libraries() {
        super::test_project_solar("libraries", "tests/foundry/libraries");
    }

    #[test]
    fn test_constructor_args() {
        super::test_project_solar("constructor_args", "tests/foundry/constructor-args");
    }

    #[test]
    fn test_multi_return() {
        super::test_project_solar("multi_return", "tests/foundry/multi-return");
    }

    #[test]
    fn test_correctness() {
        super::test_project_solar("correctness", "tests/foundry/correctness");
    }

    #[test]
    fn test_inheritance() {
        super::test_project_solar("inheritance", "tests/foundry/inheritance");
    }

    #[test]
    fn test_stack_deep() {
        super::test_project_solar_only("stack_deep", "tests/foundry/stack-deep");
    }

    #[test]
    fn test_compilation() {
        super::run_compilation_smoke();
    }

    #[test]
    #[ignore] // Requires forge-std which is not available in CI
    fn test_unifap_v2() {
        super::test_project_solar("unifap-v2", "tests/foundry/unifap-v2");
    }

    #[test]
    #[ignore] // Requires forge-std which is not available in CI
    fn test_unifap_v2_create() {
        super::test_project_solar("unifap-v2-create", "tests/foundry/unifap-v2-create");
    }

    // Example: run only mint-related tests
    #[test]
    #[ignore] // Example - enable when debugging specific tests
    fn test_unifap_mint_only() {
        super::TestConfig::new("unifap-v2-create", "tests/foundry/unifap-v2-create")
            .test_filter("testMint")
            .run();
    }

    // Example: run only tests in a specific contract
    #[test]
    #[ignore] // Example - enable when debugging specific contracts
    fn test_unifap_pair_only() {
        super::TestConfig::new("unifap-v2-create", "tests/foundry/unifap-v2-create")
            .contract_filter("UnifapV2Pair")
            .run();
    }

    // Example: combine test + contract filters
    #[test]
    #[ignore] // Example - enable when debugging
    fn test_unifap_pair_swap() {
        super::TestConfig::new("unifap-v2-create", "tests/foundry/unifap-v2-create")
            .contract_filter("UnifapV2Pair")
            .test_filter("testSwap")
            .run();
    }

    // ========== Struct Tests ==========

    #[test]
    #[ignore] // WIP: 8 struct tests have StackUnderflow issues to fix
    fn test_structs() {
        super::test_project_solar("structs", "tests/foundry/structs");
    }
}
