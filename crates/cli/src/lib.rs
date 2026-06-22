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

mod emit;
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
pub use args::Args;

pub use commands::compile::run_compiler_args;

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn main() -> ExitCode {
    signal_handler::install();
    solar_interface::panic_hook::install();
    let _guard = utils::init_logger(utils::LogDestination::Stderr);

    let args = match parse_args(std::env::args_os()) {
        Ok(args) => args,
        Err(e) => e.exit(),
    };
    commands::run(args)
}

pub fn parse_args<I, T>(itr: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args = Args::try_parse_from(itr)?;
    args.compile.finish()?;
    Ok(args)
}
