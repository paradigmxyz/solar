//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.

#![allow(unreachable_pub, unexpected_cfgs)]

use xshell::{Shell, cmd};

mod flags;

const INT_FLAGS: &[&str] = &["--package=solar-compiler", "--test=tests"];

fn main() -> anyhow::Result<()> {
    let flags = flags::Xtask::from_env_or_exit();
    match flags.subcommand {
        flags::XtaskCmd::Test(flags::Test { bless, test_name, rest }) => {
            let sh = Shell::new()?;

            let mut cmd =
                if bless { cmd!(sh, "cargo test") } else { cmd!(sh, "cargo nextest run") };
            if bless && test_name.is_none() {
                cmd = cmd.args(INT_FLAGS).env("TESTER_MODE", "ui");
            }
            if let Some(t) = test_name {
                if let Some(mode) = tester_mode(&t) {
                    cmd = cmd.args(INT_FLAGS).env("TESTER_MODE", mode);
                } else {
                    cmd = cmd.arg(t);
                }
            }
            cmd = cmd.arg("--");
            if bless {
                cmd = cmd.arg("--bless");
            }
            if !rest.is_empty() {
                cmd = cmd.args(rest);
            }
            cmd.run()?;
        }
    }

    Ok(())
}

fn tester_mode(test_name: &str) -> Option<&str> {
    match test_name {
        "ui" | "mir" | "standard-json" | "solc-solidity" | "solc-yul" => Some(test_name),
        "foundry" | "runtime" => Some("foundry"),
        _ => None,
    }
}
