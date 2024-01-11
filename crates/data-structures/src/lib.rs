//! Common data datastructures.
//!
//! Mostly modified from [`rustc_data_structures`](https://github.com/rust-lang/rust/blob/c1fc1d18cd38cab44696a9b0e0d52633863308fd/compiler/rustc_data_structures/src/lib.rs).

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/favicon.ico"
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

mod never;
pub use never::Never;

pub use smallvec;

/// Returns a structure that calls `f` when dropped.
pub fn defer<F: FnOnce()>(f: F) -> impl Drop {
    struct OnDrop<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for OnDrop<F> {
        #[inline]
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }

    OnDrop(Some(f))
}
