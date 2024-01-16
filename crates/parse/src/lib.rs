//! Solidity lexer and parser.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use sulk_interface::diagnostics::{DiagnosticBuilder, ErrorGuaranteed};

mod lexer;
pub use lexer::{is_id_continue, is_id_start, is_ident, is_whitespace, unescape, Lexer};

mod parser;
pub use parser::Parser;

mod session;
pub use session::ParseSess;

// Convenience re-exports.
pub use sulk_ast::{ast, token};
pub use sulk_interface as interface;

/// Parser error type.
pub type PErr<'a> = DiagnosticBuilder<'a, ErrorGuaranteed>;

/// Parser result type. This is a shorthand for `Result<T, PErr<'a>>`.
pub type PResult<'a, T> = Result<T, PErr<'a>>;
