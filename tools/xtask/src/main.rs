//! See <https://github.com/matklad/cargo-xtask/>.
//!
//! This binary defines various auxiliary build commands, which are not expressible with just
//! `cargo`.
//!
//! This binary is integrated into the `cargo` command line by using an alias in `.cargo/config`.

#![allow(unreachable_pub)]

use xshell as _;

mod flags;

fn main() {
    let flags = flags::Xtask::from_env_or_exit();
    // TODO
    let _ = flags;
}
