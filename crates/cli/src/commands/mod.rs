//! CLI command runners.

use crate::args::{Args, Subcommands};
use solar_data_structures::fmt::line_diff;
use std::{fmt::Display, process::ExitCode};

pub mod compile;
pub(crate) mod evm_opt;
#[cfg(feature = "lsp")]
mod lsp;
pub(crate) mod mir_opt;

fn print_pass_diff(
    name: impl Display,
    pass: impl Display,
    before: impl Display,
    after: impl Display,
) {
    let before = format!("// === {name} (before {pass}) ===\n{before}");
    let after = format!("// === {name} (after {pass}) ===\n{after}");
    print!("{}", line_diff(&before, &after));
}

pub(crate) fn run(args: Args) -> ExitCode {
    let Args { commands, compile } = args;
    match commands {
        #[cfg(feature = "lsp")]
        Some(Subcommands::Lsp(args)) => lsp::run(args),
        Some(Subcommands::MirOpt(args)) => mir_opt::run(args, compile),
        Some(Subcommands::EvmOpt(args)) => evm_opt::run(args, compile),
        None => compile::run(compile),
    }
}
