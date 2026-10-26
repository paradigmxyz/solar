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

pub use external::run_external_suite;

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
    /// The compiler leg runs once per entry, all against the one solc baseline. `None` follows
    /// the project's own optimizer settings.
    solar_levels: Vec<Option<OptLevel>>,
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
    /// Foundry profiles each compiler leg builds with `forge build` before running tests, for
    /// test suites that deploy artifacts from those profiles' output directories.
    prebuild_profiles: Vec<String>,
    /// Pass `-vvvvv --decode-internal` to `forge test`.
    traces: bool,
    /// Command shown in runtime reports for reproducing this run.
    rerun_command: String,
}

impl TestConfig {
    /// Creates a config for an in-repo project under `tests/foundry`.
    fn in_repo(name: String, path: PathBuf, solar_only: bool) -> Self {
        Self {
            name,
            path,
            test_filter: None,
            contract_filter: None,
            solar_only,
            solar_levels: OptLevel::pinned()
                .map_or_else(|| OptLevel::ALL.map(Some).to_vec(), |level| vec![Some(level)]),
            skip_tests: Vec::new(),
            skip_contracts: Vec::new(),
            policy: AssertionPolicy::InRepoStrict,
            build_only: false,
            fuzz_seed: None,
            solc_wrapper_version: None,
            foundry_profile: None,
            prebuild_profiles: Vec::new(),
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
    /// Keeps the wrapper script alive.
    _dir: Option<tempfile::TempDir>,
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

/// An optimization level of the compiler leg, set through Foundry's optimizer settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptLevel {
    None,
    Gas,
    Size,
}

impl OptLevel {
    const ALL: [Self; 3] = [Self::None, Self::Gas, Self::Size];

    /// The level `SOLAR_FOUNDRY_OPTIMIZATION` pins, if set.
    fn pinned() -> Option<Self> {
        let mode = std::env::var("SOLAR_FOUNDRY_OPTIMIZATION").ok()?;
        let level = Self::ALL.into_iter().find(|level| level.name() == mode);
        Some(level.unwrap_or_else(|| {
            panic!("invalid SOLAR_FOUNDRY_OPTIMIZATION `{mode}`; expected none, gas, or size")
        }))
    }

    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gas => "gas",
            Self::Size => "size",
        }
    }

    /// The `(optimizer, optimizer_runs)` Foundry settings that select this level.
    fn foundry_settings(self) -> (bool, u32) {
        match self {
            Self::None => (false, 200),
            Self::Gas => (true, 200),
            Self::Size => (true, 1),
        }
    }
}

/// Names a compiler leg in output: the project, plus `-O <level>` for a pinned level. `None`
/// follows the project's own optimizer settings.
fn leg_label(name: &str, level: Option<OptLevel>) -> String {
    match level {
        Some(level) => format!("{name} -O {}", level.name()),
        None => name.to_string(),
    }
}

// ============================================================================
// Helpers
// ============================================================================

static SOLAR_BINARY: OnceLock<PathBuf> = OnceLock::new();

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

            Some(TestConfig::in_repo(name, path, solar_only))
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

/// Gets the path to the compiler binary under test, set by the running suite.
fn get_solar_binary() -> PathBuf {
    SOLAR_BINARY.get().expect("the suite sets the compiler binary before running").clone()
}

/// Picks the compiler under test: `SOLAR_FOUNDRY_COMPILER` if set, else the binary Cargo built.
fn select_solar_binary(built: &Path) -> PathBuf {
    let Some(path) = std::env::var_os("SOLAR_FOUNDRY_COMPILER") else {
        return built.to_path_buf();
    };
    let path = PathBuf::from(path);
    let path = if path.is_absolute() { path } else { workspace_root().join(path) };
    assert!(path.is_file(), "SOLAR_FOUNDRY_COMPILER is not a file: {}", path.display());
    path
}

/// Gets the path to the workspace root.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// A `solc`-compatible executable for `FOUNDRY_SOLC`.
///
/// Forge passes no compiler flags, so on Unix a wrapper script adds `-Zvalidate-ir=true` and keeps
/// full MIR validation on release binaries.
fn foundry_solc() -> FoundrySolc {
    let binary = get_solar_binary();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::Builder::new()
            .prefix("solar-foundry-solc-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("failed to create Foundry compiler directory");
        let path = dir.path().join("solc");
        let script = format!(
            "#!/bin/sh\nexec '{}' -Zvalidate-ir=true \"$@\"\n",
            binary.display().to_string().replace('\'', "'\\''")
        );
        fs::write(&path, script).expect("failed to write Foundry compiler wrapper");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("failed to make Foundry compiler wrapper executable");
        FoundrySolc { path, _dir: Some(dir) }
    }
    #[cfg(not(unix))]
    FoundrySolc { path: binary, _dir: None }
}

