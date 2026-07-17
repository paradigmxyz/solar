#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Feature flag.
use ruint as _;

#[macro_use]
extern crate tracing;

use solar_interface::diagnostics::{DiagBuilder, ErrorGuaranteed};

pub mod lexer;
pub use lexer::{Cursor, Lexer, unescape};

pub mod natspec;

mod parser;
pub use parser::{Parser, ParserCheckpoint, Recovered};

// Convenience re-exports.
#[doc(no_inline)]
pub use bumpalo;
pub use solar_ast::{self as ast, token};
pub use solar_interface as interface;

/// Parser error type.
pub type PErr<'a> = DiagBuilder<'a, ErrorGuaranteed>;

/// Parser result type. This is a shorthand for `Result<T, PErr<'a>>`.
pub type PResult<'a, T> = Result<T, PErr<'a>>;
