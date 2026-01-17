//! Optimization and transformation passes for the Solar compiler.

pub mod constant_fold;
pub mod dce;

pub use constant_fold::{ConstantFolder, FoldResult};
pub use dce::DeadCodeEliminator;
