//! EVM bytecode code generation for Solar.
//!
//! This crate provides EVM bytecode generation from Solar's HIR.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod evm;
mod lower;

pub use evm::{Bytecode, EvmCodegen};
pub use lower::{MirFunction, compute_selector};
