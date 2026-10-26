//! Embeds the locked evm2 revision, so the binary reports which interpreter it measures.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock").expect("Cargo.lock is committed");
    let rev = lock
        .split("[[package]]")
        .find(|package| package.contains("\nname = \"evm2\"\n"))
        .and_then(|package| package.lines().find_map(|line| line.strip_prefix("source = ")))
        .and_then(|source| source.trim_matches('"').split_once('#'))
        .map(|(_, rev)| rev)
        .expect("Cargo.lock pins evm2 to a git revision");
    println!("cargo:rustc-env=EVM2_REV={rev}");
}
