//! Common data datastructures.
//!
//! Mostly modified from [`rustc_data_structures`](https://github.com/rust-lang/rust/blob/c1fc1d18cd38cab44696a9b0e0d52633863308fd/compiler/rustc_data_structures/src/lib.rs).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/danipopes/rsolc/main/assets/favicon.ico"
)]
#![warn(unreachable_pub, rustdoc::all)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(cell_leak))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(feature = "nightly", feature(never_type))]
#![cfg_attr(feature = "nightly", feature(rustc_attrs))]
#![cfg_attr(feature = "nightly", allow(internal_features))]

pub mod hint;
pub mod index;
pub mod map;
pub mod sync;

mod never;
pub use never::Never;

pub use smallvec;

// TODO: wait for possible perf improvements upstream
#[cfg(feature = "parallel")]
use rayon as _;
