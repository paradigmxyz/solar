//! Foundry integration test harness.
//!
//! This module tests Solar's codegen by:
//! 1. Compiling src/*.sol with Solar to get bytecode
//! 2. Compiling test/*.t.sol with solc (test harness must work correctly)
//! 3. Running tests with Solar bytecode injected via env vars
//!
//! Run with: cargo test -p solar-codegen --test foundry
#![allow(clippy::uninlined_format_args, clippy::collapsible_if, clippy::disallowed_methods)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime},
};

/// Gets the path to the Solar binary (debug build).
fn get_solar_binary() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/solar")
}

/// Gets the path to the codegen crate.
fn get_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Checks if forge is available.
fn forge_available() -> bool {
    Command::new("forge").arg("--version").output().is_ok()
}

/// Result of a single test.
#[derive(Debug, Clone)]
struct TestResult {
    name: String,
    passed: bool,
    gas: u64,
}

/// Generates a timestamp string for file naming.
fn timestamp_suffix() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Saves JSON output to a file for debugging.
fn save_json_output(project_dir: &Path, filename: &str, content: &str) -> PathBuf {
    let timestamp = timestamp_suffix();
    let json_path = project_dir.join(format!("{}-{}.json", filename, timestamp));
    if let Err(e) = std::fs::write(&json_path, content) {
        eprintln!("⚠️  Failed to save JSON to {:?}: {}", json_path, e);
    }
    json_path
}

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

/// Bytecode extracted from Solar compilation.
#[derive(Debug, Clone)]
struct ContractBytecode {
    name: String,
    creation_code: String, // hex without 0x prefix
    deployed_code: String, // hex without 0x prefix
}

