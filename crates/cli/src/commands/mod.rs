//! CLI command runners.

use crate::args::{Args, Subcommands};
use std::process::ExitCode;

pub mod compile;
pub(crate) mod evm_opt;
#[cfg(feature = "lsp")]
mod lsp;
pub(crate) mod mir_opt;

pub(crate) fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    let time_passes = compile.unstable.time_passes;
    match commands {
        #[cfg(feature = "lsp")]
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::MirOpt(args)) => mir_opt::run(args, time_passes),
        Some(Subcommands::EvmOpt(args)) => evm_opt::run(args, time_passes),
        None => compile::run(compile),
    }
}
