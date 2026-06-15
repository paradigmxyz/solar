//! Experimental LSP server scaffold.
//!
//! This module owns only the LSP transport and lifecycle handshake. It intentionally does not run
//! the compiler pipeline yet; project loading, analysis, diagnostics, and editor features belong in
//! later layers on top of this entry point.

use std::io;

mod server;
mod transport;

/// Runs the experimental LSP server over stdio.
pub(crate) fn run_stdio() -> io::Result<()> {
    server::run(io::stdin().lock(), io::stdout().lock())
}
