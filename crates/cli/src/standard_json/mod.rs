//! Standard JSON compiler support.

mod build_info;
mod compile;
mod data;
mod metadata;

pub use compile::compile_standard_json;
pub use data::{ReadCallbackResult, StandardJsonReadCallback};

pub(crate) use compile::run;
