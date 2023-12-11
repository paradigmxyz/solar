//! Shared macros for `rsolc`.
//!
//! Modified from [rustc_macros](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_macros/src/lib.rs)
//! and [`rustc_index_macros`](https://github.com/rust-lang/rust/blob/f1eee2843fd3e62c71d993f732082b28cb5b22a0/compiler/rustc_index_macros).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/favicon.ico"
)]
#![warn(rustdoc::all)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(allow_internal_unstable))]
#![cfg_attr(feature = "nightly", allow(internal_features))]

use proc_macro::TokenStream;

mod index;
mod symbols;

#[proc_macro]
pub fn symbols(input: TokenStream) -> TokenStream {
    symbols::symbols(input.into()).into()
}

#[proc_macro]
#[cfg_attr(feature = "nightly", allow_internal_unstable(step_trait, rustc_attrs, trusted_step))]
pub fn newtype_index(input: TokenStream) -> TokenStream {
    index::newtype(input)
}
