#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/ithacaxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/ithacaxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![allow(unused_crate_dependencies)]

pub use solar_ast as ast;
pub use solar_config as config;
pub use solar_data_structures as data_structures;
pub use solar_interface as interface;
pub use solar_macros as macros;
pub use solar_parse as parse;
pub use solar_sema as sema;

#[cfg(feature = "cli")]
pub use solar_cli as cli;
