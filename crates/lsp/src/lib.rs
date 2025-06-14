//! Language Server Protocol implementation for Solar.

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod capabilities;
pub mod config;
pub mod document;
pub mod error;
pub mod server;
pub mod session;

pub use server::SolarLanguageServer;

#[cfg(test)]
mod tests;
