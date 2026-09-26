//! CLI command runners.

use crate::args::{Args, Subcommands};
use std::process::ExitCode;

pub mod compile;
mod export_core;
#[cfg(feature = "lsp")]
mod lsp;

pub(crate) fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    match commands {
        #[cfg(feature = "lsp")]
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::ExportCore(args)) => export_core::run(args),
        None => compile::run(compile),
    }
}
