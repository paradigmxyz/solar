//! Standard JSON compiler support.

mod compile;
mod data;

pub use compile::compile_standard_json;
pub use data::{ReadCallbackResult, StandardJsonReadCallback};

pub(crate) use compile::run;
