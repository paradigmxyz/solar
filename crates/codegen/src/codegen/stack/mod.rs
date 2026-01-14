//! Stack scheduling for EVM code generation.
//!
//! This module handles the translation from MIR's SSA values to EVM's stack-based model.
//! The key abstraction is `StackModel`, which tracks which MIR values are at which stack depths.

mod model;
mod scheduler;
mod spill;

pub use model::{StackModel, StackOp};
pub use scheduler::{ScheduledOp, StackScheduler};
pub use spill::{SpillManager, SpillSlot};
