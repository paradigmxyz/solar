//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.

#![allow(unreachable_pub)]

use xshell::{cmd, Shell};

mod flags;

const INT_FLAGS: &[&str] = &["--package=solar", "--test=tests"];

fn main() -> anyhow::Result<()> {
    let flags = flags::Xtask::from_env_or_exit();
    match flags.subcommand {
        flags::XtaskCmd::Test(flags::Test { bless, test_name, rest }) => {
            let sh = Shell::new()?;
            let mut cmd = cmd!(sh, "cargo test");
            if bless {
                cmd = cmd.args(INT_FLAGS).env("TESTER_BLESS", "1");
            }
            if let Some(t) = test_name {
                if matches!(t.as_str(), "ui" | "solc-solidity" | "solc-yul") {
                    cmd = cmd.args(INT_FLAGS).env("TESTER_MODE", &t);
                }
                cmd = cmd.arg(t);
            }
            if !rest.is_empty() {
                cmd = cmd.arg("--").args(rest);
            }
            cmd.run()?;
        }
    }

    Ok(())
}
