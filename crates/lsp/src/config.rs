//! Server configuration types.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for the Solar LSP server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Path to the Solar binary, if not in PATH.
    pub solar_path: Option<PathBuf>,

    /// Root path of the workspace.
    pub workspace_root: Option<PathBuf>,

    /// Maximum number of concurrent requests to handle.
    pub max_concurrent_requests: Option<usize>,

    /// Logging level (trace, debug, info, warn, error).
    pub logging_level: Option<String>,

    /// Enable semantic highlighting.
    pub enable_semantic_tokens: Option<bool>,

    /// Enable code completion.
    pub enable_completion: Option<bool>,

    /// Enable hover information.
    pub enable_hover: Option<bool>,
}

impl ServerConfig {
    /// Get the maximum concurrent requests, defaulting to 4.
    pub fn max_concurrent_requests(&self) -> usize {
        self.max_concurrent_requests.unwrap_or(4)
    }

    /// Get the logging level, defaulting to "info".
    pub fn logging_level(&self) -> &str {
        self.logging_level.as_deref().unwrap_or("info")
    }
}
