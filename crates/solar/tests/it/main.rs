#![allow(unused_crate_dependencies)]

mod lint;

#[cfg(feature = "cli")]
mod debug_outputs;

#[cfg(feature = "cli")]
mod foundry;

#[cfg(feature = "cli")]
mod lsp;

#[cfg(feature = "cli")]
mod optimize_tags;

#[cfg(feature = "cli")]
mod paths;

#[cfg(feature = "cli")]
mod standard_json;

#[cfg(feature = "cli")]
mod mir_final;
