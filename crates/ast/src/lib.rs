#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/ithacaxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/ithacaxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

// Convenience re-exports.
pub use bumpalo;
pub use solar_interface as interface;

pub mod ast;
pub mod token;
pub mod visit;
