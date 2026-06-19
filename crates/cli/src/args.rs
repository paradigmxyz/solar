use clap::{Parser, Subcommand};
use solar_config::Opts;

/// Blazingly fast Solidity compiler.
#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "solar",
    version = crate::version::short_version(),
    long_version = crate::version::version(),
    arg_required_else_help = true,
)]
#[allow(clippy::manual_non_exhaustive)]
pub struct Args {
    #[command(subcommand)]
    pub commands: Option<Subcommands>,
    #[command(flatten)]
    pub default_compile: Opts,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Subcommands {
    /// Start the language server.
    Lsp,
}
