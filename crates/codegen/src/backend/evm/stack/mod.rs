//! Stack scheduling for EVM code generation.
//!
//! This module handles the translation from MIR's SSA values to EVM's stack-based model.
//! The key abstraction is `StackModel`, which tracks which MIR values are at which stack depths.
//!
//! ## Submodules
//!
//! - `model`: Core stack model tracking value positions
//! - `scheduler`: Stack scheduler for generating DUP/SWAP sequences
//! - `shuffler`: Greedy stack layout transitions
//! - `spill`: Spill management for values beyond depth 16

mod model;
mod scheduler;
pub(crate) mod shuffler;
mod spill;

pub(crate) use model::{MAX_STACK_ACCESS, StackModel, StackOp};
pub(crate) use scheduler::{ScheduledOp, StackScheduler};
pub(crate) use shuffler::TargetSlot;
pub(crate) use spill::SpillSlot;
