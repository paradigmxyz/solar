#[derive(Debug, Clone, clap::Args)]
pub struct LspArgs {
    /// Use standard input/output for LSP transport.
    ///
    /// This is the default, and ignored.
    /// This argument is recommended by the LSP specification.
    #[arg(long, hide = true)]
    pub stdio: bool,
}
