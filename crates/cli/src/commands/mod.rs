//! CLI command runners.

use std::process::ExitCode;

use crate::{Args, Subcommands};

pub mod compile;
mod lsp;
pub mod mir_opt;

pub fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    match commands {
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::MirOpt(args)) => mir_opt::run(args),
        None => compile::run(compile),
    }
}
