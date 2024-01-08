#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_sulk");

#[test]
fn solc_solidity_tests() {
    sulk_tester::solc_solidity_tests(CMD.as_ref());
}

#[test]
fn solc_yul_tests() {
    sulk_tester::solc_yul_tests(CMD.as_ref());
}
