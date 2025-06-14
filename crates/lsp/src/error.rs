//! Error handling and conversion for the LSP server.

use thiserror::Error;
use tower_lsp::{
    jsonrpc,
    lsp_types::{error_codes, Position, Range, Url},
};

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

    /// Document not found.
    #[error("document not found: {0}")]
    DocumentNotFound(Url),

    /// Invalid position in document.
    #[error("invalid position: line {}, character {}", .0.line, .0.character)]
    InvalidPosition(Position),

    /// Invalid byte offset in document.
    #[error("invalid offset: {0}")]
    InvalidOffset(usize),

    /// Invalid range in document.
    #[error("invalid range: {}:{}-{}:{}", .0.start.line, .0.start.character, .0.end.line, .0.end.character)]
    InvalidRange(Range),

    /// Stale document version.
    #[error("stale version: current {current}, received {received}")]
    StaleVersion { current: i32, received: i32 },
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
            Error::DocumentNotFound(_)
            | Error::InvalidPosition(_)
            | Error::InvalidOffset(_)
            | Error::InvalidRange(_)
            | Error::StaleVersion { .. } => jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: err.to_string().into(),
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