/// Checks if forge is available.
fn forge_available() -> bool {
    Command::new("forge").arg("--version").output().is_ok()
}

/// Returns whether this forge accepts `--no-lint`, which older releases reject.
fn forge_supports_no_lint() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        Command::new("forge")
            .args(["build", "--help"])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("--no-lint"))
    })
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

/// Writes the project's JSON report: one `solar` entry per optimization level, keyed by level
/// name (`project` for the project's own optimizer settings).
fn write_runtime_report(
    config: &TestConfig,
    solar_runs: &[(Option<OptLevel>, CompilerRun)],
    solc_run: Option<&CompilerRun>,
) {
    let Some(report_dir) = std::env::var_os("SOLAR_FOUNDRY_REPORT_DIR") else {
        return;
    };

    let report_dir = PathBuf::from(report_dir);
    let report_dir =
        if report_dir.is_absolute() { report_dir } else { workspace_root().join(report_dir) };
    fs::create_dir_all(&report_dir).expect("failed to create Foundry report directory");

    let mut rerun_env = serde_json::Map::new();
    rerun_env.insert("SOLAR_FOUNDRY_REPORT_DIR".into(), report_dir.display().to_string().into());
    for name in [
        "PATH",
        "SOLAR_FOUNDRY_COMPILER",
        "SOLAR_FOUNDRY_OPTIMIZATION",
        "FOUNDRY_OPTIMIZER",
        "FOUNDRY_OPTIMIZER_RUNS",
    ] {
        if let Ok(value) = std::env::var(name) {
            rerun_env.insert(name.into(), value.into());
        }
    }
    let forge_version = Command::new("forge")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let report = serde_json::json!({
        "forge_version": forge_version,
        "project": {
            "name": config.name.as_str(),
            "path": config.path.display().to_string(),
            "test_filter": config.test_filter.as_deref(),
            "contract_filter": config.contract_filter.as_deref(),
            "solar_only": config.solar_only,
        },
        "rerun": {
            "command": config.rerun_command.as_str(),
            "env": rerun_env,
        },
        "solar": solar_runs
            .iter()
            .map(|(level, run)| {
                (level.map_or("project", OptLevel::name).to_string(), compiler_run_json(run))
            })
            .collect::<serde_json::Map<_, _>>(),
        "solc": solc_run.map(compiler_run_json),
    });

    let path = report_dir.join(report_file_name(&config.name));
    let json = serde_json::to_string_pretty(&report).expect("failed to serialize Foundry report");
    fs::write(&path, json).expect("failed to write Foundry report");
}

// ============================================================================
// Forge Execution
// ============================================================================

