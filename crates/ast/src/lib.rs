#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

// Convenience re-exports.
pub use bumpalo;
pub use solar_interface as interface;

mod ast;
pub use ast::*;

pub mod pretty;
pub mod token;
pub mod visit;
