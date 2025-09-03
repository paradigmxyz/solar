use clap::{Parser, Subcommand};
use solar_config::Opts;

/// Blazingly fast Solidity compiler.
#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "solar",
    version = crate::version::SHORT_VERSION,
    long_version = crate::version::LONG_VERSION,
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
