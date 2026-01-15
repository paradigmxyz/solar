//! Foundry integration test harness.
//!
//! This module tests Solar's codegen by running forge tests with Solar as the compiler.
//! It uses the FOUNDRY_SOLC environment variable to point forge at Solar.
//!
//! Run with: cargo test -p solar-codegen --test foundry
#![allow(clippy::uninlined_format_args)]

use std::{
    path::PathBuf,
    process::{Command, Output},
};

/// Location of the foundry test project relative to this crate.
const TESTDATA_DIR: &str = "testdata/foundry-tests";

/// Gets the path to the Solar binary (debug build).
fn get_solar_binary() -> PathBuf {
    // Find the target directory by walking up from the crate root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/solar")
}

/// Gets the path to the testdata foundry project.
fn get_testdata_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join(TESTDATA_DIR)
}

/// Checks if forge is available.
fn forge_available() -> bool {
    Command::new("forge").arg("--version").output().is_ok()
}

/// Runs forge test with Solar as the compiler.
/// Uses a unique output directory based on the test name to allow parallel execution.
fn run_forge_test(test_name: &str, verbose: bool) -> Output {
    let solar_binary = get_solar_binary();
    let testdata_dir = get_testdata_dir();

    // Use unique out/cache dirs per test to allow parallel execution
    let out_dir = format!("out-{}", test_name);
    let cache_dir = format!("cache-{}", test_name);

    let mut cmd = Command::new("forge");
    cmd.current_dir(&testdata_dir)
        .env("FOUNDRY_SOLC", &solar_binary)
        .arg("test")
        .arg("--force")
        .arg("--out")
        .arg(&out_dir)
        .arg("--cache-path")
        .arg(&cache_dir);

    if verbose {
        cmd.arg("-vvvvv");
    }

    cmd.output().expect("failed to run forge test")
}

/// Runs forge build with Solar as the compiler.
fn run_forge_build(test_name: &str) -> Output {
    let solar_binary = get_solar_binary();
    let testdata_dir = get_testdata_dir();

    let out_dir = format!("out-{}", test_name);
    let cache_dir = format!("cache-{}", test_name);

    Command::new("forge")
        .current_dir(&testdata_dir)
        .env("FOUNDRY_SOLC", &solar_binary)
        .arg("build")
        .arg("--force")
        .arg("--out")
        .arg(&out_dir)
        .arg("--cache-path")
        .arg(&cache_dir)
        .output()
        .expect("failed to run forge build")
}

/// Parses the summary line from forge test output.
/// Example: "Ran 6 test suites ... 21 tests passed, 0 failed, 0 skipped (21 total tests)"
fn parse_summary(output: &str) -> Option<(usize, usize)> {
    for line in output.lines().rev() {
        // Look for the final summary line
        if line.contains("tests passed") && line.contains("failed") {
            // Parse: "21 tests passed, 0 failed, 0 skipped"
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut passed = 0;
            let mut failed = 0;

            for (i, part) in parts.iter().enumerate() {
                if *part == "tests" && i > 0 {
                    // Check if next word is "passed"
                    if parts.get(i + 1).is_some_and(|p| p.starts_with("passed")) {
                        passed = parts[i - 1].parse().unwrap_or(0);
                    }
                }
                if part.starts_with("failed") && i > 0 {
                    failed = parts[i - 1].parse().unwrap_or(0);
                }
            }

            if passed > 0 || failed > 0 {
                return Some((passed, failed));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Main test that runs all foundry tests with Solar.
    ///
    /// This test:
    /// 1. Builds all Solidity contracts with Solar (via FOUNDRY_SOLC)
    /// 2. Runs all forge tests against Solar-compiled bytecode
    /// 3. Verifies all tests pass
    #[test]
    fn test_foundry_suite() {
        if !forge_available() {
            eprintln!("Skipping foundry tests: forge not found in PATH");
            eprintln!("Install foundry: https://getfoundry.sh");
            return;
        }

        let solar_binary = get_solar_binary();
        if !solar_binary.exists() {
            eprintln!("Skipping foundry tests: Solar binary not found at {:?}", solar_binary);
            eprintln!("Run `cargo build` to build the Solar binary first.");
            return;
        }

        println!("Running forge tests with Solar compiler...");
        println!("Solar binary: {:?}", solar_binary);
        println!("Testdata dir: {:?}", get_testdata_dir());

        let output = run_forge_test("suite", true);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Print full output for debugging
        if !stdout.is_empty() {
            println!("\n=== STDOUT ===\n{}", stdout);
        }
        println!("\n=== STDERR ===\n{}", stderr);

        // Check exit code
        assert!(
            output.status.success(),
            "Forge test failed with exit code: {:?}",
            output.status.code()
        );

        // Parse and verify results (summary is in stdout for verbose mode)
        let combined_output = format!("{}\n{}", stdout, stderr);
        if let Some((passed, failed)) = parse_summary(&combined_output) {
            println!("\n=== SUMMARY ===");
            println!("Tests passed: {}", passed);
            println!("Tests failed: {}", failed);

            assert_eq!(failed, 0, "{} tests failed", failed);
            assert!(passed > 0, "No tests ran");

            println!("\n✓ All {} tests passed!", passed);
        } else {
            // Fallback: if we can't parse, at least check exit code
            println!("Could not parse test summary, but exit code was success");
        }
    }

    /// Test that we can compile the foundry project with Solar.
    #[test]
    fn test_foundry_compilation() {
        if !forge_available() {
            eprintln!("Skipping: forge not found");
            return;
        }

        let solar_binary = get_solar_binary();
        if !solar_binary.exists() {
            eprintln!("Skipping: Solar binary not found");
            return;
        }

        let output = run_forge_build("compile");
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(output.status.success(), "Forge build failed:\n{}", stderr);
        println!("Forge build succeeded!");
    }
}
