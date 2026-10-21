#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use clap::Parser as _;
use solar_interface::Result;
use std::process::ExitCode;

pub use solar_config::{self as config, CompileOpts, LspArgs, UnstableOpts, version};

mod bytecode;
mod emit;
mod ethdebug;
mod source_map;
pub mod standard_json;

pub mod commands;
pub mod utils;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod signal_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod signal_handler {
    #[cfg(unix)]
    use libc as _;

    /// No-op function.
    pub fn install() {}
}

mod args;
use args::Args;

pub use commands::compile::run_compiler_args;

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn main() -> ExitCode {
    signal_handler::install();
    raise_stack_limit();
    solar_interface::panic_hook::install();
    let _guard = utils::init_logger(utils::LogDestination::Stderr);

    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    commands::run(args)
}

/// Raises the stack limit of the main thread to that of the compiler threads, as it compiles in
/// place of a worker thread.
///
/// The limit applies as the stack grows, so raising it at runtime is enough.
#[cfg(target_os = "linux")]
fn raise_stack_limit() {
    let size = solar_interface::thread_stack_size() as libc::rlim_t;
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `limit` is a valid `rlimit` for both calls.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_STACK, &mut limit) == 0 && limit.rlim_cur < size {
            limit.rlim_cur = size.min(limit.rlim_max);
            libc::setrlimit(libc::RLIMIT_STACK, &limit);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn raise_stack_limit() {}

fn parse_args<I, T>(itr: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args = Args::try_parse_from(itr)?;
    args.compile.finish()?;
    // The process exits right after compiling, so skip freeing the compiler.
    args.compile.unstable.forget_gcx = true;
    Ok(args)
}
