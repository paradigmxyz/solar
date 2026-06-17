#![allow(unused_crate_dependencies)]

mod foundry_harness;

const CMD: &str = env!("CARGO_BIN_EXE_solar");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if runs_foundry_mode() {
        if std::env::args_os().any(|arg| arg == "--list") {
            return Ok(());
        }
        foundry_harness::run_default_suite();
        Ok(())
    } else {
        solar_tester::run_tests(CMD.as_ref()).map_err(Into::into)
    }
}

fn runs_foundry_mode() -> bool {
    std::env::var("TESTER_MODE").is_ok_and(|mode| mode.trim() == "foundry")
}
