#![allow(unused_crate_dependencies)]

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

const CMD: &str = env!("CARGO_BIN_EXE_solar");

/// Locate the `solar-mir-opt` binary at runtime.
///
/// `CARGO_BIN_EXE_<name>` only works for binaries in the same crate, so we
/// can't use it directly here. Instead we derive the path from the test
/// binary's own location:
///   `target/<profile>/deps/<test>` → `target/<profile>/solar-mir-opt`
///
/// If the binary isn't built, returns `None` and `Mode::Mir` is skipped.
fn mir_opt_path() -> Option<&'static Path> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut path = std::env::current_exe().ok()?;
            path.pop(); // remove test binary name
            if path.file_name().and_then(|s| s.to_str()) == Some("deps") {
                path.pop(); // remove "deps"
            }
            path.push(if cfg!(windows) { "solar-mir-opt.exe" } else { "solar-mir-opt" });
            path.exists().then_some(path)
        })
        .as_deref()
}

fn main() -> impl std::process::Termination {
    solar_tester::run_tests(CMD.as_ref(), mir_opt_path())
}
