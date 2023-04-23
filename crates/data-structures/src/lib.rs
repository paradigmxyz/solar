#![doc(test(no_crate_inject, attr(deny(warnings))))]
#![cfg_attr(all(any(feature = "nightly", feature = "nightly-tests"), test), feature(test))]
#![cfg_attr(
    feature = "nightly",
    feature(
        decl_macro,
        dropck_eyepatch,
        maybe_uninit_slice,
        min_specialization,
        new_uninit,
        pointer_byte_offsets,
        rustc_attrs,
        strict_provenance,
    )
)]

pub mod arena;
pub use arena::{DroplessArena, TypedArena};

pub mod fx;
