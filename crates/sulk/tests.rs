#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_sulk");

fn main() {
    sulk_tester::run_tests(CMD.as_ref());
}