/// Runs forge test for a compiler; `level` applies to the Solar leg only.
fn run_forge_test(
    project_dir: &Path,
    label: &str,
    config: &TestConfig,
    compiler: ForgeCompiler,
    level: Option<OptLevel>,
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
        cmd.arg("build").arg("--force");
        if forge_supports_no_lint() {
            cmd.arg("--no-lint");
        }
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
    set_forge_compiler(&mut cmd, config, foundry_solc.as_ref(), level);

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
    if !prebuild_profiles(project_dir, label, config, compiler, foundry_solc.as_ref(), level) {
        // A profile that failed to build may have left another leg's artifacts for the tests.
        eprintln!("\n[{label}] running no tests: a prebuilt profile failed to build");
        return (start.elapsed(), Vec::new(), ArtifactData::default());
    }
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

/// Points forge at a compiler leg's compiler and optimization level; the solc leg keeps forge's
/// own solc and the project's optimizer settings.
fn set_forge_compiler(
    cmd: &mut Command,
    config: &TestConfig,
    foundry_solc: Option<&FoundrySolc>,
    level: Option<OptLevel>,
) {
    if let Some(foundry_solc) = foundry_solc {
        // Foundry expects solc-compatible `--version` output when probing `FOUNDRY_SOLC`.
        cmd.env("SOLC_WRAPPER", "1").env("FOUNDRY_SOLC", foundry_solc.path());
        if let Some(version) = &config.solc_wrapper_version {
            cmd.env("SOLC_WRAPPER_VERSION", version);
        }
        if let Some(level) = level {
            let (enabled, runs) = level.foundry_settings();
            cmd.env("FOUNDRY_OPTIMIZER", enabled.to_string())
                .env("FOUNDRY_OPTIMIZER_RUNS", runs.to_string());
        }
    }
}

/// Builds the profiles whose artifacts the tests deploy with the leg's compiler, and returns
/// whether every one built.
///
/// Each profile writes its own output directory inside the project, where the tests read it, so
/// every leg rebuilds them before its `forge test`. A profile that fails to build can leave an
/// earlier leg's artifacts in that directory, so the leg then runs no tests: the compiler leg
/// misses every test solc passes, and a solc leg without tests cannot judge the project.
fn prebuild_profiles(
    project_dir: &Path,
    label: &str,
    config: &TestConfig,
    compiler: ForgeCompiler,
    foundry_solc: Option<&FoundrySolc>,
    level: Option<OptLevel>,
) -> bool {
    for profile in &config.prebuild_profiles {
        let cache_dir = tempfile::Builder::new()
            .prefix(compiler.cache_prefix())
            .tempdir()
            .expect("failed to create Foundry cache directory");
        let mut cmd = Command::new("forge");
        cmd.current_dir(project_dir).arg("build").arg("--force");
        if forge_supports_no_lint() {
            cmd.arg("--no-lint");
        }
        cmd.arg("--cache-path").arg(cache_dir.path()).env("FOUNDRY_PROFILE", profile);
        set_forge_compiler(&mut cmd, config, foundry_solc, level);
        let output = cmd.output().unwrap_or_else(|err| {
            panic!("failed to run forge build for profile `{profile}`: {err}")
        });
        if !output.status.success() {
            eprintln!("\n[{label}] forge build of profile `{profile}` failed");
            if !output.stderr.is_empty() {
                eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
            return false;
        }
    }
    true
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

/// Runs one compiler leg of a project and summarizes it; `level` applies to the Solar leg only.
fn run_leg(config: &TestConfig, compiler: ForgeCompiler, level: Option<OptLevel>) -> CompilerRun {
    let (name, label) = match compiler {
        ForgeCompiler::Solar => ("solar", leg_label(&format!("{}-solar", config.name), level)),
        ForgeCompiler::Solc => ("solc", format!("{}-solc", config.name)),
    };
    let (compile_time, tests, artifacts) =
        run_forge_test(&config.path, &label, config, compiler, level);
    let tests = filter_tests(tests, config);
    let total_passed = tests.iter().filter(|t| t.passed).count();
    let total_failed = tests.iter().filter(|t| !t.passed).count();

    CompilerRun {
        compiler: name.to_string(),
        compile_time,
        tests,
        total_passed,
        total_failed,
        bytecode_sizes: artifacts.bytecode_sizes,
        internal_only_library_stubs: artifacts.internal_only_library_stubs,
    }
}

/// Runs the compiler leg at each of the project's optimization levels.
fn run_solar_legs(config: &TestConfig) -> Vec<(Option<OptLevel>, CompilerRun)> {
    config
        .solar_levels
        .iter()
        .map(|&level| (level, run_leg(config, ForgeCompiler::Solar, level)))
        .collect()
}

/// Prints the comparison between one compiler leg and the solc baseline.
fn print_comparison(
    config: &TestConfig,
    label: &str,
    solar_run: &CompilerRun,
    solc_run: &CompilerRun,
) {
    // Print diff summary if there are regressions
    if solar_run.total_failed > 0 && solc_run.total_failed < solar_run.total_failed {
        print_test_diff(&solar_run.tests, &solc_run.tests, label);
    }

    // Print comparison
    println!("\n{}", "=".repeat(70));
    println!(" {} ", label.to_uppercase());
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
}

// ============================================================================
// Test Runner
// ============================================================================

/// Main test runner using config.
fn run_test_with_config(config: &TestConfig) {
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
    let solar_runs = run_solar_legs(config);
    write_runtime_report(config, &solar_runs, None);

    for (level, run) in &solar_runs {
        println!(
            "\n✅ [{}] Solar-only: {} passed, {} failed",
            leg_label(&config.name, *level),
            run.total_passed,
            run.total_failed
        );
    }

    enforce_levels(config, &solar_runs, None);
}

/// Runs the solc baseline once, then the compiler once per optimization level against it.
fn run_test_with_comparison(config: &TestConfig) {
    let solc_run = run_leg(config, ForgeCompiler::Solc, None);
    let solar_runs = run_solar_legs(config);
    for (level, solar_run) in &solar_runs {
        print_comparison(config, &leg_label(&config.name, *level), solar_run, &solc_run);
    }
    write_runtime_report(config, &solar_runs, Some(&solc_run));
    enforce_levels(config, &solar_runs, Some(&solc_run));
}

/// Applies the assertion policy to every level, so a failing level does not hide the others.
fn enforce_levels(
    config: &TestConfig,
    solar_runs: &[(Option<OptLevel>, CompilerRun)],
    solc_run: Option<&CompilerRun>,
) {
    let failed = solar_runs
        .iter()
        .filter_map(|(level, solar_run)| {
            let label = leg_label(&config.name, *level);
            catch_unwind(AssertUnwindSafe(|| enforce_policy(config, &label, solar_run, solc_run)))
                .is_err()
                .then_some(label)
        })
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "Foundry legs failed: {}", failed.join(", "));
}

/// Applies the config's assertion policy to the run results.
fn enforce_policy(
    config: &TestConfig,
    label: &str,
    solar_run: &CompilerRun,
    solc_run: Option<&CompilerRun>,
) {
    match config.policy {
        AssertionPolicy::InRepoStrict => {
            assert_eq!(
                solar_run.total_failed, 0,
                "[{label}] {} Solar tests failed",
                solar_run.total_failed
            );
            assert!(solar_run.total_passed > 0, "[{label}] No Solar tests ran");

            let Some(solc_run) = solc_run else { return };
            if solc_run.total_passed > solar_run.total_passed {
                eprintln!(
                    "⚠️  [{label}] solc passed {} more tests than Solar",
                    solc_run.total_passed - solar_run.total_passed
                );
            }

            println!("\n✓ [{label}] {} tests passed with Solar", solar_run.total_passed);
        }
        AssertionPolicy::ExternalDifferential => {
            let Some(solc_run) = solc_run else {
                panic!("[{label}] external projects always run the solc leg")
            };
            enforce_external(config, label, solar_run, solc_run);
        }
    }
}

/// Judges an external project differentially: solc's passing tests are the
/// oracle and the compiler must uphold every one of them. An empty solc leg
/// means the baseline itself is broken (offline, incompatible forge), so the
/// project is skipped instead of failed.
fn enforce_external(
    config: &TestConfig,
    label: &str,
    solar_run: &CompilerRun,
    solc_run: &CompilerRun,
) {
    let audit_errors = audit_artifacts(config, solar_run, solc_run);

    if config.build_only {
        if solc_run.bytecode_sizes.is_empty() {
            eprintln!("[{label}] SKIPPED (cannot judge): solc leg produced no artifacts");
            return;
        }
        assert!(
            audit_errors.is_empty(),
            "[{label}] artifact audit failed:\n  {}",
            audit_errors.join("\n  ")
        );
        println!(
            "\n✓ [{label}] build-only: {} artifacts audited against solc",
            solc_run.bytecode_sizes.len()
        );
        return;
    }

    if solc_run.tests.is_empty() {
        eprintln!("[{label}] SKIPPED (cannot judge): solc leg ran no tests");
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
        "[{label}] {} differential regressions:\n  {}",
        regressions.len(),
        regressions.join("\n  ")
    );
    assert!(
        audit_errors.is_empty(),
        "[{label}] artifact audit failed:\n  {}",
        audit_errors.join("\n  ")
    );

    // Remaining compiler failures correspond to tests that also fail under solc
    // (or only exist under the compiler); they are upstream issues, not compiler bugs.
    if solar_run.total_failed > 0 {
        eprintln!("ℹ️  [{label}] {} tests fail under both compilers", solar_run.total_failed);
    }

    let oracle = solc_run.tests.iter().filter(|test| test.passed).count();
    println!("\n✓ [{label}] differential: {oracle} solc-passing tests upheld with Solar");
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

/// Runs the default Foundry suite with the `solar` binary Cargo built for the test.
pub fn run_default_suite(solar: &Path) {
    if std::env::var_os("SOLAR_FOUNDRY_SKIP").is_some_and(|v| v != "0") {
        eprintln!("Skipping the Foundry suite: SOLAR_FOUNDRY_SKIP is set");
        return;
    }
    assert!(
        forge_available(),
        "forge not found in PATH; install Foundry, or set SOLAR_FOUNDRY_SKIP=1 to skip the \
         Foundry suite"
    );

    let _ = SOLAR_BINARY.set(select_solar_binary(solar));
    let projects = discover_projects(&foundry_root());
    assert!(!projects.is_empty(), "No Foundry projects found");

    let mut failures = Vec::new();
    for config in projects {
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
}
