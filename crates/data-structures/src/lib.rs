#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(feature = "nightly", feature(never_type))]
#![cfg_attr(feature = "nightly", feature(debug_closure_helpers))]
#![cfg_attr(feature = "nightly", feature(rustc_attrs))]
#![cfg_attr(feature = "nightly", feature(likely_unlikely))]
#![cfg_attr(feature = "nightly", feature(extern_types))]
#![cfg_attr(feature = "nightly", allow(internal_features))]

pub mod cycle;
pub mod fmt;
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

mod drop_guard;
pub use drop_guard::{DropGuard, defer};

mod interned;
pub use interned::Interned;

mod thin_slice;
pub use thin_slice::{RawThinSlice, ThinSlice};

pub use smallvec;

/// This calls the passed function while ensuring it won't be inlined into the caller.
#[inline(never)]
#[cold]
pub fn outline<R>(f: impl FnOnce() -> R) -> R {
    f()
}
