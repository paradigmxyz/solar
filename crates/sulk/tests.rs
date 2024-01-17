#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_sulk");

#[test]
fn ui_tests() {
    sulk_tester::run_tests(CMD.as_ref(), sulk_tester::Mode::Ui);
}

#[test]
fn solc_solidity_tests() {
    sulk_tester::run_tests(CMD.as_ref(), sulk_tester::Mode::SolcSolidity);
}

#[test]
fn solc_yul_tests() {
    sulk_tester::run_tests(CMD.as_ref(), sulk_tester::Mode::SolcYul);
}
