//! Solidity Abstract Syntax Tree (AST) definitions.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/favicon.ico"
)]
#![warn(unreachable_pub, rustdoc::all)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

// TODO
use rsolc_data_structures as _;
use rsolc_macros as _;

pub mod enums;
pub mod token;
