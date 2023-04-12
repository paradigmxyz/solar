//! Shared macros for `rsolc`.
//!
//! Modified from [rustc_macros](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_macros/src/lib.rs).

use proc_macro::TokenStream;

mod symbols;

#[proc_macro]
pub fn symbols(input: TokenStream) -> TokenStream {
    symbols::symbols(input.into()).into()
}
