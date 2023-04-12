#![cfg_attr(test, feature(test))]

// #[cfg(not(feature = "nightly"))]
mod arena;
// #[cfg(not(feature = "nightly"))]
pub use arena::{DroplessArena, IterExt, TypedArena};

// #[cfg(feature = "nightly")]
// mod nightly_arena;
// #[cfg(feature = "nightly")]
// pub use nightly_arena::{DroplessArena, IterExt, TypedArena};

pub mod fx;
