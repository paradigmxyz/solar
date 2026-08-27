//! Standard JSON compiler support.

mod compile;
mod data;
mod metadata;

pub use compile::compile_standard_json;
pub use data::{ReadCallbackResult, StandardJsonReadCallback};

pub(crate) use compile::{
    ethdebug_compilation_id, make_ethdebug_compilation, make_ethdebug_program,
    make_ethdebug_resources, run,
};
