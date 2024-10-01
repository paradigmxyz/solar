//! Shared macros for `solar`.
//!
//! Modified from [`rustc_macros`](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_macros/src/lib.rs)
//! and [`rustc_index_macros`](https://github.com/rust-lang/rust/blob/f1eee2843fd3e62c71d993f732082b28cb5b22a0/compiler/rustc_index_macros).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![allow(unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod symbols;
mod visitor;

/// Declare a set of pre-interned keywords and symbols.
#[proc_macro]
pub fn symbols(input: TokenStream) -> TokenStream {
    symbols::symbols(input.into()).into()
}

/// Declare constant and mutable visitor traits.
///
/// First this expands:
/// - `#mut` is removed or replaced with `mut` on the mutable visitor.
/// - `#_mut` is removed or concatenated with the previous identifier on the mutable visitor.
///
/// Then `walk_` functions are generated for each `visit_` function with the same signature and
/// block and the `visit_` functions are made to call the `walk_` functions by default.
///
/// `walk_` functions should not be overridden.
#[proc_macro]
pub fn declare_visitors(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as visitor::Input).expand().into()
}
