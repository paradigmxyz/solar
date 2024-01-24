#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_sulk");

fn main() {
    let code = sulk_tester::run_tests(CMD.as_ref());
    std::process::exit(code);
}
