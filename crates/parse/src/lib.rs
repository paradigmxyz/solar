#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use solar_interface::diagnostics::{DiagBuilder, ErrorGuaranteed};

pub mod lexer;
pub use lexer::{unescape, Cursor, Lexer};

mod parser;
pub use parser::Parser;

// Convenience re-exports.
pub use bumpalo;
pub use solar_ast::{self as ast, token};
pub use solar_interface as interface;

/// Parser error type.
pub type PErr<'a> = DiagBuilder<'a, ErrorGuaranteed>;

/// Parser result type. This is a shorthand for `Result<T, PErr<'a>>`.
pub type PResult<'a, T> = Result<T, PErr<'a>>;
