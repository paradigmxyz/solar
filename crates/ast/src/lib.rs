//! Solidity Abstract Syntax Tree (AST) definitions and visitors.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/ithacaxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

// Convenience re-exports.
pub use bumpalo;
pub use solar_interface as interface;

pub mod ast;
pub mod token;
pub mod visit;
