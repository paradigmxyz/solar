use clap::{Parser, Subcommand};
use solar_config::Opts;

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
#[command(
    name = "solar",
    version = crate::version::short_version(),
    long_version = crate::version::version(),
    arg_required_else_help = true,
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Subcommands>,
    #[command(flatten)]
    pub compile: Opts,
}

#[derive(Subcommand)]
pub enum Subcommands {
    /// Start the language server over stdio.
    Lsp,
}
