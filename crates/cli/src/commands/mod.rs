//! CLI command runners.

use crate::args::{Args, Subcommands};
use std::process::ExitCode;

pub mod compile;
mod lsp;
pub(crate) mod mir_opt;

pub(crate) fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    match commands {
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::MirOpt(args)) => mir_opt::run(args),
        None => compile::run(compile),
    }
}
