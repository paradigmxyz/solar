#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_solar");

fn main() -> impl std::process::Termination {
    solar_tester::run_tests(CMD.as_ref())
}
