//! LSP command identifiers and dispatch.

use crate::global_state::GlobalState;
use async_lsp::{ErrorCode, ResponseError};
use lsp_types::ExecuteCommandParams;
use serde_json::{Value, json};
use std::future::{Ready, ready};

pub(crate) const CLEAR_CACHE: &str = "solar.clearCache";
pub(crate) const REINDEX: &str = "solar.reindex";
pub(crate) const ALL: [&str; 2] = [CLEAR_CACHE, REINDEX];

pub(crate) fn execute_command(
    state: &mut GlobalState,
    params: ExecuteCommandParams,
) -> Ready<Result<Option<Value>, ResponseError>> {
    let result = match params.command.as_str() {
        CLEAR_CACHE => {
            state.clear_analysis_cache();
            Ok(success())
        }
        REINDEX => {
            state.reindex();
            Ok(success())
        }
        command => Err(ResponseError::new(
            ErrorCode::METHOD_NOT_FOUND,
            format!("unknown command `{command}`"),
        )),
    };
    ready(result)
}

fn success() -> Option<Value> {
    Some(json!({ "success": true }))
}
