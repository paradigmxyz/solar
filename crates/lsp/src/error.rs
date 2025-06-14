//! Error handling and conversion for the LSP server.

use thiserror::Error;
use tower_lsp::{jsonrpc, lsp_types::error_codes};

/// Result type for LSP operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Main error type for the LSP server.
#[derive(Debug, Error)]
pub enum Error {
    /// Solar compilation error.
    #[error("compilation error: {0}")]
    Compilation(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Client-provided parameters are invalid.
    #[error("invalid params: {0}")]
    InvalidParams(String),

    /// Request was cancelled.
    #[error("request cancelled")]
    Cancelled,
}

impl From<Error> for jsonrpc::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::InvalidParams(msg) => jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: msg.into(),
                data: None,
            },
            Error::Cancelled => jsonrpc::Error {
                code: jsonrpc::ErrorCode::from(error_codes::REQUEST_CANCELLED),
                message: "Request cancelled".into(),
                data: None,
            },
            Error::Compilation(msg) => jsonrpc::Error {
                code: jsonrpc::ErrorCode::from(-32001),
                message: format!("Compilation error: {msg}").into(),
                data: None,
            },
            _ => jsonrpc::Error {
                code: jsonrpc::ErrorCode::InternalError,
                message: format!("Internal error: {err}").into(),
                data: None,
            },
        }
    }
}
