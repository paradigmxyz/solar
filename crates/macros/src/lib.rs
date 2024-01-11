//! Shared macros for `sulk`.
//!
//! Modified from [`rustc_macros`](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_macros/src/lib.rs)
//! and [`rustc_index_macros`](https://github.com/rust-lang/rust/blob/f1eee2843fd3e62c71d993f732082b28cb5b22a0/compiler/rustc_index_macros).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/favicon.ico"
)]
#![allow(unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use proc_macro::TokenStream;

mod symbols;

#[proc_macro]
pub fn symbols(input: TokenStream) -> TokenStream {
    symbols::symbols(input.into()).into()
}
