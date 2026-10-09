//! Symbolic stack runs for the EVM IR passes.
//!
//! - [`model`] tracks a stack of value identities through stack operations.
//! - [`shuffler`] synthesizes a cheap sequence of stack operations between two layouts. The EVM IR
//!   `stack-normalize` pass uses it to rewrite runs of stack operations once value identities are
//!   gone.

mod model;
mod shuffler;

pub(crate) use super::op::StackOp;
pub(crate) use model::{MAX_STACK_DEPTH, StackModel};
pub(crate) use shuffler::{lowered_stack_cost, resynthesize_physical_ops};
