#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![allow(unused_crate_dependencies)]
#![allow(rustdoc::broken_intra_doc_links)] // Ignore GitHub Alerts in included README.md.

#[doc(inline)]
pub use solar_ast as ast;
#[doc(inline)]
pub use solar_config as config;
#[doc(inline)]
pub use solar_data_structures as data_structures;
#[doc(inline)]
pub use solar_interface as interface;
#[doc(inline)]
pub use solar_macros as macros;
#[doc(inline)]
pub use solar_parse as parse;
#[doc(inline)]
pub use solar_sema as sema;

#[cfg(feature = "cli")]
#[doc(inline)]
pub use solar_cli as cli;
