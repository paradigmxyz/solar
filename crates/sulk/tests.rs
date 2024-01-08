#![allow(unused_crate_dependencies)]

#[test]
fn solc_tests() {
    sulk_tester::solc_tests(env!("CARGO_BIN_EXE_sulk").as_ref());
}
