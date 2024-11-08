#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(feature = "nightly", feature(cell_leak))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(feature = "nightly", feature(never_type))]
#![cfg_attr(feature = "nightly", feature(debug_closure_helpers))]
#![cfg_attr(feature = "nightly", feature(rustc_attrs))]
#![cfg_attr(feature = "nightly", allow(internal_features))]

use std::fmt;

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

/// Wrapper for [`fmt::from_fn`].
#[cfg(feature = "nightly")]
pub fn fmt_from_fn<F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result>(
    f: F,
) -> impl fmt::Debug + fmt::Display {
    fmt::from_fn(f)
}

/// Polyfill for [`fmt::from_fn`];
#[cfg(not(feature = "nightly"))]
pub fn fmt_from_fn<F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result>(
    f: F,
) -> impl fmt::Debug + fmt::Display {
    struct FromFn<F>(F);

    impl<F> fmt::Debug for FromFn<F>
    where
        F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }

    impl<F> fmt::Display for FromFn<F>
    where
        F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }

    FromFn(f)
}
