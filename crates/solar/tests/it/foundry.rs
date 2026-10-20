//! Foundry suites, run with the `solar` binary Cargo builds for this test.

use std::path::Path;

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

#[test]
fn foundry() {
    solar_tester::foundry::run_default_suite(Path::new(SOLAR));
}

#[test]
#[ignore = "external Foundry suite; run via `cargo tq foundry-external`"]
fn external() {
    solar_tester::foundry::run_external_suite(Path::new(SOLAR));
}
