//! Experimental LSP server scaffold.
//!
//! This module owns only the LSP transport and lifecycle handshake. It intentionally does not run
//! the compiler pipeline yet; project loading, analysis, diagnostics, and editor features belong in
//! later layers on top of this entry point.

use std::io;

mod protocol;
mod server;
mod state;
mod transport;

/// Runs the experimental LSP server over stdio.
pub(crate) fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    server::run(stdin.lock(), stdout.lock())
}
