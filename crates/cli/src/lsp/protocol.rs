use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::io;

pub(super) const METHOD_NOT_FOUND: i64 = -32601;
pub(super) const INVALID_PARAMS: i64 = -32602;

pub(super) fn params<T: DeserializeOwned>(message: &Value) -> serde_json::Result<T> {
    serde_json::from_value(message.get("params").cloned().unwrap_or(Value::Null))
}

pub(super) fn response<T: Serialize>(id: Value, result: T) -> io::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": serde_json::to_value(result)?,
    }))
}

pub(super) fn error_response(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}
