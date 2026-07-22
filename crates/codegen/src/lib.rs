#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

extern crate derive_more as _;
extern crate tracing as _;

/// Scratch word holding the base of the ephemeral multi-return buffer.
///
/// The returned words themselves live at the current free-memory pointer so
/// three-or-more-value returns never overwrite Solidity's `0x40`/`0x60`
/// reserved words. Consumers must snapshot the buffer before lowering any
/// lvalue that may reuse this scratch word or the unbumped free memory.
pub(crate) const MULTI_RETURN_BUFFER_PTR_SLOT: u64 = 0x20;

pub mod mir;

mod analysis;

pub mod backend;
pub use backend::{
    Backend,
    evm::{EvmCodegen, EvmCodegenConfig},
};
mod ir_parse;

pub mod lower;

pub mod pass;
mod timing;
mod transform;
pub(crate) mod utils;
