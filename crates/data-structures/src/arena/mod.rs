//! The arena, a fast but limited type of allocator.
//!
//! Arenas are a type of allocator that destroy the objects within, all at
//! once, once the arena itself is destroyed. They do not support deallocation
//! of individual objects while the arena itself is still alive. The benefit
//! of an arena is very fast allocation; just a pointer bump.
//!
//! This crate implements several kinds of arena.

// Stolen from [rustc_arena](https://github.com/rust-lang/rust/blob/661b33f5247debc4e0cd948caa388997e18e9cb8/compiler/rustc_arena/src/lib.rs)
// with unstable APIs removed, since they aren't essential.

// Arena allocators are one of the places where this pattern is fine.
#![allow(clippy::mut_from_ref)]

#[cfg(all(any(feature = "nightly", feature = "nightly-tests"), test))]
mod benches;
#[cfg(test)]
mod tests;

#[cfg(feature = "nightly")]
mod nightly;
#[cfg(feature = "nightly")]
pub use nightly::{declare_arena, DroplessArena, IsCopy, IsNotCopy, TypedArena};

#[cfg(not(feature = "nightly"))]
mod stable;
#[cfg(not(feature = "nightly"))]
pub use stable::{DroplessArena, IterExt, TypedArena};
