/// Options for running the LSP server.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct LspArgs {
    /// Use standard input/output for LSP transport.
    ///
    /// This is the default, and ignored.
    /// This argument is recommended by the LSP specification.
    #[cfg_attr(feature = "clap", arg(long, hide = true))]
    pub stdio: bool,
}
