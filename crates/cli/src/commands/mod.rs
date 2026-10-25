//! CLI command runners.

use crate::args::Args;
#[cfg(feature = "lsp")]
use crate::args::Subcommands;
use std::process::ExitCode;

pub mod compile;
#[cfg(feature = "lsp")]
mod lsp;

pub(crate) fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    match commands {
        #[cfg(feature = "lsp")]
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        None => compile::run(compile),
    }
}
