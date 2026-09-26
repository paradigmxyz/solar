use clap::{Parser, Subcommand};
use solar_config::CompileOpts;
#[cfg(feature = "lsp")]
use solar_config::LspArgs;

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
    #[cfg(feature = "lsp")]
    Lsp(LspArgs),
    /// Write the compiler-owned `solar:core/` modules under a directory, at their import paths,
    /// for other compilers and tools to resolve.
    ExportCore(ExportCoreArgs),
}

/// Arguments of `solar export-core`.
#[derive(clap::Args)]
pub(crate) struct ExportCoreArgs {
    /// The directory to write the `solar:core/` tree under.
    pub(crate) dir: std::path::PathBuf,
}
