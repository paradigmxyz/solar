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

pub mod mir;

pub(crate) mod memory;

mod analysis;
mod immutable;

pub mod backend;
pub use backend::{Backend, evm::EvmCodegen};
mod ir_parse;

pub mod lower;

pub mod pass;
mod pass_manager;
mod timing;
mod transform;
pub(crate) mod utils;
