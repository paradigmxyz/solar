//! Standard JSON compiler support.

mod compile;
mod data;
mod metadata;

pub use compile::{compile_standard_json, compile_standard_json_with};
pub use data::{ReadCallbackResult, StandardJsonReadCallback};

pub(crate) use compile::run;
