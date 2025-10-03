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
            let mut cmd = cmd!(sh, "cargo test");
            if let Some(t) = test_name {
                if matches!(t.as_str(), "ui" | "solc-solidity" | "solc-yul") {
                    cmd = cmd.args(INT_FLAGS).env("TESTER_MODE", &t);
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
