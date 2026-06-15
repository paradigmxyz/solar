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
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Subcommands>,
    #[command(flatten)]
    pub compile: Opts,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Subcommands {
    /// Start the language server over stdio.
    Lsp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Args::command().debug_assert();
    }
}
