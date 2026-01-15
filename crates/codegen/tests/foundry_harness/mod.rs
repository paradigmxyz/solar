//! Foundry integration test harness.
//!
//! This module tests Solar's codegen by running forge tests with both Solar and solc,
//! comparing results, gas usage, compilation times, and bytecode sizes.
//!
//! Run with: cargo test -p solar-codegen --test foundry
#![allow(clippy::uninlined_format_args)]

use std::{
    collections::HashMap,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
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

/// Result of running a compiler on a project.
#[derive(Debug)]
struct CompilerRun {
    compiler: String,
    compile_time: Duration,
    tests: Vec<TestResult>,
    total_passed: usize,
    total_failed: usize,
    bytecode_sizes: HashMap<String, usize>,
}

/// Runs forge build and returns compilation time and bytecode sizes.
fn run_forge_build(project_dir: &PathBuf, compiler: &str) -> (Duration, HashMap<String, usize>) {
    let out_dir = format!("out-{}", compiler);
    let cache_dir = format!("cache-{}", compiler);

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("build")
        .arg("--force")
        .arg("--out")
        .arg(&out_dir)
        .arg("--cache-path")
        .arg(&cache_dir);

    if compiler == "solar" {
        cmd.env("FOUNDRY_SOLC", get_solar_binary());
    }

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge build");
    let compile_time = start.elapsed();

    if !output.status.success() {
        eprintln!(
            "[{}] Build failed: {}",
            compiler,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Parse bytecode sizes from artifacts
    let mut sizes = HashMap::new();
    let out_path = project_dir.join(&out_dir);
    if let Ok(entries) = std::fs::read_dir(&out_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Look for .json files inside contract directories
                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let file_path = file.path();
                        if file_path.extension().is_some_and(|e| e == "json") {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                                {
                                    // Get deployed bytecode size
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

/// Runs forge test and returns test results with gas usage.
fn run_forge_test(project_dir: &PathBuf, compiler: &str) -> (Duration, Vec<TestResult>) {
    let out_dir = format!("out-{}", compiler);
    let cache_dir = format!("cache-{}", compiler);

    let mut cmd = Command::new("forge");
    cmd.current_dir(project_dir)
        .arg("test")
        .arg("--force")
        .arg("--json")
        .arg("--out")
        .arg(&out_dir)
        .arg("--cache-path")
        .arg(&cache_dir);

    if compiler == "solar" {
        cmd.env("FOUNDRY_SOLC", get_solar_binary());
    }

    let start = Instant::now();
    let output = cmd.output().expect("failed to run forge test");
    let test_time = start.elapsed();

    let mut tests = Vec::new();

    // Parse JSON output - format is {"test/File.sol:Contract": {"test_results": {...}}}
    let stdout = String::from_utf8_lossy(&output.stdout);
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
                            // Gas is in kind.Unit.gas
                            let gas = result
                                .get("kind")
                                .and_then(|k| k.get("Unit"))
                                .and_then(|u| u.get("gas"))
                                .and_then(|g| g.as_u64())
                                .unwrap_or(0);
                            tests.push(TestResult {
                                name: name.clone(),
                                passed,
                                gas,
                            });
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

    // Run Solar
    let (solar_compile_time, solar_sizes) = run_forge_build(&project_dir, "solar");
    let (solar_test_time, solar_tests) = run_forge_test(&project_dir, "solar");
    let solar_passed = solar_tests.iter().filter(|t| t.passed).count();
    let solar_failed = solar_tests.iter().filter(|t| !t.passed).count();

    let solar_run = CompilerRun {
        compiler: "solar".to_string(),
        compile_time: solar_compile_time + solar_test_time,
        tests: solar_tests,
        total_passed: solar_passed,
        total_failed: solar_failed,
        bytecode_sizes: solar_sizes,
    };

    // Run solc
    let (solc_compile_time, solc_sizes) = run_forge_build(&project_dir, "solc");
    let (solc_test_time, solc_tests) = run_forge_test(&project_dir, "solc");
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
        ((solar_run.compile_time.as_secs_f64() / solc_run.compile_time.as_secs_f64()) - 1.0) * 100.0
    );

    // Test results
    println!("\n✅ Test Results:");
    println!(
        "   Solar: {} passed, {} failed",
        solar_run.total_passed, solar_run.total_failed
    );
    println!(
        "   solc:  {} passed, {} failed",
        solc_run.total_passed, solc_run.total_failed
    );

    // Bytecode sizes
    println!("\n📏 Bytecode Sizes (deployed):");
    let mut all_contracts: Vec<_> = solar_run
        .bytecode_sizes
        .keys()
        .chain(solc_run.bytecode_sizes.keys())
        .collect();
    all_contracts.sort();
    all_contracts.dedup();

    for contract in all_contracts {
        // Skip test contracts
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
            println!("   {:20} Solar: {:>5}B | solc: N/A (stack too deep?)", contract, solar_size);
        }
    }

    // Gas comparison for each test
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

            // Truncate test name for display
            let short_name: String = name.chars().take(35).collect();
            println!(
                "   {} {:35} Solar: {:>10} | solc: {:>10} | {:>+6.1}%",
                status, short_name, solar_test.gas, solc_test.gas, gas_diff
            );
        }
    }

    (solar_run, solc_run)
}

/// Tests a project with Solar only (for CI - must pass).
fn test_project_solar_only(project_name: &str, project_path: &str) {
    if !forge_available() {
        eprintln!("Skipping {}: forge not found in PATH", project_name);
        return;
    }

    let solar_binary = get_solar_binary();
    if !solar_binary.exists() {
        eprintln!(
            "Skipping {}: Solar binary not found at {:?}",
            project_name, solar_binary
        );
        return;
    }

    let project_dir = get_crate_dir().join(project_path);
    if !project_dir.exists() {
        panic!("Project directory not found: {:?}", project_dir);
    }

    // Run comparison (prints detailed output)
    let (solar_run, solc_run) = run_project_comparison(project_name, project_path);

    // Assert Solar tests pass
    assert_eq!(
        solar_run.total_failed, 0,
        "[{}] {} Solar tests failed",
        project_name, solar_run.total_failed
    );
    assert!(
        solar_run.total_passed > 0,
        "[{}] No Solar tests ran",
        project_name
    );

    // Warn if solc passes more tests (potential Solar bug)
    if solc_run.total_passed > solar_run.total_passed {
        eprintln!(
            "⚠️  [{}] solc passed {} more tests than Solar",
            project_name,
            solc_run.total_passed - solar_run.total_passed
        );
    }

    println!(
        "\n✓ [{}] {} tests passed with Solar",
        project_name, solar_run.total_passed
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic() {
        test_project_solar_only("arithmetic", "testdata/arithmetic");
    }

    #[test]
    fn test_control_flow() {
        test_project_solar_only("control_flow", "testdata/control-flow");
    }

    #[test]
    fn test_storage() {
        test_project_solar_only("storage", "testdata/storage");
    }

    #[test]
    fn test_events() {
        test_project_solar_only("events", "testdata/events");
    }

    #[test]
    fn test_calls() {
        test_project_solar_only("calls", "testdata/calls");
    }

    #[test]
    fn test_stack_deep() {
        test_project_solar_only("stack_deep", "testdata/stack-deep");
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
        let (compile_time, sizes) = run_forge_build(&project_dir, "solar");

        println!("Compilation time: {:?}", compile_time);
        println!("Bytecode sizes: {:?}", sizes);

        assert!(!sizes.is_empty(), "No bytecode produced");
    }
}
