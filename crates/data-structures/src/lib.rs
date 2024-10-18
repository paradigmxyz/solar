//! Common data datastructures.
//!
//! Mostly modified from [`rustc_data_structures`](https://github.com/rust-lang/rust/blob/c1fc1d18cd38cab44696a9b0e0d52633863308fd/compiler/rustc_data_structures/src/lib.rs).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
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
pub mod trustme;

mod bump_ext;
pub use bump_ext::BumpExt;

mod collect;
pub use collect::CollectAndApply;

mod never;
pub use never::Never;

mod on_drop;
pub use on_drop::{defer, OnDrop};

mod interned;
pub use interned::Interned;

pub use smallvec;

/// This calls the passed function while ensuring it won't be inlined into the caller.
#[inline(never)]
#[cold]
pub fn outline<R>(f: impl FnOnce() -> R) -> R {
    f()
}
