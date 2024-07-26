//! Solidity lexer and parser.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use sulk_interface::diagnostics::{DiagnosticBuilder, ErrorGuaranteed};

pub mod lexer;
pub use lexer::{unescape, Cursor, Lexer};

mod parser;
pub use parser::Parser;

// Convenience re-exports.
pub use bumpalo;
pub use sulk_ast::{ast, token};
pub use sulk_interface as interface;

/// Parser error type.
pub type PErr<'a> = DiagnosticBuilder<'a, ErrorGuaranteed>;

/// Parser result type. This is a shorthand for `Result<T, PErr<'a>>`.
pub type PResult<'a, T> = Result<T, PErr<'a>>;
