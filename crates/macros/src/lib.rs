#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
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