/// Compiles everything with Solar and extracts bytecode for src contracts only.
fn compile_with_solar(project_dir: &PathBuf) -> (Duration, Vec<ContractBytecode>) {
    let out_dir = "out-solar";
    let cache_dir = "cache-solar";

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("build")
        .arg("--force")
        .arg("--out")
        .arg(out_dir)
        .arg("--cache-path")
        .arg(cache_dir)
        .env("FOUNDRY_SOLC", get_solar_binary());

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge build for Solar");
    let compile_time = start.elapsed();

    if !output.status.success() {
        eprintln!("[solar] Build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Extract bytecode from src/*.sol artifacts only (skip test contracts)
    let mut bytecodes = Vec::new();
    let out_path = project_dir.join(out_dir);
    if let Ok(entries) = std::fs::read_dir(&out_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only process directories that don't end with .t.sol (test files)
            if path.is_dir() {
                let dir_name = path.file_name().unwrap().to_string_lossy();
                if dir_name.ends_with(".t.sol") {
                    continue; // Skip test contract artifacts
                }

                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let file_path = file.path();
                        if file_path.extension().is_some_and(|e| e == "json") {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&content)
                                {
                                    let creation_code = json
                                        .get("bytecode")
                                        .and_then(|b| b.get("object"))
                                        .and_then(|o| o.as_str())
                                        .unwrap_or("")
                                        .strip_prefix("0x")
                                        .unwrap_or("")
                                        .to_string();

                                    let deployed_code = json
                                        .get("deployedBytecode")
                                        .and_then(|b| b.get("object"))
                                        .and_then(|o| o.as_str())
                                        .unwrap_or("")
                                        .strip_prefix("0x")
                                        .unwrap_or("")
                                        .to_string();

                                    if !creation_code.is_empty() {
                                        let name = file_path
                                            .file_stem()
                                            .unwrap()
                                            .to_string_lossy()
                                            .to_string();
                                        bytecodes.push(ContractBytecode {
                                            name,
                                            creation_code,
                                            deployed_code,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (compile_time, bytecodes)
}

/// Runs forge build with solc for entire project (tests always use solc).
fn run_forge_build_solc(project_dir: &PathBuf) -> (Duration, HashMap<String, usize>) {
    let out_dir = "out-solc";
    let cache_dir = "cache-solc";

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("build")
        .arg("--force")
        .arg("--out")
        .arg(out_dir)
        .arg("--cache-path")
        .arg(cache_dir);

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge build");
    let compile_time = start.elapsed();

    if !output.status.success() {
        eprintln!("[solc] Build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Parse bytecode sizes from artifacts
    let mut sizes = HashMap::new();
    let out_path = project_dir.join(out_dir);
    if let Ok(entries) = std::fs::read_dir(&out_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
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

    (compile_time, sizes)
}

/// Runs forge test with Solar bytecode injected via env vars.
/// Tests are always compiled with solc, only src contracts use Solar bytecode.
fn run_forge_test_with_solar_bytecode(
    project_dir: &PathBuf,
    solar_bytecodes: &[ContractBytecode],
    label: &str,
) -> (Duration, Vec<TestResult>) {
    let out_dir = "out-solc"; // Always use solc-compiled tests
    let cache_dir = "cache-solc";

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("test")
        .arg("--json")
        .arg("-vvvvv")
        .arg("--decode-internal")
        .arg("--out")
        .arg(out_dir)
        .arg("--cache-path")
        .arg(cache_dir);

    // Inject Solar bytecode via env vars
    for bc in solar_bytecodes {
        let env_name = format!("SOLAR_{}_BYTECODE", bc.name.to_uppercase());
        cmd.env(&env_name, format!("0x{}", &bc.creation_code));
    }

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge test");
    let test_time = start.elapsed();

    let mut tests = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Always save JSON output for debugging
    let json_path = save_json_output(project_dir, "solar-test-output", &stdout);

    // Print info when there are failures
    if !output.status.success() || stdout.contains("\"status\":\"Failure\"") {
        eprintln!("\n🔍 [{}] Full JSON saved to: {:?}", label, json_path);
        eprintln!("[{}] Full forge test JSON output:", label);
        eprintln!("{}", stdout);
        if !output.stderr.is_empty() {
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(obj) = json.as_object() {
            for (_contract, contract_data) in obj {
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
                            tests.push(TestResult { name: name.clone(), passed, gas });
                        }
                    }
                }
            }
        }
    }

    (test_time, tests)
}

/// Runs forge test with pure solc (baseline).
fn run_forge_test_solc(project_dir: &PathBuf) -> (Duration, Vec<TestResult>) {
    let out_dir = "out-solc";
    let cache_dir = "cache-solc";

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("test")
        .arg("--json")
        .arg("-vvvvv")
        .arg("--decode-internal")
        .arg("--out")
        .arg(out_dir)
        .arg("--cache-path")
        .arg(cache_dir);

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge test");
    let test_time = start.elapsed();

    let mut tests = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Save JSON output for debugging (always, for later comparison)
    let _json_path = save_json_output(project_dir, "solc-test-output", &stdout);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(obj) = json.as_object() {
            for (_contract, contract_data) in obj {
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
                            tests.push(TestResult { name: name.clone(), passed, gas });
                        }
                    }
                }
            }
        }
    }

    (test_time, tests)
}

/// Runs a full comparison between Solar and solc for a project.
fn run_project_comparison(project_name: &str, project_path: &str) -> (CompilerRun, CompilerRun) {
    let project_dir = get_crate_dir().join(project_path);

    // Step 1: Compile everything with Solar, extract src contract bytecodes
    let (solar_compile_time, solar_bytecodes) = compile_with_solar(&project_dir);

    // Print extracted bytecodes
    eprintln!("\n📦 [{}] Solar bytecodes extracted:", project_name);
    for bc in &solar_bytecodes {
        eprintln!(
            "   {} - creation: {}B, deployed: {}B",
            bc.name,
            bc.creation_code.len() / 2,
            bc.deployed_code.len() / 2
        );
    }

    // Step 2: Build entire project with solc (tests need solc)
    let (solc_compile_time, solc_sizes) = run_forge_build_solc(&project_dir);

    // Step 3: Run tests with Solar bytecode injected
    let (solar_test_time, solar_tests) = run_forge_test_with_solar_bytecode(
        &project_dir,
        &solar_bytecodes,
        &format!("{}-solar", project_name),
    );
    let solar_passed = solar_tests.iter().filter(|t| t.passed).count();
    let solar_failed = solar_tests.iter().filter(|t| !t.passed).count();

    // Calculate Solar bytecode sizes
    let mut solar_sizes = HashMap::new();
    for bc in &solar_bytecodes {
        solar_sizes.insert(bc.name.clone(), bc.deployed_code.len() / 2);
    }

    let solar_run = CompilerRun {
        compiler: "solar".to_string(),
        compile_time: solar_compile_time + solar_test_time,
        tests: solar_tests,
        total_passed: solar_passed,
        total_failed: solar_failed,
        bytecode_sizes: solar_sizes,
    };

    // Step 4: Run tests with pure solc (baseline)
    let (solc_test_time, solc_tests) = run_forge_test_solc(&project_dir);
    let solc_passed = solc_tests.iter().filter(|t| t.passed).count();
    let solc_failed = solc_tests.iter().filter(|t| !t.passed).count();

    let solc_run = CompilerRun {
        compiler: "solc".to_string(),
        compile_time: solc_compile_time + solc_test_time,
        tests: solc_tests,
        total_passed: solc_passed,
        total_failed: solc_failed,
        bytecode_sizes: solc_sizes,
    };

    // Print diff summary if there are regressions
    if solar_run.total_failed > 0 && solc_run.total_failed < solar_run.total_failed {
        print_test_diff(&solar_run.tests, &solc_run.tests, project_name);
    }

    // Print comparison
    println!("\n{}", "=".repeat(70));
    println!(" {} ", project_name.to_uppercase());
    println!("{}", "=".repeat(70));

    // Compilation time
    println!("\n📦 Compilation + Test Time:");
    println!(
        "   Solar: {:>6.2}s | solc: {:>6.2}s | {:+.0}%",
        solar_run.compile_time.as_secs_f64(),
        solc_run.compile_time.as_secs_f64(),
        ((solar_run.compile_time.as_secs_f64() / solc_run.compile_time.as_secs_f64()) - 1.0)
            * 100.0
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

/// Tests a project where solc can't compile (stack too deep).
/// Uses Solar for everything - both src and test contracts.
fn test_project_solar_only(project_name: &str, project_path: &str) {
    if !forge_available() {
        eprintln!("Skipping {}: forge not found in PATH", project_name);
        return;
    }

    let solar_binary = get_solar_binary();
    if !solar_binary.exists() {
        eprintln!("Skipping {}: Solar binary not found at {:?}", project_name, solar_binary);
        return;
    }

    let project_dir = get_crate_dir().join(project_path);
    if !project_dir.exists() {
        panic!("Project directory not found: {:?}", project_dir);
    }

    // Compile everything with Solar
    let out_dir = "out-solar";
    let cache_dir = "cache-solar";

    let mut cmd = Command::new("forge");
    cmd.current_dir(&project_dir)
        .arg("test")
        .arg("--force")
        .arg("--json")
        .arg("--out")
        .arg(out_dir)
        .arg("--cache-path")
        .arg(cache_dir)
        .env("FOUNDRY_SOLC", &solar_binary);

    let output = cmd.output().expect("failed to run forge test");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut total_passed = 0;
    let mut total_failed = 0;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(obj) = json.as_object() {
            for (_contract, contract_data) in obj {
                if let Some(test_results) = contract_data.get("test_results") {
                    if let Some(tests_obj) = test_results.as_object() {
                        for (_name, result) in tests_obj {
                            let passed = result
                                .get("status")
                                .and_then(|s| s.as_str())
                                .map(|s| s == "Success")
                                .unwrap_or(false);
                            if passed {
                                total_passed += 1;
                            } else {
                                total_failed += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    println!(
        "\n✅ [{}] Solar-only: {} passed, {} failed",
        project_name, total_passed, total_failed
    );

    assert_eq!(total_failed, 0, "[{}] {} Solar tests failed", project_name, total_failed);
    assert!(total_passed > 0, "[{}] No Solar tests ran", project_name);
}

/// Tests a project with Solar bytecode injection.
fn test_project_solar(project_name: &str, project_path: &str) {
    if !forge_available() {
        eprintln!("Skipping {}: forge not found in PATH", project_name);
        return;
    }

    let solar_binary = get_solar_binary();
    if !solar_binary.exists() {
        eprintln!("Skipping {}: Solar binary not found at {:?}", project_name, solar_binary);
        return;
    }

    let project_dir = get_crate_dir().join(project_path);
    if !project_dir.exists() {
        panic!("Project directory not found: {:?}", project_dir);
    }

    let (solar_run, solc_run) = run_project_comparison(project_name, project_path);

    // Assert Solar tests pass
    assert_eq!(
        solar_run.total_failed, 0,
        "[{}] {} Solar tests failed",
        project_name, solar_run.total_failed
    );
    assert!(solar_run.total_passed > 0, "[{}] No Solar tests ran", project_name);

    if solc_run.total_passed > solar_run.total_passed {
        eprintln!(
            "⚠️  [{}] solc passed {} more tests than Solar",
            project_name,
            solc_run.total_passed - solar_run.total_passed
        );
    }

    println!("\n✓ [{}] {} tests passed with Solar", project_name, solar_run.total_passed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic() {
        test_project_solar("arithmetic", "testdata/arithmetic");
    }

    #[test]
    fn test_control_flow() {
        test_project_solar("control_flow", "testdata/control-flow");
    }

    #[test]
    fn test_storage() {
        test_project_solar("storage", "testdata/storage");
    }

    #[test]
    fn test_events() {
        test_project_solar("events", "testdata/events");
    }

    #[test]
    fn test_calls() {
        test_project_solar("calls", "testdata/calls");
    }

    #[test]
    fn test_constructor_args() {
        test_project_solar("constructor_args", "testdata/constructor-args");
    }

    #[test]
    fn test_multi_return() {
        test_project_solar("multi_return", "testdata/multi-return");
    }

    #[test]
    #[ignore] // Some stack-deep tests fail due to unimplemented codegen features
    fn test_stack_deep() {
        // Stack-deep tests can't compile with solc - use Solar for everything
        test_project_solar_only("stack_deep", "testdata/stack-deep");
    }

    #[test]
    fn test_stack_deep_solar_only() {
        // Solar-only test for stack depth >16 - solc cannot compile these contracts
        test_project_solar_only("stack_deep_solar", "testdata/stack-deep-solar");
    }

    #[test]
    fn test_compilation() {
        if !forge_available() {
            eprintln!("Skipping: forge not found");
            return;
        }

        let solar_binary = get_solar_binary();
        if !solar_binary.exists() {
            eprintln!("Skipping: Solar binary not found");
            return;
        }

        let project_dir = get_crate_dir().join("testdata/arithmetic");
        let (compile_time, bytecodes) = compile_with_solar(&project_dir);

        println!("Compilation time: {:?}", compile_time);
        println!("Bytecodes: {:?}", bytecodes.iter().map(|b| &b.name).collect::<Vec<_>>());

        assert!(!bytecodes.is_empty(), "No bytecode produced");
    }
}
