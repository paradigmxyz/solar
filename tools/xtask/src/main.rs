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
const FOUNDRY_FLAGS: &[&str] = &["--package=solar-tester"];

fn main() -> anyhow::Result<()> {
    let flags = flags::Xtask::from_env_or_exit();
    match flags.subcommand {
        flags::XtaskCmd::Test(flags::Test { bless, test_name, rest }) => {
            let sh = Shell::new()?;

            if test_name.as_deref() == Some("debug-diff") {
                anyhow::ensure!(!bless, "debug differentials cannot be blessed");
                cmd!(sh, "cargo build --package=solar-compiler --bin=solar").run()?;
                cmd!(sh, "python3 scripts/debug_diff.py").args(rest).run()?;
                return Ok(());
            }

            if test_name.as_deref() == Some("foundry-external") {
                // Release: external projects are compile-heavy, and the runner
                // prefers an existing release binary over a fresh debug one.
                cmd!(sh, "cargo build --release --package=solar-compiler --bin=solar").run()?;
                let mut cmd = cmd!(sh, "cargo nextest run");
                cmd = cmd
                    .args(FOUNDRY_FLAGS)
                    .arg("foundry::tests::external")
                    .arg("--run-ignored=only")
                    .arg("--no-capture");
                if let Some(project) = rest.first() {
                    cmd = cmd.env("SOLAR_FOUNDRY_PROJECT", project);
                }
                cmd.run()?;
                return Ok(());
            }

            if test_name.as_deref().is_some_and(|name| matches!(name, "foundry" | "runtime")) {
                cmd!(sh, "cargo build --package=solar-compiler --bin=solar").run()?;
                let mut cmd = cmd!(sh, "cargo nextest run");
                cmd = cmd.args(FOUNDRY_FLAGS).arg("foundry::tests::foundry").arg("--");
                if !rest.is_empty() {
                    cmd = cmd.args(rest);
                }
                cmd.run()?;
                return Ok(());
            }

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
        "ui" | "mir" | "evm-ir" | "standard-json" | "solc-solidity" | "solc-yul" => Some(test_name),
        _ => None,
    }
}
