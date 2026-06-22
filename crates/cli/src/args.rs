use crate::commands::mir_opt::MirOptArgs;
use clap::{Parser, Subcommand};
use solar_config::{CompileOpts, LspArgs};

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
#[command(
    name = "solar",
    version = crate::version::short_version(),
    long_version = crate::version::version(),
    arg_required_else_help = true,
)]
#[allow(clippy::manual_non_exhaustive)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) commands: Option<Subcommands>,
    #[command(flatten)]
    pub(crate) compile: CompileOpts,
}

#[derive(Subcommand)]
pub(crate) enum Subcommands {
    /// Start the language server.
    Lsp(LspArgs),
    /// Run one or more MIR passes on a Solidity or MIR file.
    MirOpt(MirOptArgs),
}
