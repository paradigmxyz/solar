#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_solar");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    solar_tester::run_tests(CMD.as_ref()).map_err(Into::into)
}
