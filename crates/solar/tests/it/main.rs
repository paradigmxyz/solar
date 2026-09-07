#![allow(unused_crate_dependencies)]

mod lint;

#[cfg(feature = "cli")]
mod debug_outputs;

#[cfg(feature = "cli")]
mod lsp;
