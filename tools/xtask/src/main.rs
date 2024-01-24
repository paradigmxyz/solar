//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.

#![allow(unreachable_pub)]

use xshell::{cmd, Shell};

mod flags;

const INT_FLAGS: &[&str] = &["--package=sulk", "--test=tests"];

fn main() -> anyhow::Result<()> {
    let flags = flags::Xtask::from_env_or_exit();
    match flags.subcommand {
        flags::XtaskCmd::Test(flags::Test { bless, test_name }) => {
            let sh = Shell::new()?;
            let mut cmd = cmd!(sh, "cargo test -q");
            if bless {
                cmd = cmd.env("TESTER_BLESS", "1");
            }
            if let Some(t) = test_name {
                if matches!(t.as_str(), "ui" | "solc-solidity" | "solc-yul") {
                    cmd = cmd.args(INT_FLAGS).env("TESTER_MODE", &t);
                }
                cmd = cmd.arg(t);
            }
            cmd.run()?;
        }
    }

    Ok(())
}
